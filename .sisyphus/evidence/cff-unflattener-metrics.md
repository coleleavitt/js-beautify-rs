# CFF Unflattener v7 — Pipeline Metrics

| Metric | Value |
|--------|-------|
| Input file (deobf'd reference) | 366,497 bytes, 15,156 lines |
| Post-pipeline v7 | 274,557 bytes, 12,205 lines |
| Delta vs input | −25.1% bytes, −19.5% lines |
| v6 (pre-CFF) baseline | 241,829 bytes, 11,044 lines |
| Delta v7 vs v6 | +32,728 bytes (+13.5%), +1,161 lines (+10.5%) |
| CFF dispatchers detected | 9 |
| CFF call sites inlined | 113 |
| Trampolines detected | 62 |
| Trampoline call sites inlined | 88 |
| `.call(this, ...)` simplified | 105 |
| `.apply/.call` simplified | 665 |
| Unary proxies inlined | 1,403 |
| Lookup forwarders inlined | 1,958 |
| Equality proxies unwrapped | 505 |
| Boolean-arithmetic folded | 66 |
| `[][[]]` → `undefined` | 188 |
| Self-init accessors flattened | 9 |
| Empty statements removed | 725 |
| Tests passing | 404/404 |
| Valid JS | ✓ yes |

## Size Explanation

v7 is larger than v6 because CFF inlining **expands** dispatcher switch-case bodies
inline at each call site (IIFE-wrapped). This is the expected trade-off: the code is
larger but each call site now shows the actual logic instead of an opaque `P6.call(this, NA, [...])`.

## Residual Patterns

| Pattern | v6 count | v7 count | Notes |
|---------|----------|----------|-------|
| `.call(this, ...)` | 54 | 0 | Fully eliminated |
| `.apply(this, ...)` | 38 | 40 | Mostly dispatcher internals (recursive) |
| `P6(` calls | 11 | 58 | Increased — CFF inlined case bodies call P6 recursively |
| `(function(` IIFEs | — | 64 | New — IIFE wrappers from CFF inlining |
