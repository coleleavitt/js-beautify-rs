use crate::Result;
use crate::token::{Token, TokenType};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct ObjectProperty {
    key: String,
    value_start: usize,
    value_end: usize,
}

#[derive(Debug, Clone)]
struct SparseObject {
    variable_name: String,
    declaration_pos: usize,
    properties: Vec<ObjectProperty>,
    last_assignment_pos: usize,
}

pub fn consolidate_sparse_objects(tokens: &[Token]) -> Result<Vec<Token>> {
    let sparse_objects = detect_sparse_objects(tokens)?;

    if sparse_objects.is_empty() {
        return Ok(tokens.to_vec());
    }

    build_consolidated_output(tokens, &sparse_objects)
}

fn detect_sparse_objects(tokens: &[Token]) -> Result<Vec<SparseObject>> {
    let mut objects = HashMap::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some((var_name, pos)) = detect_empty_object_declaration(tokens, i)? {
            objects.insert(
                var_name.clone(),
                SparseObject {
                    variable_name: var_name,
                    declaration_pos: pos,
                    properties: Vec::new(),
                    last_assignment_pos: pos,
                },
            );
            i += 1;
        } else if let Some((var_name, prop)) = detect_property_assignment(tokens, i)? {
            if let Some(obj) = objects.get_mut(&var_name) {
                obj.properties.push(prop);
                obj.last_assignment_pos = i;
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let consolidated: Vec<SparseObject> = objects
        .into_iter()
        .filter(|(_, obj)| obj.properties.len() >= 2)
        .map(|(_, obj)| obj)
        .collect();

    Ok(consolidated)
}

fn detect_empty_object_declaration(
    tokens: &[Token],
    pos: usize,
) -> Result<Option<(String, usize)>> {
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

    if tokens[pos + 4].token_type != TokenType::EndBlock {
        return Ok(None);
    }

    if tokens[pos + 5].token_type != TokenType::Semicolon {
        return Ok(None);
    }

    debug_assert!(!var_name.is_empty(), "Variable name cannot be empty");

    Ok(Some((var_name, pos)))
}

fn detect_property_assignment(
    tokens: &[Token],
    pos: usize,
) -> Result<Option<(String, ObjectProperty)>> {
    if pos + 5 >= tokens.len() {
        return Ok(None);
    }

    if tokens[pos].token_type != TokenType::Word {
        return Ok(None);
    }

    let var_name = tokens[pos].text.clone();

    if tokens[pos + 1].token_type != TokenType::Dot {
        return Ok(None);
    }

    if tokens[pos + 2].token_type != TokenType::Word {
        return Ok(None);
    }

    let prop_key = tokens[pos + 2].text.clone();

    if tokens[pos + 3].token_type != TokenType::Equals {
        return Ok(None);
    }

    let value_start = pos + 4;
    let mut value_end = value_start;
    let mut depth = 0;

    while value_end < tokens.len() {
        match tokens[value_end].token_type {
            TokenType::StartExpr | TokenType::StartBlock | TokenType::StartArray => depth += 1,
            TokenType::EndExpr | TokenType::EndBlock | TokenType::EndArray => {
                if depth > 0 {
                    depth -= 1;
                } else if depth == 0 {
                    break;
                }
            }
            TokenType::Semicolon if depth == 0 => {
                value_end -= 1;
                break;
            }
            _ => {}
        }
        value_end += 1;
    }

    debug_assert!(value_end >= value_start, "Value end must be >= start");
    debug_assert!(!var_name.is_empty(), "Variable name cannot be empty");
    debug_assert!(!prop_key.is_empty(), "Property key cannot be empty");

    Ok(Some((
        var_name,
        ObjectProperty {
            key: prop_key,
            value_start,
            value_end,
        },
    )))
}

fn build_consolidated_output(
    tokens: &[Token],
    sparse_objects: &[SparseObject],
) -> Result<Vec<Token>> {
    let mut skip_ranges = Vec::new();

    for obj in sparse_objects {
        skip_ranges.push((obj.declaration_pos, obj.declaration_pos + 5));

        for prop in &obj.properties {
            let assignment_start = find_assignment_start(tokens, prop.value_start)?;
            let assignment_end = prop.value_end + 2;
            skip_ranges.push((assignment_start, assignment_end));
        }
    }

    skip_ranges.sort_by_key(|(start, _)| *start);

    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if let Some(obj) = sparse_objects.iter().find(|o| i == o.declaration_pos) {
            result.push(Token::new(TokenType::Reserved, "var".to_string()));
            result.push(Token::new(TokenType::Word, obj.variable_name.clone()));
            result.push(Token::new(TokenType::Equals, "=".to_string()));
            result.push(Token::new(TokenType::StartBlock, "{".to_string()));

            for (idx, prop) in obj.properties.iter().enumerate() {
                result.push(Token::new(TokenType::Word, prop.key.clone()));
                result.push(Token::new(TokenType::Operator, ":".to_string()));

                for j in prop.value_start..=prop.value_end {
                    if j < tokens.len() {
                        result.push(tokens[j].clone());
                    }
                }

                if idx < obj.properties.len() - 1 {
                    result.push(Token::new(TokenType::Comma, ",".to_string()));
                }
            }

            result.push(Token::new(TokenType::EndBlock, "}".to_string()));
            result.push(Token::new(TokenType::Semicolon, ";".to_string()));

            i = obj.declaration_pos + 6;
            continue;
        }

        let in_skip_range = skip_ranges
            .iter()
            .any(|(start, end)| i >= *start && i <= *end);

        if in_skip_range {
            i += 1;
            continue;
        }

        result.push(tokens[i].clone());
        i += 1;
    }

    Ok(result)
}

fn find_assignment_start(tokens: &[Token], value_start: usize) -> Result<usize> {
    let mut pos = value_start;
    while pos > 0 {
        if tokens[pos - 1].token_type == TokenType::Word {
            pos -= 1;
            if pos > 0
                && tokens[pos - 1].token_type == TokenType::Operator
                && tokens[pos - 1].text == "."
            {
                pos -= 1;
                if pos > 0 && tokens[pos - 1].token_type == TokenType::Word {
                    return Ok(pos - 1);
                }
            }
        }
        break;
    }
    Ok(value_start.saturating_sub(4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_detect_sparse_object() {
        let code = r#"
var obj = {};
obj.a = 1;
obj.b = 2;
obj.c = 3;
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let sparse = detect_sparse_objects(&tokens).unwrap();

        assert_eq!(sparse.len(), 1);
        assert_eq!(sparse[0].variable_name, "obj");
        assert_eq!(sparse[0].properties.len(), 3);
        assert_eq!(sparse[0].properties[0].key, "a");
        assert_eq!(sparse[0].properties[1].key, "b");
        assert_eq!(sparse[0].properties[2].key, "c");
    }

    #[test]
    fn test_consolidate_sparse_object() {
        let code = r#"
var obj = {};
obj.a = 1;
obj.b = 2;
console.log(obj);
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = consolidate_sparse_objects(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("{a:1,b:2}"),
            "Should consolidate object, got: {}",
            output
        );
        assert!(
            !output.contains("obj.a"),
            "Should not have scattered assignments, got: {}",
            output
        );
    }

    #[test]
    fn test_preserve_non_sparse_objects() {
        let code = r#"
var obj = {a: 1};
var other = {};
other.x = 5;
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = consolidate_sparse_objects(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("{a:1}"),
            "Should preserve non-sparse object, got: {}",
            output
        );
        assert!(
            output.contains("other.x=5"),
            "Should preserve single assignment, got: {}",
            output
        );
    }

    #[test]
    fn test_multiple_sparse_objects() {
        let code = r#"
var a = {};
a.x = 1;
a.y = 2;
var b = {};
b.p = 3;
b.q = 4;
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let sparse = detect_sparse_objects(&tokens).unwrap();

        assert_eq!(sparse.len(), 2);
    }
}
