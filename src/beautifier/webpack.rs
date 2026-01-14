use super::Beautifier;
use crate::token::TokenType;

pub(super) trait WebpackDetector {
    fn is_webpack_require_chain(&self) -> bool;
    fn is_webpack_module_start(&self) -> bool;
    fn should_extract_large_asset(&self, text: &str) -> bool;
}

impl<'a> WebpackDetector for Beautifier<'a> {
    /// Detects webpack require chains like: t(123), t(456), t(789)
    /// This pattern appears when webpack bundles have multiple requires on one line
    fn is_webpack_require_chain(&self) -> bool {
        if self.current_index < 3 {
            return false;
        }

        // Look back at previous tokens to detect pattern: word(number),
        let prev1 = &self.tokens[self.current_index - 1];
        let prev2 = &self.tokens[self.current_index - 2];

        // Check if we have: EndExpr followed by a common webpack variable name
        // Common webpack function names: t, n, r, e, o, i, a
        prev1.token_type == TokenType::EndExpr
            && prev2.token_type == TokenType::Word
            && matches!(prev2.text.as_str(), "t" | "n" | "r" | "e" | "o" | "i" | "a")
            && prev2.text.len() == 1
    }

    /// Detects webpack module definitions like: 12345: function(e, t, n) {
    /// This pattern marks the start of a webpack module boundary
    fn is_webpack_module_start(&self) -> bool {
        if self.current_index < 2 {
            return false;
        }

        let current = &self.tokens[self.current_index];
        let prev = &self.tokens[self.current_index - 1];

        prev.token_type == TokenType::Colon
            && current.token_type == TokenType::Reserved
            && current.text == "function"
    }

    /// Checks if a string asset should be extracted to a separate file
    /// Based on the configured threshold (default 10KB)
    fn should_extract_large_asset(&self, text: &str) -> bool {
        self.options.extract_large_assets && text.len() > self.options.asset_size_threshold
    }
}
