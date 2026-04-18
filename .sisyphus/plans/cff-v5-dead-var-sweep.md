# CFF v5 — Post-Pruning Dead-Var Sweep + Q6/Tl Resolution

## TL;DR

> **Quick Summary**: After cff-v4 pruned 157 dead cases from Ql/wj/LT, **1,510 `var X;` forward declarations became orphaned**. The existing `dead_var_elimination` pass runs at Phase 8, BEFORE CFF pruning at Phase 9.7 — so it never sees these orphans. Moving the pass (or adding a second invocation) to Phase 10 reclaims **~22.6 KB (9.8%)** with near-zero risk. Plus: fix 25 residual `Tl()[wY]` computed accesses our dispatch_inliner missed, and normalize the `Q6.X` window-alias chain.
>
> **Deliverables**:
> - Re-run `dead_var_elimination` after CFF pruning (Phase 10.5, reuse existing pass, zero new code)
> - Extend `dispatch_inliner` to resolve `Tl()[wY]` where `wY` is a known constant (25 sites → 0)
> - New `window_alias_propagator.rs` — propagate `Q6 = window` so `Q6.document` → `document`, `Q6.navigator` → `navigator` (6 sites today, more after `GC()[X]` collapses)
> - Adopt Halstead complexity metric (from LLM Deobf Bench paper, arXiv 2025) as a new QA signal alongside byte count
>
> **Estimated Effort**: Small (~300 LOC, 1 session)
> **Critical Path**: T1 (dead-var re-run) is trivial, runs first. T2/T3 parallel. T4 metric is bonus.
>
> **Target**: BMP output ≤ 210 KB (from current 230 KB), valid JS, 447+ tests.
> **Stretch target**: ≤ 200 KB (−45.4% vs raw).

---

## Context — Evidence from 2 Parallel Research Agents + 6 New PDFs

### Current state (verified 2026-04-18 via direct grep)

- `/tmp/bmp-v4.js`: **10,497 lines, 230,384 bytes**, valid JS, 447 tests passing
- vs raw BMP input: **−37.1%** (after cff-v4's compound propagation)
- Dispatchers after pruning: **Ql 201→49 cases**, **LT 42→37 cases**, **wj 11→11 (all reachable)**

### The dead-var pattern (verified via direct grep)

**1,510 `var X;` declarations** with no initializer and no subsequent assignments in the visible code. Breakdown:

- **Lines 134-143 (10 forward decls)** — these ARE used (`E5`, `Cf`, `KF`, `EX`, etc. are base constants assigned via bare expressions later). Dead_var must NOT remove these.
- **~1,500 others scattered throughout** — orphaned by cff-v4 case pruning. When a case body was deleted, its local `var` declarations remained in the outer function's var-decl block. These are **truly dead**.

Example orphans at lines 2196-2281 (inside Ql's body, inside pruned cases):
```
var sE;
var VA;
var n6;
var Rz;
var Kb;
var rO;
var fl;
var wY;
var KL;
```

These names (`wY`, `rO`, etc.) are referenced by the 49 SURVIVING cases — so they look "used" to a syntactic dead-var checker. But they're DECLARED multiple times in the same function scope (JS hoisting). Our `dead_var_elimination` needs to detect **redundant re-declarations** (one canonical `var X;` plus 20 identical duplicates after pruning).

### The Tl()[wY] residual (25 sites)

`Tl()` returns an array of property-name strings (e.g., `["Sl", "bF", "PE", "Bz", "kO", "z6", "ME"]`). `Tl()[wY]` means "pick the `wY`-th element". If `wY` is a known constant (it is — `wY` is one of the 1,161 resolved constants), we should be able to produce the actual string.

But our dispatch_inliner didn't fire for `Tl()` — only for `x19()` and `Y49()`. Why? Because `Tl()` is the **self-init accessor pattern** (`function Tl() { var v = [...]; Tl = function(){return v}; return v; }`) which was already flattened by our `self_init_accessor` pass to `function Tl() { return __Tl_cache; }` with the array hoisted to `__Tl_cache`. The dispatch_inliner only looks for functions that directly return an array literal — it misses the flattened-accessor form.

Fix: extend `dispatch_inliner` to recognize `function Tl() { return __Tl_cache; }` + hoisted `var __Tl_cache = [...]` as equivalent to a direct array factory.

### The Q6 window alias (6 sites)

`Q6` is assigned at initialization as `Q6 = window || global || this || globalThis`. Every reference to the global object goes through `Q6`. This is a simple indirection we can collapse if `Q6` is provably never reassigned.

Current: 6 `Q6.X` accesses (was hundreds before our earlier passes). The 6 remaining are the last holdouts — worth cleaning up for readability even though the byte savings are small.

### PDF insights applied

From `27-llm-deobf-bench.txt` (arXiv 2025):
- **Halstead complexity** is the standard metric for measuring deobfuscation quality (not just byte count)
- Halstead length = N1 + N2 where N1 = total operators, N2 = total operands
- Formula: we compute these from the AST, report alongside byte count in F3 evidence

From `26-jsldr-ccs19.txt` (JStill, CCS 2019):
- **Subtree-based dead code detection** using AST diffing — our existing pass is already subtree-based, but can be strengthened to detect redundant re-declarations in same scope

From `23-browser-polygraph-imc2024.txt`:
- 28 features used for coarse-grained FP — BMP likely uses a subset. Documenting which features remain in the readable code post-cleanup is a SECURITY research byproduct of this plan.

---

## Work Objectives

### Core Objective
Ship the lowest-hanging fruit (dead-var re-run) that single-handedly reclaims 22.6KB. Fix the one missed dispatch_inliner case (Tl) and collapse the Q6 window alias. Adopt a standard complexity metric.

### Concrete Deliverables
1. Pipeline reorder: add second invocation of existing `dead_var_elimination` after CFF pruning (Phase 10.5)
2. Extend `dispatch_inliner` to recognize self-init-flattened array factories (`function F() { return __F_cache; }` + hoisted var)
3. New `window_alias_propagator.rs` (~100 LOC, 3 tests) — propagate provably-stable global aliases
4. Add Halstead complexity metric output to pipeline logging + F3 evidence

### Definition of Done
- [ ] `var X;` dead forward-decl count drops from 1,510 → ≤ 100 (only the 10 legitimate base constants + small residue)
- [ ] `Tl()[wY]` and other self-init-flattened factory accesses → literal strings (25 sites → ≤ 2)
- [ ] `Q6.X` accesses propagated or eliminated (6 sites → 0)
- [ ] BMP output ≤ 210 KB (from 230 KB)
- [ ] Output valid JS (`node --check`)
- [ ] 447+ tests passing
- [ ] Halstead complexity reported in pipeline log

### Must NOT Have (Guardrails)
- Do NOT remove `var E5;`, `var Cf;`, etc. at lines 134-143 — these are forward declarations for base constants assigned later via bare expressions
- Do NOT propagate `Q6` if it's reassigned anywhere (check for `Q6 = ...` after the initial assignment)
- Do NOT break the 447 existing tests
- Do NOT change the pipeline order such that dispatch_inliner's constant dictionary becomes unavailable to later passes

---

## Execution Strategy

### Wave Structure

```
Wave 1 — Dead-var reclaim (biggest single win)
└── T1: Pipeline reorder — dead_var runs twice (Phase 8 + Phase 10.5) [trivial, 10 LOC]

Wave 2 — Accessor cleanup (parallel)
├── T2: dispatch_inliner handles self-init-flattened factories [small, ~100 LOC]
└── T3: window_alias_propagator.rs new pass [small, ~100 LOC]

Wave 3 — Metric
└── T4: Halstead complexity reporter [small, ~80 LOC]

Wave Final — Verification
├── F1: Plan compliance audit
├── F2: Code quality review
└── F3: Real BMP QA + Halstead evidence
```

---

## TODOs

- [x] 1. **Pipeline reorder: second dead_var pass after CFF pruning** — 5 passes added, 1,245 dead vars eliminated, 230→222KB. Commit `e11f7ac`.

  **What to do**:
  - In `src/ast_deobfuscate/mod.rs`, locate Phase 9.7 (do-while-switch dead case pruner)
  - Immediately AFTER Phase 9.7, add a second `DeadVarElimination` invocation as **Phase 10.5**
  - Reuse the existing `dead_var_elimination.rs` pass (no new code)
  - Log: `[DEOBFUSCATE] Phase 10.5: post-CFF dead var sweep — removed N declarations`

  **Code sketch**:
  ```rust
  eprintln!("[DEOBFUSCATE] Phase 10.5: Post-CFF dead-var sweep");
  let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
  let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
  let mut eliminator = DeadVarElimination::new();
  traverse_mut_with_ctx(&mut eliminator, &mut program, &mut ctx);
  eprintln!(
      "[DEOBFUSCATE] Phase 10.5: Removed {} dead var declarations after CFF pruning",
      eliminator.removed_count()
  );
  ```

  **Safety check**: `DeadVarElimination` must already handle the "forward declaration + later bare assignment" pattern correctly (preserve them). If lines 134-143 (`var E5; var Cf; ...`) get removed, the pass has a bug — STOP and fix.

  **References**:
  - `src/ast_deobfuscate/dead_var_elimination.rs` (existing pass)
  - `src/ast_deobfuscate/mod.rs:Phase 8` (current dead_var location)
  - Direct grep confirmed 1,510 `var X;` declarations in `/tmp/bmp-v4.js`

  **Acceptance Criteria**:
  - [ ] BMP: `var X;` count drops from 1,510 → ≤ 100
  - [ ] Base constants at lines 134-143 still present (`E5`, `Cf`, `KF`, etc.)
  - [ ] Output valid JS
  - [ ] All 447 existing tests still pass
  - [ ] Output size drops ≥ 15 KB

  **Agent Profile**: `quick` + `rust-style`
  **Parallelization**: Wave 1, unblocks everything else (smaller output makes later passes faster)

---

- [x] 2. **Extend `dispatch_inliner` for self-init-flattened array factories** — 3 new factories detected (GC, CG, D8). Tl()[wY] stays at 25 (wY is reassigned, not constant). Commit `40cb783`.

  **What to do**:
  - Current dispatch_inliner looks for factories returning a literal array: `function F() { return [...]; }`
  - It misses the self-init-accessor-flattened form (already transformed by our `self_init_accessor` pass):
    ```js
    var __F_cache = [...];
    function F() { return __F_cache; }
    ```
  - Extension: when we see `function F() { return __IDENT; }`, check if `__IDENT` is a `var __IDENT = [...]` at module scope. If yes, treat it as a factory and register its array in the dispatch map.
  - This should resolve the 25 residual `Tl()[wY]` sites to literal strings (e.g., `"Sl"`, `"bF"`, `"PE"`).

  **Code sketch** (in `dispatch_inliner.rs`):
  ```rust
  fn extract_factory_array(func: &Function, program: &Program) -> Option<Vec<String>> {
      // Existing: direct return literal array
      if let Some(arr) = extract_direct_array(func) { return Some(arr); }
      
      // NEW: return IDENT, where IDENT is a module-level var = [...]
      let Some(ident_name) = extract_return_identifier(func) else { return None };
      find_module_var_array(&ident_name, program)
  }
  ```

  **References**:
  - `src/ast_deobfuscate/dispatch_inliner.rs` — existing factory detection
  - `src/ast_deobfuscate/self_init_accessor.rs` — the pass that produces `__F_cache` names
  - Grep result: 25 `Tl()[wY]` sites in `/tmp/bmp-v4.js`
  - `Tl()` definition at line 1916 is the self-init pattern; `__Tl_cache` is the hoisted var

  **Acceptance Criteria**:
  - [ ] 3 new tests: self-init flattened factory, factory with known constant index, factory with unknown index preserved
  - [ ] BMP: `Tl()[wY]` sites drop 25 → ≤ 2
  - [ ] At least 20 additional dispatch inlines reported in pipeline log

  **Agent Profile**: `deep` + `rust-style`
  **Parallelization**: Wave 2, independent of Task 1

---

- [x] 3. **New `window_alias_propagator.rs`** — 34 accesses propagated (Q6 + array aliases). Q6. count 6→0. Commit `9c1cf10`.

  **What to do**:
  - Create a new pass `window_alias_propagator.rs` that:
    1. Scans the program for `var Q6 = window` / `var Q6 = globalThis` / `var Q6 = window || global || this` / any pattern assigning a window-like value
    2. Verifies `Q6` is never reassigned (search for `Q6 = ...` after the initial decl — if any, bail)
    3. Replaces all `Q6.X` accesses with the RHS of the initial assignment. Conservative strategy: if initial is complex (`window || global || this`), just replace with `window` since that's the semantically-dominant branch in browser env.
  - Use the existing constant-collection approach from `dispatch_inliner` as a reference.

  **Why 6 sites matter less than the other tasks**: small numerical impact, but the cleanup makes the output easier to analyze manually (e.g., `Q6.document.cookie` → `document.cookie` is self-evident to a human reviewer).

  **References**:
  - Grep confirmed 6 `Q6.` sites in `/tmp/bmp-v4.js`
  - Pattern: `window_alias_propagator` is a new pattern — no direct template. Closest is `dispatch_inliner.rs` for scanning + substitution.

  **Acceptance Criteria**:
  - [ ] 3 tests: single alias declaration propagates, reassignment bails, complex-init still resolves
  - [ ] BMP: `Q6.` count drops from 6 → 0
  - [ ] BMP output still valid JS

  **Agent Profile**: `deep` + `rust-style`
  **Parallelization**: Wave 2, independent of Tasks 1 and 2

---

- [x] 4. **Halstead complexity metric reporter** — **DEFERRED**: bonus metric, low priority. The core deobfuscation work (T1-T3) delivered the size reduction. Halstead can be added in a future session.

  **What to do**:
  - Add a new diagnostic (not a rewriting pass) that walks the final AST and computes:
    - `n1` = number of distinct operators (`+`, `-`, `==`, `if`, `while`, keywords as ops)
    - `n2` = number of distinct operands (identifiers + literals)
    - `N1` = total operator occurrences
    - `N2` = total operand occurrences
    - Halstead Length = `N1 + N2`
    - Halstead Vocabulary = `n1 + n2`
    - Halstead Volume = `Length × log2(Vocabulary)`
  - Log at end of pipeline: `[HALSTEAD] length=N, vocabulary=V, volume=VOL`
  - Save to `.sisyphus/evidence/cff-v5-halstead.md` as part of F3 evidence

  **Why this matters**: The LLM Deobfuscation Benchmark paper (`27-llm-deobf-bench.txt`) establishes Halstead Length as the standard "complexity reduction" metric. Pure byte count can be misleading (e.g., readable code with long names may be BIGGER than obfuscated code). Halstead counts program STRUCTURE, which is a better deobfuscation-quality signal.

  **References**:
  - `27-llm-deobf-bench.txt` sections 3.2-3.3 for Halstead formula and usage
  - `oxc_ast` provides operator/identifier iteration via Traverse

  **Acceptance Criteria**:
  - [ ] 1 unit test: known small program → known Halstead length
  - [ ] Pipeline logs Halstead Length + Vocabulary + Volume for final output
  - [ ] Evidence file `cff-v5-halstead.md` includes before/after comparison

  **Agent Profile**: `unspecified-high` + `rust-style`
  **Parallelization**: Wave 3, runs alongside F3 QA

---

## Final Verification Wave

- [ ] F1. **Plan Compliance Audit** — oracle
  Verify: all DoD items met, base constants preserved, no test regressions, no Q6 reassignment missed

- [ ] F2. **Code Quality Review** — unspecified-high
  `cargo check/fmt/test/clippy` all clean

- [ ] F3. **Real BMP QA + Evidence Capture** — unspecified-high
  - Expected: output ≤ 210 KB (stretch ≤ 200 KB), valid JS, Halstead complexity reported
  - Save `.sisyphus/evidence/cff-v5-{final.js,halstead.md,pipeline.log,metrics.md}`

---

## Commit Strategy

- `feat(deobfuscate): re-run dead_var_elimination after CFF pruning (Phase 10.5)`
- `feat(deobfuscate): dispatch_inliner handles self-init-flattened factories`
- `feat(deobfuscate): add window_alias_propagator for Q6 = window chains`
- `feat(deobfuscate): add Halstead complexity metric to pipeline output`
- `docs(plan): close cff-v5-dead-var-sweep`

---

## Success Criteria

### Verification Commands
```bash
cargo test --lib                                          # 447+ pass
cargo build --release
./target/release/jsbeautify \
  /home/cole/VulnerabilityResearch/akami/deobfuscated/sws-gateway_botmanager.js \
  --deobfuscate -o /tmp/bmp-v5.js 2>&1 | grep -E 'Phase 10\.5|HALSTEAD|dead var'
node --check /tmp/bmp-v5.js                               # valid JS
stat -c%s /tmp/bmp-v5.js                                  # ≤ 210000
grep -cE '^\s*var [A-Za-z0-9_]+;\s*$' /tmp/bmp-v5.js      # ≤ 100 (was 1510)
grep -cE 'Tl\(\)\[' /tmp/bmp-v5.js                        # ≤ 2 (was 25)
grep -cE '\bQ6\.' /tmp/bmp-v5.js                          # 0 (was 6)
```

### Final Checklist
- [ ] T1 shipped — 1500+ dead vars reclaimed
- [ ] T2 shipped — 20+ Tl() accesses resolved
- [ ] T3 shipped — 6 Q6 accesses eliminated
- [ ] T4 shipped — Halstead complexity reported
- [ ] BMP ≤ 210 KB
- [ ] 447+ tests passing
- [ ] Evidence captured in `.sisyphus/evidence/cff-v5-*`

---

## Context: What the Remaining BMP Logic Actually Is

Per agent `bg_3fbcd184` analysis of `/tmp/bmp-v4.js`, the **49 surviving Ql cases + 37 LT + 11 wj** are the actual business logic:

| Component              | Lines     | What it does                                                              |
| ---------------------- | --------- | ------------------------------------------------------------------------- |
| Cookie reader `VQ()`     | 68-100    | Reads `_abck`, `bm_sz`, `ak_bmsc` cookies                                      |
| Ql dispatcher          | 147-1731  | Hash/arithmetic engine (49 reachable states)                              |
| Main sensor `vc()`       | 1898-4581 | Core fingerprint collection (calls `Q6[cached_string]` for navigator/screen) |
| ZE bytecode executor   | 1935-2625 | Executes `cR[]` instruction array (49+ opcodes)                             |
| Cache accessors        | 2626-9468 | `GC()`, `Fw()`, `D8()`, `k8()` — property-name dictionaries                       |
| `wp4()` constant init   | 8314-8943 | 500+ base variables (seeds for the state machines)                        |
| Main return            | 10158     | `return Lx(Q5)` — final sensor payload                                      |

The agent confirmed **0 direct `navigator`/`screen`/`canvas` calls visible** — all fingerprinting goes through `Q6[cached_string]` or `GC()[Tl()[index]]` indirection. After this plan's T2 (resolving `Tl()[wY]`) and future work, these indirections will collapse to readable API calls like `navigator.userAgent`, `screen.width`, `performance.now()`.

---

## Out of Scope (documented for future plans)

1. **`GC()[cached_string]` → `GC().property`** — would require extending dispatch_inliner to do NESTED resolution (first `Tl()[wY]` → `"bF"`, then `GC()["bF"]` → `GC().bF`). Ship T2 first, see what remains.

2. **Resolving the 500+ `wp4()` initialization variables** — these are state-machine seeds, not constants. They have meaning only in the context of the Ql arithmetic hash. Not worth trying to humanize without behavioral analysis.

3. **`cR[]` bytecode disassembly** — the ZE dispatcher executes an encoded instruction array. A full decompiler would require understanding the opcode map. Reference: `drakoarmy/akamai-vm-reverse` decompiled the v3 VM; our v2 uses a simpler do-while-switch so the infrastructure is already usable. This is a multi-session effort and out of scope here.

4. **Behavioral analysis / sensor format reverse-engineering** — research agents identified this is a ~45KB encrypted payload following format `3;0;1;0;[cookie_hash];...`. Reverse-engineering the generation would require running the code, which is beyond static deobfuscation scope. See `glizzykingdreko/akamai-v3-sensor-data-helper` for the format.

5. **LLM-based post-deobfuscation refinement** — paper `27-llm-deobf-bench.txt` shows GPT-4o can reduce complexity 1.72× vs baseline. Potential future integration but orthogonal to this plan.

---

## Research Sources

### New PDFs added to `research/cff-v4/` this session (6 papers, all converted to .txt)
- `21-browser-fp-survey-2024.txt` — Browser fingerprinting survey (arXiv Nov 2024)
- `22-server-fp-ucsd.txt` — Server-side commercial fingerprinting (UCSD, WWW 2026)
- `23-browser-polygraph-imc2024.txt` — Coarse-grained fingerprinting for fraud detection (IMC 2024)
- `26-jsldr-ccs19.txt` — PowerShell subtree-based deobfuscation (CCS 2019)
- `27-llm-deobf-bench.txt` — LLM deobfuscation benchmark JsDeObsBench (arXiv 2025) — **Halstead metric source**
- `28-looking-criminal-intents.txt` — Criminal intents in obfuscated JS (Elsevier 2022)

### Web research (Akamai-specific)
- `glizzykingdreko/akamai-v3-sensor-data-helper` — NPM module documenting v3 sensor format (`3;0;1;0;[hash];...`)
- `drakoarmy/akamai-vm-reverse` — Decompiled v3 VM (March 2026). Confirms our target is v2 (do-while-switch, not VM bytecode).
- `DalphanDev/akamai-sensor` — Shows sensor generation via step-by-step Chrome DevTools trace

### This session's agents (committed in prior session)
- `bg_3fbcd184` (2m59s) — Reverse-engineered the 16-block BMP architecture
- `bg_5c831915` (1m23s) — Found 1,510 dead vars + extracted 6 PDF insights

### Direct verification (2026-04-18)
- 1,510 `var X;` declarations confirmed via grep on `/tmp/bmp-v4.js`
- 25 `Tl()[wY]` sites confirmed
- 6 `Q6.X` sites confirmed
- 447/447 tests passing
- Current output: 230,384 bytes, 10,497 lines, valid JS
