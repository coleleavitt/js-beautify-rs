//! Object Sparsing Consolidation
//!
//! Transforms sparse object construction patterns:
//! ```js
//! var obj = {};
//! obj.a = 1;
//! obj.b = 2;
//! ```
//! Into:
//! ```js
//! var obj = {a: 1, b: 2};
//! ```

use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

#[derive(Debug, Clone)]
enum PropertyValue {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
    Identifier(String),
}

#[derive(Debug, Clone)]
struct CollectedProperty {
    name: String,
    value: PropertyValue,
    stmt_index: usize,
}

pub struct ObjectSparsingConsolidator {
    changed: bool,
}

impl ObjectSparsingConsolidator {
    pub fn new() -> Self {
        Self { changed: false }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn is_empty_object(expr: &Expression<'_>) -> bool {
        matches!(expr, Expression::ObjectExpression(obj) if obj.properties.is_empty())
    }

    fn get_empty_object_decl_name(stmt: &Statement<'_>) -> Option<String> {
        let Statement::VariableDeclaration(decl) = stmt else {
            return None;
        };

        if decl.declarations.len() != 1 {
            return None;
        }

        let declarator = &decl.declarations[0];

        let init = declarator.init.as_ref()?;
        if !Self::is_empty_object(init) {
            return None;
        }

        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            return None;
        };

        Some(id.name.as_str().to_string())
    }

    fn extract_property_assignment(
        stmt: &Statement<'_>,
        target_name: &str,
    ) -> Option<(String, PropertyValue)> {
        let Statement::ExpressionStatement(expr_stmt) = stmt else {
            return None;
        };

        let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
            return None;
        };

        if assign.operator != AssignmentOperator::Assign {
            return None;
        }

        let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            return None;
        };

        let Expression::Identifier(obj_id) = &member.object else {
            return None;
        };

        if obj_id.name.as_str() != target_name {
            return None;
        }

        let prop_name = member.property.name.as_str().to_string();
        let value = Self::extract_value(&assign.right)?;

        Some((prop_name, value))
    }

    fn extract_value(expr: &Expression<'_>) -> Option<PropertyValue> {
        match expr {
            Expression::NumericLiteral(n) => Some(PropertyValue::Number(n.value)),
            Expression::StringLiteral(s) => {
                Some(PropertyValue::String(s.value.as_str().to_string()))
            }
            Expression::BooleanLiteral(b) => Some(PropertyValue::Bool(b.value)),
            Expression::NullLiteral(_) => Some(PropertyValue::Null),
            Expression::Identifier(id) => {
                Some(PropertyValue::Identifier(id.name.as_str().to_string()))
            }
            _ => None,
        }
    }

    fn build_property_value<'a>(value: &PropertyValue, ctx: &mut Ctx<'a>) -> Expression<'a> {
        match value {
            PropertyValue::Number(n) => Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                span: SPAN,
                value: *n,
                raw: None,
                base: oxc_syntax::number::NumberBase::Decimal,
            })),
            PropertyValue::String(s) => Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
                span: SPAN,
                value: ctx.ast.atom(s),
                raw: None,
                lone_surrogates: false,
            })),
            PropertyValue::Bool(b) => Expression::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
                span: SPAN,
                value: *b,
            })),
            PropertyValue::Null => {
                Expression::NullLiteral(ctx.ast.alloc(NullLiteral { span: SPAN }))
            }
            PropertyValue::Identifier(name) => {
                Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                    span: SPAN,
                    name: ctx.ast.atom(name),
                    reference_id: Default::default(),
                }))
            }
        }
    }
}

impl Default for ObjectSparsingConsolidator {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for ObjectSparsingConsolidator {
    fn exit_statements(
        &mut self,
        stmts: &mut oxc_allocator::Vec<'a, Statement<'a>>,
        ctx: &mut Ctx<'a>,
    ) {
        let mut i = 0;
        while i < stmts.len() {
            let Some(obj_name) = Self::get_empty_object_decl_name(&stmts[i]) else {
                i += 1;
                continue;
            };

            let decl_index = i;
            let mut collected: Vec<CollectedProperty> = Vec::new();
            let mut j = i + 1;

            while j < stmts.len() {
                if let Some((prop_name, value)) =
                    Self::extract_property_assignment(&stmts[j], &obj_name)
                {
                    collected.push(CollectedProperty {
                        name: prop_name,
                        value,
                        stmt_index: j,
                    });
                    j += 1;
                } else {
                    break;
                }
            }

            if collected.is_empty() {
                i += 1;
                continue;
            }

            eprintln!(
                "[AST] Consolidating {} properties into object '{}'",
                collected.len(),
                obj_name
            );

            let indices_to_remove: Vec<usize> = collected.iter().map(|p| p.stmt_index).collect();
            for idx in indices_to_remove.into_iter().rev() {
                stmts.remove(idx);
            }

            let mut new_properties: oxc_allocator::Vec<'a, ObjectPropertyKind<'a>> =
                oxc_allocator::Vec::new_in(ctx.ast.allocator);

            for prop in &collected {
                let value_expr = Self::build_property_value(&prop.value, ctx);
                let property = ObjectPropertyKind::ObjectProperty(ctx.ast.alloc(ObjectProperty {
                    span: SPAN,
                    kind: PropertyKind::Init,
                    key: PropertyKey::StaticIdentifier(ctx.ast.alloc(IdentifierName {
                        span: SPAN,
                        name: ctx.ast.atom(&prop.name),
                    })),
                    value: value_expr,
                    method: false,
                    shorthand: false,
                    computed: false,
                }));
                new_properties.push(property);
            }

            let new_object = Expression::ObjectExpression(ctx.ast.alloc(ObjectExpression {
                span: SPAN,
                properties: new_properties,
            }));

            if let Statement::VariableDeclaration(decl) = &mut stmts[decl_index] {
                if let Some(declarator) = decl.declarations.first_mut() {
                    declarator.init = Some(new_object);
                }
            }

            self.changed = true;
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

    fn run_consolidation(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut consolidator = ObjectSparsingConsolidator::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut consolidator, &mut program, &mut ctx);

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_consolidate_simple() {
        let output = run_consolidation(
            r#"
            var obj = {};
            obj.a = 1;
            obj.b = 2;
        "#,
        );
        eprintln!("Output: {}", output);
        assert!(
            output.contains("a:") && output.contains("b:"),
            "Expected consolidated object, got: {}",
            output
        );
    }

    #[test]
    fn test_consolidate_with_strings() {
        let output = run_consolidation(
            r#"
            var config = {};
            config.name = "test";
            config.value = 42;
        "#,
        );
        eprintln!("Output: {}", output);
        assert!(
            output.contains("name:") && output.contains("value:"),
            "Expected consolidated object, got: {}",
            output
        );
    }

    #[test]
    fn test_no_consolidation_non_empty() {
        let output = run_consolidation(
            r#"
            var obj = {existing: true};
            obj.a = 1;
        "#,
        );
        eprintln!("Output: {}", output);
        assert!(
            output.contains("obj.a = 1"),
            "Should preserve non-empty object assignment, got: {}",
            output
        );
    }

    #[test]
    fn test_no_consolidation_gap() {
        let output = run_consolidation(
            r#"
            var obj = {};
            console.log("break");
            obj.a = 1;
        "#,
        );
        eprintln!("Output: {}", output);
        assert!(
            output.contains("obj.a = 1"),
            "Should preserve assignment after gap, got: {}",
            output
        );
    }
}
