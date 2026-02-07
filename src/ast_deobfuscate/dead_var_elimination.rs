use oxc_allocator::CloneIn;
use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

#[derive(Debug, Clone)]
struct VarInfo {
    declaration_count: usize,
    read_count: usize,
    write_count: usize,
    is_function_param: bool,
    is_exported: bool,
}

impl VarInfo {
    fn new() -> Self {
        Self {
            declaration_count: 0,
            read_count: 0,
            write_count: 0,
            is_function_param: false,
            is_exported: false,
        }
    }

    fn is_dead(&self) -> bool {
        self.declaration_count > 0
            && self.read_count == 0
            && !self.is_function_param
            && !self.is_exported
    }
}

pub struct DeadVarCollector {
    variables: FxHashMap<String, VarInfo>,
    in_lhs_of_assignment: bool,
    in_declaration: bool,
    current_declaration_name: Option<String>,
}

impl DeadVarCollector {
    pub fn new() -> Self {
        Self {
            variables: FxHashMap::default(),
            in_lhs_of_assignment: false,
            in_declaration: false,
            current_declaration_name: None,
        }
    }

    pub fn get_dead_vars(&self) -> FxHashSet<String> {
        self.variables
            .iter()
            .filter(|(_, info)| info.is_dead())
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn record_declaration(&mut self, name: &str) {
        let entry = self
            .variables
            .entry(name.to_string())
            .or_insert_with(VarInfo::new);
        entry.declaration_count += 1;
    }

    fn record_read(&mut self, name: &str) {
        let entry = self
            .variables
            .entry(name.to_string())
            .or_insert_with(VarInfo::new);
        entry.read_count += 1;
    }

    fn record_write(&mut self, name: &str) {
        let entry = self
            .variables
            .entry(name.to_string())
            .or_insert_with(VarInfo::new);
        entry.write_count += 1;
    }

    fn record_function_param(&mut self, name: &str) {
        let entry = self
            .variables
            .entry(name.to_string())
            .or_insert_with(VarInfo::new);
        entry.is_function_param = true;
    }

    fn record_export(&mut self, name: &str) {
        let entry = self
            .variables
            .entry(name.to_string())
            .or_insert_with(VarInfo::new);
        entry.is_exported = true;
    }
}

impl Default for DeadVarCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for DeadVarCollector {
    fn enter_variable_declarator(&mut self, decl: &mut VariableDeclarator<'a>, _ctx: &mut Ctx<'a>) {
        if let BindingPattern::BindingIdentifier(ident) = &decl.id {
            let name = ident.name.as_str();
            self.record_declaration(name);
            self.current_declaration_name = Some(name.to_string());
            self.in_declaration = true;
            eprintln!("[AST] Found declaration: {}", name);
        }
    }

    fn exit_variable_declarator(&mut self, _decl: &mut VariableDeclarator<'a>, _ctx: &mut Ctx<'a>) {
        self.in_declaration = false;
        self.current_declaration_name = None;
    }

    fn enter_formal_parameter(&mut self, param: &mut FormalParameter<'a>, _ctx: &mut Ctx<'a>) {
        if let BindingPattern::BindingIdentifier(ident) = &param.pattern {
            let name = ident.name.as_str();
            self.record_function_param(name);
            eprintln!("[AST] Found function param: {}", name);
        }
    }

    fn enter_assignment_expression(
        &mut self,
        _expr: &mut AssignmentExpression<'a>,
        _ctx: &mut Ctx<'a>,
    ) {
        self.in_lhs_of_assignment = true;
    }

    fn exit_assignment_expression(
        &mut self,
        expr: &mut AssignmentExpression<'a>,
        _ctx: &mut Ctx<'a>,
    ) {
        self.in_lhs_of_assignment = false;

        if let AssignmentTarget::AssignmentTargetIdentifier(ident) = &expr.left {
            let name = ident.name.as_str();
            self.record_write(name);
        }
    }

    fn enter_update_expression(&mut self, expr: &mut UpdateExpression<'a>, _ctx: &mut Ctx<'a>) {
        if let SimpleAssignmentTarget::AssignmentTargetIdentifier(ident) = &expr.argument {
            let name = ident.name.as_str();
            self.record_read(name);
            self.record_write(name);
        }
    }

    fn enter_identifier_reference(
        &mut self,
        ident: &mut IdentifierReference<'a>,
        _ctx: &mut Ctx<'a>,
    ) {
        let name = ident.name.as_str();

        if self.in_declaration {
            if let Some(ref decl_name) = self.current_declaration_name {
                if name == decl_name {
                    return;
                }
            }
        }

        if !self.in_lhs_of_assignment {
            self.record_read(name);
        }
    }

    fn enter_export_named_declaration(
        &mut self,
        decl: &mut ExportNamedDeclaration<'a>,
        _ctx: &mut Ctx<'a>,
    ) {
        for specifier in &decl.specifiers {
            if let ModuleExportName::IdentifierName(ident) = &specifier.local {
                self.record_export(ident.name.as_str());
            } else if let ModuleExportName::IdentifierReference(ident) = &specifier.local {
                self.record_export(ident.name.as_str());
            }
        }
    }

    fn enter_export_default_declaration(
        &mut self,
        decl: &mut ExportDefaultDeclaration<'a>,
        _ctx: &mut Ctx<'a>,
    ) {
        match &decl.declaration {
            ExportDefaultDeclarationKind::Identifier(ident) => {
                self.record_export(ident.name.as_str());
            }
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                if let Some(ref id) = func.id {
                    self.record_export(id.name.as_str());
                }
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                if let Some(ref id) = class.id {
                    self.record_export(id.name.as_str());
                }
            }
            _ => {}
        }
    }
}

pub struct DeadVarEliminator {
    dead_vars: FxHashSet<String>,
    changed: bool,
}

impl DeadVarEliminator {
    pub fn new(dead_vars: FxHashSet<String>) -> Self {
        Self {
            dead_vars,
            changed: false,
        }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn should_remove_declarator(&self, decl: &VariableDeclarator<'_>) -> bool {
        if let BindingPattern::BindingIdentifier(ident) = &decl.id {
            let name = ident.name.as_str();
            if self.dead_vars.contains(name) {
                if let Some(init) = &decl.init {
                    if Self::has_side_effects(init) {
                        return false;
                    }
                }
                return true;
            }
        }
        false
    }

    fn has_side_effects(expr: &Expression<'_>) -> bool {
        match expr {
            Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::Identifier(_)
            | Expression::ThisExpression(_) => false,

            Expression::ArrayExpression(arr) => arr.elements.iter().any(|el| match el {
                ArrayExpressionElement::SpreadElement(spread) => {
                    Self::has_side_effects(&spread.argument)
                }
                ArrayExpressionElement::Elision(_) => false,
                _ => {
                    if let Some(expr) = el.as_expression() {
                        Self::has_side_effects(expr)
                    } else {
                        true
                    }
                }
            }),

            Expression::ObjectExpression(obj) => obj.properties.iter().any(|prop| match prop {
                ObjectPropertyKind::ObjectProperty(p) => {
                    let key_has_side_effects = if p.computed {
                        p.key.as_expression().map_or(false, Self::has_side_effects)
                    } else {
                        false
                    };
                    Self::has_side_effects(&p.value) || key_has_side_effects
                }
                ObjectPropertyKind::SpreadProperty(spread) => {
                    Self::has_side_effects(&spread.argument)
                }
            }),

            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => false,

            Expression::UnaryExpression(unary) => {
                matches!(unary.operator, UnaryOperator::Delete)
                    || Self::has_side_effects(&unary.argument)
            }

            Expression::BinaryExpression(binary) => {
                Self::has_side_effects(&binary.left) || Self::has_side_effects(&binary.right)
            }

            Expression::LogicalExpression(logical) => {
                Self::has_side_effects(&logical.left) || Self::has_side_effects(&logical.right)
            }

            Expression::ConditionalExpression(cond) => {
                Self::has_side_effects(&cond.test)
                    || Self::has_side_effects(&cond.consequent)
                    || Self::has_side_effects(&cond.alternate)
            }

            Expression::SequenceExpression(seq) => {
                seq.expressions.iter().any(|e| Self::has_side_effects(e))
            }

            Expression::ParenthesizedExpression(paren) => Self::has_side_effects(&paren.expression),

            Expression::CallExpression(_)
            | Expression::NewExpression(_)
            | Expression::AssignmentExpression(_)
            | Expression::UpdateExpression(_)
            | Expression::YieldExpression(_)
            | Expression::AwaitExpression(_)
            | Expression::TaggedTemplateExpression(_) => true,

            _ => true,
        }
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for DeadVarEliminator {
    fn exit_statement(&mut self, stmt: &mut Statement<'a>, ctx: &mut Ctx<'a>) {
        if let Statement::VariableDeclaration(var_decl) = stmt {
            let all_dead = var_decl
                .declarations
                .iter()
                .all(|d| self.should_remove_declarator(d));

            if all_dead && !var_decl.declarations.is_empty() {
                for decl in &var_decl.declarations {
                    if let BindingPattern::BindingIdentifier(ident) = &decl.id {
                        eprintln!("[AST] Eliminating dead variable: {}", ident.name);
                    }
                }
                self.changed = true;
                *stmt = Statement::EmptyStatement(ctx.ast.alloc(EmptyStatement { span: SPAN }));
                return;
            }

            let has_dead = var_decl
                .declarations
                .iter()
                .any(|d| self.should_remove_declarator(d));
            if has_dead {
                let mut new_declarations = ctx.ast.vec();
                for decl in var_decl.declarations.iter() {
                    if !self.should_remove_declarator(decl) {
                        new_declarations.push(Self::clone_declarator(decl, ctx));
                    } else if let BindingPattern::BindingIdentifier(ident) = &decl.id {
                        eprintln!("[AST] Eliminating dead variable: {}", ident.name);
                        self.changed = true;
                    }
                }

                if new_declarations.is_empty() {
                    *stmt = Statement::EmptyStatement(ctx.ast.alloc(EmptyStatement { span: SPAN }));
                } else {
                    *stmt = Statement::VariableDeclaration(ctx.ast.alloc(VariableDeclaration {
                        span: SPAN,
                        kind: var_decl.kind,
                        declarations: new_declarations,
                        declare: var_decl.declare,
                    }));
                }
            }
        }
    }
}

impl DeadVarEliminator {
    fn clone_declarator<'a>(
        decl: &VariableDeclarator<'a>,
        ctx: &mut Ctx<'a>,
    ) -> VariableDeclarator<'a> {
        decl.clone_in(ctx.ast.allocator)
    }

    fn clone_binding_pattern<'a>(
        pattern: &BindingPattern<'a>,
        ctx: &mut Ctx<'a>,
    ) -> BindingPattern<'a> {
        pattern.clone_in(ctx.ast.allocator)
    }

    fn clone_expression<'a>(expr: &Expression<'a>, ctx: &mut Ctx<'a>) -> Expression<'a> {
        expr.clone_in(ctx.ast.allocator)
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

    fn run_elimination(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut collector = DeadVarCollector::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut collector, &mut program, &mut ctx);

        let dead_vars = collector.get_dead_vars();
        eprintln!("Dead vars: {:?}", dead_vars);

        if !dead_vars.is_empty() {
            let mut eliminator = DeadVarEliminator::new(dead_vars);
            let state = DeobfuscateState::new();
            let scoping = SemanticBuilder::new()
                .build(&program)
                .semantic
                .into_scoping();
            let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

            traverse_mut_with_ctx(&mut eliminator, &mut program, &mut ctx);
        }

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_remove_unused_var() {
        let output = run_elimination("var unused = 5;");
        eprintln!("Output: {}", output);
        assert!(
            !output.contains("unused"),
            "Should remove unused variable, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_used_var() {
        let output = run_elimination("var used = 5; console.log(used);");
        assert!(
            output.contains("used"),
            "Should preserve used variable, got: {}",
            output
        );
    }

    #[test]
    fn test_remove_multiple_unused() {
        let output = run_elimination("var a = 1; var b = 2;");
        assert!(
            !output.contains("a") && !output.contains("b"),
            "Should remove multiple unused variables, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_with_side_effects() {
        let output = run_elimination("var x = someFunction();");
        assert!(
            output.contains("someFunction"),
            "Should preserve declaration with side effects, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_function_params() {
        let output = run_elimination("function f(param) { return 1; }");
        assert!(
            output.contains("param"),
            "Should preserve function parameters, got: {}",
            output
        );
    }

    #[test]
    fn test_mixed_declarations() {
        let output = run_elimination("var used = 1, unused = 2; console.log(used);");
        eprintln!("Mixed output: {}", output);
        assert!(
            output.contains("used"),
            "Should preserve used variable, got: {}",
            output
        );
    }

    #[test]
    fn test_written_but_not_read() {
        let output = run_elimination("var x = 1; x = 2;");
        eprintln!("Written not read output: {}", output);
        assert!(
            !output.contains("var x"),
            "Should remove variable that is only written, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_read_after_write() {
        let output = run_elimination("var x = 1; x = 2; console.log(x);");
        assert!(
            output.contains("var x"),
            "Should preserve variable that is read, got: {}",
            output
        );
    }
}
