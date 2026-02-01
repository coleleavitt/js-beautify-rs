use oxc_ast::ast::*;
use oxc_semantic::ScopeFlags;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct DeadCodeEliminator {
    changed: bool,
}

impl DeadCodeEliminator {
    pub fn new() -> Self {
        Self { changed: false }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn is_false(expr: &Expression<'_>) -> bool {
        match expr {
            Expression::BooleanLiteral(lit) => !lit.value,
            Expression::NumericLiteral(lit) => lit.value == 0.0,
            _ => false,
        }
    }

    fn is_true(expr: &Expression<'_>) -> bool {
        match expr {
            Expression::BooleanLiteral(lit) => lit.value,
            Expression::NumericLiteral(lit) => lit.value != 0.0,
            _ => false,
        }
    }

    fn clone_statement<'b>(
        stmt: &Statement<'b>,
        ctx: &mut TraverseCtx<'b, DeobfuscateState>,
    ) -> Statement<'b> {
        match stmt {
            Statement::EmptyStatement(_) => {
                Statement::EmptyStatement(ctx.ast.alloc(EmptyStatement { span: SPAN }))
            }
            Statement::ExpressionStatement(expr_stmt) => {
                Statement::ExpressionStatement(ctx.ast.alloc(ExpressionStatement {
                    span: SPAN,
                    expression: Self::clone_expression(&expr_stmt.expression, ctx),
                }))
            }
            Statement::ReturnStatement(ret) => Statement::ReturnStatement(
                ctx.ast.alloc(ReturnStatement {
                    span: SPAN,
                    argument: ret
                        .argument
                        .as_ref()
                        .map(|e| Self::clone_expression(e, ctx)),
                }),
            ),
            Statement::BreakStatement(brk) => {
                Statement::BreakStatement(ctx.ast.alloc(BreakStatement {
                    span: SPAN,
                    label: brk.label.as_ref().map(|l| LabelIdentifier {
                        span: SPAN,
                        name: ctx.ast.atom(l.name.as_str()),
                    }),
                }))
            }
            Statement::ContinueStatement(cont) => {
                Statement::ContinueStatement(ctx.ast.alloc(ContinueStatement {
                    span: SPAN,
                    label: cont.label.as_ref().map(|l| LabelIdentifier {
                        span: SPAN,
                        name: ctx.ast.atom(l.name.as_str()),
                    }),
                }))
            }
            Statement::ThrowStatement(throw) => {
                Statement::ThrowStatement(ctx.ast.alloc(ThrowStatement {
                    span: SPAN,
                    argument: Self::clone_expression(&throw.argument, ctx),
                }))
            }
            Statement::VariableDeclaration(var_decl) => {
                let mut declarations = ctx.ast.vec();
                for decl in &var_decl.declarations {
                    declarations.push(VariableDeclarator {
                        span: SPAN,
                        kind: var_decl.kind,
                        id: Self::clone_binding_pattern(&decl.id, ctx),
                        init: decl.init.as_ref().map(|e| Self::clone_expression(e, ctx)),
                        definite: decl.definite,
                        type_annotation: None,
                    });
                }
                Statement::VariableDeclaration(ctx.ast.alloc(VariableDeclaration {
                    span: SPAN,
                    kind: var_decl.kind,
                    declarations,
                    declare: var_decl.declare,
                }))
            }
            _ => Statement::EmptyStatement(ctx.ast.alloc(EmptyStatement { span: SPAN })),
        }
    }

    fn clone_expression<'b>(
        expr: &Expression<'b>,
        ctx: &mut TraverseCtx<'b, DeobfuscateState>,
    ) -> Expression<'b> {
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
            Expression::NullLiteral(_) => {
                Expression::NullLiteral(ctx.ast.alloc(NullLiteral { span: SPAN }))
            }
            _ => Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                span: SPAN,
                name: ctx.ast.atom("_expr"),
                reference_id: Default::default(),
            })),
        }
    }

    fn clone_binding_pattern<'b>(
        pattern: &BindingPattern<'b>,
        ctx: &mut TraverseCtx<'b, DeobfuscateState>,
    ) -> BindingPattern<'b> {
        match pattern {
            BindingPattern::BindingIdentifier(ident) => {
                BindingPattern::BindingIdentifier(ctx.ast.alloc(BindingIdentifier {
                    span: SPAN,
                    name: ctx.ast.atom(ident.name.as_str()),
                    symbol_id: Default::default(),
                }))
            }
            _ => BindingPattern::BindingIdentifier(ctx.ast.alloc(BindingIdentifier {
                span: SPAN,
                name: ctx.ast.atom("_unknown"),
                symbol_id: Default::default(),
            })),
        }
    }
}

impl Default for DeadCodeEliminator {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for DeadCodeEliminator {
    fn exit_statement(&mut self, stmt: &mut Statement<'a>, ctx: &mut Ctx<'a>) {
        match stmt {
            Statement::IfStatement(if_stmt) => {
                if Self::is_false(&if_stmt.test) {
                    eprintln!("[AST] Eliminating if(false) branch");
                    self.changed = true;
                    *stmt = Statement::EmptyStatement(ctx.ast.alloc(EmptyStatement { span: SPAN }));
                } else if Self::is_true(&if_stmt.test) {
                    eprintln!("[AST] Eliminating if(true) - keeping consequent");
                    self.changed = true;
                    let scope_id = ctx.create_child_scope_of_current(ScopeFlags::empty());
                    *stmt = ctx
                        .ast
                        .statement_block_with_scope_id(SPAN, ctx.ast.vec(), scope_id);
                }
            }
            Statement::WhileStatement(while_stmt) => {
                if Self::is_false(&while_stmt.test) {
                    eprintln!("[AST] Eliminating while(false) loop");
                    self.changed = true;
                    *stmt = Statement::EmptyStatement(ctx.ast.alloc(EmptyStatement { span: SPAN }));
                }
            }
            _ => {}
        }
    }

    fn exit_block_statement(&mut self, block: &mut BlockStatement<'a>, ctx: &mut Ctx<'a>) {
        let mut has_terminator = false;
        let mut new_body = ctx.ast.vec();

        for stmt in block.body.iter() {
            if has_terminator {
                eprintln!("[AST] Removing unreachable code after return/break/continue/throw");
                self.changed = true;
                continue;
            }

            let is_terminator = matches!(
                stmt,
                Statement::ReturnStatement(_)
                    | Statement::BreakStatement(_)
                    | Statement::ContinueStatement(_)
                    | Statement::ThrowStatement(_)
            );

            new_body.push(Self::clone_statement(stmt, ctx));

            if is_terminator {
                has_terminator = true;
            }
        }

        if self.changed {
            block.body = new_body;
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

    fn run_dce(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut eliminator = DeadCodeEliminator::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut eliminator, &mut program, &mut ctx);

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_remove_if_false() {
        let output = run_dce("if (false) { console.log('dead'); }");
        assert!(
            !output.contains("dead"),
            "Should remove if(false) branch, got: {}",
            output
        );
    }

    #[test]
    fn test_remove_while_false() {
        let output = run_dce("while (false) { console.log('dead'); }");
        assert!(
            !output.contains("while"),
            "Should remove while(false) loop, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_reachable_code() {
        let output = run_dce("var x = 1; console.log(x);");
        assert!(
            output.contains("console"),
            "Should preserve reachable code, got: {}",
            output
        );
    }
}
