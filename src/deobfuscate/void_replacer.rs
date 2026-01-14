use crate::Result;
use crate::token::{Token, TokenType};

pub fn replace_void_zero(tokens: &[Token]) -> Result<Vec<Token>> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut i = 0;

    while i < tokens.len() {
        if i + 1 < tokens.len()
            && tokens[i].token_type == TokenType::Reserved
            && tokens[i].text == "void"
            && tokens[i + 1].token_type == TokenType::Number
            && tokens[i + 1].text == "0"
        {
            result.push(Token::with_position(
                TokenType::Reserved,
                "undefined",
                tokens[i].line,
                tokens[i].column,
            ));
            i += 2;
            continue;
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
    fn test_replace_void_zero() {
        let tokens = vec![
            Token::with_position(TokenType::Reserved, "var", 1, 0),
            Token::with_position(TokenType::Word, "x", 1, 4),
            Token::with_position(TokenType::Equals, "=", 1, 6),
            Token::with_position(TokenType::Reserved, "void", 1, 8),
            Token::with_position(TokenType::Number, "0", 1, 13),
        ];

        let result = replace_void_zero(&tokens).unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[3].token_type, TokenType::Reserved);
        assert_eq!(result[3].text, "undefined");
    }

    #[test]
    fn test_preserve_void_with_other_numbers() {
        let tokens = vec![
            Token::with_position(TokenType::Reserved, "void", 1, 0),
            Token::with_position(TokenType::Number, "5", 1, 5),
        ];

        let result = replace_void_zero(&tokens).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "void");
        assert_eq!(result[1].text, "5");
    }

    #[test]
    fn test_multiple_void_zero() {
        let tokens = vec![
            Token::with_position(TokenType::Reserved, "void", 1, 0),
            Token::with_position(TokenType::Number, "0", 1, 5),
            Token::with_position(TokenType::Comma, ",", 1, 6),
            Token::with_position(TokenType::Reserved, "void", 1, 8),
            Token::with_position(TokenType::Number, "0", 1, 13),
        ];

        let result = replace_void_zero(&tokens).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].text, "undefined");
        assert_eq!(result[1].text, ",");
        assert_eq!(result[2].text, "undefined");
    }
}
