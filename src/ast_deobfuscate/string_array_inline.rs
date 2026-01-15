//! String array inlining pass
//!
//! Replaces array access expressions with string literals:
//! ```js
//! var _0x1234 = ["hello", "world"];
//! console.log(_0x1234[0]); // Inlined to: console.log("hello");
//! ```

use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct StringArrayInliner {
    changed: bool,
}

impl StringArrayInliner {
    pub fn new() -> Self {
        Self { changed: false }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn try_inline_array_access<'a>(
        &mut self,
        member: &ComputedMemberExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        eprintln!("[AST] Checking computed member expression for string array access");

        let array_name = match &member.object {
            Expression::Identifier(ident) => {
                eprintln!("[AST]   Object: {}", ident.name);
                ident.name.as_str()
            }
            _ => {
                eprintln!("[AST]   Object is not identifier");
                return None;
            }
        };

        let array_info = ctx.state.string_arrays.get(array_name)?;
        eprintln!(
            "[AST]   Found array: {} with {} strings",
            array_name,
            array_info.strings.len()
        );

        let index = match &member.expression {
            Expression::NumericLiteral(lit) => {
                let idx = lit.value as usize;
                eprintln!("[AST]   Index: {}", idx);
                idx
            }
            _ => {
                eprintln!("[AST]   Index is not numeric literal");
                return None;
            }
        };

        if index >= array_info.strings.len() {
            eprintln!(
                "[AST]   Index {} out of bounds (array has {} elements)",
                index,
                array_info.strings.len()
            );
            return None;
        }

        let string_value = &array_info.strings[index];
        eprintln!(
            "[AST] ✓ Inlining: {}[{}] → \"{}\"",
            array_name, index, string_value
        );

        Some(Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
            span: SPAN,
            value: ctx.ast.atom(string_value.as_str()),
            raw: None,
            lone_surrogates: false,
        })))
    }
}

impl Default for StringArrayInliner {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for StringArrayInliner {
    fn enter_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        if let Expression::ComputedMemberExpression(member) = expr {
            if let Some(new_expr) = self.try_inline_array_access(member, ctx) {
                *expr = new_expr;
                self.changed = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_deobfuscate::{DeobfuscateState, StringArrayRotation};
    use oxc_allocator::Allocator;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;

    fn run_string_array_inline(code: &str) -> (String, StringArrayInliner) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut rotation = StringArrayRotation::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = oxc_traverse::ReusableTraverseCtx::new(state, scoping, &allocator);

        oxc_traverse::traverse_mut_with_ctx(&mut rotation, &mut program, &mut ctx);

        let mut state = DeobfuscateState::new();
        rotation.finalize(&mut state);

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = oxc_traverse::ReusableTraverseCtx::new(state, scoping, &allocator);

        let mut inliner = StringArrayInliner::new();
        oxc_traverse::traverse_mut_with_ctx(&mut inliner, &mut program, &mut ctx);

        let output = Codegen::new().build(&program).code;
        (output, inliner)
    }

    #[test]
    fn test_simple_array_access() {
        let code = r#"
            var _0x1234 = ["hello", "world", "test"];
            console.log(_0x1234[0]);
            console.log(_0x1234[1]);
        "#;

        let (output, inliner) = run_string_array_inline(code);
        eprintln!("Output:\n{}", output);

        assert!(inliner.has_changed(), "Should have inlined array access");
        assert!(
            output.contains("\"hello\""),
            "Should contain inlined 'hello'"
        );
        assert!(
            output.contains("\"world\""),
            "Should contain inlined 'world'"
        );
        assert!(
            !output.contains("_0x1234[0]"),
            "Should not contain array access"
        );
    }

    #[test]
    fn test_rotated_array_access() {
        let code = r#"
            var _0x1111 = ["a", "b", "c"];
            (function(_0x2222, _0x3333) {
                var _0x4444 = function(_0x5555) {
                    while (--_0x5555) {
                        _0x2222.push(_0x2222.shift());
                    }
                };
                _0x4444(2);
            })(_0x1111, 0x123);
            var result = _0x1111[0];
        "#;

        let (output, inliner) = run_string_array_inline(code);
        eprintln!("Output:\n{}", output);

        assert!(inliner.has_changed(), "Should have inlined array access");
        assert!(
            output.contains("\"c\""),
            "Should contain rotated first element"
        );
    }

    #[test]
    fn test_out_of_bounds_access() {
        let code = r#"
            var _0x1234 = ["hello"];
            console.log(_0x1234[10]);
        "#;

        let (output, inliner) = run_string_array_inline(code);
        eprintln!("Output:\n{}", output);

        assert!(
            !inliner.has_changed(),
            "Should not inline out-of-bounds access"
        );
        assert!(
            output.contains("_0x1234[10]"),
            "Should preserve out-of-bounds access"
        );
    }

    #[test]
    fn test_non_literal_index() {
        let code = r#"
            var _0x1234 = ["hello", "world"];
            var i = 0;
            console.log(_0x1234[i]);
        "#;

        let (output, inliner) = run_string_array_inline(code);
        eprintln!("Output:\n{}", output);

        assert!(!inliner.has_changed(), "Should not inline variable index");
        assert!(
            output.contains("_0x1234[i]"),
            "Should preserve variable index"
        );
    }

    #[test]
    fn test_mixed_access() {
        let code = r#"
            var _0x1234 = ["a", "b", "c"];
            var x = _0x1234[0];
            var y = _0x1234[someVar];
            var z = _0x1234[2];
        "#;

        let (output, inliner) = run_string_array_inline(code);
        eprintln!("Output:\n{}", output);

        assert!(inliner.has_changed(), "Should have inlined literal indices");
        assert!(output.contains("\"a\""), "Should inline index 0");
        assert!(output.contains("\"c\""), "Should inline index 2");
        assert!(
            output.contains("_0x1234[someVar]"),
            "Should preserve variable index"
        );
    }
}
