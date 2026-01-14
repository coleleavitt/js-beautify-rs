use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};

const MAX_PROPERTY_LENGTH: usize = 100;
const MAX_CONCAT_PARTS: usize = 50;

#[cfg(debug_assertions)]
macro_rules! trace_prop {
    ($($arg:tt)*) => {
        eprintln!("[PROP] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_prop {
    ($($arg:tt)*) => {};
}

pub fn convert_dynamic_properties(tokens: &[Token]) -> Result<Vec<Token>> {
    trace_prop!("=== CONVERTING DYNAMIC PROPERTIES ===");
    trace_prop!("total tokens: {}", tokens.len());

    let mut result = Vec::new();
    let mut i = 0usize;
    let mut converted_count = 0usize;

    while i < tokens.len() {
        if let Some((converted, skip)) = try_convert_property(tokens, i)? {
            debug_assert!(skip > 0, "Skip must be positive");
            debug_assert!(skip < 1000, "Skip suspiciously large: {}", skip);

            result.extend(converted);
            i = i
                .checked_add(skip)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

            converted_count = converted_count
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("count overflow".to_string()))?;

            continue;
        }

        result.push(tokens[i].clone());
        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    debug_assert!(
        result.len() <= tokens.len(),
        "Result should not exceed input"
    );

    trace_prop!("converted {} dynamic properties", converted_count);
    trace_prop!("final token count: {} -> {}", tokens.len(), result.len());

    Ok(result)
}

fn try_convert_property(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    let check_pos = pos
        .checked_add(3)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if check_pos >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type != TokenType::StartArray {
        return Ok(None);
    }

    trace_prop!(
        "try_convert_property at pos={}, next token: {:?} = '{}'",
        pos,
        tokens[pos + 1].token_type,
        tokens[pos + 1].text
    );

    if let Some(converted) = try_convert_string_literal(tokens, pos)? {
        trace_prop!("  -> converted via string_literal");
        return Ok(Some(converted));
    }

    if let Some(converted) = try_convert_hex_literal(tokens, pos)? {
        trace_prop!("  -> converted via hex_literal");
        return Ok(Some(converted));
    }

    if let Some(converted) = try_convert_string_concat(tokens, pos)? {
        trace_prop!("  -> converted via string_concat");
        return Ok(Some(converted));
    }

    trace_prop!("  -> no conversion applied");
    Ok(None)
}

fn try_convert_string_literal(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    debug_assert!(pos < tokens.len(), "Position out of bounds");
    debug_assert!(
        tokens[pos].token_type == TokenType::StartArray,
        "Must start at ["
    );

    let check_pos = pos
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if check_pos >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos + 1].token_type != TokenType::String {
        return Ok(None);
    }

    if tokens[pos + 2].token_type != TokenType::EndArray {
        return Ok(None);
    }

    let string_value = &tokens[pos + 1].text;
    let property_name = string_value.trim_matches('"').trim_matches('\'');

    debug_assert!(
        property_name.len() <= MAX_PROPERTY_LENGTH,
        "Property name too long: {}",
        property_name.len()
    );

    if !is_valid_identifier(property_name) {
        trace_prop!("  -> '{}' is not a valid identifier", property_name);
        return Ok(None);
    }

    trace_prop!("converted [\"{}\" ] -> .{}", property_name, property_name);

    let dot_token = Token::new(TokenType::Dot, ".".to_string());
    let prop_token = Token::new(TokenType::Word, property_name.to_string());

    Ok(Some((vec![dot_token, prop_token], 3)))
}

fn try_convert_hex_literal(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    debug_assert!(pos < tokens.len(), "Position out of bounds");
    debug_assert!(
        tokens[pos].token_type == TokenType::StartArray,
        "Must start at ["
    );

    let check_pos = pos
        .checked_add(3)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if check_pos >= tokens.len() {
        trace_prop!("  -> hex: not enough tokens");
        return Ok(None);
    }

    trace_prop!(
        "  -> hex: checking token[{}]: {:?} = '{}'",
        pos + 1,
        tokens[pos + 1].token_type,
        tokens[pos + 1].text
    );

    if tokens[pos + 1].token_type != TokenType::Number || tokens[pos + 1].text != "0" {
        trace_prop!("  -> hex: token[1] is not '0'");
        return Ok(None);
    }

    trace_prop!(
        "  -> hex: checking token[{}]: {:?} = '{}'",
        pos + 2,
        tokens[pos + 2].token_type,
        tokens[pos + 2].text
    );

    if tokens[pos + 2].token_type != TokenType::Word {
        trace_prop!("  -> hex: token[2] is not a Word");
        return Ok(None);
    }

    let word_part = &tokens[pos + 2].text;
    if !word_part.starts_with('x') && !word_part.starts_with('X') {
        trace_prop!("  -> hex: word doesn't start with x");
        return Ok(None);
    }

    trace_prop!(
        "  -> hex: checking token[{}]: {:?} = '{}'",
        pos + 3,
        tokens[pos + 3].token_type,
        tokens[pos + 3].text
    );

    if tokens[pos + 3].token_type != TokenType::EndArray {
        trace_prop!("  -> hex: not followed by ]");
        return Ok(None);
    }

    let hex_part = &word_part[1..];
    trace_prop!("  -> hex: parsing hex part '{}'", hex_part);

    let code = u32::from_str_radix(hex_part, 16)
        .map_err(|_| BeautifyError::BeautificationFailed("Invalid hex".to_string()))?;

    trace_prop!("  -> hex: parsed as 0x{:x} ({})", code, code);

    if code > 127 {
        trace_prop!("  -> hex 0x{:x} is not ASCII", code);
        return Ok(None);
    }

    let ch = char::from_u32(code)
        .ok_or_else(|| BeautifyError::BeautificationFailed("Invalid char code".to_string()))?;

    debug_assert!(ch.is_ascii(), "Character should be ASCII: {:?}", ch);

    let property_name = ch.to_string();
    trace_prop!(
        "  -> hex: char is '{}' (valid identifier check...)",
        property_name
    );

    if !is_valid_identifier(&property_name) {
        trace_prop!("  -> hex char '{}' is not a valid identifier", ch);
        return Ok(None);
    }

    trace_prop!("converted [0x{:x}] -> .{}", code, property_name);

    let dot_token = Token::new(TokenType::Dot, ".".to_string());
    let prop_token = Token::new(TokenType::Word, property_name);

    debug_assert_eq!(dot_token.text, ".", "Dot token text should be '.'");
    debug_assert_eq!(dot_token.token_type, TokenType::Dot, "Should be Dot type");

    Ok(Some((vec![dot_token, prop_token], 4)))
}

fn try_convert_string_concat(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    debug_assert!(pos < tokens.len(), "Position out of bounds");
    debug_assert!(
        tokens[pos].token_type == TokenType::StartArray,
        "Must start at ["
    );

    trace_prop!("  -> concat: trying string concatenation");

    let start = pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if start >= tokens.len() {
        trace_prop!("  -> concat: not enough tokens");
        return Ok(None);
    }

    let mut parts = Vec::new();
    let mut i = start;
    let mut end_pos = None;

    while i < tokens.len() {
        if tokens[i].token_type == TokenType::EndArray {
            end_pos = Some(i);
            trace_prop!("  -> concat: found closing ] at position {}", i);
            break;
        }

        trace_prop!(
            "  -> concat: token[{}]: {:?} = '{}'",
            i,
            tokens[i].token_type,
            tokens[i].text
        );

        if tokens[i].token_type == TokenType::String {
            let string_value = &tokens[i].text;
            let unquoted = string_value.trim_matches('"').trim_matches('\'');
            parts.push(unquoted.to_string());

            if parts.len() > MAX_CONCAT_PARTS {
                trace_prop!("  -> too many concat parts: {}", parts.len());
                return Ok(None);
            }
        } else if tokens[i].token_type == TokenType::Operator && tokens[i].text == "+" {
            trace_prop!("  -> concat: found + operator");
        } else {
            trace_prop!(
                "  -> found non-string token in bracket: {:?}",
                tokens[i].token_type
            );
            return Ok(None);
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    if end_pos.is_none() || parts.is_empty() {
        return Ok(None);
    }

    let property_name = parts.join("");

    debug_assert!(
        property_name.len() <= MAX_PROPERTY_LENGTH,
        "Concatenated property too long: {}",
        property_name.len()
    );

    if !is_valid_identifier(&property_name) {
        trace_prop!("  -> '{}' is not a valid identifier", property_name);
        return Ok(None);
    }

    let skip = end_pos
        .unwrap()
        .checked_sub(pos)
        .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    trace_prop!("converted string concat -> .{}", property_name);

    let dot_token = Token::new(TokenType::Dot, ".".to_string());
    let prop_token = Token::new(TokenType::Word, property_name);

    Ok(Some((vec![dot_token, prop_token], skip)))
}

fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    if s.len() > MAX_PROPERTY_LENGTH {
        return false;
    }

    let mut chars = s.chars();
    let first = chars.next().unwrap();

    if !first.is_alphabetic() && first != '_' && first != '$' {
        return false;
    }

    for ch in chars {
        if !ch.is_alphanumeric() && ch != '_' && ch != '$' {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_convert_string_literal_property() {
        let code = r#"obj["property"]"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = convert_dynamic_properties(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("obj.property"),
            "Expected obj.property, got: {}",
            output
        );
        assert!(!output.contains("["), "Should not contain [");
        assert!(!output.contains("]"), "Should not contain ]");
    }

    #[test]
    fn test_convert_hex_to_ascii_property() {
        let code = r#"obj[0x61]"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        eprintln!("=== Input tokens ===");
        for (i, token) in tokens.iter().enumerate() {
            eprintln!("[{}] {:?} = '{}'", i, token.token_type, token.text);
        }

        let result = convert_dynamic_properties(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        eprintln!("=== Output: {} ===", output);

        assert!(output.contains("obj.a"), "Expected obj.a, got: {}", output);
    }

    #[test]
    fn test_convert_concatenated_property() {
        let code = r#"obj["pro" + "perty"]"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = convert_dynamic_properties(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("obj.property"),
            "Expected obj.property, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_dynamic_property() {
        let code = r#"obj[variable]"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = convert_dynamic_properties(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("obj[variable]"),
            "Should preserve dynamic access, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_invalid_identifier() {
        let code = r#"obj["123invalid"]"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = convert_dynamic_properties(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("["),
            "Should preserve bracket notation for invalid identifier"
        );
        assert!(
            output.contains("123invalid"),
            "Should contain original property"
        );
    }

    #[test]
    fn test_preserve_invalid_hex() {
        let code = r#"obj[0x1F600]"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = convert_dynamic_properties(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("["),
            "Should preserve bracket notation for non-ASCII hex"
        );
    }
}
