#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    StartExpr,
    EndExpr,
    StartBlock,
    EndBlock,
    StartArray,
    EndArray,
    Word,
    Reserved,
    Semicolon,
    String,
    Number,
    Equals,
    Operator,
    Comma,
    BlockComment,
    Comment,
    Dot,
    Colon,
    QuestionMark,
    TemplateLiteral,
    Unknown,
    Start,
    Raw,
    Eof,
}

impl TokenType {
    pub const fn is_start_delimiter(self) -> bool {
        matches!(self, Self::StartExpr | Self::StartBlock | Self::StartArray)
    }

    pub const fn is_end_delimiter(self) -> bool {
        matches!(self, Self::EndExpr | Self::EndBlock | Self::EndArray)
    }

    pub const fn is_comment(self) -> bool {
        matches!(self, Self::Comment | Self::BlockComment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub token_type: TokenType,
    pub text: String,
    pub line: usize,
    pub column: usize,
    pub whitespace_before: String,
    pub newlines_before: usize,
}

impl Token {
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

    pub fn with_newlines(
        token_type: TokenType,
        text: impl Into<String>,
        line: usize,
        column: usize,
        newlines_before: usize,
    ) -> Self {
        Self {
            token_type,
            text: text.into(),
            line,
            column,
            whitespace_before: String::new(),
            newlines_before,
        }
    }

    pub fn is_reserved_keyword(&self, keyword: &str) -> bool {
        self.token_type == TokenType::Reserved && self.text == keyword
    }

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
