use crate::token::{Token, TokenType};
use crate::{BeautifyError, Result};

pub struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
    line: usize,
    column: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
            column: 0,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();

        while self.pos < self.input.len() {
            self.skip_whitespace();

            if self.pos >= self.input.len() {
                break;
            }

            if let Some(token) = self.next_token()? {
                tokens.push(token);
            }
        }

        tokens.push(Token::with_position(
            TokenType::Eof,
            "",
            self.line,
            self.column,
        ));

        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Option<Token>> {
        let start_pos = self.pos;
        let start_column = self.column;
        let ch = self.current_char();

        let token_type = match ch {
            '(' => TokenType::StartExpr,
            ')' => TokenType::EndExpr,
            '{' => TokenType::StartBlock,
            '}' => TokenType::EndBlock,
            ';' => TokenType::Semicolon,
            ',' => TokenType::Comma,
            '.' => TokenType::Dot,
            '=' => TokenType::Equals,
            _ if ch.is_alphabetic() || ch == '_' || ch == '$' => {
                return self.read_word();
            }
            _ if ch == '"' || ch == '\'' => {
                return self.read_string(ch);
            }
            _ if ch == '/' && self.peek_char() == Some('/') => {
                return self.read_line_comment();
            }
            _ if ch == '/' && self.peek_char() == Some('*') => {
                return self.read_block_comment();
            }
            _ => TokenType::Unknown,
        };

        self.advance();

        Ok(Some(Token::with_position(
            token_type,
            &self.input[start_pos..self.pos],
            self.line,
            start_column,
        )))
    }

    fn read_word(&mut self) -> Result<Option<Token>> {
        let start_pos = self.pos;
        let start_column = self.column;

        while self.pos < self.input.len() {
            let ch = self.current_char();
            if ch.is_alphanumeric() || ch == '_' || ch == '$' {
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.input[start_pos..self.pos];
        let token_type = if Self::is_reserved_word(text) {
            TokenType::Reserved
        } else {
            TokenType::Word
        };

        Ok(Some(Token::with_position(
            token_type,
            text,
            self.line,
            start_column,
        )))
    }

    fn read_string(&mut self, quote: char) -> Result<Option<Token>> {
        let start_pos = self.pos;
        let start_column = self.column;
        self.advance();

        while self.pos < self.input.len() {
            let ch = self.current_char();
            if ch == quote {
                self.advance();
                break;
            }
            if ch == '\\' {
                self.advance();
                if self.pos < self.input.len() {
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        Ok(Some(Token::with_position(
            TokenType::String,
            &self.input[start_pos..self.pos],
            self.line,
            start_column,
        )))
    }

    fn read_line_comment(&mut self) -> Result<Option<Token>> {
        let start_pos = self.pos;
        let start_column = self.column;

        while self.pos < self.input.len() && self.current_char() != '\n' {
            self.advance();
        }

        Ok(Some(Token::with_position(
            TokenType::Comment,
            &self.input[start_pos..self.pos],
            self.line,
            start_column,
        )))
    }

    fn read_block_comment(&mut self) -> Result<Option<Token>> {
        let start_pos = self.pos;
        let start_column = self.column;
        self.advance();
        self.advance();

        while self.pos < self.input.len() - 1 {
            if self.current_char() == '*' && self.peek_char() == Some('/') {
                self.advance();
                self.advance();
                break;
            }
            self.advance();
        }

        Ok(Some(Token::with_position(
            TokenType::BlockComment,
            &self.input[start_pos..self.pos],
            self.line,
            start_column,
        )))
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.current_char();
            if ch.is_whitespace() {
                if ch == '\n' {
                    self.line += 1;
                    self.column = 0;
                }
                self.advance();
            } else {
                break;
            }
        }
    }

    fn current_char(&self) -> char {
        self.input.chars().nth(self.pos).unwrap_or('\0')
    }

    fn peek_char(&self) -> Option<char> {
        self.input.chars().nth(self.pos + 1)
    }

    fn advance(&mut self) {
        self.pos += 1;
        self.column += 1;
    }

    fn is_reserved_word(word: &str) -> bool {
        matches!(
            word,
            "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "debugger"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "export"
                | "extends"
                | "finally"
                | "for"
                | "function"
                | "if"
                | "import"
                | "in"
                | "instanceof"
                | "let"
                | "new"
                | "return"
                | "static"
                | "super"
                | "switch"
                | "this"
                | "throw"
                | "try"
                | "typeof"
                | "var"
                | "void"
                | "while"
                | "with"
                | "yield"
                | "async"
                | "await"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_function() {
        let code = "function test() { return 42; }";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        assert_eq!(tokens[0].token_type, TokenType::Reserved);
        assert_eq!(tokens[0].text, "function");
        assert_eq!(tokens[1].token_type, TokenType::Word);
        assert_eq!(tokens[1].text, "test");
    }
}
