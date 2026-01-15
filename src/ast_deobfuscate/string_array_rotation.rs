//! String array detection and rotation pass
//!
//! Detects obfuscated string arrays and rotation IIFEs:
//! ```js
//! var _0x1234 = ["a", "b", "c"];
//! (function(_0x5678, _0x9abc) {
//!     var _0xdef = function(_0xghi) {
//!         while (--_0xghi) {
//!             _0x5678.push(_0x5678.shift());
//!         }
//!     };
//!     _0xdef(2);
//! })(_0x1234, 0x123);
//! ```
//! The array is rotated, so ["a", "b", "c"] becomes ["c", "a", "b"] after 2 rotations.

use oxc_ast::ast::*;
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::FxHashMap;

use crate::ast_deobfuscate::state::{DeobfuscateState, StringArrayInfo};

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct StringArrayRotation {
    detected_arrays: FxHashMap<String, StringArrayInfo>,
    pending_rotations: FxHashMap<String, usize>,
}

impl StringArrayRotation {
    pub fn new() -> Self {
        Self {
            detected_arrays: FxHashMap::default(),
            pending_rotations: FxHashMap::default(),
        }
    }

    fn is_obfuscated_name(name: &str) -> bool {
        name.starts_with("_0x") || name.starts_with("_0X")
    }

    fn detect_string_array<'a>(
        &mut self,
        var_decl: &VariableDeclaration<'a>,
    ) -> Option<StringArrayInfo> {
        if var_decl.declarations.len() != 1 {
            return None;
        }

        let decl = &var_decl.declarations[0];

        let var_name = match &decl.id {
            BindingPattern::BindingIdentifier(ident) => ident.name.as_str().to_string(),
            _ => return None,
        };

        if !Self::is_obfuscated_name(&var_name) {
            return None;
        }

        let init = decl.init.as_ref()?;

        let array_expr = match init {
            Expression::ArrayExpression(arr) => arr,
            _ => return None,
        };

        let mut strings = Vec::new();
        for element in &array_expr.elements {
            if let ArrayExpressionElement::StringLiteral(lit) = element {
                strings.push(lit.value.to_string());
            } else {
                return None;
            }
        }

        if strings.is_empty() {
            return None;
        }

        Some(StringArrayInfo {
            var_name: var_name.clone(),
            strings,
            rotated: false,
            rotation_count: 0,
        })
    }

    fn detect_rotation_iife<'a>(
        &mut self,
        call_expr: &CallExpression<'a>,
    ) -> Option<(String, usize)> {
        eprintln!("[AST] Checking call for rotation IIFE");

        let func = match &call_expr.callee {
            Expression::FunctionExpression(f) => {
                eprintln!("[AST]   Direct function");
                f
            }
            Expression::ParenthesizedExpression(paren) => match &paren.expression {
                Expression::FunctionExpression(f) => {
                    eprintln!("[AST]   Parenthesized function");
                    f
                }
                _ => {
                    eprintln!("[AST]   Parenthesized but not function");
                    return None;
                }
            },
            _ => {
                eprintln!("[AST]   Not function or paren");
                return None;
            }
        };

        if func.params.items.len() < 1 {
            eprintln!("[AST]   No params");
            return None;
        }

        let array_arg = call_expr.arguments.first()?;
        let array_name = match array_arg {
            Argument::Identifier(ident) => {
                eprintln!("[AST]   Arg: {}", ident.name);
                ident.name.as_str()
            }
            _ => {
                eprintln!("[AST]   Arg not identifier");
                return None;
            }
        };

        if !self.detected_arrays.contains_key(array_name) {
            eprintln!("[AST]   {} not in arrays", array_name);
            return None;
        }

        eprintln!("[AST]   Analyzing body");
        let rotation_count = self.analyze_rotation_body(&func.body.as_ref()?.statements)?;

        eprintln!(
            "[AST] Detected rotation IIFE: {} with count {}",
            array_name, rotation_count
        );

        Some((array_name.to_string(), rotation_count))
    }

    fn analyze_rotation_body<'a>(&self, statements: &[Statement<'a>]) -> Option<usize> {
        let mut has_rotation_ops = false;
        let mut rotation_count = None;

        eprintln!("[AST]     Body has {} statements", statements.len());

        for (i, stmt) in statements.iter().enumerate() {
            eprintln!("[AST]     Scanning statement {}", i);
            self.scan_for_rotation(stmt, &mut has_rotation_ops, &mut rotation_count);
        }

        eprintln!(
            "[AST]     has_rotation_ops: {}, rotation_count: {:?}",
            has_rotation_ops, rotation_count
        );

        if has_rotation_ops && rotation_count.is_some() {
            rotation_count
        } else {
            eprintln!(
                "[AST]     No rotation detected (ops={}, count={:?})",
                has_rotation_ops, rotation_count
            );
            None
        }
    }

    fn scan_for_rotation<'a>(
        &self,
        stmt: &Statement<'a>,
        has_ops: &mut bool,
        count: &mut Option<usize>,
    ) {
        match stmt {
            Statement::VariableDeclaration(var_decl) => {
                eprintln!(
                    "[AST]       Found VariableDeclaration with {} declarations",
                    var_decl.declarations.len()
                );
                for decl in &var_decl.declarations {
                    if let Some(init) = &decl.init {
                        self.scan_expr_for_rotation(init, has_ops, count);
                    }
                }
            }
            Statement::ExpressionStatement(expr_stmt) => {
                eprintln!("[AST]       Found ExpressionStatement");
                self.scan_expr_for_rotation(&expr_stmt.expression, has_ops, count);
            }
            Statement::WhileStatement(while_stmt) => {
                eprintln!("[AST]       Found WhileStatement, scanning body");
                self.scan_for_rotation(&while_stmt.body, has_ops, count);
            }
            Statement::BlockStatement(block) => {
                eprintln!(
                    "[AST]       Found BlockStatement with {} statements",
                    block.body.len()
                );
                for stmt in &block.body {
                    self.scan_for_rotation(stmt, has_ops, count);
                }
            }
            _ => {
                eprintln!("[AST]       Found other statement type");
            }
        }
    }

    fn scan_expr_for_rotation<'a>(
        &self,
        expr: &Expression<'a>,
        has_ops: &mut bool,
        count: &mut Option<usize>,
    ) {
        match expr {
            Expression::CallExpression(call) => {
                eprintln!("[AST]         Found CallExpression");
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    let method = member.property.name.as_str();
                    eprintln!("[AST]           Method: {}", method);
                    if matches!(method, "push" | "shift" | "unshift" | "pop" | "splice") {
                        eprintln!("[AST]           ✓ Found rotation operation: {}", method);
                        *has_ops = true;
                    }
                }

                for arg in &call.arguments {
                    if let Argument::NumericLiteral(lit) = arg {
                        let num = lit.value as usize;
                        eprintln!("[AST]           Numeric arg: {}", num);
                        if num > 0 && num < 1000 {
                            eprintln!("[AST]           ✓ Found rotation count: {}", num);
                            *count = Some(num);
                        }
                    }
                    if let Argument::Identifier(_ident) = arg {
                        if let Expression::StaticMemberExpression(member) = &call.callee {
                            if matches!(member.property.name.as_str(), "push" | "shift") {
                                *has_ops = true;
                            }
                        }
                    }
                }
            }
            Expression::FunctionExpression(func) => {
                eprintln!("[AST]         Found FunctionExpression, scanning body");
                if let Some(body) = &func.body {
                    for stmt in &body.statements {
                        self.scan_for_rotation(stmt, has_ops, count);
                    }
                }
            }
            _ => {
                eprintln!("[AST]         Found other expression type");
            }
        }
    }

    fn apply_rotation(strings: &mut Vec<String>, count: usize) {
        let len = strings.len();
        if len == 0 {
            return;
        }

        let actual_count = count % len;

        for _ in 0..actual_count {
            if let Some(first) = strings.first().cloned() {
                strings.remove(0);
                strings.push(first);
            }
        }
    }

    pub fn finalize(&mut self, state: &mut DeobfuscateState) {
        for (array_name, rotation_count) in &self.pending_rotations {
            if let Some(array_info) = self.detected_arrays.get_mut(array_name) {
                Self::apply_rotation(&mut array_info.strings, *rotation_count);
                array_info.rotated = true;
                array_info.rotation_count = *rotation_count;

                eprintln!(
                    "[AST] Applied rotation to {}: {} positions",
                    array_name, rotation_count
                );
                eprintln!(
                    "[AST]   Result: {:?}",
                    &array_info.strings[..5.min(array_info.strings.len())]
                );
            }
        }

        state
            .string_arrays
            .extend(self.detected_arrays.drain().map(|(k, v)| (k, v)));
    }
}

impl Default for StringArrayRotation {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for StringArrayRotation {
    fn enter_statement(&mut self, stmt: &mut Statement<'a>, _ctx: &mut Ctx<'a>) {
        if let Statement::VariableDeclaration(var_decl) = stmt {
            if let Some(array_info) = self.detect_string_array(var_decl) {
                eprintln!(
                    "[AST] Found string array: {} with {} strings",
                    array_info.var_name,
                    array_info.strings.len()
                );
                eprintln!(
                    "[AST]   First 5: {:?}",
                    &array_info.strings[..5.min(array_info.strings.len())]
                );
                self.detected_arrays
                    .insert(array_info.var_name.clone(), array_info);
            }
        }

        if let Statement::ExpressionStatement(expr_stmt) = stmt {
            if let Expression::CallExpression(call) = &expr_stmt.expression {
                if let Some((array_name, rotation_count)) = self.detect_rotation_iife(call) {
                    eprintln!(
                        "[AST] Detected rotation IIFE for {}: {} rotations",
                        array_name, rotation_count
                    );
                    self.pending_rotations.insert(array_name, rotation_count);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;

    fn run_string_array_rotation(code: &str) -> (String, StringArrayRotation, DeobfuscateState) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut pass = StringArrayRotation::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = oxc_traverse::ReusableTraverseCtx::new(state, scoping, &allocator);

        oxc_traverse::traverse_mut_with_ctx(&mut pass, &mut program, &mut ctx);

        let mut state = DeobfuscateState::new();
        pass.finalize(&mut state);

        let output = Codegen::new().build(&program).code;
        (output, pass, state)
    }

    #[test]
    fn test_detect_string_array() {
        let code = r#"var _0x1234 = ["hello", "world", "test"];"#;

        let (_, _pass, state) = run_string_array_rotation(code);

        assert_eq!(state.string_arrays.len(), 1);
        assert!(state.string_arrays.contains_key("_0x1234"));

        let array = &state.string_arrays["_0x1234"];
        assert_eq!(array.strings, vec!["hello", "world", "test"]);
        assert!(!array.rotated);
    }

    #[test]
    fn test_rotation() {
        let code = r#"
            var _0x1111 = ["a", "b", "c"];
            (function(_0x2222, _0x3333) {
                var _0x4444 = function(_0x5555) {
                    while (--_0x5555) {
                        _0x2222.push(_0x2222.shift());
                    }
                };
                _0x4444(2);
            })(_0x1111, 0x123);
        "#;

        let (_, _pass, state) = run_string_array_rotation(code);

        assert_eq!(state.string_arrays.len(), 1);
        assert!(state.string_arrays.contains_key("_0x1111"));

        let array = &state.string_arrays["_0x1111"];
        assert!(array.rotated, "Array should be marked as rotated");
        assert_eq!(array.rotation_count, 2);
        assert_eq!(
            array.strings,
            vec!["c", "a", "b"],
            "Array should be rotated by 2 positions"
        );
    }

    #[test]
    fn test_apply_rotation() {
        let mut strings = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        StringArrayRotation::apply_rotation(&mut strings, 1);
        assert_eq!(strings, vec!["b", "c", "a"]);

        let mut strings2 = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        StringArrayRotation::apply_rotation(&mut strings2, 2);
        assert_eq!(strings2, vec!["z", "x", "y"]);
    }
}
