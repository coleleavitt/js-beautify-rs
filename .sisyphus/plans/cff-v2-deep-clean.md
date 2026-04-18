# CFF v2 Deep-Clean Plan — Post-v7 Follow-up

## TL;DR

> **Quick Summary**: Based on exhaustive post-v7 analysis (3 parallel agents), the next high-impact session targets 3 specific wins: (1) dispatch-function inlining (108 sites, LARGE), (2) `Xt(value, undefined)` wrapper normalization (55 sites, SMALL), (3) in-body trampoline wrapper extension (40 sites, SMALL). Explicitly NOT recommended: Ql/wj/LT unflattening (7 sites, not worth 500 LOC) and IIFE unwrapping (0.32% savings, not worth it).
>
> **Deliverables**:
> - `dispatch_inliner.rs` (new, LARGE) — reverse-engineer x19/Y49/Fw/D8/hx cache tables, inline computed member access
> - `xt_unwrap.rs` (new, SMALL) — normalize `Xt(v, undefined)` and related wrapper calls
> - Extend `trampoline.rs` to handle in-body wrapper variants (40 sites)
> - (Optional) Modify `cff_unflattener.rs` to skip IIFE wrapping when case body has no `var` declarations
>
> **Estimated Effort**: Large (2-3 sessions)
> **Parallel Execution**: YES — dispatch_inliner, xt_unwrap, trampoline-ext are independent
> **Critical Path**: dispatch_inliner IS the critical path (most complex, highest impact)

---

## Context — Findings from Post-v7 Investigation

### Investigation 1: Do-while-switch dispatchers (Ql/wj/LT)

**Pattern**:
```js
var Ql = function SY(JK, DD) {
    do {
        switch (JK) {
            case Q4: { ...; JK = NEXT_STATE; } break;
            case xR: { ...; JK = MR; } break;
            // ... 201 cases (for Ql)
        }
    } while (JK != p9);
};
```

**Why the current CFF detector rejects them**: `dispatcher_detector.rs` line 371-389 only accepts bodies containing `SwitchStatement`, `VariableDeclaration`, `ExpressionStatement`. A `DoWhileStatement` wrapping the switch is rejected — by design, to avoid false positives.

**Why these are fundamentally harder**:
1. **State reassignment inside cases** means dispatch is dynamic — can't inline at literal call sites
2. **3-way mutually-recursive cycle** (Ql↔wj↔LT, 8+ cross-calls)
3. **Only 7 total call sites** across 3000+ lines → effort-to-impact is terrible

**Decision: DEFER indefinitely.** Document the limitation in `dispatcher_detector.rs` with a comment block.

---

### Investigation 2: Residual obfuscation patterns

#### Top-5 from agent analysis (ranked by impact × feasibility):

| # | Pattern | Count | Complexity | Home | Priority |
|---|---------|-------|------------|------|----------|
| 1 | **Dispatch-function caching**: `Fw()[x19()[Fm]](bZ, pI, NL9)` | **108** | LARGE (~500 LOC) | New `dispatch_inliner.rs` | HIGH — biggest readability win |
| 2 | **`Xt(value, undefined)` wrapper**: `Xt([], undefined)`, `Xt("", undefined)` | **55** | SMALL (~150 LOC) | New `xt_unwrap.rs` | HIGH — easy win |
| 3 | **In-body trampoline wrappers**: `function() { return SY.apply(this, [Lg, arguments]); }` INSIDE a function body | **40** | SMALL (~200 LOC) | Extend `trampoline.rs` | HIGH — our current pass only catches TOP-LEVEL wrappers |
| 4 | **`Ot` push/pop state tracking** | 112 (56 pairs) | MEDIUM (~200 LOC) | New `state_tracker_remover.rs` | MEDIUM — only helpful if Ot is never read |
| 5 | **IIFE-wrapped case bodies** (our CFF output artifact) | 57 | N/A | Fix upstream | LOW — 0.32% savings not worth 50 LOC |

#### Patterns confirmed ABSENT (honest reporting):
- Array-literal jump tables (`var JT = [fn1,...]; JT[i]()`) — 0 instances
- Object computed-key lookups (`{"a":X}["a"]`) — 0 instances
- `.constructor.constructor("code")()` prototype gymnastics — 0 instances
- Empty catch blocks — 0 instances
- Unicode escape identifiers (`\u0061`) — 0 instances
- String-number coercion tricks (`+"42"`, `~~x`) — 0 instances
- `.bind(this)` on arrow functions — 0 instances

---

### Investigation 3: IIFE unwrapping feasibility

**Verdict**: Do NOT implement. Findings:
- 57 CFF-produced IIFEs total
- **48 use `this`** (can't unwrap — changes binding)
- **2 have break/continue** (can't unwrap — changes control flow)
- **Only 9 are safe** — would save ~900 bytes (0.32%)
- Output still 12.5% above pre-CFF baseline even after all safe unwraps

**Better path**: Modify `cff_unflattener.rs` upstream to skip IIFE wrapping when:
- Case body has zero `var`/`let`/`const` declarations
- Parent is statement position
- Body doesn't use the dispatcher's `args` param

This is a smaller, less-risky change than a separate unwrapper pass.

---

## Work Objectives

### Core Objective
Shrink the v7 output below the 241KB pre-CFF baseline AND further improve readability by eliminating the 3 highest-impact residual obfuscation classes.

### Concrete Deliverables
1. `src/ast_deobfuscate/xt_unwrap.rs` (new, ~150 LOC + 4 tests)
2. Extend `src/ast_deobfuscate/trampoline.rs` to detect in-body wrappers (+ ~100 LOC + 3 tests)
3. `src/ast_deobfuscate/dispatch_inliner.rs` (new, ~500 LOC + 10 tests) — BIG task, may span 2 sessions
4. Pipeline wiring for all new passes
5. (Optional) Modify `cff_unflattener.rs` to skip IIFE when safe

### Definition of Done
- [ ] Xt wrapper pass: 55 sites normalized
- [ ] Trampoline extension: 40 in-body wrappers inlined
- [ ] Dispatch inliner: at least 80% of 108 sites resolved (some may require runtime state)
- [ ] Output valid JS
- [ ] Output below 241KB (pre-CFF baseline)
- [ ] 404+ library tests still pass

### Must NOT Have
- Do NOT attempt Ql/wj/LT unflattening (deferred indefinitely)
- Do NOT implement IIFE unwrapping pass (not worth it)
- Do NOT break the existing dispatcher_detector for the 9 working dispatchers

---

## TODOs

- [ ] 1. **`xt_unwrap.rs`** — normalize the Xt wrapper idiom (easy win)

  **What to do**:
  - First, FIND the definition of `Xt` in the BMP output. Grep for `function Xt` and `var Xt = function`
  - Determine what `Xt(value, undefined)` actually computes. Likely either:
    - Type coercion wrapper: `function Xt(a, b) { return typeof a === typeof b ? a : b }` — then `Xt([], undefined) === []`
    - Equality check: `function Xt(a, b) { return a === b }` — then `Xt([], undefined)` is a boolean
    - Identity: `function Xt(a) { return a }` — then `Xt([], undefined) === []`
  - Based on semantics, propose the rewrite
  - Add a new pass that replaces `Xt(A, B)` calls at literal-arg sites with the computed result

  **References**:
  - BMP examples: line 601 `var H8 = Xt([], []);`, line 6223 `typeof X === Xt([], undefined) ? ... : ...`, line 6471 `Xt("", undefined) ? ... : ...`
  - Pattern pass template: `src/ast_deobfuscate/fromcharcode_fold.rs` (simple const folder)

  **Acceptance Criteria**:
  - [ ] 4 tests covering the 3 call shapes (identity, type-check, comparison)
  - [ ] Run against BMP: 55 sites rewritten
  - [ ] `cargo test --lib` all pass

  **Agent**: `unspecified-high` + `rust-style`
  **Parallelization**: Wave 1, independent

---

- [ ] 2. **Extend `trampoline.rs` for in-body wrappers**

  **What to do**:
  - The current trampoline pass detects top-level `var F = function() { return D.apply(this, [S, arguments]); }` + bare `function F() { ... }`
  - BMP has 40 MORE such wrappers as **FunctionExpression assigned to object properties or nested variables**:
    ```js
    { method: function() { return SY.apply(this, [Lg, arguments]); } }
    var obj = { m: function() { return SY.apply(this, [JC, arguments]); } };
    this.x = function() { return SY.apply(this, [X6, arguments]); };
    ```
  - Extend the detector to walk into:
    - `ObjectExpression.properties[].value` (for method-shorthand & key-value)
    - `AssignmentExpression.right` (for `this.x = function() { ... }`)
  - The inlining logic stays the same — just broaden the detection

  **References**:
  - `src/ast_deobfuscate/trampoline.rs:87-130` (current `extract_info` + `TrampolineCollector`)
  - BMP examples: lines 1388, 1738, 2203 (all `return SY.apply(this, [CONST, arguments])`)

  **Acceptance Criteria**:
  - [ ] 3 new tests: object-property wrapper, assignment wrapper, nested-var wrapper
  - [ ] Run against BMP: expect 40 additional inlines beyond current count
  - [ ] All existing trampoline tests still pass

  **Agent**: `unspecified-high` + `rust-style`
  **Parallelization**: Wave 1, independent

---

- [ ] 3. **`dispatch_inliner.rs`** — reverse-engineer cache dispatch (BIG)

  **What to do**:
  - BMP has 7+ "cache dispatch" functions: `x19`, `Y49`, `Fw`, `D8`, `hx`, `CG`, `kI`, `k8`, `Tl`, `mE`
  - Usage pattern: `Fw()[x19()[CONST]](arg1, arg2)` — `Fw()` returns a cached object, `x19()[CONST]` returns a method name string, and the whole thing is a dynamic method call
  - Task: **build a reverse-map** from (cache-function, index-constant) → method-name
    1. Find each cache function definition (e.g., `function x19() { return [...array of strings...]; }`)
    2. Find each index constant definition (e.g., `var Fm = 5;`)
    3. Build map: `x19()[Fm] === x19_array[5] === "someMethod"`
    4. Rewrite `Fw()[x19()[Fm]](...)` → `Fw().someMethod(...)` → possibly further simplify if `Fw()` is itself resolvable
  - This is the single biggest readability win. 108 sites.

  **References**:
  - Similar pattern: `src/ast_deobfuscate/lookup_forwarder.rs` — 1-arg forwarder `function bI(x){return x19()[x]}` — which we ALREADY inlined. But `Fw()[x19()[Fm]](args)` is more complex.
  - BMP examples: lines 3446, 3445, 3428

  **Acceptance Criteria**:
  - [ ] Detector identifies all 7+ cache-dispatch functions
  - [ ] Builds reverse-map correctly (verify on 3 known cache+index pairs)
  - [ ] Inlines at least 80% of 108 call sites
  - [ ] 10+ tests covering edge cases (non-literal index, non-constant cache, nested calls)
  - [ ] Output valid JS

  **Agent**: `ultrabrain` + `rust-style`
  **Parallelization**: Wave 2, depends on completing Wave 1 (so tests pass cleanly)
  **Note**: This may be a 2-session task. Consider splitting into:
  - 3a. Cache-function detection + array extraction
  - 3b. Index-constant resolution + reverse-map building
  - 3c. Call-site rewriter

---

- [ ] 4. (Optional) **Modify `cff_unflattener.rs` to skip IIFE wrapping when safe**

  **What to do**:
  - When inlining a case body, currently ALWAYS wraps in `(function(args-param) { body })([call-site-args])`
  - If the case body has **no `var`/`let`/`const` declarations** AND doesn't reference the args-param, skip the wrapper entirely
  - For cases that reference `args-param`, still wrap (or alternative: prepend a single `var args-param = [call-site-args];` and splice body)

  **References**:
  - `src/ast_deobfuscate/cff_unflattener.rs:try_inline` (the rewriter)

  **Acceptance Criteria**:
  - [ ] Detect safe-to-unwrap case bodies (pure, no var decls, no closures using args)
  - [ ] Produce non-IIFE output for safe cases
  - [ ] Output still valid JS
  - [ ] Size DROP below 241KB pre-CFF baseline

  **Agent**: `deep`
  **Parallelization**: Wave 2, can run after Task 3

---

## Final Verification Wave (after all tasks)

- [ ] F1. **Plan Compliance Audit** — `oracle`
- [ ] F2. **Code Quality Review** — `unspecified-high` (cargo check/fmt/test/clippy)
- [ ] F3. **Real BMP QA + Evidence Capture** — `unspecified-high`
  - Expect: output ≤ 200KB (target), valid JS, 404+ tests pass
  - Save evidence to `.sisyphus/evidence/cff-v2-*.md`

---

## Commit Strategy

One commit per task. Conventional commits style:
- `feat(deobfuscate): add xt_unwrap for Xt(v, undefined) normalization`
- `feat(deobfuscate): extend trampoline for in-body wrappers`
- `feat(deobfuscate): add dispatch_inliner for cache-function rewriting`
- `feat(deobfuscate): skip CFF IIFE wrapping when case body is pure`

---

## Explicitly Deferred (with rationale)

1. **Ql/wj/LT do-while-switch unflattening** — Only 7 call sites. 500 LOC + state-machine simulation not worth it. Document with a comment in `dispatcher_detector.rs`.

2. **IIFE unwrapping pass** — 0.32% savings. Upstream fix to `cff_unflattener.rs` is better (Task 4).

3. **`Ot` push/pop state tracker removal** — 112 instances but first need to verify `Ot` is never READ elsewhere in the code. If it IS read (even for debugging/integrity), removal breaks semantics. Medium-risk; medium-impact. Tackle after the 3 top wins.

4. **Multi-level property access flattening** (`this[X][Y]`) — Only 4 instances. Not worth a pass.

---

## Research Sources (agent outputs from this planning session)

- `bg_67de11bf` (2m33s) — Do-while-switch dispatcher analysis
- `bg_dfd4afc6` (2m36s) — Top-20 residual pattern hunt
- `bg_33c6889a` (3m05s) — IIFE unwrap feasibility analysis

All three agents ran against `/tmp/bmp-v7.js` (the current state of deobfuscation after the cff-unflattener plan completed).
