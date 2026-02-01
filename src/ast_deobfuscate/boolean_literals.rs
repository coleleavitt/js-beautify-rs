use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct BooleanLiteralConverter {
    changed: bool,
}

impl BooleanLiteralConverter {
    pub fn new() -> Self {
        Self { changed: false }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }
}

impl Default for BooleanLiteralConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for BooleanLiteralConverter {
    fn exit_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        if let Expression::UnaryExpression(unary) = expr {
            if unary.operator != UnaryOperator::LogicalNot {
                return;
            }

            if let Expression::NumericLiteral(num) = &unary.argument {
                let value = num.value;

                if value == 0.0 {
                    eprintln!("[AST] Converting !0 -> true");
                    self.changed = true;
                    *expr = Expression::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
                        span: SPAN,
                        value: true,
                    }));
                } else if value == 1.0 {
                    eprintln!("[AST] Converting !1 -> false");
                    self.changed = true;
                    *expr = Expression::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
                        span: SPAN,
                        value: false,
                    }));
                }
            }
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

    fn run_boolean_literals(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut converter = BooleanLiteralConverter::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut converter, &mut program, &mut ctx);

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_not_zero_to_true() {
        let output = run_boolean_literals("var x = !0;");
        eprintln!("Output: {}", output);
        assert!(output.contains("true"), "Expected true, got: {}", output);
    }

    #[test]
    fn test_not_one_to_false() {
        let output = run_boolean_literals("var x = !1;");
        eprintln!("Output: {}", output);
        assert!(output.contains("false"), "Expected false, got: {}", output);
    }

    #[test]
    fn test_preserve_other_not() {
        let output = run_boolean_literals("var x = !5;");
        eprintln!("Output: {}", output);
        assert!(output.contains("!5"), "Should preserve !5, got: {}", output);
    }

    #[test]
    fn test_preserve_not_variable() {
        let output = run_boolean_literals("var x = !foo;");
        eprintln!("Output: {}", output);
        assert!(
            output.contains("!foo"),
            "Should preserve !foo, got: {}",
            output
        );
    }
}
