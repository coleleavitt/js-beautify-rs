use super::{DecoderInfo, StringArrayInfo};
use crate::Result;
use crate::token::{Token, TokenType};

pub fn find_decoders(
    tokens: &[Token],
    string_arrays: &[StringArrayInfo],
) -> Result<Vec<DecoderInfo>> {
    let mut decoders = Vec::new();

    for array_info in string_arrays {
        if let Some(decoder) = find_decoder_for_array(tokens, array_info)? {
            decoders.push(decoder);
        }
    }

    Ok(decoders)
}

fn find_decoder_for_array(
    tokens: &[Token],
    array_info: &StringArrayInfo,
) -> Result<Option<DecoderInfo>> {
    let mut i = array_info.end_index + 1;

    while i < tokens.len() {
        if let Some(decoder) = detect_decoder_function(tokens, i, &array_info.variable_name)? {
            return Ok(Some(decoder));
        }
        i += 1;
    }

    Ok(None)
}

fn detect_decoder_function(
    tokens: &[Token],
    start: usize,
    array_name: &str,
) -> Result<Option<DecoderInfo>> {
    if start + 10 >= tokens.len() {
        return Ok(None);
    }

    let function_token = &tokens[start];
    if function_token.token_type != TokenType::Reserved || function_token.text != "function" {
        return Ok(None);
    }

    let name_token = &tokens[start + 1];
    if name_token.token_type != TokenType::Word {
        return Ok(None);
    }

    let start_paren = &tokens[start + 2];
    if start_paren.token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let mut param_count = 0;
    let mut current = start + 3;

    while current < tokens.len() && tokens[current].token_type != TokenType::EndExpr {
        if tokens[current].token_type == TokenType::Word {
            param_count += 1;
        }
        current += 1;
    }

    if param_count == 0 {
        return Ok(None);
    }

    let body_start = current + 1;
    if body_start >= tokens.len() || tokens[body_start].token_type != TokenType::StartBlock {
        return Ok(None);
    }

    let mut references_array = false;
    let mut has_offset = false;
    let mut end_index = body_start;

    let mut brace_depth = 1;
    current = body_start + 1;

    while current < tokens.len() && brace_depth > 0 {
        match tokens[current].token_type {
            TokenType::StartBlock => brace_depth += 1,
            TokenType::EndBlock => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    end_index = current;
                }
            }
            TokenType::Word if tokens[current].text == array_name => {
                references_array = true;
            }
            TokenType::Operator if tokens[current].text == "-" => {
                if current > 0 && tokens[current - 1].token_type == TokenType::Word {
                    has_offset = true;
                }
            }
            _ => {}
        }
        current += 1;
    }

    if !references_array {
        return Ok(None);
    }

    Ok(Some(DecoderInfo {
        function_name: name_token.text.clone(),
        start_index: start,
        end_index,
        array_ref: array_name.to_string(),
        has_offset,
        offset_value: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::super::string_array::find_string_arrays;
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_find_simple_decoder() {
        let code = r#"
var _0x1234 = ["hello", "world"];
function _0xdecoder(a) {
    return _0x1234[a];
}
        "#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let arrays = find_string_arrays(&tokens).unwrap();
        let decoders = find_decoders(&tokens, &arrays).unwrap();

        assert_eq!(decoders.len(), 1);
        assert_eq!(decoders[0].function_name, "_0xdecoder");
        assert_eq!(decoders[0].array_ref, "_0x1234");
        assert!(!decoders[0].has_offset);
    }

    #[test]
    fn test_find_decoder_with_offset() {
        let code = r#"
var _0xabcd = ["foo", "bar"];
function _0xdecode(a) {
    a = a - 0x123;
    return _0xabcd[a];
}
        "#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let arrays = find_string_arrays(&tokens).unwrap();
        let decoders = find_decoders(&tokens, &arrays).unwrap();

        assert_eq!(decoders.len(), 1);
        assert_eq!(decoders[0].function_name, "_0xdecode");
        assert!(decoders[0].has_offset);
    }
}
