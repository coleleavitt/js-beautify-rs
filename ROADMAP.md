# js-beautify-rs - Feature Roadmap

## ✅ Phase 1: Core Beautification (COMPLETE)

All basic features implemented and tested.

- [x] Token-based parser with 24 token types
- [x] Mode stack (7 modes)
- [x] Auto-indentation
- [x] Operator spacing
- [x] Comment preservation
- [x] Arrow functions
- [x] Template literals
- [x] Ternary operators
- [x] Number/string literals
- [x] CLI tool

---

## ✅ Phase 2: Webpack Optimization (COMPLETE)

Webpack-specific features for bundle readability.

- [x] Webpack import chain breaking (`t(123), t(456)` → multi-line)
- [x] Module separators (visual breaks between modules)
- [x] Large asset extraction (>10KB threshold)
- [x] Pattern detection (number: function pattern)

---

## 🚀 Phase 3: Advanced Deobfuscation (HIGH PRIORITY)

Based on webcrack analysis - make bundles readable.

### 3.1 String Array Deobfuscation
**Priority:** HIGH | **Complexity:** Medium | **Impact:** Very High

Detect and inline obfuscated string arrays (obfuscator.io style).

**Pattern:**
```javascript
// Obfuscated
var _0x1234 = ["hello", "world"];
function _0x5678(a, b) { return _0x1234[a - 0x123]; }
console.log(_0x5678(0x124, 0x125));

// Deobfuscated
console.log("world");
```

**Implementation:**
- [ ] Detect string array declarations (array of strings assigned to variable)
- [ ] Find decoder function (returns indexed string from array)
- [ ] Detect array rotation/shuffling (anti-tampering)
- [ ] Build mapping of decoder calls → actual strings
- [ ] Inline decoded strings throughout code
- [ ] Remove unused decoder and string array

**Files to create:**
- `src/deobfuscate/string_array.rs` - String array detection
- `src/deobfuscate/decoder.rs` - Decoder function analysis
- `src/deobfuscate/inline_strings.rs` - Replace calls with literals

**References:**
- webcrack: `src/deobfuscate/string-array.ts`
- webcrack: `src/deobfuscate/decoder.ts`
- webcrack: `src/deobfuscate/inline-decoded-strings.ts`

---

### 3.2 Control Flow Simplification
**Priority:** HIGH | **Complexity:** High | **Impact:** Very High

Flatten control flow obfuscation (switch statements, flow objects).

**Pattern 1 - Control Flow Switch:**
```javascript
// Obfuscated
var _flow = "3|1|0|2|4".split("|"), _i = 0;
while (true) {
  switch (_flow[_i++]) {
    case "0": console.log("step 3"); continue;
    case "1": console.log("step 2"); continue;
    case "2": console.log("step 4"); continue;
    case "3": console.log("step 1"); continue;
    case "4": console.log("step 5"); break;
  }
  break;
}

// Deobfuscated
console.log("step 1");
console.log("step 2");
console.log("step 3");
console.log("step 4");
console.log("step 5");
```

**Pattern 2 - Control Flow Object:**
```javascript
// Obfuscated
var _ctrl = {
  "a": function() { return x + 1; },
  "b": function() { return x * 2; }
};
result = _ctrl["a"]();

// Deobfuscated
result = x + 1;
```

**Implementation:**
- [ ] Detect control flow switch patterns (sequence string + switch)
- [ ] Reconstruct original statement order from sequence
- [ ] Detect control flow objects (object with function properties)
- [ ] Inline control flow object method calls
- [ ] Remove unused control flow variables

**Files to create:**
- `src/deobfuscate/control_flow_switch.rs`
- `src/deobfuscate/control_flow_object.rs`

**References:**
- webcrack: `src/deobfuscate/control-flow-switch.ts` (80 lines)
- webcrack: `src/deobfuscate/control-flow-object.ts` (sophisticated pattern matching)

---

### 3.3 Dead Code Elimination
**Priority:** MEDIUM | **Complexity:** Medium | **Impact:** Medium

Remove unreachable code and unused variables.

**Patterns:**
```javascript
// Dead code after return
function test() {
  return 42;
  console.log("never runs");  // Remove
}

// Unused variables
var unused = 123;  // Remove if never referenced

// Always-false conditions
if (false) {  // Remove entire block
  console.log("never runs");
}

// Debug protection
while (true) {
  debugger;  // Remove anti-debugging
}
```

**Implementation:**
- [ ] Detect unreachable code after return/throw/break/continue
- [ ] Track variable usage and remove unused declarations
- [ ] Evaluate constant conditions (true/false)
- [ ] Remove debug protection patterns
- [ ] Remove self-defending code

**Files to create:**
- `src/deobfuscate/dead_code.rs`
- `src/deobfuscate/debug_protection.rs`
- `src/deobfuscate/self_defending.rs`

**References:**
- webcrack: `src/deobfuscate/dead-code.ts`
- webcrack: `src/deobfuscate/debug-protection.ts`
- webcrack: `src/deobfuscate/self-defending.ts`

---

## 🎨 Phase 4: Unminification Transforms (MEDIUM PRIORITY)

Make minified code readable again.

### 4.1 Sequence Expression Breaking
**Priority:** HIGH | **Complexity:** Medium | **Impact:** High

Convert comma-separated expressions into separate statements.

**Pattern:**
```javascript
// Minified
x = (a(), b(), c());

// Unminified
a();
b();
x = c();
```

**Implementation:**
- [ ] Detect sequence expressions (comma operator)
- [ ] Break into individual statements
- [ ] Handle return value correctly (last expression)
- [ ] Preserve side effects order

**Files to create:**
- `src/unminify/sequence.rs`

**References:**
- webcrack: `src/unminify/transforms/sequence.ts` (177 lines - sophisticated)

---

### 4.2 Ternary to If Conversion
**Priority:** MEDIUM | **Complexity:** Low | **Impact:** Medium

Convert ternary operators to if-else statements for readability.

**Pattern:**
```javascript
// Minified
result = condition ? value1 : value2;

// Unminified
if (condition) {
  result = value1;
} else {
  result = value2;
}
```

**Implementation:**
- [ ] Detect ternary expressions
- [ ] Convert to if-else when appropriate (not inline)
- [ ] Handle nested ternaries

**Files to create:**
- `src/unminify/ternary_to_if.rs`

**References:**
- webcrack: `src/unminify/transforms/ternary-to-if.ts`

---

### 4.3 Logical Expression to If
**Priority:** MEDIUM | **Complexity:** Low | **Impact:** Medium

Convert short-circuit evaluation to explicit if statements.

**Pattern:**
```javascript
// Minified
condition && doSomething();
condition || setDefault();

// Unminified
if (condition) {
  doSomething();
}
if (!condition) {
  setDefault();
}
```

**Implementation:**
- [ ] Detect logical AND/OR used for control flow
- [ ] Convert to if statements
- [ ] Preserve expression semantics

**Files to create:**
- `src/unminify/logical_to_if.rs`

**References:**
- webcrack: `src/unminify/transforms/logical-to-if.ts`

---

### 4.4 Variable Splitting
**Priority:** LOW | **Complexity:** Low | **Impact:** Low

Split combined variable declarations.

**Pattern:**
```javascript
// Minified
var a = 1, b = 2, c = 3;

// Unminified
var a = 1;
var b = 2;
var c = 3;
```

**Implementation:**
- [ ] Detect multi-variable declarations
- [ ] Split into individual statements
- [ ] Preserve declaration type (var/let/const)

**Files to create:**
- `src/unminify/split_variables.rs`

**References:**
- webcrack: `src/unminify/transforms/split-variable-declarations.ts`

---

### 4.5 Literal Simplification
**Priority:** LOW | **Complexity:** Low | **Impact:** Medium

Simplify obfuscated literals.

**Pattern:**
```javascript
// Obfuscated
var a = !0;        // true
var b = !1;        // false
var c = void 0;    // undefined
var d = 1 / 0;     // Infinity

// Simplified
var a = true;
var b = false;
var c = undefined;
var d = Infinity;
```

**Implementation:**
- [ ] Detect literal patterns (!0, !1, void 0, etc.)
- [ ] Replace with actual literals
- [ ] Handle numeric expressions (1/0, 0/0)

**Files to create:**
- `src/unminify/literal_simplify.rs`

**References:**
- webcrack: `src/unminify/transforms/literal-to-primitive.ts`
- webcrack: `src/unminify/transforms/infinity.ts`

---

### 4.6 Additional Transforms (24 total from webcrack)

Lower priority transforms for comprehensive unminification:

- [ ] **String merging** - Combine adjacent string literals
- [ ] **Computed properties** - `obj["prop"]` → `obj.prop`
- [ ] **Boolean simplification** - `!!x` → `x`
- [ ] **Comparison normalization** - `5 === x` → `x === 5`
- [ ] **For-to-while** - Convert for loops to while
- [ ] **Early return** - Flatten nested if-returns
- [ ] **Conditional assignment** - Simplify conditional patterns
- [ ] **Object spread** - Merge object assignments
- [ ] **Template literals** - Convert string concat to templates
- [ ] **Array destructuring** - Use modern syntax
- [ ] **Optional chaining** - `a && a.b` → `a?.b`
- [ ] **Nullish coalescing** - Use `??` operator

**Files to create:**
- `src/unminify/transform_suite.rs` - Orchestrator
- Individual transform modules as needed

**References:**
- webcrack: `src/unminify/transforms/index.ts` - Lists all 24 transforms

---

## 📦 Phase 5: Bundle Extraction (MEDIUM PRIORITY)

Extract individual modules from bundled code.

### 5.1 Webpack Bundle Unpacking
**Priority:** MEDIUM | **Complexity:** High | **Impact:** High

Extract webpack modules into separate files.

**Features:**
- [ ] Detect webpack runtime (`__webpack_require__`)
- [ ] Extract module map (number → function)
- [ ] Identify entry point module
- [ ] Resolve inter-module dependencies
- [ ] Generate file structure
- [ ] Reconstruct import/export statements
- [ ] Handle both webpack 4 and 5 formats

**Output:**
```
bundle.js → extracted/
  ├── module-12345.js  (entry point)
  ├── module-67890.js
  └── manifest.json    (dependency map)
```

**Files to create:**
- `src/unpack/webpack/mod.rs` - Main unpacker
- `src/unpack/webpack/runtime.rs` - Runtime detection
- `src/unpack/webpack/module.rs` - Module extraction
- `src/unpack/webpack/dependencies.rs` - Dep graph
- `src/unpack/bundle.rs` - Bundle class

**References:**
- webcrack: `src/unpack/webpack/unpack-webpack-5.ts`
- webcrack: `src/unpack/webpack/unpack-webpack-4.ts`
- webcrack: `src/unpack/webpack/common-matchers.ts`
- webcrack: `src/unpack/bundle.ts`
- webcrack: `src/unpack/module.ts`

---

### 5.2 Browserify Bundle Unpacking
**Priority:** LOW | **Complexity:** Medium | **Impact:** Medium

Extract browserify modules.

**Implementation:**
- [ ] Detect browserify runtime
- [ ] Extract module definitions
- [ ] Resolve require() calls
- [ ] Generate file structure

**Files to create:**
- `src/unpack/browserify.rs`

**References:**
- webcrack: `src/unpack/browserify.ts`

---

## 🔌 Phase 6: Plugin System (LOW PRIORITY)

Extensibility for custom transforms.

### 6.1 Plugin Architecture
**Priority:** LOW | **Complexity:** Medium | **Impact:** High (for extensibility)

Allow custom transformations via plugins.

**Features:**
- [ ] Plugin trait with hooks at key stages
- [ ] Plugin registration system
- [ ] Hook points: afterParse, afterDeobfuscate, afterUnminify, afterUnpack
- [ ] Plugin can access and modify token stream
- [ ] Plugin can register custom transforms
- [ ] Plugin configuration via Options

**Example:**
```rust
trait Plugin {
    fn name(&self) -> &str;
    fn after_parse(&self, tokens: &mut Vec<Token>) -> Result<()>;
    fn after_deobfuscate(&self, tokens: &mut Vec<Token>) -> Result<()>;
    fn after_unminify(&self, tokens: &mut Vec<Token>) -> Result<()>;
}
```

**Files to create:**
- `src/plugin/mod.rs` - Plugin system
- `src/plugin/hooks.rs` - Hook definitions

**References:**
- webcrack: `src/plugin.ts` - Plugin system with 5 hooks

---

## 🧬 Phase 7: AST-Based Transforms (FUTURE)

Use proper AST for advanced transforms.

### 7.1 SWC Integration
**Priority:** LOW | **Complexity:** High | **Impact:** Very High

Integrate SWC for full AST-based transformations.

**Why SWC?**
- Fast (Rust-based)
- Full JavaScript/TypeScript parser
- Modern transforms built-in
- Active development

**Features:**
- [ ] Parse with SWC instead of token-based
- [ ] Access full AST for sophisticated transforms
- [ ] Use SWC visitor pattern
- [ ] Enable scope analysis
- [ ] Type-aware transforms

**Tradeoffs:**
- More dependencies
- Heavier (full parser vs lightweight tokens)
- But: Much more powerful transforms possible

**Files to create:**
- `src/ast/mod.rs` - AST integration
- `src/ast/visitor.rs` - AST visitor
- `src/ast/transforms/` - AST-based transforms

**References:**
- SWC: https://swc.rs/
- swc_ecma_parser crate
- swc_ecma_transforms crate

---

## 🎯 Implementation Priority

### Immediate (Next Sprint)
1. **String array deobfuscation** - Highest impact for obfuscated code
2. **Control flow simplification** - Critical for readability
3. **Dead code elimination** - Clean up output

### Short-term (1-2 months)
4. **Sequence expression breaking** - Common in minified code
5. **Ternary/logical to if** - Readability improvement
6. **Webpack bundle unpacking** - Complete webpack support

### Medium-term (3-6 months)
7. **Additional unminify transforms** - Comprehensive unminification
8. **Plugin system** - Extensibility
9. **Browserify support** - Additional bundler

### Long-term (6+ months)
10. **SWC integration** - Full AST capabilities
11. **Advanced transforms** - Type-aware, scope-aware

---

## 📊 Complexity Estimates

| Feature | Lines of Code | Time Estimate | Difficulty |
|---------|---------------|---------------|------------|
| String array deobfuscation | ~300 | 1 week | Medium |
| Control flow simplification | ~400 | 2 weeks | High |
| Dead code elimination | ~200 | 3 days | Medium |
| Sequence breaking | ~150 | 3 days | Medium |
| Ternary to if | ~100 | 2 days | Low |
| Variable splitting | ~80 | 1 day | Low |
| Literal simplification | ~120 | 2 days | Low |
| Webpack unpacking | ~500 | 2 weeks | High |
| Plugin system | ~300 | 1 week | Medium |
| SWC integration | ~800 | 3 weeks | High |

**Total estimated effort for all features:** ~8-10 weeks of focused development

---

## 🧪 Testing Strategy

For each new feature:
- [ ] Unit tests for pattern detection
- [ ] Integration tests with real-world samples
- [ ] Benchmark against webcrack (if applicable)
- [ ] Property-based tests (where appropriate)
- [ ] Corpus of obfuscated samples

---

## 📚 Learning Resources

### Webcrack Source (Primary Reference)
- Architecture: `~/WebstormProjects/forks/webcrack/packages/webcrack/src/`
- Transforms: Focus on `deobfuscate/` and `unminify/` directories
- Pattern matching: `ast-utils/matcher.ts`

### JS Beautify Source
- Token patterns: `~/WebstormProjects/forks/js-beautify/js/src/javascript/`
- Indentation algorithm: `beautifier.js`

### Analysis Documents
- `/tmp/sitemaps/` - Comprehensive webcrack/js-beautify analysis
- Ready-to-use algorithm descriptions

---

## 🚀 Current Status

**Version:** 0.1.0  
**Phase:** 2 of 7 complete  
**Tests:** 22/22 passing  
**Next milestone:** Phase 3 (Deobfuscation)

Ready to implement the most impactful features from webcrack!
