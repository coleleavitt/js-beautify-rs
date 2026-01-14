use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};

const MAX_TERNARY_DEPTH: usize = 10;

#[cfg(debug_assertions)]
macro_rules! trace_ternary {
    ($($arg:tt)*) => {
        eprintln!("[TERNARY] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_ternary {
    ($($arg:tt)*) => {};
}

pub fn simplify_ternary_chains(tokens: &[Token]) -> Result<Vec<Token>> {
    trace_ternary!("=== SIMPLIFYING TERNARY CHAINS ===");
    trace_ternary!("total tokens: {}", tokens.len());

    let mut result = Vec::new();
    let mut i = 0usize;
    let mut simplified_count = 0usize;

    while i < tokens.len() {
        trace_ternary!(
            "checking position {}: {:?} = '{}'",
            i,
            tokens[i].token_type,
            tokens[i].text
        );

        if let Some((simplified, skip)) = try_simplify_ternary(tokens, i)? {
            debug_assert!(skip > 0, "Skip must be positive");
            debug_assert!(skip < 10000, "Skip suspiciously large: {}", skip);

            result.extend(simplified);
            i = i
                .checked_add(skip)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

            simplified_count = simplified_count
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("count overflow".to_string()))?;

            continue;
        }

        result.push(tokens[i].clone());
        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    debug_assert!(
        result.len() <= tokens.len(),
        "Result should not exceed input"
    );

    trace_ternary!("simplified {} ternary chains", simplified_count);
    trace_ternary!("final token count: {} -> {}", tokens.len(), result.len());

    Ok(result)
}

fn try_simplify_ternary(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if !is_ternary_start(tokens, pos) {
        return Ok(None);
    }

    trace_ternary!("potential ternary at position {}", pos);

    if let Some(simplified) = try_simplify_constant_condition(tokens, pos)? {
        trace_ternary!("  -> simplified constant condition");
        return Ok(Some(simplified));
    }

    Ok(None)
}

fn is_ternary_start(tokens: &[Token], pos: usize) -> bool {
    for i in pos..tokens.len().min(pos.saturating_add(20)) {
        if tokens[i].token_type == TokenType::QuestionMark {
            return true;
        }

        if matches!(
            tokens[i].token_type,
            TokenType::Semicolon | TokenType::StartBlock | TokenType::EndBlock
        ) {
            return false;
        }
    }

    false
}

fn try_simplify_constant_condition(
    tokens: &[Token],
    pos: usize,
) -> Result<Option<(Vec<Token>, usize)>> {
    let check_pos = pos
        .checked_add(5)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if check_pos >= tokens.len() {
        return Ok(None);
    }

    let is_true = tokens[pos].token_type == TokenType::Word && tokens[pos].text == "true";
    let is_false = tokens[pos].token_type == TokenType::Word && tokens[pos].text == "false";

    if !is_true && !is_false {
        return Ok(None);
    }

    let question_pos = pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if question_pos >= tokens.len() {
        return Ok(None);
    }
    if tokens[question_pos].token_type != TokenType::QuestionMark {
        return Ok(None);
    }
    trace_ternary!(
        "found constant ternary: {} ? ...",
        if is_true { "true" } else { "false" }
    );

    let true_branch_start = question_pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if true_branch_start >= tokens.len() {
        return Ok(None);
    }

    let (colon_pos, _depth) = find_ternary_colon(tokens, question_pos)?;

    if colon_pos.is_none() {
        trace_ternary!("  -> could not find colon");
        return Ok(None);
    }

    let colon_pos = colon_pos.unwrap();

    let false_branch_start = colon_pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if false_branch_start >= tokens.len() {
        return Ok(None);
    }

    let ternary_end = find_ternary_end(tokens, false_branch_start)?;

    debug_assert!(
        ternary_end > false_branch_start,
        "Ternary end must be after false branch start"
    );

    let branch_start = if is_true {
        true_branch_start
    } else {
        false_branch_start
    };

    let branch_end = if is_true { colon_pos } else { ternary_end };

    debug_assert!(
        branch_end > branch_start,
        "Branch end must be after branch start"
    );

    let mut result = Vec::new();
    for idx in branch_start..branch_end {
        result.push(tokens[idx].clone());
    }

    let skip = ternary_end
        .checked_sub(pos)
        .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?;

    trace_ternary!(
        "simplified {} ? ... : ... to {} tokens",
        if is_true { "true" } else { "false" },
        result.len()
    );

    Ok(Some((result, skip)))
}

fn find_ternary_colon(tokens: &[Token], question_pos: usize) -> Result<(Option<usize>, usize)> {
    debug_assert!(question_pos < tokens.len(), "Question pos out of bounds");

    let mut depth = 1usize;
    let mut i = question_pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartExpr => {
                paren_depth = paren_depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("paren depth overflow".to_string())
                })?;
            }
            TokenType::EndExpr => {
                paren_depth = paren_depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("paren depth underflow".to_string())
                })?;
            }
            TokenType::StartArray => {
                bracket_depth = bracket_depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("bracket depth overflow".to_string())
                })?;
            }
            TokenType::EndArray => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("bracket depth underflow".to_string())
                })?;
            }
            TokenType::StartBlock => {
                brace_depth = brace_depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("brace depth overflow".to_string())
                })?;
            }
            TokenType::EndBlock => {
                brace_depth = brace_depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("brace depth underflow".to_string())
                })?;
            }
            TokenType::QuestionMark => {
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                    depth = depth.checked_add(1).ok_or_else(|| {
                        BeautifyError::BeautificationFailed("depth overflow".to_string())
                    })?;

                    if depth > MAX_TERNARY_DEPTH {
                        trace_ternary!("  -> ternary too deeply nested");
                        return Ok((None, 0));
                    }
                }
            }
            TokenType::Colon => {
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                    depth = depth.checked_sub(1).ok_or_else(|| {
                        BeautifyError::BeautificationFailed("depth underflow".to_string())
                    })?;

                    if depth == 0 {
                        return Ok((Some(i), depth));
                    }
                }
            }
            TokenType::Semicolon | TokenType::Comma => {
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                    trace_ternary!("  -> hit statement boundary");
                    return Ok((None, depth));
                }
            }
            _ => {}
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    Ok((None, depth))
}

fn find_ternary_end(tokens: &[Token], start: usize) -> Result<usize> {
    debug_assert!(start < tokens.len(), "Start pos out of bounds");

    let mut i = start;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;

    while i < tokens.len() {
        match tokens[i].token_type {
            TokenType::StartExpr => {
                paren_depth = paren_depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("paren depth overflow".to_string())
                })?;
            }
            TokenType::EndExpr => {
                paren_depth = paren_depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("paren depth underflow".to_string())
                })?;
            }
            TokenType::StartArray => {
                bracket_depth = bracket_depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("bracket depth overflow".to_string())
                })?;
            }
            TokenType::EndArray => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("bracket depth underflow".to_string())
                })?;
            }
            TokenType::StartBlock => {
                brace_depth = brace_depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("brace depth overflow".to_string())
                })?;
            }
            TokenType::EndBlock => {
                brace_depth = brace_depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("brace depth underflow".to_string())
                })?;
            }
            TokenType::Semicolon | TokenType::Comma => {
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    Ok(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_simplify_true_ternary() {
        let code = "var x = true ? 1 : 2;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_ternary_chains(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("varx=1"),
            "Should keep true branch, got: {}",
            output
        );
        assert!(
            !output.contains("?"),
            "Should remove ternary operator, got: {}",
            output
        );
        assert!(
            !output.contains(":"),
            "Should remove colon, got: {}",
            output
        );
        assert!(
            !output.contains("2"),
            "Should remove false branch, got: {}",
            output
        );
    }

    #[test]
    fn test_simplify_false_ternary() {
        let code = "var x = false ? 1 : 2;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_ternary_chains(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("varx=2"),
            "Should keep false branch, got: {}",
            output
        );
        assert!(
            !output.contains("?"),
            "Should remove ternary operator, got: {}",
            output
        );
        assert!(
            !output.contains(":"),
            "Should remove colon, got: {}",
            output
        );
        assert!(
            !output.contains("1"),
            "Should remove true branch, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_non_constant_ternary() {
        let code = "var x = condition ? 1 : 2;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = simplify_ternary_chains(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("condition"),
            "Should keep non-constant condition, got: {}",
            output
        );
        assert!(
            output.contains("?"),
            "Should keep ternary operator, got: {}",
            output
        );
        assert!(output.contains(":"), "Should keep colon, got: {}", output);
    }
}
