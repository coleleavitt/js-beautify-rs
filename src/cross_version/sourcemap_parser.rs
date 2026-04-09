//! Sourcemap parsing and name extraction via VLQ decoding.

use crate::{BeautifyError, Result};
use rustc_hash::FxHashMap;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct NameMapping {
    pub minified_name: String,
    pub original_name: String,
    pub source_file: String,
    pub original_line: u32,
    pub original_column: u32,
    pub minified_line: u32,
    pub minified_column: u32,
}

#[derive(Debug, Deserialize)]
struct RawSourcemap {
    sources: Vec<String>,
    #[serde(default)]
    names: Vec<String>,
    mappings: String,
    #[serde(rename = "sourcesContent", default)]
    sources_content: Vec<Option<String>>,
}

pub struct SourcemapParser {
    identifier_pattern: regex::Regex,
}

impl SourcemapParser {
    #[must_use]
    pub fn new() -> Self {
        Self {
            identifier_pattern: regex::Regex::new(r"^[a-zA-Z_$][a-zA-Z0-9_$]*").unwrap(),
        }
    }

    /// Extracts name mappings from a sourcemap JSON and bundle source.
    ///
    /// # Errors
    /// Returns an error if the sourcemap cannot be parsed.
    pub fn extract_names(&self, sourcemap_json: &str, bundle_source: &str) -> Result<Vec<NameMapping>> {
        let raw: RawSourcemap = serde_json::from_str(sourcemap_json)
            .map_err(|e| BeautifyError::InvalidInput(format!("Invalid sourcemap JSON: {e}")))?;

        let bundle_identifiers = self.extract_identifiers_with_positions(bundle_source);
        let decoded = self.decode_mappings(&raw.mappings, raw.sources.len())?;

        let mut mappings = Vec::new();

        for (minified_line, minified_col, source_idx, orig_line, orig_col) in decoded {
            let Some(source_content) = raw.sources_content.get(source_idx).and_then(Option::as_ref) else {
                continue;
            };

            let Some(original_name) = self.get_identifier_at(source_content, orig_line, orig_col) else {
                continue;
            };

            let Some(minified_name) = bundle_identifiers.get(&(minified_line, minified_col)).cloned() else {
                continue;
            };

            mappings.push(NameMapping {
                minified_name,
                original_name,
                source_file: raw.sources.get(source_idx).cloned().unwrap_or_default(),
                original_line: orig_line,
                original_column: orig_col,
                minified_line,
                minified_column: minified_col,
            });
        }

        Ok(mappings)
    }

    fn extract_identifiers_with_positions(&self, source: &str) -> FxHashMap<(u32, u32), String> {
        let mut result = FxHashMap::default();
        let mut in_string = false;
        let mut string_char = ' ';
        let mut prev_char = ' ';
        let mut current_word = String::new();
        let mut word_start_col = 0u32;
        let mut line = 1u32;
        let mut col = 0u32;

        for ch in source.chars() {
            if ch == '\n' {
                if !current_word.is_empty() && self.is_identifier(&current_word) {
                    result.insert((line, word_start_col), current_word.clone());
                }
                current_word.clear();
                line += 1;
                col = 0;
                prev_char = ch;
                continue;
            }

            if !in_string && (ch == '"' || ch == '\'' || ch == '`') {
                if !current_word.is_empty() && self.is_identifier(&current_word) {
                    result.insert((line, word_start_col), current_word.clone());
                }
                current_word.clear();
                in_string = true;
                string_char = ch;
                prev_char = ch;
                col += 1;
                continue;
            }

            if in_string {
                if ch == string_char && prev_char != '\\' {
                    in_string = false;
                }
                prev_char = ch;
                col += 1;
                continue;
            }

            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
                if current_word.is_empty() {
                    word_start_col = col;
                }
                current_word.push(ch);
            } else {
                if !current_word.is_empty() && self.is_identifier(&current_word) {
                    result.insert((line, word_start_col), current_word.clone());
                }
                current_word.clear();
            }

            prev_char = ch;
            col += 1;
        }

        if !current_word.is_empty() && self.is_identifier(&current_word) {
            result.insert((line, word_start_col), current_word);
        }

        result
    }

    fn is_identifier(&self, word: &str) -> bool {
        word.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
    }

    fn get_identifier_at(&self, source: &str, line: u32, column: u32) -> Option<String> {
        let target_line = source.lines().nth((line.saturating_sub(1)) as usize)?;
        let rest = target_line.get((column as usize)..)?;
        self.identifier_pattern.find(rest).map(|m| m.as_str().to_string())
    }

    fn decode_mappings(&self, mappings: &str, source_count: usize) -> Result<Vec<(u32, u32, usize, u32, u32)>> {
        let mut result = Vec::new();
        let mut gen_line = 1u32;
        let mut gen_col: i64;
        let mut source_idx = 0i64;
        let mut orig_line = 0i64;
        let mut orig_col = 0i64;

        for line in mappings.split(';') {
            gen_col = 0;

            if line.is_empty() {
                gen_line += 1;
                continue;
            }

            for segment in line.split(',') {
                if segment.is_empty() {
                    continue;
                }

                let values = decode_vlq(segment)?;
                if values.is_empty() {
                    continue;
                }

                gen_col += values[0];

                if values.len() >= 4 {
                    source_idx += values[1];
                    orig_line += values[2];
                    orig_col += values[3];

                    if source_idx >= 0 && (source_idx as usize) < source_count {
                        result.push((
                            gen_line,
                            gen_col as u32,
                            source_idx as usize,
                            (orig_line + 1) as u32,
                            orig_col as u32,
                        ));
                    }
                }
            }

            gen_line += 1;
        }

        Ok(result)
    }
}

impl Default for SourcemapParser {
    fn default() -> Self {
        Self::new()
    }
}

fn decode_vlq(input: &str) -> Result<Vec<i64>> {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut values = Vec::new();
    let mut shift = 0u32;
    let mut value = 0i64;

    for ch in input.chars() {
        let digit = BASE64_CHARS
            .iter()
            .position(|&c| c == ch as u8)
            .ok_or_else(|| BeautifyError::InvalidInput(format!("Invalid VLQ character: {ch}")))?;

        let digit = digit as i64;
        let continuation = (digit & 32) != 0;
        let digit_value = digit & 31;

        value += digit_value << shift;

        if continuation {
            shift += 5;
        } else {
            let is_negative = (value & 1) != 0;
            value >>= 1;
            if is_negative {
                value = -value;
            }
            values.push(value);
            value = 0;
            shift = 0;
        }
    }

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_vlq_simple() {
        assert_eq!(decode_vlq("A").unwrap(), vec![0]);
        assert_eq!(decode_vlq("C").unwrap(), vec![1]);
        assert_eq!(decode_vlq("D").unwrap(), vec![-1]);
    }

    #[test]
    fn test_decode_vlq_multi() {
        let values = decode_vlq("AAAA").unwrap();
        assert_eq!(values.len(), 4);
    }

    #[test]
    fn test_identifier_extraction() {
        let parser = SourcemapParser::new();
        let source = "var foo = bar;";
        let ids = parser.extract_identifiers_with_positions(source);
        assert!(ids.contains_key(&(1, 4)));
        assert!(ids.contains_key(&(1, 10)));
    }
}
