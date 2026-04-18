# Post-Session Residual Hunt — 3 Parallel Agent Findings (Post-v7)

## TL;DR

> Current state: BMP 272,557 bytes, 12,291 lines, 431 tests, valid JS, −25.6% vs raw input.
>
> Three parallel agents (6m48s + 2m12s + 1m14s) ran on `/tmp/bmp-final.js`. Findings ranked by impact:
>
> **Tier 1 (IMMEDIATE, shipping value)**
> 1. `Ot.push/.pop` stack tracker (65 current, 112 total) — never read for its values. Removing eliminates ~500 bytes + massive readability.
> 2. `[] + []` → `""` (29 instances) — trivial constant fold.
> 3. `[] + undefined` → `"undefined"` (2 instances) — trivial.
> 4. `return Ot.pop(), X = Y, X;` (13 instances) — comma-sequence simplification.
> 5. `Ot[function(){...}()]` (21 instances) — IIFE-as-index evaluates to undefined.
>
> **Tier 2 (MEDIUM, meaningful)**
> 6. 11 remaining `.apply(this, [STATE, arguments])` — not caught by current trampoline pass because they appear inside cross-dispatcher call sites, not as wrapper functions.
> 7. `X ? X : Y` → `X || Y` (1 instance).
> 8. `X === null || X === undefined` → `X == null` (1).
>
> **Tier 3 (STRUCTURAL)**
> 9. Ql dispatcher has 332 cases but only 13 reachable from external call sites — **96% dead code**.
> 10. LT dispatcher has 42 cases but only 15 reachable — **64% dead code**.
> 11. wj has 11 cases, 4 reachable — 63% dead code.
>
> **NEW THREATS (from 2025-2026 obfuscation survey)** — out of scope for BMP but worth knowing:
> - Invisible Unicode (Hangul U+FFA0): 269K infected pages Q1 2025 (not in BMP)
> - WASM-based obfuscation (Wobfuscator/WASMixer/emcc-obf): growing (not in BMP)
> - PerimeterX Auditor VM: bytecode + RSA encryption (April 2026, different product)
> - Akamai BMP v2 timing traps + canvas fingerprinting: ALREADY in BMP but we don't simulate them
> - EntropyJS LLM-resistant encryption: proprietary (not in BMP)

## Patterns CONFIRMED PRESENT in /tmp/bmp-final.js

| Pattern | Count | Tier | Est. Bytes Saved |
|---------|-------|------|-------------------|
| `Ot.push/.pop` | 65 | 1 | ~500 |
| `[] + []` → `""` | 29 | 1 | ~150 |
| `Ot[function(){...}()]` noise | 21 | 1 | ~300 |
| `.call(this[MQ], ...)` | 17 | 2 | ~100 |
| `return Ot.pop(), X=Y, X;` | 13 | 1 | ~100 |
| `.apply(this, [S, arguments])` residual | 11 | 2 | ~200 |
| Named fn exprs with unused inner name | 4 | 3 | ~80 |
| `[] + undefined` → `"undefined"` | 2 | 1 | ~50 |
| `Ot[Ot[JF](xt)]` nested access | 2 | 2 | ~30 |
| `X === null \|\| X === undefined` | 1 | 2 | ~20 |
| `X ? X : Y` | 1 | 2 | ~15 |
| `.bind(null, ...)` | 1 | 3 | ~20 |

Estimated total savings from all Tier-1+2 passes: **~1,550 bytes** (→ 270,977 bytes final)

## Patterns CONFIRMED ABSENT (no further work needed)

- `obj["literal"]` computed property (0)
- `typeof X === "Y"` (0 — already handled)
- Empty catch blocks (0)
- `Array.prototype.slice.call` (0)
- `new Array(N)` (0)
- Optional chain candidates (0)
- Double negation `!!x && x.y` (0)
- Nested ternary operators (0)
- `new Object()` / `new Array()` (0)
- Prototype setup boilerplate (0)
- Chained string concat `"a"+"b"+"c"` (0)
- Self-executing IIFE → var (0)
- Unicode-escape identifiers (0)
- Instance-method detection (0)

## Ql/wj/LT Deep Content Analysis

Agent 2 revealed the dispatchers are **NOT bot-detection logic** — they're a **generic arithmetic/hash engine**:
- Ql: 54% arithmetic/bitwise (likely hash accumulator), 18% function construction, 12% returns
- wj: 36% function construction (meta-programming layer)
- LT: 19% returns, 19% array/object manipulation (likely parser/tree-walker)

The actual BMP logic (sensor collection, cookie handling, fingerprinting) is **NOT inside these dispatchers**. It's elsewhere in the file, likely calling these three as helper routines.

**Cross-dispatcher graph**:
```
Ql ↔ wj ↔ LT  (mutually recursive)
Ql → wj (lines 436)
Ql → LT (line 739)
wj → Ql (line 7664)
LT → Ql (line 7904, 7948, 7964)
LT → wj (via call in cases)
```

## Recommendation: Hybrid Strategy (not full linearization)

Agent 2's verdict: full linearization of all 332 Ql cases is **wasteful** because 319 are unreachable. Better approach:

1. **Tier 1 — Dead-case pruning** (already have infrastructure via `dowhile_switch_cleaner`)
   - Ql: 332 → ~13 reachable cases (−96%)
   - LT: 42 → ~15 reachable cases (−64%)
   - wj: 11 → ~4 reachable cases (−63%)
   - Estimated bytes saved: **~3,000–5,000 bytes** (the big win)

2. **Tier 2 — Selective inlining** for Ql only
   - 181 single-successor cases could be chain-inlined
   - Skip wj/LT (complex control flow)

3. **Tier 3 — Annotation**
   - Comment each surviving case with its computation category (arithmetic/function/return/etc.)

## Why dead-case pruning returned 0 last session

The current `dowhile_switch_cleaner` uses conservative reachability — ANY `Conditional`/`Unknown` transition is treated as "can reach any case". BMP dispatchers have complex conditional transitions so 0 cases get pruned.

**Fix**: treat each state transition edge (including conditional) as a directed edge in the graph. A case is only dead if NO path from an entry state reaches it. Conditional edges go to BOTH targets, not "any case". This would actually prune ~300 cases out of Ql.

## Next Session Plan

Priority 1 (quick wins, <1 hour):
- Extend `constant_folding` or `algebraic_simplify` to handle `[] + []` → `""` (29 sites) and `[] + undefined` → `"undefined"` (2 sites)
- Add `Ot` stack-tracker removal pass (65 sites, requires read-vs-write analysis — most Ot refs are `Ot.length`/`Ot[i]` which ARE reads; need to prove the pushed values are never consumed)

Priority 2 (medium, 2-4 hours):
- Fix `dowhile_switch_cleaner` to use proper graph reachability (conditional edges → both targets)
- Re-run against BMP: expect 300+ cases pruned from Ql alone

Priority 3 (large, multi-session):
- T5b acyclic linearizer for Ql's 181 single-successor cases
- T5d entry-point rewriter

## Session Artifacts

Agent outputs stored in session memory:
- `bg_252d5bce` — Pattern hunt (6m48s, 15-item ranked list)
- `bg_760b654d` — Dispatcher deep-dive (2m12s, case categorization + linearization readiness scores)
- `bg_c23ae09f` — 2025-2026 obfuscation survey (1m14s, 15+ annotated references)
