//! Multi-variable declaration splitting pass
//!
//! Splits multi-variable `VariableDeclaration` nodes into individual
//! `VariableDeclaration` statements. For example:
//!
//! ```js
//! var a = 1, b = 2, c = 3;  // single VariableDeclaration with 3 declarators
//! ```
//!
//! Becomes:
//!
//! ```js
//! var a = 1;
//! var b = 2;
//! var c = 3;  // three separate VariableDeclarations
//! ```
//!
//! Only splits VariableDeclarations that are direct children of program body,
//! block body, or function body. For-loop variable declarations (e.g.
//! `for (var i = 0, j = 10; ...)`) are left untouched.

use oxc_allocator::CloneIn;
use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct MultiVarSplitter {
    split_count: usize,
}

impl MultiVarSplitter {
    pub fn new() -> Self {
        Self { split_count: 0 }
    }

    pub fn split_count(&self) -> usize {
        self.split_count
    }
}

impl Default for MultiVarSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for MultiVarSplitter {
    fn exit_program(&mut self, program: &mut Program<'a>, ctx: &mut Ctx<'a>) {
        let has_multi = program.body.iter().any(|s| {
            matches!(
                s,
                Statement::VariableDeclaration(var_decl)
                    if var_decl.declarations.len() > 1
            )
        });
        if !has_multi {
            return;
        }

        let before = program.body.len();
        let mut new_body = ctx.ast.vec();
        for stmt in program.body.iter() {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                if var_decl.declarations.len() > 1 {
                    eprintln!(
                        "[MULTI_VAR_SPLIT] Splitting {:?} declaration with {} declarators in program body",
                        var_decl.kind,
                        var_decl.declarations.len()
                    );
                    for declarator in var_decl.declarations.iter() {
                        let mut single_declarations = ctx.ast.vec();
                        single_declarations.push(declarator.clone_in(ctx.ast.allocator));
                        new_body.push(Statement::VariableDeclaration(ctx.ast.alloc(
                            VariableDeclaration {
                                span: SPAN,
                                kind: var_decl.kind,
                                declarations: single_declarations,
                                declare: var_decl.declare,
                            },
                        )));
                    }
                    self.split_count += 1;
                    continue;
                }
            }
            new_body.push(stmt.clone_in(ctx.ast.allocator));
        }
        let after = new_body.len();
        eprintln!(
            "[MULTI_VAR_SPLIT] Program body: {} -> {} statements",
            before, after
        );
        program.body = new_body;
    }

    fn exit_block_statement(&mut self, block: &mut BlockStatement<'a>, ctx: &mut Ctx<'a>) {
        let has_multi = block.body.iter().any(|s| {
            matches!(
                s,
                Statement::VariableDeclaration(var_decl)
                    if var_decl.declarations.len() > 1
            )
        });
        if !has_multi {
            return;
        }

        let before = block.body.len();
        let mut new_body = ctx.ast.vec();
        for stmt in block.body.iter() {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                if var_decl.declarations.len() > 1 {
                    eprintln!(
                        "[MULTI_VAR_SPLIT] Splitting {:?} declaration with {} declarators in block",
                        var_decl.kind,
                        var_decl.declarations.len()
                    );
                    for declarator in var_decl.declarations.iter() {
                        let mut single_declarations = ctx.ast.vec();
                        single_declarations.push(declarator.clone_in(ctx.ast.allocator));
                        new_body.push(Statement::VariableDeclaration(ctx.ast.alloc(
                            VariableDeclaration {
                                span: SPAN,
                                kind: var_decl.kind,
                                declarations: single_declarations,
                                declare: var_decl.declare,
                            },
                        )));
                    }
                    self.split_count += 1;
                    continue;
                }
            }
            new_body.push(stmt.clone_in(ctx.ast.allocator));
        }
        let after = new_body.len();
        eprintln!(
            "[MULTI_VAR_SPLIT] Block body: {} -> {} statements",
            before, after
        );
        block.body = new_body;
    }

    fn exit_function_body(&mut self, body: &mut FunctionBody<'a>, ctx: &mut Ctx<'a>) {
        let has_multi = body.statements.iter().any(|s| {
            matches!(
                s,
                Statement::VariableDeclaration(var_decl)
                    if var_decl.declarations.len() > 1
            )
        });
        if !has_multi {
            return;
        }

        let before = body.statements.len();
        let mut new_stmts = ctx.ast.vec();
        for stmt in body.statements.iter() {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                if var_decl.declarations.len() > 1 {
                    eprintln!(
                        "[MULTI_VAR_SPLIT] Splitting {:?} declaration with {} declarators in function body",
                        var_decl.kind,
                        var_decl.declarations.len()
                    );
                    for declarator in var_decl.declarations.iter() {
                        let mut single_declarations = ctx.ast.vec();
                        single_declarations.push(declarator.clone_in(ctx.ast.allocator));
                        new_stmts.push(Statement::VariableDeclaration(ctx.ast.alloc(
                            VariableDeclaration {
                                span: SPAN,
                                kind: var_decl.kind,
                                declarations: single_declarations,
                                declare: var_decl.declare,
                            },
                        )));
                    }
                    self.split_count += 1;
                    continue;
                }
            }
            new_stmts.push(stmt.clone_in(ctx.ast.allocator));
        }
        let after = new_stmts.len();
        eprintln!(
            "[MULTI_VAR_SPLIT] Function body: {} -> {} statements",
            before, after
        );
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

    fn run_split(code: &str) -> (String, usize) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut splitter = MultiVarSplitter::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut splitter, &mut program, &mut ctx);

        (Codegen::new().build(&program).code, splitter.split_count())
    }

    #[test]
    fn test_split_var() {
        let (output, count) = run_split("var a = 1, b = 2, c = 3;");
        assert!(
            count >= 1,
            "Should have split at least 1 declaration, got: {}",
            count
        );
        let var_count = output.matches("var ").count();
        assert_eq!(
            var_count, 3,
            "Should have 3 separate var declarations, got {} in: {}",
            var_count, output
        );
        assert!(
            output.contains("a = 1"),
            "Should contain 'a = 1', got: {}",
            output
        );
        assert!(
            output.contains("b = 2"),
            "Should contain 'b = 2', got: {}",
            output
        );
        assert!(
            output.contains("c = 3"),
            "Should contain 'c = 3', got: {}",
            output
        );
    }

    #[test]
    fn test_split_let() {
        let (output, count) = run_split("let x = 1, y = 2;");
        assert!(
            count >= 1,
            "Should have split at least 1 declaration, got: {}",
            count
        );
        let let_count = output.matches("let ").count();
        assert_eq!(
            let_count, 2,
            "Should have 2 separate let declarations, got {} in: {}",
            let_count, output
        );
        assert!(
            output.contains("x = 1"),
            "Should contain 'x = 1', got: {}",
            output
        );
        assert!(
            output.contains("y = 2"),
            "Should contain 'y = 2', got: {}",
            output
        );
    }

    #[test]
    fn test_no_split_single() {
        let (_output, count) = run_split("var a = 1;");
        assert_eq!(
            count, 0,
            "Should not split single-declarator var, got: {}",
            count
        );
    }

    #[test]
    fn test_split_in_function() {
        let (output, count) = run_split("function f() { var a = 1, b = 2; }");
        assert!(
            count >= 1,
            "Should have split declaration in function body, got: {}",
            count
        );
        let var_count = output.matches("var ").count();
        assert_eq!(
            var_count, 2,
            "Should have 2 separate var declarations in function, got {} in: {}",
            var_count, output
        );
        assert!(
            output.contains("a = 1") && output.contains("b = 2"),
            "Should contain both declarations, got: {}",
            output
        );
    }

    #[test]
    fn test_no_split_for_var() {
        let (output, count) = run_split("for (var i = 0, j = 10; i < j; i++) {}");
        assert_eq!(
            count, 0,
            "Should NOT split for-loop var declaration, got: {}",
            count
        );
        assert!(
            output.contains("for"),
            "Should preserve for loop, got: {}",
            output
        );
    }
}
