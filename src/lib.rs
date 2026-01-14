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
pub mod chunk_detector;
pub mod chunk_splitter;
pub mod deobfuscate;
pub mod options;
pub mod output;
pub mod sourcemap;
pub mod token;
pub mod tokenizer;

pub use beautifier::beautify;
pub use chunk_detector::{ChunkDetector, ChunkMetadata};
pub use chunk_splitter::{ChunkManifest, ChunkSplitter};
pub use deobfuscate::DeobfuscateContext;
pub use options::Options;
pub use token::{Token, TokenType};

#[derive(Debug, thiserror::Error)]
pub enum BeautifyError {
    #[error("tokenization failed at line {line}, column {column}: {message}")]
    TokenizationFailed {
        message: String,
        line: usize,
        column: usize,
    },

    #[error("beautification failed: {0}")]
    BeautificationFailed(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("chunk detection failed: {0}")]
    ChunkDetectionFailed(#[from] chunk_detector::ChunkDetectorError),

    #[error("chunk splitting failed: {0}")]
    ChunkSplittingFailed(#[from] chunk_splitter::ChunkSplitterError),
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
