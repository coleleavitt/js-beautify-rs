use crate::token::{Token, TokenType};
use crate::{BeautifyError, Result};
use std::collections::HashMap;

#[cfg(debug_assertions)]
macro_rules! trace_inline {
    ($($arg:tt)*) => {
        eprintln!("[INLINE] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_inline {
    ($($arg:tt)*) => {};
}

const MAX_PARAMS: usize = 20;
const MAX_BODY_TOKENS: usize = 100;
const MAX_INLINE_CANDIDATES: usize = 1000;

#[derive(Debug, Clone)]
pub struct InlinableFunction {
    pub name: String,
    pub start_index: usize,
    pub end_index: usize,
    pub params: Vec<String>,
    pub body_tokens: Vec<Token>,
    pub is_simple: bool,
}

impl InlinableFunction {
    fn new(
        name: String,
        start_index: usize,
        end_index: usize,
        params: Vec<String>,
        body_tokens: Vec<Token>,
    ) -> Self {
        debug_assert!(!name.is_empty(), "Function name must not be empty");
        debug_assert!(
            start_index < end_index,
            "Start must be before end: {} >= {}",
            start_index,
            end_index
        );
        debug_assert!(
            params.len() <= MAX_PARAMS,
            "Too many parameters: {}",
            params.len()
        );
        debug_assert!(
            body_tokens.len() <= MAX_BODY_TOKENS,
            "Body too large: {} tokens",
            body_tokens.len()
        );

        let is_simple = Self::check_simplicity(&body_tokens, params.len());

        Self {
            name,
            start_index,
            end_index,
            params,
            body_tokens,
            is_simple,
        }
    }

    fn check_simplicity(body: &[Token], _param_count: usize) -> bool {
        const MAX_SIMPLE_TOKENS: usize = 20;

        if body.is_empty() {
            return true;
        }

        if body.len() > MAX_SIMPLE_TOKENS {
            trace_inline!("body too large for simple: {} tokens", body.len());
            return false;
        }

        let has_return = body
            .iter()
            .any(|t| t.token_type == TokenType::Reserved && t.text == "return");

        let semicolon_count = body
            .iter()
            .filter(|t| t.token_type == TokenType::Semicolon)
            .count();

        let has_single_statement = semicolon_count <= 2;

        let has_loops = body.iter().any(|t| {
            t.token_type == TokenType::Reserved && matches!(t.text.as_str(), "for" | "while" | "do")
        });

        let has_conditionals = body
            .iter()
            .any(|t| t.token_type == TokenType::Reserved && t.text == "if");

        let is_simple = (has_return || body.len() <= 5)
            && has_single_statement
            && !has_loops
            && !has_conditionals;

        debug_assert!(
            !body.is_empty() || is_simple,
            "Empty body should be marked simple"
        );

        is_simple
    }
}

pub fn detect_inlinable_functions(tokens: &[Token]) -> Result<HashMap<String, InlinableFunction>> {
    trace_inline!("=== DETECTING INLINABLE FUNCTIONS ===");

    let mut functions = HashMap::new();
    let mut i = 0usize;

    while i < tokens.len() {
        if let Some(func) = try_detect_inlinable(tokens, i)? {
            trace_inline!(
                "found inlinable function: {} ({} params, {} tokens, simple={})",
                func.name,
                func.params.len(),
                func.body_tokens.len(),
                func.is_simple
            );

            if functions.len() >= MAX_INLINE_CANDIDATES {
                trace_inline!(
                    "reached max inline candidates limit: {}",
                    MAX_INLINE_CANDIDATES
                );
                break;
            }

            functions.insert(func.name.clone(), func);
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;
    }

    debug_assert!(
        functions.len() <= MAX_INLINE_CANDIDATES,
        "Function count exceeds limit"
    );
    trace_inline!("detected {} inlinable functions", functions.len());

    Ok(functions)
}

fn try_detect_inlinable(tokens: &[Token], pos: usize) -> Result<Option<InlinableFunction>> {
    const MIN_TOKENS: usize = 8;

    trace_inline!(
        "try_detect at pos={}, tokens.len()={}, need {} more",
        pos,
        tokens.len(),
        MIN_TOKENS
    );

    if pos
        .checked_add(MIN_TOKENS)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?
        >= tokens.len()
    {
        trace_inline!("  -> early return: not enough tokens");
        return Ok(None);
    }

    trace_inline!(
        "  -> checking token[{}]: {:?} = '{}'",
        pos,
        tokens[pos].token_type,
        tokens[pos].text
    );

    if tokens[pos].token_type != TokenType::Reserved || tokens[pos].text != "function" {
        trace_inline!("  -> not a function keyword");
        return Ok(None);
    }

    let name_pos = pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if name_pos >= tokens.len() || tokens[name_pos].token_type != TokenType::Word {
        return Ok(None);
    }

    let name = tokens[name_pos].text.clone();
    debug_assert!(!name.is_empty(), "Function name cannot be empty");

    let paren_pos = name_pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if paren_pos >= tokens.len() || tokens[paren_pos].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let (params, params_end) = extract_params(tokens, paren_pos)?;
    debug_assert!(params_end > paren_pos, "Params end must be after start");

    let body_start = params_end
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if body_start >= tokens.len() || tokens[body_start].token_type != TokenType::StartBlock {
        return Ok(None);
    }

    let body_end = find_matching_brace(tokens, body_start)?;
    debug_assert!(body_end > body_start, "Body end must be after start");

    let body_tokens = extract_body_tokens(tokens, body_start, body_end)?;

    if body_tokens.len() > MAX_BODY_TOKENS {
        trace_inline!(
            "skipping {}: body too large ({} tokens)",
            name,
            body_tokens.len()
        );
        return Ok(None);
    }

    Ok(Some(InlinableFunction::new(
        name,
        pos,
        body_end,
        params,
        body_tokens,
    )))
}

fn extract_params(tokens: &[Token], start: usize) -> Result<(Vec<String>, usize)> {
    debug_assert!(start < tokens.len(), "Start position out of bounds");
    debug_assert!(
        tokens[start].token_type == TokenType::StartExpr,
        "Must start at opening paren"
    );

    let mut params = Vec::new();
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
                    debug_assert!(params.len() <= MAX_PARAMS, "Too many parameters");
                    return Ok((params, i));
                }
            }
            TokenType::Word => {
                if depth == 1 {
                    debug_assert!(!tokens[i].text.is_empty(), "Parameter name cannot be empty");
                    params.push(tokens[i].text.clone());
                }
            }
            _ => {}
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    let end_pos = i
        .checked_sub(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?;

    Ok((params, end_pos))
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

    let end_pos = i
        .checked_sub(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?;

    Ok(end_pos)
}

fn extract_body_tokens(tokens: &[Token], start: usize, end: usize) -> Result<Vec<Token>> {
    debug_assert!(start < end, "Start must be before end");
    debug_assert!(end <= tokens.len(), "End out of bounds");

    let body_start = start
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if body_start >= end {
        return Ok(Vec::new());
    }

    let body_tokens = tokens[body_start..end].to_vec();

    debug_assert!(
        body_tokens.len() <= MAX_BODY_TOKENS,
        "Body too large: {} tokens",
        body_tokens.len()
    );

    Ok(body_tokens)
}

fn count_function_uses(tokens: &[Token], func_name: &str) -> Result<usize> {
    debug_assert!(!func_name.is_empty(), "Function name cannot be empty");

    let mut count = 0usize;
    let mut i = 0usize;

    while i < tokens.len() {
        if tokens[i].token_type == TokenType::Word && tokens[i].text == func_name {
            trace_inline!(
                "  -> found '{}' at position {}, checking context...",
                func_name,
                i
            );

            if i > 0
                && tokens[i
                    .checked_sub(1)
                    .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?]
                .token_type
                    == TokenType::Reserved
                && tokens[i
                    .checked_sub(1)
                    .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?]
                .text
                    == "function"
            {
                trace_inline!("  -> skipping: this is a function declaration, not a call");
                i = i
                    .checked_add(1)
                    .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
                continue;
            }

            let next_pos = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

            if next_pos < tokens.len() && tokens[next_pos].token_type == TokenType::StartExpr {
                count = count.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("count overflow".to_string())
                })?;

                trace_inline!("  -> YES: found call to {} at position {}", func_name, i);
            } else {
                trace_inline!(
                    "  -> NO: '{}' at {} not followed by '(' (next token: {:?})",
                    func_name,
                    i,
                    if next_pos < tokens.len() {
                        format!("{:?}", tokens[next_pos].token_type)
                    } else {
                        "EOF".to_string()
                    }
                );
            }
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    debug_assert!(count < 10000, "Suspiciously high use count: {}", count);
    trace_inline!("function {} used {} times", func_name, count);

    Ok(count)
}

pub fn inline_single_use_functions(
    tokens: &[Token],
    functions: &HashMap<String, InlinableFunction>,
) -> Result<Vec<Token>> {
    trace_inline!("=== INLINING SINGLE-USE FUNCTIONS ===");

    let mut single_use_funcs = HashMap::new();

    for (name, func) in functions {
        let use_count = count_function_uses(tokens, name)?;

        if use_count == 1 && func.is_simple {
            trace_inline!(
                "function {} is single-use and simple, marking for inline",
                name
            );
            single_use_funcs.insert(name.clone(), func.clone());
        } else {
            trace_inline!(
                "function {} not inlineable: uses={}, simple={}",
                name,
                use_count,
                func.is_simple
            );
        }
    }

    if single_use_funcs.is_empty() {
        trace_inline!("no single-use functions to inline");
        return Ok(tokens.to_vec());
    }

    trace_inline!("inlining {} single-use functions", single_use_funcs.len());

    let without_decls = remove_function_declarations(tokens, &single_use_funcs)?;
    trace_inline!(
        "after removing declarations: {} -> {} tokens",
        tokens.len(),
        without_decls.len()
    );
    trace_inline!(
        "tokens after removal: {}",
        without_decls
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );

    let with_inlined = inline_calls(&without_decls, &single_use_funcs)?;
    trace_inline!(
        "after inlining calls: {} -> {} tokens",
        without_decls.len(),
        with_inlined.len()
    );
    trace_inline!(
        "tokens after inlining: {}",
        with_inlined
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );

    debug_assert!(
        with_inlined.len() <= without_decls.len(),
        "Inlined tokens should not exceed cleaned: {} > {}",
        with_inlined.len(),
        without_decls.len()
    );

    trace_inline!(
        "inlining complete: {} -> {} tokens",
        tokens.len(),
        with_inlined.len()
    );

    Ok(with_inlined)
}

fn inline_calls(
    tokens: &[Token],
    functions: &HashMap<String, InlinableFunction>,
) -> Result<Vec<Token>> {
    let mut result = Vec::new();
    let mut i = 0usize;
    let mut inlined_count = 0usize;

    while i < tokens.len() {
        if tokens[i].token_type == TokenType::Word && functions.contains_key(&tokens[i].text) {
            if let Some((inlined, skip)) = try_inline_call(tokens, i, functions)? {
                debug_assert!(skip > 0, "Skip must be positive");
                debug_assert!(skip < 1000, "Skip suspiciously large: {}", skip);

                result.extend(inlined);
                i = i
                    .checked_add(skip)
                    .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

                inlined_count = inlined_count.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("count overflow".to_string())
                })?;

                continue;
            }
        }

        result.push(tokens[i].clone());
        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    trace_inline!("inlined {} function calls", inlined_count);
    Ok(result)
}

fn try_inline_call(
    tokens: &[Token],
    pos: usize,
    functions: &HashMap<String, InlinableFunction>,
) -> Result<Option<(Vec<Token>, usize)>> {
    debug_assert!(pos < tokens.len(), "Position out of bounds");

    let func_name = &tokens[pos].text;
    let func = match functions.get(func_name) {
        Some(f) => f,
        None => return Ok(None),
    };

    let paren_pos = pos
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    if paren_pos >= tokens.len() || tokens[paren_pos].token_type != TokenType::StartExpr {
        return Ok(None);
    }

    let call_args = extract_call_args(tokens, paren_pos)?;

    if call_args.len() != func.params.len() {
        trace_inline!(
            "skipping {} call: arg count mismatch ({} != {})",
            func_name,
            call_args.len(),
            func.params.len()
        );
        return Ok(None);
    }

    let inlined_body = substitute_params(&func.body_tokens, &func.params, &call_args)?;

    let args_end = find_call_end(tokens, paren_pos)?;
    let skip = args_end
        .checked_sub(pos)
        .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))?
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    trace_inline!("inlined call to {} (skipping {} tokens)", func_name, skip);

    Ok(Some((inlined_body, skip)))
}

fn extract_call_args(tokens: &[Token], start: usize) -> Result<Vec<Vec<Token>>> {
    debug_assert!(start < tokens.len(), "Start position out of bounds");
    debug_assert!(
        tokens[start].token_type == TokenType::StartExpr,
        "Must start at opening paren"
    );

    let mut args = Vec::new();
    let mut current_arg = Vec::new();
    let mut depth = 1usize;
    let mut i = start
        .checked_add(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

    while i < tokens.len() && depth > 0 {
        match tokens[i].token_type {
            TokenType::StartExpr | TokenType::StartBlock | TokenType::StartArray => {
                depth = depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("depth overflow".to_string())
                })?;
                current_arg.push(tokens[i].clone());
            }
            TokenType::EndExpr | TokenType::EndBlock | TokenType::EndArray => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("depth underflow".to_string())
                })?;

                if depth == 0 {
                    if !current_arg.is_empty() {
                        args.push(current_arg);
                    }
                    break;
                }
                current_arg.push(tokens[i].clone());
            }
            TokenType::Comma => {
                if depth == 1 {
                    if !current_arg.is_empty() {
                        args.push(current_arg);
                        current_arg = Vec::new();
                    }
                } else {
                    current_arg.push(tokens[i].clone());
                }
            }
            _ => {
                current_arg.push(tokens[i].clone());
            }
        }

        i = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
    }

    debug_assert!(
        args.len() <= MAX_PARAMS,
        "Too many arguments: {}",
        args.len()
    );

    Ok(args)
}

fn find_call_end(tokens: &[Token], start: usize) -> Result<usize> {
    debug_assert!(start < tokens.len(), "Start position out of bounds");

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

    i.checked_sub(1)
        .ok_or_else(|| BeautifyError::BeautificationFailed("underflow".to_string()))
}

fn extract_return_expression(body: &[Token]) -> &[Token] {
    let mut start = 0;
    let mut end = body.len();

    if !body.is_empty() && body[0].token_type == TokenType::Reserved && body[0].text == "return" {
        start = 1;
    }

    if end > start && body[end - 1].token_type == TokenType::Semicolon {
        end = end.saturating_sub(1);
    }

    &body[start..end]
}

fn substitute_params(body: &[Token], params: &[String], args: &[Vec<Token>]) -> Result<Vec<Token>> {
    debug_assert_eq!(params.len(), args.len(), "Param count must match arg count");

    let expression = extract_return_expression(body);

    let mut result = Vec::new();

    for token in expression {
        if token.token_type == TokenType::Word {
            if let Some(param_index) = params.iter().position(|p| p == &token.text) {
                debug_assert!(param_index < args.len(), "Param index out of bounds");

                result.extend(args[param_index].clone());

                trace_inline!(
                    "substituted param {} with {} tokens",
                    token.text,
                    args[param_index].len()
                );
            } else {
                result.push(token.clone());
            }
        } else {
            result.push(token.clone());
        }
    }

    debug_assert!(
        result.len() <= 10000,
        "Substituted body too large: {} tokens",
        result.len()
    );

    Ok(result)
}

fn remove_function_declarations(
    tokens: &[Token],
    functions: &HashMap<String, InlinableFunction>,
) -> Result<Vec<Token>> {
    trace_inline!("removing {} function declarations", functions.len());
    trace_inline!(
        "function names to remove: {:?}",
        functions.keys().collect::<Vec<_>>()
    );

    let mut result = Vec::new();
    let mut i = 0usize;
    let mut removed_count = 0usize;

    while i < tokens.len() {
        let mut should_skip = false;

        if tokens[i].token_type == TokenType::Reserved && tokens[i].text == "function" {
            trace_inline!("found 'function' keyword at position {}", i);
            let name_pos = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

            if name_pos < tokens.len() && tokens[name_pos].token_type == TokenType::Word {
                let name = &tokens[name_pos].text;
                trace_inline!("  -> function name: '{}'", name);

                if functions.contains_key(name) {
                    trace_inline!("  -> name '{}' is in removal list", name);
                    let paren_pos = name_pos.checked_add(1).ok_or_else(|| {
                        BeautifyError::BeautificationFailed("overflow".to_string())
                    })?;

                    if paren_pos < tokens.len()
                        && tokens[paren_pos].token_type == TokenType::StartExpr
                    {
                        let mut params_end = paren_pos;
                        let mut depth: usize = 1;

                        while params_end < tokens.len() && depth > 0 {
                            params_end = params_end.checked_add(1).ok_or_else(|| {
                                BeautifyError::BeautificationFailed("overflow".to_string())
                            })?;

                            if params_end >= tokens.len() {
                                break;
                            }

                            if tokens[params_end].token_type == TokenType::StartExpr {
                                depth = depth.checked_add(1).ok_or_else(|| {
                                    BeautifyError::BeautificationFailed("overflow".to_string())
                                })?;
                            } else if tokens[params_end].token_type == TokenType::EndExpr {
                                depth = depth.checked_sub(1).ok_or_else(|| {
                                    BeautifyError::BeautificationFailed("underflow".to_string())
                                })?;
                            }
                        }

                        let body_start = params_end.checked_add(1).ok_or_else(|| {
                            BeautifyError::BeautificationFailed("overflow".to_string())
                        })?;

                        if body_start < tokens.len()
                            && tokens[body_start].token_type == TokenType::StartBlock
                        {
                            let body_end = find_matching_brace(tokens, body_start)?;

                            let skip_count = body_end
                                .checked_sub(i)
                                .ok_or_else(|| {
                                    BeautifyError::BeautificationFailed("underflow".to_string())
                                })?
                                .checked_add(1)
                                .ok_or_else(|| {
                                    BeautifyError::BeautificationFailed("overflow".to_string())
                                })?;

                            trace_inline!(
                                "removing function {} declaration ({} tokens, from {} to {})",
                                name,
                                skip_count,
                                i,
                                body_end
                            );

                            i = i.checked_add(skip_count).ok_or_else(|| {
                                BeautifyError::BeautificationFailed("overflow".to_string())
                            })?;

                            removed_count = removed_count.checked_add(1).ok_or_else(|| {
                                BeautifyError::BeautificationFailed("count overflow".to_string())
                            })?;

                            should_skip = true;
                        }
                    }
                }
            }
        }

        if !should_skip {
            result.push(tokens[i].clone());
            i = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
        }
    }

    debug_assert!(
        result.len() <= tokens.len(),
        "Result should not exceed original"
    );
    if removed_count != functions.len() {
        trace_inline!(
            "WARNING: expected to remove {} functions but removed {}",
            functions.len(),
            removed_count
        );
    }

    trace_inline!("removed {} function declarations", removed_count);

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_detect_simple_wrapper() {
        let code = "function wrap(x) { return x + 1; }";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let functions = detect_inlinable_functions(&tokens).unwrap();

        assert_eq!(functions.len(), 1);
        assert!(functions.contains_key("wrap"));
        assert!(functions["wrap"].is_simple);
    }

    #[test]
    fn test_count_single_use() {
        let code = "function helper(x) { return x * 2; } var y = helper(5);";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let count = count_function_uses(&tokens, "helper").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_inline_single_use_function() {
        let code = "function twice(n) { return n * 2; } var result = twice(10);";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let functions = detect_inlinable_functions(&tokens).unwrap();
        let inlined = inline_single_use_functions(&tokens, &functions).unwrap();

        let output: String = inlined.iter().map(|t| t.text.as_str()).collect();

        assert!(
            !output.contains("function twice"),
            "Function declaration should be removed"
        );
        assert!(
            !output.contains("twice(10)"),
            "Function call should be inlined"
        );
    }
}
