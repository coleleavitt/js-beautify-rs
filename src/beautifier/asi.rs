use crate::token::{Token, TokenType};

/// JavaScript keywords that are "restricted productions" per the spec.
/// If these are followed by a newline, the newline acts as a semicolon.
///
/// Examples:
/// ```text
/// return
/// {a: 1}  // This returns undefined, not the object
///
/// throw
/// new Error()  // SyntaxError: newline not allowed
/// ```
const RESTRICTED_PRODUCTIONS: &[&str] = &["return", "throw", "break", "continue", "yield"];

pub fn is_restricted_production(token: &Token) -> bool {
    token.token_type == TokenType::Reserved && RESTRICTED_PRODUCTIONS.contains(&token.text.as_str())
}

pub fn needs_asi(prev_token: &Token, current_token: &Token) -> bool {
    // Rule 1: Restricted productions followed by newline
    if is_restricted_production(prev_token) && current_token.newlines_before > 0 {
        // Special case: return/break/continue can have labels/expressions on same line
        // But if there's a newline, ASI kicks in
        match prev_token.text.as_str() {
            "return" | "throw" | "yield" => {
                // If next token is on a new line and isn't a semicolon, ASI applies
                current_token.token_type != TokenType::Semicolon
            }
            "break" | "continue" => {
                // break/continue can have an optional label, but newline triggers ASI
                current_token.token_type != TokenType::Semicolon
                    && current_token.token_type != TokenType::Word
            }
            _ => false,
        }
    } else {
        false
    }
}

pub fn needs_asi_for_postfix(prev_token: &Token, current_token: &Token) -> bool {
    if current_token.newlines_before > 0
        && current_token.token_type == TokenType::Operator
        && matches!(current_token.text.as_str(), "++" | "--")
    {
        // If previous token could be postfix-incremented and there's a newline,
        // treat the ++ as prefix for next statement
        matches!(
            prev_token.token_type,
            TokenType::Word | TokenType::EndExpr | TokenType::EndArray
        )
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_restricted_production() {
        let return_token = Token::new(TokenType::Reserved, "return");
        assert!(is_restricted_production(&return_token));

        let throw_token = Token::new(TokenType::Reserved, "throw");
        assert!(is_restricted_production(&throw_token));

        let if_token = Token::new(TokenType::Reserved, "if");
        assert!(!is_restricted_production(&if_token));
    }

    #[test]
    fn test_needs_asi_return_with_newline() {
        let return_token = Token::new(TokenType::Reserved, "return");
        let mut brace_token = Token::new(TokenType::StartBlock, "{");
        brace_token.newlines_before = 1;

        assert!(needs_asi(&return_token, &brace_token));
    }

    #[test]
    fn test_needs_asi_return_same_line() {
        let return_token = Token::new(TokenType::Reserved, "return");
        let brace_token = Token::new(TokenType::StartBlock, "{");

        assert!(!needs_asi(&return_token, &brace_token));
    }

    #[test]
    fn test_needs_asi_for_postfix() {
        let mut word_token = Token::new(TokenType::Word, "a");
        let mut inc_token = Token::new(TokenType::Operator, "++");
        inc_token.newlines_before = 1;

        assert!(needs_asi_for_postfix(&word_token, &inc_token));

        let inc_same_line = Token::new(TokenType::Operator, "++");
        assert!(!needs_asi_for_postfix(&word_token, &inc_same_line));
    }
}
