use js_beautify_rs::{DeobfuscateContext, Options, beautify};

#[test]
fn test_beautify_with_deobfuscation_enabled() {
    let obfuscated_code = r#"
var _0x1234 = ["hello", "world"];
function _0xdec(a) {
    return _0x1234[a];
}
console.log(_0xdec(0));
    "#;

    let mut options = Options::default();
    options.deobfuscate = true;

    let result = beautify(obfuscated_code, &options);
    assert!(result.is_ok());

    let beautified = result.unwrap();

    assert!(
        beautified.contains("hello") || beautified.contains("world"),
        "Should contain inlined strings"
    );

    println!("Beautified with deobfuscation:\n{}", beautified);
}

#[test]
fn test_beautify_without_deobfuscation() {
    let obfuscated_code = r#"
var _0x1234 = ["hello", "world"];
function _0xdec(a) {
    return _0x1234[a];
}
console.log(_0xdec(0));
    "#;

    let options = Options::default();

    let result = beautify(obfuscated_code, &options);
    assert!(result.is_ok());

    let beautified = result.unwrap();

    assert!(
        beautified.contains("_0x1234"),
        "Should preserve obfuscated code when deobfuscation disabled"
    );
    assert!(
        beautified.contains("_0xdec"),
        "Should preserve obfuscated function names"
    );

    println!("Beautified without deobfuscation:\n{}", beautified);
}

#[test]
fn test_full_deobfuscation_pipeline() {
    let obfuscated_code = r#"
var _0x1234 = ["hello", "world", "test"];
(function(_0x2222, _0x3333) {
    var _0x4444 = function(_0x5555) {
        while (--_0x5555) {
            _0x2222.push(_0x2222.shift());
        }
    };
    _0x4444(1);
})(_0x1234, 0x123);
function _0xdec(a) {
    return _0x1234[a];
}
console.log(_0xdec(0));
console.log(_0xdec(1));
    "#;

    let result = beautify(obfuscated_code, &Options::default());
    assert!(result.is_ok());

    let beautified = result.unwrap();

    assert!(
        beautified.contains("hello") || beautified.contains("world") || beautified.contains("test"),
        "Should contain inlined strings"
    );

    let has_decoder = beautified.contains("_0xdec");
    let has_array = beautified.contains("_0x1234");

    println!("Beautified output:\n{}", beautified);
    println!("Has decoder: {}", has_decoder);
    println!("Has array: {}", has_array);
}

#[test]
fn test_deobfuscate_context_workflow() {
    let code = r#"
var _0xabcd = ["foo", "bar"];
function _0xdecode(a) {
    a = a - 291;
    return _0xabcd[a];
}
var x = _0xdecode(292);
    "#;

    let mut tokenizer = js_beautify_rs::tokenizer::Tokenizer::new(code);
    let mut tokens = tokenizer.tokenize().unwrap();

    let mut ctx = DeobfuscateContext::new();
    ctx.analyze(&tokens).unwrap();

    assert_eq!(ctx.string_arrays.len(), 1, "Should find 1 string array");
    assert_eq!(ctx.decoders.len(), 1, "Should find 1 decoder");

    ctx.deobfuscate(&mut tokens).unwrap();

    let has_foo_or_bar = tokens
        .iter()
        .any(|t| t.text.contains("foo") || t.text.contains("bar"));

    let token_text: String = tokens.iter().map(|t| t.text.as_str()).collect();

    assert!(
        has_foo_or_bar,
        "Should have inlined string, got tokens: {}",
        token_text
    );
}
