use crate::Result;
use crate::token::{Token, TokenType};

pub fn remove_dead_code_conditionals(tokens: &[Token]) -> Result<Vec<Token>> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some((unwrapped, skip)) = try_unwrap_always_true_if(tokens, i)? {
            result.extend(unwrapped);
            i += skip;
        } else if let Some(skip) = try_remove_dead_if(tokens, i)? {
            i += skip;
        } else {
            result.push(tokens[i].clone());
            i += 1;
        }
    }

    Ok(result)
}

fn try_remove_dead_if(tokens: &[Token], pos: usize) -> Result<Option<usize>> {
    if pos + 3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "if" {
        return Ok(None);
    }

    if tokens[pos + 1].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let condition_start = pos + 2;
    let mut depth = 1;
    let mut condition_end = None;
    let mut i = pos + 2;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartExpr => depth += 1,
            TokenType::EndExpr => {
                depth -= 1;
                if depth == 0 {
                    condition_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let condition_end = match condition_end {
        Some(end) => end,
        None => return Ok(None),
    };

    let is_false = is_constant_false(&tokens[condition_start..condition_end])?;

    if !is_false {
        return Ok(None);
    }

    let body_start = condition_end + 1;
    if body_start >= tokens.len() {
        return Ok(None);
    }

    let (_block_end, total_skip) = if tokens[body_start].token_type == TokenType::StartBlock {
        let mut depth = 1;
        let mut i = body_start + 1;

        while i < tokens.len() && depth > 0 {
            match tokens[i].token_type {
                TokenType::StartBlock => depth += 1,
                TokenType::EndBlock => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(Some(i - pos + 1));
                    }
                }
                _ => {}
            }
            i += 1;
        }

        return Ok(None);
    } else {
        let mut i = body_start;
        while i < tokens.len() && tokens[i].token_type != TokenType::Semicolon {
            i += 1;
        }

        if i < tokens.len() {
            (i, i - pos + 1)
        } else {
            return Ok(None);
        }
    };

    Ok(Some(total_skip))
}

fn try_unwrap_always_true_if(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 3 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "if" {
        return Ok(None);
    }

    if tokens[pos + 1].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let condition_start = pos + 2;
    let mut depth = 1;
    let mut condition_end = None;
    let mut i = pos + 2;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartExpr => depth += 1,
            TokenType::EndExpr => {
                depth -= 1;
                if depth == 0 {
                    condition_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let condition_end = match condition_end {
        Some(end) => end,
        None => return Ok(None),
    };

    let is_true = is_constant_true(&tokens[condition_start..condition_end])?;

    if !is_true {
        return Ok(None);
    }

    let body_start = condition_end + 1;
    if body_start >= tokens.len() {
        return Ok(None);
    }

    if tokens[body_start].token_type == TokenType::StartBlock {
        let mut depth = 1;
        let mut i = body_start + 1;
        let mut body_tokens = Vec::new();

        while i < tokens.len() && depth > 0 {
            match tokens[i].token_type {
                TokenType::StartBlock => {
                    depth += 1;
                    body_tokens.push(tokens[i].clone());
                }
                TokenType::EndBlock => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(Some((body_tokens, i - pos + 1)));
                    }
                    body_tokens.push(tokens[i].clone());
                }
                _ => {
                    body_tokens.push(tokens[i].clone());
                }
            }
            i += 1;
        }
    }

    Ok(None)
}

fn is_constant_true(condition: &[Token]) -> Result<bool> {
    if condition.is_empty() {
        return Ok(false);
    }

    if condition.len() == 1 {
        match condition[0].token_type {
            TokenType::Word | TokenType::Reserved if condition[0].text == "true" => {
                return Ok(true);
            }
            TokenType::Number if condition[0].text != "0" => return Ok(true),
            _ => {}
        }
    }

    Ok(false)
}

fn is_constant_false(condition: &[Token]) -> Result<bool> {
    if condition.is_empty() {
        return Ok(false);
    }

    if condition.len() == 1 {
        match condition[0].token_type {
            TokenType::Word | TokenType::Reserved if condition[0].text == "false" => {
                return Ok(true);
            }
            TokenType::Number if condition[0].text == "0" => return Ok(true),
            _ => {}
        }
    }

    if condition.len() == 2 {
        if condition[0].token_type == TokenType::Operator && condition[0].text == "!" {
            let is_true_token = (condition[1].token_type == TokenType::Word
                || condition[1].token_type == TokenType::Reserved)
                && condition[1].text == "true";

            if is_true_token {
                return Ok(true);
            }
        }
    }

    if condition.len() == 2 {
        if condition[0].token_type == TokenType::Operator && condition[0].text == "!" {
            if condition[1].token_type == TokenType::StartArray {
                if condition.len() >= 3 && condition[2].token_type == TokenType::EndArray {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_remove_if_false() {
        let code = "var x = 1; if (false) { console.log('dead'); } var y = 2;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        eprintln!("\n=== Original Tokens (if false) ===");
        for (i, token) in tokens.iter().enumerate() {
            eprintln!("Token {}: {:?} = '{}'", i, token.token_type, token.text);
        }

        let result = remove_dead_code_conditionals(&tokens).unwrap();

        eprintln!("\n=== Result Tokens ===");
        for (i, token) in result.iter().enumerate() {
            eprintln!("Token {}: {:?} = '{}'", i, token.token_type, token.text);
        }

        let output: String = result.iter().map(|t| t.text.as_str()).collect();
        eprintln!("Output: {}", output);

        assert!(
            !output.contains("if"),
            "Should remove if statement, got: {}",
            output
        );
        assert!(
            !output.contains("console.log"),
            "Should remove dead code block, got: {}",
            output
        );
        assert!(output.contains("x"), "Should keep var x, got: {}", output);
        assert!(output.contains("y"), "Should keep var y, got: {}", output);
    }

    #[test]
    fn test_unwrap_if_true() {
        let code = "before(); if (true) { inside(); } after();";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = remove_dead_code_conditionals(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            !output.contains("if"),
            "Should remove if statement, got: {}",
            output
        );
        assert!(
            output.contains("inside"),
            "Should keep body code, got: {}",
            output
        );
        assert!(
            output.contains("before"),
            "Should keep before code, got: {}",
            output
        );
        assert!(
            output.contains("after"),
            "Should keep after code, got: {}",
            output
        );
    }

    #[test]
    fn test_opaque_predicate_after_folding() {
        use crate::deobfuscate::{constant_folding, simplify};

        let code = "var x = 1; if (5 > 3) { x = 2; } if (10 < 5) { x = 99; } var y = x;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let simplified = simplify::simplify_expressions(&tokens).unwrap();
        let folded = constant_folding::fold_constants(&simplified).unwrap();
        let result = remove_dead_code_conditionals(&folded).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("x=2"),
            "Should keep if(true) body, got: {}",
            output
        );
        assert!(
            !output.contains("99"),
            "Should remove if(false) body, got: {}",
            output
        );
    }

    #[test]
    fn test_remove_if_zero() {
        let code = "if (0) { deadCode(); } liveCode();";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = remove_dead_code_conditionals(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        eprintln!("if (0) output: {}", output);

        assert!(
            !output.contains("if"),
            "Should remove if statement, got: {}",
            output
        );
        assert!(
            !output.contains("deadCode"),
            "Should remove dead code, got: {}",
            output
        );
        assert!(
            output.contains("liveCode"),
            "Should keep live code, got: {}",
            output
        );
    }

    #[test]
    fn test_remove_if_not_true() {
        let code = "if (!true) { deadCode(); } liveCode();";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = remove_dead_code_conditionals(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        eprintln!("if (!true) output: {}", output);

        assert!(
            !output.contains("deadCode"),
            "Should remove dead code, got: {}",
            output
        );
        assert!(
            output.contains("liveCode"),
            "Should keep live code, got: {}",
            output
        );
    }
}
