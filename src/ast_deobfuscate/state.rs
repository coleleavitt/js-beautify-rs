use rustc_hash::FxHashMap;

pub struct DeobfuscateState {
    pub changed: bool,
    pub dispatchers: FxHashMap<String, DispatcherInfo>,
    pub string_arrays: FxHashMap<String, StringArrayInfo>,
    pub decoders: FxHashMap<String, DecoderInfo>,
}

impl DeobfuscateState {
    pub fn new() -> Self {
        Self {
            changed: false,
            dispatchers: FxHashMap::default(),
            string_arrays: FxHashMap::default(),
            decoders: FxHashMap::default(),
        }
    }
}

impl Default for DeobfuscateState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct DispatcherInfo {
    pub var_name: String,
    pub functions: FxHashMap<String, FunctionInfo>,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub key: String,
    pub return_value: Option<ReturnValue>,
}

#[derive(Debug, Clone)]
pub enum ReturnValue {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
    Identifier(String),
}

#[derive(Debug, Clone)]
pub struct StringArrayInfo {
    pub var_name: String,
    pub strings: Vec<String>,
    pub rotated: bool,
    pub rotation_count: usize,
}

#[derive(Debug, Clone)]
pub struct DecoderInfo {
    pub function_name: String,
    pub array_name: String,
    pub offset: i32,
    pub offset_operation: OffsetOperation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OffsetOperation {
    None,
    Subtract,
    Add,
}
