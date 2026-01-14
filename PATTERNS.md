# Obfuscation Patterns Reference

Quick reference for detecting and deobfuscating common patterns.

---

## 🔤 String Array Obfuscation

### Pattern 1: Simple String Array
```javascript
// Obfuscated
var _0x1234 = ["hello", "world", "test"];
function _0x5678(a) { return _0x1234[a]; }
console.log(_0x5678(1));  // "world"

// Detection:
// 1. Array of strings assigned to variable
// 2. Function that indexes into that array
// 3. Calls to that function with numeric literals
```

### Pattern 2: With Offset
```javascript
// Obfuscated
var _0xabcd = ["foo", "bar"];
function _0xdecoder(a, b) { 
  a = a - 0x123;  // Offset subtraction
  return _0xabcd[a]; 
}
console.log(_0xdecoder(0x124));  // "bar"

// Detection:
// - Arithmetic operations on index (subtract constant)
```

### Pattern 3: Array Rotation (Anti-tampering)
```javascript
// Obfuscated
var _0x1111 = ["a", "b", "c"];
(function(_0x2222, _0x3333) {
  var _0x4444 = function(_0x5555) {
    while (--_0x5555) {
      _0x2222.push(_0x2222.shift());  // Rotate array
    }
  };
  _0x4444(++_0x3333);
})(_0x1111, 0x123);

// Detection:
// - IIFE that modifies the string array
// - push/shift or unshift/pop operations
// - Must execute to get final array state
```

### Pattern 4: Multiple Decoders
```javascript
// Obfuscated
var _strings = ["x", "y"];
function _decode1(a) { return _strings[a]; }
function _decode2(a, b) { return _strings[a - b]; }
function _decode3(a) { return atob(_strings[a]); }  // Base64

// Detection:
// - Multiple functions referencing same string array
// - May apply transformations (atob, unescape, etc.)
```

---

## 🔀 Control Flow Obfuscation

### Pattern 1: Switch-Based
```javascript
// Obfuscated
var _flow = "3|1|0|2|4".split("|");
var _i = 0;
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

// Detection:
// - Sequence string with split("|")
// - while(true) + switch statement
// - Cases increment counter
// - Reconstruct: Follow sequence order
```

### Pattern 2: Object-Based
```javascript
// Obfuscated
var _ctrl = {
  "XyZ": function() { return x + 1; },
  "AbC": function() { return x * 2; },
  "DeF": function(a, b) { return a + b; }
};
result = _ctrl["XyZ"]();

// Detection:
// - Object with string keys → functions
// - Property access with string literals
// - Replace with direct function call or inline body
```

### Pattern 3: Nested Control Flow
```javascript
// Obfuscated
while (true) {
  switch (state) {
    case 0:
      if (condition) { state = 2; continue; }
      state = 1; continue;
    case 1:
      doSomething(); state = 3; continue;
    case 2:
      doOtherThing(); state = 3; continue;
    case 3:
      break;
  }
  break;
}

// Detection:
// - State machine pattern
// - Integer state variable
// - State transitions in each case
// - Reconstruct: Build control flow graph
```

---

## 💀 Dead Code Patterns

### Pattern 1: Always-False Conditions
```javascript
// Obfuscated
if (!![]) {  // Always true (empty array is truthy)
  actualCode();
}
if (![]) {   // Always false
  deadCode();  // Never runs
}

// Remove: Evaluate condition, remove dead branch
```

### Pattern 2: Debug Protection
```javascript
// Obfuscated
setInterval(function() {
  debugger;  // Infinite debugger trap
}, 100);

while (true) {
  debugger;
}

// Remove: Detect infinite debugger patterns
```

### Pattern 3: Self-Defending
```javascript
// Obfuscated
var _check = function() {
  return _check.toString().length === expectedLength;
};
if (!_check()) {
  throw new Error("Tampered!");
}

// Remove: Detect function.toString() checks
```

### Pattern 4: Unreachable Code
```javascript
// Obfuscated
function test() {
  return result;
  console.log("never runs");  // Dead
  var x = 42;                 // Dead
}

// Remove: Everything after unconditional return/throw/break/continue
```

---

## 🔄 Minification Patterns

### Pattern 1: Sequence Expressions
```javascript
// Minified
x = (a(), b(), c());

// Deobfuscated
a();
b();
x = c();

// Rule: Last expression is the value, rest are side effects
```

### Pattern 2: Comma in Return
```javascript
// Minified
return a(), b(), c();

// Deobfuscated
a();
b();
return c();
```

### Pattern 3: Ternary Chains
```javascript
// Minified
x = a ? b : c ? d : e ? f : g;

// Deobfuscated
if (a) {
  x = b;
} else if (c) {
  x = d;
} else if (e) {
  x = f;
} else {
  x = g;
}
```

### Pattern 4: Logical Short-Circuit
```javascript
// Minified
condition && doSomething();
condition || setDefault();
x = value || defaultValue;

// Deobfuscated
if (condition) {
  doSomething();
}
if (!condition) {
  setDefault();
}
if (!value) {
  x = defaultValue;
} else {
  x = value;
}
```

### Pattern 5: Boolean Obfuscation
```javascript
// Minified
var a = !0;        // true
var b = !1;        // false
var c = void 0;    // undefined
var d = 1/0;       // Infinity
var e = 0/0;       // NaN

// Simplify: Replace with actual literals
```

---

## 📦 Webpack Patterns

### Pattern 1: Module Definition (Webpack 5)
```javascript
// Webpack 5
var __webpack_modules__ = {
  12345: function(module, exports, __webpack_require__) {
    // Module code
  },
  67890: function(module, exports, __webpack_require__) {
    // Module code
  }
};

// Detection:
// - Object/array named __webpack_modules__
// - Keys are numeric module IDs
// - Values are functions with signature (module, exports, require)
```

### Pattern 2: Webpack Require
```javascript
// Webpack require function
function __webpack_require__(moduleId) {
  if (cache[moduleId]) {
    return cache[moduleId].exports;
  }
  var module = cache[moduleId] = {
    exports: {}
  };
  __webpack_modules__[moduleId](module, module.exports, __webpack_require__);
  return module.exports;
}

// Detection:
// - Function named __webpack_require__
// - Takes moduleId parameter
// - Returns module.exports
```

### Pattern 3: Entry Point
```javascript
// Webpack entry
__webpack_require__.s = 12345;  // Entry module ID
__webpack_require__(__webpack_require__.s);

// Detection:
// - Property assignment __webpack_require__.s
// - Call to __webpack_require__ with .s value
```

### Pattern 4: Import Chain
```javascript
// Common in webpack bundles
var r = t(123), n = t(456), o = t(789), i = t(999);

// Detection:
// - Single-char variable names (t, n, r, e, o, i, a, s, u)
// - Function call with numeric argument
// - Multiple on same line separated by commas
```

---

## 🎯 Detection Priority

**High Priority (Biggest Impact):**
1. String array deobfuscation - Makes obfuscated code readable
2. Control flow simplification - Restores logic structure
3. Dead code elimination - Cleans up noise

**Medium Priority:**
4. Sequence expression breaking - Common in minified code
5. Webpack module extraction - Complete bundle handling
6. Boolean/literal simplification - Readability

**Low Priority:**
7. Ternary to if - Stylistic preference
8. Variable splitting - Minor readability gain

---

## 🧪 Test Cases

For each pattern, maintain corpus in `tests/fixtures/`:

```
tests/fixtures/
├── string-arrays/
│   ├── simple.js           - Basic string array
│   ├── with-offset.js      - Offset subtraction
│   ├── with-rotation.js    - Array rotation
│   └── multiple-decoders.js
├── control-flow/
│   ├── switch-based.js
│   ├── object-based.js
│   └── nested.js
├── dead-code/
│   ├── unreachable.js
│   ├── debug-protection.js
│   └── self-defending.js
└── minified/
    ├── sequences.js
    ├── ternary.js
    └── logical.js
```

Each test:
- Original obfuscated code
- Expected deobfuscated output
- Detection algorithm notes

---

## 📚 Implementation References

- **webcrack source:** `~/WebstormProjects/forks/webcrack/packages/webcrack/src/`
- **Pattern matchers:** `ast-utils/matcher.ts`
- **String arrays:** `deobfuscate/string-array.ts`, `decoder.ts`
- **Control flow:** `deobfuscate/control-flow-switch.ts`, `control-flow-object.ts`
- **Analysis docs:** `/tmp/sitemaps/` (comprehensive algorithm descriptions)

---

## 🎓 Algorithm Templates

### String Array Detection Algorithm
```
1. Find array declaration (var _x = ["...", "..."])
2. Find decoder function(s) that reference that array
3. Detect rotation/shuffling (IIFE that modifies array)
4. Build index → string mapping
5. Find all decoder calls in code
6. Replace calls with actual strings
7. Remove unused decoder and array
```

### Control Flow Switch Algorithm
```
1. Find sequence string (split by "|" or similar)
2. Find while(true) + switch pattern
3. Extract case statements
4. Follow sequence order to reconstruct statements
5. Replace entire pattern with sequential code
```

### Dead Code Algorithm
```
1. Track control flow (returns, breaks, throws)
2. Mark code after unconditional exit as dead
3. Evaluate constant conditions (true/false)
4. Remove dead branches
5. Remove unused variables
```

Ready to implement! 🚀
