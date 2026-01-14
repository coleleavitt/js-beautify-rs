use std::default::Default;

#[derive(Debug, Clone)]
pub struct Options {
    pub indent_size: usize,
    pub indent_char: String,
    pub indent_with_tabs: bool,
    pub eol: String,
    pub preserve_newlines: bool,
    pub max_preserve_newlines: usize,
    pub space_after_anon_function: bool,
    pub brace_style: BraceStyle,
    pub break_webpack_imports: bool,
    pub add_webpack_module_separators: bool,
    pub extract_large_assets: bool,
    pub asset_size_threshold: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceStyle {
    Collapse,
    Expand,
    EndExpand,
    None,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            indent_size: 4,
            indent_char: " ".to_string(),
            indent_with_tabs: false,
            eol: "\n".to_string(),
            preserve_newlines: true,
            max_preserve_newlines: 2,
            space_after_anon_function: false,
            brace_style: BraceStyle::Collapse,
            break_webpack_imports: true,
            add_webpack_module_separators: true,
            extract_large_assets: true,
            asset_size_threshold: 10_000,
        }
    }
}
