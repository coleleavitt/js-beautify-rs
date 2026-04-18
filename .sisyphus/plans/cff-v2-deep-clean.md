# CFF v2 Deep-Clean Plan — Post-v7 Follow-up

## TL;DR

> **Quick Summary**: Based on exhaustive post-v7 analysis (3 parallel agents), the next high-impact session targets 5 wins: (1) dispatch-function inlining (108 sites, LARGE), (2) `Xt(value, undefined)` wrapper normalization (55 sites, SMALL), (3) in-body trampoline wrapper extension (40 sites, SMALL), (4) CFF IIFE elision upstream fix (size regression), (5) **Ql/wj/LT do-while-switch unflattener** (3 dispatchers, 3000+ lines of boilerplate — HARD but high-value). Explicitly NOT recommended: standalone IIFE-unwrapping pass (0.32% savings, not worth a dedicated pass — upstream fix in Task 4 is better).
>
> **Deliverables**:
> - `dispatch_inliner.rs` (new, LARGE) — reverse-engineer x19/Y49/Fw/D8/hx cache tables, inline computed member access
> - `xt_unwrap.rs` (new, SMALL) — normalize `Xt(v, undefined)` and related wrapper calls
> - Extend `trampoline.rs` to handle in-body wrapper variants (40 sites)
> - Modify `cff_unflattener.rs` to skip IIFE wrapping when case body has no `var` declarations
> - `dowhile_switch_unflattener.rs` (new, HARD) — state-graph extractor + linearizer for Ql/wj/LT do-while-switch dispatchers (5 sub-tasks: 5a extractor, 5b acyclic linearizer, 5c cycle-aware linearizer, 5d entry-point rewriter, 5e tuning)
>
> **Estimated Effort**: Very Large (3-4 sessions across 9 tasks total)
> **Parallel Execution**: YES for Wave 1 (Tasks 1-2 independent). Sequential after that.
> **Critical Path**: dispatch_inliner (Task 3) OR dowhile_switch_unflattener (Task 5) — both are LARGE. Task 5 has highest impact on readability; Task 3 has highest impact on call-site clarity.

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
Shrink the v7 output below the 241KB pre-CFF baseline AND further improve readability by eliminating the top residual obfuscation classes, including the 3 do-while-switch dispatchers (Ql/wj/LT) that our current detector can't handle.

### Concrete Deliverables
1. `src/ast_deobfuscate/xt_unwrap.rs` (new, ~150 LOC + 4 tests)
2. Extend `src/ast_deobfuscate/trampoline.rs` to detect in-body wrappers (+ ~100 LOC + 3 tests)
3. `src/ast_deobfuscate/dispatch_inliner.rs` (new, ~500 LOC + 10 tests) — BIG task, may span 2 sessions
4. Modify `cff_unflattener.rs` to skip IIFE wrapping when case body is pure (upstream fix for size regression)
5. `src/ast_deobfuscate/dowhile_switch_unflattener.rs` (new, ~750 LOC across 5 sub-tasks + 20 tests) — state-machine simulator for Ql/wj/LT
6. Pipeline wiring for all new passes

### Definition of Done
- [ ] Xt wrapper pass: 55 sites normalized
- [ ] Trampoline extension: 40 in-body wrappers inlined
- [ ] Dispatch inliner: at least 80% of 108 sites resolved (some may require runtime state)
- [ ] CFF IIFE elision: output drops below 241KB pre-CFF baseline
- [ ] Do-while-switch unflattener: all 3 dispatchers (Ql/wj/LT) detected; at least sub-tasks 5a + 5c land (detector + preserve-loop-envelope cleaner) even if full linearization proves intractable
- [ ] Output valid JS (`node --check`)
- [ ] 404+ library tests still pass

### Must NOT Have
- Do NOT implement a standalone IIFE-unwrapping pass (use upstream fix in Task 4 instead)
- Do NOT force-linearize SCCs blindly in Task 5 (would blow up code size — preserve-loop fallback is the safe option)
- Do NOT break the existing dispatcher_detector for the 9 already-working dispatchers

---

## TODOs

- [x] 1. **`xt_unwrap.rs`** — normalize the Xt wrapper idiom (easy win) — **RESOLVED**: Xt was an addition proxy (`a+b`) missed by operator_proxy due to name collision with a lookup forwarder. Fixed in `lookup_forwarder.rs`. 289 call sites inlined, 0 remaining.

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

- [x] 2. **Extend `trampoline.rs` for in-body wrappers** — 77 trampolines found (was 62), 104 inlined (was 88). Remaining `.apply(this,[...])` reduced from 40 to 11.

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

- [x] 4. **Modify `cff_unflattener.rs` to skip IIFE wrapping when safe** (upstream fix for size regression) — IIFEs reduced 57→7. Size roughly neutral (274KB); real size win requires Tasks 3+5.

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

- [ ] 5. **`dowhile_switch_unflattener.rs`** — state-machine simulator for Ql/wj/LT (HARD)

  **Status**: PROMOTED from "deferred" to a proper task. The goal is **not** to inline case bodies at call sites (that's impossible because dispatch is dynamic), but to **rewrite each dispatcher into a readable imperative function** by statically simulating the state machine.

  **Target functions** (from v7 output):
  - `Ql` (aliased `SY`) at line 147–3413 — 201 cases, exit sentinel `p9`
  - `wj` (aliased `l29`) at line 7567–7817 — 11 cases, exit sentinel `sC`
  - `LT` (aliased `O29`) at line 7821–9645 — 42 cases, exit sentinel `Ck`

  **Pattern to match**:
  ```js
  var NAME = function ALT_NAME(STATE_PARAM, ARGS_PARAM) {
      do {
          switch (STATE_PARAM) {
              case LABEL_A: { ...body_A...; STATE_PARAM = LABEL_B; } break;
              case LABEL_B: { ...body_B...; STATE_PARAM = LABEL_C; } break;
              ...
              case LABEL_Z: { ...body_Z...; return VALUE; }  // exit via return
              ...
          }
      } while (STATE_PARAM != EXIT_SENTINEL);
  };
  ```

  ### Algorithm (simulation + linearization)

  **Phase A — Extract state graph** (per-dispatcher):
  1. Parse the `do { switch } while (STATE != SENTINEL)` envelope
  2. For each `case LABEL:`, record the case body (Vec<Statement>)
  3. Classify each case's exit:
     - **sequential**: ends with `STATE = NEXT_LABEL; break;` → edge `LABEL → NEXT_LABEL`
     - **conditional**: body has `if (cond) { STATE = A; } else { STATE = B; } break;` → edges `LABEL → A` and `LABEL → B`
     - **return**: body ends with `return VALUE;` → exit node
     - **loop-exit**: body sets `STATE = SENTINEL; break;` → exit node
     - **cross-call**: body calls another dispatcher (`P6(X, [...])`, `Ql(Y, [...])`) — these are side-effects in the body, not state transitions, so treated as normal statements
  4. Strip the `STATE = NEXT_LABEL;` trailer from each case body (it's control flow, not business logic)
  5. Build directed graph: `Map<StateLabel, { body: Statements, transitions: Vec<(Condition, NextLabel)> }>`

  **Phase B — Find entry states**:
  - The dispatcher's FIRST iteration starts at `STATE_PARAM`'s initial value — whatever is passed by the caller
  - Every distinct `ENTRY_LABEL` passed at call sites (via `Ql(ENTRY, [...])`, `Ql.call(this, ENTRY, [...])`, `Ql.apply(this, [ENTRY, args])`) becomes an entry point
  - For Ql: 201 possible entries; in practice only N are actually called (grep analysis)

  **Phase C — Linearize** (rewrite dispatcher as imperative code):

  The rewrite strategy depends on graph topology:

  **Strategy 1 — "Single straight chain"**: `A → B → C → ... → Z` (no branches, single entry, single exit)
  ```js
  // Rewrite to:
  function Ql(ENTRY, args) {
      // body_A
      // body_B  (STATE = B trailer stripped)
      // body_C
      ...
      return ...;
  }
  ```

  **Strategy 2 — "Fan-out with if/else"**: case body branches to A or B conditionally
  ```js
  // Rewrite to:
  if (cond) {
      // body_A inlined
      // body_A's successors inlined recursively (if non-recursive)
  } else {
      // body_B inlined
      ...
  }
  ```

  **Strategy 3 — "SCC preserved"**: when states form a cycle (self-loop or mutually-recursive among cases)
  - Keep the `while(true) { switch(state) { ... } }` structure but
  - Rename states to human-readable identifiers (via a state-label dictionary we can infer)
  - Strip the `do { ... } while (state != SENTINEL)` envelope in favor of a clean `while(true)` with explicit `break` on exit states
  - Example: a loop like "accumulate characters, increment pointer, check bounds, exit" stays as a loop but becomes recognizable

  **Strategy 4 — "Unreachable pruning"**: states never referenced by any entry point OR any successor → delete

  ### Algorithm sketch (pseudocode)

  ```rust
  struct StateNode<'a> {
      label: String,                          // e.g., "Q4"
      body: Vec<Statement<'a>>,               // case body with trailer stripped
      transitions: Vec<Transition<'a>>,       // edges to other states
      is_exit: bool,                          // ends with return or state=SENTINEL
  }

  enum Transition<'a> {
      Unconditional(String),                  // STATE = NEXT
      Conditional(Expression<'a>, String, String),  // if(cond) { STATE=A } else { STATE=B }
  }

  fn extract_state_graph<'a>(func: &Function<'a>) -> Option<StateGraph<'a>> {
      let (switch, exit_sentinel) = find_do_while_switch(&func.body)?;
      let state_param = get_discriminant_ident(&switch.discriminant)?;
      let mut graph = StateGraph::new();
      for case in &switch.cases {
          let label = get_case_label(&case.test)?;
          let (body, trans, is_exit) = classify_case_body(&case.consequent, &state_param, &exit_sentinel);
          graph.insert(label, StateNode { label, body, transitions: trans, is_exit });
      }
      Some(graph)
  }

  fn linearize<'a>(graph: &StateGraph<'a>, entries: &HashSet<String>) -> Vec<Statement<'a>> {
      let sccs = tarjan_scc(&graph);
      let mut out = Vec::new();

      if entries.len() == 1 && graph.is_single_chain(entries.iter().next().unwrap()) {
          // Strategy 1: inline the whole chain
          out = inline_chain(graph, entries.iter().next().unwrap());
      } else if sccs.is_empty() || sccs.iter().all(|s| s.len() == 1) {
          // Strategy 2: acyclic — emit as nested if/else
          out = emit_acyclic(graph, entries);
      } else {
          // Strategy 3: has cycles — preserve loop but rename + clean envelope
          out = emit_loop(graph, entries, exit_sentinel);
      }
      out
  }

  fn rewrite_dispatcher<'a>(ast: &mut Function<'a>) -> bool {
      let graph = extract_state_graph(ast).or_else(|| return false)?;
      let entries = find_all_entries_from_call_sites(ast, ast.id);
      let new_body = linearize(&graph, &entries);
      replace_do_while_switch_with(ast, new_body);
      true
  }
  ```

  ### Detector-level preconditions (bail-out rules)

  Reject dispatchers where any of:
  - Case bodies use `throw` (exception as control flow — too hard)
  - `STATE` is reassigned inside a nested function or closure (escapes scope analysis)
  - More than N cases in an SCC (cap at, say, 20 to prevent code explosion)
  - Any case reads from `args_param` via a non-literal index (dynamic argument access)

  Log which cases are handled and which are skipped. If any case is unhandled, emit the dispatcher **in a simplified form** rather than unchanged — strip the `do { } while` envelope, rename states to inferred human names, but keep the switch.

  ### Implementation phases (split into sub-tasks)

  - [ ] **5a. State graph extractor** (~150 LOC + 4 tests)
    - Detect the `do { switch } while` envelope (the fix referenced in earlier Ql/wj/LT analysis — extend `dispatcher_detector.rs` OR create a new `dowhile_detector.rs`)
    - Parse each case body into `StateNode { body, transitions, is_exit }`
    - Tests: simple chain A→B→C, conditional fan-out, return-exit, SCC

  - [ ] **5b. Linearizer (acyclic)** (~200 LOC + 6 tests)
    - Implement Strategy 1 (chain) and Strategy 2 (if/else tree)
    - Unit tests with hand-crafted 5-10 state machines
    - Verify output is valid JS via `cargo test` (semantic equivalence check by running both versions)

  - [ ] **5c. Cycle-aware linearizer** (~200 LOC + 4 tests)
    - Implement Strategy 3 (preserve loop but clean envelope + rename)
    - Tarjan's SCC algorithm
    - Decide rename scheme: use the case body's first meaningful identifier as the state name?

  - [ ] **5d. Entry-point collection + dispatcher rewriter** (~100 LOC + 3 tests)
    - Walk the whole program to find every `DISPATCHER_NAME(X, [...])` / `.call(this, X, ...)` / `.apply(this, [X, ...])` with literal X
    - Feed the resulting set of entry labels into the linearizer
    - Replace the dispatcher function body with linearized output

  - [ ] **5e. Ql/wj/LT-specific tuning** (~100 LOC)
    - The 3 real dispatchers are mutually-recursive and LARGE (Ql has 201 cases)
    - May need to linearize ONLY cases reachable from observed entry points (the 7 known sites) — this keeps output size manageable
    - Dead-case elimination for unreachable states

  **References**:
  - Agent `bg_67de11bf` output: detailed case-by-case structure of Ql/wj/LT, exit sentinels, recursive calls
  - `src/ast_deobfuscate/dispatcher_detector.rs:382-388` — the current rejection point that must be bypassed
  - `src/ast_deobfuscate/cff_unflattener.rs` — existing pattern for cloning + rewriting
  - Prior art: **ollvm-unflattener** (symbolic execution approach), **d810** (microcode CFG reconstruction) — both solve the binary-CFF equivalent

  **Acceptance Criteria** (for the combined sub-tasks):
  - [ ] All 3 dispatchers (Ql, wj, LT) detected by the new do-while detector
  - [ ] For Ql: at least 50% of cases linearized into readable imperative code (the hard 100-case tail can remain in preserved-loop form)
  - [ ] For wj (11 cases): fully linearized if acyclic, else loop-preserved
  - [ ] For LT (42 cases): same treatment as Ql
  - [ ] Output valid JS (`node --check`)
  - [ ] 20+ unit tests across sub-tasks 5a-5d
  - [ ] Size REDUCTION: expect 1000-2000 lines of dispatcher boilerplate eliminated (3000+ → ~1500 linearized)

  **Must NOT**:
  - Do NOT force-linearize through SCCs blindly — would blow up code size. Preserve-loop fallback is the safe option.
  - Do NOT rewrite case bodies that have nested `switch`/`throw` — keep those cases as-is.
  - Do NOT mix this with Task 3 (dispatch_inliner) — they operate at different layers.

  **Agent**: `ultrabrain` + `rust-style` (this is the hardest task in the plan; needs deep algorithmic thinking)
  **Parallelization**: Wave 3, depends on Tasks 1-3 completing (so the input is as clean as possible before this big rewrite). Sub-tasks 5a-5d are sequential.

  **Honest risk assessment**:
  - LOC: ~750 across 5 sub-tasks
  - Risk: HIGH — state machine simulation is subtle; easy to introduce bugs that silently change behavior
  - Expected benefit on BMP: eliminates 3000+ lines of boilerplate switch-scaffolding. If successful, brings final output well under 200KB and makes the core BMP logic actually readable.
  - If it proves intractable mid-session: ship the **detector + preserve-loop-envelope cleaner** (sub-tasks 5a + 5c only, skip full linearization). That's still ~1500 lines of boilerplate removed (just the `do { ... } while` envelopes replaced with `while(true) { break }`).

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

1. **Standalone IIFE-unwrapping pass** — 0.32% savings on v7 output. Upstream fix to `cff_unflattener.rs` (Task 4) achieves the same goal by never emitting unnecessary IIFEs in the first place. No separate post-hoc unwrapper is needed.

2. **`Ot` push/pop state tracker removal** — 112 instances but first need to verify `Ot` is never READ elsewhere in the code. If it IS read (even for debugging/integrity), removal breaks semantics. Medium-risk; medium-impact. Tackle after the 5 top wins.

3. **Multi-level property access flattening** (`this[X][Y]`) — Only 4 instances. Not worth a pass.

4. **String-padding loops** (`while (s.length < N) s += s;`) — Only 6 instances. Not worth a pass.

---

## Research Sources (agent outputs from this planning session)

- `bg_67de11bf` (2m33s) — Do-while-switch dispatcher analysis
- `bg_dfd4afc6` (2m36s) — Top-20 residual pattern hunt
- `bg_33c6889a` (3m05s) — IIFE unwrap feasibility analysis

All three agents ran against `/tmp/bmp-v7.js` (the current state of deobfuscation after the cff-unflattener plan completed).
