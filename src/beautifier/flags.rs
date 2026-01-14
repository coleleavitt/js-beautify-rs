use crate::token::{Token, TokenType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    BlockStatement,
    Statement,
    ObjectLiteral,
    ArrayLiteral,
    ForInitializer,
    Conditional,
    Expression,
}

#[derive(Debug, Clone)]
pub struct Flags {
    pub mode: Mode,
    pub last_token: Token,
    pub last_word: String,
    pub indentation_level: usize,
    pub line_indent_level: usize,
    pub in_case_statement: bool,
    pub in_case: bool,
    pub case_body: bool,
    pub if_block: bool,
    pub else_block: bool,
    pub do_block: bool,
    pub do_while: bool,
    pub ternary_depth: usize,
}

impl Flags {
    pub fn new(mode: Mode, indentation_level: usize) -> Self {
        Self {
            mode,
            last_token: Token::new(TokenType::Start, ""),
            last_word: String::new(),
            indentation_level,
            line_indent_level: indentation_level,
            in_case_statement: false,
            in_case: false,
            case_body: false,
            if_block: false,
            else_block: false,
            do_block: false,
            do_while: false,
            ternary_depth: 0,
        }
    }
}
