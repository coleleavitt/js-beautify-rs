# js-beautify-rs

Rust port of [js-beautify](https://github.com/beautifier/js-beautify) with webpack-specific improvements.

## Goals

1. **Port core JavaScript beautifier to Rust**
2. **Add webpack bundle handling improvements**
   - Break up long import chains
   - Add module boundary separators
   - Handle inline assets (SVGs, etc.)
3. **Performance** - Leverage Rust's speed for large bundles
4. **Safety** - Type-safe AST manipulation

## Architecture (from js-beautify analysis)

### Core Components:

- **Tokenizer** - Lexical analysis (TOKEN types)
- **Beautifier** - Token stream processor with state machine
- **Output** - Formatted output generator
- **Options** - Configuration

### Token Types:
```rust
enum TokenType {
    StartExpr,    // (
    EndExpr,      // )
    StartBlock,   // {
    EndBlock,     // }
    Word,         // identifiers
    Reserved,     // keywords
    Semicolon,    // ;
    String,       // "..." or '...'
    Equals,       // =
    Operator,     // +, -, *, /
    Comma,        // ,
    BlockComment, // /* */
    Comment,      // //
    Dot,          // .
    Unknown,
    Eof,
}
```

### Mode Stack (Formatting Contexts):
```rust
enum Mode {
    BlockStatement,
    Statement,
    ObjectLiteral,
    ArrayLiteral,
    ForInitializer,
    Conditional,
    Expression,
}
```

## Implementation Plan

### Phase 1: Core Tokenizer
- [ ] Implement Token struct
- [ ] Implement Tokenizer with regex patterns
- [ ] Handle all TOKEN types
- [ ] Write tests for tokenization

### Phase 2: Beautifier State Machine
- [ ] Implement Mode enum and state tracking
- [ ] Implement Output generator
- [ ] Port beautification rules
- [ ] Handle indentation logic

### Phase 3: Webpack Improvements
- [ ] Detect webpack module boundaries
- [ ] Break up long import chains
- [ ] Add module separators
- [ ] Handle inline assets (SVG extraction)

### Phase 4: CLI & Testing
- [ ] CLI tool
- [ ] Test suite with real bundles
- [ ] Benchmarks vs js-beautify

## Usage (planned)

```bash
# As library
cargo add js-beautify-rs

# As CLI
cargo install --path .
js-beautify-rs input.js -o output.js
```

## Development

```bash
cargo build
cargo test
cargo run -- input.js
```

