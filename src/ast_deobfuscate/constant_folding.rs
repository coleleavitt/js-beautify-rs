use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_traverse::{Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct ConstantFolder {
    changed: bool,
}

impl ConstantFolder {
    pub fn new() -> Self {
        Self { changed: false }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn try_fold_binary<'a>(
        &mut self,
        expr: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        let left_val = Self::extract_number(&expr.left)?;
        let right_val = Self::extract_number(&expr.right)?;

        let result = match expr.operator {
            BinaryOperator::Addition => left_val.checked_add(right_val)?,
            BinaryOperator::Subtraction => left_val.checked_sub(right_val)?,
            BinaryOperator::Multiplication => left_val.checked_mul(right_val)?,
            BinaryOperator::Division => {
                if right_val == 0 {
                    return None;
                }
                left_val.checked_div(right_val)?
            }
            BinaryOperator::Remainder => {
                if right_val == 0 {
                    return None;
                }
                left_val.checked_rem(right_val)?
            }
            BinaryOperator::BitwiseAnd => left_val & right_val,
            BinaryOperator::BitwiseOR => left_val | right_val,
            BinaryOperator::BitwiseXOR => left_val ^ right_val,
            BinaryOperator::ShiftLeft => {
                let shift = (right_val & 0x1F) as u32;
                (left_val as i32).checked_shl(shift)? as i64
            }
            BinaryOperator::ShiftRight => {
                let shift = (right_val & 0x1F) as u32;
                (left_val as i32).checked_shr(shift)? as i64
            }
            BinaryOperator::ShiftRightZeroFill => {
                let shift = (right_val & 0x1F) as u32;
                ((left_val as u32) >> shift) as i64
            }
            _ => return None,
        };

        eprintln!(
            "[AST] Folding: {} {:?} {} = {}",
            left_val, expr.operator, right_val, result
        );

        self.changed = true;
        Some(Self::make_number(result, ctx))
    }

    fn try_fold_comparison<'a>(
        &mut self,
        expr: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        let left_val = Self::extract_number(&expr.left)?;
        let right_val = Self::extract_number(&expr.right)?;

        let result = match expr.operator {
            BinaryOperator::StrictEquality | BinaryOperator::Equality => left_val == right_val,
            BinaryOperator::StrictInequality | BinaryOperator::Inequality => left_val != right_val,
            BinaryOperator::LessThan => left_val < right_val,
            BinaryOperator::LessEqualThan => left_val <= right_val,
            BinaryOperator::GreaterThan => left_val > right_val,
            BinaryOperator::GreaterEqualThan => left_val >= right_val,
            _ => return None,
        };

        eprintln!(
            "[AST] Folding comparison: {} {:?} {} = {}",
            left_val, expr.operator, right_val, result
        );

        self.changed = true;
        Some(Self::make_boolean(result, ctx))
    }

    fn try_fold_logical<'a>(
        &mut self,
        expr: &LogicalExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        let left_bool = Self::extract_boolean(&expr.left)?;
        let right_bool = Self::extract_boolean(&expr.right)?;

        let result = match expr.operator {
            LogicalOperator::And => left_bool && right_bool,
            LogicalOperator::Or => left_bool || right_bool,
            LogicalOperator::Coalesce => return None,
        };

        eprintln!(
            "[AST] Folding logical: {} {:?} {} = {}",
            left_bool, expr.operator, right_bool, result
        );

        self.changed = true;
        Some(Self::make_boolean(result, ctx))
    }

    fn try_fold_unary<'a>(
        &mut self,
        expr: &UnaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        match expr.operator {
            UnaryOperator::UnaryNegation => {
                let val = Self::extract_number(&expr.argument)?;
                let result = val.checked_neg()?;
                eprintln!("[AST] Folding unary: -{} = {}", val, result);
                self.changed = true;
                Some(Self::make_number(result, ctx))
            }
            UnaryOperator::BitwiseNot => {
                let val = Self::extract_number(&expr.argument)?;
                let result = !val;
                eprintln!("[AST] Folding unary: ~{} = {}", val, result);
                self.changed = true;
                Some(Self::make_number(result, ctx))
            }
            UnaryOperator::LogicalNot => {
                let val = Self::extract_boolean(&expr.argument)?;
                let result = !val;
                eprintln!("[AST] Folding unary: !{} = {}", val, result);
                self.changed = true;
                Some(Self::make_boolean(result, ctx))
            }
            UnaryOperator::UnaryPlus => {
                let val = Self::extract_number(&expr.argument)?;
                eprintln!("[AST] Folding unary: +{} = {}", val, val);
                self.changed = true;
                Some(Self::make_number(val, ctx))
            }
            _ => None,
        }
    }

    fn extract_number(expr: &Expression<'_>) -> Option<i64> {
        match expr {
            Expression::NumericLiteral(lit) => {
                let val = lit.value;
                if val.fract() != 0.0 || val > i64::MAX as f64 || val < i64::MIN as f64 {
                    return None;
                }
                Some(val as i64)
            }
            Expression::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation =>
            {
                let inner = Self::extract_number(&unary.argument)?;
                inner.checked_neg()
            }
            _ => None,
        }
    }

    fn extract_boolean(expr: &Expression<'_>) -> Option<bool> {
        match expr {
            Expression::BooleanLiteral(lit) => Some(lit.value),
            _ => None,
        }
    }

    fn make_number<'a>(val: i64, ctx: &mut Ctx<'a>) -> Expression<'a> {
        let raw = Some(ctx.ast.atom(&val.to_string()));

        Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
            span: SPAN,
            value: val as f64,
            raw,
            base: NumberBase::Decimal,
        }))
    }

    fn make_boolean<'a>(val: bool, ctx: &mut Ctx<'a>) -> Expression<'a> {
        Expression::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
            span: SPAN,
            value: val,
        }))
    }
}

impl Default for ConstantFolder {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for ConstantFolder {
    fn exit_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        let replacement = match expr {
            Expression::BinaryExpression(binary) => {
                if matches!(
                    binary.operator,
                    BinaryOperator::StrictEquality
                        | BinaryOperator::Equality
                        | BinaryOperator::StrictInequality
                        | BinaryOperator::Inequality
                        | BinaryOperator::LessThan
                        | BinaryOperator::LessEqualThan
                        | BinaryOperator::GreaterThan
                        | BinaryOperator::GreaterEqualThan
                ) {
                    self.try_fold_comparison(binary, ctx)
                } else {
                    self.try_fold_binary(binary, ctx)
                }
            }
            Expression::LogicalExpression(logical) => self.try_fold_logical(logical, ctx),
            Expression::UnaryExpression(unary) => self.try_fold_unary(unary, ctx),
            _ => None,
        };

        if let Some(new_expr) = replacement {
            *expr = new_expr;
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
    use oxc_traverse::{ReusableTraverseCtx, traverse_mut_with_ctx};

    fn run_fold(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut folder = ConstantFolder::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut folder, &mut program, &mut ctx);

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_fold_addition() {
        let output = run_fold("var x = 5 + 10;");
        assert!(
            output.contains("15"),
            "Should fold 5 + 10 to 15, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_multiplication() {
        let output = run_fold("var x = 5 * 16;");
        assert!(
            output.contains("80"),
            "Should fold 5 * 16 to 80, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_subtraction() {
        let output = run_fold("var x = 100 - 30;");
        assert!(
            output.contains("70"),
            "Should fold 100 - 30 to 70, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_division() {
        let output = run_fold("var x = 100 / 5;");
        assert!(
            output.contains("20"),
            "Should fold 100 / 5 to 20, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_bitwise_and() {
        let output = run_fold("var x = 12 & 10;");
        assert!(
            output.contains("8"),
            "Should fold 12 & 10 to 8, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_bitwise_or() {
        let output = run_fold("var x = 12 | 10;");
        assert!(
            output.contains("14"),
            "Should fold 12 | 10 to 14, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_bitwise_xor() {
        let output = run_fold("var x = 12 ^ 10;");
        assert!(
            output.contains("6"),
            "Should fold 12 ^ 10 to 6, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_shift_left() {
        let output = run_fold("var x = 5 << 2;");
        assert!(
            output.contains("20"),
            "Should fold 5 << 2 to 20, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_shift_right() {
        let output = run_fold("var x = 20 >> 2;");
        assert!(
            output.contains("5"),
            "Should fold 20 >> 2 to 5, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_comparison_equal() {
        let output = run_fold("var x = 10 === 10;");
        assert!(
            output.contains("true"),
            "Should fold 10 === 10 to true, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_comparison_not_equal() {
        let output = run_fold("var x = 10 !== 5;");
        assert!(
            output.contains("true"),
            "Should fold 10 !== 5 to true, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_comparison_less_than() {
        let output = run_fold("var x = 5 < 10;");
        assert!(
            output.contains("true"),
            "Should fold 5 < 10 to true, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_comparison_greater_than() {
        let output = run_fold("var x = 10 > 5;");
        assert!(
            output.contains("true"),
            "Should fold 10 > 5 to true, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_logical_and() {
        let output = run_fold("var x = true && false;");
        assert!(
            output.contains("false"),
            "Should fold true && false to false, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_logical_or() {
        let output = run_fold("var x = true || false;");
        assert!(
            output.contains("true"),
            "Should fold true || false to true, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_unary_negation() {
        let output = run_fold("var x = -(-42);");
        assert!(
            output.contains("42"),
            "Should fold -(-42) to 42, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_unary_not() {
        let output = run_fold("var x = !false;");
        assert!(
            output.contains("true"),
            "Should fold !false to true, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_chained() {
        let output = run_fold("var x = 2 + 3 * 4;");
        eprintln!("Chained output: {}", output);
        assert!(
            output.contains("14"),
            "Should fold 2 + 3 * 4 to 14, got: {}",
            output
        );
    }

    #[test]
    fn test_no_fold_division_by_zero() {
        let output = run_fold("var x = 10 / 0;");
        assert!(
            output.contains("/"),
            "Should NOT fold division by zero, got: {}",
            output
        );
    }
}
