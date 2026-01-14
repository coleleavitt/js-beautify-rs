//! Loop Unrolling Optimization
//!
//! Transforms constant-bounded for loops into unrolled statements.
//! Safety constraints: numeric literal bounds, increment by 1, max 10 iterations.

use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::*;
use oxc_semantic::SemanticBuilder;
use oxc_span::SPAN;
use oxc_syntax::scope::ScopeFlags;
use oxc_traverse::{ReusableTraverseCtx, Traverse, TraverseCtx, traverse_mut_with_ctx};

use crate::oxc_opts::state::OptimizationState;

pub type Ctx<'a> = TraverseCtx<'a, OptimizationState>;

pub struct LoopUnroller {
    changed: bool,
}

impl LoopUnroller {
    pub fn new() -> Self {
        Self { changed: false }
    }

    pub fn run<'a>(&mut self, program: &mut Program<'a>, allocator: &'a Allocator) -> bool {
        self.changed = false;

        let state = OptimizationState::new();
        let scoping = SemanticBuilder::new()
            .build(program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, allocator);

        traverse_mut_with_ctx(self, program, &mut ctx);

        self.changed
    }

    fn try_unroll_for_loop<'a>(
        &mut self,
        stmt: &ForStatement<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<oxc_allocator::Vec<'a, Statement<'a>>> {
        let init = stmt.init.as_ref()?;
        let var_decl = match init {
            ForStatementInit::VariableDeclaration(v) => v,
            _ => return None,
        };

        if var_decl.declarations.len() != 1 {
            return None;
        }

        let decl = &var_decl.declarations[0];

        let loop_var_name = match &decl.id {
            BindingPattern::BindingIdentifier(ident) => ident.name.as_str().to_string(),
            _ => return  None,
        };

        let init_value = match &decl.init {
            Some(Expression::NumericLiteral(lit)) => lit.value as i64,
            _ => return None,
        };

        let test = stmt.test.as_ref()?;
        let bin = match test {
            Expression::BinaryExpression(b) => b,
            _ => return None,
        };

        let limit = match &bin.right {
            Expression::NumericLiteral(lit) => lit.value as i64,
            _ => return None,
        };

        let increment = match &stmt.update {
            Some(Expression::UpdateExpression(update)) => {
                if update.operator == UpdateOperator::Increment {
                    1
                } else if update.operator == UpdateOperator::Decrement {
                    -1
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        let iterations = match bin.operator {
            BinaryOperator::LessThan => limit.checked_sub(init_value)?,
            BinaryOperator::LessEqualThan => limit.checked_sub(init_value)?.checked_add(1)?,
            _ => return None,
        };

        if iterations <= 0 || iterations > 10 {
            return None;
        }

        let mut unrolled_stmts = ctx.ast.vec();

        for i in 0..iterations {
            let current_value = init_value.checked_add(i.checked_mul(increment)?)?;

            let body_clone = self.clone_and_substitute_loop_body(
                &stmt.body,
                &loop_var_name,
                current_value,
                ctx,
            )?;

            unrolled_stmts.push(body_clone);
        }

        self.changed = true;
        Some(unrolled_stmts)
    }

    fn clone_and_substitute_loop_body<'a>(
        &self,
        body: &Statement<'a>,
        loop_var: &str,
        value: i64,
        ctx: &mut Ctx<'a>,
    ) -> Option<Statement<'a>> {
        match body {
            Statement::BlockStatement(block) => {
                let mut new_stmts = ctx.ast.vec();

                for stmt in &block.body {
                    if let Some(new_stmt) = self.substitute_in_statement(stmt, loop_var, value, ctx)
                    {
                        new_stmts.push(new_stmt);
                    } else {
                        return None;
                    }
                }

                let scope_id = ctx.create_child_scope_of_current(ScopeFlags::empty());
                let block = ctx
                    .ast
                    .block_statement_with_scope_id(SPAN, new_stmts, scope_id);
                Some(Statement::BlockStatement(ctx.ast.alloc(block)))
            }
            Statement::ExpressionStatement(_) => {
                self.substitute_in_statement(body, loop_var, value, ctx)
            }
            _ => None,
        }
    }

    fn substitute_in_statement<'a>(
        &self,
        stmt: &Statement<'a>,
        loop_var: &str,
        value: i64,
        ctx: &mut Ctx<'a>,
    ) -> Option<Statement<'a>> {
        match stmt {
            Statement::ExpressionStatement(expr_stmt) => {
                let new_expr =
                    self.substitute_in_expression(&expr_stmt.expression, loop_var, value, ctx)?;
                Some(Statement::ExpressionStatement(ctx.ast.alloc(
                    ExpressionStatement {
                        span: SPAN,
                        expression: new_expr,
                    },
                )))
            }
            Statement::VariableDeclaration(var_decl) => {
                let mut new_decls = ctx.ast.vec();
                for decl in &var_decl.declarations {
                    let new_init = if let Some(init) = &decl.init {
                        Some(self.substitute_in_expression(init, loop_var, value, ctx)?)
                    } else {
                        None
                    };

                    let new_id = decl.id.clone_in(ctx.ast.allocator);

                    new_decls.push(VariableDeclarator {
                        span: SPAN,
                        kind: var_decl.kind,
                        id: new_id,
                        type_annotation: None,
                        init: new_init,
                        definite: decl.definite,
                    });
                }

                Some(Statement::VariableDeclaration(ctx.ast.alloc(
                    VariableDeclaration {
                        span: SPAN,
                        kind: var_decl.kind,
                        declarations: new_decls,
                        declare: var_decl.declare,
                    },
                )))
            }
            _ => None,
        }
    }

    fn substitute_in_expression<'a>(
        &self,
        expr: &Expression<'a>,
        loop_var: &str,
        value: i64,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        match expr {
            Expression::Identifier(ident) => {
                if ident.name.as_str() == loop_var {
                    Some(Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                        span: SPAN,
                        value: value as f64,
                        raw: None,
                        base: NumberBase::Decimal,
                    })))
                } else {
                    Some(Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                        span: SPAN,
                        name: ctx.ast.atom(ident.name.as_str()),
                        reference_id: None.into(),
                    })))
                }
            }
            Expression::CallExpression(call) => {
                let new_callee =
                    self.substitute_in_expression(&call.callee, loop_var, value, ctx)?;
                let mut new_args = ctx.ast.vec();

                for arg in &call.arguments {
                    if let Argument::SpreadElement(_) = arg {
                        return None;
                    }
                    let new_arg = self.substitute_in_argument(arg, loop_var, value, ctx)?;
                    new_args.push(new_arg);
                }

                Some(Expression::CallExpression(ctx.ast.alloc(CallExpression {
                    span: SPAN,
                    callee: new_callee,
                    type_arguments: None,
                    arguments: new_args,
                    optional: call.optional,
                    pure: false,
                })))
            }
            Expression::BinaryExpression(bin) => {
                let left = self.substitute_in_expression(&bin.left, loop_var, value, ctx)?;
                let right = self.substitute_in_expression(&bin.right, loop_var, value, ctx)?;

                Some(Expression::BinaryExpression(ctx.ast.alloc(
                    BinaryExpression {
                        span: SPAN,
                        left,
                        operator: bin.operator,
                        right,
                    },
                )))
            }
            Expression::StaticMemberExpression(static_mem) => {
                let object =
                    self.substitute_in_expression(&static_mem.object, loop_var, value, ctx)?;

                Some(Expression::StaticMemberExpression(ctx.ast.alloc(
                    StaticMemberExpression {
                        span: SPAN,
                        object,
                        property: IdentifierName {
                            span: SPAN,
                            name: ctx.ast.atom(static_mem.property.name.as_str()),
                        },
                        optional: static_mem.optional,
                    },
                )))
            }
            Expression::ComputedMemberExpression(computed) => {
                let object =
                    self.substitute_in_expression(&computed.object, loop_var, value, ctx)?;
                let property =
                    self.substitute_in_expression(&computed.expression, loop_var, value, ctx)?;

                Some(Expression::ComputedMemberExpression(ctx.ast.alloc(
                    ComputedMemberExpression {
                        span: SPAN,
                        object,
                        expression: property,
                        optional: computed.optional,
                    },
                )))
            }
            Expression::NumericLiteral(lit) => {
                Some(Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                    span: SPAN,
                    value: lit.value,
                    raw: lit.raw,
                    base: lit.base,
                })))
            }
            Expression::StringLiteral(lit) => {
                Some(Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
                    span: SPAN,
                    value: ctx.ast.atom(lit.value.as_str()),
                    raw: None,
                    lone_surrogates: false,
                })))
            }
            _ => None,
        }
    }

    fn substitute_in_argument<'a>(
        &self,
        arg: &Argument<'a>,
        loop_var: &str,
        value: i64,
        ctx: &mut Ctx<'a>,
    ) -> Option<Argument<'a>> {
        match arg {
            Argument::SpreadElement(_) => None,
            Argument::Identifier(ident) => {
                if ident.name.as_str() == loop_var {
                    Some(Argument::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                        span: SPAN,
                        value: value as f64,
                        raw: None,
                        base: NumberBase::Decimal,
                    })))
                } else {
                    Some(Argument::Identifier(ctx.ast.alloc(IdentifierReference {
                        span: SPAN,
                        name: ctx.ast.atom(ident.name.as_str()),
                        reference_id: None.into(),
                    })))
                }
            }
            Argument::NumericLiteral(lit) => {
                Some(Argument::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                    span: SPAN,
                    value: lit.value,
                    raw: lit.raw,
                    base: lit.base,
                })))
            }
            Argument::StringLiteral(lit) => {
                Some(Argument::StringLiteral(ctx.ast.alloc(StringLiteral {
                    span: SPAN,
                    value: ctx.ast.atom(lit.value.as_str()),
                    raw: None,
                    lone_surrogates: false,
                })))
            }
            Argument::BooleanLiteral(lit) => {
                Some(Argument::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
                    span: SPAN,
                    value: lit.value,
                })))
            }
            Argument::NullLiteral(_) => Some(Argument::NullLiteral(
                ctx.ast.alloc(NullLiteral { span: SPAN }),
            )),
            Argument::BinaryExpression(bin) => {
                let left = self.substitute_in_expression(&bin.left, loop_var, value, ctx)?;
                let right = self.substitute_in_expression(&bin.right, loop_var, value, ctx)?;
                Some(Argument::BinaryExpression(ctx.ast.alloc(
                    BinaryExpression {
                        span: SPAN,
                        left,
                        operator: bin.operator,
                        right,
                    },
                )))
            }
            Argument::CallExpression(call) => {
                let new_callee =
                    self.substitute_in_expression(&call.callee, loop_var, value, ctx)?;
                let mut new_args = ctx.ast.vec();
                for a in &call.arguments {
                    if let Argument::SpreadElement(_) = a {
                        return None;
                    }
                    let new_a = self.substitute_in_argument(a, loop_var, value, ctx)?;
                    new_args.push(new_a);
                }
                Some(Argument::CallExpression(ctx.ast.alloc(CallExpression {
                    span: SPAN,
                    callee: new_callee,
                    type_arguments: None,
                    arguments: new_args,
                    optional: call.optional,
                    pure: false,
                })))
            }
            _ => None,
        }
    }
}

impl Default for LoopUnroller {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, OptimizationState> for LoopUnroller {
    fn enter_statement(&mut self, stmt: &mut Statement<'a>, ctx: &mut Ctx<'a>) {
        if let Statement::ForStatement(for_stmt) = stmt {
            if let Some(unrolled) = self.try_unroll_for_loop(for_stmt, ctx) {
                let scope_id = ctx.create_child_scope_of_current(ScopeFlags::empty());
                let block = ctx
                    .ast
                    .block_statement_with_scope_id(SPAN, unrolled, scope_id);
                *stmt = Statement::BlockStatement(ctx.ast.alloc(block));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn test_unroll(input: &str, expected_contains: &[&str]) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, input, source_type).parse();
        let mut program = ret.program;

        let mut unroller = LoopUnroller::new();
        let _changed = unroller.run(&mut program, &allocator);

        let output = Codegen::new().build(&program).code;

        for expected in expected_contains {
            assert!(
                output.contains(expected),
                "Expected output to contain '{}', got:\n{}",
                expected,
                output
            );
        }
    }

    #[test]
    fn test_unroll_simple_loop() {
        test_unroll(
            "for (let i = 0; i < 3; i++) { console.log(i); }",
            &["console.log(0)", "console.log(1)", "console.log(2)"],
        );
    }

    #[test]
    fn test_no_unroll_large_loop() {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let input = "for (let i = 0; i < 100; i++) { console.log(i); }";
        let ret = Parser::new(&allocator, input, source_type).parse();
        let mut program = ret.program;

        let mut unroller = LoopUnroller::new();
        let changed = unroller.run(&mut program, &allocator);

        assert!(!changed, "Should not unroll loop with 100 iterations");
    }
}
