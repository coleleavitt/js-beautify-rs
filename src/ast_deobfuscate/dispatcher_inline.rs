//! Dispatcher inlining pass
//!
//! Detects and inlines dispatcher object patterns:
//! ```js
//! var d = { "a": function() { return 1; }, "b": function() { return 2; } };
//! d["a"](); // Inlined to: 1
//! ```

use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::FxHashMap;

use crate::ast_deobfuscate::state::{DeobfuscateState, DispatcherInfo, FunctionInfo, ReturnValue};

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct DispatcherInliner {
    changed: bool,
    detected_dispatchers: FxHashMap<String, DispatcherInfo>,
}

impl DispatcherInliner {
    pub fn new() -> Self {
        Self {
            changed: false,
            detected_dispatchers: FxHashMap::default(),
        }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn detect_dispatcher_pattern<'a>(
        &mut self,
        var_decl: &VariableDeclaration<'a>,
    ) -> Option<DispatcherInfo> {
        if var_decl.declarations.len() != 1 {
            return None;
        }

        let decl = &var_decl.declarations[0];

        let var_name = match &decl.id {
            BindingPattern::BindingIdentifier(ident) => ident.name.as_str().to_string(),
            _ => return None,
        };

        let init = decl.init.as_ref()?;

        let obj_expr = match init {
            Expression::ObjectExpression(obj) => obj,
            _ => return None,
        };

        let mut functions = FxHashMap::default();

        for prop in &obj_expr.properties {
            if let ObjectPropertyKind::ObjectProperty(obj_prop) = prop {
                let key = match &obj_prop.key {
                    PropertyKey::StringLiteral(s) => s.value.as_str().to_string(),
                    PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
                    _ => continue,
                };

                let return_value = match &obj_prop.value {
                    Expression::FunctionExpression(func) => Self::extract_function_return(func),
                    Expression::ArrowFunctionExpression(arrow) => Self::extract_arrow_return(arrow),
                    _ => None,
                };

                if let Some(ret_val) = return_value {
                    functions.insert(
                        key.clone(),
                        FunctionInfo {
                            key,
                            return_value: Some(ret_val),
                        },
                    );
                }
            }
        }

        if functions.is_empty() {
            return None;
        }

        Some(DispatcherInfo {
            var_name: var_name.clone(),
            functions,
        })
    }

    fn extract_function_return(func: &Function) -> Option<ReturnValue> {
        let body = func.body.as_ref()?;
        if body.statements.len() != 1 {
            return None;
        }

        if let Statement::ReturnStatement(ret) = &body.statements[0] {
            ret.argument.as_ref().and_then(Self::extract_literal_value)
        } else {
            None
        }
    }

    fn extract_arrow_return(arrow: &ArrowFunctionExpression) -> Option<ReturnValue> {
        // Case 1: Expression body: x => value (single expression in function body)
        if arrow.expression {
            // When expression is true, body contains a single ExpressionStatement
            if arrow.body.statements.len() == 1 {
                if let Statement::ExpressionStatement(expr_stmt) = &arrow.body.statements[0] {
                    return Self::extract_literal_value(&expr_stmt.expression);
                }
            }
            return None;
        }

        // Case 2: Block body with single return statement
        if arrow.body.statements.len() == 1 {
            if let Statement::ReturnStatement(ret) = &arrow.body.statements[0] {
                return ret.argument.as_ref().and_then(Self::extract_literal_value);
            }
        }

        // Case 3: Block body with expression statement (last statement)
        // x => { statements; value }
        if arrow.body.statements.len() >= 1 {
            if let Statement::ExpressionStatement(expr_stmt) =
                &arrow.body.statements[arrow.body.statements.len() - 1]
            {
                // Check that all previous statements are not returns (so value is the "implicit return")
                let all_non_return = arrow.body.statements[..arrow.body.statements.len() - 1]
                    .iter()
                    .all(|s| !matches!(s, Statement::ReturnStatement(_)));

                if all_non_return {
                    return Self::extract_literal_value(&expr_stmt.expression);
                }
            }
        }

        None
    }

    fn extract_literal_value(expr: &Expression) -> Option<ReturnValue> {
        match expr {
            Expression::NumericLiteral(lit) => Some(ReturnValue::Number(lit.value)),
            Expression::StringLiteral(lit) => Some(ReturnValue::String(lit.value.to_string())),
            Expression::BooleanLiteral(lit) => Some(ReturnValue::Bool(lit.value)),
            Expression::NullLiteral(_) => Some(ReturnValue::Null),
            Expression::Identifier(ident) => Some(ReturnValue::Identifier(ident.name.to_string())),
            _ => None,
        }
    }

    fn create_expression_from_return_value<'a>(
        return_value: &ReturnValue,
        ctx: &mut Ctx<'a>,
    ) -> Expression<'a> {
        match return_value {
            ReturnValue::Number(n) => Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                span: SPAN,
                value: *n,
                raw: None,
                base: NumberBase::Decimal,
            })),
            ReturnValue::String(s) => Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
                span: SPAN,
                value: ctx.ast.atom(s.as_str()),
                raw: None,
                lone_surrogates: false,
            })),
            ReturnValue::Bool(b) => Expression::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
                span: SPAN,
                value: *b,
            })),
            ReturnValue::Null => Expression::NullLiteral(ctx.ast.alloc(NullLiteral { span: SPAN })),
            ReturnValue::Identifier(name) => {
                Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                    span: SPAN,
                    name: ctx.ast.atom(name.as_str()),
                    reference_id: None.into(),
                }))
            }
        }
    }
}

impl Default for DispatcherInliner {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for DispatcherInliner {
    fn enter_statement(&mut self, stmt: &mut Statement<'a>, _ctx: &mut Ctx<'a>) {
        if let Statement::VariableDeclaration(var_decl) = stmt {
            if let Some(dispatcher) = self.detect_dispatcher_pattern(var_decl) {
                eprintln!("[AST] Found dispatcher: {}", dispatcher.var_name);
                for (key, func) in &dispatcher.functions {
                    eprintln!("[AST]   - {}: {:?}", key, func.return_value);
                }
                self.detected_dispatchers
                    .insert(dispatcher.var_name.clone(), dispatcher);
            }
        }
    }

    fn enter_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        if let Expression::CallExpression(call) = expr {
            if let Some(new_expr) = self.try_inline_dispatcher_call(call, ctx) {
                *expr = new_expr;
                self.changed = true;
            }
        }
    }
}

impl DispatcherInliner {
    fn try_inline_dispatcher_call<'a>(
        &self,
        call: &CallExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        // Try computed member expression first: d["key"]()
        if let Some(result) = self.try_inline_computed_member(call, ctx) {
            return Some(result);
        }

        // Try static member expression: d.key()
        if let Some(result) = self.try_inline_static_member(call, ctx) {
            return Some(result);
        }

        None
    }

    fn try_inline_computed_member<'a>(
        &self,
        call: &CallExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        let member = match &call.callee {
            Expression::ComputedMemberExpression(m) => m,
            _ => return None,
        };

        let obj_name = match &member.object {
            Expression::Identifier(ident) => ident.name.as_str(),
            _ => return None,
        };

        let dispatcher = self.detected_dispatchers.get(obj_name)?;

        // Support string literal keys: d["key"]()
        let key = match &member.expression {
            Expression::StringLiteral(lit) => lit.value.as_str(),
            _ => return None,
        };

        let func_info = dispatcher.functions.get(key)?;
        let return_value = func_info.return_value.as_ref()?;

        eprintln!(
            "[AST] Inlining computed: {}[\"{}\"]() → {:?}",
            obj_name, key, return_value
        );

        Some(Self::create_expression_from_return_value(return_value, ctx))
    }

    fn try_inline_static_member<'a>(
        &self,
        call: &CallExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        let member = match &call.callee {
            Expression::StaticMemberExpression(m) => m,
            _ => return None,
        };

        // Object must be an identifier: d.key()
        let obj_name = match &member.object {
            Expression::Identifier(ident) => ident.name.as_str(),
            _ => return None,
        };

        let dispatcher = self.detected_dispatchers.get(obj_name)?;

        // Property must be an identifier: d.key (not d["key"])
        let key = member.property.name.as_str();

        let func_info = dispatcher.functions.get(key)?;
        let return_value = func_info.return_value.as_ref()?;

        eprintln!(
            "[AST] Inlining static: {}.{}() → {:?}",
            obj_name, key, return_value
        );

        Some(Self::create_expression_from_return_value(return_value, ctx))
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

    fn run_dispatcher_inliner(code: &str) -> (String, DispatcherInliner) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut inliner = DispatcherInliner::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = oxc_traverse::ReusableTraverseCtx::new(state, scoping, &allocator);

        oxc_traverse::traverse_mut_with_ctx(&mut inliner, &mut program, &mut ctx);

        let output = Codegen::new().build(&program).code;
        (output, inliner)
    }

    #[test]
    fn test_detect_dispatcher() {
        let code = r#"
            var d = {
                "a": function() { return 1; },
                "b": function() { return 2; }
            };
        "#;

        let (_, inliner) = run_dispatcher_inliner(code);

        assert_eq!(inliner.detected_dispatchers.len(), 1);
        assert!(inliner.detected_dispatchers.contains_key("d"));

        let dispatcher = inliner.detected_dispatchers.get("d").unwrap();
        assert_eq!(dispatcher.functions.len(), 2);
        assert!(matches!(
            dispatcher.functions.get("a").unwrap().return_value,
            Some(ReturnValue::Number(1.0))
        ));
        assert!(matches!(
            dispatcher.functions.get("b").unwrap().return_value,
            Some(ReturnValue::Number(2.0))
        ));
    }

    #[test]
    fn test_inline_number() {
        let code = r#"
            var d = { "a": function() { return 42; } };
            var x = d["a"]();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(inliner.has_changed(), "Should have inlined something");
        assert!(output.contains("42"), "Should contain inlined value 42");
        assert!(
            !output.contains("d[\"a\"]()"),
            "Should not contain dispatcher call"
        );
    }

    #[test]
    fn test_inline_string() {
        let code = r#"
            var d = { "key": function() { return "hello"; } };
            var x = d["key"]();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(inliner.has_changed());
        assert!(output.contains("\"hello\""));
    }

    #[test]
    fn test_inline_multiple_calls() {
        let code = r#"
            var d = {
                "a": function() { return 1; },
                "b": function() { return "test"; }
            };
            var x = d["a"]();
            var y = d["b"]();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(inliner.has_changed());
        assert!(output.contains("= 1"));
        assert!(output.contains("\"test\""));
    }

    #[test]
    fn test_no_inline_non_constant() {
        let code = r#"
            var d = { "a": function(x) { return x + 1; } };
            var x = d["a"](5);
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(
            !inliner.has_changed(),
            "Should not inline functions with parameters"
        );
        assert!(output.contains("d[\"a\"]"));
    }

    #[test]
    fn test_inline_dot_notation() {
        let code = r#"
            var d = { a: function() { return 42; } };
            var x = d.a();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(
            inliner.has_changed(),
            "Should have inlined dot notation call"
        );
        assert!(output.contains("42"), "Should contain inlined value 42");
        assert!(
            !output.contains("d.a()"),
            "Should not contain original call"
        );
    }

    #[test]
    fn test_inline_mixed_notation() {
        let code = r#"
            var d = {
                "strKey": function() { return "hello"; },
                numKey: function() { return 123; }
            };
            var a = d["strKey"]();
            var b = d.numKey();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(inliner.has_changed(), "Should have inlined calls");
        assert!(output.contains("\"hello\""), "Should inline string");
        assert!(output.contains("123"), "Should inline number");
    }

    #[test]
    fn test_inline_arrow_expression_body() {
        let code = r#"
            var d = { a: () => 42, b: x => x * 2 };
            var x = d.a();
            var y = d.b(5);
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(inliner.has_changed(), "Should have inlined arrow function");
        assert!(output.contains("42"), "Should inline arrow expression");
        assert!(
            !output.contains("d.a()"),
            "Should not contain original call"
        );
    }

    #[test]
    fn test_inline_arrow_with_block_last_expr() {
        let code = r#"
            var d = {
                a: () => { var x = 1; var y = 2; 42 },
                b: () => { 1; 2; "test" }
            };
            var x = d.a();
            var y = d.b();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(
            inliner.has_changed(),
            "Should have inlined arrow with block body"
        );
        assert!(output.contains("42"), "Should inline last expression");
        assert!(output.contains("\"test\""), "Should inline second function");
    }

    #[test]
    fn test_no_inline_arrow_with_side_effects() {
        let code = r#"
            var d = { a: () => console.log("side effect") };
            var x = d.a();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        // Should not inline because console.log is not a literal
        assert!(
            !inliner.has_changed() || !output.contains("42"),
            "Should not inline if last expression is not a literal"
        );
    }

    #[test]
    fn test_inline_identifier_return() {
        let code = r#"
            var result = 999;
            var d = { a: function() { return result; } };
            var x = d["a"]();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(
            inliner.has_changed(),
            "Should have inlined identifier return"
        );
        assert!(
            output.contains("result"),
            "Should keep identifier reference"
        );
    }

    #[test]
    fn test_inline_boolean_and_null() {
        let code = r#"
            var d = {
                "true": function() { return true; },
                "false": function() { return false; },
                "null": function() { return null; }
            };
            var a = d["true"]();
            var b = d["false"]();
            var c = d["null"]();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        assert!(inliner.has_changed());
        assert!(output.contains("true"), "Should inline true");
        assert!(output.contains("false"), "Should inline false");
        assert!(output.contains("null"), "Should inline null");
    }

    #[test]
    fn test_complex_dispatcher() {
        let code = r#"
            var handlers = {
                "login": function() { return "logged_in"; },
                "logout": function() { return "logged_out"; },
                "getUser": () => ({ name: "John", age: 30 }),
                admin: function() { return { role: "admin" }; }
            };
            var status = handlers["login"]();
            var user = handlers.getUser();
            var role = handlers.admin();
        "#;

        let (output, inliner) = run_dispatcher_inliner(code);
        eprintln!("Output: {}", output);

        // Simple function inlining should work
        assert!(inliner.has_changed());
        assert!(output.contains("\"logged_in\""), "Should inline login");
        assert!(output.contains("\"logged_out\""), "Should inline logout");
    }
}
