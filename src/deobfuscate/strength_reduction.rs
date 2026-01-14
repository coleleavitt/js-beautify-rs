use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};

#[cfg(debug_assertions)]
macro_rules! trace_strength {
    ($($arg:tt)*) => {
        eprintln!("[STRENGTH] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_strength {
    ($($arg:tt)*) => {};
}

/// Applies strength reduction by replacing expensive operations with cheaper equivalents.
///
/// Transformations:
/// - `x * 2` → `x << 1` (multiply by power of 2)
/// - `x * 4` → `x << 2`
/// - `x / 2` → `x >> 1` (divide by power of 2)
/// - `x / 4` → `x >> 2`
/// - `x % 2` → `x & 1` (modulo by power of 2)
/// - `x % 4` → `x & 3`
///
/// Only applies to powers of 2 for correctness.
pub fn apply_strength_reduction(tokens: &[Token]) -> Result<Vec<Token>> {
    trace_strength!("=== APPLYING STRENGTH REDUCTION ===");
    trace_strength!("total tokens: {}", tokens.len());

    let mut result = Vec::new();
    let mut i = 0usize;
    let mut reduced_count = 0usize;

    while i < tokens.len() {
        if let Some((reduced, skip)) = try_reduce_at(tokens, i)? {
            debug_assert!(skip > 0, "skip must be positive");
            debug_assert!(skip < 100, "skip suspiciously large: {}", skip);

            result.extend(reduced);
            i = i
                .checked_add(skip)
                .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;

            reduced_count = reduced_count
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("count overflow".to_string()))?;

            continue;
        }

        result.push(tokens[i].clone());
        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;
    }

    trace_strength!("applied {} strength reductions", reduced_count);
    Ok(result)
}

fn try_reduce_at(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    debug_assert!(i < tokens.len(), "index out of bounds");

    if let Some(result) = try_reduce_multiply(tokens, i)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_reduce_divide(tokens, i)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_reduce_modulo(tokens, i)? {
        return Ok(Some(result));
    }

    Ok(None)
}

fn try_reduce_multiply(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
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

    if tokens[i3].token_type != TokenType::Number {
        return Ok(None);
    }

    let n = tokens[i3].text.parse::<u32>().ok();
    if n.is_none() {
        return Ok(None);
    }
    let n = n.unwrap();

    if !is_power_of_two(n) {
        return Ok(None);
    }

    let shift_amount = (n as f64).log2() as u32;

    trace_strength!(
        "reducing {} * {} to {} << {}",
        tokens[i].text,
        n,
        tokens[i].text,
        shift_amount
    );

    let mut reduced = Vec::new();
    reduced.push(tokens[i].clone());
    reduced.push(Token::new(TokenType::Operator, "<<".to_string()));
    reduced.push(Token::new(TokenType::Number, shift_amount.to_string()));

    Ok(Some((reduced, 3)))
}

fn try_reduce_divide(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i2].token_type != TokenType::Operator || tokens[i2].text != "/" {
        return Ok(None);
    }

    if tokens[i3].token_type != TokenType::Number {
        return Ok(None);
    }

    let n = tokens[i3].text.parse::<u32>().ok();
    if n.is_none() {
        return Ok(None);
    }
    let n = n.unwrap();

    if !is_power_of_two(n) {
        return Ok(None);
    }

    let shift_amount = (n as f64).log2() as u32;

    trace_strength!(
        "reducing {} / {} to {} >> {}",
        tokens[i].text,
        n,
        tokens[i].text,
        shift_amount
    );

    let mut reduced = Vec::new();
    reduced.push(tokens[i].clone());
    reduced.push(Token::new(TokenType::Operator, ">>".to_string()));
    reduced.push(Token::new(TokenType::Number, shift_amount.to_string()));

    Ok(Some((reduced, 3)))
}

fn try_reduce_modulo(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i2].token_type != TokenType::Operator || tokens[i2].text != "%" {
        return Ok(None);
    }

    if tokens[i3].token_type != TokenType::Number {
        return Ok(None);
    }

    let n = tokens[i3].text.parse::<u32>().ok();
    if n.is_none() {
        return Ok(None);
    }
    let n = n.unwrap();

    if !is_power_of_two(n) {
        return Ok(None);
    }

    let mask = n
        .checked_sub(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?;

    trace_strength!(
        "reducing {} % {} to {} & {}",
        tokens[i].text,
        n,
        tokens[i].text,
        mask
    );

    let mut reduced = Vec::new();
    reduced.push(tokens[i].clone());
    reduced.push(Token::new(TokenType::Operator, "&".to_string()));
    reduced.push(Token::new(TokenType::Number, mask.to_string()));

    Ok(Some((reduced, 3)))
}

fn is_power_of_two(n: u32) -> bool {
    n > 0 && (n & (n.wrapping_sub(1))) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize_expr(code: &str) -> Vec<Token> {
        code.split_whitespace()
            .map(|s| {
                let token_type = match s {
                    "*" | "/" | "%" | "<<" | ">>" | "&" => TokenType::Operator,
                    _ if s.parse::<u32>().is_ok() => TokenType::Number,
                    _ => TokenType::Word,
                };
                Token::new(token_type, s.to_string())
            })
            .collect()
    }

    #[test]
    fn test_reduce_multiply_2() {
        let tokens = tokenize_expr("x * 2");
        let result = apply_strength_reduction(&tokens).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].text, "x");
        assert_eq!(result[1].text, "<<");
        assert_eq!(result[2].text, "1");
    }

    #[test]
    fn test_reduce_multiply_4() {
        let tokens = tokenize_expr("y * 4");
        let result = apply_strength_reduction(&tokens).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].text, "y");
        assert_eq!(result[1].text, "<<");
        assert_eq!(result[2].text, "2");
    }

    #[test]
    fn test_reduce_divide_2() {
        let tokens = tokenize_expr("x / 2");
        let result = apply_strength_reduction(&tokens).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].text, "x");
        assert_eq!(result[1].text, ">>");
        assert_eq!(result[2].text, "1");
    }

    #[test]
    fn test_reduce_modulo_4() {
        let tokens = tokenize_expr("x % 4");
        let result = apply_strength_reduction(&tokens).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].text, "x");
        assert_eq!(result[1].text, "&");
        assert_eq!(result[2].text, "3");
    }

    #[test]
    fn test_no_reduction_non_power_of_two() {
        let tokens = tokenize_expr("x * 3");
        let result = apply_strength_reduction(&tokens).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[1].text, "*");
    }

    #[test]
    fn test_power_of_two_check() {
        assert!(is_power_of_two(1));
        assert!(is_power_of_two(2));
        assert!(is_power_of_two(4));
        assert!(is_power_of_two(8));
        assert!(is_power_of_two(16));
        assert!(!is_power_of_two(0));
        assert!(!is_power_of_two(3));
        assert!(!is_power_of_two(5));
        assert!(!is_power_of_two(6));
    }
}
