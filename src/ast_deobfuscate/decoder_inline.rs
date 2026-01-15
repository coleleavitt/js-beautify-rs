//! Decoder function detection and inlining pass
//!
//! Detects decoder functions that wrap string array access:
//! ```js
//! var _0x1234 = ["hello", "world"];
//! function _0xdec(a) { return _0x1234[a - 291]; }
//! console.log(_0xdec(291)); // Inlined to: console.log("hello");
//! ```

use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::FxHashMap;

use crate::ast_deobfuscate::state::{DecoderInfo, DeobfuscateState, OffsetOperation};

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct DecoderInliner {
    detected_decoders: FxHashMap<String, DecoderInfo>,
    changed: bool,
}

impl DecoderInliner {
    pub fn new() -> Self {
        Self {
            detected_decoders: FxHashMap::default(),
            changed: false,
        }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn detect_decoder_function<'a>(
        &mut self,
        func: &Function<'a>,
        func_name: &str,
        ctx: &Ctx<'a>,
    ) -> Option<DecoderInfo> {
        eprintln!("[AST] Analyzing function: {}", func_name);

        if func.params.items.len() != 1 {
            eprintln!(
                "[AST]   Function has {} params, need exactly 1",
                func.params.items.len()
            );
            return None;
        }

        let param_name = match &func.params.items[0].pattern {
            BindingPattern::BindingIdentifier(ident) => {
                eprintln!("[AST]   Parameter: {}", ident.name);
                ident.name.as_str()
            }
            _ => {
                eprintln!("[AST]   Parameter is not simple identifier");
                return None;
            }
        };

        let body = func.body.as_ref()?;
        eprintln!(
            "[AST]   Function body has {} statements",
            body.statements.len()
        );

        let (array_name, offset, offset_op) =
            self.analyze_function_body(&body.statements, param_name, ctx)?;

        eprintln!(
            "[AST] ✓ Detected decoder: {} for array {} (offset: {:?} {})",
            func_name, array_name, offset_op, offset
        );

        Some(DecoderInfo {
            function_name: func_name.to_string(),
            array_name,
            offset,
            offset_operation: offset_op,
        })
    }

    fn analyze_function_body<'a>(
        &self,
        statements: &[Statement<'a>],
        param_name: &str,
        ctx: &Ctx<'a>,
    ) -> Option<(String, i32, OffsetOperation)> {
        eprintln!(
            "[AST]   Analyzing body with {} statements",
            statements.len()
        );

        let mut array_name = None;
        let mut offset = 0i32;
        let mut offset_op = OffsetOperation::None;

        for stmt in statements {
            match stmt {
                Statement::ReturnStatement(ret) => {
                    eprintln!("[AST]     Found return statement");
                    if let Some(arg) = &ret.argument {
                        if let Some((arr, off, op)) =
                            self.extract_array_access(arg, param_name, ctx)
                        {
                            eprintln!(
                                "[AST]       extract_array_access returned: array={}, offset={}, op={:?}",
                                arr, off, op
                            );
                            array_name = Some(arr);
                            if offset_op == OffsetOperation::None {
                                eprintln!(
                                    "[AST]       No assignment offset found, using inline offset"
                                );
                                offset = off;
                                offset_op = op;
                            } else {
                                eprintln!(
                                    "[AST]       Already have assignment offset ({:?} {}), keeping it",
                                    offset_op, offset
                                );
                            }
                        }
                    }
                }
                Statement::ExpressionStatement(expr_stmt) => {
                    eprintln!("[AST]     Found expression statement");
                    if let Some((off, op)) =
                        self.extract_offset_assignment(&expr_stmt.expression, param_name)
                    {
                        offset = off;
                        offset_op = op;
                        eprintln!("[AST]       Extracted offset assignment: {:?} {}", op, off);
                    }
                }
                _ => {
                    eprintln!("[AST]     Found other statement type");
                }
            }
        }

        if let Some(arr) = array_name {
            if ctx.state.string_arrays.contains_key(&arr) {
                eprintln!("[AST]     Array {} is a known string array", arr);
                Some((arr, offset, offset_op))
            } else {
                eprintln!("[AST]     Array {} not found in string arrays", arr);
                None
            }
        } else {
            eprintln!("[AST]     No array access found");
            None
        }
    }

    fn extract_array_access<'a>(
        &self,
        expr: &Expression<'a>,
        param_name: &str,
        _ctx: &Ctx<'a>,
    ) -> Option<(String, i32, OffsetOperation)> {
        match expr {
            Expression::ComputedMemberExpression(member) => {
                eprintln!("[AST]       Found computed member expression");

                let array_name = match &member.object {
                    Expression::Identifier(ident) => {
                        eprintln!("[AST]         Array: {}", ident.name);
                        ident.name.to_string()
                    }
                    _ => return None,
                };

                let (offset, offset_op) = match &member.expression {
                    Expression::Identifier(ident) if ident.name.as_str() == param_name => {
                        eprintln!("[AST]         Index: {} (no offset)", param_name);
                        (0, OffsetOperation::None)
                    }
                    Expression::BinaryExpression(bin) => {
                        eprintln!("[AST]         Index is binary expression");
                        self.extract_offset_from_binary(bin, param_name)?
                    }
                    _ => {
                        eprintln!("[AST]         Index is not identifier or binary");
                        return None;
                    }
                };

                Some((array_name, offset, offset_op))
            }
            _ => {
                eprintln!("[AST]       Return expression is not array access");
                None
            }
        }
    }

    fn extract_offset_assignment<'a>(
        &self,
        expr: &Expression<'a>,
        param_name: &str,
    ) -> Option<(i32, OffsetOperation)> {
        if let Expression::AssignmentExpression(assign) = expr {
            if let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left {
                if target.name.as_str() == param_name {
                    if let Expression::BinaryExpression(bin) = &assign.right {
                        return self.extract_offset_from_binary(bin, param_name);
                    }
                }
            }
        }
        None
    }

    fn extract_offset_from_binary<'a>(
        &self,
        bin: &BinaryExpression<'a>,
        param_name: &str,
    ) -> Option<(i32, OffsetOperation)> {
        let left_is_param =
            matches!(&bin.left, Expression::Identifier(id) if id.name.as_str() == param_name);

        if !left_is_param {
            eprintln!("[AST]           Left side is not parameter");
            return None;
        }

        let offset = match &bin.right {
            Expression::NumericLiteral(lit) => {
                eprintln!("[AST]           Right side is number: {}", lit.value);
                lit.value as i32
            }
            _ => {
                eprintln!("[AST]           Right side is not numeric literal");
                return None;
            }
        };

        let offset_op = match bin.operator {
            BinaryOperator::Subtraction => {
                eprintln!("[AST]           Operation: subtract");
                OffsetOperation::Subtract
            }
            BinaryOperator::Addition => {
                eprintln!("[AST]           Operation: add");
                OffsetOperation::Add
            }
            _ => {
                eprintln!("[AST]           Operation: unsupported");
                return None;
            }
        };

        Some((offset, offset_op))
    }

    fn try_inline_decoder_call<'a>(
        &mut self,
        call: &CallExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        eprintln!("[AST] Checking call for decoder inlining");

        let func_name = match &call.callee {
            Expression::Identifier(ident) => {
                eprintln!("[AST]   Function: {}", ident.name);
                ident.name.as_str()
            }
            _ => {
                eprintln!("[AST]   Callee is not identifier");
                return None;
            }
        };

        let decoder = self.detected_decoders.get(func_name)?;
        eprintln!("[AST]   Found decoder for function {}", func_name);

        if call.arguments.len() != 1 {
            eprintln!(
                "[AST]   Call has {} args, need exactly 1",
                call.arguments.len()
            );
            return None;
        }

        let arg_value = match &call.arguments[0] {
            Argument::NumericLiteral(lit) => {
                eprintln!("[AST]   Argument: {}", lit.value);
                lit.value as i32
            }
            _ => {
                eprintln!("[AST]   Argument is not numeric literal");
                return None;
            }
        };

        eprintln!(
            "[AST]   Decoder offset_operation: {:?}, offset: {}",
            decoder.offset_operation, decoder.offset
        );

        let actual_index = match decoder.offset_operation {
            OffsetOperation::None => {
                eprintln!(
                    "[AST]   No offset, using argument value directly: {}",
                    arg_value
                );
                arg_value as usize
            }
            OffsetOperation::Subtract => {
                let result = arg_value - decoder.offset;
                eprintln!(
                    "[AST]   Applying offset: {} - {} = {}",
                    arg_value, decoder.offset, result
                );
                if result < 0 {
                    eprintln!("[AST]   Negative index after offset");
                    return None;
                }
                result as usize
            }
            OffsetOperation::Add => {
                let result = arg_value + decoder.offset;
                eprintln!(
                    "[AST]   Applying offset: {} + {} = {}",
                    arg_value, decoder.offset, result
                );
                result as usize
            }
        };

        let array_info = ctx.state.string_arrays.get(&decoder.array_name)?;
        eprintln!(
            "[AST]   Array {} has {} strings",
            decoder.array_name,
            array_info.strings.len()
        );

        if actual_index >= array_info.strings.len() {
            eprintln!(
                "[AST]   Index {} out of bounds (array has {})",
                actual_index,
                array_info.strings.len()
            );
            return None;
        }

        let string_value = &array_info.strings[actual_index];
        eprintln!(
            "[AST] ✓ Inlining decoder call: {}({}) → \"{}\"",
            func_name, arg_value, string_value
        );

        Some(Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
            span: SPAN,
            value: ctx.ast.atom(string_value.as_str()),
            raw: None,
            lone_surrogates: false,
        })))
    }

    pub fn finalize(&mut self, state: &mut DeobfuscateState) {
        state
            .decoders
            .extend(self.detected_decoders.drain().map(|(k, v)| (k, v)));
    }
}

impl Default for DecoderInliner {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for DecoderInliner {
    fn enter_statement(&mut self, stmt: &mut Statement<'a>, ctx: &mut Ctx<'a>) {
        if let Statement::FunctionDeclaration(func_decl) = stmt {
            let func_name = func_decl
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .unwrap_or("");

            if !func_name.is_empty() {
                if let Some(decoder) = self.detect_decoder_function(func_decl, func_name, ctx) {
                    eprintln!(
                        "[AST] Storing decoder: {} -> array={}, offset={}, op={:?}",
                        decoder.function_name,
                        decoder.array_name,
                        decoder.offset,
                        decoder.offset_operation
                    );
                    self.detected_decoders
                        .insert(decoder.function_name.clone(), decoder);
                }
            }
        }
    }

    fn enter_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        if let Expression::CallExpression(call) = expr {
            if let Some(new_expr) = self.try_inline_decoder_call(call, ctx) {
                *expr = new_expr;
                self.changed = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_deobfuscate::{DeobfuscateState, StringArrayInliner, StringArrayRotation};
    use oxc_allocator::Allocator;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;

    fn run_decoder_inline(code: &str) -> (String, DecoderInliner) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut rotation = StringArrayRotation::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = oxc_traverse::ReusableTraverseCtx::new(state, scoping, &allocator);

        oxc_traverse::traverse_mut_with_ctx(&mut rotation, &mut program, &mut ctx);

        let mut state = DeobfuscateState::new();
        rotation.finalize(&mut state);

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = oxc_traverse::ReusableTraverseCtx::new(state, scoping, &allocator);

        let mut inliner = StringArrayInliner::new();
        oxc_traverse::traverse_mut_with_ctx(&mut inliner, &mut program, &mut ctx);

        let mut decoder = DecoderInliner::new();
        oxc_traverse::traverse_mut_with_ctx(&mut decoder, &mut program, &mut ctx);

        let output = Codegen::new().build(&program).code;
        (output, decoder)
    }

    #[test]
    fn test_simple_decoder() {
        let code = r#"
            var _0x1234 = ["hello", "world", "test"];
            function _0xdec(a) {
                return _0x1234[a];
            }
            console.log(_0xdec(0));
            console.log(_0xdec(1));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed(), "Should have inlined decoder calls");
        assert!(
            output.contains("\"hello\""),
            "Should contain inlined 'hello'"
        );
        assert!(
            output.contains("\"world\""),
            "Should contain inlined 'world'"
        );
        assert!(
            !output.contains("_0xdec(0)"),
            "Should not contain decoder call"
        );
    }

    #[test]
    fn test_decoder_with_offset() {
        let code = r#"
            var _0xabcd = ["foo", "bar", "baz"];
            function _0xdecode(a) {
                return _0xabcd[a - 100];
            }
            console.log(_0xdecode(100));
            console.log(_0xdecode(101));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed());
        assert!(output.contains("\"foo\""), "Should inline with offset");
        assert!(output.contains("\"bar\""), "Should inline with offset");
    }

    #[test]
    fn test_decoder_with_assignment_offset() {
        let code = r#"
            var _0x1234 = ["a", "b", "c"];
            function _0xdec(a) {
                a = a - 291;
                return _0x1234[a];
            }
            console.log(_0xdec(291));
            console.log(_0xdec(292));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed());
        assert!(output.contains("\"a\""));
        assert!(output.contains("\"b\""));
    }

    #[test]
    fn test_non_constant_arg() {
        let code = r#"
            var _0x1234 = ["hello", "world"];
            function _0xdec(a) {
                return _0x1234[a];
            }
            var i = 0;
            console.log(_0xdec(i));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(!decoder.has_changed(), "Should not inline variable arg");
        assert!(
            output.contains("_0xdec(i)"),
            "Should preserve variable arg call"
        );
    }

    #[test]
    fn test_decoder_with_rotated_array() {
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
            function _0xdec(a) {
                return _0x1111[a];
            }
            console.log(_0xdec(0));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed());
        assert!(output.contains("\"c\""), "Should use rotated array");
    }
}
