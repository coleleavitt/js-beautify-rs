pub mod algebraic_simplify;
pub mod array_unpack;
pub mod boolean_literals;
pub mod call_proxy;
pub mod constant_folding;
pub mod control_flow;
pub mod dead_code;
pub mod dead_code_removal;
pub mod dead_var_elimination;
pub mod decoder;
pub mod dynamic_property;
pub mod expression_simplify;
pub mod function_inline;
pub mod inline_strings;
pub mod object_dispatcher;
pub mod object_sparsing;
pub mod operator_proxy;
pub mod rotation;
pub mod simplify;
pub mod strength_reduction;
pub mod string_array;
pub mod ternary;
pub mod try_catch;
pub mod unicode_mangling;
pub mod void_replacer;

use crate::Result;
use crate::token::Token;

pub struct DeobfuscateContext {
    pub string_arrays: Vec<StringArrayInfo>,
    pub decoders: Vec<DecoderInfo>,
    pub control_flows: Vec<control_flow::ControlFlowInfo>,
    pub dispatchers: Vec<object_dispatcher::DispatcherInfo>,
    pub call_proxies: Vec<call_proxy::ProxyInfo>,
    pub operator_proxies: Vec<operator_proxy::OperatorProxyInfo>,
}

impl DeobfuscateContext {
    pub fn new() -> Self {
        Self {
            string_arrays: Vec::new(),
            decoders: Vec::new(),
            control_flows: Vec::new(),
            dispatchers: Vec::new(),
            call_proxies: Vec::new(),
            operator_proxies: Vec::new(),
        }
    }

    pub fn analyze(&mut self, tokens: &[Token]) -> Result<()> {
        self.find_string_arrays(tokens)?;
        self.find_decoders(tokens)?;
        self.find_control_flows(tokens)?;
        self.find_dispatchers(tokens)?;
        self.find_call_proxies(tokens)?;
        self.find_operator_proxies(tokens)?;
        Ok(())
    }

    pub fn deobfuscate(&self, tokens: &mut Vec<Token>) -> Result<()> {
        inline_strings::inline_decoded_strings(tokens, &self.string_arrays, &self.decoders)?;

        self.unflatten_control_flow(tokens)?;

        let simplified_tokens = simplify::simplify_expressions(tokens)?;
        *tokens = simplified_tokens;

        let folded_tokens = constant_folding::fold_constants(tokens)?;
        *tokens = folded_tokens;

        let expr_simplified = expression_simplify::simplify_expressions(tokens)?;
        *tokens = expr_simplified;

        let strength_reduced = strength_reduction::apply_strength_reduction(tokens)?;
        *tokens = strength_reduced;

        let algebra_simplified = algebraic_simplify::simplify_algebraic(tokens)?;
        *tokens = algebra_simplified;

        let array_unpacked = array_unpack::unpack_array_access(tokens)?;
        *tokens = array_unpacked;

        let dead_vars_removed = dead_var_elimination::eliminate_dead_variables(tokens)?;
        *tokens = dead_vars_removed;

        let cleaned_tokens =
            dead_code::remove_dead_code(tokens, &self.string_arrays, &self.decoders)?;
        *tokens = cleaned_tokens;

        let dead_removed_tokens = dead_code_removal::remove_dead_code_conditionals(tokens)?;
        *tokens = dead_removed_tokens;

        let dispatcher_inlined =
            object_dispatcher::inline_dispatcher_calls(tokens, &self.dispatchers)?;
        *tokens = dispatcher_inlined;

        let proxy_inlined = call_proxy::inline_call_proxies(tokens, &self.call_proxies)?;
        *tokens = proxy_inlined;

        let operator_inlined =
            operator_proxy::inline_operator_proxies(tokens, &self.operator_proxies)?;
        *tokens = operator_inlined;

        let inlinable_funcs = function_inline::detect_inlinable_functions(tokens)?;
        let func_inlined = function_inline::inline_single_use_functions(tokens, &inlinable_funcs)?;
        *tokens = func_inlined;

        let props_converted = dynamic_property::convert_dynamic_properties(tokens)?;
        *tokens = props_converted;

        let try_catch_removed = try_catch::remove_empty_try_catch(tokens)?;
        *tokens = try_catch_removed;

        let ternary_simplified = ternary::simplify_ternary_chains(tokens)?;
        *tokens = ternary_simplified;

        let sparse_consolidated = object_sparsing::consolidate_sparse_objects(tokens)?;
        *tokens = sparse_consolidated;

        let unicode_normalized = unicode_mangling::normalize_unicode(tokens)?;
        *tokens = unicode_normalized;

        let boolean_replaced = boolean_literals::replace_boolean_literals(tokens)?;
        *tokens = boolean_replaced;

        let void_replaced = void_replacer::replace_void_zero(tokens)?;
        *tokens = void_replaced;

        Ok(())
    }

    fn unflatten_control_flow(&self, tokens: &mut Vec<Token>) -> Result<()> {
        if self.control_flows.is_empty() {
            return Ok(());
        }

        for cf_info in &self.control_flows {
            let reconstructed = control_flow::reconstruct_control_flow(tokens, cf_info)?;

            tokens.splice(cf_info.start_index..=cf_info.end_index, reconstructed);
        }

        Ok(())
    }

    fn find_string_arrays(&mut self, tokens: &[Token]) -> Result<()> {
        self.string_arrays = string_array::find_string_arrays(tokens)?;

        for array in &mut self.string_arrays {
            rotation::detect_and_apply_rotation(tokens, array)?;
        }

        Ok(())
    }

    fn find_decoders(&mut self, tokens: &[Token]) -> Result<()> {
        self.decoders = decoder::find_decoders(tokens, &self.string_arrays)?;
        Ok(())
    }

    fn find_control_flows(&mut self, tokens: &[Token]) -> Result<()> {
        self.control_flows = control_flow::detect_control_flow_flattening(tokens)?;
        Ok(())
    }

    fn find_dispatchers(&mut self, tokens: &[Token]) -> Result<()> {
        self.dispatchers = object_dispatcher::detect_object_dispatchers(tokens)?;
        Ok(())
    }

    fn find_call_proxies(&mut self, tokens: &[Token]) -> Result<()> {
        self.call_proxies = call_proxy::detect_call_proxies(tokens)?;
        Ok(())
    }

    fn find_operator_proxies(&mut self, tokens: &[Token]) -> Result<()> {
        self.operator_proxies = operator_proxy::detect_operator_proxies(tokens)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StringArrayInfo {
    pub variable_name: String,
    pub start_index: usize,
    pub end_index: usize,
    pub strings: Vec<String>,
    pub rotated: bool,
}

#[derive(Debug, Clone)]
pub struct DecoderInfo {
    pub name: String,
    pub array_name: String,
    pub start_index: usize,
    pub end_index: usize,
    pub offset: i32,
}

impl Default for DeobfuscateContext {
    fn default() -> Self {
        Self::new()
    }
}
