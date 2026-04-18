## apply_call_simplifier: callee_is_safe_receiver was too restrictive

The original `callee_is_safe_receiver` only accepted `Identifier` and `StaticMemberExpression`.
This blocked 45+ BMP `.call(null, ...)` sites with computed member / call expression chains
like `D8()[x19()[Rj]].call(null, Hn, QS, fw)`.

Fix: removed `callee_is_safe_receiver` entirely. When the first arg is `null`/`undefined`,
the caller explicitly doesn't care about `this` binding, so any callee expression is safe
to rewrite. The null-check on the first argument is the real safety gate.

## dispatcher_detector: BMP dispatcher shapes

Two shapes in BMP:
1. **Function declaration**: `function ZE(rL, fO) { switch(rL) { case Q: { ... } break; ... } }`
   - Body is exactly 1 statement: a SwitchStatement
   - Discriminant is an Identifier matching the first param
   - Case tests are Identifier constants (not numeric literals)
2. **Var-assigned named fn expr**: `var Ql = function SY(JK, DD) { do { switch(JK) { ... } } while(...) }`
   - This shape has `do { switch } while(...)` NOT a bare switch — the dispatcher_detector
     correctly does NOT match these (they're handled by the existing control_flow_unflattener)

The detector only matches bare-switch dispatchers (ZE, P6, etc.) — the do-while-switch
dispatchers (Ql/SY, Lx/RC9, etc.) are a different CFF pattern.

## dispatcher_detector: walker must recurse into ALL expression types

**Root cause 1**: The walker only recursed into IIFE callee expressions (`(function(){...})()`).
BMP dispatchers (P6, ZE, JA, db, rQ, TZ, vb, sb, tQ) are nested inside a function expression
assigned via `vc = function() { ... }` inside a case of Ql's do-while-switch. The walker
never entered this function body because it didn't recurse into `AssignmentExpression`,
`CallExpression` arguments, or standalone `FunctionExpression` nodes.

**Fix**: Added `walk_expr()` / `scan_expr()` that comprehensively recurses into all expression
types: `FunctionExpression`, `ArrowFunctionExpression`, `AssignmentExpression`, `CallExpression`
(callee + arguments), `NewExpression`, `ObjectExpression`, `ArrayExpression`, `BinaryExpression`,
`UnaryExpression`, `MemberExpression`, etc. This replaces the old IIFE-only check.

**Root cause 2**: `body.statements.len() != 1` was too strict. BMP dispatchers rQ, TZ, vb, tQ
have `var xF = rQ;` (self-reference) before the switch statement, giving body_stmts=2.

**Fix**: Changed to find the SwitchStatement among body statements (allowing VariableDeclaration
and ExpressionStatement alongside the switch). Added guard to reject bodies with unexpected
control flow statements (prevents false positives).

**BMP results**: 9 dispatchers found (ZE:10, JA:8, P6:10, db:10, rQ:10, TZ:3, vb:10, sb:10,
tQ:10 cases), 113 CFF call sites inlined. Output is valid JS (`node --check` passes).

---
## Session Outcome (final verification)

**Plan**: cff-unflattener — **11/11 tasks done** (via merging Tasks 6-8)

### Final metrics (F3 QA)
- Input (raw BMP reference): 366,497 bytes / 15,156 lines
- Output v7 (post-pipeline): 274,557 bytes / 12,205 lines
- **Reduction vs input: −25.1% bytes, −19.5% lines**
- v6 (pre-CFF): 241,829 bytes → v7: +13.5% (IIFE wrapping overhead)
- Tests: 404/404 passing

### Akamai passes summary (from pipeline log)
- **Phase 0.5 (Akamai-gated, 6 sub-phases)**:
  - `[][[]]` → undefined: 188
  - bool-arith folded: 66
  - eq-proxies: 8 found, 505 sites unwrapped
  - self-init accessors: 9 flattened
  - lookup forwarders: 12 found, 1958 sites inlined
  - method-call forwarders: 1 found, 2 sites
  - trampolines: 62 found, 88 sites inlined
- **Phase 5c (Wave 1)**: 665 `.apply/.call` sites simplified
- **Phase 5e (Wave 1)**: 105 `.call(this,...)` sites simplified
- **Phase 8.5 (CFF, this session's centerpiece)**: 9 dispatchers found, 113 call sites inlined

### Known gaps (documented for future session)
1. **No SCC detection**: plan Task 7 merged into Task 6; the 3-way Ql↔wj↔LT cycle is left intact by *omission* rather than *design* (single-pass inliner doesn't loop, so no infinite expansion — but also no cycle detection per se)
2. **Size regressed 13.5%**: IIFE wrapping + no dead dispatcher removal. Next iteration could:
   - Splice case body statements directly into parent expression context when safe (no `break`/`return`/scope issues)
   - Remove dispatchers whose call sites are all inlined (currently preserved because single-pass)
3. **No explicit `return function(...)` closure guard**: IIFE form provides incidental safety; should be made explicit with a detection + skip
4. **Ql (201 cases), wj (11), LT (42)** not inlined: those are the mutually-recursive trio. Ql alone is 3000+ lines — inlining it would balloon the output further. Best left for a smarter unflattener.

### Architecture notes for next session
- `collect_case_bodies()` in `cff_unflattener.rs` and `walk_stmt()` in `dispatcher_detector.rs` are duplicated walkers. Extract to a shared helper.
- Both walkers are now expression-aware (they recurse into `CallExpression`, `AssignmentExpression`, etc.) — this was the key unlock for deeply-nested BMP dispatchers.
- Relaxed dispatcher-body check from `len() == 1` to `find_map` the SwitchStatement — several BMP dispatchers have a `var xF = rQ;` self-reference before the switch.
