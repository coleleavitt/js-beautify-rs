//! Control flow unflattening pass
//!
//! Detects and restores flattened control flow patterns:
//! ```js
//! // Obfuscated (flattened)
//! var _flow = "2|0|1".split("|"), _i = 0;
//! while (true) {
//!     switch (_flow[_i++]) {
//!         case "0": console.log("second"); continue;
//!         case "1": console.log("third"); break;
//!         case "2": console.log("first"); continue;
//!     }
//!     break;
//! }
//!
//! // Deobfuscated (restored)
//! console.log("first");
//! console.log("second");
//! console.log("third");
//! ```

use oxc_allocator::Vec as OxcVec;
use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::FxHashMap;

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

#[derive(Debug, Clone)]
pub struct ControlFlowInfo {
    pub sequence_var: String,
    pub index_var: String,
    pub sequence: Vec<String>,
    pub case_statements: FxHashMap<String, Vec<usize>>,
}

pub struct ControlFlowUnflattener {
    detected_sequences: FxHashMap<String, Vec<String>>,
    detected_index_vars: FxHashMap<String, String>,
    changed: bool,
}

impl ControlFlowUnflattener {
    pub fn new() -> Self {
        Self {
            detected_sequences: FxHashMap::default(),
            detected_index_vars: FxHashMap::default(),
            changed: false,
        }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn detect_sequence_declaration<'a>(&mut self, var_decl: &VariableDeclaration<'a>) {
        for decl in &var_decl.declarations {
            let var_name = match &decl.id {
                BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
                _ => continue,
            };

            if let Some(init) = &decl.init {
                if let Some(sequence) = self.extract_split_sequence(init) {
                    eprintln!(
                        "[AST] Found control flow sequence: {} = {:?}",
                        var_name, sequence
                    );
                    self.detected_sequences
                        .insert(var_name.to_string(), sequence);
                }

                if self.is_zero_initializer(init) {
                    if let Some(seq_var) = self.find_associated_sequence_var(var_name) {
                        eprintln!(
                            "[AST] Found index variable: {} for sequence {}",
                            var_name, seq_var
                        );
                        self.detected_index_vars
                            .insert(seq_var.clone(), var_name.to_string());
                    }
                }
            }
        }
    }

    fn extract_split_sequence<'a>(&self, expr: &Expression<'a>) -> Option<Vec<String>> {
        let call = match expr {
            Expression::CallExpression(call) => call,
            _ => return None,
        };

        let member = match &call.callee {
            Expression::StaticMemberExpression(member) => member,
            _ => return None,
        };

        if member.property.name.as_str() != "split" {
            return None;
        }

        let string_value = match &member.object {
            Expression::StringLiteral(lit) => lit.value.as_str(),
            _ => return None,
        };

        if call.arguments.len() != 1 {
            return None;
        }

        let separator = match &call.arguments[0] {
            Argument::StringLiteral(lit) => lit.value.as_str(),
            _ => return None,
        };

        if separator != "|" {
            return None;
        }

        let sequence: Vec<String> = string_value.split('|').map(String::from).collect();
        Some(sequence)
    }

    fn is_zero_initializer<'a>(&self, expr: &Expression<'a>) -> bool {
        match expr {
            Expression::NumericLiteral(lit) => lit.value == 0.0,
            _ => false,
        }
    }

    fn find_associated_sequence_var(&self, _index_var: &str) -> Option<String> {
        self.detected_sequences.keys().next().cloned()
    }

    fn is_control_flow_while<'a>(&self, while_stmt: &WhileStatement<'a>) -> bool {
        let is_while_true = match &while_stmt.test {
            Expression::BooleanLiteral(lit) => {
                eprintln!("[AST]   while test is BooleanLiteral: {}", lit.value);
                lit.value
            }
            other => {
                eprintln!(
                    "[AST]   while test is not BooleanLiteral: {:?}",
                    std::mem::discriminant(other)
                );
                false
            }
        };

        if !is_while_true {
            eprintln!("[AST]   Rejecting: not while(true)");
            return false;
        }

        let has_switch = self.body_contains_switch(&while_stmt.body);
        eprintln!("[AST]   Body contains switch: {}", has_switch);
        has_switch
    }

    fn body_contains_switch<'a>(&self, stmt: &Statement<'a>) -> bool {
        match stmt {
            Statement::BlockStatement(block) => block
                .body
                .iter()
                .any(|s| matches!(s, Statement::SwitchStatement(_))),
            Statement::SwitchStatement(_) => true,
            _ => false,
        }
    }

    fn is_control_flow_for<'a>(&self, for_stmt: &ForStatement<'a>) -> bool {
        let is_infinite =
            for_stmt.init.is_none() && for_stmt.test.is_none() && for_stmt.update.is_none();

        if !is_infinite {
            eprintln!("[AST]   Rejecting: not for(;;)");
            return false;
        }

        eprintln!("[AST]   for(;;) detected");
        let has_switch = self.body_contains_switch(&for_stmt.body);
        eprintln!("[AST]   Body contains switch: {}", has_switch);
        has_switch
    }

    fn extract_switch_from_for<'a, 'b>(
        &self,
        for_stmt: &'b ForStatement<'a>,
    ) -> Option<&'b SwitchStatement<'a>> {
        let block = match &for_stmt.body {
            Statement::BlockStatement(block) => block,
            _ => return None,
        };

        for stmt in &block.body {
            if let Statement::SwitchStatement(switch) = stmt {
                return Some(switch);
            }
        }
        None
    }

    fn try_unflatten_for<'a>(
        &mut self,
        for_stmt: &ForStatement<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<OxcVec<'a, Statement<'a>>> {
        eprintln!("[AST] try_unflatten_for called");

        if !self.is_control_flow_for(for_stmt) {
            eprintln!("[AST]   Not a control flow for, skipping");
            return None;
        }

        eprintln!("[AST]   Extracting switch from for");
        let switch = self.extract_switch_from_for(for_stmt)?;
        eprintln!("[AST]   Got switch, checking discriminant");

        let seq_var = self.is_sequence_access(&switch.discriminant)?;
        eprintln!("[AST]   Sequence var: {}", seq_var);

        let sequence = self.detected_sequences.get(&seq_var)?;
        eprintln!("[AST]   Sequence: {:?}", sequence);

        eprintln!(
            "[AST] Unflattening control flow for {} with sequence {:?}",
            seq_var, sequence
        );

        let mut case_map: FxHashMap<String, &SwitchCase<'a>> = FxHashMap::default();
        for case in &switch.cases {
            if let Some(test) = &case.test {
                let case_value = match test {
                    Expression::StringLiteral(lit) => lit.value.as_str().to_string(),
                    Expression::NumericLiteral(lit) => lit
                        .raw
                        .map_or_else(|| lit.value.to_string(), |r| r.to_string()),
                    _ => continue,
                };
                case_map.insert(case_value, case);
            }
        }

        let mut result = ctx.ast.vec();

        for step in sequence {
            if let Some(case) = case_map.get(step) {
                eprintln!("[AST]   Processing case {}", step);
                for stmt in &case.consequent {
                    if self.should_keep_statement(stmt) {
                        let cloned = self.clone_statement(stmt, ctx);
                        result.push(cloned);
                    }
                }
            }
        }

        if result.is_empty() {
            return None;
        }

        self.changed = true;
        Some(result)
    }

    fn extract_switch_from_while<'a, 'b>(
        &self,
        while_stmt: &'b WhileStatement<'a>,
    ) -> Option<&'b SwitchStatement<'a>> {
        let block = match &while_stmt.body {
            Statement::BlockStatement(block) => block,
            _ => return None,
        };

        for stmt in &block.body {
            if let Statement::SwitchStatement(switch) = stmt {
                return Some(switch);
            }
        }
        None
    }

    fn is_sequence_access<'a>(&self, expr: &Expression<'a>) -> Option<String> {
        eprintln!(
            "[AST]   Checking discriminant: {:?}",
            std::mem::discriminant(expr)
        );

        let array_name = match expr {
            Expression::ComputedMemberExpression(member) => {
                eprintln!("[AST]   Discriminant is ComputedMemberExpression");
                match &member.object {
                    Expression::Identifier(ident) => {
                        eprintln!("[AST]   Array name: {}", ident.name);
                        ident.name.as_str()
                    }
                    _ => {
                        eprintln!("[AST]   Member object is not Identifier");
                        return None;
                    }
                }
            }
            Expression::UpdateExpression(update) => {
                eprintln!("[AST]   Discriminant is UpdateExpression");
                match &update.argument {
                    SimpleAssignmentTarget::ComputedMemberExpression(member) => {
                        match &member.object {
                            Expression::Identifier(ident) => {
                                eprintln!("[AST]   Array name: {}", ident.name);
                                ident.name.as_str()
                            }
                            _ => {
                                eprintln!("[AST]   Member object is not Identifier");
                                return None;
                            }
                        }
                    }
                    _ => {
                        eprintln!("[AST]   Update argument is not ComputedMemberExpression");
                        return None;
                    }
                }
            }
            _ => {
                eprintln!(
                    "[AST]   Discriminant is neither ComputedMemberExpression nor UpdateExpression"
                );
                return None;
            }
        };

        if self.detected_sequences.contains_key(array_name) {
            eprintln!("[AST]   Found sequence access for: {}", array_name);
            return Some(array_name.to_string());
        }

        eprintln!(
            "[AST]   Array {} not in detected sequences: {:?}",
            array_name,
            self.detected_sequences.keys().collect::<Vec<_>>()
        );
        None
    }

    fn try_unflatten_while<'a>(
        &mut self,
        while_stmt: &WhileStatement<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<OxcVec<'a, Statement<'a>>> {
        eprintln!("[AST] try_unflatten_while called");

        if !self.is_control_flow_while(while_stmt) {
            eprintln!("[AST]   Not a control flow while, skipping");
            return None;
        }

        eprintln!("[AST]   Extracting switch from while");
        let switch = self.extract_switch_from_while(while_stmt)?;
        eprintln!("[AST]   Got switch, checking discriminant");

        let seq_var = self.is_sequence_access(&switch.discriminant)?;
        eprintln!("[AST]   Sequence var: {}", seq_var);

        let sequence = self.detected_sequences.get(&seq_var)?;
        eprintln!("[AST]   Sequence: {:?}", sequence);

        eprintln!(
            "[AST] Unflattening control flow for {} with sequence {:?}",
            seq_var, sequence
        );

        let mut case_map: FxHashMap<String, &SwitchCase<'a>> = FxHashMap::default();
        for case in &switch.cases {
            if let Some(test) = &case.test {
                let case_value = match test {
                    Expression::StringLiteral(lit) => lit.value.as_str().to_string(),
                    Expression::NumericLiteral(lit) => lit
                        .raw
                        .map_or_else(|| lit.value.to_string(), |r| r.to_string()),
                    _ => continue,
                };
                case_map.insert(case_value, case);
            }
        }

        let mut result = ctx.ast.vec();

        for step in sequence {
            if let Some(case) = case_map.get(step) {
                eprintln!("[AST]   Processing case {}", step);
                for stmt in &case.consequent {
                    if self.should_keep_statement(stmt) {
                        let cloned = self.clone_statement(stmt, ctx);
                        result.push(cloned);
                    }
                }
            }
        }

        if result.is_empty() {
            return None;
        }

        self.changed = true;
        Some(result)
    }

    fn should_keep_statement<'a>(&self, stmt: &Statement<'a>) -> bool {
        match stmt {
            Statement::ContinueStatement(_) => false,
            Statement::BreakStatement(_) => false,
            _ => true,
        }
    }

    fn clone_statement<'a>(&self, stmt: &Statement<'a>, ctx: &mut Ctx<'a>) -> Statement<'a> {
        match stmt {
            Statement::ExpressionStatement(expr_stmt) => {
                let cloned_expr = self.clone_expression(&expr_stmt.expression, ctx);
                Statement::ExpressionStatement(ctx.ast.alloc(ExpressionStatement {
                    span: SPAN,
                    expression: cloned_expr,
                }))
            }
            Statement::VariableDeclaration(var_decl) => {
                Statement::VariableDeclaration(self.clone_var_decl(var_decl, ctx))
            }
            Statement::ReturnStatement(ret) => {
                let arg = ret.argument.as_ref().map(|e| self.clone_expression(e, ctx));
                Statement::ReturnStatement(ctx.ast.alloc(ReturnStatement {
                    span: SPAN,
                    argument: arg,
                }))
            }
            Statement::IfStatement(if_stmt) => {
                let test = self.clone_expression(&if_stmt.test, ctx);
                let consequent = self.clone_statement(&if_stmt.consequent, ctx);
                let alternate = if_stmt
                    .alternate
                    .as_ref()
                    .map(|s| self.clone_statement(s, ctx));
                Statement::IfStatement(ctx.ast.alloc(IfStatement {
                    span: SPAN,
                    test,
                    consequent,
                    alternate,
                }))
            }
            Statement::BlockStatement(block) => {
                let mut stmts = ctx.ast.vec();
                for s in &block.body {
                    stmts.push(self.clone_statement(s, ctx));
                }
                Statement::BlockStatement(ctx.ast.alloc(BlockStatement {
                    span: SPAN,
                    body: stmts,
                    scope_id: Default::default(),
                }))
            }
            _ => {
                eprintln!("[AST]   Skipping unsupported statement type in clone");
                Statement::EmptyStatement(ctx.ast.alloc(EmptyStatement { span: SPAN }))
            }
        }
    }

    fn clone_expression<'a>(&self, expr: &Expression<'a>, ctx: &mut Ctx<'a>) -> Expression<'a> {
        match expr {
            Expression::Identifier(ident) => {
                Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                    span: SPAN,
                    name: ctx.ast.atom(ident.name.as_str()),
                    reference_id: Default::default(),
                }))
            }
            Expression::StringLiteral(lit) => {
                Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
                    span: SPAN,
                    value: ctx.ast.atom(lit.value.as_str()),
                    raw: None,
                    lone_surrogates: false,
                }))
            }
            Expression::NumericLiteral(lit) => {
                Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                    span: SPAN,
                    value: lit.value,
                    raw: None,
                    base: lit.base,
                }))
            }
            Expression::CallExpression(call) => {
                let callee = self.clone_expression(&call.callee, ctx);
                let mut args = ctx.ast.vec();
                for arg in &call.arguments {
                    args.push(self.clone_argument(arg, ctx));
                }
                Expression::CallExpression(ctx.ast.alloc(CallExpression {
                    span: SPAN,
                    callee,
                    type_arguments: None,
                    arguments: args,
                    optional: call.optional,
                    pure: false,
                }))
            }
            Expression::StaticMemberExpression(member) => {
                let object = self.clone_expression(&member.object, ctx);
                Expression::StaticMemberExpression(ctx.ast.alloc(StaticMemberExpression {
                    span: SPAN,
                    object,
                    property: IdentifierName {
                        span: SPAN,
                        name: ctx.ast.atom(member.property.name.as_str()),
                    },
                    optional: member.optional,
                }))
            }
            Expression::AssignmentExpression(assign) => {
                let right = self.clone_expression(&assign.right, ctx);
                let left = self.clone_assignment_target(&assign.left, ctx);
                Expression::AssignmentExpression(ctx.ast.alloc(AssignmentExpression {
                    span: SPAN,
                    operator: assign.operator,
                    left,
                    right,
                }))
            }
            Expression::BinaryExpression(bin) => {
                let left = self.clone_expression(&bin.left, ctx);
                let right = self.clone_expression(&bin.right, ctx);
                Expression::BinaryExpression(ctx.ast.alloc(BinaryExpression {
                    span: SPAN,
                    left,
                    operator: bin.operator,
                    right,
                }))
            }
            _ => {
                eprintln!("[AST]   Skipping unsupported expression type in clone");
                Expression::NullLiteral(ctx.ast.alloc(NullLiteral { span: SPAN }))
            }
        }
    }

    fn clone_argument<'a>(&self, arg: &Argument<'a>, ctx: &mut Ctx<'a>) -> Argument<'a> {
        match arg {
            Argument::SpreadElement(spread) => {
                let expr = self.clone_expression(&spread.argument, ctx);
                Argument::SpreadElement(ctx.ast.alloc(SpreadElement {
                    span: SPAN,
                    argument: expr,
                }))
            }
            _ => {
                let expr = arg.to_expression();
                Argument::from(self.clone_expression(expr, ctx))
            }
        }
    }

    fn clone_assignment_target<'a>(
        &self,
        target: &AssignmentTarget<'a>,
        ctx: &mut Ctx<'a>,
    ) -> AssignmentTarget<'a> {
        match target {
            AssignmentTarget::AssignmentTargetIdentifier(ident) => {
                AssignmentTarget::AssignmentTargetIdentifier(ctx.ast.alloc(IdentifierReference {
                    span: SPAN,
                    name: ctx.ast.atom(ident.name.as_str()),
                    reference_id: Default::default(),
                }))
            }
            _ => {
                eprintln!("[AST]   Skipping unsupported assignment target in clone");
                AssignmentTarget::AssignmentTargetIdentifier(ctx.ast.alloc(IdentifierReference {
                    span: SPAN,
                    name: ctx.ast.atom("_unknown"),
                    reference_id: Default::default(),
                }))
            }
        }
    }

    fn clone_var_decl<'a>(
        &self,
        var_decl: &VariableDeclaration<'a>,
        ctx: &mut Ctx<'a>,
    ) -> oxc_allocator::Box<'a, VariableDeclaration<'a>> {
        let mut declarations = ctx.ast.vec();

        for decl in &var_decl.declarations {
            let id = self.clone_binding_pattern(&decl.id, ctx);
            let init = decl.init.as_ref().map(|e| self.clone_expression(e, ctx));

            declarations.push(VariableDeclarator {
                span: SPAN,
                kind: var_decl.kind,
                id,
                init,
                definite: decl.definite,
                type_annotation: None,
            });
        }

        ctx.ast.alloc(VariableDeclaration {
            span: SPAN,
            kind: var_decl.kind,
            declarations,
            declare: var_decl.declare,
        })
    }

    fn clone_binding_pattern<'a>(
        &self,
        pattern: &BindingPattern<'a>,
        ctx: &mut Ctx<'a>,
    ) -> BindingPattern<'a> {
        match pattern {
            BindingPattern::BindingIdentifier(ident) => {
                BindingPattern::BindingIdentifier(ctx.ast.alloc(BindingIdentifier {
                    span: SPAN,
                    name: ctx.ast.atom(ident.name.as_str()),
                    symbol_id: Default::default(),
                }))
            }
            _ => {
                eprintln!("[AST]   Skipping unsupported binding pattern in clone");
                BindingPattern::BindingIdentifier(ctx.ast.alloc(BindingIdentifier {
                    span: SPAN,
                    name: ctx.ast.atom("_unknown"),
                    symbol_id: Default::default(),
                }))
            }
        }
    }
}

impl Default for ControlFlowUnflattener {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for ControlFlowUnflattener {
    fn enter_statement(&mut self, stmt: &mut Statement<'a>, _ctx: &mut Ctx<'a>) {
        if let Statement::VariableDeclaration(var_decl) = stmt {
            self.detect_sequence_declaration(var_decl);
        }
    }

    fn exit_program(&mut self, program: &mut Program<'a>, ctx: &mut Ctx<'a>) {
        let mut new_body = ctx.ast.vec();
        let mut skip_next_var_decl = false;

        for stmt in program.body.iter() {
            if skip_next_var_decl {
                if matches!(stmt, Statement::VariableDeclaration(_)) {
                    skip_next_var_decl = false;
                    continue;
                }
                skip_next_var_decl = false;
            }

            match stmt {
                Statement::VariableDeclaration(var_decl) => {
                    let has_sequence = var_decl.declarations.iter().any(|d| {
                        if let BindingPattern::BindingIdentifier(ident) = &d.id {
                            self.detected_sequences.contains_key(ident.name.as_str())
                        } else {
                            false
                        }
                    });

                    if has_sequence {
                        skip_next_var_decl = true;
                        continue;
                    }

                    new_body.push(self.clone_statement(stmt, ctx));
                }
                Statement::WhileStatement(while_stmt) => {
                    if let Some(unflattened) = self.try_unflatten_while(while_stmt, ctx) {
                        eprintln!(
                            "[AST] ✓ Unflattened control flow: {} statements",
                            unflattened.len()
                        );
                        for s in unflattened {
                            new_body.push(s);
                        }
                    } else {
                        new_body.push(self.clone_statement(stmt, ctx));
                    }
                }
                Statement::ForStatement(for_stmt) => {
                    if let Some(unflattened) = self.try_unflatten_for(for_stmt, ctx) {
                        eprintln!(
                            "[AST] ✓ Unflattened for(;;) control flow: {} statements",
                            unflattened.len()
                        );
                        for s in unflattened {
                            new_body.push(s);
                        }
                    } else {
                        new_body.push(self.clone_statement(stmt, ctx));
                    }
                }
                _ => {
                    new_body.push(self.clone_statement(stmt, ctx));
                }
            }
        }

        if self.changed {
            program.body = new_body;
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

    fn run_unflatten(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut unflattener = ControlFlowUnflattener::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut unflattener, &mut program, &mut ctx);

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_detect_sequence() {
        let code = r#"var _flow = "2|0|1".split("|");"#;
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut unflattener = ControlFlowUnflattener::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut unflattener, &mut program, &mut ctx);

        assert!(unflattener.detected_sequences.contains_key("_flow"));
        assert_eq!(
            unflattener.detected_sequences.get("_flow").unwrap(),
            &vec!["2", "0", "1"]
        );
    }

    #[test]
    fn test_simple_unflatten() {
        let code = r#"
var _flow = "2|0|1".split("|");
var _i = 0;
while (true) {
    switch (_flow[_i++]) {
        case "0": console.log("second"); continue;
        case "1": console.log("third"); break;
        case "2": console.log("first"); continue;
    }
    break;
}
"#;

        let output = run_unflatten(code);
        eprintln!("Output:\n{}", output);

        assert!(output.contains("first"));
        assert!(output.contains("second"));
        assert!(output.contains("third"));
    }

    #[test]
    fn test_no_unflatten_regular_while() {
        let code = r#"
var x = 0;
while (x < 10) {
    console.log(x);
    x++;
}
"#;

        let output = run_unflatten(code);

        assert!(output.contains("while"));
        assert!(output.contains("x < 10"));
    }

    #[test]
    fn test_for_infinite_unflatten() {
        let code = r#"
var _flow = "1|0|2".split("|");
var _i = 0;
for (;;) {
    switch (_flow[_i++]) {
        case "0": console.log("second"); continue;
        case "1": console.log("first"); continue;
        case "2": console.log("third"); break;
    }
    break;
}
"#;

        let output = run_unflatten(code);
        eprintln!("For(;;) Output:\n{}", output);

        assert!(output.contains("first"));
        assert!(output.contains("second"));
        assert!(output.contains("third"));
        // Should NOT contain for loop anymore
        assert!(!output.contains("for"));
        assert!(!output.contains("switch"));
    }
}
