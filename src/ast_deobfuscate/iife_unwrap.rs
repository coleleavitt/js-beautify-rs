use oxc_allocator::CloneIn;
use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Ancestor, Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct IifeUnwrap {
    unwrapped_count: usize,
}

impl IifeUnwrap {
    pub fn new() -> Self {
        Self { unwrapped_count: 0 }
    }

    pub fn unwrapped_count(&self) -> usize {
        self.unwrapped_count
    }
}

impl Default for IifeUnwrap {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_zero_arg_arrow_iife<'a, 'b>(
    expr: &'b Expression<'a>,
) -> Option<&'b ArrowFunctionExpression<'a>> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    if !call.arguments.is_empty() {
        return None;
    }
    let Expression::ParenthesizedExpression(paren) = &call.callee else {
        return None;
    };
    let Expression::ArrowFunctionExpression(arrow) = &paren.expression else {
        return None;
    };
    if !arrow.params.items.is_empty() {
        return None;
    }
    if arrow.expression {
        return None;
    }
    if arrow.r#async {
        return None;
    }
    Some(arrow)
}

fn has_iife_candidate(stmt: &Statement<'_>) -> bool {
    if let Statement::ExpressionStatement(expr_stmt) = stmt {
        if extract_zero_arg_arrow_iife(&expr_stmt.expression).is_some() {
            return true;
        }
    }
    if let Statement::VariableDeclaration(var_decl) = stmt {
        if var_decl.declarations.len() == 1 {
            if let Some(init) = &var_decl.declarations[0].init {
                if extract_zero_arg_arrow_iife(init).is_some() {
                    return true;
                }
            }
        }
    }
    false
}

fn try_unwrap_standalone<'a>(
    stmt: &Statement<'a>,
    new_body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
    ctx: &mut Ctx<'a>,
) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Some(arrow) = extract_zero_arg_arrow_iife(&expr_stmt.expression) else {
        return false;
    };
    for inner_stmt in arrow.body.statements.iter() {
        new_body.push(inner_stmt.clone_in_with_semantic_ids(ctx.ast.allocator));
    }
    true
}

fn contains_return(stmt: &Statement<'_>) -> bool {
    match stmt {
        Statement::ReturnStatement(_) => true,
        Statement::IfStatement(if_stmt) => {
            if contains_return(&if_stmt.consequent) {
                return true;
            }
            if let Some(alt) = &if_stmt.alternate {
                if contains_return(alt) {
                    return true;
                }
            }
            false
        }
        Statement::BlockStatement(block) => block.body.iter().any(|s| contains_return(s)),
        Statement::ForStatement(f) => contains_return(&f.body),
        Statement::ForInStatement(f) => contains_return(&f.body),
        Statement::ForOfStatement(f) => contains_return(&f.body),
        Statement::WhileStatement(w) => contains_return(&w.body),
        Statement::DoWhileStatement(d) => contains_return(&d.body),
        Statement::SwitchStatement(sw) => sw
            .cases
            .iter()
            .any(|c| c.consequent.iter().any(|s| contains_return(s))),
        Statement::TryStatement(t) => {
            if t.block.body.iter().any(|s| contains_return(s)) {
                return true;
            }
            if let Some(handler) = &t.handler {
                if handler.body.body.iter().any(|s| contains_return(s)) {
                    return true;
                }
            }
            if let Some(finalizer) = &t.finalizer {
                if finalizer.body.iter().any(|s| contains_return(s)) {
                    return true;
                }
            }
            false
        }
        Statement::LabeledStatement(l) => contains_return(&l.body),
        Statement::WithStatement(w) => contains_return(&w.body),
        _ => false,
    }
}

fn try_unwrap_assigned<'a>(
    stmt: &Statement<'a>,
    new_body: &mut oxc_allocator::Vec<'a, Statement<'a>>,
    ctx: &mut Ctx<'a>,
) -> bool {
    let Statement::VariableDeclaration(var_decl) = stmt else {
        return false;
    };
    if var_decl.declarations.len() != 1 {
        return false;
    }
    let Some(init) = &var_decl.declarations[0].init else {
        return false;
    };
    let Some(arrow) = extract_zero_arg_arrow_iife(init) else {
        return false;
    };
    let body_stmts = &arrow.body.statements;
    if body_stmts.is_empty() {
        return false;
    }
    let last_idx = body_stmts.len().wrapping_sub(1);
    let Statement::ReturnStatement(ret) = &body_stmts[last_idx] else {
        return false;
    };
    let Some(return_expr) = &ret.argument else {
        return false;
    };
    let has_early_return = body_stmts[..last_idx].iter().any(|s| contains_return(s));
    if has_early_return {
        return false;
    }
    for inner_stmt in body_stmts[..last_idx].iter() {
        new_body.push(inner_stmt.clone_in_with_semantic_ids(ctx.ast.allocator));
    }
    let new_init = return_expr.clone_in_with_semantic_ids(ctx.ast.allocator);
    let new_declarator = VariableDeclarator {
        span: SPAN,
        kind: var_decl.kind,
        id: var_decl.declarations[0].id.clone_in_with_semantic_ids(ctx.ast.allocator),
        type_annotation: None,
        init: Some(new_init),
        definite: false,
    };
    let mut new_declarations = ctx.ast.vec();
    new_declarations.push(new_declarator);
    let new_var = Statement::VariableDeclaration(ctx.ast.alloc(VariableDeclaration {
        span: SPAN,
        kind: var_decl.kind,
        declarations: new_declarations,
        declare: var_decl.declare,
    }));
    new_body.push(new_var);
    true
}

impl<'a> Traverse<'a, DeobfuscateState> for IifeUnwrap {
    fn exit_program(&mut self, program: &mut Program<'a>, ctx: &mut Ctx<'a>) {
        let has_candidate = program.body.iter().any(|s| has_iife_candidate(s));
        if !has_candidate {
            return;
        }

        let mut new_body = ctx.ast.vec();
        for stmt in program.body.iter() {
            if try_unwrap_standalone(stmt, &mut new_body, ctx) {
                self.unwrapped_count = self.unwrapped_count.wrapping_add(1);
                continue;
            }
            if try_unwrap_assigned(stmt, &mut new_body, ctx) {
                self.unwrapped_count = self.unwrapped_count.wrapping_add(1);
                continue;
            }
            new_body.push(stmt.clone_in_with_semantic_ids(ctx.ast.allocator));
        }
        program.body = new_body;
    }

    fn exit_block_statement(&mut self, block: &mut BlockStatement<'a>, ctx: &mut Ctx<'a>) {
        let has_candidate = block.body.iter().any(|s| has_iife_candidate(s));
        if !has_candidate {
            return;
        }

        let mut new_body = ctx.ast.vec();
        for stmt in block.body.iter() {
            if try_unwrap_standalone(stmt, &mut new_body, ctx) {
                self.unwrapped_count = self.unwrapped_count.wrapping_add(1);
                continue;
            }
            if try_unwrap_assigned(stmt, &mut new_body, ctx) {
                self.unwrapped_count = self.unwrapped_count.wrapping_add(1);
                continue;
            }
            new_body.push(stmt.clone_in_with_semantic_ids(ctx.ast.allocator));
        }
        block.body = new_body;
    }

    fn exit_function_body(&mut self, body: &mut FunctionBody<'a>, ctx: &mut Ctx<'a>) {
        if let Ancestor::ArrowFunctionExpressionBody(arrow) = ctx.parent() {
            if *arrow.expression() {
                return;
            }
        }

        let has_candidate = body.statements.iter().any(|s| has_iife_candidate(s));
        if !has_candidate {
            return;
        }

        let mut new_stmts = ctx.ast.vec();
        for stmt in body.statements.iter() {
            if try_unwrap_standalone(stmt, &mut new_stmts, ctx) {
                self.unwrapped_count = self.unwrapped_count.wrapping_add(1);
                continue;
            }
            if try_unwrap_assigned(stmt, &mut new_stmts, ctx) {
                self.unwrapped_count = self.unwrapped_count.wrapping_add(1);
                continue;
            }
            new_stmts.push(stmt.clone_in_with_semantic_ids(ctx.ast.allocator));
        }
        body.statements = new_stmts;
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

    fn run_unwrap(code: &str) -> (String, usize) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut unwrapper = IifeUnwrap::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut unwrapper, &mut program, &mut ctx);

        (
            Codegen::new().build(&program).code,
            unwrapper.unwrapped_count(),
        )
    }

    #[test]
    fn test_unwrap_standalone_iife() {
        let (output, count) = run_unwrap("(() => { doA(); doB(); })();");
        assert!(
            count >= 1,
            "Should have unwrapped at least 1 IIFE, got: {}",
            count
        );
        assert!(
            output.contains("doA()"),
            "Should contain 'doA()', got: {}",
            output
        );
        assert!(
            output.contains("doB()"),
            "Should contain 'doB()', got: {}",
            output
        );
        assert!(
            !output.contains("=>"),
            "Should NOT contain '=>', got: {}",
            output
        );
    }

    #[test]
    fn test_unwrap_assigned_iife_single_return() {
        let (output, count) = run_unwrap("let x = (() => { return 42; })();");
        assert!(
            count >= 1,
            "Should have unwrapped at least 1 IIFE, got: {}",
            count
        );
        assert!(
            output.contains("let x = 42"),
            "Should contain 'let x = 42', got: {}",
            output
        );
        assert!(
            !output.contains("=>"),
            "Should NOT contain '=>', got: {}",
            output
        );
    }

    #[test]
    fn test_unwrap_assigned_iife_with_body() {
        let (output, count) = run_unwrap("let x = (() => { let q = 1; return q + 2; })();");
        assert!(
            count >= 1,
            "Should have unwrapped at least 1 IIFE, got: {}",
            count
        );
        assert!(
            output.contains("let q = 1"),
            "Should contain 'let q = 1', got: {}",
            output
        );
        assert!(
            output.contains("let x = q + 2"),
            "Should contain 'let x = q + 2', got: {}",
            output
        );
        assert!(
            !output.contains("=>"),
            "Should NOT contain '=>', got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_iife_with_args() {
        let (output, count) = run_unwrap("let x = ((a) => { return a + 1; })(5);");
        assert_eq!(
            count, 0,
            "Should NOT unwrap IIFE with parameters, got: {}",
            count
        );
        assert!(
            output.contains("=>"),
            "Should still contain '=>', got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_iife_with_early_return() {
        let (output, count) = run_unwrap("let x = (() => { if (true) return 1; return 2; })();");
        assert_eq!(
            count, 0,
            "Should NOT unwrap IIFE with early return, got: {}",
            count
        );
        assert!(
            output.contains("=>"),
            "Should still contain '=>', got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_non_iife_arrow() {
        let (output, count) = run_unwrap("const f = () => { return 1; };");
        assert_eq!(count, 0, "Should NOT unwrap non-IIFE arrow, got: {}", count);
        assert!(
            output.contains("=>"),
            "Should still contain '=>', got: {}",
            output
        );
    }
}
