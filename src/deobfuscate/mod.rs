pub mod control_flow;
pub mod dead_code;
pub mod dead_code_removal;
pub mod decoder;
pub mod inline_strings;
pub mod rotation;
pub mod simplify;
pub mod string_array;

use crate::Result;
use crate::token::Token;

pub struct DeobfuscateContext {
    pub string_arrays: Vec<StringArrayInfo>,
    pub decoders: Vec<DecoderInfo>,
    pub control_flows: Vec<control_flow::ControlFlowInfo>,
}

impl DeobfuscateContext {
    pub fn new() -> Self {
        Self {
            string_arrays: Vec::new(),
            decoders: Vec::new(),
            control_flows: Vec::new(),
        }
    }

    pub fn analyze(&mut self, tokens: &[Token]) -> Result<()> {
        self.find_string_arrays(tokens)?;
        self.find_decoders(tokens)?;
        self.find_control_flows(tokens)?;
        Ok(())
    }

    pub fn deobfuscate(&self, tokens: &mut Vec<Token>) -> Result<()> {
        inline_strings::inline_decoded_strings(tokens, &self.string_arrays, &self.decoders)?;

        self.unflatten_control_flow(tokens)?;

        let simplified_tokens = simplify::simplify_expressions(tokens)?;
        *tokens = simplified_tokens;

        let cleaned_tokens =
            dead_code::remove_dead_code(tokens, &self.string_arrays, &self.decoders)?;
        *tokens = cleaned_tokens;

        let dead_removed_tokens = dead_code_removal::remove_dead_code_conditionals(tokens)?;
        *tokens = dead_removed_tokens;

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
