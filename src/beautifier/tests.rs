use super::*;

#[test]
fn test_webpack_require_chain_breaking() {
    let code = "var r=t(123),n=t(456),o=t(789);";
    let mut options = Options::default();
    options.break_webpack_imports = true;
    let result = beautify(code, &options).unwrap();

    let lines: Vec<&str> = result.lines().collect();
    assert!(lines.len() > 1, "Should break into multiple lines");
}

#[test]
fn test_webpack_require_chain_disabled() {
    let code = "var r=t(123),n=t(456),o=t(789);";
    let mut options = Options::default();
    options.break_webpack_imports = false;
    let result = beautify(code, &options).unwrap();

    println!("Result: {:?}", result);
    assert!(result.contains("t(123)"));
    assert!(result.contains("t(456)"));
    assert!(!result.contains("\nt("));
}

#[test]
fn test_webpack_module_separators() {
    let code = "12345: function(e, t, n) { return 1; }";
    let mut options = Options::default();
    options.add_webpack_module_separators = true;
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("===="), "Should contain separator");
}

#[test]
fn test_large_asset_extraction() {
    let large_string = format!("\"{}\"", "x".repeat(15000));
    let code = format!("var svg = {};", large_string);

    let mut options = Options::default();
    options.extract_large_assets = true;
    options.asset_size_threshold = 10000;
    let result = beautify(&code, &options).unwrap();

    assert!(
        result.contains("__WEBPACK_LARGE_ASSET_"),
        "Should extract large asset"
    );
}

#[test]
fn test_small_asset_not_extracted() {
    let code = "var icon = \"small string\";";
    let mut options = Options::default();
    options.extract_large_assets = true;
    options.asset_size_threshold = 10000;
    let result = beautify(code, &options).unwrap();

    assert!(
        !result.contains("__WEBPACK_LARGE_ASSET_"),
        "Should not extract small asset"
    );
    assert!(result.contains("\"small string\""));
}

#[test]
fn test_nested_blocks_indentation() {
    let code = "function test() { if (x) { return 42; } }";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    let lines: Vec<&str> = result.lines().collect();
    assert!(
        lines.iter().any(|l| l.starts_with("        ")),
        "Should have double indentation"
    );
}

#[test]
fn test_arrow_functions() {
    let code = "const fn = (x) => { return x * 2; };";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("=>"));
    assert!(result.contains("return x * 2;"));
}

#[test]
fn test_template_literals() {
    let code = "const msg = `Hello ${name}`;";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("`Hello ${name}`"));
}

#[test]
fn test_array_literals() {
    let code = "var arr = [1, 2, 3];";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("[1, 2, 3]"));
}

#[test]
fn test_object_literals() {
    let code = "var obj = {a: 1, b: 2};";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("{"));
    assert!(result.contains("}"));
}

#[test]
fn test_comments_preserved() {
    let code = "// comment\nfunction test() { /* block */ return 1; }";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("// comment"));
    assert!(result.contains("/* block */"));
}

#[test]
fn test_operators() {
    let code = "var x = 1 + 2 * 3;";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("1 + 2 * 3"));
}

#[test]
fn test_ternary_operator() {
    let code = "var x = true ? 1 : 2;";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("?"));
    assert!(result.contains(":"));
}
