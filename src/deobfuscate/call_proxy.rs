use crate::Result;
use crate::token::{Token, TokenType};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ProxyInfo {
    pub proxy_name: String,
    pub target_name: String,
    pub start_index: usize,
    pub end_index: usize,
}

pub fn detect_call_proxies(tokens: &[Token]) -> Result<Vec<ProxyInfo>> {
    let mut proxies = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some(proxy) = try_detect_proxy(tokens, i)? {
            debug_assert!(!proxy.proxy_name.is_empty(), "Proxy name cannot be empty");
            debug_assert!(!proxy.target_name.is_empty(), "Target name cannot be empty");
            debug_assert!(
                proxy.start_index <= proxy.end_index,
                "Invalid proxy indices"
            );
            proxies.push(proxy);
            i += 1;
        } else {
            i += 1;
        }
    }

    Ok(proxies)
}

fn try_detect_proxy(tokens: &[Token], pos: usize) -> Result<Option<ProxyInfo>> {
    if pos + 10 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "function" {
        return Ok(None);
    }

    if tokens[pos + 1].token_type != TokenType::Word {
        return Ok(None);
    }

    let proxy_name = tokens[pos + 1].text.clone();

    if tokens[pos + 2].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let _params_start = pos + 3;
    let mut params_end = None;
    let mut depth = 1;
    let mut i = pos + 3;
    let mut param_names = Vec::new();

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartExpr => depth += 1,
            TokenType::EndExpr => {
                depth -= 1;
                if depth == 0 {
                    params_end = Some(i);
                    break;
                }
            }
            TokenType::Word => {
                if depth == 1 {
                    param_names.push(tokens[i].text.clone());
                }
            }
            TokenType::Comma => {}
            _ => {}
        }
        i += 1;
    }

    let params_end = match params_end {
        Some(end) => end,
        None => return Ok(None),
    };

    let body_start = params_end + 1;
    if body_start >= tokens.len() || tokens[body_start].token_type != TokenType::StartBlock {
        return Ok(None);
    }

    let mut i = body_start + 1;
    if i >= tokens.len()
        || tokens[i].token_type != TokenType::Reserved
        || tokens[i].text != "return"
    {
        return Ok(None);
    }

    i += 1;
    if i >= tokens.len() || tokens[i].token_type != TokenType::Word {
        return Ok(None);
    }

    let target_name = tokens[i].text.clone();
    i += 1;

    if i >= tokens.len() || tokens[i].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    i += 1;
    let mut call_params = Vec::new();
    while i < tokens.len() && tokens[i].token_type != TokenType::EndExpr {
        if tokens[i].token_type == TokenType::Word {
            call_params.push(tokens[i].text.clone());
        }
        i += 1;
    }

    if call_params != param_names {
        return Ok(None);
    }

    i += 1;
    if i >= tokens.len() || tokens[i].token_type != TokenType::Semicolon {
        return Ok(None);
    }

    i += 1;
    if i >= tokens.len() || tokens[i].token_type != TokenType::EndBlock {
        return Ok(None);
    }

    let end_index = i;

    Ok(Some(ProxyInfo {
        proxy_name,
        target_name,
        start_index: pos,
        end_index,
    }))
}

pub fn inline_call_proxies(tokens: &[Token], proxies: &[ProxyInfo]) -> Result<Vec<Token>> {
    let mut proxy_map = HashMap::new();
    for proxy in proxies {
        proxy_map.insert(proxy.proxy_name.clone(), proxy.target_name.clone());
    }

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
            if let Some(target) = proxy_map.get(&tokens[i].text) {
                let mut new_token = tokens[i].clone();
                new_token.text = target.clone();
                result.push(new_token);
                i += 1;
                continue;
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
    fn test_detect_simple_proxy() {
        let code = r#"
function _0xabc(p) {
    return _0xdec(p);
}
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let proxies = detect_call_proxies(&tokens).unwrap();

        assert_eq!(proxies.len(), 1, "Should detect 1 proxy");
        assert_eq!(proxies[0].proxy_name, "_0xabc");
        assert_eq!(proxies[0].target_name, "_0xdec");
    }

    #[test]
    fn test_inline_proxy_calls() {
        let code = r#"
function _0xabc(p) {
    return _0xdec(p);
}
var x = _0xabc(123);
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let proxies = detect_call_proxies(&tokens).unwrap();
        let result = inline_call_proxies(&tokens, &proxies).unwrap();

        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("_0xdec"),
            "Should replace proxy with target, got: {}",
            output
        );
        assert!(
            !output.contains("function_0xabc"),
            "Should remove proxy function, got: {}",
            output
        );
    }

    #[test]
    fn test_multi_param_proxy() {
        let code = r#"
function _wrap(a, b, c) {
    return _target(a, b, c);
}
var result = _wrap(1, 2, 3);
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let proxies = detect_call_proxies(&tokens).unwrap();

        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].proxy_name, "_wrap");
        assert_eq!(proxies[0].target_name, "_target");

        let result = inline_call_proxies(&tokens, &proxies).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(output.contains("_target(1,2,3)"));
    }
}
