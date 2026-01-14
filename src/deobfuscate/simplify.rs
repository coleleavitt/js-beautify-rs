use crate::Result;
use crate::token::{Token, TokenType};

pub fn simplify_expressions(tokens: &[Token]) -> Result<Vec<Token>> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some((simplified, skip)) = try_simplify_at(tokens, i)? {
            result.extend(simplified);
            i += skip;
        } else {
            result.push(tokens[i].clone());
            i += 1;
        }
    }

    Ok(result)
}

fn try_simplify_at(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if let Some(simplified) = try_simplify_bracket_property(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_boolean(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_void(tokens, pos)? {
        return Ok(Some(simplified));
    }

    Ok(None)
}

fn try_simplify_bracket_property(
    tokens: &[Token],
    pos: usize,
) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type == TokenType::StartArray
        && tokens[pos + 1].token_type == TokenType::String
        && tokens[pos + 2].token_type == TokenType::EndArray
    {
        let property_name = tokens[pos + 1].text.trim_matches('"').trim_matches('\'');

        if is_valid_identifier(property_name) {
            let dot_token = Token::new(TokenType::Dot, ".".to_string());
            let prop_token = Token::new(TokenType::Word, property_name.to_string());

            return Ok(Some((vec![dot_token, prop_token], 3)));
        }
    }

    Ok(None)
}

fn try_simplify_boolean(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 1 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type == TokenType::Operator && tokens[pos].text == "!" {
        if tokens[pos + 1].token_type == TokenType::Number {
            if tokens[pos + 1].text == "0" {
                let true_token = Token::new(TokenType::Word, "true".to_string());
                return Ok(Some((vec![true_token], 2)));
            } else if tokens[pos + 1].text == "1" {
                let false_token = Token::new(TokenType::Word, "false".to_string());
                return Ok(Some((vec![false_token], 2)));
            }
        }
    }

    Ok(None)
}

fn try_simplify_void(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 2 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type == TokenType::Reserved && tokens[pos].text == "void" {
        let undefined_token = Token::new(TokenType::Word, "undefined".to_string());

        let mut skip = 1;
        if pos + 1 < tokens.len() && tokens[pos + 1].token_type == TokenType::Number {
            skip = 2;
        }

        return Ok(Some((vec![undefined_token], skip)));
    }

    Ok(None)
}

fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let first = s.chars().next().unwrap();
    if !first.is_alphabetic() && first != '_' && first != '$' {
        return false;
    }

    s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_simplify_bracket_notation() {
        let code = r#"obj["property"]"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(
            output.contains(".property"),
            "Should convert to dot notation"
        );
        assert!(
            !output.contains("[\"property\"]"),
            "Should not contain bracket notation"
        );
    }

    #[test]
    fn test_simplify_boolean_true() {
        let code = "var x = !0;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(output.contains("true"), "Should convert !0 to true");
    }

    #[test]
    fn test_simplify_boolean_false() {
        let code = "var x = !1;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(output.contains("false"), "Should convert !1 to false");
    }

    #[test]
    fn test_simplify_void() {
        let code = "var x = void 0;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(
            output.contains("undefined"),
            "Should convert void 0 to undefined"
        );
    }

    #[test]
    fn test_preserve_invalid_identifiers() {
        let code = r#"obj["some-property"]"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(
            output.contains("[\"some-property\"]"),
            "Should preserve bracket notation for invalid identifiers"
        );
    }
}
