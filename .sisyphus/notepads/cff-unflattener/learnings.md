## apply_call_simplifier: callee_is_safe_receiver was too restrictive

The original `callee_is_safe_receiver` only accepted `Identifier` and `StaticMemberExpression`.
This blocked 45+ BMP `.call(null, ...)` sites with computed member / call expression chains
like `D8()[x19()[Rj]].call(null, Hn, QS, fw)`.

Fix: removed `callee_is_safe_receiver` entirely. When the first arg is `null`/`undefined`,
the caller explicitly doesn't care about `this` binding, so any callee expression is safe
to rewrite. The null-check on the first argument is the real safety gate.
