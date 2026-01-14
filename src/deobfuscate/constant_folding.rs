use crate::Result;
use crate::token::{Token, TokenType};

pub fn fold_constants(tokens: &[Token]) -> Result<Vec<Token>> {
    let mut current = tokens.to_vec();
    let mut pass = 0;

    loop {
        pass += 1;
        debug_assert!(pass < 10, "Too many folding passes, possible infinite loop");

        let mut result = Vec::new();
        let mut i = 0;
        let mut changed = false;

        while i < current.len() {
            if let Some((folded, skip)) = try_fold_at(&current, i)? {
                debug_assert!(skip > 0, "Skip must be positive");
                debug_assert!(skip <= 3, "Skip should not exceed 3");
                result.extend(folded);
                i += skip;
                changed = true;
            } else {
                result.push(current[i].clone());
                i += 1;
            }
        }

        if !changed {
            return Ok(result);
        }

        current = result;
    }
}

fn try_fold_at(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if let Some(result) = try_fold_comparison(tokens, pos)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_fold_logical(tokens, pos)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_fold_binary_operation(tokens, pos)? {
        return Ok(Some(result));
    }

    if let Some(result) = try_fold_unary_operation(tokens, pos)? {
        return Ok(Some(result));
    }

    Ok(None)
}

fn try_fold_comparison(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 2 >= tokens.len() {
        return Ok(None);
    }

    let left = &tokens[pos];
    let op = &tokens[pos + 1];
    let right = &tokens[pos + 2];

    if left.token_type != TokenType::Number {
        return Ok(None);
    }

    if op.token_type != TokenType::Operator {
        return Ok(None);
    }

    if right.token_type != TokenType::Number {
        return Ok(None);
    }

    let left_val = match parse_number(&left.text) {
        Some(v) => {
            debug_assert!(
                v != 0 || left.text == "0",
                "Zero should only parse from '0'"
            );
            v
        }
        None => return Ok(None),
    };

    let right_val = match parse_number(&right.text) {
        Some(v) => {
            debug_assert!(
                v != 0 || right.text == "0",
                "Zero should only parse from '0'"
            );
            v
        }
        None => return Ok(None),
    };

    let result = match op.text.as_str() {
        "===" | "==" => {
            let res = left_val == right_val;
            debug_assert!((left_val == right_val) == res, "Comparison result mismatch");
            res
        }
        "!==" | "!=" => {
            let res = left_val != right_val;
            debug_assert!((left_val != right_val) == res, "Comparison result mismatch");
            res
        }
        "<" => left_val < right_val,
        "<=" => left_val <= right_val,
        ">" => left_val > right_val,
        ">=" => left_val >= right_val,
        _ => return Ok(None),
    };

    let result_token = Token::new(
        TokenType::Reserved,
        if result { "true" } else { "false" }.to_string(),
    );

    Ok(Some((vec![result_token], 3)))
}

fn try_fold_logical(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 2 >= tokens.len() {
        return Ok(None);
    }

    let left = &tokens[pos];
    let op = &tokens[pos + 1];
    let right = &tokens[pos + 2];

    if left.token_type != TokenType::Reserved {
        return Ok(None);
    }

    if op.token_type != TokenType::Operator {
        return Ok(None);
    }

    if right.token_type != TokenType::Reserved {
        return Ok(None);
    }

    let left_bool = match left.text.as_str() {
        "true" => true,
        "false" => false,
        _ => return Ok(None),
    };

    let right_bool = match right.text.as_str() {
        "true" => true,
        "false" => false,
        _ => return Ok(None),
    };

    let result = match op.text.as_str() {
        "&&" => left_bool && right_bool,
        "||" => left_bool || right_bool,
        _ => return Ok(None),
    };

    let result_token = Token::new(
        TokenType::Reserved,
        if result { "true" } else { "false" }.to_string(),
    );

    Ok(Some((vec![result_token], 3)))
}

fn try_fold_binary_operation(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 2 >= tokens.len() {
        return Ok(None);
    }

    let left = &tokens[pos];
    let op = &tokens[pos + 1];
    let right = &tokens[pos + 2];

    if left.token_type != TokenType::Number {
        return Ok(None);
    }

    if op.token_type != TokenType::Operator {
        return Ok(None);
    }

    if right.token_type != TokenType::Number {
        return Ok(None);
    }

    let left_val = match parse_number(&left.text) {
        Some(v) => {
            debug_assert!(v.abs() < i64::MAX / 2, "Left value too large: {}", v);
            v
        }
        None => return Ok(None),
    };

    let right_val = match parse_number(&right.text) {
        Some(v) => {
            debug_assert!(v.abs() < i64::MAX / 2, "Right value too large: {}", v);
            v
        }
        None => return Ok(None),
    };

    let result_val = match op.text.as_str() {
        "+" => {
            let result = left_val.checked_add(right_val);
            debug_assert!(
                result.is_some(),
                "Overflow in addition: {} + {}",
                left_val,
                right_val
            );
            result.unwrap_or(left_val + right_val)
        }
        "-" => {
            let result = left_val.checked_sub(right_val);
            debug_assert!(
                result.is_some(),
                "Overflow in subtraction: {} - {}",
                left_val,
                right_val
            );
            result.unwrap_or(left_val - right_val)
        }
        "*" => {
            let result = left_val.checked_mul(right_val);
            debug_assert!(
                result.is_some(),
                "Overflow in multiplication: {} * {}",
                left_val,
                right_val
            );
            result.unwrap_or(left_val * right_val)
        }
        "/" => {
            if right_val == 0 {
                return Ok(None);
            }
            debug_assert!(
                right_val != 0,
                "Division by zero: {} / {}",
                left_val,
                right_val
            );
            left_val / right_val
        }
        "%" => {
            if right_val == 0 {
                return Ok(None);
            }
            debug_assert!(
                right_val != 0,
                "Modulo by zero: {} % {}",
                left_val,
                right_val
            );
            left_val % right_val
        }
        "&" => left_val & right_val,
        "|" => left_val | right_val,
        "^" => left_val ^ right_val,
        "<<" => {
            let shift_amount = right_val & 0x1F;
            debug_assert!(
                shift_amount >= 0 && shift_amount < 32,
                "Shift amount out of range: {}",
                shift_amount
            );
            left_val << shift_amount
        }
        ">>" => {
            let shift_amount = right_val & 0x1F;
            debug_assert!(
                shift_amount >= 0 && shift_amount < 32,
                "Shift amount out of range: {}",
                shift_amount
            );
            left_val >> shift_amount
        }
        ">>>" => {
            let shift_amount = right_val & 0x1F;
            debug_assert!(
                shift_amount >= 0 && shift_amount < 32,
                "Shift amount out of range: {}",
                shift_amount
            );
            ((left_val as u64) >> shift_amount) as i64
        }
        _ => return Ok(None),
    };

    let result_token = Token::new(TokenType::Number, result_val.to_string());

    debug_assert!(
        result_val.abs() < i64::MAX / 2,
        "Result value too large: {}",
        result_val
    );

    Ok(Some((vec![result_token], 3)))
}

fn try_fold_unary_operation(tokens: &[Token], pos: usize) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 1 >= tokens.len() {
        return Ok(None);
    }

    let op = &tokens[pos];
    let operand = &tokens[pos + 1];

    if op.token_type != TokenType::Operator {
        return Ok(None);
    }

    if operand.token_type != TokenType::Number {
        return Ok(None);
    }

    let val = match parse_number(&operand.text) {
        Some(v) => {
            debug_assert!(v.abs() < i64::MAX, "Operand value too large: {}", v);
            v
        }
        None => return Ok(None),
    };

    let result_val = match op.text.as_str() {
        "-" => {
            let result = val.checked_neg();
            debug_assert!(result.is_some(), "Overflow in negation: -{}", val);
            result.unwrap_or(-val)
        }
        "~" => {
            let result = !val;
            debug_assert!(result != val, "Bitwise NOT should change value");
            result
        }
        _ => return Ok(None),
    };

    let result_token = Token::new(TokenType::Number, result_val.to_string());

    debug_assert!(
        result_val.abs() < i64::MAX,
        "Result value too large: {}",
        result_val
    );

    Ok(Some((vec![result_token], 2)))
}

fn parse_number(s: &str) -> Option<i64> {
    if s.starts_with("0x") || s.starts_with("0X") {
        i64::from_str_radix(&s[2..], 16).ok()
    } else if s.starts_with("0b") || s.starts_with("0B") {
        i64::from_str_radix(&s[2..], 2).ok()
    } else if s.starts_with("0o") || s.starts_with("0O") {
        i64::from_str_radix(&s[2..], 8).ok()
    } else {
        s.parse::<i64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_fold_addition() {
        let code = "var x = 5 + 10;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = fold_constants(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("15"),
            "Should fold 5 + 10 to 15, got: {}",
            output
        );
        assert!(
            !output.contains("5+10") && !output.contains("5 + 10"),
            "Should not contain original expression, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_multiplication() {
        let code = "var result = 5 * 16;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = fold_constants(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("80"),
            "Should fold 5 * 16 to 80, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_hex_after_simplify() {
        use crate::deobfuscate::simplify;

        let code = "var result = 0x5 * 0x10;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let simplified = simplify::simplify_expressions(&tokens).unwrap();
        let result = fold_constants(&simplified).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("80"),
            "Should fold 0x5 * 0x10 to 80 (after simplify), got: {}",
            output
        );
    }

    #[test]
    fn test_fold_bitwise_operations() {
        let code = "var a = 12 & 10; var b = 12 | 10; var c = 12 ^ 10;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = fold_constants(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("8"),
            "Should fold 12 & 10 to 8, got: {}",
            output
        );
        assert!(
            output.contains("14"),
            "Should fold 12 | 10 to 14, got: {}",
            output
        );
        assert!(
            output.contains("6"),
            "Should fold 12 ^ 10 to 6, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_shift_operations() {
        let code = "var left = 5 << 2; var right = 20 >> 2;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = fold_constants(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("20"),
            "Should fold 5 << 2 to 20, got: {}",
            output
        );
        assert!(
            output.contains("5"),
            "Should fold 20 >> 2 to 5, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_unary_negation() {
        let code = "var x = -42;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = fold_constants(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(output.contains("-42"), "Should keep -42, got: {}", output);
    }

    #[test]
    fn test_opaque_predicate_example() {
        use crate::deobfuscate::simplify;

        let code = "var check = 0x123 * 0x456;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let simplified = simplify::simplify_expressions(&tokens).unwrap();
        let result = fold_constants(&simplified).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        let expected = 0x123 * 0x456;
        assert!(
            output.contains(&expected.to_string()),
            "Should fold 0x123 * 0x456 to {} (after simplify), got: {}",
            expected,
            output
        );

        debug_assert_eq!(expected, 323010, "Expected value should be 323010");
    }

    #[test]
    fn test_fold_comparison_operators() {
        let code = "var a = 5 > 3; var b = 10 == 10; var c = 7 < 2;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = fold_constants(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("true"),
            "Should fold 5 > 3 to true, got: {}",
            output
        );
        assert!(
            output.contains("false"),
            "Should fold 7 < 2 to false, got: {}",
            output
        );
    }

    #[test]
    fn test_fold_logical_operators() {
        let code = "var a = true && false; var b = true || false;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = fold_constants(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("false"),
            "Should fold true && false to false, got: {}",
            output
        );
        assert!(
            output.contains("true"),
            "Should fold true || false to true, got: {}",
            output
        );
    }

    #[test]
    fn test_opaque_predicate_detection() {
        use crate::deobfuscate::simplify;

        let code = "if (0x123 * 0x456 === 0x4edc2) { runCode(); }";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let simplified = simplify::simplify_expressions(&tokens).unwrap();
        let result = fold_constants(&simplified).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("true"),
            "Should fold opaque predicate 0x123 * 0x456 === 0x4edc2 to true, got: {}",
            output
        );
    }
}
