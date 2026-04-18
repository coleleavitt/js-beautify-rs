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
