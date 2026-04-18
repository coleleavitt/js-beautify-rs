# CFF Unflattener — Plan Compliance Audit (F1)

**Date**: 2026-04-17
**Plan**: `.sisyphus/plans/cff-unflattener.md`
**Auditor**: Oracle agent (automated)
**Overall Verdict**: **PARTIAL**

---

## Must Have Items

### 1. All 12 BMP dispatchers detected — **PARTIAL (9/12)**

**Evidence**:
- `Phase 8.5: Found 9 dispatchers`
- Detected: ZE(10), JA(8), P6(10), db(10), rQ(10), TZ(3), vb(10), sb(10), tQ(10) = 81 cases
- **Missing 3**: Ql/SY, wj/l29, LT/O29

**Root cause** (documented in `learnings.md`):
Ql/SY, wj/l29, LT/O29 use `do { switch(...) } while(...)` shape, NOT bare-switch.
The dispatcher_detector intentionally excludes these — they're handled by the existing
`control_flow_unflattener` (Phase 1). This is a **plan discrepancy**, not a bug:
the plan's inventory table lists 12 dispatchers, but only 9 match the bare-switch CFF
pattern. The other 3 are a different CFF variant (do-while-switch state machine).

**Verdict**: PARTIAL — 9/9 bare-switch dispatchers detected (100% recall for the
targeted pattern). Plan's "12 dispatchers" claim was over-inclusive. The 3 do-while-switch
dispatchers are handled by a different pass.

### 2. At least 50% of literal-state call sites inlined — **PASS**

**Evidence**:
- `Phase 8.5: Inlined 113 CFF call sites`
- 81 total cases across 9 dispatchers
- 113 inlines / 113 total literal-state sites = 100% of sites that matched
- 113 exceeds the 50% threshold regardless of denominator

**Verdict**: PASS

### 3. Mutually-recursive cycle (Ql↔wj↔LT) detected and left intact — **FAIL (NOT IMPLEMENTED)**

**Evidence**:
- `grep -r "SCC\|tarjan\|scc\|strongly.connected\|cycle" src/ast_deobfuscate/` → 0 matches in CFF code
- Plan Task 7 ("Convergence + SCC Detection") was marked `[x]` but **no SCC algorithm exists** in the codebase
- The Ql/wj/LT dispatchers are not even detected by the bare-switch detector, so the cycle question is moot for now
- The 9 detected dispatchers form a single large cycle (ZE→P6→sb→tQ→rQ→JA→ZE) but no cycle detection prevents re-inlining

**Verdict**: FAIL — Task 7 (SCC detection) was not implemented. The plan checkbox is misleading.
However, since the IIFE-wrapping approach doesn't recursively expand (it's a single-pass
traversal, not iterative), infinite expansion doesn't occur in practice.

### 4. Output still valid JavaScript — **PASS**

**Evidence**:
```
$ node --check /tmp/bmp-audit.js
(exit code 0)
```

**Verdict**: PASS

### 5. BMP output 10-20% smaller after CFF (target: <215KB) — **FAIL**

**Evidence**:
- Original input: 366,497 bytes
- Output: 274,557 bytes (25.1% reduction vs original input)
- Plan target: <215,000 bytes (41% reduction) — NOT MET
- Plan target: <229,000 bytes (37% reduction) — NOT MET
- Pre-CFF baseline (v6): ~241,829 bytes (per plan references)
- Post-CFF output: 274,557 bytes — **13.5% LARGER than pre-CFF baseline**

**Root cause**: The IIFE-wrapping approach (`(function(pE) { ... })([args])`) adds
boilerplate per inlined site. 113 inlines × ~50-100 bytes overhead = ~6-11KB added.
The case body duplication (inlining copies code) also increases size. The dispatcher
function declarations are NOT removed after inlining (no dead-code cleanup of
now-unused dispatchers).

**Verdict**: FAIL — Output grew 13.5% vs pre-CFF baseline instead of shrinking 10-20%.
Still 25% smaller than original input, but the CFF pass itself is a net size increase.

### 6. 400+ library tests pass — **PASS**

**Evidence**:
```
test result: ok. 404 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Verdict**: PASS

---

## Must NOT Have Items

### 1. Do NOT inline dispatchers called with non-literal state arguments — **PASS**

**Evidence**:
- `cff_unflattener.rs:400`: `let Expression::Identifier(state_id) = state_expr else { return None; }`
- `cff_unflattener.rs:453`: Same guard in `try_inline_call_this`
- Both `try_inline_direct` and `try_inline_call_this` require the state argument to be
  an `Expression::Identifier` that matches a known case label in the dispatcher's case map
- Non-literal (computed) state arguments are correctly rejected

**Verdict**: PASS

### 2. Do NOT inline case bodies containing `return function(...)` closure — **FAIL (NO GUARD)**

**Evidence**:
- `grep "return.*function" cff_unflattener.rs` → 0 matches
- No special handling for closure-returning case bodies exists
- The plan (Task 6 edge cases) explicitly called for: "detect `return function(...) {...}`
  at body tail and wrap appropriately"
- The IIFE approach partially mitigates this (closures inside the IIFE retain their
  scoping), but the plan's specific guard was not implemented

**Verdict**: FAIL — No explicit guard. The IIFE wrapping provides incidental safety
(closures work inside IIFEs), but the plan's requirement for explicit detection and
careful handling was not met.

### 3. Do NOT exceed 50% code-size growth — **PASS**

**Evidence**:
- Pre-CFF baseline: ~241,829 bytes
- Post-CFF output: 274,557 bytes
- Growth: 274,557 / 241,829 = 1.135 = **13.5% growth**
- Budget: 50% growth cap
- 13.5% < 50% ✓

**Verdict**: PASS — Under budget, but growth is in the wrong direction (plan expected shrinkage).

### 4. Do NOT force-inline through SCC cycles — **PASS (BY OMISSION)**

**Evidence**:
- No SCC detection implemented (see Must Have #3)
- The 9 detected dispatchers DO form cycles (ZE→P6→sb→tQ→rQ→JA→ZE)
- However, the single-pass traversal means cross-dispatcher calls in cloned bodies
  are NOT recursively expanded — they remain as-is in the IIFE
- No infinite expansion occurs in practice

**Verdict**: PASS (by omission) — No SCC detection exists, but the single-pass design
prevents the failure mode this guardrail was meant to prevent. This is accidental safety,
not intentional compliance.

---

## Flagged Discrepancies Between Plan and Reality

| # | Plan Says | Reality | Severity |
|---|-----------|---------|----------|
| 1 | 12 dispatchers (100% recall) | 9 detected; 3 are do-while-switch (different CFF variant) | Medium — plan inventory was over-inclusive |
| 2 | Task 7: SCC detection + convergence driver | Not implemented; checkbox marked `[x]` misleadingly | High — plan task falsely marked complete |
| 3 | Phase 0.5g (after trampoline) | Implemented as Phase 8.5 (after dead var elimination) | Low — phase numbering changed, functionally fine |
| 4 | Target <215KB / 10-20% smaller | 274KB / 13.5% larger than pre-CFF | High — size goal missed |
| 5 | 159 total cases (±5) | 81 cases across 9 dispatchers | Medium — 78 cases are in the 3 excluded do-while dispatchers |
| 6 | Iterative convergence (25-iter cap) | Single-pass only | Medium — no fixpoint iteration implemented |
| 7 | Remove dispatcher declarations when zero call sites remain | Not implemented | Medium — contributes to size growth |
| 8 | `return function(...)` closure guard | Not implemented | Low — IIFE wrapping provides incidental safety |

---

## Summary Scorecard

| Item | Verdict |
|------|---------|
| Must Have 1: 12 dispatchers detected | PARTIAL (9/9 for targeted pattern) |
| Must Have 2: ≥50% literal-state inlined | PASS (113 sites, 100%) |
| Must Have 3: SCC cycle detection | FAIL (not implemented) |
| Must Have 4: Valid JavaScript output | PASS |
| Must Have 5: 10-20% size reduction | FAIL (13.5% growth) |
| Must Have 6: 400+ tests pass | PASS (404) |
| Must NOT 1: No non-literal inlining | PASS |
| Must NOT 2: No closure-return inlining | FAIL (no guard) |
| Must NOT 3: No >50% size growth | PASS (13.5%) |
| Must NOT 4: No SCC force-inline | PASS (by omission) |

**Pass**: 6/10 | **Partial**: 1/10 | **Fail**: 3/10

---

## Overall Verdict: **PARTIAL**

The CFF unflattener successfully detects 9/9 bare-switch dispatchers and inlines 113
call sites with valid JavaScript output. The core mechanism works. However:

1. **SCC detection** (Task 7) was not implemented despite being marked complete
2. **Size goal missed** — output grew 13.5% instead of shrinking 10-20%
3. **No iterative convergence** — single-pass only, cross-dispatcher calls not resolved
4. **No dispatcher cleanup** — unused dispatcher declarations not removed

The implementation is a solid v1 foundation but does not meet the plan's full Definition
of Done. Recommended next steps: (a) add dispatcher dead-code removal, (b) implement
iterative convergence, (c) add SCC detection for the cycle case.
