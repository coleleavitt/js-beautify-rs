use super::{DecoderInfo, StringArrayInfo};
use crate::Result;
use crate::token::{Token, TokenType};

pub fn inline_decoded_strings(
    tokens: &mut Vec<Token>,
    string_arrays: &[StringArrayInfo],
    decoders: &[DecoderInfo],
) -> Result<()> {
    if decoders.is_empty() || string_arrays.is_empty() {
        return Ok(());
    }

    for decoder in decoders {
        inline_decoder_calls(tokens, decoder, string_arrays)?;
    }

    Ok(())
}

fn inline_decoder_calls(
    tokens: &mut Vec<Token>,
    decoder: &DecoderInfo,
    string_arrays: &[StringArrayInfo],
) -> Result<()> {
    let array_info = string_arrays
        .iter()
        .find(|a| a.variable_name == decoder.array_ref)
        .ok_or_else(|| {
            crate::BeautifyError::BeautificationFailed(format!(
                "String array {} not found",
                decoder.array_ref
            ))
        })?;

    let mut i = 0;
    while i < tokens.len() {
        if is_decoder_call(tokens, i, &decoder.function_name) {
            if let Some(index) = extract_call_index(tokens, i) {
                if let Some(decoded_string) = get_decoded_string(array_info, decoder, index) {
                    replace_decoder_call(tokens, i, decoded_string);
                }
            }
        }
        i += 1;
    }

    Ok(())
}

fn is_decoder_call(tokens: &[Token], start: usize, function_name: &str) -> bool {
    if start + 2 >= tokens.len() {
        return false;
    }

    tokens[start].token_type == TokenType::Word
        && tokens[start].text == function_name
        && tokens[start + 1].token_type == TokenType::StartExpr
}

fn extract_call_index(tokens: &[Token], start: usize) -> Option<usize> {
    if start + 3 >= tokens.len() {
        return None;
    }

    let arg_token = &tokens[start + 2];

    if arg_token.token_type == TokenType::Number {
        if let Ok(index) = arg_token.text.parse::<usize>() {
            return Some(index);
        }
    }

    None
}

fn get_decoded_string(
    array_info: &StringArrayInfo,
    decoder: &DecoderInfo,
    mut index: usize,
) -> Option<String> {
    if decoder.has_offset {
        if let Some(offset) = decoder.offset_value {
            index = (index as i32 - offset) as usize;
        }
    }

    if index < array_info.strings.len() {
        Some(array_info.strings[index].clone())
    } else {
        None
    }
}

fn replace_decoder_call(tokens: &mut Vec<Token>, start: usize, decoded_string: String) {
    let mut end = start + 1;

    while end < tokens.len() && tokens[end].token_type != TokenType::EndExpr {
        end += 1;
    }

    if end < tokens.len() {
        end += 1;
    }

    tokens[start] = Token::new(TokenType::String, decoded_string);

    for i in (start + 1..end).rev() {
        tokens.remove(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deobfuscate::decoder::find_decoders;
    use crate::deobfuscate::string_array::find_string_arrays;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_inline_simple_decoder_call() {
        let code = r#"
var _0x1234 = ["hello", "world"];
function _0xdec(a) { return _0x1234[a]; }
console.log(_0xdec(1));
        "#;
        let mut tokenizer = Tokenizer::new(code);
        let mut tokens = tokenizer.tokenize().unwrap();

        let arrays = find_string_arrays(&tokens).unwrap();
        let decoders = find_decoders(&tokens, &arrays).unwrap();

        inline_decoded_strings(&mut tokens, &arrays, &decoders).unwrap();

        let has_world = tokens.iter().any(|t| t.text == "\"world\"");
        assert!(has_world, "Should have inlined 'world' string");
    }
}
