//! Pre-AST decryption of PRNG XOR + Caesar cipher encrypted eval patterns.
//!
//! Detects and decrypts the encryption scheme used by Tycoon2FA and similar
//! phishing kits. The pattern embeds a colon-delimited string containing:
//!
//! ```text
//! "BASE64_PAYLOAD:COUNTER:BASE64_SEED"
//! ```
//!
//! followed by a `.split(':')` call (possibly obfuscated). The payload is
//! decrypted using a PRNG-seeded XOR keystream combined with a Caesar cipher,
//! then `eval()`'d via color-hex steganography or similar indirection.
//!
//! This pass runs as Phase 0 (before AST parsing) because the encrypted blob
//! is not valid JavaScript — it must be decrypted at the source level.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use regex::Regex;

/// Regex pattern for the colon-delimited encrypted blob inside a string literal.
///
/// Matches: `"<base64_payload>:<counter>:<base64_seed>"`
/// where payload is at least 200 base64 chars, counter is 1-10 digits,
/// and seed is a shorter base64 string.
const BLOB_PATTERN: &str = r#""([A-Za-z0-9+/=]{200,}):(\d{1,10}):([A-Za-z0-9+/=]+)""#;

/// Regex pattern for the split call that follows the blob.
///
/// Matches any of:
/// - `.split(':')`
/// - `["split"](":")`
/// - `[["s","p","l","i","t"].join("")](":")`
const SPLIT_PATTERN: &str = concat!(
    r"(?:",
    r#"\.split\s*\(\s*"[:]"\s*\)"#, // .split(':')
    r"|",
    r#"\[\s*"split"\s*\]\s*\(\s*"[:]"\s*\)"#, // ["split"](":")
    r"|",
    r#"\[\s*\[\s*"s"\s*,\s*"p"\s*,\s*"l"\s*,\s*"i"\s*,\s*"t"\s*\]"#, // [["s","p","l","i","t"]
    r#"\s*\.\s*join\s*\(\s*""\s*\)\s*\]\s*\(\s*"[:]"\s*\)"#,         //  .join("")](":")"
    r")",
);

/// Linear congruential PRNG matching the JavaScript implementation.
///
/// Uses the recurrence: `state = (state * 9301 + 49297) % 233280`
/// and returns `state / 233280.0` as the output value.
struct Prng {
    state: u64,
}

impl Prng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> f64 {
        self.state = (self.state * 9301 + 49297) % 233280;
        self.state as f64 / 233280.0
    }
}

/// Decrypts a PRNG XOR + Caesar cipher encrypted payload.
///
/// # Algorithm
///
/// 1. Base64-decode the payload and seed
/// 2. Generate XOR keystream using PRNG seeded with `counter + seed[0]`
/// 3. Generate Caesar shift values using PRNG seeded with `counter + 99`
/// 4. For each byte: reverse Caesar shift (if ASCII alpha), then XOR
fn decrypt_payload(payload_b64: &str, counter_str: &str, seed_b64: &str) -> Option<String> {
    let counter: u64 = counter_str.parse().ok()?;
    let seed = BASE64_STANDARD.decode(seed_b64).ok()?;
    let payload = BASE64_STANDARD.decode(payload_b64).ok()?;

    if seed.is_empty() || payload.is_empty() {
        return None;
    }

    let payload_len = payload.len();

    // Generate XOR keystream
    let mut xor_prng = Prng::new(counter + seed[0] as u64);
    let xor_key: Vec<u8> = (0..payload_len)
        .map(|_| (xor_prng.next() * 256.0).floor() as u8)
        .collect();

    // Generate Caesar shift values
    let mut caesar_prng = Prng::new(counter + 99);
    let shifts: Vec<u8> = (0..payload_len)
        .map(|_| (caesar_prng.next() * 25.0).floor() as u8 + 1)
        .collect();

    // Decrypt: reverse Caesar (ASCII alpha only) then XOR
    let mut result = String::with_capacity(payload_len);
    for i in 0..payload_len {
        let b = payload[i];
        let mut wd = b as i32;

        // Only apply Caesar shift to ASCII letters (A-Z, a-z)
        // This matches JavaScript's /[A-Za-z]/.test() which is ASCII-only
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_uppercase() { 65 } else { 97 };
            wd = ((wd - base - shifts[i] as i32 + 26) % 26) + base;
        }

        wd ^= xor_key[i] as i32;
        result.push(char::from(wd as u8));
    }

    // Sanity check: decrypted output should look like JavaScript
    // (contains common JS keywords or patterns)
    let trimmed = result.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Check for at least one JS-like indicator
    let has_js_indicator = trimmed.contains("function")
        || trimmed.contains("var ")
        || trimmed.contains("const ")
        || trimmed.contains("let ")
        || trimmed.contains("document")
        || trimmed.contains("window")
        || trimmed.contains("if (")
        || trimmed.contains("if(");

    if !has_js_indicator {
        eprintln!("[DEOBFUSCATE] Phase 0: Decrypted payload does not look like JavaScript, skipping");
        return None;
    }

    Some(result)
}

/// Detects and decrypts PRNG XOR + Caesar cipher encrypted eval patterns.
///
/// Scans the source for the colon-delimited base64 blob pattern followed by
/// a split call. If found, decrypts the payload and replaces the entire
/// encrypted script body with the decrypted JavaScript.
///
/// Returns `Some(decrypted_source)` if an encrypted pattern was found and
/// successfully decrypted, or `None` if no pattern was detected.
pub fn decrypt_encrypted_evals(source: &str) -> Option<String> {
    // Strip HTML wrapper if present — extract JS from <script> tags
    let js_source = extract_script_content(source);
    let working_source = js_source.as_deref().unwrap_or(source);

    let blob_re = Regex::new(BLOB_PATTERN).ok()?;
    let split_re = Regex::new(SPLIT_PATTERN).ok()?;

    // Find the encrypted blob
    let blob_match = blob_re.find(working_source)?;
    let blob_end = blob_match.end();

    // Check that a split call follows the blob (possibly with whitespace between)
    let after_blob = &working_source[blob_end..];
    let trimmed_after = after_blob.trim_start();
    if !split_re.is_match_at(trimmed_after, 0) {
        return None;
    }

    // Extract the three parts from the blob
    let caps = blob_re.captures(working_source)?;
    let payload_b64 = caps.get(1)?.as_str();
    let counter_str = caps.get(2)?.as_str();
    let seed_b64 = caps.get(3)?.as_str();

    eprintln!(
        "[DEOBFUSCATE] Phase 0: Found encrypted eval pattern (payload: {} bytes b64, counter: {}, seed: {})",
        payload_b64.len(),
        counter_str,
        seed_b64
    );

    let decrypted = decrypt_payload(payload_b64, counter_str, seed_b64)?;

    eprintln!(
        "[DEOBFUSCATE] Phase 0: Successfully decrypted {} bytes of JavaScript",
        decrypted.len()
    );

    // If the original source was HTML-wrapped, re-wrap the decrypted JS
    if js_source.is_some() {
        Some(decrypted)
    } else {
        Some(decrypted)
    }
}

/// Extracts JavaScript content from `<script>...</script>` tags.
///
/// Returns `Some(js_content)` if a script tag was found, `None` otherwise.
fn extract_script_content(source: &str) -> Option<String> {
    // Case-insensitive search for <script> tags
    let lower = source.to_ascii_lowercase();
    let start_tag_pos = lower.find("<script")?;

    // Find the end of the opening tag (handle attributes)
    let tag_close = lower[start_tag_pos..].find('>')?;
    let js_start = start_tag_pos + tag_close + 1;

    // Find closing </script>
    let end_tag_pos = lower[js_start..].find("</script>")?;
    let js_end = js_start + end_tag_pos;

    let js_content = source[js_start..js_end].trim();
    if js_content.is_empty() {
        return None;
    }

    Some(js_content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encrypts plaintext using the forward PRNG XOR + Caesar cipher.
    /// This is the inverse of `decrypt_payload` — used to build test fixtures.
    fn encrypt_payload(plaintext: &[u8], counter: u64, seed_bytes: &[u8]) -> Vec<u8> {
        let mut xor_prng = Prng::new(counter + seed_bytes[0] as u64);
        let xor_key: Vec<u8> = (0..plaintext.len())
            .map(|_| (xor_prng.next() * 256.0).floor() as u8)
            .collect();

        let mut caesar_prng = Prng::new(counter + 99);
        let shifts: Vec<u8> = (0..plaintext.len())
            .map(|_| (caesar_prng.next() * 25.0).floor() as u8 + 1)
            .collect();

        let mut encrypted = Vec::with_capacity(plaintext.len());
        for i in 0..plaintext.len() {
            let mut b = plaintext[i] ^ xor_key[i];
            if b.is_ascii_alphabetic() {
                let base = if b.is_ascii_uppercase() { 65 } else { 97 };
                b = ((b - base + shifts[i]) % 26) + base;
            }
            encrypted.push(b);
        }
        encrypted
    }

    /// Builds a synthetic encrypted source string with the given plaintext,
    /// padded to at least `min_bytes` to meet the 200-char base64 minimum.
    fn build_encrypted_source(plaintext: &[u8], counter: u64, seed_bytes: &[u8], split_variant: &str) -> String {
        let min_bytes = 200;
        let mut padded = vec![b' '; min_bytes.max(plaintext.len())];
        padded[..plaintext.len()].copy_from_slice(plaintext);

        let encrypted = encrypt_payload(&padded, counter, seed_bytes);
        let payload_b64 = BASE64_STANDARD.encode(&encrypted);
        let seed_b64 = BASE64_STANDARD.encode(seed_bytes);

        format!(r#"const nf = "{payload_b64}:{counter}:{seed_b64}"{split_variant}"#)
    }

    #[test]
    fn test_prng_deterministic() {
        let mut prng = Prng::new(170526);
        // (170526 * 9301 + 49297) % 233280 = 40903
        let val = prng.next();
        assert!((val - 40903.0 / 233280.0).abs() < 1e-10);
    }

    #[test]
    fn test_decrypt_roundtrip() {
        let plaintext = b"var x = 1;";
        let seed_bytes = b"\x00";
        let encrypted = encrypt_payload(plaintext, 1, seed_bytes);

        let payload_b64 = BASE64_STANDARD.encode(&encrypted);
        let seed_b64 = BASE64_STANDARD.encode(seed_bytes);

        let decrypted = decrypt_payload(&payload_b64, "1", &seed_b64);
        assert_eq!(decrypted.as_deref(), Some("var x = 1;"));
    }

    #[test]
    fn test_no_match_on_normal_js() {
        let normal_js = r#"const x = "hello world"; console.log(x);"#;
        assert!(decrypt_encrypted_evals(normal_js).is_none());
    }

    #[test]
    fn test_extract_script_content() {
        let html = r"<html><body><script>var x = 1;</script></body></html>";
        assert_eq!(extract_script_content(html).as_deref(), Some("var x = 1;"));
    }

    #[test]
    fn test_extract_script_with_attributes() {
        let html = r#"<script type="text/javascript">var x = 1;</script>"#;
        assert_eq!(extract_script_content(html).as_deref(), Some("var x = 1;"));
    }

    #[test]
    fn test_detect_dot_split() {
        let source = build_encrypted_source(b"var x = 42;", 100, b"test", r#".split(":")"#);
        let result = decrypt_encrypted_evals(&source);
        assert!(result.is_some(), "Should detect .split(\":\") pattern");
        assert!(result.as_deref().unwrap_or("").starts_with("var x = 42;"));
    }

    #[test]
    fn test_detect_bracket_split() {
        let source = build_encrypted_source(b"const y = true;", 50, b"AB", r#"["split"](":")"#);
        let result = decrypt_encrypted_evals(&source);
        assert!(result.is_some(), "Should detect [\"split\"](\":\") pattern");
    }

    #[test]
    fn test_detect_join_split() {
        let source = build_encrypted_source(b"let z = null;", 75, b"XY", r#"[["s","p","l","i","t"].join("")](":")"#);
        let result = decrypt_encrypted_evals(&source);
        assert!(result.is_some());
    }

    #[test]
    fn test_real_tycoon2fa_payload() {
        let html = std::fs::read_to_string(
            "/home/cole/VulnerabilityResearch/fiwealth.com/phishing-kit/raw/stage2_after_post.html",
        );
        let html = match html {
            Ok(h) => h,
            Err(_) => {
                eprintln!("Skipping real payload test: file not found");
                return;
            }
        };

        let result = decrypt_encrypted_evals(&html);
        assert!(result.is_some(), "Should decrypt real Tycoon2FA payload");

        let decrypted = result.unwrap_or_default();
        assert!(
            decrypted.contains("hsinchucity.com"),
            "Decrypted JS should contain hsinchucity.com"
        );
        assert!(
            decrypted.contains("commercetools.com"),
            "Decrypted JS should contain commercetools.com"
        );
        assert!(
            decrypted.contains("globalThis"),
            "Decrypted JS should contain globalThis"
        );
        assert!(
            decrypted.contains("addEventListener"),
            "Decrypted JS should contain addEventListener"
        );
    }
}
