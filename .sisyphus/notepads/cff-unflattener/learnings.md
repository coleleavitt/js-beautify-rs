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
