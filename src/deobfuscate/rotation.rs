use super::StringArrayInfo;
use crate::Result;
use crate::token::{Token, TokenType};

pub fn detect_and_apply_rotation(
    tokens: &[Token],
    array_info: &mut StringArrayInfo,
) -> Result<bool> {
    if let Some(rotation_count) =
        detect_rotation_iife(tokens, array_info.end_index + 1, &array_info.variable_name)?
    {
        apply_rotation(&mut array_info.strings, rotation_count);
        array_info.rotated = true;
        return Ok(true);
    }

    Ok(false)
}

fn detect_rotation_iife(
    tokens: &[Token],
    start_search: usize,
    array_name: &str,
) -> Result<Option<usize>> {
    let mut i = start_search;

    while i < tokens.len() && i < start_search + 50 {
        if is_iife_start(tokens, i) {
            if let Some(count) = analyze_rotation_iife(tokens, i, array_name)? {
                return Ok(Some(count));
            }
        }
        i += 1;
    }

    Ok(None)
}

fn is_iife_start(tokens: &[Token], pos: usize) -> bool {
    if pos + 1 >= tokens.len() {
        return false;
    }

    tokens[pos].token_type == TokenType::StartExpr
        && pos + 1 < tokens.len()
        && tokens[pos + 1].token_type == TokenType::Reserved
        && tokens[pos + 1].text == "function"
}

fn analyze_rotation_iife(
    tokens: &[Token],
    start: usize,
    array_name: &str,
) -> Result<Option<usize>> {
    let mut i = start;
    let mut has_push_shift = false;
    let mut rotation_count = None;
    let mut iife_end = None;

    let mut depth = 0;
    while i < tokens.len() {
        match tokens[i].token_type {
            TokenType::StartExpr | TokenType::StartBlock => depth += 1,
            TokenType::EndExpr | TokenType::EndBlock => {
                depth -= 1;
                if depth == 0 && i > start {
                    iife_end = Some(i);
                    break;
                }
            }
            TokenType::Word if tokens[i].text == "push" || tokens[i].text == "shift" => {
                has_push_shift = true;
            }
            TokenType::Number => {
                if let Ok(num) = tokens[i].text.parse::<usize>() {
                    if num > 0 && num < 1000 {
                        rotation_count = Some(num);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(end) = iife_end {
        let mut j = end + 1;

        while j < tokens.len() && j < end + 10 {
            if tokens[j].token_type == TokenType::StartExpr {
                for k in j + 1..std::cmp::min(j + 5, tokens.len()) {
                    if tokens[k].token_type == TokenType::Word && tokens[k].text == array_name {
                        if has_push_shift && rotation_count.is_some() {
                            return Ok(rotation_count);
                        }
                    }
                }
            }
            j += 1;
        }
    }

    Ok(None)
}

fn apply_rotation(strings: &mut Vec<String>, count: usize) {
    let len = strings.len();
    if len == 0 {
        return;
    }

    let actual_count = count % len;

    for _ in 0..actual_count {
        if let Some(first) = strings.first().cloned() {
            strings.remove(0);
            strings.push(first);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deobfuscate::string_array::find_string_arrays;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_rotation_detection() {
        let code = r#"
var _0x1111 = ["a", "b", "c"];
(function(_0x2222, _0x3333) {
    var _0x4444 = function(_0x5555) {
        while (--_0x5555) {
            _0x2222.push(_0x2222.shift());
        }
    };
    _0x4444(2);
})(_0x1111, 0x123);
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let mut arrays = find_string_arrays(&tokens).unwrap();
        assert_eq!(arrays.len(), 1);

        let rotated = detect_and_apply_rotation(&tokens, &mut arrays[0]).unwrap();
        assert!(rotated);
        assert!(arrays[0].rotated);
    }

    #[test]
    fn test_apply_rotation() {
        let mut strings = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        apply_rotation(&mut strings, 1);
        assert_eq!(strings, vec!["b", "c", "a"]);

        let mut strings2 = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        apply_rotation(&mut strings2, 2);
        assert_eq!(strings2, vec!["z", "x", "y"]);
    }

    #[test]
    fn test_no_rotation() {
        let code = r#"var _0x1234 = ["hello", "world"];"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let mut arrays = find_string_arrays(&tokens).unwrap();
        let rotated = detect_and_apply_rotation(&tokens, &mut arrays[0]).unwrap();
        assert!(!rotated);
    }
}
