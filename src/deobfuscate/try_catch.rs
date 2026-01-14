use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};

#[cfg(debug_assertions)]
macro_rules! trace_try {
    ($($arg:tt)*) => {
        eprintln!("[TRY] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_try {
    ($($arg:tt)*) => {};
}

pub fn remove_empty_try_catch(tokens: &[Token]) -> Result<Vec<Token>> {
    trace_try!("=== REMOVING EMPTY TRY-CATCH BLOCKS ===");
    trace_try!("total tokens: {}", tokens.len());

    let mut result = Vec::new();
    let mut i = 0usize;
    let mut removed_count = 0usize;

    while i < tokens.len() {
        if let Some(skip) = try_remove_at(tokens, i, &mut result)? {
            debug_assert!(skip > 0, "Skip must be positive");
            debug_assert!(skip < 10000, "Skip suspiciously large: {}", skip);

            i = i
                .checked_add(skip)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

            removed_count = removed_count
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

    trace_try!("removed {} empty try-catch blocks", removed_count);
    trace_try!("final token count: {} -> {}", tokens.len(), result.len());

    Ok(result)
}

fn try_remove_at(tokens: &[Token], pos: usize, result: &mut Vec<Token>) -> Result<Option<usize>> {
    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "try" {
        return Ok(None);
    }

    trace_try!("found 'try' at position {}", pos);

    let try_body_start = pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if try_body_start >= tokens.len() {
        trace_try!("  -> not enough tokens after 'try'");
        return Ok(None);
    }

    if tokens[try_body_start].token_type != TokenType::StartBlock {
        trace_try!("  -> 'try' not followed by {{");
        return Ok(None);
    }

    let try_body_end = find_matching_brace(tokens, try_body_start)?;

    debug_assert!(
        try_body_end > try_body_start,
        "Try body end must be after start"
    );

    let catch_pos = try_body_end
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if catch_pos >= tokens.len() {
        trace_try!("  -> no tokens after try block");
        return Ok(None);
    }

    if tokens[catch_pos].token_type != TokenType::Reserved || tokens[catch_pos].text != "catch" {
        trace_try!("  -> try block not followed by 'catch'");
        return Ok(None);
    }

    trace_try!("found 'catch' at position {}", catch_pos);

    let catch_param_start = catch_pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if catch_param_start >= tokens.len() {
        trace_try!("  -> no tokens after 'catch'");
        return Ok(None);
    }

    if tokens[catch_param_start].token_type != TokenType::StartExpr {
        trace_try!("  -> 'catch' not followed by (");
        return Ok(None);
    }

    let catch_param_end = find_matching_paren(tokens, catch_param_start)?;

    let catch_body_start = catch_param_end
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if catch_body_start >= tokens.len() {
        trace_try!("  -> no tokens after catch parameters");
        return Ok(None);
    }

    if tokens[catch_body_start].token_type != TokenType::StartBlock {
        trace_try!("  -> catch parameters not followed by {{");
        return Ok(None);
    }

    let catch_body_end = find_matching_brace(tokens, catch_body_start)?;

    debug_assert!(
        catch_body_end > catch_body_start,
        "Catch body end must be after start"
    );

    let is_catch_empty = is_block_empty(tokens, catch_body_start, catch_body_end)?;

    if !is_catch_empty {
        trace_try!("  -> catch block is not empty, keeping try-catch");
        return Ok(None);
    }

    trace_try!("  -> catch block is empty, extracting try body");

    for idx in (try_body_start
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?)
        ..try_body_end
    {
        result.push(tokens[idx].clone());
    }

    let skip = catch_body_end
        .checked_sub(pos)
        .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    trace_try!(
        "removed try-catch (from {} to {}), extracted {} tokens from try body",
        pos,
        catch_body_end,
        try_body_end
            .saturating_sub(try_body_start)
            .saturating_sub(1)
    );

    Ok(Some(skip))
}

fn find_matching_brace(tokens: &[Token], start: usize) -> Result<usize> {
    debug_assert!(start < tokens.len(), "Start position out of bounds");
    debug_assert!(
        tokens[start].token_type == TokenType::StartBlock,
        "Must start at opening brace"
    );

    let mut depth = 1usize;
    let mut i = start
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartBlock => {
                depth = depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("depth overflow".to_string())
                })?;
            }
            TokenType::EndBlock => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("depth underflow".to_string())
                })?;

                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    Err(BeautifyError::BeautificationFailed(
        "Unmatched brace".to_string(),
    ))
}

fn find_matching_paren(tokens: &[Token], start: usize) -> Result<usize> {
    debug_assert!(start < tokens.len(), "Start position out of bounds");
    debug_assert!(
        tokens[start].token_type == TokenType::StartExpr,
        "Must start at opening paren"
    );

    let mut depth = 1usize;
    let mut i = start
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartExpr => {
                depth = depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("depth overflow".to_string())
                })?;
            }
            TokenType::EndExpr => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("depth underflow".to_string())
                })?;

                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    Err(BeautifyError::BeautificationFailed(
        "Unmatched paren".to_string(),
    ))
}

fn is_block_empty(tokens: &[Token], start: usize, end: usize) -> Result<bool> {
    debug_assert!(start < end, "Start must be before end");
    debug_assert!(start < tokens.len(), "Start out of bounds");
    debug_assert!(end < tokens.len(), "End out of bounds");

    let body_start = start
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if body_start >= end {
        return Ok(true);
    }

    for idx in body_start..end {
        let token = &tokens[idx];

        if token.token_type == TokenType::Eof {
            continue;
        }

        trace_try!(
            "  -> checking token[{}]: {:?} = '{}'",
            idx,
            token.token_type,
            token.text
        );

        return Ok(false);
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_remove_empty_catch() {
        let code = r#"try { var x = 1; } catch(e) {}"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = remove_empty_try_catch(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            !output.contains("try"),
            "Should remove try keyword, got: {}",
            output
        );
        assert!(
            !output.contains("catch"),
            "Should remove catch keyword, got: {}",
            output
        );
        assert!(
            output.contains("varx=1"),
            "Should keep try body, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_non_empty_catch() {
        let code = r#"try { var x = 1; } catch(e) { console.log(e); }"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = remove_empty_try_catch(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("try"),
            "Should keep try with non-empty catch, got: {}",
            output
        );
        assert!(
            output.contains("catch"),
            "Should keep non-empty catch, got: {}",
            output
        );
    }

    #[test]
    fn test_multiple_empty_catches() {
        let code = r#"
            try { var x = 1; } catch(e) {}
            try { var y = 2; } catch(err) {}
        "#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = remove_empty_try_catch(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            !output.contains("try"),
            "Should remove all try keywords, got: {}",
            output
        );
        assert!(
            !output.contains("catch"),
            "Should remove all catch keywords, got: {}",
            output
        );
        assert!(
            output.contains("varx=1"),
            "Should keep first try body, got: {}",
            output
        );
        assert!(
            output.contains("vary=2"),
            "Should keep second try body, got: {}",
            output
        );
    }
}
