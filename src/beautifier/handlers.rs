use super::helpers::Helpers;
use super::webpack::WebpackDetector;
use super::{Beautifier, Mode};
use crate::Result;
use crate::token::{Token, TokenType};

pub(super) trait Handlers {
    fn handle_token(&mut self, token: &Token) -> Result<()>;
    fn handle_start_expr(&mut self, token: &Token) -> Result<()>;
    fn handle_end_expr(&mut self, token: &Token) -> Result<()>;
    fn handle_start_block(&mut self, token: &Token) -> Result<()>;
    fn handle_end_block(&mut self, token: &Token) -> Result<()>;
    fn handle_word(&mut self, token: &Token) -> Result<()>;
    fn handle_number(&mut self, token: &Token) -> Result<()>;
    fn handle_semicolon(&mut self, token: &Token) -> Result<()>;
    fn handle_string(&mut self, token: &Token) -> Result<()>;
    fn handle_equals(&mut self, token: &Token) -> Result<()>;
    fn handle_operator(&mut self, token: &Token) -> Result<()>;
    fn handle_comma(&mut self, token: &Token) -> Result<()>;
    fn handle_colon(&mut self, token: &Token) -> Result<()>;
    fn handle_question_mark(&mut self, token: &Token) -> Result<()>;
    fn handle_dot(&mut self, token: &Token) -> Result<()>;
    fn handle_comment(&mut self, token: &Token) -> Result<()>;
    fn handle_block_comment(&mut self, token: &Token) -> Result<()>;
    fn handle_template_literal(&mut self, token: &Token) -> Result<()>;
    fn handle_unknown(&mut self, token: &Token) -> Result<()>;
}

impl<'a> Handlers for Beautifier<'a> {
    fn handle_token(&mut self, token: &Token) -> Result<()> {
        if self.is_webpack_module_start() && self.options.add_webpack_module_separators {
            self.output.add_newline();
            self.output.add_newline();
            let separator = format!("// {}", "=".repeat(60));
            self.output.add_token(&separator);
            self.output.add_newline();
        }

        match token.token_type {
            TokenType::StartExpr => self.handle_start_expr(token),
            TokenType::EndExpr => self.handle_end_expr(token),
            TokenType::StartBlock => self.handle_start_block(token),
            TokenType::EndBlock => self.handle_end_block(token),
            TokenType::Word | TokenType::Reserved => self.handle_word(token),
            TokenType::Number => self.handle_number(token),
            TokenType::Semicolon => self.handle_semicolon(token),
            TokenType::String => self.handle_string(token),
            TokenType::Equals => self.handle_equals(token),
            TokenType::Operator => self.handle_operator(token),
            TokenType::Comma => self.handle_comma(token),
            TokenType::Colon => self.handle_colon(token),
            TokenType::QuestionMark => self.handle_question_mark(token),
            TokenType::Dot => self.handle_dot(token),
            TokenType::Comment => self.handle_comment(token),
            TokenType::BlockComment => self.handle_block_comment(token),
            TokenType::TemplateLiteral => self.handle_template_literal(token),
            _ => self.handle_unknown(token),
        }
    }

    fn handle_start_expr(&mut self, token: &Token) -> Result<()> {
        if self.should_add_space_before_paren() {
            self.output.add_space();
        }

        self.output.add_token(&token.text);
        self.push_mode(Mode::Expression);
        Ok(())
    }

    fn handle_end_expr(&mut self, _token: &Token) -> Result<()> {
        self.output.add_token(")");
        self.pop_mode();
        Ok(())
    }

    fn handle_start_block(&mut self, _token: &Token) -> Result<()> {
        self.output.add_space();
        self.output.add_token("{");
        self.output.add_newline();
        self.push_mode(Mode::BlockStatement);
        Ok(())
    }

    fn handle_end_block(&mut self, _token: &Token) -> Result<()> {
        self.pop_mode();
        self.output.add_newline();
        self.output.add_token("}");
        Ok(())
    }

    fn handle_word(&mut self, token: &Token) -> Result<()> {
        if self.should_add_space_before_word(token) {
            self.output.add_space();
        }

        if self.should_add_newline_before_word(token) {
            self.output.add_newline();
        }

        self.output.add_token(&token.text);
        Ok(())
    }

    fn handle_number(&mut self, token: &Token) -> Result<()> {
        let last = &self.current_flags().last_token;

        if last.token_type == TokenType::Reserved || last.token_type == TokenType::Word {
            self.output.add_space();
        }

        self.output.add_token(&token.text);
        Ok(())
    }

    fn handle_semicolon(&mut self, _token: &Token) -> Result<()> {
        self.output.add_token(";");
        self.output.add_newline();
        Ok(())
    }

    fn handle_string(&mut self, token: &Token) -> Result<()> {
        if self.should_extract_large_asset(&token.text) {
            let placeholder = format!("__WEBPACK_LARGE_ASSET_{}_extracted__", self.current_index);
            self.output.add_token(&placeholder);
        } else {
            if self.should_add_space_before_string() {
                self.output.add_space();
            }
            self.output.add_token(&token.text);
        }
        Ok(())
    }

    fn handle_equals(&mut self, _token: &Token) -> Result<()> {
        self.output.add_space();
        self.output.add_token("=");
        self.output.add_space();
        Ok(())
    }

    fn handle_operator(&mut self, token: &Token) -> Result<()> {
        let last_type = self.current_flags().last_token.token_type;
        let last_text = &self.current_flags().last_token.text;

        let is_unary = matches!(token.text.as_str(), "!" | "~" | "+" | "-")
            && !matches!(
                last_type,
                TokenType::Number | TokenType::Word | TokenType::EndExpr | TokenType::EndArray
            );

        let prev_was_unary =
            last_type == TokenType::Operator && matches!(last_text.as_str(), "!" | "~" | "+" | "-");

        if token.text == "=>" {
            self.output.add_space();
            self.output.add_token(&token.text);
            self.output.add_space();
        } else if is_unary || prev_was_unary {
            if !prev_was_unary
                && last_type != TokenType::StartExpr
                && last_type != TokenType::Comma
                && last_type != TokenType::Equals
            {
                self.output.add_space();
            }
            self.output.add_token(&token.text);
        } else {
            self.output.add_space();
            self.output.add_token(&token.text);
            self.output.add_space();
        }
        Ok(())
    }

    fn handle_comma(&mut self, _token: &Token) -> Result<()> {
        self.output.add_token(",");

        if self.options.break_webpack_imports && self.is_webpack_require_chain() {
            self.output.add_newline();
        } else {
            self.output.add_space();
        }
        Ok(())
    }

    fn handle_colon(&mut self, _token: &Token) -> Result<()> {
        let in_ternary = self.current_flags().ternary_depth > 0;

        if in_ternary {
            self.current_flags_mut().ternary_depth -= 1;

            if self
                .output
                .line_exceeds_length(self.options.max_line_length)
            {
                self.output.add_newline();
                self.output.add_token(":");
                self.output.add_space();
            } else {
                self.output.add_token(":");
                self.output.add_space();
            }
        } else {
            self.output.add_token(":");
            self.output.add_space();
        }
        Ok(())
    }

    fn handle_question_mark(&mut self, _token: &Token) -> Result<()> {
        self.current_flags_mut().ternary_depth += 1;

        if self
            .output
            .line_exceeds_length(self.options.max_line_length)
        {
            self.output.add_newline();
            self.output.add_token("?");
            self.output.add_space();
        } else {
            self.output.add_space();
            self.output.add_token("?");
            self.output.add_space();
        }
        Ok(())
    }

    fn handle_dot(&mut self, _token: &Token) -> Result<()> {
        self.output.add_token(".");
        Ok(())
    }

    fn handle_comment(&mut self, token: &Token) -> Result<()> {
        self.output.add_newline();
        self.output.add_token(&token.text);
        self.output.add_newline();
        Ok(())
    }

    fn handle_block_comment(&mut self, token: &Token) -> Result<()> {
        self.output.add_newline();
        self.output.add_token(&token.text);
        self.output.add_newline();
        Ok(())
    }

    fn handle_template_literal(&mut self, token: &Token) -> Result<()> {
        if self.should_add_space_before_string() {
            self.output.add_space();
        }
        self.output.add_template_literal(&token.text);
        Ok(())
    }

    fn handle_unknown(&mut self, token: &Token) -> Result<()> {
        self.output.add_token(&token.text);
        Ok(())
    }
}
