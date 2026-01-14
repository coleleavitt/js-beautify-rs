# js-beautify-rs

Rust port of [js-beautify](https://github.com/beautifier/js-beautify) with webpack-specific improvements and JavaScript deobfuscation.

## Features

### ✅ Core Beautifier (Complete)
- **Tokenizer** - Complete lexical analysis (24 token types)
- **Beautifier** - Token stream processor with mode stack
- **Auto-indentation** - Proper formatting with configurable indent
- **Template literals** - Full support including nested expressions
- **Arrow functions** - Modern JavaScript syntax
- **All JavaScript constructs** - Functions, objects, arrays, operators, etc.

### ✅ Webpack Bundle Features (Complete)
- **Import chain breaking** - Break up long webpack `var r=t(123),n=t(456)...` chains
- **Module separators** - Visual breaks between webpack modules
- **Large asset extraction** - Extract large inline assets (>10KB) to placeholders

### ✅ JavaScript Deobfuscation (Phase 3 Complete)
- **String array detection** - Finds obfuscated string arrays (`var _0x1234 = ["str1", "str2"]`)
- **Array rotation** - Detects and reverses IIFE-based array shuffling
- **Decoder detection** - Finds functions that decode string indices
- **String inlining** - Replaces decoder calls with actual strings
- **Dead code removal** - Removes unused obfuscation artifacts
- **Offset handling** - Supports decoder functions with arithmetic offsets

## Installation

```bash
# Add to your project
cargo add js-beautify-rs

# Or install CLI tool
cargo install --path .
```

## Usage

### As a Library

```rust
use js_beautify_rs::{beautify, Options};

fn main() {
    let code = "function test(){console.log('hello');}";
    let options = Options::default();
    
    match beautify(code, &options) {
        Ok(beautified) => println!("{}", beautified),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

### With Deobfuscation

```rust
use js_beautify_rs::{beautify, Options};

fn main() {
    let obfuscated = r#"
        var _0x1234 = ["hello", "world"];
        function _0xdec(a) { return _0x1234[a]; }
        console.log(_0xdec(0));
    "#;
    
    let mut options = Options::default();
    options.deobfuscate = true;  // Enable deobfuscation
    
    let result = beautify(obfuscated, &options).unwrap();
    // Output: console.log("hello");
}
```

### Configuration Options

```rust
pub struct Options {
    // Indentation
    pub indent_size: usize,          // Default: 4
    pub indent_char: String,         // Default: " "
    pub indent_with_tabs: bool,      // Default: false
    
    // Newlines
    pub eol: String,                 // Default: "\n"
    pub preserve_newlines: bool,     // Default: true
    pub max_preserve_newlines: usize, // Default: 2
    
    // Formatting
    pub space_after_anon_function: bool,  // Default: false
    pub brace_style: BraceStyle,          // Default: Collapse
    
    // Webpack Features
    pub break_webpack_imports: bool,      // Default: true
    pub add_webpack_module_separators: bool, // Default: true
    pub extract_large_assets: bool,       // Default: true
    pub asset_size_threshold: usize,      // Default: 10_000
    
    // Deobfuscation
    pub deobfuscate: bool,                // Default: false
}
```

### CLI Usage

```bash
# Beautify a file
js-beautify-rs input.js -o output.js

# Beautify with deobfuscation
js-beautify-rs input.js -o output.js --deobfuscate

# Pipe from stdin
cat bundle.js | js-beautify-rs > beautified.js
```

## Deobfuscation Examples

### Simple String Array
```javascript
// Input (obfuscated):
var _0x1234 = ["hello", "world"];
function _0xdec(a) {
    return _0x1234[a];
}
console.log(_0xdec(0));

// Output (deobfuscated):
console.log("hello");
```

### With Array Rotation
```javascript
// Input (obfuscated):
var _0x5a3b = ["Hello", "World", "Test"];
(function(_0x4d8f, _0x3c2a) {
    var _0x1b9e = function(_0x2f7d) {
        while (--_0x2f7d) {
            _0x4d8f.push(_0x4d8f.shift());
        }
    };
    _0x1b9e(2);
})(_0x5a3b, 0x192);
function _0xdec(a) {
    return _0x5a3b[a];
}
console.log(_0xdec(0));

// Output (deobfuscated):
console.log("Test");  // Array was rotated!
```

### With Offset
```javascript
// Input (obfuscated):
var _0x1a2b = ["apple", "banana"];
function _0xdec(a) {
    a = a - 100;
    return _0x1a2b[a];
}
console.log(_0xdec(100));

// Output (deobfuscated):
console.log("apple");
```

## Architecture

### Core Components

- **Tokenizer** (`src/tokenizer.rs`) - Lexical analysis
- **Beautifier** (`src/beautifier/`) - Token processing and formatting
- **Output** (`src/output.rs`) - Formatted output generation
- **Options** (`src/options.rs`) - Configuration
- **Deobfuscator** (`src/deobfuscate/`) - String array deobfuscation

### Deobfuscation Pipeline

1. **String Array Detection** - Find obfuscated arrays
2. **Rotation Detection** - Detect and apply IIFE rotations
3. **Decoder Detection** - Find decoder functions
4. **String Inlining** - Replace calls with actual strings
5. **Dead Code Removal** - Clean up obfuscation artifacts

## Testing

```bash
# Run all tests (41 tests)
cargo test

# Run specific test suite
cargo test --test integration_test
cargo test --test real_samples_test

# Run with output
cargo test -- --nocapture
```

### Test Coverage
- **33 unit tests** - Core functionality
- **4 integration tests** - End-to-end workflows
- **3 real sample tests** - Actual obfuscated code
- **1 doc test** - Documentation examples

## Project Status

### ✅ Completed (Phase 1-3)
- [x] Core tokenizer (24 token types)
- [x] Core beautifier with mode stack
- [x] All JavaScript constructs
- [x] Webpack bundle improvements
- [x] String array deobfuscation
- [x] Array rotation detection
- [x] Decoder function detection
- [x] String inlining
- [x] Dead code removal
- [x] Integration tests
- [x] CLI tool

### 🚧 In Progress (Phase 4+)
- [ ] Control flow flattening deobfuscation
- [ ] More obfuscation patterns
- [ ] Performance benchmarks
- [ ] More real-world testing

## Performance

Rust's performance makes this ideal for processing large webpack bundles:
- Fast tokenization
- Zero-cost abstractions
- Memory-safe transformations

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run CLI
cargo run -- input.js

# Format code
cargo fmt

# Lint
cargo clippy
```

## Contributing

Contributions welcome! Areas of interest:
- Additional obfuscation patterns
- Performance improvements
- More webpack-specific features
- Bug fixes and edge cases

## License

MIT

## Credits

Based on [js-beautify](https://github.com/beautifier/js-beautify) by Liam Newman and contributors.

Deobfuscation patterns inspired by [webcrack](https://github.com/j4k0xb/webcrack).
