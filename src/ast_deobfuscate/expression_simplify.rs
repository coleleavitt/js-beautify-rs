use oxc_allocator::CloneIn;
use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_traverse::{Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct ExpressionSimplifier {
    changed: bool,
}

impl ExpressionSimplifier {
    pub fn new() -> Self {
        Self { changed: false }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn try_simplify_not_number<'a>(
        &mut self,
        unary: &UnaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if unary.operator != UnaryOperator::LogicalNot {
            return None;
        }

        if let Expression::NumericLiteral(num) = &unary.argument {
            let result = num.value == 0.0;
            eprintln!("[AST] Simplifying !{} to {}", num.value as i64, result);
            self.changed = true;
            return Some(Self::make_boolean(result, ctx));
        }

        None
    }

    fn try_simplify_double_not<'a>(
        &mut self,
        unary: &UnaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if unary.operator != UnaryOperator::LogicalNot {
            return None;
        }

        if let Expression::UnaryExpression(inner_unary) = &unary.argument {
            if inner_unary.operator != UnaryOperator::LogicalNot {
                return None;
            }

            if let Expression::ArrayExpression(arr) = &inner_unary.argument {
                if arr.elements.is_empty() {
                    eprintln!("[AST] Simplifying !![] to true");
                    self.changed = true;
                    return Some(Self::make_boolean(true, ctx));
                }
            }

            if let Expression::NumericLiteral(num) = &inner_unary.argument {
                let result = num.value != 0.0;
                eprintln!("[AST] Simplifying !!{} to {}", num.value as i64, result);
                self.changed = true;
                return Some(Self::make_boolean(result, ctx));
            }
        }

        if let Expression::BooleanLiteral(bool_lit) = &unary.argument {
            let result = !bool_lit.value;
            eprintln!("[AST] Simplifying !{} to {}", bool_lit.value, result);
            self.changed = true;
            return Some(Self::make_boolean(result, ctx));
        }

        None
    }

    fn try_simplify_plus_array<'a>(
        &mut self,
        unary: &UnaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if unary.operator != UnaryOperator::UnaryPlus {
            return None;
        }

        if let Expression::ArrayExpression(arr) = &unary.argument {
            if arr.elements.is_empty() {
                eprintln!("[AST] Simplifying +[] to 0");
                self.changed = true;
                return Some(Self::make_number(0, ctx));
            }
        }

        if let Expression::UnaryExpression(inner_unary) = &unary.argument {
            if inner_unary.operator == UnaryOperator::LogicalNot {
                if let Expression::ArrayExpression(arr) = &inner_unary.argument {
                    if arr.elements.is_empty() {
                        eprintln!("[AST] Simplifying +![] to 1");
                        self.changed = true;
                        return Some(Self::make_number(1, ctx));
                    }
                }
            }
        }

        None
    }

    fn try_simplify_void<'a>(
        &mut self,
        unary: &UnaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if unary.operator != UnaryOperator::Void {
            return None;
        }

        eprintln!("[AST] Simplifying void expr to undefined");
        self.changed = true;
        Some(Expression::Identifier(ctx.ast.alloc(IdentifierReference {
            span: SPAN,
            name: ctx.ast.atom("undefined").into(),
            reference_id: Default::default(),
        })))
    }

    fn try_simplify_bracket_to_dot<'a>(
        &mut self,
        member: &ComputedMemberExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if let Expression::StringLiteral(lit) = &member.expression {
            let prop_name = lit.value.as_str();

            if Self::is_valid_identifier(prop_name) {
                eprintln!("[AST] Converting [\"{}\"]] to .{}", prop_name, prop_name);
                self.changed = true;

                return Some(Expression::StaticMemberExpression(ctx.ast.alloc(
                    StaticMemberExpression {
                        span: SPAN,
                        object: Self::clone_expression(&member.object, ctx),
                        property: IdentifierName {
                            span: SPAN,
                            name: ctx.ast.atom(prop_name).into(),
                        },
                        optional: member.optional,
                    },
                )));
            }
        }

        None
    }

    fn try_simplify_string_concat<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::Addition {
            return None;
        }

        if let (Expression::StringLiteral(left), Expression::StringLiteral(right)) =
            (&binary.left, &binary.right)
        {
            let combined = format!("{}{}", left.value.as_str(), right.value.as_str());
            eprintln!(
                "[AST] Concatenating \"{}\" + \"{}\" = \"{}\"",
                left.value.as_str(),
                right.value.as_str(),
                combined
            );
            self.changed = true;

            return Some(Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
                span: SPAN,
                value: ctx.ast.atom(&combined),
                raw: None,
                lone_surrogates: false,
            })));
        }

        None
    }

    fn is_valid_identifier(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        let mut chars = s.chars();
        let first = chars.next().unwrap();

        if !first.is_alphabetic() && first != '_' && first != '$' {
            return false;
        }

        chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    }

    fn clone_expression<'a>(expr: &Expression<'a>, ctx: &mut Ctx<'a>) -> Expression<'a> {
        expr.clone_in_with_semantic_ids(ctx.ast.allocator)
    }

    fn make_boolean<'a>(val: bool, ctx: &mut Ctx<'a>) -> Expression<'a> {
        Expression::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
            span: SPAN,
            value: val,
        }))
    }

    fn make_number<'a>(val: i64, ctx: &mut Ctx<'a>) -> Expression<'a> {
        Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
            span: SPAN,
            value: val as f64,
            raw: Some(ctx.ast.atom(&val.to_string())),
            base: NumberBase::Decimal,
        }))
    }
}

impl Default for ExpressionSimplifier {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for ExpressionSimplifier {
    fn exit_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        let replacement = match expr {
            Expression::UnaryExpression(unary) => self
                .try_simplify_double_not(unary, ctx)
                .or_else(|| self.try_simplify_plus_array(unary, ctx))
                .or_else(|| self.try_simplify_not_number(unary, ctx))
                .or_else(|| self.try_simplify_void(unary, ctx)),
            Expression::ComputedMemberExpression(member) => {
                self.try_simplify_bracket_to_dot(member, ctx)
            }
            Expression::BinaryExpression(binary) => self.try_simplify_string_concat(binary, ctx),
            _ => None,
        };

        if let Some(new_expr) = replacement {
            *expr = new_expr;
        }
    }

    fn exit_statement(&mut self, stmt: &mut Statement<'a>, _ctx: &mut Ctx<'a>) {
        if let Statement::DebuggerStatement(_) = stmt {
            eprintln!("[AST] Removing debugger statement");
            self.changed = true;
            *stmt = Statement::EmptyStatement(oxc_allocator::Box::new_in(
                EmptyStatement { span: SPAN },
                _ctx.ast.allocator,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_deobfuscate::DeobfuscateState;
    use oxc_allocator::Allocator;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use oxc_traverse::{traverse_mut_with_ctx, ReusableTraverseCtx};

    fn run_simplify(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut simplifier = ExpressionSimplifier::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut simplifier, &mut program, &mut ctx);

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_not_zero_to_true() {
        let output = run_simplify("var x = !0;");
        assert!(
            output.contains("true"),
            "Should convert !0 to true, got: {}",
            output
        );
    }

    #[test]
    fn test_not_one_to_false() {
        let output = run_simplify("var x = !1;");
        assert!(
            output.contains("false"),
            "Should convert !1 to false, got: {}",
            output
        );
    }

    #[test]
    fn test_double_not_array_to_true() {
        let output = run_simplify("var x = !![];");
        assert!(
            output.contains("true"),
            "Should convert !![] to true, got: {}",
            output
        );
    }

    #[test]
    fn test_double_not_zero_to_false() {
        let output = run_simplify("var x = !!0;");
        assert!(
            output.contains("false"),
            "Should convert !!0 to false, got: {}",
            output
        );
    }

    #[test]
    fn test_plus_array_to_zero() {
        let output = run_simplify("var x = +[];");
        assert!(
            output.contains("0"),
            "Should convert +[] to 0, got: {}",
            output
        );
    }

    #[test]
    fn test_plus_not_array_to_one() {
        let output = run_simplify("var x = +![];");
        assert!(
            output.contains("1"),
            "Should convert +![] to 1, got: {}",
            output
        );
    }

    #[test]
    fn test_void_to_undefined() {
        let output = run_simplify("var x = void 0;");
        assert!(
            output.contains("undefined"),
            "Should convert void 0 to undefined, got: {}",
            output
        );
    }

    #[test]
    fn test_bracket_to_dot_notation() {
        let output = run_simplify("obj[\"property\"];");
        assert!(
            output.contains(".property"),
            "Should convert bracket to dot notation, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_invalid_identifiers() {
        let output = run_simplify("obj[\"some-property\"];");
        assert!(
            output.contains("[\"some-property\"]"),
            "Should preserve bracket notation for invalid identifiers, got: {}",
            output
        );
    }

    #[test]
    fn test_string_concat() {
        let output = run_simplify("var x = \"Hel\" + \"lo\";");
        assert!(
            output.contains("Hello"),
            "Should concatenate strings, got: {}",
            output
        );
    }

    #[test]
    fn test_remove_debugger() {
        let output = run_simplify("var x = 1; debugger; var y = 2;");
        assert!(
            !output.contains("debugger"),
            "Should remove debugger statement, got: {}",
            output
        );
    }
}
