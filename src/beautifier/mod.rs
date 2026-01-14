use crate::chunk_detector::ChunkDetector;
use crate::chunk_splitter::ChunkSplitter;
use crate::options::Options;
use crate::output::Output;
use crate::token::Token;
use crate::tokenizer::Tokenizer;
use crate::{BeautifyError, Result};

mod asi;
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

            let prev_token = &self.current_flags().last_token;
            if asi::needs_asi(prev_token, &token) {
                self.output.add_token(";");
                self.output.add_newline();
            } else if asi::needs_asi_for_postfix(prev_token, &token) {
                self.output.add_token(";");
                self.output.add_newline();
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
    let mut tokens = tokenizer.tokenize()?;

    if options.deobfuscate {
        let mut ctx = crate::deobfuscate::DeobfuscateContext::new();
        ctx.analyze(&tokens)?;
        ctx.deobfuscate(&mut tokens)?;
    }

    if options.split_chunks {
        eprintln!("[BEAUTIFY] Chunk splitting enabled, detecting chunks...");

        let mut detector = ChunkDetector::new();
        match detector.detect_chunks(&tokens) {
            Ok(()) => {
                eprintln!("[BEAUTIFY] ✓ Detected {} chunks", detector.chunk_count());

                if detector.chunk_count() > 0 {
                    if detector.has_boundaries() {
                        let splitter = ChunkSplitter::new(detector);

                        eprintln!(
                            "[BEAUTIFY] Splitting chunks to: {}",
                            options.chunk_dir.display()
                        );

                        let manifest = splitter.split_and_write(&tokens, options)?;

                        eprintln!(
                            "[BEAUTIFY] ✓ Successfully wrote {} chunk files",
                            manifest.total_chunks
                        );

                        if let Some(ref map_path) = options.chunk_map_output {
                            eprintln!("[BEAUTIFY] ✓ Chunk map written to: {}", map_path.display());
                        }

                        return Ok(format!(
                            "// Chunks written to: {}\n// Total chunks: {}\n",
                            options.chunk_dir.display(),
                            manifest.total_chunks
                        ));
                    } else {
                        eprintln!("[BEAUTIFY] ⚠ Chunks detected but no embedded code found");
                        eprintln!(
                            "[BEAUTIFY]   This appears to be a code-split bundle (chunks are already separate files)"
                        );
                        eprintln!(
                            "[BEAUTIFY]   Chunk metadata extracted: {} chunks",
                            detector.chunk_count()
                        );

                        if let Some(ref map_path) = options.chunk_map_output {
                            use crate::chunk_splitter::ChunkManifest;
                            use std::fs;

                            let manifest = ChunkManifest::from_detector(&detector);
                            let json = serde_json::to_string_pretty(&manifest).map_err(|e| {
                                BeautifyError::BeautificationFailed(format!(
                                    "failed to serialize manifest: {}",
                                    e
                                ))
                            })?;
                            fs::write(map_path, json).map_err(|e| {
                                BeautifyError::BeautificationFailed(format!(
                                    "failed to write manifest: {}",
                                    e
                                ))
                            })?;

                            eprintln!(
                                "[BEAUTIFY] ✓ Chunk metadata written to: {}",
                                map_path.display()
                            );
                        }

                        eprintln!("[BEAUTIFY] Proceeding with normal beautification");
                    }
                } else {
                    eprintln!(
                        "[BEAUTIFY] ⚠ No chunks detected, proceeding with normal beautification"
                    );
                }
            }
            Err(e) => {
                eprintln!("[BEAUTIFY] ⚠ Chunk detection failed: {}", e);
                eprintln!("[BEAUTIFY] Falling back to normal beautification");
            }
        }
    }

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
