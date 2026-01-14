use crate::Result;
use crate::options::Options;
use crate::output::Output;
use crate::token::Token;
use crate::tokenizer::Tokenizer;

mod flags;
mod handlers;
mod helpers;
mod webpack;

pub use flags::{Flags, Mode};
use handlers::Handlers;

pub struct Beautifier<'a> {
    tokens: Vec<Token>,
    current_index: usize,
    output: Output,
    options: &'a Options,
    flag_stack: Vec<Flags>,
    last_last_text: String,
}

impl<'a> Beautifier<'a> {
    fn new(tokens: Vec<Token>, options: &'a Options) -> Self {
        let indent_str = if options.indent_with_tabs {
            "\t".to_string()
        } else {
            options.indent_char.repeat(options.indent_size)
        };

        let initial_flags = Flags::new(Mode::BlockStatement, 0);

        Self {
            tokens,
            current_index: 0,
            output: Output::new(indent_str),
            options,
            flag_stack: vec![initial_flags],
            last_last_text: String::new(),
        }
    }

    pub(crate) fn current_flags(&self) -> &Flags {
        self.flag_stack.last().unwrap()
    }

    pub(crate) fn current_flags_mut(&mut self) -> &mut Flags {
        self.flag_stack.last_mut().unwrap()
    }

    pub(crate) fn push_mode(&mut self, mode: Mode) {
        let indent_level = self.current_flags().indentation_level + 1;
        self.flag_stack.push(Flags::new(mode, indent_level));
        self.output.add_indent();
    }

    pub(crate) fn pop_mode(&mut self) {
        if self.flag_stack.len() > 1 {
            self.flag_stack.pop();
            self.output.remove_indent();
        }
    }

    fn beautify_tokens(&mut self) -> Result<String> {
        while self.current_index < self.tokens.len() {
            let token = self.tokens[self.current_index].clone();

            if token.token_type == crate::token::TokenType::Eof {
                break;
            }

            self.handle_token(&token)?;

            self.last_last_text = self.current_flags().last_token.text.clone();
            self.current_flags_mut().last_token = token.clone();

            if token.token_type == crate::token::TokenType::Reserved
                || token.token_type == crate::token::TokenType::Word
            {
                self.current_flags_mut().last_word = token.text.clone();
            }

            self.current_index += 1;
        }

        Ok(self.output.to_string())
    }
}

pub fn beautify(code: &str, options: &Options) -> Result<String> {
    let mut tokenizer = Tokenizer::new(code);
    let tokens = tokenizer.tokenize()?;

    let mut beautifier = Beautifier::new(tokens, options);
    beautifier.beautify_tokens()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod basic_tests {
    use super::*;

    #[test]
    fn test_beautify_simple() {
        let code = "function test(){return 42;}";
        let options = Options::default();
        let result = beautify(code, &options).unwrap();
        assert!(result.contains("function"));
        assert!(result.contains("return"));
    }

    #[test]
    fn test_beautify_with_spaces() {
        let code = "function test(){return 42;}";
        let options = Options::default();
        let result = beautify(code, &options).unwrap();
        assert!(result.contains("function test"));
    }
}
