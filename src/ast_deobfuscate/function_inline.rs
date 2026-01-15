use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_syntax::number::NumberBase;
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::FxHashMap;

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

const MAX_BODY_STATEMENTS: usize = 5;
const MAX_PARAMS: usize = 10;

#[derive(Debug, Clone)]
pub struct InlinableFunction {
    pub name: String,
    pub params: Vec<String>,
    pub return_expression: Option<ReturnExpr>,
    pub is_simple: bool,
}

#[derive(Debug, Clone)]
pub enum ReturnExpr {
    Identifier(String),
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Binary {
        left: Box<ReturnExpr>,
        op: BinaryOp,
        right: Box<ReturnExpr>,
    },
    Unary {
        op: UnaryOp,
        arg: Box<ReturnExpr>,
    },
    Call {
        callee: String,
        args: Vec<ReturnExpr>,
    },
    ParamRef(usize),
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Not,
    Neg,
    Plus,
    BitNot,
}

pub struct FunctionCollector {
    functions: FxHashMap<String, InlinableFunction>,
    call_counts: FxHashMap<String, usize>,
}

impl FunctionCollector {
    pub fn new() -> Self {
        Self {
            functions: FxHashMap::default(),
            call_counts: FxHashMap::default(),
        }
    }

    pub fn get_single_use_functions(&self) -> FxHashMap<String, InlinableFunction> {
        self.functions
            .iter()
            .filter(|(name, func)| {
                let count = self.call_counts.get(*name).copied().unwrap_or(0);
                count == 1 && func.is_simple
            })
            .map(|(name, func)| (name.clone(), func.clone()))
            .collect()
    }

    fn try_extract_inlinable(&mut self, func: &Function<'_>) -> Option<InlinableFunction> {
        let name = func.id.as_ref()?.name.as_str().to_string();

        if func.r#async || func.generator {
            return None;
        }

        let body = func.body.as_ref()?;

        if body.statements.len() > MAX_BODY_STATEMENTS {
            return None;
        }

        let params: Vec<String> = func
            .params
            .items
            .iter()
            .filter_map(|p| {
                if let BindingPattern::BindingIdentifier(ident) = &p.pattern {
                    Some(ident.name.as_str().to_string())
                } else {
                    None
                }
            })
            .collect();

        if params.len() > MAX_PARAMS || params.len() != func.params.items.len() {
            return None;
        }

        let return_expression = self.extract_return_expression(body, &params);
        let is_simple = return_expression.is_some() && body.statements.len() == 1;

        eprintln!(
            "[AST] Found inlinable function: {} ({} params, simple={})",
            name,
            params.len(),
            is_simple
        );

        Some(InlinableFunction {
            name,
            params,
            return_expression,
            is_simple,
        })
    }

    fn extract_return_expression(
        &self,
        body: &FunctionBody<'_>,
        params: &[String],
    ) -> Option<ReturnExpr> {
        if body.statements.len() != 1 {
            return None;
        }

        if let Statement::ReturnStatement(ret) = &body.statements[0] {
            return ret
                .argument
                .as_ref()
                .and_then(|expr| self.expr_to_return_expr(expr, params));
        }

        None
    }

    fn expr_to_return_expr(&self, expr: &Expression<'_>, params: &[String]) -> Option<ReturnExpr> {
        match expr {
            Expression::Identifier(ident) => {
                let name = ident.name.as_str();
                if let Some(idx) = params.iter().position(|p| p == name) {
                    Some(ReturnExpr::ParamRef(idx))
                } else {
                    Some(ReturnExpr::Identifier(name.to_string()))
                }
            }
            Expression::NumericLiteral(num) => Some(ReturnExpr::Number(num.value)),
            Expression::StringLiteral(s) => Some(ReturnExpr::String(s.value.as_str().to_string())),
            Expression::BooleanLiteral(b) => Some(ReturnExpr::Boolean(b.value)),
            Expression::NullLiteral(_) => Some(ReturnExpr::Null),
            Expression::BinaryExpression(binary) => {
                let left = self.expr_to_return_expr(&binary.left, params)?;
                let right = self.expr_to_return_expr(&binary.right, params)?;
                let op = match binary.operator {
                    BinaryOperator::Addition => BinaryOp::Add,
                    BinaryOperator::Subtraction => BinaryOp::Sub,
                    BinaryOperator::Multiplication => BinaryOp::Mul,
                    BinaryOperator::Division => BinaryOp::Div,
                    BinaryOperator::Remainder => BinaryOp::Mod,
                    BinaryOperator::BitwiseAnd => BinaryOp::BitAnd,
                    BinaryOperator::BitwiseOR => BinaryOp::BitOr,
                    BinaryOperator::BitwiseXOR => BinaryOp::BitXor,
                    BinaryOperator::Equality => BinaryOp::Eq,
                    BinaryOperator::Inequality => BinaryOp::NotEq,
                    BinaryOperator::StrictEquality => BinaryOp::StrictEq,
                    BinaryOperator::StrictInequality => BinaryOp::StrictNotEq,
                    BinaryOperator::LessThan => BinaryOp::Lt,
                    BinaryOperator::LessEqualThan => BinaryOp::Lte,
                    BinaryOperator::GreaterThan => BinaryOp::Gt,
                    BinaryOperator::GreaterEqualThan => BinaryOp::Gte,
                    _ => return None,
                };
                Some(ReturnExpr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                })
            }
            Expression::UnaryExpression(unary) => {
                let arg = self.expr_to_return_expr(&unary.argument, params)?;
                let op = match unary.operator {
                    UnaryOperator::LogicalNot => UnaryOp::Not,
                    UnaryOperator::UnaryNegation => UnaryOp::Neg,
                    UnaryOperator::UnaryPlus => UnaryOp::Plus,
                    UnaryOperator::BitwiseNot => UnaryOp::BitNot,
                    _ => return None,
                };
                Some(ReturnExpr::Unary {
                    op,
                    arg: Box::new(arg),
                })
            }
            Expression::ParenthesizedExpression(paren) => {
                self.expr_to_return_expr(&paren.expression, params)
            }
            _ => None,
        }
    }
}

impl Default for FunctionCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for FunctionCollector {
    fn enter_function(&mut self, func: &mut Function<'a>, _ctx: &mut Ctx<'a>) {
        if let Some(inlinable) = self.try_extract_inlinable(func) {
            self.functions.insert(inlinable.name.clone(), inlinable);
        }
    }

    fn enter_call_expression(&mut self, call: &mut CallExpression<'a>, _ctx: &mut Ctx<'a>) {
        if let Expression::Identifier(ident) = &call.callee {
            let name = ident.name.as_str().to_string();
            *self.call_counts.entry(name).or_insert(0) += 1;
        }
    }
}

pub struct FunctionInliner {
    functions: FxHashMap<String, InlinableFunction>,
    changed: bool,
}

impl FunctionInliner {
    pub fn new(functions: FxHashMap<String, InlinableFunction>) -> Self {
        Self {
            functions,
            changed: false,
        }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn try_inline_call<'a>(
        &mut self,
        call: &CallExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        let name = if let Expression::Identifier(ident) = &call.callee {
            ident.name.as_str()
        } else {
            return None;
        };

        let func = self.functions.get(name)?;

        if call.arguments.len() != func.params.len() {
            return None;
        }

        let return_expr = func.return_expression.as_ref()?;

        let args: Vec<_> = call
            .arguments
            .iter()
            .filter_map(|arg| arg.as_expression())
            .collect();

        if args.len() != func.params.len() {
            return None;
        }

        eprintln!("[AST] Inlining call to function: {}", name);
        self.changed = true;

        Some(self.build_expression(return_expr, &args, ctx))
    }

    fn build_expression<'a>(
        &self,
        ret_expr: &ReturnExpr,
        args: &[&Expression<'a>],
        ctx: &mut Ctx<'a>,
    ) -> Expression<'a> {
        match ret_expr {
            ReturnExpr::ParamRef(idx) => {
                if *idx < args.len() {
                    Self::clone_expression(args[*idx], ctx)
                } else {
                    Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                        span: SPAN,
                        name: ctx.ast.atom("undefined"),
                        reference_id: Default::default(),
                    }))
                }
            }
            ReturnExpr::Identifier(name) => {
                Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                    span: SPAN,
                    name: ctx.ast.atom(name),
                    reference_id: Default::default(),
                }))
            }
            ReturnExpr::Number(val) => Expression::NumericLiteral(ctx.ast.alloc(NumericLiteral {
                span: SPAN,
                value: *val,
                raw: Some(ctx.ast.atom(&val.to_string())),
                base: NumberBase::Decimal,
            })),
            ReturnExpr::String(s) => Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
                span: SPAN,
                value: ctx.ast.atom(s),
                raw: None,
                lone_surrogates: false,
            })),
            ReturnExpr::Boolean(b) => Expression::BooleanLiteral(ctx.ast.alloc(BooleanLiteral {
                span: SPAN,
                value: *b,
            })),
            ReturnExpr::Null => Expression::NullLiteral(ctx.ast.alloc(NullLiteral { span: SPAN })),
            ReturnExpr::Binary { left, op, right } => {
                let left_expr = self.build_expression(left, args, ctx);
                let right_expr = self.build_expression(right, args, ctx);
                let operator = match op {
                    BinaryOp::Add => BinaryOperator::Addition,
                    BinaryOp::Sub => BinaryOperator::Subtraction,
                    BinaryOp::Mul => BinaryOperator::Multiplication,
                    BinaryOp::Div => BinaryOperator::Division,
                    BinaryOp::Mod => BinaryOperator::Remainder,
                    BinaryOp::BitAnd => BinaryOperator::BitwiseAnd,
                    BinaryOp::BitOr => BinaryOperator::BitwiseOR,
                    BinaryOp::BitXor => BinaryOperator::BitwiseXOR,
                    BinaryOp::And => BinaryOperator::BitwiseAnd,
                    BinaryOp::Or => BinaryOperator::BitwiseOR,
                    BinaryOp::Eq => BinaryOperator::Equality,
                    BinaryOp::NotEq => BinaryOperator::Inequality,
                    BinaryOp::StrictEq => BinaryOperator::StrictEquality,
                    BinaryOp::StrictNotEq => BinaryOperator::StrictInequality,
                    BinaryOp::Lt => BinaryOperator::LessThan,
                    BinaryOp::Lte => BinaryOperator::LessEqualThan,
                    BinaryOp::Gt => BinaryOperator::GreaterThan,
                    BinaryOp::Gte => BinaryOperator::GreaterEqualThan,
                };
                Expression::BinaryExpression(ctx.ast.alloc(BinaryExpression {
                    span: SPAN,
                    left: left_expr,
                    operator,
                    right: right_expr,
                }))
            }
            ReturnExpr::Unary { op, arg } => {
                let arg_expr = self.build_expression(arg, args, ctx);
                let operator = match op {
                    UnaryOp::Not => UnaryOperator::LogicalNot,
                    UnaryOp::Neg => UnaryOperator::UnaryNegation,
                    UnaryOp::Plus => UnaryOperator::UnaryPlus,
                    UnaryOp::BitNot => UnaryOperator::BitwiseNot,
                };
                Expression::UnaryExpression(ctx.ast.alloc(UnaryExpression {
                    span: SPAN,
                    operator,
                    argument: arg_expr,
                }))
            }
            ReturnExpr::Call {
                callee,
                args: call_args,
            } => {
                let mut arguments = ctx.ast.vec();
                for arg in call_args {
                    let expr = self.build_expression(arg, args, ctx);
                    arguments.push(Argument::from(expr));
                }
                Expression::CallExpression(ctx.ast.alloc(CallExpression {
                    span: SPAN,
                    callee: Expression::Identifier(ctx.ast.alloc(IdentifierReference {
                        span: SPAN,
                        name: ctx.ast.atom(callee),
                        reference_id: Default::default(),
                    })),
                    arguments,
                    optional: false,
                    type_arguments: None,
                    pure: false,
                }))
            }
        }
    }

    fn clone_expression<'a>(expr: &Expression<'a>, ctx: &mut Ctx<'a>) -> Expression<'a> {
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
}

impl<'a> Traverse<'a, DeobfuscateState> for FunctionInliner {
    fn exit_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        if let Expression::CallExpression(call) = expr {
            if let Some(inlined) = self.try_inline_call(call, ctx) {
                *expr = inlined;
            }
        }
    }

    fn exit_statement(&mut self, stmt: &mut Statement<'a>, ctx: &mut Ctx<'a>) {
        if let Statement::FunctionDeclaration(func) = stmt {
            if let Some(id) = &func.id {
                let name = id.name.as_str();
                if self.functions.contains_key(name) {
                    eprintln!("[AST] Removing inlined function declaration: {}", name);
                    self.changed = true;
                    *stmt = Statement::EmptyStatement(ctx.ast.alloc(EmptyStatement { span: SPAN }));
                }
            }
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

    fn run_inline(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut collector = FunctionCollector::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut collector, &mut program, &mut ctx);

        let single_use = collector.get_single_use_functions();
        eprintln!(
            "Single use functions: {:?}",
            single_use.keys().collect::<Vec<_>>()
        );

        if !single_use.is_empty() {
            let mut inliner = FunctionInliner::new(single_use);
            let state = DeobfuscateState::new();
            let scoping = SemanticBuilder::new()
                .build(&program)
                .semantic
                .into_scoping();
            let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

            traverse_mut_with_ctx(&mut inliner, &mut program, &mut ctx);
        }

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_inline_simple_wrapper() {
        let output = run_inline("function twice(n) { return n * 2; } var result = twice(10);");
        eprintln!("Output: {}", output);
        assert!(
            !output.contains("function twice"),
            "Function declaration should be removed, got: {}",
            output
        );
        assert!(
            output.contains("10 * 2") || output.contains("10*2"),
            "Call should be inlined, got: {}",
            output
        );
    }

    #[test]
    fn test_inline_identity() {
        let output = run_inline("function id(x) { return x; } var y = id(42);");
        eprintln!("Output: {}", output);
        assert!(
            !output.contains("function id"),
            "Function should be removed, got: {}",
            output
        );
        assert!(
            output.contains("42"),
            "Should contain the inlined value, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_multi_use() {
        let output = run_inline(
            "function add(a, b) { return a + b; } var x = add(1, 2); var y = add(3, 4);",
        );
        eprintln!("Output: {}", output);
        assert!(
            output.contains("function add"),
            "Multi-use function should be preserved, got: {}",
            output
        );
    }

    #[test]
    fn test_inline_with_constant() {
        let output = run_inline("function getConst() { return 42; } var x = getConst();");
        eprintln!("Output: {}", output);
        assert!(
            !output.contains("function getConst"),
            "Function should be removed, got: {}",
            output
        );
        assert!(
            output.contains("42"),
            "Should contain the constant, got: {}",
            output
        );
    }
}
