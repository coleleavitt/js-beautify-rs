use super::{DecoderInfo, StringArrayInfo};
use crate::Result;
use crate::token::{Token, TokenType};

pub fn remove_dead_code(
    tokens: &[Token],
    string_arrays: &[StringArrayInfo],
    decoders: &[DecoderInfo],
) -> Result<Vec<Token>> {
    let usage_counts = count_decoder_usage(tokens, decoders)?;

    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if should_remove_token_sequence(tokens, i, string_arrays, decoders, &usage_counts)? {
            i = skip_declaration(tokens, i)?;
        } else {
            result.push(tokens[i].clone());
            i += 1;
        }
    }

    Ok(result)
}

fn count_decoder_usage(
    tokens: &[Token],
    decoders: &[DecoderInfo],
) -> Result<std::collections::HashMap<String, usize>> {
    use std::collections::HashMap;

    let mut usage_counts = HashMap::new();

    for decoder in decoders {
        usage_counts.insert(decoder.name.clone(), 0);
    }

    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].token_type == TokenType::Word {
            let name = &tokens[i].text;
            if let Some(count) = usage_counts.get_mut(name) {
                let in_definition = is_in_decoder_definition(tokens, i, decoders)?;
                if !in_definition {
                    *count += 1;
                }
            }
        }
        i += 1;
    }

    Ok(usage_counts)
}

fn is_in_decoder_definition(
    _tokens: &[Token],
    pos: usize,
    decoders: &[DecoderInfo],
) -> Result<bool> {
    for decoder in decoders {
        if pos >= decoder.start_index && pos <= decoder.end_index {
            return Ok(true);
        }
    }
    Ok(false)
}

fn should_remove_token_sequence(
    tokens: &[Token],
    pos: usize,
    string_arrays: &[StringArrayInfo],
    decoders: &[DecoderInfo],
    usage_counts: &std::collections::HashMap<String, usize>,
) -> Result<bool> {
    if is_string_array_declaration(tokens, pos, string_arrays)? {
        return Ok(true);
    }

    if is_decoder_function_declaration(tokens, pos, decoders, usage_counts)? {
        return Ok(true);
    }

    if is_rotation_iife(tokens, pos, string_arrays)? {
        return Ok(true);
    }

    Ok(false)
}

fn is_string_array_declaration(
    tokens: &[Token],
    pos: usize,
    string_arrays: &[StringArrayInfo],
) -> Result<bool> {
    if pos + 3 >= tokens.len() {
        return Ok(false);
    }

    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "var" {
        return Ok(false);
    }

    if tokens[pos + 1].token_type != TokenType::Word {
        return Ok(false);
    }

    let var_name = &tokens[pos + 1].text;

    for array in string_arrays {
        if var_name == &array.variable_name {
            return Ok(true);
        }
    }

    Ok(false)
}

fn is_decoder_function_declaration(
    tokens: &[Token],
    pos: usize,
    decoders: &[DecoderInfo],
    usage_counts: &std::collections::HashMap<String, usize>,
) -> Result<bool> {
    if pos + 2 >= tokens.len() {
        return Ok(false);
    }

    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "function" {
        return Ok(false);
    }

    if tokens[pos + 1].token_type != TokenType::Word {
        return Ok(false);
    }

    let func_name = &tokens[pos + 1].text;

    for decoder in decoders {
        if func_name == &decoder.name {
            let usage_count = usage_counts.get(&decoder.name).copied().unwrap_or(0);
            return Ok(usage_count == 0);
        }
    }

    Ok(false)
}

fn is_rotation_iife(
    tokens: &[Token],
    pos: usize,
    string_arrays: &[StringArrayInfo],
) -> Result<bool> {
    if pos + 5 >= tokens.len() {
        return Ok(false);
    }

    if tokens[pos].token_type != TokenType::StartExpr
        || tokens[pos + 1].token_type != TokenType::Reserved
        || tokens[pos + 1].text != "function"
    {
        return Ok(false);
    }

    let mut depth = 0;
    let mut i = pos;
    let mut has_push_shift = false;

    while i < tokens.len() && i < pos + 100 {
        match tokens[i].token_type {
            TokenType::StartExpr | TokenType::StartBlock => depth += 1,
            TokenType::EndExpr | TokenType::EndBlock => {
                depth -= 1;
                if depth == 0 {
                    if i + 2 < tokens.len() && tokens[i + 1].token_type == TokenType::StartExpr {
                        for j in i + 2..std::cmp::min(i + 10, tokens.len()) {
                            if tokens[j].token_type == TokenType::Word {
                                for array in string_arrays {
                                    if tokens[j].text == array.variable_name {
                                        return Ok(has_push_shift);
                                    }
                                }
                            }
                        }
                    }
                    break;
                }
            }
            TokenType::Word if tokens[i].text == "push" || tokens[i].text == "shift" => {
                has_push_shift = true;
            }
            _ => {}
        }
        i += 1;
    }

    Ok(false)
}

fn skip_declaration(tokens: &[Token], start: usize) -> Result<usize> {
    let mut i = start;
    let mut depth = 0;

    while i < tokens.len() {
        match tokens[i].token_type {
            TokenType::StartExpr | TokenType::StartBlock | TokenType::StartArray => depth += 1,
            TokenType::EndExpr | TokenType::EndBlock | TokenType::EndArray => {
                depth -= 1;
                if depth <= 0 {
                    i += 1;
                    while i < tokens.len() && tokens[i].token_type == TokenType::Semicolon {
                        i += 1;
                    }
                    return Ok(i);
                }
            }
            TokenType::Semicolon if depth == 0 => {
                return Ok(i + 1);
            }
            _ => {}
        }
        i += 1;
    }

    Ok(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_remove_string_array() {
        let code = r#"
var _0x1234 = ["hello", "world"];
console.log("test");
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let array_info = StringArrayInfo {
            variable_name: "_0x1234".to_string(),
            strings: vec!["hello".to_string(), "world".to_string()],
            start_index: 1,
            end_index: 11,
            rotated: false,
        };

        let result = remove_dead_code(&tokens, &[array_info], &[]).unwrap();

        assert!(result.len() < tokens.len());

        let has_array_var = result
            .iter()
            .any(|t| t.token_type == TokenType::Word && t.text == "_0x1234");
        assert!(!has_array_var);
    }

    #[test]
    fn test_remove_decoder_function() {
        let code = r#"
function _0xdec(a) {
    return _0x1234[a];
}
console.log("test");
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let decoder_info = DecoderInfo {
            name: "_0xdec".to_string(),
            array_name: "_0x1234".to_string(),
            start_index: 1,
            end_index: 14,
            offset: 0,
        };

        let result = remove_dead_code(&tokens, &[], &[decoder_info]).unwrap();

        assert!(result.len() < tokens.len());
    }

    #[test]
    fn test_preserve_other_code() {
        let code = r#"console.log("test");"#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = remove_dead_code(&tokens, &[], &[]).unwrap();

        assert_eq!(result.len(), tokens.len());
    }
}
