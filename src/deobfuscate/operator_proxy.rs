use crate::Result;
use crate::token::{Token, TokenType};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct OperatorProxyInfo {
    pub function_name: String,
    pub operator: String,
    pub param1: String,
    pub param2: String,
    pub start_index: usize,
    pub end_index: usize,
}

pub fn detect_operator_proxies(tokens: &[Token]) -> Result<Vec<OperatorProxyInfo>> {
    let mut proxies = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some(proxy) = try_detect_operator_proxy(tokens, i)? {
            debug_assert!(
                !proxy.function_name.is_empty(),
                "Function name cannot be empty"
            );
            debug_assert!(!proxy.operator.is_empty(), "Operator cannot be empty");
            proxies.push(proxy);
            i += 1;
        } else {
            i += 1;
        }
    }

    Ok(proxies)
}

fn try_detect_operator_proxy(tokens: &[Token], pos: usize) -> Result<Option<OperatorProxyInfo>> {
    if pos + 14 > tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "function" {
        return Ok(None);
    }

    if tokens[pos + 1].token_type != TokenType::Word {
        return Ok(None);
    }

    let function_name = tokens[pos + 1].text.clone();

    if tokens[pos + 2].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    if tokens[pos + 3].token_type != TokenType::Word {
        return Ok(None);
    }

    let param1 = tokens[pos + 3].text.clone();

    if tokens[pos + 4].token_type != TokenType::Comma {
        return Ok(None);
    }

    if tokens[pos + 5].token_type != TokenType::Word {
        return Ok(None);
    }

    let param2 = tokens[pos + 5].text.clone();

    if tokens[pos + 6].token_type != TokenType::EndExpr {
        return Ok(None);
    }

    if tokens[pos + 7].token_type != TokenType::StartBlock {
        return Ok(None);
    }

    if tokens[pos + 8].token_type != TokenType::Reserved || tokens[pos + 8].text != "return" {
        return Ok(None);
    }

    if tokens[pos + 9].token_type != TokenType::Word || tokens[pos + 9].text != param1 {
        return Ok(None);
    }

    if tokens[pos + 10].token_type != TokenType::Operator {
        return Ok(None);
    }

    let operator = tokens[pos + 10].text.clone();

    if tokens[pos + 11].token_type != TokenType::Word || tokens[pos + 11].text != param2 {
        return Ok(None);
    }

    if tokens[pos + 12].token_type != TokenType::Semicolon {
        return Ok(None);
    }

    if tokens[pos + 13].token_type != TokenType::EndBlock {
        return Ok(None);
    }

    let end_index = pos + 13;

    debug_assert!(
        !function_name.is_empty(),
        "Function name should not be empty"
    );
    debug_assert!(!operator.is_empty(), "Operator should not be empty");
    debug_assert!(!param1.is_empty(), "Param1 should not be empty");
    debug_assert!(!param2.is_empty(), "Param2 should not be empty");

    Ok(Some(OperatorProxyInfo {
        function_name,
        operator,
        param1,
        param2,
        start_index: pos,
        end_index,
    }))
}

pub fn inline_operator_proxies(
    tokens: &[Token],
    proxies: &[OperatorProxyInfo],
) -> Result<Vec<Token>> {
    let proxy_map: HashMap<String, String> = proxies
        .iter()
        .map(|p| (p.function_name.clone(), p.operator.clone()))
        .collect();

    let proxy_ranges: Vec<(usize, usize)> = proxies
        .iter()
        .map(|p| (p.start_index, p.end_index))
        .collect();

    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let in_proxy_range = proxy_ranges
            .iter()
            .any(|(start, end)| i >= *start && i <= *end);

        if in_proxy_range {
            i += 1;
            continue;
        }

        if tokens[i].token_type == TokenType::Word {
            if let Some(operator) = proxy_map.get(&tokens[i].text) {
                if i + 5 < tokens.len()
                    && tokens[i + 1].token_type == TokenType::StartExpr
                    && tokens[i + 3].token_type == TokenType::Comma
                    && tokens[i + 5].token_type == TokenType::EndExpr
                {
                    let arg1 = tokens[i + 2].clone();
                    let arg2 = tokens[i + 4].clone();

                    result.push(arg1);
                    result.push(Token::new(TokenType::Operator, operator.clone()));
                    result.push(arg2);

                    i += 6;
                    continue;
                }
            }
        }

        result.push(tokens[i].clone());
        i += 1;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_detect_add_proxy() {
        let code = r#"
function _0xadd(a, b) {
    return a + b;
}
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let proxies = detect_operator_proxies(&tokens).unwrap();

        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].function_name, "_0xadd");
        assert_eq!(proxies[0].operator, "+");
        assert_eq!(proxies[0].param1, "a");
        assert_eq!(proxies[0].param2, "b");
    }

    #[test]
    fn test_inline_operator_proxy() {
        let code = r#"
function _0xadd(a, b) {
    return a + b;
}
var x = _0xadd(5, 10);
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let proxies = detect_operator_proxies(&tokens).unwrap();
        let result = inline_operator_proxies(&tokens, &proxies).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("5+10"),
            "Should inline to 5+10, got: {}",
            output
        );
        assert!(
            !output.contains("_0xadd"),
            "Should remove function, got: {}",
            output
        );
    }

    #[test]
    fn test_multiple_operators() {
        let code = r#"
function _add(a, b) {
    return a + b;
}
function _mul(a, b) {
    return a * b;
}
var x = _add(1, 2);
var y = _mul(3, 4);
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let proxies = detect_operator_proxies(&tokens).unwrap();

        assert_eq!(proxies.len(), 2);

        let result = inline_operator_proxies(&tokens, &proxies).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(output.contains("1+2"));
        assert!(output.contains("3*4"));
    }
}
