//! Common Subexpression Elimination (CSE)
//!
//! Identifies duplicate expressions and tracks them for potential optimization.
//! Currently detects common subexpressions; full rewriting deferred to future work.

use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::*;
use oxc_semantic::SemanticBuilder;

use oxc_traverse::{ReusableTraverseCtx, Traverse, TraverseCtx, traverse_mut_with_ctx};
use rustc_hash::FxHashMap;

use crate::oxc_opts::state::OptimizationState;

pub type Ctx<'a> = TraverseCtx<'a, OptimizationState>;

pub struct CommonSubexpressionElimination {
    changed: bool,
    expr_to_var: FxHashMap<String, String>,
    var_counter: usize,
}

impl CommonSubexpressionElimination {
    pub fn new() -> Self {
        Self {
            changed: false,
            expr_to_var: FxHashMap::default(),
            var_counter: 0,
        }
    }

    pub fn run<'a>(&mut self, program: &mut Program<'a>, allocator: &'a Allocator) -> bool {
        self.changed = false;
        self.expr_to_var.clear();
        self.var_counter = 0;

        let state = OptimizationState::new();
        let scoping = SemanticBuilder::new()
            .build(program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, allocator);

        traverse_mut_with_ctx(self, program, &mut ctx);

        self.changed
    }

    #[allow(dead_code)]
    fn generate_temp_var(&mut self) -> String {
        let var_name = format!("__cse_temp_{}", self.var_counter);
        self.var_counter = self.var_counter.saturating_add(1);
        var_name
    }

    fn expression_to_string(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::BinaryExpression(bin) => {
                let left = self.expression_to_string(&bin.left)?;
                let op = format!("{:?}", bin.operator);
                let right = self.expression_to_string(&bin.right)?;
                Some(format!("({} {} {})", left, op, right))
            }
            Expression::CallExpression(call) => {
                let callee = self.expression_to_string(&call.callee)?;
                let args: Vec<String> = call
                    .arguments
                    .iter()
                    .filter_map(|arg| self.argument_to_string(arg))
                    .collect();
                Some(format!("{}({})", callee, args.join(", ")))
            }
            Expression::StaticMemberExpression(static_mem) => {
                let obj = self.expression_to_string(&static_mem.object)?;
                Some(format!("{}.{}", obj, static_mem.property.name))
            }
            Expression::ComputedMemberExpression(computed) => {
                let obj = self.expression_to_string(&computed.object)?;
                let prop = self.expression_to_string(&computed.expression)?;
                Some(format!("{}[{}]", obj, prop))
            }
            Expression::Identifier(ident) => Some(ident.name.to_string()),
            Expression::NumericLiteral(lit) => Some(lit.value.to_string()),
            Expression::StringLiteral(lit) => Some(format!("\"{}\"", lit.value)),
            Expression::BooleanLiteral(lit) => Some(lit.value.to_string()),
            Expression::UnaryExpression(unary) => {
                let operand = self.expression_to_string(&unary.argument)?;
                let op = format!("{:?}", unary.operator);
                Some(format!("({} {})", op, operand))
            }
            _ => None,
        }
    }

    fn argument_to_string(&self, arg: &Argument) -> Option<String> {
        match arg {
            Argument::SpreadElement(_) => None,
            Argument::Identifier(ident) => Some(ident.name.to_string()),
            Argument::NumericLiteral(lit) => Some(lit.value.to_string()),
            Argument::StringLiteral(lit) => Some(format!("\"{}\"", lit.value)),
            Argument::BooleanLiteral(lit) => Some(lit.value.to_string()),
            Argument::BinaryExpression(bin) => {
                let left = self.expression_to_string(&bin.left)?;
                let op = format!("{:?}", bin.operator);
                let right = self.expression_to_string(&bin.right)?;
                Some(format!("({} {} {})", left, op, right))
            }
            _ => None,
        }
    }

    fn is_complex_expression(&self, expr: &Expression) -> bool {
        matches!(
            expr,
            Expression::BinaryExpression(_)
                | Expression::CallExpression(_)
                | Expression::UnaryExpression(_)
                | Expression::ComputedMemberExpression(_)
        )
    }

    fn process_block_for_cse<'a>(
        &mut self,
        statements: &mut oxc_allocator::Vec<'a, Statement<'a>>,
        ctx: &mut Ctx<'a>,
    ) {
        let mut new_statements = ctx.ast.vec();
        let mut expr_map: FxHashMap<String, String> = FxHashMap::default();

        for stmt in statements.iter() {
            match stmt {
                Statement::ExpressionStatement(expr_stmt) => {
                    if let Some(expr_str) = self.expression_to_string(&expr_stmt.expression) {
                        if self.is_complex_expression(&expr_stmt.expression)
                            && expr_map.contains_key(&expr_str)
                        {
                            self.changed = true;
                            continue;
                        } else if self.is_complex_expression(&expr_stmt.expression) {
                            expr_map.insert(expr_str, String::new());
                        }
                    }
                    new_statements.push(self.clone_statement(stmt, ctx));
                }
                Statement::VariableDeclaration(var_decl) => {
                    for declarator in &var_decl.declarations {
                        if let Some(init) = &declarator.init {
                            if let Some(expr_str) = self.expression_to_string(init) {
                                if self.is_complex_expression(init) {
                                    if expr_map.contains_key(&expr_str) {
                                        self.changed = true;
                                    } else if let BindingPattern::BindingIdentifier(ident) =
                                        &declarator.id
                                    {
                                        expr_map.insert(expr_str, ident.name.to_string());
                                    }
                                }
                            }
                        }
                    }
                    new_statements.push(self.clone_statement(stmt, ctx));
                }
                _ => {
                    new_statements.push(self.clone_statement(stmt, ctx));
                }
            }
        }

        *statements = new_statements;
    }

    fn clone_statement<'a>(&self, stmt: &Statement<'a>, ctx: &mut Ctx<'a>) -> Statement<'a> {
        match stmt {
            Statement::ExpressionStatement(expr_stmt) => {
                Statement::ExpressionStatement(ctx.ast.alloc(ExpressionStatement {
                    span: expr_stmt.span,
                    expression: self.clone_expression(&expr_stmt.expression, ctx),
                }))
            }
            Statement::VariableDeclaration(var_decl) => {
                let mut new_decls = ctx.ast.vec();
                for decl in &var_decl.declarations {
                    let new_id = decl.id.clone_in(ctx.ast.allocator);
                    new_decls.push(VariableDeclarator {
                        span: decl.span,
                        kind: var_decl.kind,
                        id: new_id,
                        type_annotation: None,
                        init: decl.init.as_ref().map(|e| self.clone_expression(e, ctx)),
                        definite: decl.definite,
                    });
                }
                Statement::VariableDeclaration(ctx.ast.alloc(VariableDeclaration {
                    span: var_decl.span,
                    kind: var_decl.kind,
                    declarations: new_decls,
                    declare: var_decl.declare,
                }))
            }
            _ => stmt.clone_in(ctx.ast.allocator),
        }
    }

    fn clone_expression<'a>(&self, expr: &Expression<'a>, ctx: &mut Ctx<'a>) -> Expression<'a> {
        match expr {
            Expression::Identifier(ident) => {
                Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                    span: ident.span,
                    name: ctx.ast.atom(ident.name.as_str()),
                    reference_id: None.into(),
                }))
            }
            Expression::NumericLiteral(lit) => {
                Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                    span: lit.span,
                    value: lit.value,
                    raw: lit.raw,
                    base: lit.base,
                }))
            }
            Expression::StringLiteral(lit) => {
                Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
                    span: lit.span,
                    value: ctx.ast.atom(lit.value.as_str()),
                    raw: None,
                    lone_surrogates: false,
                }))
            }
            Expression::BinaryExpression(bin) => {
                Expression::BinaryExpression(ctx.ast.alloc(BinaryExpression {
                    span: bin.span,
                    left: self.clone_expression(&bin.left, ctx),
                    operator: bin.operator,
                    right: self.clone_expression(&bin.right, ctx),
                }))
            }
            _ => expr.clone_in(ctx.ast.allocator),
        }
    }
}

impl Default for CommonSubexpressionElimination {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, OptimizationState> for CommonSubexpressionElimination {
    fn enter_statement(&mut self, stmt: &mut Statement<'a>, ctx: &mut Ctx<'a>) {
        if let Statement::BlockStatement(block) = stmt {
            self.process_block_for_cse(&mut block.body, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn test_cse(input: &str, should_change: bool) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, input, source_type).parse();
        let mut program = ret.program;

        let mut cse = CommonSubexpressionElimination::new();
        let changed = cse.run(&mut program, &allocator);

        assert_eq!(changed, should_change);

        if changed {
            let output = Codegen::new().build(&program).code;
            println!("CSE output: {}", output);
        }
    }

    #[test]
    fn test_cse_simple() {
        test_cse("{ const a = b + c; const d = b + c; }", true);
    }

    #[test]
    fn test_cse_no_common() {
        test_cse("{ const a = b + c; const d = e + f; }", false);
    }

    #[test]
    fn test_cse_complex_expression() {
        test_cse(
            "{ const a = expensive(x, y); const b = expensive(x, y); }",
            true,
        );
    }
}
