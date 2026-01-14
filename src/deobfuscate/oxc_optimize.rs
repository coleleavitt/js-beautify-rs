use crate::Result;
use crate::oxc_opts::OxcOptimizer;
use crate::token::Token;

pub fn apply_oxc_optimizations(tokens: &[Token]) -> Result<Vec<Token>> {
    let code = tokens_to_string(tokens);

    let mut optimizer = OxcOptimizer::new();
    let optimized = match optimizer.optimize(&code) {
        Ok(opt) => opt,
        Err(e) => {
            eprintln!(
                "[WARN] Oxc optimization skipped - parse failed: {}",
                e.lines().next().unwrap_or(&e)
            );
            eprintln!("[INFO] Continuing with token-based optimizations only");
            return Ok(tokens.to_vec());
        }
    };

    let mut tokenizer = crate::tokenizer::Tokenizer::new(&optimized);
    let new_tokens = tokenizer.tokenize().map_err(|e| {
        crate::BeautifyError::BeautificationFailed(format!(
            "Re-tokenization after oxc failed: {}",
            e
        ))
    })?;

    Ok(new_tokens)
}

fn tokens_to_string(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_oxc_optimization_roundtrip() {
        let code = r#"
            for (let i = 0; i < 3; i++) {
                console.log(i);
            }
        "#;

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = apply_oxc_optimizations(&tokens);
        assert!(result.is_ok());

        let optimized_tokens = result.unwrap();
        let optimized_code = tokens_to_string(&optimized_tokens);

        assert!(optimized_code.contains("console") && optimized_code.contains("log"));
        assert!(optimized_code.contains("0"));
        assert!(optimized_code.contains("1"));
        assert!(optimized_code.contains("2"));
    }

    #[test]
    fn test_cse_optimization() {
        let code = "const a = x + y; const b = x + y;";

        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = apply_oxc_optimizations(&tokens);
        assert!(result.is_ok());
    }
}
