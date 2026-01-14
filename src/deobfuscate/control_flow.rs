use crate::Result;
use crate::token::{Token, TokenType};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ControlFlowInfo {
    pub start_index: usize,
    pub end_index: usize,
    pub sequence_var: String,
    pub sequence: Vec<String>,
    pub cases: HashMap<String, CaseBlock>,
}

#[derive(Debug, Clone)]
pub struct CaseBlock {
    pub case_value: String,
    pub start_index: usize,
    pub end_index: usize,
    pub tokens: Vec<Token>,
}

pub fn detect_control_flow_flattening(tokens: &[Token]) -> Result<Vec<ControlFlowInfo>> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some(cf_info) = detect_switch_dispatcher(tokens, i)? {
            result.push(cf_info);
            i = result.last().unwrap().end_index + 1;
        } else {
            i += 1;
        }
    }

    Ok(result)
}

fn detect_switch_dispatcher(tokens: &[Token], start: usize) -> Result<Option<ControlFlowInfo>> {
    if start + 20 >= tokens.len() {
        return Ok(None);
    }

    let sequence_info = detect_sequence_variable(tokens, start)?;
    if sequence_info.is_none() {
        return Ok(None);
    }

    let (seq_var, sequence, seq_end) = sequence_info.unwrap();

    if let Some((switch_start, switch_end)) = find_while_switch_loop(tokens, seq_end, &seq_var)? {
        let cases = extract_switch_cases(tokens, switch_start, switch_end)?;

        return Ok(Some(ControlFlowInfo {
            start_index: start,
            end_index: switch_end,
            sequence_var: seq_var,
            sequence,
            cases,
        }));
    }

    Ok(None)
}

fn detect_sequence_variable(
    tokens: &[Token],
    start: usize,
) -> Result<Option<(String, Vec<String>, usize)>> {
    if start + 10 >= tokens.len() {
        return Ok(None);
    }

    if tokens[start].token_type != TokenType::Reserved || tokens[start].text != "var" {
        return Ok(None);
    }

    let var_name = &tokens[start + 1];
    if var_name.token_type != TokenType::Word {
        return Ok(None);
    }

    if tokens[start + 2].token_type != TokenType::Equals {
        return Ok(None);
    }

    let string_token = &tokens[start + 3];
    if string_token.token_type != TokenType::String {
        return Ok(None);
    }

    let mut i = start + 4;
    let mut found_split = false;

    while i < tokens.len() && i < start + 15 {
        if tokens[i].token_type == TokenType::Word && tokens[i].text == "split" {
            found_split = true;
            break;
        }
        i += 1;
    }

    if !found_split {
        return Ok(None);
    }

    let sequence_str = string_token.text.trim_matches('"').trim_matches('\'');
    let sequence: Vec<String> = sequence_str.split('|').map(String::from).collect();

    let mut end_index = i;
    while end_index < tokens.len() && tokens[end_index].token_type != TokenType::Semicolon {
        end_index += 1;
    }

    Ok(Some((var_name.text.clone(), sequence, end_index)))
}

fn find_while_switch_loop(
    tokens: &[Token],
    start: usize,
    _seq_var: &str,
) -> Result<Option<(usize, usize)>> {
    let mut i = start;

    while i < tokens.len() && i < start + 100 {
        if is_while_true_start(tokens, i) || is_for_infinite_start(tokens, i) {
            if let Some(switch_pos) = find_switch_in_loop(tokens, i)? {
                if let Some(end) = find_loop_end(tokens, i)? {
                    return Ok(Some((switch_pos, end)));
                }
            }
        }
        i += 1;
    }

    Ok(None)
}

fn is_while_true_start(tokens: &[Token], pos: usize) -> bool {
    if pos + 5 >= tokens.len() {
        return false;
    }

    tokens[pos].token_type == TokenType::Reserved
        && tokens[pos].text == "while"
        && tokens[pos + 1].token_type == TokenType::StartExpr
        && tokens[pos + 2].text == "true"
        && tokens[pos + 3].token_type == TokenType::EndExpr
}

fn is_for_infinite_start(tokens: &[Token], pos: usize) -> bool {
    if pos + 7 >= tokens.len() {
        return false;
    }

    tokens[pos].token_type == TokenType::Reserved
        && tokens[pos].text == "for"
        && tokens[pos + 1].token_type == TokenType::StartExpr
        && tokens[pos + 2].token_type == TokenType::Semicolon
        && tokens[pos + 3].token_type == TokenType::Semicolon
        && tokens[pos + 4].token_type == TokenType::EndExpr
}

fn find_switch_in_loop(tokens: &[Token], loop_pos: usize) -> Result<Option<usize>> {
    let mut i = loop_pos;
    let mut depth = 0;

    while i < tokens.len() && i < loop_pos + 50 {
        match tokens[i].token_type {
            TokenType::StartBlock => depth += 1,
            TokenType::EndBlock => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            }
            TokenType::Reserved if tokens[i].text == "switch" => {
                return Ok(Some(i));
            }
            _ => {}
        }
        i += 1;
    }

    Ok(None)
}

fn find_loop_end(tokens: &[Token], loop_pos: usize) -> Result<Option<usize>> {
    let mut i = loop_pos;
    let mut depth = 0;

    while i < tokens.len() {
        match tokens[i].token_type {
            TokenType::StartBlock => depth += 1,
            TokenType::EndBlock => {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some(i));
                }
            }
            _ => {}
        }
        i += 1;
    }

    Ok(None)
}

fn extract_switch_cases(
    tokens: &[Token],
    switch_start: usize,
    switch_end: usize,
) -> Result<HashMap<String, CaseBlock>> {
    let mut cases = HashMap::new();
    let mut i = switch_start;

    while i < switch_end {
        if tokens[i].token_type == TokenType::Reserved && tokens[i].text == "case" {
            if i + 2 < tokens.len() {
                let case_value = tokens[i + 1]
                    .text
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();

                let case_end = find_case_end(tokens, i)?;
                let case_tokens = tokens[i..case_end].to_vec();

                cases.insert(
                    case_value.clone(),
                    CaseBlock {
                        case_value,
                        start_index: i,
                        end_index: case_end,
                        tokens: case_tokens,
                    },
                );

                i = case_end;
                continue;
            }
        }
        i += 1;
    }

    Ok(cases)
}

fn find_case_end(tokens: &[Token], case_start: usize) -> Result<usize> {
    let mut i = case_start + 1;

    while i < tokens.len() {
        if (tokens[i].token_type == TokenType::Reserved && tokens[i].text == "case")
            || (tokens[i].token_type == TokenType::Reserved && tokens[i].text == "default")
            || tokens[i].token_type == TokenType::EndBlock
        {
            return Ok(i);
        }
        i += 1;
    }

    Ok(i)
}

pub fn reconstruct_control_flow(
    _tokens: &[Token],
    cf_info: &ControlFlowInfo,
) -> Result<Vec<Token>> {
    let mut result = Vec::new();

    for step in &cf_info.sequence {
        if let Some(case_block) = cf_info.cases.get(step) {
            let mut case_tokens = case_block.tokens.clone();

            case_tokens.retain(|t| {
                !(t.token_type == TokenType::Reserved
                    && (t.text == "continue" || t.text == "break"))
                    && !(t.token_type == TokenType::Reserved && t.text == "case")
                    && t.token_type != TokenType::Colon
            });

            result.extend(case_tokens);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_detect_sequence_variable() {
        let code = r#"var _flow = "3|1|0|2|4".split("|");"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = detect_sequence_variable(&tokens, 0).unwrap();
        assert!(result.is_some());

        let (var_name, sequence, _) = result.unwrap();
        assert_eq!(var_name, "_flow");
        assert_eq!(sequence.len(), 5);
        assert_eq!(sequence[0], "3");
        assert_eq!(sequence[1], "1");
    }

    #[test]
    fn test_detect_while_true() {
        let code = r#"while (true) { console.log("test"); }"#;
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let while_pos = tokens
            .iter()
            .position(|t| t.token_type == TokenType::Reserved && t.text == "while")
            .expect("Should find while token");

        assert!(is_while_true_start(&tokens, while_pos));
    }

    #[test]
    fn test_detect_control_flow_pattern() {
        let code = r#"
var _flow = "1|0|2".split("|");
var _i = 0;
while (true) {
    switch (_flow[_i++]) {
        case "0": console.log("step 2"); continue;
        case "1": console.log("step 1"); continue;
        case "2": console.log("step 3"); break;
    }
    break;
}
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = detect_control_flow_flattening(&tokens).unwrap();
        assert!(result.len() > 0, "Should detect control flow pattern");
    }
}
