//! # js-beautify-rs
//!
//! A Rust port of js-beautify with webpack-specific improvements.
//!
//! ## Example
//!
//! ```rust
//! use js_beautify_rs::{beautify, Options};
//!
//! let code = "function test(){console.log('hello');}";
//! let options = Options::default();
//! let beautified = beautify(code, &options).expect("beautification failed");
//! ```

pub mod beautifier;
pub mod deobfuscate;
pub mod options;
pub mod output;
pub mod token;
pub mod tokenizer;

pub use beautifier::beautify;
pub use deobfuscate::DeobfuscateContext;
pub use options::Options;
pub use token::{Token, TokenType};

#[derive(Debug, thiserror::Error)]
pub enum BeautifyError {
    #[error("tokenization failed: {0}")]
    TokenizationFailed(String),

    #[error("beautification failed: {0}")]
    BeautificationFailed(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),
}

pub type Result<T> = std::result::Result<T, BeautifyError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_beautify() {
        let code = "function test(){return 42;}";
        let options = Options::default();
        let result = beautify(code, &options).expect("beautification failed");

        assert!(result.contains("function test()"));
        assert!(result.contains("return 42;"));
    }
}
