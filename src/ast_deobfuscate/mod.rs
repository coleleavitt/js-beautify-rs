//! AST-based deobfuscation using Oxc
//!
//! This module implements all deobfuscation transformations as AST traversals
//! using the Oxc parser and traversal infrastructure. This ensures:
//! - Valid JavaScript output (no syntax errors from token manipulation)
//! - Access to full Oxc optimization suite
//! - Better performance (single parse/codegen cycle)
//! - More maintainable code following oxc_minifier patterns

pub mod algebraic_simplify;
pub mod array_unpack;
pub mod boolean_literals;
pub mod call_proxy;
pub mod constant_folding;
pub mod control_flow_unflatten;
pub mod dead_code;
pub mod dead_var_elimination;
pub mod decoder_inline;
pub mod dispatcher_inline;
pub mod dynamic_property;
pub mod expression_simplify;
pub mod function_inline;
pub mod object_sparsing;
pub mod operator_proxy;
pub mod state;
pub mod strength_reduction;
pub mod string_array_inline;
pub mod string_array_rotation;
pub mod ternary;
pub mod try_catch;
pub mod unicode_mangling;
pub mod variable_rename;
pub mod void_replacer;

pub use algebraic_simplify::AlgebraicSimplifier;
pub use array_unpack::ArrayUnpacker;
pub use boolean_literals::BooleanLiteralConverter;
pub use call_proxy::{CallProxyCollector, CallProxyInliner};
pub use constant_folding::ConstantFolder;
pub use control_flow_unflatten::ControlFlowUnflattener;
pub use dead_code::DeadCodeEliminator;
pub use dead_var_elimination::{DeadVarCollector, DeadVarEliminator};
pub use decoder_inline::DecoderInliner;
pub use dispatcher_inline::DispatcherInliner;
pub use dynamic_property::DynamicPropertyConverter;
pub use expression_simplify::ExpressionSimplifier;
pub use function_inline::{FunctionCollector, FunctionInliner};
pub use object_sparsing::ObjectSparsingConsolidator;
pub use operator_proxy::{OperatorProxyCollector, OperatorProxyInliner};
pub use state::DeobfuscateState;
pub use strength_reduction::StrengthReducer;
pub use string_array_inline::StringArrayInliner;
pub use string_array_rotation::StringArrayRotation;
pub use ternary::TernarySimplifier;
pub use try_catch::TryCatchRemover;
pub use unicode_mangling::UnicodeNormalizer;
pub use variable_rename::VariableRenamer;
pub use void_replacer::VoidReplacer;

use oxc_allocator::Allocator;
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_traverse::{ReusableTraverseCtx, traverse_mut_with_ctx};

use crate::Result;

pub struct AstDeobfuscator {
    control_flow_unflattener: ControlFlowUnflattener,
    string_array_rotation: StringArrayRotation,
    decoder_inliner: DecoderInliner,
    string_array_inliner: StringArrayInliner,
    dispatcher_inliner: DispatcherInliner,
    constant_folder: ConstantFolder,
    expression_simplifier: ExpressionSimplifier,
    algebraic_simplifier: AlgebraicSimplifier,
    strength_reducer: StrengthReducer,
    dead_code_eliminator: DeadCodeEliminator,
    array_unpacker: ArrayUnpacker,
    dynamic_property_converter: DynamicPropertyConverter,
    ternary_simplifier: TernarySimplifier,
    try_catch_remover: TryCatchRemover,
    unicode_normalizer: UnicodeNormalizer,
    boolean_literal_converter: BooleanLiteralConverter,
    void_replacer: VoidReplacer,
    object_sparsing_consolidator: ObjectSparsingConsolidator,
    variable_renamer: VariableRenamer,
}

impl AstDeobfuscator {
    pub fn new() -> Self {
        Self {
            control_flow_unflattener: ControlFlowUnflattener::new(),
            string_array_rotation: StringArrayRotation::new(),
            decoder_inliner: DecoderInliner::new(),
            string_array_inliner: StringArrayInliner::new(),
            dispatcher_inliner: DispatcherInliner::new(),
            constant_folder: ConstantFolder::new(),
            expression_simplifier: ExpressionSimplifier::new(),
            algebraic_simplifier: AlgebraicSimplifier::new(),
            strength_reducer: StrengthReducer::new(),
            dead_code_eliminator: DeadCodeEliminator::new(),
            array_unpacker: ArrayUnpacker::new(),
            dynamic_property_converter: DynamicPropertyConverter::new(),
            ternary_simplifier: TernarySimplifier::new(),
            try_catch_remover: TryCatchRemover::new(),
            unicode_normalizer: UnicodeNormalizer::new(),
            boolean_literal_converter: BooleanLiteralConverter::new(),
            void_replacer: VoidReplacer::new(),
            object_sparsing_consolidator: ObjectSparsingConsolidator::new(),
            variable_renamer: VariableRenamer::new(),
        }
    }

    pub fn deobfuscate(&mut self, code: &str) -> Result<String> {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();

        let parse_result = Parser::new(&allocator, code, source_type).parse();

        if !parse_result.errors.is_empty() {
            return Err(crate::BeautifyError::BeautificationFailed(format!(
                "Parse failed: {:?}",
                parse_result.errors.first()
            )));
        }

        let mut program = parse_result.program;

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);

        traverse_mut_with_ctx(&mut self.control_flow_unflattener, &mut program, &mut ctx);

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);

        traverse_mut_with_ctx(&mut self.string_array_rotation, &mut program, &mut ctx);

        let mut state = ctx.into_state();
        self.string_array_rotation.finalize(&mut state);

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut self.decoder_inliner, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.string_array_inliner, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.dispatcher_inliner, &mut program, &mut ctx);

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        let mut call_proxy_collector = CallProxyCollector::new();
        traverse_mut_with_ctx(&mut call_proxy_collector, &mut program, &mut ctx);
        let call_proxies = call_proxy_collector.get_single_use_proxies();
        if !call_proxies.is_empty() {
            let scoping = SemanticBuilder::new()
                .build(&program)
                .semantic
                .into_scoping();
            let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
            let mut inliner = CallProxyInliner::new(call_proxies);
            traverse_mut_with_ctx(&mut inliner, &mut program, &mut ctx);
        }

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        let mut op_proxy_collector = OperatorProxyCollector::new();
        traverse_mut_with_ctx(&mut op_proxy_collector, &mut program, &mut ctx);
        let op_proxies = op_proxy_collector.get_proxies();
        if !op_proxies.is_empty() {
            let scoping = SemanticBuilder::new()
                .build(&program)
                .semantic
                .into_scoping();
            let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
            let mut inliner = OperatorProxyInliner::new(op_proxies);
            traverse_mut_with_ctx(&mut inliner, &mut program, &mut ctx);
        }

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        traverse_mut_with_ctx(&mut self.expression_simplifier, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.constant_folder, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.algebraic_simplifier, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.strength_reducer, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.dead_code_eliminator, &mut program, &mut ctx);

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        let mut collector = DeadVarCollector::new();
        traverse_mut_with_ctx(&mut collector, &mut program, &mut ctx);
        let dead_vars = collector.get_dead_vars();
        if !dead_vars.is_empty() {
            let scoping = SemanticBuilder::new()
                .build(&program)
                .semantic
                .into_scoping();
            let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
            let mut eliminator = DeadVarEliminator::new(dead_vars);
            traverse_mut_with_ctx(&mut eliminator, &mut program, &mut ctx);
        }

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        let mut func_collector = FunctionCollector::new();
        traverse_mut_with_ctx(&mut func_collector, &mut program, &mut ctx);
        let single_use = func_collector.get_single_use_functions();
        if !single_use.is_empty() {
            let scoping = SemanticBuilder::new()
                .build(&program)
                .semantic
                .into_scoping();
            let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
            let mut inliner = FunctionInliner::new(single_use);
            traverse_mut_with_ctx(&mut inliner, &mut program, &mut ctx);
        }

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        traverse_mut_with_ctx(&mut self.array_unpacker, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.dynamic_property_converter, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.ternary_simplifier, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.try_catch_remover, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.unicode_normalizer, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.boolean_literal_converter, &mut program, &mut ctx);
        traverse_mut_with_ctx(&mut self.void_replacer, &mut program, &mut ctx);
        traverse_mut_with_ctx(
            &mut self.object_sparsing_consolidator,
            &mut program,
            &mut ctx,
        );

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        traverse_mut_with_ctx(&mut self.variable_renamer, &mut program, &mut ctx);

        let output = Codegen::new().build(&program).code;
        Ok(output)
    }
}

impl Default for AstDeobfuscator {
    fn default() -> Self {
        Self::new()
    }
}
