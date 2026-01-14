use super::Beautifier;
use crate::token::{Token, TokenType};

pub(super) trait Helpers {
    fn should_add_space_before_paren(&self) -> bool;
    fn should_add_space_before_word(&self, token: &Token) -> bool;
    fn should_add_newline_before_word(&self, token: &Token) -> bool;
    fn should_add_space_before_string(&self) -> bool;
    fn is_line_starter_keyword(&self, word: &str) -> bool;
}

impl<'a> Helpers for Beautifier<'a> {
    fn should_add_space_before_paren(&self) -> bool {
        let last = &self.current_flags().last_token;

        if last.token_type == TokenType::Reserved {
            matches!(
                last.text.as_str(),
                "if" | "for" | "while" | "switch" | "catch" | "function"
            )
        } else {
            self.options.space_after_anon_function
        }
    }

    fn should_add_space_before_word(&self, token: &Token) -> bool {
        let last = &self.current_flags().last_token;

        if last.token_type == TokenType::Reserved
            || last.token_type == TokenType::Word
            || last.token_type == TokenType::EndExpr
        {
            return true;
        }

        if token.token_type == TokenType::Reserved {
            return true;
        }

        false
    }

    fn should_add_newline_before_word(&self, token: &Token) -> bool {
        if token.token_type != TokenType::Reserved {
            return false;
        }

        let last = &self.current_flags().last_token;

        if last.token_type == TokenType::EndBlock
            && matches!(token.text.as_str(), "else" | "catch" | "finally")
        {
            return false;
        }

        self.is_line_starter_keyword(&token.text)
    }

    fn should_add_space_before_string(&self) -> bool {
        let last = &self.current_flags().last_token;
        !matches!(
            last.token_type,
            TokenType::StartExpr | TokenType::Comma | TokenType::Equals | TokenType::Operator
        )
    }

    fn is_line_starter_keyword(&self, word: &str) -> bool {
        matches!(
            word,
            "break"
                | "continue"
                | "return"
                | "throw"
                | "var"
                | "let"
                | "const"
                | "if"
                | "switch"
                | "case"
                | "default"
                | "for"
                | "while"
                | "do"
                | "function"
                | "class"
                | "import"
                | "export"
                | "try"
        )
    }
}
