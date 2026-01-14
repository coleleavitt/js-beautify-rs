use crate::Result;
use crate::token::{Token, TokenType};
use std::collections::HashMap;

const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // ZERO WIDTH SPACE
    '\u{200C}', // ZERO WIDTH NON-JOINER
    '\u{200D}', // ZERO WIDTH JOINER
    '\u{FEFF}', // ZERO WIDTH NO-BREAK SPACE
    '\u{2060}', // WORD JOINER
];

pub fn normalize_unicode(tokens: &[Token]) -> Result<Vec<Token>> {
    let mut result = Vec::new();
    let mut rename_map: HashMap<String, String> = HashMap::new();

    for token in tokens.iter() {
        let mut new_token = token.clone();

        match token.token_type {
            TokenType::Word | TokenType::String => {
                if contains_problematic_unicode(&token.text) {
                    let normalized = normalize_identifier(&token.text);

                    if token.token_type == TokenType::Word {
                        if let Some(renamed) = rename_map.get(&token.text) {
                            new_token.text = renamed.clone();
                        } else {
                            let clean_name = generate_clean_name(&normalized, rename_map.len());
                            debug_assert!(!clean_name.is_empty(), "Clean name cannot be empty");
                            debug_assert!(
                                clean_name.chars().next().unwrap().is_ascii_alphanumeric()
                                    || clean_name.starts_with('_'),
                                "Clean name must start with alphanumeric or underscore"
                            );
                            rename_map.insert(token.text.clone(), clean_name.clone());
                            new_token.text = clean_name;
                        }
                    } else {
                        new_token.text = normalized;
                    }
                }
            }
            _ => {}
        }

        result.push(new_token);
    }

    Ok(result)
}

fn contains_problematic_unicode(text: &str) -> bool {
    for ch in text.chars() {
        if ZERO_WIDTH_CHARS.contains(&ch) {
            return true;
        }

        if is_confusable(ch) {
            return true;
        }
    }
    false
}

fn normalize_identifier(text: &str) -> String {
    text.chars()
        .filter(|ch| !ZERO_WIDTH_CHARS.contains(ch))
        .map(|ch| normalize_confusable(ch))
        .collect()
}

fn is_confusable(ch: char) -> bool {
    matches!(
        ch,
        '\u{0410}'..='\u{044F}' | // Cyrillic
        '\u{0391}'..='\u{03C9}' | // Greek
        '\u{2000}'..='\u{206F}'   // General Punctuation with confusables
    )
}

fn normalize_confusable(ch: char) -> char {
    match ch {
        '\u{0410}' => 'A', // Cyrillic A
        '\u{0412}' => 'B', // Cyrillic Ve
        '\u{0415}' => 'E', // Cyrillic Ie
        '\u{041A}' => 'K', // Cyrillic Ka
        '\u{041C}' => 'M', // Cyrillic Em
        '\u{041D}' => 'H', // Cyrillic En
        '\u{041E}' => 'O', // Cyrillic O
        '\u{0420}' => 'P', // Cyrillic Er
        '\u{0421}' => 'C', // Cyrillic Es
        '\u{0422}' => 'T', // Cyrillic Te
        '\u{0425}' => 'X', // Cyrillic Ha
        '\u{0430}' => 'a', // Cyrillic a
        '\u{0435}' => 'e', // Cyrillic ie
        '\u{043E}' => 'o', // Cyrillic o
        '\u{0440}' => 'p', // Cyrillic er
        '\u{0441}' => 'c', // Cyrillic es
        '\u{0445}' => 'x', // Cyrillic ha

        '\u{0391}' => 'A', // Greek Alpha
        '\u{0392}' => 'B', // Greek Beta
        '\u{0395}' => 'E', // Greek Epsilon
        '\u{0396}' => 'Z', // Greek Zeta
        '\u{0397}' => 'H', // Greek Eta
        '\u{0399}' => 'I', // Greek Iota
        '\u{039A}' => 'K', // Greek Kappa
        '\u{039C}' => 'M', // Greek Mu
        '\u{039D}' => 'N', // Greek Nu
        '\u{039F}' => 'O', // Greek Omicron
        '\u{03A1}' => 'P', // Greek Rho
        '\u{03A4}' => 'T', // Greek Tau
        '\u{03A7}' => 'X', // Greek Chi
        '\u{03A5}' => 'Y', // Greek Upsilon

        _ => ch,
    }
}

fn generate_clean_name(base: &str, index: usize) -> String {
    if base.is_empty() || !base.chars().next().unwrap().is_alphabetic() {
        format!("_unicode_{}", index)
    } else {
        let clean: String = base
            .chars()
            .filter(|ch| ch.is_alphanumeric() || *ch == '_')
            .collect();

        if clean.is_empty() {
            format!("_unicode_{}", index)
        } else if clean == base {
            base.to_string()
        } else {
            format!("{}_{}", clean, index)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    #[test]
    fn test_detect_zero_width_chars() {
        assert!(contains_problematic_unicode("hello\u{200B}world"));
        assert!(contains_problematic_unicode("test\u{200C}"));
        assert!(contains_problematic_unicode("\u{FEFF}data"));
        assert!(!contains_problematic_unicode("normal"));
    }

    #[test]
    fn test_normalize_zero_width() {
        let text = "hello\u{200B}world\u{200C}test";
        let normalized = normalize_identifier(text);
        assert_eq!(normalized, "helloworldtest");
        assert!(!normalized.contains('\u{200B}'));
        assert!(!normalized.contains('\u{200C}'));
    }

    #[test]
    fn test_normalize_cyrillic_confusables() {
        let text = "АВС"; // Cyrillic A, Ve, Es
        let normalized = normalize_identifier(text);
        assert_eq!(normalized, "ABC");
    }

    #[test]
    fn test_normalize_tokens() {
        let mut tokens = vec![
            Token::new(TokenType::Reserved, "var".to_string()),
            Token::new(TokenType::Word, "hello\u{200B}world".to_string()),
            Token::new(TokenType::Equals, "=".to_string()),
            Token::new(TokenType::Number, "123".to_string()),
            Token::new(TokenType::Semicolon, ";".to_string()),
        ];

        let result = normalize_unicode(&tokens).unwrap();

        let has_zero_width = result
            .iter()
            .any(|t| t.text.chars().any(|ch| ZERO_WIDTH_CHARS.contains(&ch)));

        assert!(!has_zero_width, "Should remove zero-width characters");

        let word_token = result
            .iter()
            .find(|t| t.token_type == TokenType::Word)
            .unwrap();
        assert!(
            !word_token.text.contains('\u{200B}'),
            "Word should not contain zero-width chars"
        );
    }

    #[test]
    fn test_rename_confusable_identifiers() {
        let tokens = vec![
            Token::new(TokenType::Reserved, "var".to_string()),
            Token::new(TokenType::Word, "АВС".to_string()), // Cyrillic ABC
            Token::new(TokenType::Equals, "=".to_string()),
            Token::new(TokenType::Number, "1".to_string()),
            Token::new(TokenType::Semicolon, ";".to_string()),
        ];

        let result = normalize_unicode(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            !output.contains('А'),
            "Should normalize Cyrillic, got: {}",
            output
        );

        let word_token = result
            .iter()
            .find(|t| t.token_type == TokenType::Word)
            .unwrap();
        assert!(
            !word_token.text.contains('А'),
            "Word token should not contain Cyrillic, got: {}",
            word_token.text
        );
    }

    #[test]
    fn test_preserve_normal_identifiers() {
        let code = "var normalVar = 123;";
        let mut tokenizer = Tokenizer::new(code);
        let tokens = tokenizer.tokenize().unwrap();

        let result = normalize_unicode(&tokens).unwrap();
        let output: String = result.iter().map(|t| t.text.as_str()).collect();

        assert!(
            output.contains("normalVar"),
            "Should preserve normal identifiers, got: {}",
            output
        );
    }
}
