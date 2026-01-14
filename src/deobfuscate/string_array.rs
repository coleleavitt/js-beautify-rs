use super::StringArrayInfo;
use crate::Result;
use crate::token::{Token, TokenType};

fn is_obfuscated_name(name: &str) -> bool {
    name.starts_with("_0x") || name.starts_with("_0X")
}

pub fn find_string_arrays(tokens: &[Token]) -> Result<Vec<StringArrayInfo>> {
    let mut arrays = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some(array_info) = detect_string_array(tokens, i)? {
            arrays.push(array_info);
            i = arrays.last().unwrap().end_index + 1;
        } else {
            i += 1;
        }
    }

    Ok(arrays)
}

fn detect_string_array(tokens: &[Token], start: usize) -> Result<Option<StringArrayInfo>> {
    if start + 5 >= tokens.len() {
        return Ok(None);
    }

    let var_token = &tokens[start];
    if var_token.token_type != TokenType::Reserved || var_token.text != "var" {
        return Ok(None);
    }

    let name_token = &tokens[start + 1];
    if name_token.token_type != TokenType::Word {
        return Ok(None);
    }

    if !is_obfuscated_name(&name_token.text) {
        return Ok(None);
    }

    let equals_token = &tokens[start + 2];
    if equals_token.token_type != TokenType::Equals {
        return Ok(None);
    }

    let start_array = &tokens[start + 3];
    if start_array.token_type != TokenType::StartArray {
        return Ok(None);
    }

    let mut strings = Vec::new();
    let mut current = start + 4;
    let mut end_index = start + 4;

    while current < tokens.len() {
        let token = &tokens[current];

        match token.token_type {
            TokenType::String => {
                strings.push(token.text.clone());
                current += 1;
            }
            TokenType::Comma => {
                current += 1;
            }
            TokenType::EndArray => {
                end_index = current;
                break;
            }
            _ => {
                return Ok(None);
            }
        }
    }

    if strings.is_empty() {
        return Ok(None);
    }

    Ok(Some(StringArrayInfo {
        variable_name: name_token.text.clone(),
        start_index: start,
        end_index,
        strings,
        rotated: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_detect_simple_string_array() {
        let code = r#"var _0x1234 = ["hello", "world", "test"];"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let arrays = find_string_arrays(&tokens).unwrap();
        assert_eq!(arrays.len(), 1);
        assert_eq!(arrays[0].variable_name, "_0x1234");
        assert_eq!(arrays[0].strings.len(), 3);
        assert_eq!(arrays[0].strings[0], "\"hello\"");
        assert_eq!(arrays[0].strings[1], "\"world\"");
        assert_eq!(arrays[0].strings[2], "\"test\"");
    }

    #[test]
    fn test_no_string_array() {
        let code = "var x = 42;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let arrays = find_string_arrays(&tokens).unwrap();
        assert_eq!(arrays.len(), 0);
    }
}
