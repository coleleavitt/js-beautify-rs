//! Bun minifier alphabet extraction and slot computation
//!
//! Bun's minifier uses a frequency-based alphabet ordering:
//! - HEAD (54 chars): Valid identifier start chars (a-z, A-Z, _, $) ordered by frequency
//! - TAIL (64 chars): Valid identifier continuation chars (a-z, A-Z, 0-9, _, $) ordered by frequency
//!
//! This module extracts the alphabet from a minified bundle by analyzing character
//! frequencies, then provides slot number computation for any minified name.
//!
//! The key insight: slot numbers are stable across versions because they represent
//! the frequency rank of each symbol. By normalizing names to slots, we get
//! alphabet-independent comparisons.

use rustc_hash::FxHashMap;

/// Default Bun HEAD alphabet (54 chars - valid identifier starts, no digits)
pub const DEFAULT_HEAD: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_$";

/// Default Bun TAIL alphabet (64 chars - valid identifier continuations, includes digits)
pub const DEFAULT_TAIL: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$";

/// Represents an extracted Bun minifier alphabet
#[derive(Debug, Clone)]
pub struct BunAlphabet {
    /// HEAD alphabet: 54 characters for first position (no digits)
    pub head: String,
    /// TAIL alphabet: 64 characters for subsequent positions (includes digits)
    pub tail: String,
    /// Reverse mapping: char -> position in HEAD
    head_to_pos: FxHashMap<char, usize>,
    /// Reverse mapping: char -> position in TAIL
    tail_to_pos: FxHashMap<char, usize>,
}

impl BunAlphabet {
    /// Creates a new alphabet from HEAD and TAIL strings
    #[must_use]
    pub fn new(head: String, tail: String) -> Self {
        let head_to_pos: FxHashMap<char, usize> = head.chars().enumerate().map(|(i, c)| (c, i)).collect();
        let tail_to_pos: FxHashMap<char, usize> = tail.chars().enumerate().map(|(i, c)| (c, i)).collect();

        Self {
            head,
            tail,
            head_to_pos,
            tail_to_pos,
        }
    }

    /// Creates the default Bun alphabet (alphabetical order)
    #[must_use]
    pub fn default_alphabet() -> Self {
        Self::new(DEFAULT_HEAD.to_string(), DEFAULT_TAIL.to_string())
    }

    /// Converts a slot number to a minified name using this alphabet
    ///
    /// This is Bun's `numberToMinifiedName` algorithm:
    /// - First char: slot % 54 -> HEAD[index]
    /// - Subsequent chars: (slot / 54) in base-64 -> TAIL[index]
    #[must_use]
    pub fn slot_to_name(&self, mut slot: usize) -> String {
        let head_chars: Vec<char> = self.head.chars().collect();
        let tail_chars: Vec<char> = self.tail.chars().collect();

        let mut name = String::new();

        // First character from HEAD (base-54)
        let first_idx = slot % 54;
        name.push(head_chars.get(first_idx).copied().unwrap_or('?'));
        slot /= 54;

        // Subsequent characters from TAIL (base-64)
        while slot > 0 {
            slot -= 1;
            let idx = slot % 64;
            name.push(tail_chars.get(idx).copied().unwrap_or('?'));
            slot /= 64;
        }

        name
    }

    /// Converts a minified name back to its slot number
    ///
    /// Returns None if the name contains characters not in the alphabet
    #[must_use]
    pub fn name_to_slot(&self, name: &str) -> Option<usize> {
        let chars: Vec<char> = name.chars().collect();
        if chars.is_empty() {
            return None;
        }

        // First character uses HEAD alphabet (base-54)
        let first_pos = self.head_to_pos.get(&chars[0])?;
        let mut slot = *first_pos;

        // Subsequent characters use TAIL alphabet (base-64)
        if chars.len() > 1 {
            let mut multiplier = 54_usize; // First position was base-54
            for &c in &chars[1..] {
                let pos = self.tail_to_pos.get(&c)?;
                slot += (pos + 1) * multiplier;
                multiplier *= 64;
            }
        }

        Some(slot)
    }

    /// Checks if a name looks like a Bun-minified identifier
    #[must_use]
    pub fn is_minified_name(&self, name: &str) -> bool {
        let len = name.len();

        // Must be 1-4 characters (Bun typically uses short names)
        if len == 0 || len > 4 {
            return false;
        }

        // All characters must be in the alphabet
        let chars: Vec<char> = name.chars().collect();

        // First char must be in HEAD
        if !self.head_to_pos.contains_key(&chars[0]) {
            return false;
        }

        // Subsequent chars must be in TAIL
        for &c in &chars[1..] {
            if !self.tail_to_pos.contains_key(&c) {
                return false;
            }
        }

        true
    }
}

impl Default for BunAlphabet {
    fn default() -> Self {
        Self::default_alphabet()
    }
}

/// Extracts the Bun alphabet from source code by analyzing identifier frequencies
///
/// Algorithm:
/// 1. Count frequency of each single-character identifier -> HEAD ordering
/// 2. Count frequency of second character in 2-char identifiers -> TAIL ordering
#[derive(Debug, Default)]
pub struct AlphabetExtractor {
    /// Frequency count for single-char identifiers (determines HEAD)
    single_char_freq: FxHashMap<char, usize>,
    /// Frequency count for second char in 2-char identifiers (determines TAIL)
    second_char_freq: FxHashMap<char, usize>,
}

impl AlphabetExtractor {
    /// Creates a new alphabet extractor
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an identifier for frequency analysis
    pub fn record_identifier(&mut self, name: &str) {
        let chars: Vec<char> = name.chars().collect();
        let len = chars.len();

        // Skip non-minified looking names
        if len == 0 || len > 4 {
            return;
        }

        // Skip names that look like keywords or common patterns
        if is_likely_keyword_or_common(name) {
            return;
        }

        // Single-char identifiers contribute to HEAD frequency
        if len == 1 {
            let c = chars[0];
            if is_valid_head_char(c) {
                *self.single_char_freq.entry(c).or_insert(0) += 1;
            }
        }

        // Two-char identifiers: second char contributes to TAIL frequency
        if len == 2 {
            let c = chars[1];
            if is_valid_tail_char(c) {
                *self.second_char_freq.entry(c).or_insert(0) += 1;
            }
        }
    }

    /// Builds the extracted alphabet from recorded frequencies
    #[must_use]
    pub fn build_alphabet(&self) -> BunAlphabet {
        // Build HEAD: sort single-char frequencies descending
        let head = build_sorted_alphabet(&self.single_char_freq, false);

        // Build TAIL: sort second-char frequencies descending
        let tail = build_sorted_alphabet(&self.second_char_freq, true);

        BunAlphabet::new(head, tail)
    }

    /// Returns the number of single-char identifiers recorded
    #[must_use]
    pub fn single_char_count(&self) -> usize {
        self.single_char_freq.values().sum()
    }

    /// Returns the number of two-char identifiers recorded
    #[must_use]
    pub fn two_char_count(&self) -> usize {
        self.second_char_freq.values().sum()
    }
}

/// Checks if a character is valid for HEAD (identifier start)
fn is_valid_head_char(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

/// Checks if a character is valid for TAIL (identifier continuation)
fn is_valid_tail_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// Checks if a name is likely a keyword or common non-minified identifier
fn is_likely_keyword_or_common(name: &str) -> bool {
    matches!(
        name,
        // Keywords
        "if" | "in" | "do" | "of" | "is" | "as" | "to" | "or" | "on" | "be" | "at" | "no" | "it" | "an" | "by" |
        // Common abbreviations
        "id" | "fn" | "el" | "fs" | "os" | "db" | "ui" | "io" | "rx" | "tx" | "ok" | "vs" | "re" | "eq" | "ne" |
        "gt" | "lt" | "ge" | "le" | "up" | "md"
    )
}

/// Builds a sorted alphabet string from frequency map
fn build_sorted_alphabet(freq: &FxHashMap<char, usize>, include_digits: bool) -> String {
    let mut chars: Vec<(char, usize)> = freq.iter().map(|(&c, &count)| (c, count)).collect();

    // Sort by frequency descending, then by char for stability
    chars.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut result: Vec<char> = chars.into_iter().map(|(c, _)| c).collect();

    // Ensure we have all required characters (fill in missing ones from default)
    let default_chars: &str = if include_digits { DEFAULT_TAIL } else { DEFAULT_HEAD };

    for c in default_chars.chars() {
        if !result.contains(&c) {
            result.push(c);
        }
    }

    // Truncate to correct length
    let target_len = if include_digits { 64 } else { 54 };
    result.truncate(target_len);

    result.into_iter().collect()
}

/// Extracts the Bun alphabet from raw JavaScript source code
///
/// This is a convenience function that scans the code and extracts all identifiers.
#[must_use]
pub fn extract_alphabet_from_source(source: &str) -> BunAlphabet {
    let mut extractor = AlphabetExtractor::new();

    // Simple regex-free identifier extraction
    // Look for patterns like: var X, let X, const X, function X, (X) =>, X:, X =
    let mut chars = source.chars().peekable();
    let mut current_word = String::new();
    let mut in_string = false;
    let mut string_char = ' ';

    while let Some(c) = chars.next() {
        // Handle string literals
        if !in_string && (c == '"' || c == '\'' || c == '`') {
            in_string = true;
            string_char = c;
            continue;
        }
        if in_string {
            if c == string_char && source.chars().nth(chars.clone().count().saturating_sub(1)) != Some('\\') {
                in_string = false;
            }
            continue;
        }

        // Build identifier
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            current_word.push(c);
        } else {
            // End of identifier
            if !current_word.is_empty() {
                // Check if it starts with a valid identifier char (not digit)
                if current_word
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_alphabetic() || first == '_' || first == '$')
                {
                    extractor.record_identifier(&current_word);
                }
                current_word.clear();
            }
        }
    }

    // Don't forget the last word
    if !current_word.is_empty()
        && current_word
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_' || first == '$')
    {
        extractor.record_identifier(&current_word);
    }

    eprintln!(
        "[BUN_ALPHABET] Extracted alphabet from {} single-char and {} two-char identifiers",
        extractor.single_char_count(),
        extractor.two_char_count()
    );

    extractor.build_alphabet()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_to_name_single_char() {
        let alphabet = BunAlphabet::default_alphabet();

        // Slot 0 -> 'a' (first char of default HEAD)
        assert_eq!(alphabet.slot_to_name(0), "a");
        // Slot 1 -> 'b'
        assert_eq!(alphabet.slot_to_name(1), "b");
        // Slot 25 -> 'z'
        assert_eq!(alphabet.slot_to_name(25), "z");
        // Slot 26 -> 'A'
        assert_eq!(alphabet.slot_to_name(26), "A");
        // Slot 52 -> '_'
        assert_eq!(alphabet.slot_to_name(52), "_");
        // Slot 53 -> '$'
        assert_eq!(alphabet.slot_to_name(53), "$");
    }

    #[test]
    fn test_slot_to_name_two_char() {
        let alphabet = BunAlphabet::default_alphabet();

        // Slot 54 -> 'aa' (first char from HEAD[0], second from TAIL[0])
        assert_eq!(alphabet.slot_to_name(54), "aa");
        // Slot 55 -> 'ba'
        assert_eq!(alphabet.slot_to_name(55), "ba");
    }

    #[test]
    fn test_name_to_slot_single_char() {
        let alphabet = BunAlphabet::default_alphabet();

        assert_eq!(alphabet.name_to_slot("a"), Some(0));
        assert_eq!(alphabet.name_to_slot("b"), Some(1));
        assert_eq!(alphabet.name_to_slot("z"), Some(25));
        assert_eq!(alphabet.name_to_slot("A"), Some(26));
        assert_eq!(alphabet.name_to_slot("_"), Some(52));
        assert_eq!(alphabet.name_to_slot("$"), Some(53));
    }

    #[test]
    fn test_name_to_slot_two_char() {
        let alphabet = BunAlphabet::default_alphabet();

        assert_eq!(alphabet.name_to_slot("aa"), Some(54));
        assert_eq!(alphabet.name_to_slot("ba"), Some(55));
    }

    #[test]
    fn test_roundtrip() {
        let alphabet = BunAlphabet::default_alphabet();

        for slot in 0..1000 {
            let name = alphabet.slot_to_name(slot);
            let recovered = alphabet.name_to_slot(&name);
            assert_eq!(recovered, Some(slot), "Roundtrip failed for slot {slot} -> {name}");
        }
    }

    #[test]
    fn test_custom_alphabet() {
        // Simulate extracted alphabet where 'q' is most frequent
        let head = "qKzYOAwjHJMXuPWfaDZsvGLkbTVgyxEShNRCdnpmIt$iBreFcUQol".to_string();
        let tail = "f68o1ns7q4drKte53_A9$zYO0y2pwjHDMiPaJXkRWvZThNmGLbclESIuVxCgBFQU".to_string();
        let alphabet = BunAlphabet::new(head, tail);

        // Slot 0 should now be 'q'
        assert_eq!(alphabet.slot_to_name(0), "q");
        // Slot 1 should be 'K'
        assert_eq!(alphabet.slot_to_name(1), "K");

        // And reverse
        assert_eq!(alphabet.name_to_slot("q"), Some(0));
        assert_eq!(alphabet.name_to_slot("K"), Some(1));
    }

    #[test]
    fn test_extract_alphabet() {
        let source = r#"
            var q = 1;
            var K = 2;
            var z = 3;
            var q6 = 4;
            var K8 = 5;
        "#;

        let alphabet = extract_alphabet_from_source(source);

        // 'q' should be early in the alphabet (high frequency)
        let q_pos = alphabet.head.find('q');
        let k_pos = alphabet.head.find('K');
        let z_pos = alphabet.head.find('z');

        assert!(q_pos.is_some());
        assert!(k_pos.is_some());
        assert!(z_pos.is_some());

        // All should be in first few positions
        assert!(q_pos.unwrap() < 10);
        assert!(k_pos.unwrap() < 10);
        assert!(z_pos.unwrap() < 10);
    }
}
