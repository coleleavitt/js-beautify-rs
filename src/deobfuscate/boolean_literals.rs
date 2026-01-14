use crate::Result;
use crate::token::{Token, TokenType};

pub fn replace_boolean_literals(tokens: &[Token]) -> Result<Vec<Token>> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut i = 0;

    while i < tokens.len() {
        if i + 1 < tokens.len()
            && tokens[i].token_type == TokenType::Operator
            && tokens[i].text == "!"
            && tokens[i + 1].token_type == TokenType::Number
        {
            if tokens[i + 1].text == "0" {
                result.push(Token::with_newlines(
                    TokenType::Reserved,
                    "true",
                    tokens[i].line,
                    tokens[i].column,
                    tokens[i].newlines_before,
                ));
                i += 2;
                continue;
            } else if tokens[i + 1].text == "1" {
                result.push(Token::with_newlines(
                    TokenType::Reserved,
                    "false",
                    tokens[i].line,
                    tokens[i].column,
                    tokens[i].newlines_before,
                ));
                i += 2;
                continue;
            }
        }

        result.push(tokens[i].clone());
        i += 1;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_not_zero() {
        let tokens = vec![
            Token::with_position(TokenType::Reserved, "var", 1, 0),
            Token::with_position(TokenType::Word, "x", 1, 4),
            Token::with_position(TokenType::Equals, "=", 1, 6),
            Token::with_position(TokenType::Operator, "!", 1, 8),
            Token::with_position(TokenType::Number, "0", 1, 9),
        ];

        let result = replace_boolean_literals(&tokens).unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[3].token_type, TokenType::Reserved);
        assert_eq!(result[3].text, "true");
    }

    #[test]
    fn test_replace_not_one() {
        let tokens = vec![
            Token::with_position(TokenType::Reserved, "var", 1, 0),
            Token::with_position(TokenType::Word, "y", 1, 4),
            Token::with_position(TokenType::Equals, "=", 1, 6),
            Token::with_position(TokenType::Operator, "!", 1, 8),
            Token::with_position(TokenType::Number, "1", 1, 9),
        ];

        let result = replace_boolean_literals(&tokens).unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[3].token_type, TokenType::Reserved);
        assert_eq!(result[3].text, "false");
    }

    #[test]
    fn test_preserve_other_numbers() {
        let tokens = vec![
            Token::with_position(TokenType::Operator, "!", 1, 0),
            Token::with_position(TokenType::Number, "5", 1, 1),
        ];

        let result = replace_boolean_literals(&tokens).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "!");
        assert_eq!(result[1].text, "5");
    }
}
