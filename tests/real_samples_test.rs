use js_beautify_rs::{DeobfuscateContext, tokenizer::Tokenizer};
use std::fs;

#[test]
fn test_simple_string_array_fixture() {
    let code = fs::read_to_string("tests/fixtures/obfuscated/simple_string_array.js")
        .expect("Failed to read fixture");

    let mut tokenizer = Tokenizer::new(&code);
    let mut tokens = tokenizer.tokenize().unwrap();

    let mut ctx = DeobfuscateContext::new();
    ctx.analyze(&tokens).unwrap();

    assert_eq!(ctx.string_arrays.len(), 1, "Should find 1 string array");
    assert_eq!(
        ctx.string_arrays[0].strings.len(),
        4,
        "Should have 4 strings"
    );
    assert!(
        ctx.string_arrays[0].rotated,
        "Array should be marked as rotated"
    );

    assert_eq!(ctx.decoders.len(), 1, "Should find 1 decoder function");
    assert_eq!(ctx.decoders[0].name, "_0xdec");

    ctx.deobfuscate(&mut tokens).unwrap();

    let output: String = tokens.iter().map(|t| t.text.as_str()).collect();

    let has_hello = output.contains("Hello");
    let has_world = output.contains("World");
    let has_test = output.contains("Test");

    assert!(
        has_hello || has_world || has_test,
        "Should contain at least one of the original strings after deobfuscation"
    );

    let has_decoder_removed = !output.contains("function _0xdec");
    assert!(has_decoder_removed, "Decoder function should be removed");
}

#[test]
fn test_string_array_with_offset_fixture() {
    let code = fs::read_to_string("tests/fixtures/obfuscated/string_array_with_offset.js")
        .expect("Failed to read fixture");

    let mut tokenizer = Tokenizer::new(&code);
    let mut tokens = tokenizer.tokenize().unwrap();

    let mut ctx = DeobfuscateContext::new();
    ctx.analyze(&tokens).unwrap();

    assert_eq!(ctx.string_arrays.len(), 1, "Should find 1 string array");
    assert_eq!(ctx.decoders.len(), 1, "Should find 1 decoder");
    assert_eq!(ctx.decoders[0].offset, 100, "Should detect offset of 100");

    ctx.deobfuscate(&mut tokens).unwrap();

    let output: String = tokens.iter().map(|t| t.text.as_str()).collect();

    let has_fruits = output.contains("apple") || output.contains("banana");
    assert!(has_fruits, "Should contain inlined fruit names");
}

#[test]
fn test_no_false_positives_on_normal_code() {
    let code = r#"
var myArray = ["test", "data"];
function getItem(index) {
    return myArray[index];
}
console.log(getItem(0));
    "#;

    let mut tokenizer = Tokenizer::new(code);
    let tokens = tokenizer.tokenize().unwrap();

    let mut ctx = DeobfuscateContext::new();
    ctx.analyze(&tokens).unwrap();

    assert_eq!(
        ctx.string_arrays.len(),
        0,
        "Should not detect normal arrays as obfuscated"
    );
    assert_eq!(
        ctx.decoders.len(),
        0,
        "Should not detect normal functions as decoders"
    );
}
