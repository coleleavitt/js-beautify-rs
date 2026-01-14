use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};

#[cfg(debug_assertions)]
macro_rules! trace_algebra {
    ($($arg:tt)*) => {
        eprintln!("[ALGEBRA] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_algebra {
    ($($arg:tt)*) => {};
}

/// Applies algebraic simplifications to reduce expression complexity.
///
/// Transformations:
/// - `x - x` → `0`
/// - `x * 0` → `0`
/// - `0 * x` → `0`
/// - `x / x` → `1` (when x is same identifier)
/// - `x % x` → `0`
/// - `x ^ x` → `0` (XOR with self)
///
/// Only applies to simple identifiers for safety.
pub fn simplify_algebraic(tokens: &[Token]) -> Result<Vec<Token>> {
    trace_algebra!("=== SIMPLIFYING ALGEBRAIC EXPRESSIONS ===");
    trace_algebra!("total tokens: {}", tokens.len());

    let mut result = Vec::new();
    let mut i = 0usize;
    let mut simplified_count = 0usize;

    while i < tokens.len() {
        if let Some((simplified, skip)) = try_simplify_at(tokens, i)? {
            debug_assert!(skip > 0, "skip must be positive");
            debug_assert!(skip < 100, "skip suspiciously large: {}", skip);

            result.extend(simplified);
            i = i
                .checked_add(skip)
                .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;

            simplified_count = simplified_count
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("count overflow".to_string()))?;

            continue;
        }

        result.push(tokens[i].clone());
        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;
    }

    trace_algebra!("simplified {} algebraic expressions", simplified_count);
    Ok(result)
}

fn try_simplify_at(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    debug_assert!(i < tokens.len(), "index out of bounds");

    if let Some(result) = try_simplify_self_subtract(tokens, i)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_simplify_multiply_zero(tokens, i)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_simplify_self_divide(tokens, i)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_simplify_self_modulo(tokens, i)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_simplify_self_xor(tokens, i)? {
        return Ok(Some(result));
    }

    Ok(None)
}

fn try_simplify_self_subtract(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i].token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[i2].token_type != TokenType::Operator || tokens[i2].text != "-" {
        return Ok(None);
    }

    if tokens[i3].token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[i].text != tokens[i3].text {
        return Ok(None);
    }

    trace_algebra!("simplifying {} - {} to 0", tokens[i].text, tokens[i3].text);

    Ok(Some((
        vec![Token::new(TokenType::Number, "0".to_string())],
        3,
    )))
}

fn try_simplify_multiply_zero(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i2].token_type != TokenType::Operator || tokens[i2].text != "*" {
        return Ok(None);
    }

    let zero_pos = if tokens[i].text == "0" {
        Some(i)
    } else if tokens[i3].text == "0" {
        Some(i3)
    } else {
        None
    };

    if zero_pos.is_none() {
        return Ok(None);
    }

    trace_algebra!("simplifying {} * {} to 0", tokens[i].text, tokens[i3].text);

    Ok(Some((
        vec![Token::new(TokenType::Number, "0".to_string())],
        3,
    )))
}

fn try_simplify_self_divide(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i].token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[i2].token_type != TokenType::Operator || tokens[i2].text != "/" {
        return Ok(None);
    }

    if tokens[i3].token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[i].text != tokens[i3].text {
        return Ok(None);
    }

    trace_algebra!("simplifying {} / {} to 1", tokens[i].text, tokens[i3].text);

    Ok(Some((
        vec![Token::new(TokenType::Number, "1".to_string())],
        3,
    )))
}

fn try_simplify_self_modulo(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i].token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[i2].token_type != TokenType::Operator || tokens[i2].text != "%" {
        return Ok(None);
    }

    if tokens[i3].token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[i].text != tokens[i3].text {
        return Ok(None);
    }

    trace_algebra!("simplifying {} % {} to 0", tokens[i].text, tokens[i3].text);

    Ok(Some((
        vec![Token::new(TokenType::Number, "0".to_string())],
        3,
    )))
}

fn try_simplify_self_xor(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i].token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[i2].token_type != TokenType::Operator || tokens[i2].text != "^" {
        return Ok(None);
    }

    if tokens[i3].token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[i].text != tokens[i3].text {
        return Ok(None);
    }

    trace_algebra!("simplifying {} ^ {} to 0", tokens[i].text, tokens[i3].text);

    Ok(Some((
        vec![Token::new(TokenType::Number, "0".to_string())],
        3,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize_expr(code: &str) -> Vec<Token> {
        code.split_whitespace()
            .map(|s| {
                let token_type = match s {
                    "-" | "*" | "/" | "%" | "^" => TokenType::Operator,
                    _ if s.parse::<u32>().is_ok() => TokenType::Number,
                    _ => TokenType::Word,
                };
                Token::new(token_type, s.to_string())
            })
            .collect()
    }

    #[test]
    fn test_self_subtract() {
        let tokens = tokenize_expr("x - x");
        let result = simplify_algebraic(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "0");
    }

    #[test]
    fn test_multiply_zero_left() {
        let tokens = tokenize_expr("0 * y");
        let result = simplify_algebraic(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "0");
    }

    #[test]
    fn test_multiply_zero_right() {
        let tokens = tokenize_expr("x * 0");
        let result = simplify_algebraic(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "0");
    }

    #[test]
    fn test_self_divide() {
        let tokens = tokenize_expr("z / z");
        let result = simplify_algebraic(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "1");
    }

    #[test]
    fn test_self_modulo() {
        let tokens = tokenize_expr("a % a");
        let result = simplify_algebraic(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "0");
    }

    #[test]
    fn test_self_xor() {
        let tokens = tokenize_expr("b ^ b");
        let result = simplify_algebraic(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "0");
    }

    #[test]
    fn test_no_simplification() {
        let tokens = tokenize_expr("x - y");
        let result = simplify_algebraic(&tokens).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[1].text, "-");
    }
}
