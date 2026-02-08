# js-beautify-rs

A fast JavaScript beautifier and deobfuscator written in Rust, powered by [oxc](https://github.com/oxc-project/oxc).

Takes minified, obfuscated webpack bundles and produces readable JavaScript. Handles real-world production bundles — tested against 11MB+ Anthropic CLI builds.

## Usage

```bash
# Beautify
jsbeautify input.js -o output.js

# Beautify + deobfuscate
jsbeautify input.js --deobfuscate -o output.js

# Pipe from stdin
cat bundle.js | jsbeautify - > output.js

# Extract webpack modules to separate files
jsbeautify bundle.js --extract-modules --module-dir ./modules

# Generate dependency graph
jsbeautify bundle.js --extract-modules --dependency-graph deps.dot
```

```
Usage: jsbeautify [OPTIONS] <FILE>

Arguments:
  <FILE>  Input JavaScript file (use "-" for stdin)

Options:
  -o, --output <FILE>            Write output to a file instead of stdout
  -d, --deobfuscate              Enable AST-based deobfuscation (19-phase pipeline)
      --split-chunks             Split webpack chunks into separate files
      --chunk-dir <DIR>          Directory for chunk output [default: ./chunks]
      --chunk-map <FILE>         Write chunk metadata to a JSON file
      --extract-modules          Extract webpack modules to separate files
      --module-dir <DIR>         Directory for module output [default: ./modules]
      --dependency-graph <FILE>  Generate a dependency graph in DOT format
      --source-maps              Generate source maps
      --indent-size <N>          Indentation size in spaces [default: 4]
      --indent-with-tabs         Use tabs for indentation instead of spaces
  -h, --help                     Print help
  -V, --version                  Print version
```

## Deobfuscation Pipeline

The `--deobfuscate` flag runs a 19-phase AST transformation pipeline. Each phase feeds the next — order matters.

| Phase | Pass | What it does |
|-------|------|-------------|
| 1 | Control flow unflattening | Reconstructs original control flow from switch-based state machines |
| 2 | String array rotation | Detects and applies array rotation (shift/push IIFE patterns) |
| 3 | Decoder / string array / dispatcher inlining | Resolves decoder functions (base64, RC4, XOR, offset) and inlines string values |
| 4 | Call proxy inlining | Detects single-use wrapper functions and inlines their targets |
| 5 | Operator proxy inlining | Resolves proxy functions that wrap binary operators |
| 6 | Expression simplification | Bracket-to-dot notation, `!0`→`true`, `void 0`→`undefined`, constant folding, algebraic simplification, strength reduction |
| 7 | Dead code elimination | Removes `if(false)`, `while(false)`, unreachable code after return/throw |
| 8 | Dead variable elimination | Removes variables that are never read |
| 9 | Function inlining | Inlines single-use functions with simple bodies |
| 10 | Array unpacking / dynamic property / ternary / try-catch | Resolves constant array indexing, simplifies constant ternaries, removes empty catch blocks |
| 11 | Unicode / boolean / void / object sparsing | Normalizes unicode identifiers, boolean literals, void expressions, sparse object patterns |
| 12 | Variable renaming | Renames hex-encoded identifiers (`_0x1a2b`) to readable names |
| 13 | Empty statement cleanup | Removes leftover empty statements from prior passes |
| 14 | Sequence expression splitting | Splits comma expressions (`a(), b(), c()`) into individual statements |
| 15 | Multi-variable splitting | Splits `let a=1, b=2, c=3` into individual declarations |
| 16 | Ternary to if/else | Converts standalone ternary expression statements to if/else blocks |
| 17 | Short-circuit to if | Converts standalone `a && b()` / `a \|\| b()` to if statements |
| 18 | IIFE unwrapping | Unwraps zero-argument arrow IIFEs into inline statements |
| 19 | Webpack module annotation | Labels module boundaries with comment separators |

## Building

Requires a local checkout of [oxc](https://github.com/oxc-project/oxc) at `../../forks/oxc` (relative to this repo).

```bash
cargo build --release
```

The binary is at `./target/release/jsbeautify`.

## Running Tests

```bash
# Unit tests (211 tests across all deobfuscation passes)
cargo test --lib
```

## License

MIT
