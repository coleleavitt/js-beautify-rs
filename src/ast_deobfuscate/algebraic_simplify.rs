use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_traverse::{Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct AlgebraicSimplifier {
    changed: bool,
}

impl AlgebraicSimplifier {
    pub fn new() -> Self {
        Self { changed: false }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn try_simplify_self_subtract<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::Subtraction {
            return None;
        }

        if let (Expression::Identifier(left), Expression::Identifier(right)) =
            (&binary.left, &binary.right)
        {
            if left.name == right.name {
                eprintln!("[AST] Simplifying {} - {} to 0", left.name, right.name);
                self.changed = true;
                return Some(Self::make_number(0, ctx));
            }
        }

        None
    }

    fn try_simplify_multiply_zero<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::Multiplication {
            return None;
        }

        let left_is_zero = Self::is_zero(&binary.left);
        let right_is_zero = Self::is_zero(&binary.right);

        if left_is_zero || right_is_zero {
            eprintln!("[AST] Simplifying x * 0 to 0");
            self.changed = true;
            return Some(Self::make_number(0, ctx));
        }

        None
    }

    fn try_simplify_self_divide<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::Division {
            return None;
        }

        if let (Expression::Identifier(left), Expression::Identifier(right)) =
            (&binary.left, &binary.right)
        {
            if left.name == right.name {
                eprintln!("[AST] Simplifying {} / {} to 1", left.name, right.name);
                self.changed = true;
                return Some(Self::make_number(1, ctx));
            }
        }

        None
    }

    fn try_simplify_self_modulo<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::Remainder {
            return None;
        }

        if let (Expression::Identifier(left), Expression::Identifier(right)) =
            (&binary.left, &binary.right)
        {
            if left.name == right.name {
                eprintln!("[AST] Simplifying {} % {} to 0", left.name, right.name);
                self.changed = true;
                return Some(Self::make_number(0, ctx));
            }
        }

        None
    }

    fn try_simplify_self_xor<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::BitwiseXOR {
            return None;
        }

        if let (Expression::Identifier(left), Expression::Identifier(right)) =
            (&binary.left, &binary.right)
        {
            if left.name == right.name {
                eprintln!("[AST] Simplifying {} ^ {} to 0", left.name, right.name);
                self.changed = true;
                return Some(Self::make_number(0, ctx));
            }
        }

        None
    }

    fn try_simplify_add_zero<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::Addition {
            return None;
        }

        if Self::is_zero(&binary.left) {
            eprintln!("[AST] Simplifying 0 + x to x");
            self.changed = true;
            return Some(Self::clone_expression(&binary.right, ctx));
        }

        if Self::is_zero(&binary.right) {
            eprintln!("[AST] Simplifying x + 0 to x");
            self.changed = true;
            return Some(Self::clone_expression(&binary.left, ctx));
        }

        None
    }

    fn try_simplify_multiply_one<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::Multiplication {
            return None;
        }

        if Self::is_one(&binary.left) {
            eprintln!("[AST] Simplifying 1 * x to x");
            self.changed = true;
            return Some(Self::clone_expression(&binary.right, ctx));
        }

        if Self::is_one(&binary.right) {
            eprintln!("[AST] Simplifying x * 1 to x");
            self.changed = true;
            return Some(Self::clone_expression(&binary.left, ctx));
        }

        None
    }

    fn try_simplify_divide_one<'a>(
        &mut self,
        binary: &BinaryExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if binary.operator != BinaryOperator::Division {
            return None;
        }

        if Self::is_one(&binary.right) {
            eprintln!("[AST] Simplifying x / 1 to x");
            self.changed = true;
            return Some(Self::clone_expression(&binary.left, ctx));
        }

        None
    }

    fn try_simplify_logical_and<'a>(
        &mut self,
        logical: &LogicalExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if logical.operator != LogicalOperator::And {
            return None;
        }

        if Self::is_true(&logical.left) {
            eprintln!("[AST] Simplifying true && x to x");
            self.changed = true;
            return Some(Self::clone_expression(&logical.right, ctx));
        }

        if Self::is_false(&logical.left) {
            eprintln!("[AST] Simplifying false && x to false");
            self.changed = true;
            return Some(Self::make_boolean(false, ctx));
        }

        if Self::is_true(&logical.right) {
            eprintln!("[AST] Simplifying x && true to x");
            self.changed = true;
            return Some(Self::clone_expression(&logical.left, ctx));
        }

        if Self::is_false(&logical.right) {
            eprintln!("[AST] Simplifying x && false to false");
            self.changed = true;
            return Some(Self::make_boolean(false, ctx));
        }

        None
    }

    fn try_simplify_logical_or<'a>(
        &mut self,
        logical: &LogicalExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        if logical.operator != LogicalOperator::Or {
            return None;
        }

        if Self::is_true(&logical.left) {
            eprintln!("[AST] Simplifying true || x to true");
            self.changed = true;
            return Some(Self::make_boolean(true, ctx));
        }

        if Self::is_false(&logical.left) {
            eprintln!("[AST] Simplifying false || x to x");
            self.changed = true;
            return Some(Self::clone_expression(&logical.right, ctx));
        }

        if Self::is_true(&logical.right) {
            eprintln!("[AST] Simplifying x || true to true");
            self.changed = true;
            return Some(Self::make_boolean(true, ctx));
        }

        if Self::is_false(&logical.right) {
            eprintln!("[AST] Simplifying x || false to x");
            self.changed = true;
            return Some(Self::clone_expression(&logical.left, ctx));
        }

        None
    }

    fn is_zero(expr: &Expression<'_>) -> bool {
        if let Expression::NumericLiteral(num) = expr {
            num.value == 0.0
        } else {
            false
        }
    }

    fn is_one(expr: &Expression<'_>) -> bool {
        if let Expression::NumericLiteral(num) = expr {
            num.value == 1.0
        } else {
            false
        }
    }

    fn is_true(expr: &Expression<'_>) -> bool {
        matches!(expr, Expression::BooleanLiteral(b) if b.value)
    }

    fn is_false(expr: &Expression<'_>) -> bool {
        matches!(expr, Expression::BooleanLiteral(b) if !b.value)
    }

    fn clone_expression<'a>(expr: &Expression<'a>, ctx: &mut Ctx<'a>) -> Expression<'a> {
        match expr {
            Expression::Identifier(id) => {
                Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                    span: SPAN,
                    name: ctx.ast.atom(id.name.as_str()),
                    reference_id: Default::default(),
                }))
            }
            Expression::NumericLiteral(num) => {
                Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                    span: SPAN,
                    value: num.value,
                    raw: num.raw.map(|r| ctx.ast.atom(r.as_str())),
                    base: num.base,
                }))
            }
            Expression::StringLiteral(s) => {
                Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
                    span: SPAN,
                    value: ctx.ast.atom(s.value.as_str()),
                    raw: None,
                    lone_surrogates: false,
                }))
            }
            Expression::BooleanLiteral(b) => {
                Expression::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
                    span: SPAN,
                    value: b.value,
                }))
            }
            _ => Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                span: SPAN,
                name: ctx.ast.atom("_expr"),
                reference_id: Default::default(),
            })),
        }
    }

    fn make_number<'a>(val: i64, ctx: &mut Ctx<'a>) -> Expression<'a> {
        Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
            span: SPAN,
            value: val as f64,
            raw: Some(ctx.ast.atom(&val.to_string())),
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

impl Default for AlgebraicSimplifier {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for AlgebraicSimplifier {
    fn exit_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        let replacement = match expr {
            Expression::BinaryExpression(binary) => self
                .try_simplify_self_subtract(binary, ctx)
                .or_else(|| self.try_simplify_multiply_zero(binary, ctx))
                .or_else(|| self.try_simplify_self_divide(binary, ctx))
                .or_else(|| self.try_simplify_self_modulo(binary, ctx))
                .or_else(|| self.try_simplify_self_xor(binary, ctx))
                .or_else(|| self.try_simplify_add_zero(binary, ctx))
                .or_else(|| self.try_simplify_multiply_one(binary, ctx))
                .or_else(|| self.try_simplify_divide_one(binary, ctx)),
            Expression::LogicalExpression(logical) => self
                .try_simplify_logical_and(logical, ctx)
                .or_else(|| self.try_simplify_logical_or(logical, ctx)),
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
    use oxc_traverse::{traverse_mut_with_ctx, ReusableTraverseCtx};

    fn run_simplify(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut simplifier = AlgebraicSimplifier::new();
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
    fn test_self_subtract() {
        let output = run_simplify("var r = x - x;");
        assert!(
            output.contains("= 0"),
            "Should simplify x - x to 0, got: {}",
            output
        );
    }

    #[test]
    fn test_multiply_zero_left() {
        let output = run_simplify("var r = 0 * y;");
        assert!(
            output.contains("= 0"),
            "Should simplify 0 * y to 0, got: {}",
            output
        );
    }

    #[test]
    fn test_multiply_zero_right() {
        let output = run_simplify("var r = x * 0;");
        assert!(
            output.contains("= 0"),
            "Should simplify x * 0 to 0, got: {}",
            output
        );
    }

    #[test]
    fn test_self_divide() {
        let output = run_simplify("var r = z / z;");
        assert!(
            output.contains("= 1"),
            "Should simplify z / z to 1, got: {}",
            output
        );
    }

    #[test]
    fn test_self_modulo() {
        let output = run_simplify("var r = a % a;");
        assert!(
            output.contains("= 0"),
            "Should simplify a % a to 0, got: {}",
            output
        );
    }

    #[test]
    fn test_self_xor() {
        let output = run_simplify("var r = b ^ b;");
        assert!(
            output.contains("= 0"),
            "Should simplify b ^ b to 0, got: {}",
            output
        );
    }

    #[test]
    fn test_add_zero_left() {
        let output = run_simplify("var r = 0 + x;");
        assert!(
            output.contains("= x"),
            "Should simplify 0 + x to x, got: {}",
            output
        );
    }

    #[test]
    fn test_add_zero_right() {
        let output = run_simplify("var r = x + 0;");
        assert!(
            output.contains("= x"),
            "Should simplify x + 0 to x, got: {}",
            output
        );
    }

    #[test]
    fn test_multiply_one_left() {
        let output = run_simplify("var r = 1 * x;");
        assert!(
            output.contains("= x"),
            "Should simplify 1 * x to x, got: {}",
            output
        );
    }

    #[test]
    fn test_multiply_one_right() {
        let output = run_simplify("var r = x * 1;");
        assert!(
            output.contains("= x"),
            "Should simplify x * 1 to x, got: {}",
            output
        );
    }

    #[test]
    fn test_divide_one() {
        let output = run_simplify("var r = x / 1;");
        assert!(
            output.contains("= x"),
            "Should simplify x / 1 to x, got: {}",
            output
        );
    }

    #[test]
    fn test_no_simplification() {
        let output = run_simplify("var r = x - y;");
        assert!(
            output.contains("x - y"),
            "Should not simplify x - y, got: {}",
            output
        );
    }
}
