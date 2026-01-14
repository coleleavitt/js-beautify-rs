use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};
use std::collections::{HashMap, HashSet};

#[cfg(debug_assertions)]
macro_rules! trace_dead {
    ($($arg:tt)*) => {
        eprintln!("[DEAD_VAR] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_dead {
    ($($arg:tt)*) => {};
}

/// Eliminates unused variable declarations.
///
/// Detects variables declared with let/const/var that are never read.
/// Only removes simple declarations - complex patterns preserved for safety.
pub fn eliminate_dead_variables(tokens: &[Token]) -> Result<Vec<Token>> {
    trace_dead!("=== ELIMINATING DEAD VARIABLES ===");
    trace_dead!("total tokens: {}", tokens.len());

    let declared = find_declared_variables(tokens)?;
    let used = find_used_variables(tokens)?;

    let mut dead_vars = HashSet::new();
    for (var_name, _pos) in &declared {
        if !used.contains(var_name) {
            dead_vars.insert(var_name.clone());
            trace_dead!("dead variable: {}", var_name);
        }
    }

    if dead_vars.is_empty() {
        trace_dead!("no dead variables found");
        return Ok(tokens.to_vec());
    }

    let mut result = Vec::new();
    let mut i = 0usize;
    let mut removed_count = 0usize;

    while i < tokens.len() {
        if let Some(skip) = try_remove_declaration(tokens, i, &dead_vars)? {
            debug_assert!(skip > 0, "skip must be positive");

            i = i
                .checked_add(skip)
                .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;

            removed_count = removed_count
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("count overflow".to_string()))?;

            continue;
        }

        result.push(tokens[i].clone());
        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;
    }

    trace_dead!("removed {} dead variables", removed_count);
    Ok(result)
}

fn find_declared_variables(tokens: &[Token]) -> Result<HashMap<String, usize>> {
    let mut declared = HashMap::new();

    for (i, token) in tokens.iter().enumerate() {
        if token.token_type == TokenType::Reserved
            && (token.text == "let" || token.text == "const" || token.text == "var")
        {
            let next_idx = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

            if next_idx < tokens.len() && tokens[next_idx].token_type == TokenType::Word {
                let var_name = tokens[next_idx].text.clone();
                declared.insert(var_name, i);
            }
        }
    }

    trace_dead!("found {} declared variables", declared.len());
    Ok(declared)
}

fn find_used_variables(tokens: &[Token]) -> Result<HashSet<String>> {
    let mut used = HashSet::new();
    let mut in_declaration = false;

    for token in tokens {
        if token.token_type == TokenType::Reserved
            && (token.text == "let" || token.text == "const" || token.text == "var")
        {
            in_declaration = true;
            continue;
        }

        if in_declaration && token.token_type == TokenType::Equals {
            in_declaration = false;
        }

        if in_declaration && token.token_type == TokenType::Semicolon {
            in_declaration = false;
        }

        if !in_declaration && token.token_type == TokenType::Word {
            used.insert(token.text.clone());
        }
    }

    trace_dead!("found {} used variables", used.len());
    Ok(used)
}

fn try_remove_declaration(
    tokens: &[Token],
    i: usize,
    dead_vars: &HashSet<String>,
) -> Result<Option<usize>> {
    debug_assert!(i < tokens.len(), "index out of bounds");

    if tokens[i].token_type != TokenType::Reserved {
        return Ok(None);
    }

    if tokens[i].text != "let" && tokens[i].text != "const" && tokens[i].text != "var" {
        return Ok(None);
    }

    let next_idx = i
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if next_idx >= tokens.len() {
        return Ok(None);
    }

    if tokens[next_idx].token_type != TokenType::Word {
        return Ok(None);
    }

    let var_name = &tokens[next_idx].text;

    if !dead_vars.contains(var_name) {
        return Ok(None);
    }

    let stmt_end = find_statement_end(tokens, i)?;
    if stmt_end.is_none() {
        return Ok(None);
    }
    let stmt_end = stmt_end.unwrap();

    let skip = stmt_end.checked_sub(i).ok_or_else(|| {
        BeautifyError::BeautificationFailed("skip calculation underflow".to_string())
    })?;
    let skip = skip.checked_add(1).ok_or_else(|| {
        BeautifyError::BeautificationFailed("skip calculation overflow".to_string())
    })?;

    trace_dead!("removing declaration: {}", var_name);
    Ok(Some(skip))
}

fn find_statement_end(tokens: &[Token], start: usize) -> Result<Option<usize>> {
    debug_assert!(start < tokens.len(), "start out of bounds");

    let mut i = start;

    while i < tokens.len() {
        if tokens[i].token_type == TokenType::Semicolon {
            return Ok(Some(i));
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize_decl(code: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let parts: Vec<&str> = code.split_whitespace().collect();

        for part in parts {
            let token_type = match part {
                "let" | "const" | "var" => TokenType::Reserved,
                "=" => TokenType::Equals,
                ";" => TokenType::Semicolon,
                _ if part.parse::<i32>().is_ok() => TokenType::Number,
                _ => TokenType::Word,
            };
            tokens.push(Token::new(token_type, part.to_string()));
        }

        tokens
    }

    #[test]
    fn test_remove_unused_let() {
        let tokens = tokenize_decl("let unused = 5 ;");
        let result = eliminate_dead_variables(&tokens).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_preserve_used_variable() {
        let tokens = tokenize_decl("let x = 5 ; console log x ;");
        let result = eliminate_dead_variables(&tokens).unwrap();

        assert!(result.len() > 0);
        assert!(result.iter().any(|t| t.text == "x"));
    }

    #[test]
    fn test_remove_multiple_unused() {
        let tokens = tokenize_decl("let a = 1 ; let b = 2 ;");
        let result = eliminate_dead_variables(&tokens).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_mixed_used_unused() {
        let tokens = tokenize_decl("let used = 1 ; let unused = 2 ; foo used ;");
        let result = eliminate_dead_variables(&tokens).unwrap();

        assert!(result.iter().any(|t| t.text == "used"));
        assert!(!result.iter().any(|t| t.text == "unused"));
    }
}
