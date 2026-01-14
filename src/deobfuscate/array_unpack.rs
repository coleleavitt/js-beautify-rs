use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};

#[cfg(debug_assertions)]
macro_rules! trace_unpack {
    ($($arg:tt)*) => {
        eprintln!("[UNPACK] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_unpack {
    ($($arg:tt)*) => {};
}

/// Unpacks constant array access patterns to direct values.
///
/// Patterns unpacked:
/// - `[a, b][0]` → `a`
/// - `[a, b][1]` → `b`
/// - `[x, y, z][2]` → `z`
///
/// Only handles constant numeric indices. Dynamic indices are preserved.
pub fn unpack_array_access(tokens: &[Token]) -> Result<Vec<Token>> {
    trace_unpack!("=== UNPACKING ARRAY ACCESS ===");
    trace_unpack!("total tokens: {}", tokens.len());

    let mut result = Vec::new();
    let mut i = 0usize;
    let mut unpacked_count = 0usize;

    while i < tokens.len() {
        if let Some((unpacked, skip)) = try_unpack_at(tokens, i)? {
            debug_assert!(skip > 0, "skip must be positive");
            debug_assert!(skip < 1000, "skip suspiciously large: {}", skip);

            result.extend(unpacked);
            i = i
                .checked_add(skip)
                .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;

            unpacked_count = unpacked_count
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("count overflow".to_string()))?;

            continue;
        }

        result.push(tokens[i].clone());
        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;
    }

    trace_unpack!("unpacked {} array accesses", unpacked_count);
    Ok(result)
}

fn try_unpack_at(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    debug_assert!(i < tokens.len(), "index out of bounds");

    if tokens[i].token_type != TokenType::StartArray {
        return Ok(None);
    }

    let array_end = find_matching_bracket(tokens, i)?;
    if array_end.is_none() {
        return Ok(None);
    }
    let array_end = array_end.unwrap();

    let next_idx = array_end
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if next_idx >= tokens.len() {
        return Ok(None);
    }

    if tokens[next_idx].token_type != TokenType::StartArray {
        return Ok(None);
    }

    let index_end = find_matching_bracket(tokens, next_idx)?;
    if index_end.is_none() {
        return Ok(None);
    }
    let index_end = index_end.unwrap();

    let index_pos = next_idx
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if index_pos >= tokens.len() {
        return Ok(None);
    }

    if tokens[index_pos].token_type != TokenType::Number {
        return Ok(None);
    }

    let index_text = &tokens[index_pos].text;
    let index_val = index_text.parse::<usize>().ok();
    if index_val.is_none() {
        return Ok(None);
    }
    let index_val = index_val.unwrap();

    let elements = extract_array_elements(tokens, i, array_end)?;

    if index_val >= elements.len() {
        trace_unpack!(
            "index {} out of bounds for array of length {}",
            index_val,
            elements.len()
        );
        return Ok(None);
    }

    let element = elements[index_val].clone();
    trace_unpack!("unpacked array[{}] to {}", index_val, element.text);

    let skip = index_end.checked_sub(i).ok_or_else(|| {
        BeautifyError::BeautificationFailed("skip calculation underflow".to_string())
    })?;
    let skip = skip.checked_add(1).ok_or_else(|| {
        BeautifyError::BeautificationFailed("skip calculation overflow".to_string())
    })?;

    Ok(Some((vec![element], skip)))
}

fn find_matching_bracket(tokens: &[Token], start: usize) -> Result<Option<usize>> {
    debug_assert!(start < tokens.len(), "start out of bounds");
    debug_assert!(
        tokens[start].token_type == TokenType::StartArray,
        "must start with ["
    );

    let mut depth = 0usize;
    let mut i = start;

    while i < tokens.len() {
        if tokens[i].token_type == TokenType::StartArray {
            depth = depth
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("depth overflow".to_string()))?;
        } else if tokens[i].token_type == TokenType::EndArray {
            depth = depth.checked_sub(1).ok_or_else(|| {
                BeautifyError::BeautificationFailed("depth underflow".to_string())
            })?;

            if depth == 0 {
                return Ok(Some(i));
            }
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;
    }

    Ok(None)
}

fn extract_array_elements(tokens: &[Token], start: usize, end: usize) -> Result<Vec<Token>> {
    debug_assert!(start < tokens.len(), "start out of bounds");
    debug_assert!(end < tokens.len(), "end out of bounds");
    debug_assert!(start < end, "start must be before end");
    debug_assert!(tokens[start].token_type == TokenType::StartArray);
    debug_assert!(tokens[end].token_type == TokenType::EndArray);

    let mut elements = Vec::new();
    let mut i = start
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    while i < end {
        if tokens[i].token_type == TokenType::Comma {
            i = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
            continue;
        }

        elements.push(tokens[i].clone());

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    Ok(elements)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize_array(code: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let parts: Vec<&str> = code.split_whitespace().collect();

        for part in parts {
            let token_type = match part {
                "[" => TokenType::StartArray,
                "]" => TokenType::EndArray,
                "," => TokenType::Comma,
                _ if part.parse::<i32>().is_ok() => TokenType::Number,
                _ => TokenType::Word,
            };
            tokens.push(Token::new(token_type, part.to_string()));
        }

        tokens
    }

    #[test]
    fn test_unpack_array_first_element() {
        let tokens = tokenize_array("[ a , b ] [ 0 ]");
        let result = unpack_array_access(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "a");
    }

    #[test]
    fn test_unpack_array_second_element() {
        let tokens = tokenize_array("[ x , y ] [ 1 ]");
        let result = unpack_array_access(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "y");
    }

    #[test]
    fn test_unpack_array_third_element() {
        let tokens = tokenize_array("[ a , b , c ] [ 2 ]");
        let result = unpack_array_access(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "c");
    }

    #[test]
    fn test_out_of_bounds_preserved() {
        let tokens = tokenize_array("[ a , b ] [ 5 ]");
        let result = unpack_array_access(&tokens).unwrap();

        assert_eq!(result.len(), tokens.len());
    }

    #[test]
    fn test_no_unpacking_without_access() {
        let tokens = tokenize_array("[ a , b ]");
        let result = unpack_array_access(&tokens).unwrap();

        assert_eq!(result.len(), tokens.len());
    }
}
