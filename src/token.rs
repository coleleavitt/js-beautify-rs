//! Token types and definitions
//!
//! Represents the lexical tokens produced by the JavaScript tokenizer.

/// Token types recognized by the beautifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    /// Opening parenthesis `(`
    StartExpr,
    /// Closing parenthesis `)`
    EndExpr,
    /// Opening brace `{`
    StartBlock,
    /// Closing brace `}`
    EndBlock,
    /// Identifiers and keywords
    Word,
    /// Reserved JavaScript keywords
    Reserved,
    /// Semicolon `;`
    Semicolon,
    /// String literals
    String,
    /// Assignment operator `=`
    Equals,
    /// Operators (+, -, *, /, etc.)
    Operator,
    /// Comma `,`
    Comma,
    /// Block comment `/* */`
    BlockComment,
    /// Line comment `//`
    Comment,
    /// Dot operator `.`
    Dot,
    /// Unknown token
    Unknown,
    /// Start of file marker
    Start,
    /// Raw token (no processing)
    Raw,
    /// End of file
    Eof,
}

impl TokenType {
    /// Returns true if this is a start delimiter
    pub const fn is_start_delimiter(self) -> bool {
        matches!(self, Self::StartExpr | Self::StartBlock)
    }

    /// Returns true if this is an end delimiter
    pub const fn is_end_delimiter(self) -> bool {
        matches!(self, Self::EndExpr | Self::EndBlock)
    }

    /// Returns true if this is a comment
    pub const fn is_comment(self) -> bool {
        matches!(self, Self::Comment | Self::BlockComment)
    }
}

/// A token with its type and text content
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The type of token
    pub token_type: TokenType,
    /// The raw text of the token
    pub text: String,
    /// Line number where token appears (1-indexed)
    pub line: usize,
    /// Column where token starts (0-indexed)
    pub column: usize,
    /// Preceding whitespace
    pub whitespace_before: String,
    /// Whether this token starts a new line
    pub newlines_before: usize,
}

impl Token {
    /// Creates a new token
    pub fn new(token_type: TokenType, text: impl Into<String>) -> Self {
        Self {
            token_type,
            text: text.into(),
            line: 1,
            column: 0,
            whitespace_before: String::new(),
            newlines_before: 0,
        }
    }

    /// Creates a token with position information
    pub fn with_position(
        token_type: TokenType,
        text: impl Into<String>,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            token_type,
            text: text.into(),
            line,
            column,
            whitespace_before: String::new(),
            newlines_before: 0,
        }
    }

    /// Returns true if this is a reserved keyword
    pub fn is_reserved_keyword(&self, keyword: &str) -> bool {
        self.token_type == TokenType::Reserved && self.text == keyword
    }

    /// Returns true if this is a word token with the given text
    pub fn is_word(&self, word: &str) -> bool {
        self.token_type == TokenType::Word && self.text == word
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let token = Token::new(TokenType::Word, "test");
        assert_eq!(token.token_type, TokenType::Word);
        assert_eq!(token.text, "test");
    }

    #[test]
    fn test_token_type_checks() {
        assert!(TokenType::StartExpr.is_start_delimiter());
        assert!(TokenType::EndBlock.is_end_delimiter());
        assert!(TokenType::Comment.is_comment());
        assert!(!TokenType::Word.is_comment());
    }

    #[test]
    fn test_is_reserved_keyword() {
        let token = Token::new(TokenType::Reserved, "function");
        assert!(token.is_reserved_keyword("function"));
        assert!(!token.is_reserved_keyword("if"));
    }
}
