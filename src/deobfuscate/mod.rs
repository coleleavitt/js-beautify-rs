pub mod decoder;
pub mod inline_strings;
pub mod rotation;
pub mod string_array;

use crate::Result;
use crate::token::Token;

pub struct DeobfuscateContext {
    pub string_arrays: Vec<StringArrayInfo>,
    pub decoders: Vec<DecoderInfo>,
}

impl DeobfuscateContext {
    pub fn new() -> Self {
        Self {
            string_arrays: Vec::new(),
            decoders: Vec::new(),
        }
    }

    pub fn analyze(&mut self, tokens: &[Token]) -> Result<()> {
        self.find_string_arrays(tokens)?;
        self.find_decoders(tokens)?;
        Ok(())
    }

    pub fn deobfuscate(&self, tokens: &mut Vec<Token>) -> Result<()> {
        inline_strings::inline_decoded_strings(tokens, &self.string_arrays, &self.decoders)?;
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
    pub function_name: String,
    pub start_index: usize,
    pub end_index: usize,
    pub array_ref: String,
    pub has_offset: bool,
    pub offset_value: Option<i32>,
}

impl Default for DeobfuscateContext {
    fn default() -> Self {
        Self::new()
    }
}
