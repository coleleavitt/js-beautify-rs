use super::*;

#[test]
fn test_variable_declarations() {
    let code = "var r=t(123),n=t(456),o=t(789);";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    assert!(result.contains("t(123)"));
    assert!(result.contains("t(456)"));
    assert!(result.contains("t(789)"));
}

#[test]
fn test_nested_blocks_indentation() {
    let code = "function test() { if (x) { return 42; } }";
    let options = Options::default();
    let result = beautify(code, &options).unwrap();

    let lines: Vec<&str> = result.lines().collect();
    assert!(lines.len() > 1, "Should have multiple lines");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with('\t') || l.starts_with("  ")),
        "Should have indentation"
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

    assert!(result.contains("1"));
    assert!(result.contains("2"));
    assert!(result.contains("3"));
    assert!(result.contains("["));
    assert!(result.contains("]"));
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
