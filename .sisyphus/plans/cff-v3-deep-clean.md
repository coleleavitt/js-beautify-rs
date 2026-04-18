# CFF v3 Deep-Clean — Post-v7 Residual Cleanup

## TL;DR

> **Quick Summary**: Based on 3-agent post-v7 analysis (research commit `8459a4f`), ship 5 targeted passes that eliminate the remaining residual obfuscation in BMP output. The critical fix — **`dowhile_switch_cleaner` reachability is too conservative** — alone should prune ~300 cases from Ql (96% dead code) and unlock ~3000-5000 bytes of savings.
>
> **Deliverables**:
> - `array_coerce_fold.rs` — new constant-folding pass for `[] + []` / `[] + undefined` (43 sites)
> - `stack_tracker.rs` — new pass to remove `Ot.push`/`Ot.pop` when pushed values are never consumed (65 sites)
> - `comma_return_simplifier.rs` — new pass for `return X.pop(), A = B, A;` (13 sites)
> - Fix `dowhile_switch_cleaner.rs` reachability — directed graph with conditional-edge branching (expected 300+ case prune)
> - Extend `trampoline.rs` for in-body residual `.apply(this, [S, arguments])` (11 sites)
>
> **Estimated Effort**: Medium (1-2 sessions, ~1200 LOC)
> **Parallel Execution**: YES for Wave 1 (Tasks 1-3 independent). Task 4 depends on graph-analysis refactor. Task 5 is independent.
> **Critical Path**: Task 4 (reachability fix) has the highest single-task impact
>
> **Target**: BMP output < 265KB (from current 272KB), valid JS, 431+ tests passing.

---

## Context — Research Inputs

### Current state (verified 2026-04-18)
- `/tmp/bmp-final.js`: **12,291 lines, 272,557 bytes**, valid JS
- Test suite: **431/431 passing**
- vs raw BMP input (366,497 bytes): **−25.6%**
- Last commit: `8459a4f` (cff-v3-findings research doc)

### Three parallel agents (committed as research in commit 8459a4f)

1. **`bg_252d5bce`** (6m48s) — Pattern hunt on latest output
   - 12 residual patterns with line numbers + exact counts
   - 14 hunt targets confirmed ABSENT
   - Est. ~1,550 bytes savings from Tier 1+2

2. **`bg_760b654d`** (2m12s) — Ql/wj/LT content deep-dive
   - **Ql has 332 cases, 13 reachable (96% dead code)**
   - LT: 42 cases, 15 reachable (64% dead)
   - wj: 11 cases, 4 reachable (63% dead)
   - Ql content: 54% arithmetic/bitwise (hash engine, not bot logic)
   - Mutual recursion: Ql ↔ wj ↔ LT

3. **`bg_c23ae09f`** (1m14s) — 2025-2026 obfuscation SOTA survey
   - 15+ annotated references (NDSS 2026, CCS 2025, arXiv)
   - 5 gaps identified (all OUT OF SCOPE for current BMP target):
     * Invisible Unicode (Hangul), WASM-based, PerimeterX VM, EntropyJS, timing traps

### Verified pattern counts (2026-04-18)

| Pattern                              | Count | Tier | Est. Bytes |
|--------------------------------------|------:|------|-----------:|
| `Ot.push` + `Ot.pop`                     | 30+35 | 1    | ~500       |
| `[] + []` → `""`                         | 29    | 1    | ~150       |
| `Ot[function(){...}()]` noise          | 21    | 1    | ~300       |
| `[] + undefined` → `"undefined"`         | 14    | 1    | ~350       |
| `return Ot.pop(), X=Y, X;`             | 13    | 1    | ~100       |
| `.call(this[MQ], ...)`                 | 12    | 2    | ~80        |
| `.apply(this, [STATE, arguments])`     | 11    | 2    | ~200       |
| Named fn exprs with unused inner name  | 4     | 3    | ~80        |
| `Ot[Ot[JF](xt)]` nested access         | 2     | 2    | ~30        |
| Ternary `X ? X : Y`                    | 1     | 2    | ~15        |
| `X === null \|\| X === undefined`        | 1     | 2    | ~20        |
| `.bind(null, ...)`                     | 1     | 3    | ~20        |

**Estimated Tier 1+2 savings: ~1,845 bytes** + **Task 4 Ql dead-case prune: ~3,000-5,000 bytes** = **~5-7KB total**

### Patterns CONFIRMED ABSENT (no pass needed)
- `obj["literal"]` computed property (0)
- `typeof X === "Y"` (0, handled)
- Empty catch blocks (0)
- `Array.prototype.slice.call` (0)
- `new Array(N)`, `new Object()` (0)
- Optional chain candidates (0)
- Double negation `!!x && x.y` (0)
- Nested ternary operators (0)
- Prototype setup boilerplate (0)
- Chained string concat `"a"+"b"+"c"` (0)
- Unicode-escape identifiers (0)
- Instance-method detection (0)

---

## Work Objectives

### Core Objective
Eliminate the remaining residual obfuscation in BMP output by shipping 5 targeted passes. Reduce output size below 265KB, keep output valid JS, preserve all 431 tests.

### Concrete Deliverables
1. `src/ast_deobfuscate/array_coerce_fold.rs` (~150 LOC + 4 tests) — fold `[] + []` and `[] + undefined`
2. `src/ast_deobfuscate/stack_tracker.rs` (~200 LOC + 5 tests) — remove `Ot.push`/`Ot.pop` when pushed values are unused
3. `src/ast_deobfuscate/comma_return_simplifier.rs` (~150 LOC + 3 tests) — simplify `return X.pop(), A = B, A;`
4. Fix `src/ast_deobfuscate/dowhile_switch_cleaner.rs` (+ ~100 LOC + 3 tests) — proper directed-graph reachability
5. Extend `src/ast_deobfuscate/trampoline.rs` (+ ~80 LOC + 2 tests) — in-body `.apply(this, [S, arguments])` not at top-level
6. Pipeline wiring for new passes

### Definition of Done
- [ ] `Ot.push`/`Ot.pop` count drops to 0 (from 65)
- [ ] `[] + []` count drops to 0 (from 29)
- [ ] `[] + undefined` count drops to 0 (from 14)
- [ ] `return X.pop(), A=B, A;` count drops to 0 (from 13)
- [ ] Ql dispatcher cases reduced from 332 → ≤50 (via reachability fix)
- [ ] BMP output ≤ 265KB (was 272KB)
- [ ] Output valid JS (`node --check`)
- [ ] 431+ tests passing

### Must NOT Have (Guardrails)
- Do NOT remove `Ot.push`/`Ot.pop` without proving the pushed values are never READ. `Ot.length`, `Ot[i]`, `Ot = [...]` are all reads/writes that must be preserved.
- Do NOT prune do-while case bodies that contain `throw`, `return`, or side-effects visible outside (cross-dispatcher calls).
- Do NOT rewrite `.call(this[X], ...)` when `X` is not a resolved constant (dispatch_inliner should have resolved it already).
- Do NOT touch the 5 out-of-scope 2025-2026 threats (Hangul Unicode, WASM, PerimeterX VM, EntropyJS, timing traps) — they're not in the current BMP target.

---

## Execution Strategy

### Wave Structure

```
Wave 1 — Independent constant-folding passes (run in parallel)
├── Task 1: array_coerce_fold.rs [small]
├── Task 2: comma_return_simplifier.rs [small]
└── Task 5: trampoline.rs in-body extension [small]

Wave 2 — Data-flow passes (need semantic analysis)
└── Task 3: stack_tracker.rs [medium, needs read-vs-write analysis]

Wave 3 — Graph algorithm fix
└── Task 4: dowhile_switch_cleaner.rs reachability fix [medium, HIGH IMPACT]

Wave Final — Verification
├── F1: Plan compliance audit (oracle)
├── F2: Code quality review (cargo check/fmt/test/clippy)
└── F3: Real BMP QA + evidence capture
```

---

## TODOs

- [ ] 1. **`array_coerce_fold.rs`** — constant-fold array-to-string coercions

  **What to do**:
  - Create new pass that recognizes:
    - `[] + []` → `""` (empty + empty = empty string)
    - `[] + undefined` → `"undefined"` (array-to-string + undefined-to-string)
    - `[x] + []` → `String(x)` (if x is a known-safe literal)
    - `"" + []` → `""` (string + array = string)
  - Only fire when BOTH operands are literal (no variables, no side-effects)
  - Walk `BinaryExpression` with `Addition` operator in `exit_expression`

  **References**:
  - Pattern template: `src/ast_deobfuscate/fromcharcode_fold.rs`
  - BMP examples (lines 604, 804, 963, 3909, 3911): `var H8 = [] + [];`, `typeof window !== [] + undefined`

  **Acceptance Criteria**:
  - [ ] 4 unit tests (empty+empty, empty+undefined, string+empty, non-literal preserved)
  - [ ] BMP: 29 `[] + []` sites → 0
  - [ ] BMP: 14 `[] + undefined` sites → 0
  - [ ] `cargo test --lib` all pass

  **Agent Profile**: `unspecified-high` + `rust-style`
  **Parallelization**: Wave 1, independent

---

- [ ] 2. **`comma_return_simplifier.rs`** — simplify `return X, A = B, A;`

  **What to do**:
  - Create new pass matching `Statement::ReturnStatement` with `Expression::SequenceExpression` argument
  - Pattern: `return EXPR1, EXPR2, ..., EXPRn;` where EXPRn is a simple identifier
  - If EXPR1..EXPRn-1 are side-effect-free OR consist solely of `X.pop()` / `X.push(Y)` calls on a stack-tracker variable, AND the last expression is `= ASSIGN`, rewrite to:
    - Drop all side-effect-free prefix expressions
    - Hoist any assignments as statements BEFORE the return
    - Return just the final value

  Example:
  ```js
  // Before:
  return Ot.pop(), jC9 = Jc9, jC9;
  // After (if Ot is stack-tracker):
  Ot.pop();
  jC9 = Jc9;
  return jC9;
  // OR (if Ot.pop is already removed):
  jC9 = Jc9;
  return jC9;
  ```

  **References**:
  - Pattern template: `src/ast_deobfuscate/sequence_expression_split.rs` (if it exists — check)
  - BMP examples (lines 3476, 6369, 6408, 6438, 6461, 6530, 6534, 6548, 6624, 6718, 9433, 9515, 10088)

  **Acceptance Criteria**:
  - [ ] 3 unit tests (simple return-comma-assign-return, non-simple preserved, multi-stmt expansion)
  - [ ] BMP: 13 sites → 0
  - [ ] Output still valid JS
  - [ ] All tests pass

  **Agent Profile**: `unspecified-high` + `rust-style`
  **Parallelization**: Wave 1, independent

---

- [ ] 3. **`stack_tracker.rs`** — remove `Ot.push/.pop` when values never consumed

  **What to do**:
  - Scan program for candidate stack-tracker variables. Candidate: `var IDENT = [];` (or `IDENT = []`) where IDENT is only referenced in these ways:
    - `IDENT.push(...)` — write (removable)
    - `IDENT.pop()` — read of unused value (removable)
    - `IDENT.length` — read, but semantically stable if push/pop balanced (load-bearing, cannot remove trivially)
    - `IDENT[idx]` — read, LOAD-BEARING (cannot remove)
    - `IDENT = [...]` — reassignment (write)
  - If `IDENT.length` and `IDENT[idx]` references are PRESENT, do NOT remove push/pop (they affect the observable state).
  - If `IDENT.length` and `IDENT[idx]` are ABSENT, push/pop are pure side-effects on a dead variable — remove them and remove the `var IDENT = []` declaration.
  - For `Ot` in BMP: it IS indexed via `Ot[i]` in ~20 sites, so push/pop are NOT removable without breaking `Ot.length`-based reads. **Report this honestly — the pass may produce 0 removals on BMP.**

  **Alternative (fallback)**: if full removal is unsafe, at least remove the `return Ot.pop(), ...` comma-returns (Task 2) and bare `Ot.pop();` as a statement (where return value isn't used).

  **References**:
  - BMP: 30 `Ot.push()` + 35 `Ot.pop()`, 115 total `Ot` refs, 20+ `Ot[...]` reads

  **Acceptance Criteria**:
  - [ ] 5 unit tests: pure-tracker removal, tracker with `.length` preserved, tracker with `[i]` preserved, nested scope handling, reassignment handling
  - [ ] On BMP: honest output (either N sites removed OR 0 with explanation)
  - [ ] Output valid JS
  - [ ] All tests pass

  **Agent Profile**: `deep` + `rust-style` (needs scope analysis)
  **Parallelization**: Wave 2, independent of Wave 1

---

- [ ] 4. **Fix `dowhile_switch_cleaner.rs` reachability** — proper directed-graph traversal

  **What to do**:
  - Current bug: `Conditional`/`Unknown` transitions treated as "reaches ANY case" (conservative over-approximation). Result: 0 cases pruned on BMP.
  - Fix: represent state machine as directed graph with ordered edge types:
    - `Sequential(next)`: edge `cur → next`
    - `Conditional(branch_a, branch_b)`: edges `cur → branch_a` AND `cur → branch_b` (BOTH, not "any")
    - `Return`, `LoopExit`: terminal, no outgoing edge
    - `Unknown`: conservatively treat as "reaches ALL labels" (preserve safety)
  - BFS/DFS from each entry state, compute set of reachable labels.
  - Cases NOT in reachable set AND NOT `Unknown`-sourced are dead.

  **Algorithm**:
  ```rust
  fn compute_reachable(dispatcher: &DoWhileDispatcherInfo, entries: &HashSet<String>) -> HashSet<String> {
      let mut reachable: HashSet<String> = entries.clone();
      let mut queue: VecDeque<String> = entries.iter().cloned().collect();
      while let Some(label) = queue.pop_front() {
          if let Some(case) = dispatcher.cases.iter().find(|c| c.label == label) {
              let successors: Vec<String> = match &case.transition {
                  StateTransition::Sequential(next) => vec![next.clone()],
                  StateTransition::Conditional(a, b) => vec![a.clone(), b.clone()],
                  StateTransition::Return | StateTransition::LoopExit => vec![],
                  StateTransition::Unknown => dispatcher.cases.iter().map(|c| c.label.clone()).collect(),
              };
              for succ in successors {
                  if reachable.insert(succ.clone()) {
                      queue.push_back(succ);
                  }
              }
          }
      }
      reachable
  }
  ```

  - **IMPORTANT**: The `StateTransition::Conditional` variant in `dowhile_switch_detector.rs` currently doesn't store the two successor labels. That enum must be extended:
    ```rust
    pub enum StateTransition {
        Sequential(String),
        Conditional(String, String),  // ← needs both branches
        Return,
        LoopExit,
        Unknown,
    }
    ```
  - Update the detector to extract both labels from `if (cond) { STATE = A } else { STATE = B }` patterns.

  **References**:
  - `src/ast_deobfuscate/dowhile_switch_detector.rs` — where `StateTransition::Conditional` is defined
  - `src/ast_deobfuscate/dowhile_switch_cleaner.rs` — where reachability is computed
  - Agent `bg_760b654d`: Ql has 332 cases, 13 reachable from external entries → 319 expected prunes

  **Acceptance Criteria**:
  - [ ] `StateTransition::Conditional` stores both successor labels
  - [ ] Detector extracts both labels from if/else STATE assignments
  - [ ] Cleaner uses BFS/DFS with directed edges
  - [ ] 3 unit tests: linear chain prunes dead branch, conditional preserves both, Unknown conservatively preserves all
  - [ ] On BMP: `[DOWHILE] pruned N dead cases` with N > 100 (target: ~300 from Ql)
  - [ ] Output valid JS
  - [ ] All tests pass

  **Agent Profile**: `ultrabrain` + `rust-style` (graph algorithm correctness is critical)
  **Parallelization**: Wave 3, can run after Wave 1 (independent of Task 1-3)

  **Honest risk assessment**:
  - HIGH correctness risk: pruning live cases = broken semantics
  - Mitigation: `Unknown` transitions preserve all cases conservatively
  - Mitigation: run `node --check` after each dispatcher prune, NOT at end

---

- [ ] 5. **Extend `trampoline.rs`** — in-body `.apply(this, [STATE, arguments])`

  **What to do**:
  - Current trampoline pass catches:
    - `var F = function() { return X.apply(this, [S, arguments]); };`
    - `function F() { return X.apply(this, [S, arguments]); }`
    - `F = function() { return X.apply(this, [S, arguments]); };` (assignment)
  - Missed pattern: the 11 residual `.apply(this, [S, arguments])` calls are **NOT wrappers** — they appear inside dispatcher case bodies as the return value of larger expressions, e.g.:
    ```js
    return function(a, b) { ... dispatch_call_via_Lx(X, args) ... Lx.apply(this, [NP, arguments]); };
    ```
  - Since these are tail-calls from a closure, the appropriate rewrite is to the `.call` form: `.apply(this, [S, arguments])` → `.call(this, S, [arguments[0], arguments[1], ...])` but we don't know arity.
  - **Decision**: leave these alone. The agent flagged them but inlining them requires arity analysis and could break semantics. Document the 11 remaining sites as acceptable residue.

  **Alternative**: if we DO want to rewrite, only do it when the enclosing function has a fixed parameter list — then `arguments` can be replaced by `[p1, p2, p3]` directly.

  **References**:
  - `src/ast_deobfuscate/trampoline.rs`
  - BMP lines 2243, 3441, 3508, 6282, 6284, 6287, 6516, 6613, 6641, 9385, 9450

  **Acceptance Criteria**:
  - [ ] Decision made: inline OR leave (document rationale)
  - [ ] If inlined: 2 tests, BMP count drops to near-0
  - [ ] If left: add a comment in trampoline.rs explaining why these are kept
  - [ ] All tests pass

  **Agent Profile**: `unspecified-high` + `rust-style`
  **Parallelization**: Wave 1, independent

---

## Final Verification Wave

- [ ] F1. **Plan Compliance Audit** — `oracle`
  Verify all Must Have items present, all Must NOT Have items absent. Specifically:
  - `Ot`-dependent reads (`Ot.length`, `Ot[i]`) were preserved during Task 3
  - No case bodies with `throw`/`return`/cross-dispatcher calls pruned in Task 4
  - Output valid JS at each phase boundary

- [ ] F2. **Code Quality Review** — `unspecified-high`
  Run `cargo check --lib`, `cargo fmt --check`, `cargo test --lib`, `cargo clippy --lib`. Zero errors. Review new pass files for AI slop.

- [ ] F3. **Real BMP QA + Evidence Capture** — `unspecified-high`
  Run pipeline end-to-end. Expected:
  - Output < 265KB (target)
  - Valid JS (`node --check`)
  - All pipeline phases log expected counts (e.g., `Phase 9.5: Inlined 242 dispatch sites` still present)
  - Save evidence: `.sisyphus/evidence/cff-v3-final.js`, `.sisyphus/evidence/cff-v3-pipeline.log`, `.sisyphus/evidence/cff-v3-metrics.md`

---

## Commit Strategy

One commit per task. Conventional commits:
- `feat(deobfuscate): add array_coerce_fold for [] + [] → ""`
- `feat(deobfuscate): add comma_return_simplifier`
- `feat(deobfuscate): add stack_tracker for pure Ot.push/pop removal`
- `fix(deobfuscate): dowhile_switch_cleaner uses proper reachability graph`
- `feat(deobfuscate): extend trampoline for in-body .apply` (or doc commit if decided to leave)

---

## Explicitly Out-of-Scope (documented for awareness)

From agent `bg_c23ae09f` SOTA survey — all 5 are NOT in the current BMP target:

1. **Invisible Unicode obfuscation** (Hangul U+FFA0/U+3164) — 269K infected pages Q1 2025
2. **WASM-based obfuscation** (Wobfuscator/WASMixer/emcc-obf)
3. **PerimeterX Auditor VM** — bytecode + RSA encryption (April 2026)
4. **Akamai BMP v2 timing traps + canvas fingerprinting** — present in current BMP but we don't simulate runtime
5. **EntropyJS LLM-resistant encryption** — proprietary, server-keyed

Addressing these requires runtime execution, WASM decompilation, or cryptographic primitives — all significantly larger effort than this plan justifies for current corpus.

---

## Research Sources

- `bg_252d5bce` (6m48s) — Pattern hunt on `/tmp/bmp-final.js` produced the 12-pattern table + absent list
- `bg_760b654d` (2m12s) — Ql/wj/LT content analysis + reachability numbers
- `bg_c23ae09f` (1m14s) — SOTA obfuscation survey 2025-2026 (used for out-of-scope section only)
- Direct verification run (2026-04-18) confirmed current counts on `/tmp/bmp-final.js`

---

## Success Criteria

### Verification Commands
```bash
cargo test --lib                                    # 431+ pass
cargo build --release
./target/release/jsbeautify \
  /home/cole/VulnerabilityResearch/akami/deobfuscated/sws-gateway_botmanager.js \
  --deobfuscate -o /tmp/bmp-v3.js
node --check /tmp/bmp-v3.js                         # valid JS
stat -c%s /tmp/bmp-v3.js                            # expect < 265,000
grep -cE '\[\] \+ \[\]' /tmp/bmp-v3.js                # 0
grep -cE '\[\] \+ undefined' /tmp/bmp-v3.js           # 0
grep -cE 'return Ot\.pop\(\),' /tmp/bmp-v3.js         # 0
./target/release/jsbeautify ... 2>&1 | grep 'Phase 8.8: Pruned'  # N > 100
```

### Final Checklist
- [ ] All 5 Tasks shipped (or Task 5 explicitly documented as deferred with rationale)
- [ ] BMP output ≤ 265KB
- [ ] Ql case count ≤ 50 (was 332)
- [ ] Output valid JS
- [ ] 431+ tests passing
- [ ] 0 new clippy errors
- [ ] Evidence captured in `.sisyphus/evidence/cff-v3-*`
