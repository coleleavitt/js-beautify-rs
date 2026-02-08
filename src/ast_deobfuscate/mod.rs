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
pub mod empty_statement_cleanup;
pub mod expression_simplify;
pub mod function_inline;
pub mod iife_unwrap;
pub mod multi_var_split;
pub mod object_sparsing;
pub mod operator_proxy;
pub mod sequence_expression_split;
pub mod short_circuit_to_if;
pub mod state;
pub mod strength_reduction;
pub mod string_array_inline;
pub mod string_array_rotation;
pub mod ternary;
pub mod ternary_to_if_else;
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
pub use empty_statement_cleanup::EmptyStatementCleanup;
pub use expression_simplify::ExpressionSimplifier;
pub use function_inline::{FunctionCollector, FunctionInliner};
pub use iife_unwrap::IifeUnwrap;
pub use multi_var_split::MultiVarSplitter;
pub use object_sparsing::ObjectSparsingConsolidator;
pub use operator_proxy::{OperatorProxyCollector, OperatorProxyInliner};
pub use sequence_expression_split::SequenceExpressionSplitter;
pub use short_circuit_to_if::ShortCircuitToIf;
pub use state::DeobfuscateState;
pub use strength_reduction::StrengthReducer;
pub use string_array_inline::StringArrayInliner;
pub use string_array_rotation::StringArrayRotation;
pub use ternary::TernarySimplifier;
pub use ternary_to_if_else::TernaryToIfElse;
pub use try_catch::TryCatchRemover;
pub use unicode_mangling::UnicodeNormalizer;
pub use variable_rename::VariableRenamer;
pub use void_replacer::VoidReplacer;

use oxc_allocator::Allocator;
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_traverse::{traverse_mut_with_ctx, ReusableTraverseCtx};

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
    empty_statement_cleanup: EmptyStatementCleanup,
    sequence_expression_splitter: SequenceExpressionSplitter,
    multi_var_splitter: MultiVarSplitter,
    ternary_to_if_else: TernaryToIfElse,
    short_circuit_to_if: ShortCircuitToIf,
    iife_unwrap: IifeUnwrap,
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
            empty_statement_cleanup: EmptyStatementCleanup::new(),
            sequence_expression_splitter: SequenceExpressionSplitter::new(),
            multi_var_splitter: MultiVarSplitter::new(),
            ternary_to_if_else: TernaryToIfElse::new(),
            short_circuit_to_if: ShortCircuitToIf::new(),
            iife_unwrap: IifeUnwrap::new(),
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

        eprintln!("[DEOBFUSCATE] Phase 1: SemanticBuilder for control_flow_unflattener");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);

        eprintln!("[DEOBFUSCATE] Phase 1: Running control_flow_unflattener");
        traverse_mut_with_ctx(&mut self.control_flow_unflattener, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 1: control_flow_unflattener DONE");

        eprintln!("[DEOBFUSCATE] Phase 2: SemanticBuilder for string_array_rotation");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);

        eprintln!("[DEOBFUSCATE] Phase 2: Running string_array_rotation");
        traverse_mut_with_ctx(&mut self.string_array_rotation, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 2: string_array_rotation DONE");

        let mut state = ctx.into_state();
        self.string_array_rotation.finalize(&mut state);

        eprintln!("[DEOBFUSCATE] Phase 3: SemanticBuilder for decoder/string_array/dispatcher");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        eprintln!("[DEOBFUSCATE] Phase 3: Running decoder_inliner");
        traverse_mut_with_ctx(&mut self.decoder_inliner, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 3: Running string_array_inliner");
        traverse_mut_with_ctx(&mut self.string_array_inliner, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 3: Running dispatcher_inliner");
        traverse_mut_with_ctx(&mut self.dispatcher_inliner, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 3: DONE");

        eprintln!("[DEOBFUSCATE] Phase 4: SemanticBuilder for call_proxy");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        let mut call_proxy_collector = CallProxyCollector::new();
        eprintln!("[DEOBFUSCATE] Phase 4: Running call_proxy_collector");
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

        eprintln!("[DEOBFUSCATE] Phase 5: SemanticBuilder for operator_proxy");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        let mut op_proxy_collector = OperatorProxyCollector::new();
        eprintln!("[DEOBFUSCATE] Phase 5: Running operator_proxy_collector");
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

        eprintln!("[DEOBFUSCATE] Phase 6: SemanticBuilder for simplification passes");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 6: Running expression_simplifier");
        traverse_mut_with_ctx(&mut self.expression_simplifier, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 6: Running constant_folder");
        traverse_mut_with_ctx(&mut self.constant_folder, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 6: Running algebraic_simplifier");
        traverse_mut_with_ctx(&mut self.algebraic_simplifier, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 6: Running strength_reducer");
        traverse_mut_with_ctx(&mut self.strength_reducer, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 6: DONE");

        eprintln!("[DEOBFUSCATE] Phase 7: SemanticBuilder for dead_code_eliminator");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 7: Running dead_code_eliminator");
        traverse_mut_with_ctx(&mut self.dead_code_eliminator, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 7: DONE");

        eprintln!("[DEOBFUSCATE] Phase 8: SemanticBuilder for dead_var");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        let mut collector = DeadVarCollector::new();
        eprintln!("[DEOBFUSCATE] Phase 8: Running dead_var_collector");
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

        eprintln!("[DEOBFUSCATE] Phase 9: SemanticBuilder for function_inline");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        let mut func_collector = FunctionCollector::new();
        eprintln!("[DEOBFUSCATE] Phase 9: Running function_collector");
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

        eprintln!("[DEOBFUSCATE] Phase 10: SemanticBuilder for array/dynamic/ternary/try_catch");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 10: Running array_unpacker");
        traverse_mut_with_ctx(&mut self.array_unpacker, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 10: Running dynamic_property_converter");
        traverse_mut_with_ctx(&mut self.dynamic_property_converter, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 10: Running ternary_simplifier");
        traverse_mut_with_ctx(&mut self.ternary_simplifier, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 10: Running try_catch_remover");
        traverse_mut_with_ctx(&mut self.try_catch_remover, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 10: DONE");

        // Rebuild scoping after try_catch_remover and dead_code_eliminator
        // which create new BlockStatement nodes during traversal.
        eprintln!(
            "[DEOBFUSCATE] Phase 11: SemanticBuilder for unicode/boolean/void/object_sparsing"
        );
        let semantic_result = SemanticBuilder::new().build(&program);
        eprintln!(
            "[DEOBFUSCATE] Phase 11: SemanticBuilder completed, errors: {}",
            semantic_result.errors.len()
        );
        for (i, err) in semantic_result.errors.iter().enumerate().take(5) {
            eprintln!("[DEOBFUSCATE] Phase 11: Semantic error {}: {:?}", i, err);
        }
        let scoping = semantic_result.semantic.into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 11: Running unicode_normalizer");
        traverse_mut_with_ctx(&mut self.unicode_normalizer, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 11: unicode_normalizer DONE");
        eprintln!("[DEOBFUSCATE] Phase 11: Running boolean_literal_converter");
        traverse_mut_with_ctx(&mut self.boolean_literal_converter, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 11: Running void_replacer");
        traverse_mut_with_ctx(&mut self.void_replacer, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 11: Running object_sparsing_consolidator");
        traverse_mut_with_ctx(
            &mut self.object_sparsing_consolidator,
            &mut program,
            &mut ctx,
        );
        eprintln!("[DEOBFUSCATE] Phase 11: DONE");

        eprintln!("[DEOBFUSCATE] Phase 12: SemanticBuilder for variable_renamer");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 12: Running variable_renamer");
        traverse_mut_with_ctx(&mut self.variable_renamer, &mut program, &mut ctx);
        eprintln!("[DEOBFUSCATE] Phase 12: DONE");

        eprintln!("[DEOBFUSCATE] Phase 13: SemanticBuilder for empty_statement_cleanup");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 13: Running empty_statement_cleanup");
        traverse_mut_with_ctx(&mut self.empty_statement_cleanup, &mut program, &mut ctx);
        eprintln!(
            "[DEOBFUSCATE] Phase 13: Removed {} empty statements",
            self.empty_statement_cleanup.removed_count()
        );

        eprintln!("[DEOBFUSCATE] Phase 14: SemanticBuilder for sequence_expression_split");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 14: Running sequence_expression_splitter");
        traverse_mut_with_ctx(
            &mut self.sequence_expression_splitter,
            &mut program,
            &mut ctx,
        );
        eprintln!(
            "[DEOBFUSCATE] Phase 14: Split {} sequence expressions",
            self.sequence_expression_splitter.split_count()
        );

        eprintln!("[DEOBFUSCATE] Phase 15: SemanticBuilder for multi_var_split");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 15: Running multi_var_splitter");
        traverse_mut_with_ctx(&mut self.multi_var_splitter, &mut program, &mut ctx);
        eprintln!(
            "[DEOBFUSCATE] Phase 15: Split {} multi-var declarations",
            self.multi_var_splitter.split_count()
        );

        eprintln!("[DEOBFUSCATE] Phase 16: SemanticBuilder for ternary_to_if_else");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 16: Running ternary_to_if_else");
        traverse_mut_with_ctx(&mut self.ternary_to_if_else, &mut program, &mut ctx);
        eprintln!(
            "[DEOBFUSCATE] Phase 16: Converted {} ternary expressions to if/else",
            self.ternary_to_if_else.converted_count()
        );

        eprintln!("[DEOBFUSCATE] Phase 17: SemanticBuilder for short_circuit_to_if");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 17: Running short_circuit_to_if");
        traverse_mut_with_ctx(&mut self.short_circuit_to_if, &mut program, &mut ctx);
        eprintln!(
            "[DEOBFUSCATE] Phase 17: Converted {} short-circuit expressions to if statements",
            self.short_circuit_to_if.converted_count()
        );

        eprintln!("[DEOBFUSCATE] Phase 18: SemanticBuilder for iife_unwrap");
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        eprintln!("[DEOBFUSCATE] Phase 18: Running iife_unwrap");
        traverse_mut_with_ctx(&mut self.iife_unwrap, &mut program, &mut ctx);
        eprintln!(
            "[DEOBFUSCATE] Phase 18: Unwrapped {} IIFEs",
            self.iife_unwrap.unwrapped_count()
        );

        eprintln!("[DEOBFUSCATE] Generating output code");
        let output = Codegen::new().build(&program).code;
        eprintln!("[DEOBFUSCATE] Output generated, {} bytes", output.len());

        eprintln!("[DEOBFUSCATE] Phase 19: Annotating webpack modules");
        let output = annotate_webpack_modules(&output);

        Ok(output)
    }
}

fn annotate_webpack_modules(code: &str) -> String {
    let mut result = String::with_capacity(code.len() + 4096);
    let mut count = 0u32;

    for line in code.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("var ") {
            if let Some(name_end) = rest.find(" = v(() =>") {
                let module_name = &rest[..name_end];
                if !module_name.is_empty()
                    && module_name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                {
                    result.push_str("// ═══════════════════════════════════════\n");
                    result.push_str("// Webpack Module: ");
                    result.push_str(module_name);
                    result.push('\n');
                    result.push_str("// ═══════════════════════════════════════\n");
                    count = count.wrapping_add(1);
                }
            }
        }
        result.push_str(line);
        result.push('\n');
    }

    if count > 0 {
        eprintln!(
            "[DEOBFUSCATE] Phase 19: Annotated {} webpack modules",
            count
        );
    }

    result
}

impl Default for AstDeobfuscator {
    fn default() -> Self {
        Self::new()
    }
}
