use crate::Result;
use crate::token::{Token, TokenType};
use base64::{Engine, engine::general_purpose::STANDARD};

fn decode_hex_escape(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if next == 'x' {
                    chars.next();
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                        continue;
                    }
                } else if next == 'u' {
                    chars.next();
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(unicode_char) = char::from_u32(code) {
                            result.push(unicode_char);
                            continue;
                        }
                    }
                }
            }
        }
        result.push(ch);
    }

    result
}

fn try_decode_string(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if tokens[pos].token_type != TokenType::String {
        return Ok(None);
    }

    let original = &tokens[pos].text;
    if !original.contains("\\x") && !original.contains("\\u") {
        return Ok(None);
    }

    let quote_char = if original.starts_with('"') { '"' } else { '\'' };
    let inner = original.trim_matches(quote_char);
    let decoded = decode_hex_escape(inner);

    if decoded != inner {
        let new_string = format!("{}{}{}", quote_char, decoded, quote_char);
        let token = Token::new(TokenType::String, new_string);
        return Ok(Some((vec![token], 1)));
    }

    Ok(None)
}

fn try_simplify_from_char_code(
    tokens: &[Token],
    pos: usize,
) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 6 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type == TokenType::Word
        && tokens[pos].text == "String"
        && tokens[pos + 1].token_type == TokenType::Dot
        && tokens[pos + 2].text == "fromCharCode"
        && tokens[pos + 3].token_type == TokenType::StartExpr
    {
        let mut codes = Vec::new();
        let mut i = pos + 4;

        while i < tokens.len() {
            if tokens[i].token_type == TokenType::EndExpr {
                if !codes.is_empty() {
                    let result_string: String = codes
                        .iter()
                        .filter_map(|&code| char::from_u32(code))
                        .collect();

                    let token = Token::new(TokenType::String, format!("\"{}\"", result_string));
                    return Ok(Some((vec![token], i - pos + 1)));
                }
                break;
            }

            if tokens[i].token_type == TokenType::Number {
                if let Ok(code) = tokens[i].text.parse::<u32>() {
                    codes.push(code);
                }
            }

            i += 1;
        }
    }

    Ok(None)
}

fn try_simplify_string_concat(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 2 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type == TokenType::String
        && tokens[pos + 1].token_type == TokenType::Operator
        && tokens[pos + 1].text == "+"
        && tokens[pos + 2].token_type == TokenType::String
    {
        let str1 = tokens[pos].text.trim_matches('"').trim_matches('\'');
        let str2 = tokens[pos + 2].text.trim_matches('"').trim_matches('\'');

        let combined = format!("\"{}{}\"", str1, str2);
        let token = Token::new(TokenType::String, combined);

        return Ok(Some((vec![token], 3)));
    }

    Ok(None)
}

fn try_simplify_atob(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    // Pattern: atob("base64string")
    if pos + 5 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type == TokenType::Word
        && tokens[pos].text == "atob"
        && tokens[pos + 1].token_type == TokenType::StartExpr
        && tokens[pos + 2].token_type == TokenType::String
        && tokens[pos + 3].token_type == TokenType::EndExpr
    {
        let base64_str = tokens[pos + 2].text.trim_matches('"').trim_matches('\'');

        if let Ok(decoded_bytes) = STANDARD.decode(base64_str) {
            if let Ok(decoded_str) = String::from_utf8(decoded_bytes) {
                let result_token = Token::new(TokenType::String, format!("\"{}\"", decoded_str));
                return Ok(Some((vec![result_token], 4)));
            }
        }
    }

    Ok(None)
}

fn try_extract_comma_sequence(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if tokens[pos].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let start = pos;
    let mut depth = 0;
    let mut comma_positions = Vec::new();
    let mut i = pos;
    let mut end_pos = None;

    while i < tokens.len() {
        match tokens[i].token_type {
            TokenType::StartExpr | TokenType::StartArray | TokenType::StartBlock => {
                depth += 1;
            }
            TokenType::EndExpr | TokenType::EndArray | TokenType::EndBlock => {
                depth -= 1;
                if depth == 0 {
                    end_pos = Some(i);
                    break;
                }
            }
            TokenType::Comma if depth == 1 => {
                comma_positions.push(i);
            }
            _ => {}
        }
        i += 1;
    }

    if comma_positions.is_empty() || end_pos.is_none() {
        return Ok(None);
    }

    let end_pos = end_pos.unwrap();

    let last_segment_start = if let Some(&last_comma) = comma_positions.last() {
        last_comma + 1
    } else {
        start + 1
    };

    let mut result = Vec::new();
    for j in last_segment_start..end_pos {
        result.push(tokens[j].clone());
    }

    let total_consumed = end_pos - start + 1;

    Ok(Some((result, total_consumed)))
}

fn try_simplify_advanced_boolean(
    tokens: &[Token],
    pos: usize,
) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 1 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type == TokenType::Operator && tokens[pos].text == "!" {
        if pos + 2 < tokens.len() && tokens[pos + 1].text == "!" {
            if tokens[pos + 2].token_type == TokenType::StartArray {
                if pos + 3 < tokens.len() && tokens[pos + 3].token_type == TokenType::EndArray {
                    let token = Token::new(TokenType::Word, "true".to_string());
                    return Ok(Some((vec![token], 4)));
                }
            }

            if tokens[pos + 2].token_type == TokenType::Number && tokens[pos + 2].text == "0" {
                let token = Token::new(TokenType::Word, "false".to_string());
                return Ok(Some((vec![token], 3)));
            }
        }
    }

    if tokens[pos].token_type == TokenType::Operator && tokens[pos].text == "+" {
        if pos + 1 < tokens.len() {
            if tokens[pos + 1].token_type == TokenType::StartArray {
                if pos + 2 < tokens.len() && tokens[pos + 2].token_type == TokenType::EndArray {
                    let token = Token::new(TokenType::Number, "0".to_string());
                    return Ok(Some((vec![token], 3)));
                }
            }

            if tokens[pos + 1].token_type == TokenType::Operator && tokens[pos + 1].text == "!" {
                if pos + 2 < tokens.len() && tokens[pos + 2].token_type == TokenType::StartArray {
                    if pos + 3 < tokens.len() && tokens[pos + 3].token_type == TokenType::EndArray {
                        let token = Token::new(TokenType::Number, "1".to_string());
                        return Ok(Some((vec![token], 4)));
                    }
                }
            }
        }
    }

    Ok(None)
}

fn try_simplify_number(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 1 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type == TokenType::Number && tokens[pos].text == "0" {
        if tokens[pos + 1].token_type == TokenType::Word {
            let word = &tokens[pos + 1].text;

            if word.starts_with('x') || word.starts_with('X') {
                if let Ok(num) = i64::from_str_radix(&word[1..], 16) {
                    let token = Token::new(TokenType::Number, num.to_string());
                    return Ok(Some((vec![token], 2)));
                }
            }

            if word.starts_with('o') || word.starts_with('O') {
                if let Ok(num) = i64::from_str_radix(&word[1..], 8) {
                    let token = Token::new(TokenType::Number, num.to_string());
                    return Ok(Some((vec![token], 2)));
                }
            }

            if word.starts_with('b') || word.starts_with('B') {
                if let Ok(num) = i64::from_str_radix(&word[1..], 2) {
                    let token = Token::new(TokenType::Number, num.to_string());
                    return Ok(Some((vec![token], 2)));
                }
            }
        }
    }

    Ok(None)
}

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
    if let Some(simplified) = try_decode_string(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_from_char_code(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_string_concat(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_atob(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_bracket_property(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_boolean(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_advanced_boolean(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_number(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_simplify_void(tokens, pos)? {
        return Ok(Some(simplified));
    }

    if let Some(simplified) = try_extract_comma_sequence(tokens, pos)? {
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

    #[test]
    fn test_decode_hex_string() {
        let code = r#"var x = "\x48\x65\x6c\x6c\x6f";"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(
            output.contains("Hello"),
            "Should decode hex escape sequences"
        );
    }

    #[test]
    fn test_decode_unicode_string() {
        let code = r#"var x = "\u0048\u0065\u006c\u006c\u006f";"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(
            output.contains("Hello"),
            "Should decode unicode escape sequences"
        );
    }

    #[test]
    fn test_from_char_code() {
        let code = "var x = String.fromCharCode(72, 101, 108, 108, 111);";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(output.contains("Hello"), "Should decode fromCharCode");
    }

    #[test]
    fn test_string_concatenation() {
        let code = r#"var x = "Hel" + "lo";"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(
            output.contains("Hello"),
            "Should combine string concatenation"
        );
    }

    #[test]
    fn test_atob_base64() {
        let code = r#"var x = atob("SGVsbG8=");"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(
            output.contains("Hello"),
            "Should decode base64 atob(), got: {}",
            output
        );
        assert!(
            !output.contains("atob"),
            "Should not contain atob call, got: {}",
            output
        );
        assert!(
            !output.contains("SGVsbG8="),
            "Should not contain base64 string, got: {}",
            output
        );
    }

    #[test]
    fn test_double_negation_array() {
        let code = "var x = !![];";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(output.contains("true"), "Should convert !![] to true");
    }

    #[test]
    fn test_double_negation_zero() {
        let code = "var x = !!0;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(output.contains("false"), "Should convert !!0 to false");
    }

    #[test]
    fn test_array_coercion_zero() {
        let code = "var x = +[];";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(output.contains("0"), "Should convert +[] to 0");
    }

    #[test]
    fn test_array_coercion_one() {
        let code = "var x = +![];";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        assert!(output.contains("1"), "Should convert +![] to 1");
    }

    #[test]
    fn test_hex_number() {
        let code = "var x = 0x48;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("72"),
            "Should convert hex 0x48 to decimal 72, got: {}",
            output
        );
        assert!(
            !output.contains("0x48"),
            "Should not contain hex notation, got: {}",
            output
        );
    }

    #[test]
    fn test_binary_number() {
        let code = "var x = 0b1001000;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("72"),
            "Should convert binary 0b1001000 to decimal 72, got: {}",
            output
        );
        assert!(
            !output.contains("0b1001000"),
            "Should not contain binary notation, got: {}",
            output
        );
    }

    #[test]
    fn test_octal_number() {
        let code = "var x = 0o110;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("72"),
            "Should convert octal 0o110 to decimal 72, got: {}",
            output
        );
        assert!(
            !output.contains("0o110"),
            "Should not contain octal notation, got: {}",
            output
        );
    }

    #[test]
    fn test_comma_sequence_extraction() {
        let code = "var x = (a(), b(), c());";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        eprintln!("\n=== Original Tokens ===");
        for (i, token) in tokens.iter().enumerate() {
            eprintln!("Token {}: {:?} = '{}'", i, token.token_type, token.text);
        }
        eprintln!("=== End Original ===\n");

        let result = simplify_expressions(&tokens).unwrap();

        eprintln!("\n=== Simplified Tokens ===");
        for (i, token) in result.iter().enumerate() {
            eprintln!("Token {}: {:?} = '{}'", i, token.token_type, token.text);
        }
        eprintln!("=== End Simplified ===\n");

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        eprintln!("Output: {}", output);

        assert!(
            output.contains("c()"),
            "Should contain last expression c(), got: {}",
            output
        );
        assert!(
            !output.contains("a()"),
            "Should not contain first expression a(), got: {}",
            output
        );
        assert!(
            !output.contains("b()"),
            "Should not contain middle expression b(), got: {}",
            output
        );

        let open_parens = output.matches('(').count();
        let close_parens = output.matches(')').count();
        eprintln!(
            "Open parens: {}, Close parens: {}",
            open_parens, close_parens
        );

        assert_eq!(
            open_parens, 1,
            "Should have only 1 open paren (from c()), got: {}",
            output
        );
        assert_eq!(
            close_parens, 1,
            "Should have only 1 close paren (from c()), got: {}",
            output
        );
    }

    #[test]
    fn test_comma_sequence_simple() {
        let code = "var x = (1, 2, 3);";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        eprintln!("Simple comma sequence output: {}", output);

        assert!(
            output.contains("3"),
            "Should contain last value 3, got: {}",
            output
        );
        assert!(
            !output.contains("1,"),
            "Should not contain first value with comma, got: {}",
            output
        );
        assert!(
            !output.contains("2,"),
            "Should not contain middle value with comma, got: {}",
            output
        );
    }

    #[test]
    fn test_comma_sequence_nested() {
        let code = "var x = (f(1, 2), g(3, 4));";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_expressions(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        eprintln!("Nested comma sequence output: {}", output);

        assert!(
            output.contains("g(3,4)") || output.contains("g(3, 4)"),
            "Should contain last expression g(3,4), got: {}",
            output
        );
        assert!(
            !output.contains("f("),
            "Should not contain first expression f(), got: {}",
            output
        );
    }
}
