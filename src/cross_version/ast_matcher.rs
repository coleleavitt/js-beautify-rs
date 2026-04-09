//! AST-based statement matching using structure hashes.

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub struct IdentifierInfo {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub order_index: usize,
}

#[derive(Debug, Clone)]
pub struct StatementInfo {
    pub start: u32,
    pub end: u32,
    pub hash: String,
    pub identifiers: Vec<IdentifierInfo>,
}

pub struct StatementMatcher {
    max_depth: usize,
}

impl StatementMatcher {
    #[must_use]
    pub fn new(max_depth: usize) -> Self {
        Self { max_depth }
    }

    pub fn extract_statements(&self, source: &str) -> Vec<StatementInfo> {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();

        let parse_result = Parser::new(&allocator, source, source_type).parse();

        if !parse_result.errors.is_empty() {
            eprintln!(
                "[AST_MATCHER] Parse had {} errors (continuing anyway), first: {:?}",
                parse_result.errors.len(),
                parse_result.errors.first()
            );
        }

        let mut statements = Vec::new();
        eprintln!(
            "[AST_MATCHER] Program body has {} statements",
            parse_result.program.body.len()
        );

        for stmt in &parse_result.program.body {
            let mut normalizer = AstNormalizer::new(self.max_depth);
            normalizer.visit_statement(stmt);
            let hash = format!("{:x}", md5::compute(&normalizer.output));

            let mut id_collector = IdentifierCollector::new();
            id_collector.visit_statement(stmt);

            statements.push(StatementInfo {
                start: stmt.span().start,
                end: stmt.span().end,
                hash,
                identifiers: id_collector.identifiers,
            });
        }

        statements
    }

    pub fn match_statements<'a>(
        &self,
        source_stmts: &'a [StatementInfo],
        target_stmts: &'a [StatementInfo],
    ) -> Vec<(&'a StatementInfo, &'a StatementInfo)> {
        let mut target_by_hash: FxHashMap<&str, Vec<&StatementInfo>> = FxHashMap::default();
        for stmt in target_stmts {
            target_by_hash.entry(&stmt.hash).or_default().push(stmt);
        }

        let mut used_targets: FxHashMap<(u32, u32), ()> = FxHashMap::default();
        let mut matches = Vec::new();

        for source_stmt in source_stmts {
            let Some(candidates) = target_by_hash.get(source_stmt.hash.as_str()) else {
                continue;
            };

            let target_stmt = candidates
                .iter()
                .find(|t| !used_targets.contains_key(&(t.start, t.end)));

            if let Some(&target_stmt) = target_stmt {
                used_targets.insert((target_stmt.start, target_stmt.end), ());
                matches.push((source_stmt, target_stmt));
            }
        }

        matches
    }
}

impl Default for StatementMatcher {
    fn default() -> Self {
        Self::new(12)
    }
}

struct AstNormalizer {
    output: String,
    depth: usize,
    max_depth: usize,
}

impl AstNormalizer {
    fn new(max_depth: usize) -> Self {
        Self {
            output: String::new(),
            depth: 0,
            max_depth,
        }
    }

    fn push(&mut self, s: &str) {
        self.output.push_str(s);
    }
}

impl<'a> Visit<'a> for AstNormalizer {
    fn visit_statement(&mut self, stmt: &Statement<'a>) {
        if self.depth >= self.max_depth {
            return;
        }
        self.depth += 1;

        match stmt {
            Statement::BlockStatement(_) => self.push("Block"),
            Statement::BreakStatement(_) => self.push("Break"),
            Statement::ContinueStatement(_) => self.push("Continue"),
            Statement::DebuggerStatement(_) => self.push("Debugger"),
            Statement::DoWhileStatement(_) => self.push("DoWhile"),
            Statement::EmptyStatement(_) => self.push("Empty"),
            Statement::ExpressionStatement(_) => self.push("Expr"),
            Statement::ForInStatement(_) => self.push("ForIn"),
            Statement::ForOfStatement(_) => self.push("ForOf"),
            Statement::ForStatement(_) => self.push("For"),
            Statement::IfStatement(_) => self.push("If"),
            Statement::LabeledStatement(_) => self.push("Labeled"),
            Statement::ReturnStatement(_) => self.push("Return"),
            Statement::SwitchStatement(_) => self.push("Switch"),
            Statement::ThrowStatement(_) => self.push("Throw"),
            Statement::TryStatement(_) => self.push("Try"),
            Statement::WhileStatement(_) => self.push("While"),
            Statement::WithStatement(_) => self.push("With"),
            Statement::VariableDeclaration(_) => self.push("VarDecl"),
            Statement::FunctionDeclaration(_) => self.push("FuncDecl"),
            Statement::ClassDeclaration(_) => self.push("ClassDecl"),
            Statement::ImportDeclaration(_) => self.push("Import"),
            Statement::ExportAllDeclaration(_) => self.push("ExportAll"),
            Statement::ExportDefaultDeclaration(_) => self.push("ExportDefault"),
            Statement::ExportNamedDeclaration(_) => self.push("ExportNamed"),
            Statement::TSTypeAliasDeclaration(_) => self.push("TSTypeAlias"),
            Statement::TSInterfaceDeclaration(_) => self.push("TSInterface"),
            Statement::TSEnumDeclaration(_) => self.push("TSEnum"),
            Statement::TSModuleDeclaration(_) => self.push("TSModule"),
            Statement::TSImportEqualsDeclaration(_) => self.push("TSImportEquals"),
            Statement::TSExportAssignment(_) => self.push("TSExportAssign"),
            Statement::TSNamespaceExportDeclaration(_) => self.push("TSNsExport"),
            Statement::TSGlobalDeclaration(_) => self.push("TSGlobal"),
        }

        walk::walk_statement(self, stmt);
        self.depth -= 1;
    }

    fn visit_expression(&mut self, expr: &Expression<'a>) {
        if self.depth >= self.max_depth {
            return;
        }
        self.depth += 1;

        match expr {
            Expression::BooleanLiteral(_) => self.push("Bool"),
            Expression::NullLiteral(_) => self.push("Null"),
            Expression::NumericLiteral(_) => self.push("Num"),
            Expression::BigIntLiteral(_) => self.push("BigInt"),
            Expression::RegExpLiteral(_) => self.push("RegExp"),
            Expression::StringLiteral(_) => self.push("Str"),
            Expression::TemplateLiteral(_) => self.push("Template"),
            Expression::Identifier(_) => self.push("Id"),
            Expression::MetaProperty(_) => self.push("Meta"),
            Expression::Super(_) => self.push("Super"),
            Expression::ArrayExpression(_) => self.push("Array"),
            Expression::ArrowFunctionExpression(_) => self.push("Arrow"),
            Expression::AssignmentExpression(_) => self.push("Assign"),
            Expression::AwaitExpression(_) => self.push("Await"),
            Expression::BinaryExpression(e) => {
                self.push("Bin");
                self.push(e.operator.as_str());
            }
            Expression::CallExpression(_) => self.push("Call"),
            Expression::ChainExpression(_) => self.push("Chain"),
            Expression::ClassExpression(_) => self.push("Class"),
            Expression::ConditionalExpression(_) => self.push("Cond"),
            Expression::FunctionExpression(_) => self.push("Func"),
            Expression::ImportExpression(_) => self.push("DynImport"),
            Expression::LogicalExpression(e) => {
                self.push("Logic");
                self.push(e.operator.as_str());
            }
            Expression::NewExpression(_) => self.push("New"),
            Expression::ObjectExpression(_) => self.push("Obj"),
            Expression::ParenthesizedExpression(_) => self.push("Paren"),
            Expression::SequenceExpression(_) => self.push("Seq"),
            Expression::TaggedTemplateExpression(_) => self.push("TaggedTemplate"),
            Expression::ThisExpression(_) => self.push("This"),
            Expression::UnaryExpression(e) => {
                self.push("Unary");
                self.push(e.operator.as_str());
            }
            Expression::UpdateExpression(e) => {
                self.push("Update");
                self.push(e.operator.as_str());
            }
            Expression::YieldExpression(_) => self.push("Yield"),
            Expression::PrivateInExpression(_) => self.push("PrivateIn"),
            Expression::StaticMemberExpression(_) => self.push("StaticMember"),
            Expression::ComputedMemberExpression(_) => self.push("ComputedMember"),
            Expression::PrivateFieldExpression(_) => self.push("PrivateField"),
            Expression::TSAsExpression(_) => self.push("TSAs"),
            Expression::TSSatisfiesExpression(_) => self.push("TSSatisfies"),
            Expression::TSTypeAssertion(_) => self.push("TSTypeAssertion"),
            Expression::TSNonNullExpression(_) => self.push("TSNonNull"),
            Expression::TSInstantiationExpression(_) => self.push("TSInstantiation"),
            Expression::JSXElement(_) => self.push("JSXElement"),
            Expression::JSXFragment(_) => self.push("JSXFragment"),
            Expression::V8IntrinsicExpression(_) => self.push("V8Intrinsic"),
        }

        walk::walk_expression(self, expr);
        self.depth -= 1;
    }
}

struct IdentifierCollector {
    identifiers: Vec<IdentifierInfo>,
    order_index: usize,
}

impl IdentifierCollector {
    fn new() -> Self {
        Self {
            identifiers: Vec::new(),
            order_index: 0,
        }
    }

    fn is_reserved(name: &str) -> bool {
        const RESERVED: &[&str] = &[
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "default",
            "delete",
            "do",
            "else",
            "enum",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "let",
            "new",
            "null",
            "return",
            "static",
            "super",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "var",
            "void",
            "while",
            "with",
            "yield",
            "implements",
            "interface",
            "package",
            "private",
            "protected",
            "public",
            "arguments",
            "eval",
            "undefined",
            "NaN",
            "Infinity",
            "Object",
            "Array",
            "String",
            "Number",
            "Boolean",
            "Function",
            "Symbol",
            "Error",
            "Promise",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "JSON",
            "Math",
            "console",
            "process",
            "require",
            "module",
            "exports",
            "global",
            "globalThis",
            "window",
            "document",
        ];
        RESERVED.contains(&name)
    }
}

impl<'a> Visit<'a> for IdentifierCollector {
    fn visit_identifier_reference(&mut self, ident: &IdentifierReference<'a>) {
        if !Self::is_reserved(ident.name.as_str()) {
            self.identifiers.push(IdentifierInfo {
                name: ident.name.to_string(),
                start: ident.span.start,
                end: ident.span.end,
                order_index: self.order_index,
            });
            self.order_index += 1;
        }
    }

    fn visit_binding_identifier(&mut self, ident: &BindingIdentifier<'a>) {
        if !Self::is_reserved(ident.name.as_str()) {
            self.identifiers.push(IdentifierInfo {
                name: ident.name.to_string(),
                start: ident.span.start,
                end: ident.span.end,
                order_index: self.order_index,
            });
            self.order_index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statement_extraction() {
        let matcher = StatementMatcher::default();
        let source = "var x = 1;\nvar y = 2;\nfunction foo() { return x + y; }";
        let stmts = matcher.extract_statements(source);
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn test_identical_statements_match() {
        let matcher = StatementMatcher::default();
        let source = "var x = 1;";
        let stmts1 = matcher.extract_statements(source);
        let stmts2 = matcher.extract_statements(source);

        let matches = matcher.match_statements(&stmts1, &stmts2);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_different_var_names_same_structure() {
        let matcher = StatementMatcher::default();
        let source1 = "var abc = 1;";
        let source2 = "var xyz = 1;";
        let stmts1 = matcher.extract_statements(source1);
        let stmts2 = matcher.extract_statements(source2);

        assert_eq!(stmts1[0].hash, stmts2[0].hash);
    }

    #[test]
    fn test_identifier_collection() {
        let matcher = StatementMatcher::default();
        let source = "var x = y + z;";
        let stmts = matcher.extract_statements(source);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].identifiers.len(), 3);
    }
}
