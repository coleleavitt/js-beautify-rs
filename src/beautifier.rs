use crate::options::Options;
use crate::output::Output;
use crate::token::{Token, TokenType};
use crate::tokenizer::Tokenizer;
use crate::{BeautifyError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    BlockStatement,
    Statement,
    ObjectLiteral,
    ArrayLiteral,
    ForInitializer,
    Conditional,
    Expression,
}

pub struct Beautifier<'a> {
    tokens: Vec<Token>,
    current_index: usize,
    output: Output,
    options: &'a Options,
    mode_stack: Vec<Mode>,
}

impl<'a> Beautifier<'a> {
    fn new(tokens: Vec<Token>, options: &'a Options) -> Self {
        let indent_str = if options.indent_with_tabs {
            "\t".to_string()
        } else {
            options.indent_char.repeat(options.indent_size)
        };

        Self {
            tokens,
            current_index: 0,
            output: Output::new(indent_str),
            options,
            mode_stack: vec![Mode::BlockStatement],
        }
    }

    fn beautify_tokens(&mut self) -> Result<String> {
        while self.current_index < self.tokens.len() {
            let token = &self.tokens[self.current_index].clone();

            if token.token_type == TokenType::Eof {
                break;
            }

            self.handle_token(token)?;
            self.current_index += 1;
        }

        Ok(self.output.to_string())
    }

    fn handle_token(&mut self, token: &Token) -> Result<()> {
        match token.token_type {
            TokenType::StartBlock => {
                self.output.add_token(" {");
                self.output.add_newline();
                self.output.add_indent();
                self.mode_stack.push(Mode::BlockStatement);
            }
            TokenType::EndBlock => {
                self.output.remove_indent();
                self.output.add_newline();
                self.output.add_token(&self.output.get_indent());
                self.output.add_token("}");
                self.mode_stack.pop();
            }
            TokenType::Semicolon => {
                self.output.add_token(";");
                self.output.add_newline();
            }
            TokenType::Comment | TokenType::BlockComment => {
                self.output.add_token(&token.text);
                self.output.add_newline();
            }
            _ => {
                if self.should_add_space_before(token) {
                    self.output.add_token(" ");
                }
                self.output.add_token(&token.text);
            }
        }

        Ok(())
    }

    fn should_add_space_before(&self, _token: &Token) -> bool {
        false
    }
}

pub fn beautify(code: &str, options: &Options) -> Result<String> {
    let mut tokenizer = Tokenizer::new(code);
    let tokens = tokenizer.tokenize()?;

    let mut beautifier = Beautifier::new(tokens, options);
    beautifier.beautify_tokens()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beautify_simple() {
        let code = "function test(){return 42;}";
        let options = Options::default();
        let result = beautify(code, &options).unwrap();
        assert!(result.contains("function"));
        assert!(result.contains("return"));
    }
}
