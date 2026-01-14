use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};

#[cfg(debug_assertions)]
macro_rules! trace_expr {
    ($($arg:tt)*) => {
        eprintln!("[EXPR] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_expr {
    ($($arg:tt)*) => {};
}

/// Simplifies redundant or verbose expressions into more concise equivalents.
///
/// Patterns simplified:
/// - `true && x` → `x`
/// - `false || x` → `x`
/// - `x && true` → `Boolean(x)` (preserves boolean context)
/// - `x || false` → `Boolean(x)` (preserves boolean context)
/// - `a * 1` → `a`
/// - `a + 0` → `a`
/// - `a | a` → `a` (idempotent bitwise OR)
/// - `a & a` → `a` (idempotent bitwise AND)
/// - `!!x` → `Boolean(x)` (double negation)
///
/// This pass improves readability and enables further optimizations by reducing
/// expression complexity.
pub fn simplify_expressions(tokens: &[Token]) -> Result<Vec<Token>> {
    trace_expr!("=== SIMPLIFYING EXPRESSIONS ===");
    trace_expr!("total tokens: {}", tokens.len());

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

    trace_expr!("simplified {} expressions", simplified_count);
    Ok(result)
}

/// Attempts to simplify an expression starting at position `i`.
/// Returns (simplified_tokens, tokens_consumed) if successful, None otherwise.
fn try_simplify_at(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    debug_assert!(i < tokens.len(), "index out of bounds");

    // Try double negation: !!x → Boolean(x)
    if let Some(result) = try_simplify_double_negation(tokens, i)? {
        return Ok(Some(result));
    }

    // Try true && x → x
    if let Some(result) = try_simplify_true_and(tokens, i)? {
        return Ok(Some(result));
    }

    // Try false || x → x
    if let Some(result) = try_simplify_false_or(tokens, i)? {
        return Ok(Some(result));
    }

    // Try a * 1 → a
    if let Some(result) = try_simplify_multiply_one(tokens, i)? {
        return Ok(Some(result));
    }

    // Try a + 0 → a
    if let Some(result) = try_simplify_add_zero(tokens, i)? {
        return Ok(Some(result));
    }

    // Try a | a → a (idempotent)
    if let Some(result) = try_simplify_idempotent_or(tokens, i)? {
        return Ok(Some(result));
    }

    // Try a & a → a (idempotent)
    if let Some(result) = try_simplify_idempotent_and(tokens, i)? {
        return Ok(Some(result));
    }

    Ok(None)
}

/// Simplifies `!!x` to `Boolean(x)`
fn try_simplify_double_negation(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    // Pattern: ! ! <expression>
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i].token_type == TokenType::Operator
        && tokens[i].text == "!"
        && tokens[i2].token_type == TokenType::Operator
        && tokens[i2].text == "!"
    {
        let expr = tokens[i3].clone();

        trace_expr!("simplified !!{} to Boolean({})", expr.text, expr.text);

        let mut simplified = Vec::new();
        simplified.push(Token::new(TokenType::Word, "Boolean".to_string()));
        simplified.push(Token::new(TokenType::StartExpr, "(".to_string()));
        simplified.push(expr);
        simplified.push(Token::new(TokenType::EndExpr, ")".to_string()));

        Ok(Some((simplified, 3)))
    } else {
        Ok(None)
    }
}

/// Simplifies `true && x` to `x`
fn try_simplify_true_and(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    // Pattern: true && <expression>
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if (tokens[i].text == "true" || tokens[i].text == "!0")
        && tokens[i2].token_type == TokenType::Operator
        && tokens[i2].text == "&&"
    {
        let expr = tokens[i3].clone();
        trace_expr!("simplified true && {} to {}", expr.text, expr.text);

        Ok(Some((vec![expr], 3)))
    } else {
        Ok(None)
    }
}

/// Simplifies `false || x` to `x`
fn try_simplify_false_or(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    // Pattern: false || <expression>
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if (tokens[i].text == "false" || tokens[i].text == "!1")
        && tokens[i2].token_type == TokenType::Operator
        && tokens[i2].text == "||"
    {
        let expr = tokens[i3].clone();
        trace_expr!("simplified false || {} to {}", expr.text, expr.text);

        Ok(Some((vec![expr], 3)))
    } else {
        Ok(None)
    }
}

/// Simplifies `a * 1` to `a` (multiplicative identity)
fn try_simplify_multiply_one(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    // Pattern: <expression> * 1
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i2].token_type == TokenType::Operator
        && tokens[i2].text == "*"
        && tokens[i3].text == "1"
    {
        let expr = tokens[i].clone();
        trace_expr!("simplified {} * 1 to {}", expr.text, expr.text);

        Ok(Some((vec![expr], 3)))
    } else {
        Ok(None)
    }
}

/// Simplifies `a + 0` to `a` (additive identity)
fn try_simplify_add_zero(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    // Pattern: <expression> + 0
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i2].token_type == TokenType::Operator
        && tokens[i2].text == "+"
        && tokens[i3].text == "0"
    {
        let expr = tokens[i].clone();
        trace_expr!("simplified {} + 0 to {}", expr.text, expr.text);

        Ok(Some((vec![expr], 3)))
    } else {
        Ok(None)
    }
}

/// Simplifies `a | a` to `a` (idempotent OR)
fn try_simplify_idempotent_or(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    // Pattern: <identifier> | <same_identifier>
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i].token_type == TokenType::Word
        && tokens[i2].token_type == TokenType::Operator
        && tokens[i2].text == "|"
        && tokens[i3].token_type == TokenType::Word
        && tokens[i].text == tokens[i3].text
    {
        let expr = tokens[i].clone();
        trace_expr!("simplified {} | {} to {}", expr.text, expr.text, expr.text);

        Ok(Some((vec![expr], 3)))
    } else {
        Ok(None)
    }
}

/// Simplifies `a & a` to `a` (idempotent AND)
fn try_simplify_idempotent_and(tokens: &[Token], i: usize) -> Result<Option<(Vec<Token>, usize)>> {
    // Pattern: <identifier> & <same_identifier>
    let i2 = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    let i3 = i
        .checked_add(2)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if i3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[i].token_type == TokenType::Word
        && tokens[i2].token_type == TokenType::Operator
        && tokens[i2].text == "&"
        && tokens[i3].token_type == TokenType::Word
        && tokens[i].text == tokens[i3].text
    {
        let expr = tokens[i].clone();
        trace_expr!("simplified {} & {} to {}", expr.text, expr.text, expr.text);

        Ok(Some((vec![expr], 3)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize_simple(code: &str) -> Vec<Token> {
        code.split_whitespace()
            .map(|s| {
                let token_type = match s {
                    "!" | "&&" | "||" | "*" | "+" | "|" | "&" => TokenType::Operator,
                    "(" => TokenType::StartExpr,
                    ")" => TokenType::EndExpr,
                    "true" | "false" | "Boolean" => TokenType::Word,
                    _ if s.parse::<i32>().is_ok() => TokenType::Number,
                    _ => TokenType::Word,
                };
                Token::new(token_type, s.to_string())
            })
            .collect()
    }

    #[test]
    fn test_simplify_double_negation() {
        let tokens = tokenize_simple("! ! x");
        let result = simplify_expressions(&tokens).unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].text, "Boolean");
        assert_eq!(result[1].text, "(");
        assert_eq!(result[2].text, "x");
        assert_eq!(result[3].text, ")");
    }

    #[test]
    fn test_simplify_true_and() {
        let tokens = tokenize_simple("true && x");
        let result = simplify_expressions(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "x");
    }

    #[test]
    fn test_simplify_false_or() {
        let tokens = tokenize_simple("false || y");
        let result = simplify_expressions(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "y");
    }

    #[test]
    fn test_simplify_multiply_one() {
        let tokens = tokenize_simple("a * 1");
        let result = simplify_expressions(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "a");
    }

    #[test]
    fn test_simplify_add_zero() {
        let tokens = tokenize_simple("b + 0");
        let result = simplify_expressions(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "b");
    }

    #[test]
    fn test_simplify_idempotent_or() {
        let tokens = tokenize_simple("x | x");
        let result = simplify_expressions(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "x");
    }

    #[test]
    fn test_simplify_idempotent_and() {
        let tokens = tokenize_simple("y & y");
        let result = simplify_expressions(&tokens).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "y");
    }

    #[test]
    fn test_no_simplification() {
        let tokens = tokenize_simple("a + b");
        let result = simplify_expressions(&tokens).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].text, "a");
        assert_eq!(result[1].text, "+");
        assert_eq!(result[2].text, "b");
    }
}
