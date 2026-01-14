use crate::Result;
use crate::token::{Token, TokenType};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DispatcherInfo {
    pub var_name: String,
    pub start_index: usize,
    pub end_index: usize,
    pub functions: HashMap<String, FunctionInfo>,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub key: String,
    pub return_value: Option<Token>,
    pub params: Vec<String>,
    pub body_tokens: Vec<Token>,
}

pub fn detect_object_dispatchers(tokens: &[Token]) -> Result<Vec<DispatcherInfo>> {
    let mut dispatchers = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some(dispatcher) = try_detect_dispatcher_at(tokens, i)? {
            eprintln!(
                "\n=== Found Dispatcher '{}' at pos {} ===",
                dispatcher.var_name, i
            );
            eprintln!("Functions in dispatcher: {}", dispatcher.functions.len());
            for (key, func) in &dispatcher.functions {
                eprintln!(
                    "  - Key '{}': return_value = {:?}",
                    key,
                    func.return_value.as_ref().map(|t| &t.text)
                );
            }
            dispatchers.push(dispatcher);
        }
        i += 1;
    }

    eprintln!("\nTotal dispatchers found: {}", dispatchers.len());
    Ok(dispatchers)
}

fn try_detect_dispatcher_at(tokens: &[Token], pos: usize) -> Result<Option<DispatcherInfo>> {
    if pos + 5 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "var" {
        return Ok(None);
    }

    if tokens[pos + 1].token_type != TokenType::Word {
        return Ok(None);
    }

    let var_name = tokens[pos + 1].text.clone();

    if tokens[pos + 2].token_type != TokenType::Equals {
        return Ok(None);
    }

    if tokens[pos + 3].token_type != TokenType::StartBlock {
        return Ok(None);
    }

    let obj_start = pos + 3;
    let mut depth = 1;
    let mut i = pos + 4;
    let mut obj_end = None;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartBlock => depth += 1,
            TokenType::EndBlock => {
                depth -= 1;
                if depth == 0 {
                    obj_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let obj_end = match obj_end {
        Some(end) => end,
        None => return Ok(None),
    };

    let functions = parse_object_functions(&tokens[obj_start + 1..obj_end])?;

    if functions.is_empty() {
        return Ok(None);
    }

    let has_dispatcher_pattern = functions.len() >= 1
        && functions
            .values()
            .all(|f| f.return_value.is_some() || !f.body_tokens.is_empty());

    if !has_dispatcher_pattern {
        return Ok(None);
    }

    Ok(Some(DispatcherInfo {
        var_name,
        start_index: pos,
        end_index: obj_end,
        functions,
    }))
}

fn parse_object_functions(tokens: &[Token]) -> Result<HashMap<String, FunctionInfo>> {
    let mut functions = HashMap::new();
    let mut i = 0;

    eprintln!("\n=== Parsing Object Functions ===");
    eprintln!("Token count: {}", tokens.len());

    while i < tokens.len() {
        eprintln!(
            "  Token {}: {:?} = '{}'",
            i, tokens[i].token_type, tokens[i].text
        );

        if tokens[i].token_type == TokenType::String {
            let key = tokens[i]
                .text
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            eprintln!("    Found key: '{}'", key);

            if i + 1 < tokens.len() && tokens[i + 1].token_type == TokenType::Colon {
                if i + 2 < tokens.len()
                    && tokens[i + 2].token_type == TokenType::Reserved
                    && tokens[i + 2].text == "function"
                {
                    eprintln!("    Found function for key '{}'", key);

                    if let Some((func_info, consumed)) = parse_function(&tokens[i + 2..])? {
                        eprintln!(
                            "    Parsed function: params={}, return_value={:?}",
                            func_info.params.len(),
                            func_info.return_value.as_ref().map(|t| &t.text)
                        );

                        functions.insert(key.clone(), func_info);
                        i += 2 + consumed;

                        if i < tokens.len() && tokens[i].token_type == TokenType::Comma {
                            i += 1;
                        }
                        continue;
                    }
                }
            }
        }

        i += 1;
    }

    eprintln!("Total functions parsed: {}", functions.len());
    Ok(functions)
}

fn parse_function(tokens: &[Token]) -> Result<Option<(FunctionInfo, usize)>> {
    if tokens[0].text != "function" {
        return Ok(None);
    }

    if tokens.len() < 4 {
        return Ok(None);
    }

    if tokens[1].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let mut params = Vec::new();
    let mut i = 2;

    while i < tokens.len() && tokens[i].token_type != TokenType::EndExpr {
        if tokens[i].token_type == TokenType::Word {
            params.push(tokens[i].text.clone());
        }
        i += 1;
    }

    if i >= tokens.len() || tokens[i].token_type != TokenType::EndExpr {
        return Ok(None);
    }

    i += 1;

    if i >= tokens.len() || tokens[i].token_type != TokenType::StartBlock {
        return Ok(None);
    }

    let body_start = i + 1;
    let mut depth = 1;
    i += 1;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartBlock => depth += 1,
            TokenType::EndBlock => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let body_end = i;
    let body_tokens: Vec<Token> = tokens[body_start..body_end].to_vec();

    let return_value = extract_simple_return_value(&body_tokens)?;

    Ok(Some((
        FunctionInfo {
            key: String::new(),
            return_value,
            params,
            body_tokens,
        },
        body_end + 1,
    )))
}

fn extract_simple_return_value(body: &[Token]) -> Result<Option<Token>> {
    if body.len() < 2 {
        return Ok(None);
    }

    if body[0].token_type == TokenType::Reserved && body[0].text == "return" {
        if body.len() >= 2 {
            match body[1].token_type {
                TokenType::String | TokenType::Number | TokenType::Word => {
                    return Ok(Some(body[1].clone()));
                }
                _ => {}
            }
        }
    }

    Ok(None)
}

pub fn inline_dispatcher_calls(
    tokens: &[Token],
    dispatchers: &[DispatcherInfo],
) -> Result<Vec<Token>> {
    let mut result = Vec::new();
    let mut i = 0;

    eprintln!("\n=== Inlining Dispatcher Calls ===");
    eprintln!("Total dispatchers: {}", dispatchers.len());

    while i < tokens.len() {
        if let Some((inlined, skip)) = try_inline_call(tokens, i, dispatchers)? {
            result.extend(inlined);
            i += skip;
        } else {
            result.push(tokens[i].clone());
            i += 1;
        }
    }

    Ok(result)
}

fn try_inline_call(
    tokens: &[Token],
    pos: usize,
    dispatchers: &[DispatcherInfo],
) -> Result<Option<(Vec<Token>, usize)>> {
    if pos + 5 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type != TokenType::Word {
        return Ok(None);
    }

    let var_name = &tokens[pos].text;

    let dispatcher = match dispatchers.iter().find(|d| &d.var_name == var_name) {
        Some(d) => d,
        None => return Ok(None),
    };

    if tokens[pos + 1].token_type != TokenType::StartArray {
        return Ok(None);
    }

    if tokens[pos + 2].token_type != TokenType::String {
        return Ok(None);
    }

    let key = tokens[pos + 2].text.trim_matches('"').trim_matches('\'');

    if tokens[pos + 3].token_type != TokenType::EndArray {
        return Ok(None);
    }

    if tokens[pos + 4].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let mut call_end = pos + 4;
    let mut depth = 1;
    let mut i = pos + 5;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartExpr => depth += 1,
            TokenType::EndExpr => {
                depth -= 1;
                if depth == 0 {
                    call_end = i;
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(func) = dispatcher.functions.get(key) {
        if let Some(return_val) = &func.return_value {
            eprintln!(
                "Inlining dispatcher call: {}[\"{}\"]() -> {:?}",
                var_name, key, return_val.text
            );
            return Ok(Some((vec![return_val.clone()], call_end - pos + 1)));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_detect_simple_dispatcher() {
        let code = r#"
var _dispatch = {
    "abc": function() { return "value1"; },
    "def": function() { return "value2"; }
};
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let dispatchers = detect_object_dispatchers(&tokens).unwrap();

        assert_eq!(dispatchers.len(), 1, "Should detect 1 dispatcher");
        assert_eq!(dispatchers[0].var_name, "_dispatch");
        assert_eq!(dispatchers[0].functions.len(), 2, "Should have 2 functions");
        assert!(dispatchers[0].functions.contains_key("abc"));
        assert!(dispatchers[0].functions.contains_key("def"));
    }

    #[test]
    fn test_inline_dispatcher_call() {
        let code = r#"
var _dispatch = {
    "abc": function() { return "value1"; }
};
var result = _dispatch["abc"]();
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let dispatchers = detect_object_dispatchers(&tokens).unwrap();
        assert_eq!(dispatchers.len(), 1);

        let inlined = inline_dispatcher_calls(&tokens, &dispatchers).unwrap();
        let output: String = inlined.iter().map(|t| t.text.as_str()).collect();

        eprintln!("Inlined output: {}", output);

        assert!(
            output.contains("value1"),
            "Should inline the return value, got: {}",
            output
        );
        assert!(
            !output.contains("_dispatch["),
            "Should remove dispatcher call, got: {}",
            output
        );
    }
}
