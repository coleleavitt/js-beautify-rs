//! esbuild Runtime Helper Detection and Annotation
//!
//! Detects and annotates esbuild-specific runtime helpers:
//! - `__commonJS` / `__commonJSMin` (B) - Lazy CJS module wrapper
//! - `__esm` / `__esmMin` (L) - Lazy ESM initializer
//! - `__export` (G8) - Named exports with property descriptors
//! - `__toESM` (Y6) - CJS→ESM conversion
//! - `__toCommonJS` (bq) - ESM→CJS wrapper
//! - Object aliases (HI6 = Object.defineProperty, etc.)
//!
//! Also handles Anthropic-specific modifications:
//! - `__export` with `configurable: true` and `set:` accessor
//! - WeakMap caching in `__toESM` / `__toCommonJS` (dual-cache oz5/az5, single-cache jH7)
//! - Getter/setter bind helpers (MH7, sz5, tz5)

use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, Expression, Function, Program, Statement, VariableDeclarator,
};
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::FxHashMap;

use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

/// Detected bundle type based on helper patterns
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BundleType {
    /// Standard esbuild without modifications
    Standard,
    /// Anthropic-modified esbuild (Claude Code, etc.)
    Anthropic,
    /// Unable to determine
    #[default]
    Unknown,
}

/// Anthropic-specific modifications to standard esbuild helpers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnthropicModification {
    /// `__export` with `configurable: true` and `set:` accessor
    ConfigurableExport,
    /// WeakMap caching in module conversion helpers
    WeakMapCache(WeakMapCacheKind),
    /// Getter bind helper using `this[param]` pattern
    GetterBind,
    /// Setter bind helper for configurable exports
    SetterBind,
    /// Identity function `(q) => q` used by setter bind
    IdentityFunction,
}

/// Types of WeakMap caches used by Anthropic's esbuild fork
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeakMapCacheKind {
    /// Cache for `__toESM` in node mode (K=true) - e.g., `oz5`
    ToEsmNodeMode,
    /// Cache for `__toESM` in browser mode (K=false) - e.g., `az5`
    ToEsmBrowserMode,
    /// Cache for `__toCommonJS` - e.g., `jH7`
    ToCommonJs,
}

/// Anthropic-only support helpers (not in standard esbuild)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportHelperKind {
    /// Getter bind: `function(q) { return this[q] }` - used for efficient property access
    GetterBind,
    /// Identity function: `(q) => q` - used by setter bind
    Identity,
    /// Setter bind: `function(q, K) { this[q] = ... }` - used by configurable export
    SetterBind,
}

/// Types of esbuild runtime helpers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EsbuildHelperKind {
    /// `(cb, mod) => () => (mod || cb((mod = {exports: {}}).exports, mod), mod.exports)`
    CommonJs,
    /// `(fn, res) => () => (fn && (res = fn(fn = 0)), res)`
    Esm,
    /// `for (name in all) __defProp(target, name, {get: all[name], enumerable: true})`
    Export,
    /// CJS→ESM conversion with prototype chain
    ToEsm,
    /// ESM→CJS wrapper with `__esModule` flag
    ToCommonJs,
    /// Property copier for re-exports
    CopyProps,
    /// `export * from` implementation
    ReExport,
    /// `var __defProp = Object.defineProperty`
    ObjectAlias(ObjectAliasKind),
    /// Anthropic-only support helpers (MH7, sz5, tz5)
    SupportHelper(SupportHelperKind),
    /// WeakMap cache variable declaration
    WeakMapCache(WeakMapCacheKind),

    /// `(mod && mod.__esModule) ? mod : { "default": mod }`
    ImportDefault,
    /// `if (mod && mod.__esModule) return mod; ... __setModuleDefault(result, mod)`
    ImportStar,
    /// `Object.defineProperty(o, k2, { enumerable: true, get: function() { return m[k]; } })`
    CreateBinding,
    /// `for (var p in m) if (p !== "default" && !hasOwnProperty.call(exports, p)) __createBinding(...)`
    ExportStar,

    /// Checks for `Symbol.dispose` or `Symbol.for("Symbol.dispose")`
    AddDisposableResource,
    /// `SuppressedError` constructor, resource stack management
    DisposeResources,

    /// `new (P || (P = Promise))(function (resolve, reject) { ... step(generator.next(value)) })`
    Awaiter,
    /// `{ label: 0, sent: function() {}, trys: [], ops: [] }` state machine
    Generator,
    /// `Symbol.asyncIterator`, `q = []`, async iteration protocol
    AsyncGenerator,
    /// Simpler than `__awaiter`, uses Promise.resolve chaining
    Async,

    /// `if (kind === "a" && !f) throw new TypeError(...); return kind === "m" ? f : ...`
    PrivateGet,
    /// `if (kind === "m") throw new TypeError("Private method is not writable")`
    PrivateSet,
    /// `if (state.has(obj)) throw new TypeError(...); state.add(obj)`
    PrivateAdd,
    /// Returns the private method bound to the instance
    PrivateMethod,
    /// `typeof state === "function" ? receiver === state : state.has(receiver)`
    PrivateIn,
    /// Simple field assignment with defineProperty
    PublicField,
    /// Private field getter/setter wrapper
    PrivateWrapper,

    /// `Math.pow(base, exp)` polyfill for `**` operator
    Pow,
    /// `Object.defineProperty(f, "name", { value: name, configurable: true })`
    Name,
    /// Fallback require() with Proxy support for missing modules
    Require,
    /// `import.meta.glob` handler
    Glob,
    /// Object rest key normalization for `...rest` patterns
    RestKey,
    /// `{ ...rest } = obj` object rest extraction
    ObjRest,

    /// `for (k in b) if (__hasOwnProp.call(b, k)) a[k] = b[k]` with symbol support
    SpreadValues,
    /// Spread via Object.defineProperties with descriptors
    SpreadProps,
    /// `{ a, ...rest } = obj` rest property extraction (tslib)
    Rest,
    /// Legacy spread: `ar.concat(__read(arguments[i]))`
    Spread,
    /// `Array(s)` concatenation spread
    SpreadArrays,
    /// Modern spread with `Array.prototype.slice.call`
    SpreadArray,

    /// `extendStatics(d, b); d.prototype = Object.create(b.prototype)`
    Extends,
    /// `Object.assign || function(t) { for (var p in s) ... }`
    Assign,

    /// `Symbol.iterator` iteration protocol
    Values,
    /// `while ((n === void 0 || n-- > 0) && !(r = i.next()).done) ar.push(r.value)`
    Read,
    /// `this instanceof __await ? (this.v = v, this) : new __await(v)`
    Await,
    /// `yield* o` async delegation wrapper
    AsyncDelegator,
    /// `Symbol.asyncIterator` async iteration
    AsyncValues,
    /// `yield*` in async generators
    YieldStar,
    /// `for await (const x of y)` lowering
    ForAwait,

    /// `Reflect.get(target, prop, receiver)` super property access
    SuperGet,
    /// `Reflect.set(target, prop, value, receiver)` super property assignment
    SuperSet,
    /// Combined super getter/setter wrapper
    SuperWrapper,
    /// Tagged template literal: `Object.defineProperty(cooked, "raw", { value: raw })`
    Template,
    /// `{ raw: [...] }` template object creation (tslib)
    MakeTemplateObject,

    /// `atob` / Base64 decode to Uint8Array (browser)
    ToBinary,
    /// `Buffer.from(base64, "base64")` (Node.js)
    ToBinaryNode,
    /// `typeof x === "symbol" ? x : "".concat(x)` property key normalization
    PropKey,
    /// `Object.defineProperty(f, "name", ...)` function name setter
    SetFunctionName,
    /// Temporal dead zone error: `throw new ReferenceError("...")`
    EarlyAccess,

    /// `decorators.reduceRight((o, d) => d(o), target)` decorator application
    Decorate,
    /// `function(paramIndex, decorator) { return (target, key) => decorator(...) }`
    Param,
    /// ES decorators: `context.addInitializer`, `Reflect.decorate`
    EsDecorate,
    /// `Reflect.metadata(metadataKey, metadataValue)`
    Metadata,
    /// Class/method/field decorator application (esbuild)
    DecorateClass,
    /// Parameter decorator wrapper (esbuild)
    DecorateParam,
    /// JS decorator metadata initialization
    DecoratorStart,
    /// Attach metadata to decorated element
    DecoratorMetadata,
    /// Execute decorator initializers
    RunInitializers,
    /// JS decorator element processing
    DecorateElement,
}

/// Types of Object method aliases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectAliasKind {
    DefineProperty,
    GetOwnPropertyNames,
    GetOwnPropertyDescriptor,
    GetPrototypeOf,
    Create,
    HasOwnProperty,
    PropertyIsEnumerable,
}

/// Information about a detected esbuild helper
#[derive(Debug, Clone)]
pub struct EsbuildHelperInfo {
    pub mangled_name: String,
    pub kind: EsbuildHelperKind,
    pub line_number: u32,
    pub anthropic_modifications: Vec<AnthropicModification>,
}

impl EsbuildHelperInfo {
    #[must_use]
    pub fn new(mangled_name: String, kind: EsbuildHelperKind, line_number: u32) -> Self {
        Self {
            mangled_name,
            kind,
            line_number,
            anthropic_modifications: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_modification(mut self, modification: AnthropicModification) -> Self {
        self.anthropic_modifications.push(modification);
        self
    }

    #[must_use]
    pub fn is_anthropic_modified(&self) -> bool {
        !self.anthropic_modifications.is_empty()
    }
}

/// Collector for esbuild runtime helpers
pub struct EsbuildHelperCollector {
    helpers: FxHashMap<String, EsbuildHelperInfo>,
    object_aliases: FxHashMap<String, ObjectAliasKind>,
    module_wrappers: Vec<String>,
    weakmap_caches: FxHashMap<String, WeakMapCacheKind>,
    support_helpers: FxHashMap<String, SupportHelperKind>,
    bundle_type: BundleType,
}

impl EsbuildHelperCollector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            helpers: FxHashMap::default(),
            object_aliases: FxHashMap::default(),
            module_wrappers: Vec::new(),
            weakmap_caches: FxHashMap::default(),
            support_helpers: FxHashMap::default(),
            bundle_type: BundleType::Unknown,
        }
    }

    #[must_use]
    pub fn get_helpers(&self) -> &FxHashMap<String, EsbuildHelperInfo> {
        &self.helpers
    }

    #[must_use]
    pub fn get_object_aliases(&self) -> &FxHashMap<String, ObjectAliasKind> {
        &self.object_aliases
    }

    #[must_use]
    pub fn get_module_wrappers(&self) -> &[String] {
        &self.module_wrappers
    }

    #[must_use]
    pub fn get_weakmap_caches(&self) -> &FxHashMap<String, WeakMapCacheKind> {
        &self.weakmap_caches
    }

    #[must_use]
    pub fn get_support_helpers(&self) -> &FxHashMap<String, SupportHelperKind> {
        &self.support_helpers
    }

    #[must_use]
    pub fn get_bundle_type(&self) -> BundleType {
        self.bundle_type
    }

    fn determine_bundle_type(&mut self) {
        let has_anthropic_export = self.helpers.values().any(|h| {
            matches!(h.kind, EsbuildHelperKind::Export)
                && h.anthropic_modifications
                    .iter()
                    .any(|m| matches!(m, AnthropicModification::ConfigurableExport))
        });

        let has_weakmap_cache = !self.weakmap_caches.is_empty();
        let has_support_helpers = !self.support_helpers.is_empty();

        self.bundle_type = if has_anthropic_export || has_weakmap_cache || has_support_helpers {
            BundleType::Anthropic
        } else if !self.helpers.is_empty() {
            BundleType::Standard
        } else {
            BundleType::Unknown
        };
    }

    fn try_detect_object_alias(&mut self, declarator: &VariableDeclarator<'_>) {
        let Some(init) = &declarator.init else {
            return;
        };

        let var_name = match &declarator.id {
            BindingPattern::BindingIdentifier(ident) => ident.name.as_str().to_string(),
            _ => return,
        };

        let Expression::StaticMemberExpression(member) = init else {
            return;
        };

        let Expression::Identifier(obj) = &member.object else {
            return;
        };

        if obj.name.as_str() != "Object" {
            return;
        }

        let kind = match member.property.name.as_str() {
            "defineProperty" => ObjectAliasKind::DefineProperty,
            "getOwnPropertyNames" => ObjectAliasKind::GetOwnPropertyNames,
            "getOwnPropertyDescriptor" => ObjectAliasKind::GetOwnPropertyDescriptor,
            "getPrototypeOf" => ObjectAliasKind::GetPrototypeOf,
            "create" => ObjectAliasKind::Create,
            _ => return,
        };

        eprintln!("[ESBUILD] Detected Object alias: {} = Object.{:?}", var_name, kind);
        self.object_aliases.insert(var_name.clone(), kind);
        self.helpers.insert(
            var_name.clone(),
            EsbuildHelperInfo::new(var_name, EsbuildHelperKind::ObjectAlias(kind), declarator.span.start),
        );
    }

    fn try_detect_prototype_alias(&mut self, declarator: &VariableDeclarator<'_>) {
        let Some(init) = &declarator.init else {
            return;
        };

        let var_name = match &declarator.id {
            BindingPattern::BindingIdentifier(ident) => ident.name.as_str().to_string(),
            _ => return,
        };

        let Expression::StaticMemberExpression(member) = init else {
            return;
        };

        let Expression::StaticMemberExpression(inner) = &member.object else {
            return;
        };

        let Expression::Identifier(obj) = &inner.object else {
            return;
        };

        if obj.name.as_str() != "Object" || inner.property.name.as_str() != "prototype" {
            return;
        }

        let kind = match member.property.name.as_str() {
            "hasOwnProperty" => ObjectAliasKind::HasOwnProperty,
            "propertyIsEnumerable" => ObjectAliasKind::PropertyIsEnumerable,
            _ => return,
        };

        eprintln!(
            "[ESBUILD] Detected Object.prototype alias: {} = Object.prototype.{:?}",
            var_name, kind
        );
        self.object_aliases.insert(var_name.clone(), kind);
        self.helpers.insert(
            var_name.clone(),
            EsbuildHelperInfo::new(var_name, EsbuildHelperKind::ObjectAlias(kind), declarator.span.start),
        );
    }

    /// Pattern: `(cb, mod) => () => (mod || cb((mod = {exports: {}}).exports, mod), mod.exports)`
    fn is_commonjs_pattern(arrow: &ArrowFunctionExpression<'_>) -> bool {
        if arrow.params.items.len() != 2 || !arrow.expression {
            return false;
        }

        let Some(body) = &arrow.body.statements.first() else {
            return false;
        };

        let Statement::ExpressionStatement(expr_stmt) = body else {
            return false;
        };

        let Expression::ArrowFunctionExpression(inner_arrow) = &expr_stmt.expression else {
            return false;
        };

        if !inner_arrow.params.items.is_empty() || !inner_arrow.expression {
            return false;
        }

        let Some(inner_body) = inner_arrow.body.statements.first() else {
            return false;
        };

        let Statement::ExpressionStatement(_inner_expr) = inner_body else {
            return false;
        };

        let Statement::ExpressionStatement(expr_stmt) = body else {
            eprintln!("[CJS DEBUG] Failed: not ExpressionStatement");
            return false;
        };

        let Expression::ArrowFunctionExpression(inner_arrow) = &expr_stmt.expression else {
            eprintln!("[CJS DEBUG] Failed: inner expression is not ArrowFunctionExpression");
            return false;
        };

        eprintln!(
            "[CJS DEBUG] Found inner arrow, params: {}, expression: {}",
            inner_arrow.params.items.len(),
            inner_arrow.expression
        );

        if !inner_arrow.params.items.is_empty() || !inner_arrow.expression {
            eprintln!("[CJS DEBUG] Failed: inner params or expression check");
            return false;
        }

        eprintln!(
            "[CJS DEBUG] Inner body statements: {}",
            inner_arrow.body.statements.len()
        );

        let Some(inner_body) = inner_arrow.body.statements.first() else {
            eprintln!("[CJS DEBUG] Failed: no inner body statements");
            return false;
        };

        let Statement::ExpressionStatement(inner_expr) = inner_body else {
            eprintln!("[CJS DEBUG] Failed: inner body not expression statement");
            return false;
        };

        let inner_expr_unwrapped = match &inner_expr.expression {
            Expression::ParenthesizedExpression(paren) => &paren.expression,
            other => other,
        };

        if let Expression::SequenceExpression(seq) = inner_expr_unwrapped {
            if seq.expressions.len() == 2 {
                if let Expression::StaticMemberExpression(member) = &seq.expressions[1] {
                    if member.property.name.as_str() == "exports" {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Pattern: `(fn, res) => () => (fn && (res = fn(fn = 0)), res)`
    fn is_esm_pattern(arrow: &ArrowFunctionExpression<'_>) -> bool {
        if arrow.params.items.len() != 2 || !arrow.expression {
            return false;
        }

        let Some(body) = &arrow.body.statements.first() else {
            return false;
        };

        let Statement::ExpressionStatement(expr_stmt) = body else {
            return false;
        };

        let Expression::ArrowFunctionExpression(inner_arrow) = &expr_stmt.expression else {
            return false;
        };

        if !inner_arrow.params.items.is_empty() || !inner_arrow.expression {
            return false;
        }

        let Some(inner_body) = inner_arrow.body.statements.first() else {
            return false;
        };

        let Statement::ExpressionStatement(inner_expr) = inner_body else {
            return false;
        };

        let inner_expr_unwrapped = match &inner_expr.expression {
            Expression::ParenthesizedExpression(paren) => &paren.expression,
            other => other,
        };

        if let Expression::SequenceExpression(seq) = inner_expr_unwrapped {
            if seq.expressions.len() == 2 {
                if let Expression::LogicalExpression(logical) = &seq.expressions[0] {
                    if matches!(logical.operator, oxc_ast::ast::LogicalOperator::And) {
                        if let Expression::ParenthesizedExpression(paren) = &logical.right {
                            if let Expression::AssignmentExpression(_) = &paren.expression {
                                return true;
                            }
                        }
                        if let Expression::AssignmentExpression(_) = &logical.right {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Pattern: `for (name in all) __defProp(target, name, {get: all[name], enumerable: true})`
    fn is_export_pattern(arrow: &ArrowFunctionExpression<'_>) -> bool {
        if arrow.params.items.len() != 2 || arrow.expression {
            return false;
        }

        let Some(body) = &arrow.body.statements.first() else {
            return false;
        };

        matches!(body, Statement::ForInStatement(_))
    }

    fn is_toesm_pattern(arrow: &ArrowFunctionExpression<'_>) -> bool {
        if arrow.params.items.len() < 2 || arrow.params.items.len() > 3 || arrow.expression {
            return false;
        }

        let statements = &arrow.body.statements;
        if statements.len() < 3 {
            return false;
        }

        for stmt in statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(init) = &decl.init {
                        if let Expression::ConditionalExpression(_) = init {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    fn is_tocommonjs_pattern(arrow: &ArrowFunctionExpression<'_>) -> bool {
        if arrow.params.items.len() != 1 {
            return false;
        }

        if !arrow.expression {
            let statements = &arrow.body.statements;
            if statements.len() >= 2 {
                for stmt in statements {
                    if let Statement::IfStatement(if_stmt) = stmt {
                        if let Expression::AssignmentExpression(_) = &if_stmt.test {
                            return true;
                        }
                    }
                }
            }
            return false;
        }

        let Some(body) = &arrow.body.statements.first() else {
            return false;
        };

        let Statement::ExpressionStatement(expr_stmt) = body else {
            return false;
        };

        if let Expression::CallExpression(call) = &expr_stmt.expression {
            for arg in &call.arguments {
                if let Some(expr) = arg.as_expression() {
                    if let Expression::CallExpression(inner_call) = expr {
                        if inner_call.arguments.len() >= 2 {
                            if let Some(arg1) = inner_call.arguments.get(1) {
                                if let Some(Expression::StringLiteral(s)) = arg1.as_expression() {
                                    if s.value.as_str() == "__esModule" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Detect identity function pattern: `(q) => q`
    fn is_identity_pattern(arrow: &ArrowFunctionExpression<'_>) -> bool {
        if arrow.params.items.len() != 1 || !arrow.expression {
            return false;
        }

        let param_name = match arrow.params.items.first() {
            Some(param) => match &param.pattern {
                BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
                _ => return false,
            },
            None => return false,
        };

        let Some(body) = arrow.body.statements.first() else {
            return false;
        };

        let Statement::ExpressionStatement(expr_stmt) = body else {
            return false;
        };

        if let Expression::Identifier(ident) = &expr_stmt.expression {
            return ident.name.as_str() == param_name;
        }

        false
    }

    /// Detect getter bind pattern: `function(q) { return this[q] }`
    fn is_getter_bind_function(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else {
            return false;
        };

        if func.params.items.len() != 1 || body.statements.len() != 1 {
            return false;
        }

        let param_name = match func.params.items.first() {
            Some(param) => match &param.pattern {
                BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
                _ => return false,
            },
            None => return false,
        };

        let Some(stmt) = body.statements.first() else {
            return false;
        };

        let Statement::ReturnStatement(ret) = stmt else {
            return false;
        };

        let Some(arg) = &ret.argument else {
            return false;
        };

        if let Expression::ComputedMemberExpression(member) = arg {
            if let Expression::ThisExpression(_) = &member.object {
                if let Expression::Identifier(ident) = &member.expression {
                    return ident.name.as_str() == param_name;
                }
            }
        }

        false
    }

    /// Detect setter bind pattern: `function(q, K) { this[q] = ... }`
    fn is_setter_bind_function(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else {
            return false;
        };

        if func.params.items.len() != 2 || body.statements.is_empty() {
            return false;
        }

        let param_name = match func.params.items.first() {
            Some(param) => match &param.pattern {
                BindingPattern::BindingIdentifier(ident) => ident.name.as_str(),
                _ => return false,
            },
            None => return false,
        };

        let Some(stmt) = body.statements.first() else {
            return false;
        };

        let Statement::ExpressionStatement(expr_stmt) = stmt else {
            return false;
        };

        if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
            if let oxc_ast::ast::AssignmentTarget::ComputedMemberExpression(member) = &assign.left {
                if let Expression::ThisExpression(_) = &member.object {
                    if let Expression::Identifier(ident) = &member.expression {
                        return ident.name.as_str() == param_name;
                    }
                }
            }
        }

        false
    }

    /// Detect Anthropic's modified __export with configurable:true and set: accessor
    fn has_anthropic_export_modifications(arrow: &ArrowFunctionExpression<'_>) -> bool {
        if !Self::is_export_pattern(arrow) {
            return false;
        }

        let Some(body) = arrow.body.statements.first() else {
            return false;
        };

        let Statement::ForInStatement(for_in) = body else {
            return false;
        };

        let Statement::ExpressionStatement(expr_stmt) = &for_in.body else {
            return false;
        };

        let Expression::CallExpression(call) = &expr_stmt.expression else {
            return false;
        };

        if call.arguments.len() < 3 {
            return false;
        }

        let Some(obj_arg) = call.arguments.get(2) else {
            return false;
        };

        let Some(Expression::ObjectExpression(obj)) = obj_arg.as_expression() else {
            return false;
        };

        let mut has_configurable = false;
        let mut has_set = false;

        for prop in &obj.properties {
            if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key {
                    match key.name.as_str() {
                        "configurable" => {
                            if let Expression::BooleanLiteral(b) = &p.value {
                                has_configurable = b.value;
                            }
                        }
                        "set" => has_set = true,
                        _ => {}
                    }
                }
            }
        }

        has_configurable && has_set
    }

    /// `__importDefault`: `(mod && mod.__esModule) ? mod : { "default": mod }`
    fn is_import_default_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 1 || body.statements.len() != 1 {
            return false;
        }
        let Some(Statement::ReturnStatement(ret)) = body.statements.first() else {
            return false;
        };
        let Some(ret_arg) = &ret.argument else { return false };

        let cond = match ret_arg {
            Expression::ConditionalExpression(c) => c,
            Expression::ParenthesizedExpression(p) => {
                if let Expression::ConditionalExpression(c) = &p.expression {
                    c
                } else {
                    return false;
                }
            }
            _ => return false,
        };

        let test = match &cond.test {
            Expression::ParenthesizedExpression(p) => &p.expression,
            other => other,
        };

        if let Expression::LogicalExpression(logical) = test {
            if let Expression::StaticMemberExpression(member) = &logical.right {
                if member.property.name.as_str() == "__esModule" {
                    return true;
                }
            }
        }
        false
    }

    /// `__importStar`: checks `mod.__esModule`, calls `__setModuleDefault`
    fn is_import_star_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 1 || body.statements.len() < 2 {
            return false;
        }

        let mut has_esmodule_check = false;

        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::LogicalExpression(logical) = &if_stmt.test {
                    if let Expression::StaticMemberExpression(member) = &logical.right {
                        if member.property.name.as_str() == "__esModule" {
                            has_esmodule_check = true;
                        }
                    }
                }
            }
        }

        fn check_for_set_module_default(stmts: &[Statement<'_>]) -> bool {
            for stmt in stmts {
                if let Statement::ExpressionStatement(expr_stmt) = stmt {
                    if let Expression::CallExpression(call) = &expr_stmt.expression {
                        if let Expression::Identifier(ident) = &call.callee {
                            if ident.name.as_str().contains("setModuleDefault")
                                || ident.name.as_str().contains("SetModuleDefault")
                            {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        }

        let has_set_module_default = check_for_set_module_default(&body.statements);
        has_esmodule_check && has_set_module_default
    }

    /// `__createBinding`: `Object.defineProperty(o, k2, { enumerable: true, get: ... })`
    fn is_create_binding_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 3 {
            return false;
        }

        for stmt in &body.statements {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::CallExpression(call) = &expr_stmt.expression {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Object" && member.property.name.as_str() == "defineProperty" {
                                if call.arguments.len() >= 3 {
                                    if let Some(arg) = call.arguments.get(2) {
                                        if let Some(Expression::ObjectExpression(obj_expr)) = arg.as_expression() {
                                            for prop in &obj_expr.properties {
                                                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                                                    if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key {
                                                        if key.name.as_str() == "enumerable" {
                                                            return true;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__exportStar`: `for (var p in m) if (p !== "default") __createBinding(...)`
    fn is_export_star_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }

        for stmt in &body.statements {
            if let Statement::ForInStatement(for_in) = stmt {
                if let Statement::IfStatement(if_stmt) = &for_in.body {
                    if let Expression::BinaryExpression(bin) = &if_stmt.test {
                        if let Expression::StringLiteral(s) = &bin.right {
                            if s.value.as_str() == "default" {
                                return true;
                            }
                        }
                    }
                    if let Expression::LogicalExpression(logical) = &if_stmt.test {
                        if let Expression::BinaryExpression(bin) = &logical.left {
                            if let Expression::StringLiteral(s) = &bin.right {
                                if s.value.as_str() == "default" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__awaiter`: `new (P || (P = Promise))(...step(generator.next(value))...)`
    fn is_awaiter_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 3 {
            return false;
        }

        fn contains_promise_pattern(expr: &Expression<'_>) -> bool {
            match expr {
                Expression::NewExpression(new_expr) => {
                    // Unwrap parenthesized expression if present: new ($ || ($ = Promise))(...)
                    let callee = match &new_expr.callee {
                        Expression::ParenthesizedExpression(paren) => &paren.expression,
                        other => other,
                    };
                    if let Expression::LogicalExpression(logical) = callee {
                        // Unwrap parenthesized: ($ = Promise)
                        let right = match &logical.right {
                            Expression::ParenthesizedExpression(paren) => &paren.expression,
                            other => other,
                        };
                        if let Expression::AssignmentExpression(assign) = right {
                            if let Expression::Identifier(ident) = &assign.right {
                                if ident.name.as_str() == "Promise" {
                                    return true;
                                }
                            }
                        }
                        if let Expression::Identifier(ident) = &logical.left {
                            if ident.name.as_str() == "Promise" {
                                return true;
                            }
                        }
                    }
                    if let Expression::Identifier(ident) = callee {
                        if ident.name.as_str() == "Promise" {
                            return true;
                        }
                    }
                    false
                }
                _ => false,
            }
        }

        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(expr) = &ret.argument {
                    if contains_promise_pattern(expr) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `__generator`: `{ label: 0, sent: function() {}, trys: [], ops: [] }`
    fn is_generator_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }

        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::ObjectExpression(obj)) = &ret.argument {
                    let mut has_label = false;
                    let mut has_ops = false;

                    for prop in &obj.properties {
                        if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                            if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key {
                                match key.name.as_str() {
                                    "label" => has_label = true,
                                    "ops" => has_ops = true,
                                    _ => {}
                                }
                            }
                        }
                    }

                    if has_label && has_ops {
                        return true;
                    }
                }
            }
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(Expression::ObjectExpression(obj)) = &decl.init {
                        let mut has_label = false;
                        let mut has_ops = false;

                        for prop in &obj.properties {
                            if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                                if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key {
                                    match key.name.as_str() {
                                        "label" => has_label = true,
                                        "ops" => has_ops = true,
                                        _ => {}
                                    }
                                }
                            }
                        }

                        if has_label && has_ops {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__asyncGenerator`: `Symbol.asyncIterator` with queue management
    fn is_async_generator_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };

        fn check_for_async_iterator(stmts: &[Statement<'_>]) -> bool {
            for stmt in stmts {
                if let Statement::ExpressionStatement(expr_stmt) = stmt {
                    if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                        if let oxc_ast::ast::AssignmentTarget::ComputedMemberExpression(member) = &assign.left {
                            if let Expression::StaticMemberExpression(sym_member) = &member.expression {
                                if let Expression::Identifier(ident) = &sym_member.object {
                                    if ident.name.as_str() == "Symbol"
                                        && sym_member.property.name.as_str() == "asyncIterator"
                                    {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            false
        }

        check_for_async_iterator(&body.statements)
    }

    /// `__privateGet`: `if (kind === "a" && !f) throw new TypeError(...)`
    fn is_private_get_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 3 {
            return false;
        }

        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Statement::ThrowStatement(throw_stmt) = &if_stmt.consequent {
                    if let Expression::NewExpression(new_expr) = &throw_stmt.argument {
                        if let Expression::Identifier(ident) = &new_expr.callee {
                            if ident.name.as_str() == "TypeError" {
                                if let Expression::LogicalExpression(logical) = &if_stmt.test {
                                    if let Expression::BinaryExpression(bin) = &logical.left {
                                        if let Expression::StringLiteral(s) = &bin.right {
                                            if s.value.as_str() == "a" || s.value.as_str() == "m" {
                                                return true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__privateSet`: `if (kind === "m") throw new TypeError("Private method is not writable")`
    fn is_private_set_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 4 {
            return false;
        }

        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::BinaryExpression(bin) = &if_stmt.test {
                    if let Expression::StringLiteral(s) = &bin.right {
                        if s.value.as_str() == "m" {
                            if let Statement::ThrowStatement(throw_stmt) = &if_stmt.consequent {
                                if let Expression::NewExpression(new_expr) = &throw_stmt.argument {
                                    if let Expression::Identifier(ident) = &new_expr.callee {
                                        if ident.name.as_str() == "TypeError" {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__privateAdd`: `if (state.has(obj)) throw new TypeError(...); state.add(obj)`
    fn is_private_add_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }

        let mut has_state_has_check = false;
        let mut has_state_add_or_set = false;

        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::CallExpression(call) = &if_stmt.test {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if member.property.name.as_str() == "has" {
                            has_state_has_check = true;
                        }
                    }
                }
            }
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::CallExpression(call) = &expr_stmt.expression {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if member.property.name.as_str() == "add" || member.property.name.as_str() == "set" {
                            has_state_add_or_set = true;
                        }
                    }
                }
            }
        }

        has_state_has_check && has_state_add_or_set
    }

    /// `__privateIn`: `typeof state === "function" ? receiver === state : state.has(receiver)`
    fn is_private_in_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 || body.statements.len() < 1 {
            return false;
        }

        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::BinaryExpression(bin) = &if_stmt.test {
                    if let Expression::UnaryExpression(unary) = &bin.left {
                        if unary.operator == oxc_ast::ast::UnaryOperator::Typeof {
                            return true;
                        }
                    }
                }
            }
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::ConditionalExpression(cond)) = &ret.argument {
                    if let Expression::BinaryExpression(bin) = &cond.test {
                        if let Expression::UnaryExpression(unary) = &bin.left {
                            if unary.operator == oxc_ast::ast::UnaryOperator::Typeof {
                                if let Expression::StringLiteral(s) = &bin.right {
                                    if s.value.as_str() == "function" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__addDisposableResource`: checks for `Symbol.dispose` or `Symbol.for("Symbol.dispose")`
    fn is_add_disposable_resource_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }

        fn contains_symbol_dispose(expr: &Expression<'_>) -> bool {
            match expr {
                Expression::StaticMemberExpression(member) => {
                    if let Expression::Identifier(obj) = &member.object {
                        if obj.name.as_str() == "Symbol"
                            && (member.property.name.as_str() == "dispose"
                                || member.property.name.as_str() == "asyncDispose")
                        {
                            return true;
                        }
                    }
                    false
                }
                Expression::CallExpression(call) => {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Symbol" && member.property.name.as_str() == "for" {
                                if let Some(arg) = call.arguments.first() {
                                    if let Some(Expression::StringLiteral(s)) = arg.as_expression() {
                                        if s.value.as_str().contains("dispose") {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    false
                }
                _ => false,
            }
        }

        for stmt in &body.statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(init) = &decl.init {
                        if contains_symbol_dispose(init) {
                            return true;
                        }
                        if let Expression::LogicalExpression(logical) = init {
                            if contains_symbol_dispose(&logical.left) || contains_symbol_dispose(&logical.right) {
                                return true;
                            }
                        }
                    }
                }
            }
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::Identifier(ident) = &if_stmt.test {
                    if ident.name.as_str().contains("async") {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `__disposeResources`: `SuppressedError` constructor, resource stack
    fn is_dispose_resources_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }

        for stmt in &body.statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(Expression::ConditionalExpression(cond)) = &decl.init {
                        if let Expression::BinaryExpression(bin) = &cond.test {
                            if let Expression::UnaryExpression(unary) = &bin.left {
                                if unary.operator == oxc_ast::ast::UnaryOperator::Typeof {
                                    if let Expression::Identifier(ident) = &unary.argument {
                                        if ident.name.as_str() == "SuppressedError" {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__extends`: `extendStatics(d, b); d.prototype = Object.create(b.prototype)`
    fn is_extends_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                    if let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left {
                        if member.property.name.as_str() == "prototype" {
                            if let Expression::CallExpression(call) = &assign.right {
                                if let Expression::StaticMemberExpression(callee) = &call.callee {
                                    if let Expression::Identifier(obj) = &callee.object {
                                        if obj.name.as_str() == "Object" && callee.property.name.as_str() == "create" {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__assign`: `Object.assign || function(t) { for (var p in s) ... }`
    fn is_assign_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.is_empty() {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForInStatement(_) = stmt {
                return true;
            }
        }
        false
    }

    /// `__rest`: `{ a, ...rest } = obj` rest extraction
    fn is_rest_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForInStatement(for_in) = stmt {
                if let Statement::IfStatement(if_stmt) = &for_in.body {
                    if let Expression::CallExpression(call) = &if_stmt.test {
                        if let Expression::StaticMemberExpression(member) = &call.callee {
                            if member.property.name.as_str() == "indexOf" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__spread` / `__spreadArrays` / `__spreadArray`: spread helper patterns
    fn is_spread_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::ForStatement(for_stmt) = stmt {
                if matches!(&for_stmt.body, Statement::ForStatement(_)) {
                    return true;
                }
            }
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if member.property.name.as_str() == "concat" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__values`: `Symbol.iterator` iteration
    fn is_values_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 1 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(Expression::LogicalExpression(logical)) = &decl.init {
                        if let Expression::ComputedMemberExpression(member) = &logical.right {
                            if let Expression::StaticMemberExpression(sym) = &member.expression {
                                if let Expression::Identifier(ident) = &sym.object {
                                    if ident.name.as_str() == "Symbol" && sym.property.name.as_str() == "iterator" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__read`: `while ((n === void 0 || n-- > 0) && !(r = i.next()).done) ar.push(r.value)`
    fn is_read_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 1 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::WhileStatement(while_stmt) = stmt {
                if let Expression::LogicalExpression(_) = &while_stmt.test {
                    return true;
                }
            }
            if let Statement::TryStatement(_) = stmt {
                return true;
            }
        }
        false
    }

    /// `__decorate`: `decorators.reduceRight((o, d) => d(o), target)`
    fn is_decorate_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForStatement(for_stmt) = stmt {
                if let Statement::ExpressionStatement(expr_stmt) = &for_stmt.body {
                    if let Expression::AssignmentExpression(_) = &expr_stmt.expression {
                        return true;
                    }
                }
            }
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(init) = &decl.init {
                        if let Expression::ConditionalExpression(cond) = init {
                            if let Expression::BinaryExpression(bin) = &cond.test {
                                if let Expression::StaticMemberExpression(member) = &bin.left {
                                    if member.property.name.as_str() == "length" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__param`: `function(paramIndex, decorator) { return (target, key) => decorator(...) }`
    fn is_param_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 || body.statements.len() != 1 {
            return false;
        }
        if let Some(Statement::ReturnStatement(ret)) = body.statements.first() {
            if let Some(Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)) = &ret.argument {
                return true;
            }
        }
        false
    }

    /// `__metadata`: `Reflect.metadata(metadataKey, metadataValue)`
    fn is_metadata_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::LogicalExpression(logical) = &if_stmt.test {
                    if let Expression::BinaryExpression(bin) = &logical.left {
                        if let Expression::UnaryExpression(unary) = &bin.left {
                            if unary.operator == oxc_ast::ast::UnaryOperator::Typeof {
                                if let Expression::Identifier(ident) = &unary.argument {
                                    if ident.name.as_str() == "Reflect" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(ident) = &member.object {
                            if ident.name.as_str() == "Reflect" && member.property.name.as_str() == "metadata" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__template`: `Object.defineProperty(cooked, "raw", { value: raw })`
    fn is_template_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Statement::BlockStatement(block) = &if_stmt.consequent {
                    for inner in &block.body {
                        if let Statement::ExpressionStatement(expr_stmt) = inner {
                            if let Expression::CallExpression(call) = &expr_stmt.expression {
                                if let Expression::StaticMemberExpression(member) = &call.callee {
                                    if let Expression::Identifier(obj) = &member.object {
                                        if obj.name.as_str() == "Object"
                                            && member.property.name.as_str() == "defineProperty"
                                        {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                    if let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left {
                        if member.property.name.as_str() == "raw" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__name`: `Object.defineProperty(f, "name", { value: name, configurable: true })`
    fn is_name_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                let arg = match &ret.argument {
                    Some(Expression::ParenthesizedExpression(paren)) => Some(&paren.expression),
                    other => other.as_ref(),
                };
                if let Some(Expression::SequenceExpression(seq)) = arg {
                    for expr in &seq.expressions {
                        if let Expression::CallExpression(call) = expr {
                            if let Expression::StaticMemberExpression(member) = &call.callee {
                                if let Expression::Identifier(obj) = &member.object {
                                    if obj.name.as_str() == "Object"
                                        && member.property.name.as_str() == "defineProperty"
                                    {
                                        if call.arguments.len() >= 2 {
                                            if let Some(arg) = call.arguments.get(1) {
                                                if let Some(Expression::StringLiteral(s)) = arg.as_expression() {
                                                    if s.value.as_str() == "name" {
                                                        return true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__superGet` / `__superSet`: `Reflect.get/set(target, prop, receiver)`
    fn is_super_get_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Reflect" && member.property.name.as_str() == "get" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn is_super_set_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 3 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Reflect" && member.property.name.as_str() == "set" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__pow`: `Math.pow(base, exp)`
    fn is_pow_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Math" && member.property.name.as_str() == "pow" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__propKey`: `typeof x === "symbol" ? x : "".concat(x)`
    fn is_prop_key_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 1 || body.statements.len() != 1 {
            return false;
        }
        if let Some(Statement::ReturnStatement(ret)) = body.statements.first() {
            if let Some(Expression::ConditionalExpression(cond)) = &ret.argument {
                if let Expression::BinaryExpression(bin) = &cond.test {
                    if let Expression::UnaryExpression(unary) = &bin.left {
                        if unary.operator == oxc_ast::ast::UnaryOperator::Typeof {
                            if let Expression::StringLiteral(s) = &bin.right {
                                if s.value.as_str() == "symbol" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__toBinary` / `__toBinaryNode`: Base64 to Uint8Array conversion
    fn is_to_binary_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(Expression::CallExpression(call)) = &decl.init {
                        if let Expression::Identifier(ident) = &call.callee {
                            if ident.name.as_str() == "atob" {
                                return true;
                            }
                        }
                        if let Expression::StaticMemberExpression(member) = &call.callee {
                            if let Expression::Identifier(obj) = &member.object {
                                if obj.name.as_str() == "Buffer" && member.property.name.as_str() == "from" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__earlyAccess`: `throw new ReferenceError("...")`
    fn is_early_access_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::ThrowStatement(throw_stmt) = stmt {
                if let Expression::NewExpression(new_expr) = &throw_stmt.argument {
                    if let Expression::Identifier(ident) = &new_expr.callee {
                        if ident.name.as_str() == "ReferenceError" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__async`: `Promise.resolve().then(...)` async lowering (esbuild)
    fn is_async_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if member.property.name.as_str() == "then" {
                            if let Expression::CallExpression(inner) = &member.object {
                                if let Expression::StaticMemberExpression(inner_member) = &inner.callee {
                                    if let Expression::Identifier(obj) = &inner_member.object {
                                        if obj.name.as_str() == "Promise"
                                            && inner_member.property.name.as_str() == "resolve"
                                        {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__privateMethod`: Returns the private method function
    fn is_private_method_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::Identifier(_)) = &ret.argument {
                    if body.statements.len() == 1 {
                        return false;
                    }
                }
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if member.property.name.as_str() == "call" || member.property.name.as_str() == "bind" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__publicField`: `Object.defineProperty(obj, key, { value, ... })`
    fn is_public_field_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::SequenceExpression(seq) = &expr_stmt.expression {
                    for expr in &seq.expressions {
                        if let Expression::CallExpression(call) = expr {
                            if let Expression::StaticMemberExpression(member) = &call.callee {
                                if let Expression::Identifier(obj) = &member.object {
                                    if obj.name.as_str() == "Object"
                                        && member.property.name.as_str() == "defineProperty"
                                    {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
                if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                    if let oxc_ast::ast::AssignmentTarget::ComputedMemberExpression(_) = &assign.left {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `__privateWrapper`: Private field getter/setter wrapper
    fn is_private_wrapper_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        let mut has_get = false;
        let mut has_set = false;
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::ObjectExpression(obj)) = &ret.argument {
                    for prop in &obj.properties {
                        if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                            if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key {
                                match key.name.as_str() {
                                    "get" => has_get = true,
                                    "set" => has_set = true,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
        has_get || has_set
    }

    /// `__require`: Fallback require with Proxy
    fn is_require_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::NewExpression(new_expr)) = &ret.argument {
                    if let Expression::Identifier(ident) = &new_expr.callee {
                        if ident.name.as_str() == "Proxy" {
                            return true;
                        }
                    }
                }
            }
            if let Statement::TryStatement(_) = stmt {
                return true;
            }
        }
        false
    }

    /// `__glob`: `import.meta.glob` handler
    fn is_glob_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(Expression::ObjectExpression(_)) = &decl.init {
                        if let BindingPattern::BindingIdentifier(ident) = &decl.id {
                            if ident.name.as_str().contains("glob") || ident.name.as_str().contains("modules") {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__restKey`: Rest key normalization
    fn is_rest_key_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 1 || body.statements.len() != 1 {
            return false;
        }
        if let Some(Statement::ReturnStatement(ret)) = body.statements.first() {
            if let Some(Expression::BinaryExpression(bin)) = &ret.argument {
                if bin.operator == oxc_ast::ast::BinaryOperator::Addition {
                    if let Expression::StringLiteral(_) = &bin.left {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `__objRest`: Object rest extraction
    fn is_obj_rest_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForInStatement(for_in) = stmt {
                if let Statement::IfStatement(if_stmt) = &for_in.body {
                    if let Expression::UnaryExpression(unary) = &if_stmt.test {
                        if unary.operator == oxc_ast::ast::UnaryOperator::LogicalNot {
                            if let Expression::CallExpression(call) = &unary.argument {
                                if let Expression::StaticMemberExpression(member) = &call.callee {
                                    if member.property.name.as_str() == "includes"
                                        || member.property.name.as_str() == "indexOf"
                                    {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__spreadValues`: Spread with symbol support
    fn is_spread_values_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForInStatement(_) = stmt {
                for inner in &body.statements {
                    if let Statement::IfStatement(if_stmt) = inner {
                        if let Expression::Identifier(ident) = &if_stmt.test {
                            if ident.name.as_str().contains("getOwnPropertySymbols")
                                || ident.name.as_str().contains("Symbols")
                            {
                                return true;
                            }
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    /// `__spreadProps`: Spread via defineProperties
    fn is_spread_props_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Object" && member.property.name.as_str() == "defineProperties" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__await`: Await marker for async generators
    fn is_await_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 1 || body.statements.len() > 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::ConditionalExpression(cond)) = &ret.argument {
                    if let Expression::BinaryExpression(bin) = &cond.test {
                        if bin.operator == oxc_ast::ast::BinaryOperator::Instanceof {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__asyncDelegator`: yield* delegation wrapper
    fn is_async_delegator_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 1 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(Expression::ObjectExpression(obj)) = &decl.init {
                        let mut has_next = false;
                        let mut has_throw = false;
                        for prop in &obj.properties {
                            if let oxc_ast::ast::ObjectPropertyKind::SpreadProperty(_) = prop {
                                continue;
                            }
                            if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                                if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key {
                                    match key.name.as_str() {
                                        "next" => has_next = true,
                                        "throw" => has_throw = true,
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if has_next && has_throw {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__asyncValues`: Symbol.asyncIterator iteration
    fn is_async_values_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::UnaryExpression(unary) = &if_stmt.test {
                    if unary.operator == oxc_ast::ast::UnaryOperator::LogicalNot {
                        if let Expression::StaticMemberExpression(member) = &unary.argument {
                            if let Expression::Identifier(obj) = &member.object {
                                if obj.name.as_str() == "Symbol" && member.property.name.as_str() == "asyncIterator" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(Expression::CallExpression(call)) = &decl.init {
                        if let Expression::ComputedMemberExpression(member) = &call.callee {
                            if let Expression::StaticMemberExpression(sym) = &member.expression {
                                if let Expression::Identifier(obj) = &sym.object {
                                    if obj.name.as_str() == "Symbol" && sym.property.name.as_str() == "asyncIterator" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__yieldStar`: yield* in async generators
    fn is_yield_star_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let BindingPattern::BindingIdentifier(ident) = &decl.id {
                        if ident.name.as_str() == "inner" || ident.name.as_str() == "e" {
                            if let Some(Expression::CallExpression(_)) = &decl.init {
                                return true;
                            }
                        }
                    }
                }
            }
            if let Statement::TryStatement(try_stmt) = stmt {
                if let Some(handler) = &try_stmt.handler {
                    if !handler.body.body.is_empty() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `__forAwait`: for-await loop lowering
    fn is_for_await_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::TryStatement(try_stmt) = stmt {
                for inner in &try_stmt.block.body {
                    if let Statement::WhileStatement(while_stmt) = inner {
                        if let Expression::UnaryExpression(unary) = &while_stmt.test {
                            if unary.operator == oxc_ast::ast::UnaryOperator::LogicalNot {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__superWrapper`: Combined super getter/setter
    fn is_super_wrapper_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::ObjectExpression(obj)) = &ret.argument {
                    let mut has_get = false;
                    let mut has_set = false;
                    for prop in &obj.properties {
                        if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                            if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key {
                                match key.name.as_str() {
                                    "get" | "_ " => has_get = true,
                                    "set" => has_set = true,
                                    _ => {}
                                }
                            }
                        }
                    }
                    if has_get && has_set {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `__makeTemplateObject`: tslib template object creation
    fn is_make_template_object_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::StaticMemberExpression(member) = &if_stmt.test {
                    if let Expression::Identifier(obj) = &member.object {
                        if obj.name.as_str() == "Object" {
                            return true;
                        }
                    }
                }
            }
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                    if let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left {
                        if member.property.name.as_str() == "raw" {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__setFunctionName`: Set function name property
    fn is_set_function_name_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::IfStatement(if_stmt) = stmt {
                if let Expression::BinaryExpression(bin) = &if_stmt.test {
                    if let Expression::UnaryExpression(unary) = &bin.left {
                        if unary.operator == oxc_ast::ast::UnaryOperator::Typeof {
                            if let Expression::StringLiteral(s) = &bin.right {
                                if s.value.as_str() == "symbol" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::CallExpression(call)) = &ret.argument {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Object" && member.property.name.as_str() == "defineProperty" {
                                if call.arguments.len() >= 2 {
                                    if let Some(arg) = call.arguments.get(1) {
                                        if let Some(Expression::StringLiteral(s)) = arg.as_expression() {
                                            if s.value.as_str() == "name" {
                                                return true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__esDecorate`: ES decorators with context.addInitializer
    fn is_es_decorate_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 3 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for decl in &var_decl.declarations {
                    if let Some(Expression::ObjectExpression(obj)) = &decl.init {
                        for prop in &obj.properties {
                            if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                                if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key {
                                    if key.name.as_str() == "addInitializer" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__decorateClass`: esbuild class decorator application
    fn is_decorate_class_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForStatement(for_stmt) = stmt {
                if let Statement::BlockStatement(block) = &for_stmt.body {
                    for inner in &block.body {
                        if let Statement::VariableDeclaration(var_decl) = inner {
                            for decl in &var_decl.declarations {
                                if let BindingPattern::BindingIdentifier(ident) = &decl.id {
                                    if ident.name.as_str() == "desc" || ident.name.as_str() == "decorator" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__decorateParam`: esbuild parameter decorator wrapper
    fn is_decorate_param_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() != 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::FunctionExpression(inner_func)) = &ret.argument {
                    if let Some(inner_body) = &inner_func.body {
                        for inner_stmt in &inner_body.statements {
                            if let Statement::ExpressionStatement(expr_stmt) = inner_stmt {
                                if let Expression::CallExpression(_) = &expr_stmt.expression {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__decoratorStart`: Decorator metadata initialization
    fn is_decorator_start_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::ReturnStatement(ret) = stmt {
                if let Some(Expression::ArrayExpression(arr)) = &ret.argument {
                    if arr.elements.is_empty() || arr.elements.len() <= 3 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `__decoratorMetadata`: Attach metadata to decorated element
    fn is_decorator_metadata_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        for stmt in &body.statements {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::CallExpression(call) = &expr_stmt.expression {
                    if let Expression::StaticMemberExpression(member) = &call.callee {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Object" && member.property.name.as_str() == "defineProperty" {
                                if call.arguments.len() >= 2 {
                                    if let Some(arg) = call.arguments.get(1) {
                                        if let Some(Expression::StaticMemberExpression(sym_member)) =
                                            arg.as_expression()
                                        {
                                            if let Expression::Identifier(sym_obj) = &sym_member.object {
                                                if sym_obj.name.as_str() == "Symbol" {
                                                    return true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__runInitializers`: Execute decorator initializers
    fn is_run_initializers_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForStatement(for_stmt) = stmt {
                if let Statement::BlockStatement(block) = &for_stmt.body {
                    for inner in &block.body {
                        if let Statement::ExpressionStatement(expr_stmt) = inner {
                            if let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                                if let Expression::ConditionalExpression(_) = &assign.right {
                                    return true;
                                }
                                if let Expression::CallExpression(call) = &assign.right {
                                    if let Expression::StaticMemberExpression(member) = &call.callee {
                                        if member.property.name.as_str() == "call" {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// `__decorateElement`: JS decorator element processing
    fn is_decorate_element_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 3 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::SwitchStatement(switch_stmt) = stmt {
                if switch_stmt.cases.len() >= 3 {
                    return true;
                }
            }
        }
        false
    }

    /// `__copyProps`: Property copier for re-exports
    fn is_copy_props_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForStatement(for_stmt) = stmt {
                if let Statement::ExpressionStatement(expr_stmt) = &for_stmt.body {
                    if let Expression::LogicalExpression(logical) = &expr_stmt.expression {
                        if logical.operator == oxc_ast::ast::LogicalOperator::Or {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// `__reExport`: export * from handler
    fn is_re_export_pattern(func: &Function<'_>) -> bool {
        let Some(body) = &func.body else { return false };
        if func.params.items.len() < 2 {
            return false;
        }
        for stmt in &body.statements {
            if let Statement::ForInStatement(for_in) = stmt {
                if let Statement::ExpressionStatement(expr_stmt) = &for_in.body {
                    if let Expression::LogicalExpression(logical) = &expr_stmt.expression {
                        if logical.operator == oxc_ast::ast::LogicalOperator::Or {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn try_detect_helper(&mut self, var_name: &str, arrow: &ArrowFunctionExpression<'_>, line: u32) {
        if Self::is_identity_pattern(arrow) {
            eprintln!("[ESBUILD] Detected Anthropic identity helper: {}", var_name);
            self.support_helpers
                .insert(var_name.to_string(), SupportHelperKind::Identity);
            self.helpers.insert(
                var_name.to_string(),
                EsbuildHelperInfo::new(
                    var_name.to_string(),
                    EsbuildHelperKind::SupportHelper(SupportHelperKind::Identity),
                    line,
                )
                .with_modification(AnthropicModification::IdentityFunction),
            );
            return;
        }

        let kind = if Self::is_commonjs_pattern(arrow) {
            EsbuildHelperKind::CommonJs
        } else if Self::is_esm_pattern(arrow) {
            EsbuildHelperKind::Esm
        } else if Self::is_export_pattern(arrow) {
            EsbuildHelperKind::Export
        } else if Self::is_toesm_pattern(arrow) {
            EsbuildHelperKind::ToEsm
        } else if Self::is_tocommonjs_pattern(arrow) {
            EsbuildHelperKind::ToCommonJs
        } else {
            return;
        };

        let mut info = EsbuildHelperInfo::new(var_name.to_string(), kind, line);

        if matches!(kind, EsbuildHelperKind::Export) && Self::has_anthropic_export_modifications(arrow) {
            eprintln!("[ESBUILD] Detected Anthropic __export modification: {}", var_name);
            info = info.with_modification(AnthropicModification::ConfigurableExport);
        }

        eprintln!("[ESBUILD] Detected helper: {} = {:?}", var_name, kind);
        self.helpers.insert(var_name.to_string(), info);
    }

    fn try_detect_function_helper(&mut self, var_name: &str, func: &Function<'_>, line: u32) {
        let kind = if Self::is_getter_bind_function(func) {
            eprintln!("[ESBUILD] Detected Anthropic getter bind helper: {}", var_name);
            self.support_helpers
                .insert(var_name.to_string(), SupportHelperKind::GetterBind);
            Some((
                EsbuildHelperKind::SupportHelper(SupportHelperKind::GetterBind),
                Some(AnthropicModification::GetterBind),
            ))
        } else if Self::is_setter_bind_function(func) {
            eprintln!("[ESBUILD] Detected Anthropic setter bind helper: {}", var_name);
            self.support_helpers
                .insert(var_name.to_string(), SupportHelperKind::SetterBind);
            Some((
                EsbuildHelperKind::SupportHelper(SupportHelperKind::SetterBind),
                Some(AnthropicModification::SetterBind),
            ))
        } else if Self::is_import_default_pattern(func) {
            eprintln!("[ESBUILD] Detected __importDefault: {}", var_name);
            Some((EsbuildHelperKind::ImportDefault, None))
        } else if Self::is_import_star_pattern(func) {
            eprintln!("[ESBUILD] Detected __importStar: {}", var_name);
            Some((EsbuildHelperKind::ImportStar, None))
        } else if Self::is_create_binding_pattern(func) {
            eprintln!("[ESBUILD] Detected __createBinding: {}", var_name);
            Some((EsbuildHelperKind::CreateBinding, None))
        } else if Self::is_export_star_pattern(func) {
            eprintln!("[ESBUILD] Detected __exportStar: {}", var_name);
            Some((EsbuildHelperKind::ExportStar, None))
        } else if Self::is_awaiter_pattern(func) {
            eprintln!("[ESBUILD] Detected __awaiter: {}", var_name);
            Some((EsbuildHelperKind::Awaiter, None))
        } else if Self::is_generator_pattern(func) {
            eprintln!("[ESBUILD] Detected __generator: {}", var_name);
            Some((EsbuildHelperKind::Generator, None))
        } else if Self::is_async_generator_pattern(func) {
            eprintln!("[ESBUILD] Detected __asyncGenerator: {}", var_name);
            Some((EsbuildHelperKind::AsyncGenerator, None))
        } else if Self::is_private_get_pattern(func) {
            eprintln!("[ESBUILD] Detected __privateGet: {}", var_name);
            Some((EsbuildHelperKind::PrivateGet, None))
        } else if Self::is_private_set_pattern(func) {
            eprintln!("[ESBUILD] Detected __privateSet: {}", var_name);
            Some((EsbuildHelperKind::PrivateSet, None))
        } else if Self::is_private_add_pattern(func) {
            eprintln!("[ESBUILD] Detected __privateAdd: {}", var_name);
            Some((EsbuildHelperKind::PrivateAdd, None))
        } else if Self::is_private_in_pattern(func) {
            eprintln!("[ESBUILD] Detected __privateIn: {}", var_name);
            Some((EsbuildHelperKind::PrivateIn, None))
        } else if Self::is_add_disposable_resource_pattern(func) {
            eprintln!("[ESBUILD] Detected __addDisposableResource: {}", var_name);
            Some((EsbuildHelperKind::AddDisposableResource, None))
        } else if Self::is_dispose_resources_pattern(func) {
            eprintln!("[ESBUILD] Detected __disposeResources: {}", var_name);
            Some((EsbuildHelperKind::DisposeResources, None))
        } else if Self::is_extends_pattern(func) {
            eprintln!("[ESBUILD] Detected __extends: {}", var_name);
            Some((EsbuildHelperKind::Extends, None))
        } else if Self::is_rest_pattern(func) {
            eprintln!("[ESBUILD] Detected __rest: {}", var_name);
            Some((EsbuildHelperKind::Rest, None))
        } else if Self::is_assign_pattern(func) {
            eprintln!("[ESBUILD] Detected __assign: {}", var_name);
            Some((EsbuildHelperKind::Assign, None))
        } else if Self::is_spread_pattern(func) {
            eprintln!("[ESBUILD] Detected __spread/__spreadArray: {}", var_name);
            Some((EsbuildHelperKind::SpreadArray, None))
        } else if Self::is_values_pattern(func) {
            eprintln!("[ESBUILD] Detected __values: {}", var_name);
            Some((EsbuildHelperKind::Values, None))
        } else if Self::is_read_pattern(func) {
            eprintln!("[ESBUILD] Detected __read: {}", var_name);
            Some((EsbuildHelperKind::Read, None))
        } else if Self::is_decorate_class_pattern(func) {
            eprintln!("[ESBUILD] Detected __decorateClass: {}", var_name);
            Some((EsbuildHelperKind::DecorateClass, None))
        } else if Self::is_decorate_pattern(func) {
            eprintln!("[ESBUILD] Detected __decorate: {}", var_name);
            Some((EsbuildHelperKind::Decorate, None))
        } else if Self::is_param_pattern(func) {
            eprintln!("[ESBUILD] Detected __param: {}", var_name);
            Some((EsbuildHelperKind::Param, None))
        } else if Self::is_metadata_pattern(func) {
            eprintln!("[ESBUILD] Detected __metadata: {}", var_name);
            Some((EsbuildHelperKind::Metadata, None))
        } else if Self::is_template_pattern(func) {
            eprintln!("[ESBUILD] Detected __template/__makeTemplateObject: {}", var_name);
            Some((EsbuildHelperKind::Template, None))
        } else if Self::is_name_pattern(func) {
            eprintln!("[ESBUILD] Detected __name: {}", var_name);
            Some((EsbuildHelperKind::Name, None))
        } else if Self::is_super_get_pattern(func) {
            eprintln!("[ESBUILD] Detected __superGet: {}", var_name);
            Some((EsbuildHelperKind::SuperGet, None))
        } else if Self::is_super_set_pattern(func) {
            eprintln!("[ESBUILD] Detected __superSet: {}", var_name);
            Some((EsbuildHelperKind::SuperSet, None))
        } else if Self::is_pow_pattern(func) {
            eprintln!("[ESBUILD] Detected __pow: {}", var_name);
            Some((EsbuildHelperKind::Pow, None))
        } else if Self::is_prop_key_pattern(func) {
            eprintln!("[ESBUILD] Detected __propKey: {}", var_name);
            Some((EsbuildHelperKind::PropKey, None))
        } else if Self::is_to_binary_pattern(func) {
            eprintln!("[ESBUILD] Detected __toBinary: {}", var_name);
            Some((EsbuildHelperKind::ToBinary, None))
        } else if Self::is_early_access_pattern(func) {
            eprintln!("[ESBUILD] Detected __earlyAccess: {}", var_name);
            Some((EsbuildHelperKind::EarlyAccess, None))
        } else if Self::is_async_pattern(func) {
            eprintln!("[ESBUILD] Detected __async: {}", var_name);
            Some((EsbuildHelperKind::Async, None))
        } else if Self::is_private_method_pattern(func) {
            eprintln!("[ESBUILD] Detected __privateMethod: {}", var_name);
            Some((EsbuildHelperKind::PrivateMethod, None))
        } else if Self::is_public_field_pattern(func) {
            eprintln!("[ESBUILD] Detected __publicField: {}", var_name);
            Some((EsbuildHelperKind::PublicField, None))
        } else if Self::is_private_wrapper_pattern(func) {
            eprintln!("[ESBUILD] Detected __privateWrapper: {}", var_name);
            Some((EsbuildHelperKind::PrivateWrapper, None))
        } else if Self::is_require_pattern(func) {
            eprintln!("[ESBUILD] Detected __require: {}", var_name);
            Some((EsbuildHelperKind::Require, None))
        } else if Self::is_glob_pattern(func) {
            eprintln!("[ESBUILD] Detected __glob: {}", var_name);
            Some((EsbuildHelperKind::Glob, None))
        } else if Self::is_rest_key_pattern(func) {
            eprintln!("[ESBUILD] Detected __restKey: {}", var_name);
            Some((EsbuildHelperKind::RestKey, None))
        } else if Self::is_obj_rest_pattern(func) {
            eprintln!("[ESBUILD] Detected __objRest: {}", var_name);
            Some((EsbuildHelperKind::ObjRest, None))
        } else if Self::is_spread_values_pattern(func) {
            eprintln!("[ESBUILD] Detected __spreadValues: {}", var_name);
            Some((EsbuildHelperKind::SpreadValues, None))
        } else if Self::is_spread_props_pattern(func) {
            eprintln!("[ESBUILD] Detected __spreadProps: {}", var_name);
            Some((EsbuildHelperKind::SpreadProps, None))
        } else if Self::is_await_pattern(func) {
            eprintln!("[ESBUILD] Detected __await: {}", var_name);
            Some((EsbuildHelperKind::Await, None))
        } else if Self::is_async_delegator_pattern(func) {
            eprintln!("[ESBUILD] Detected __asyncDelegator: {}", var_name);
            Some((EsbuildHelperKind::AsyncDelegator, None))
        } else if Self::is_async_values_pattern(func) {
            eprintln!("[ESBUILD] Detected __asyncValues: {}", var_name);
            Some((EsbuildHelperKind::AsyncValues, None))
        } else if Self::is_yield_star_pattern(func) {
            eprintln!("[ESBUILD] Detected __yieldStar: {}", var_name);
            Some((EsbuildHelperKind::YieldStar, None))
        } else if Self::is_for_await_pattern(func) {
            eprintln!("[ESBUILD] Detected __forAwait: {}", var_name);
            Some((EsbuildHelperKind::ForAwait, None))
        } else if Self::is_super_wrapper_pattern(func) {
            eprintln!("[ESBUILD] Detected __superWrapper: {}", var_name);
            Some((EsbuildHelperKind::SuperWrapper, None))
        } else if Self::is_make_template_object_pattern(func) {
            eprintln!("[ESBUILD] Detected __makeTemplateObject: {}", var_name);
            Some((EsbuildHelperKind::MakeTemplateObject, None))
        } else if Self::is_set_function_name_pattern(func) {
            eprintln!("[ESBUILD] Detected __setFunctionName: {}", var_name);
            Some((EsbuildHelperKind::SetFunctionName, None))
        } else if Self::is_es_decorate_pattern(func) {
            eprintln!("[ESBUILD] Detected __esDecorate: {}", var_name);
            Some((EsbuildHelperKind::EsDecorate, None))
        } else if Self::is_decorate_param_pattern(func) {
            eprintln!("[ESBUILD] Detected __decorateParam: {}", var_name);
            Some((EsbuildHelperKind::DecorateParam, None))
        } else if Self::is_decorator_start_pattern(func) {
            eprintln!("[ESBUILD] Detected __decoratorStart: {}", var_name);
            Some((EsbuildHelperKind::DecoratorStart, None))
        } else if Self::is_decorator_metadata_pattern(func) {
            eprintln!("[ESBUILD] Detected __decoratorMetadata: {}", var_name);
            Some((EsbuildHelperKind::DecoratorMetadata, None))
        } else if Self::is_run_initializers_pattern(func) {
            eprintln!("[ESBUILD] Detected __runInitializers: {}", var_name);
            Some((EsbuildHelperKind::RunInitializers, None))
        } else if Self::is_decorate_element_pattern(func) {
            eprintln!("[ESBUILD] Detected __decorateElement: {}", var_name);
            Some((EsbuildHelperKind::DecorateElement, None))
        } else if Self::is_copy_props_pattern(func) {
            eprintln!("[ESBUILD] Detected __copyProps: {}", var_name);
            Some((EsbuildHelperKind::CopyProps, None))
        } else if Self::is_re_export_pattern(func) {
            eprintln!("[ESBUILD] Detected __reExport: {}", var_name);
            Some((EsbuildHelperKind::ReExport, None))
        } else {
            None
        };

        if let Some((helper_kind, modification)) = kind {
            let mut info = EsbuildHelperInfo::new(var_name.to_string(), helper_kind, line);
            if let Some(mod_kind) = modification {
                info = info.with_modification(mod_kind);
            }
            self.helpers.insert(var_name.to_string(), info);
        }
    }

    fn try_detect_module_wrapper(&mut self, declarator: &VariableDeclarator<'_>) {
        let Some(init) = &declarator.init else {
            return;
        };

        let var_name = match &declarator.id {
            BindingPattern::BindingIdentifier(ident) => ident.name.as_str().to_string(),
            _ => return,
        };

        let Expression::CallExpression(call) = init else {
            return;
        };

        let callee_name = match &call.callee {
            Expression::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };

        if let Some(helper) = self.helpers.get(callee_name) {
            if matches!(helper.kind, EsbuildHelperKind::CommonJs | EsbuildHelperKind::Esm) {
                eprintln!(
                    "[ESBUILD] Detected module wrapper: {} using {:?}",
                    var_name, helper.kind
                );
                self.module_wrappers.push(var_name);
            }
        }
    }
}

impl Default for EsbuildHelperCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for EsbuildHelperCollector {
    fn enter_program(&mut self, program: &mut Program<'a>, _ctx: &mut Ctx<'a>) {
        for stmt in &program.body {
            match stmt {
                Statement::VariableDeclaration(var_decl) => {
                    for declarator in &var_decl.declarations {
                        self.try_detect_object_alias(declarator);
                        self.try_detect_prototype_alias(declarator);

                        if let Some(init) = &declarator.init {
                            match init {
                                Expression::ArrowFunctionExpression(arrow) => {
                                    if let BindingPattern::BindingIdentifier(ident) = &declarator.id {
                                        let var_name = ident.name.as_str();
                                        self.try_detect_helper(var_name, arrow, declarator.span.start);
                                    }
                                }
                                Expression::FunctionExpression(func) => {
                                    if let BindingPattern::BindingIdentifier(ident) = &declarator.id {
                                        let var_name = ident.name.as_str();
                                        self.try_detect_function_helper(var_name, func, declarator.span.start);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Statement::FunctionDeclaration(func) => {
                    if let Some(id) = &func.id {
                        let var_name = id.name.as_str();
                        self.try_detect_function_helper(var_name, func, func.span.start);
                    }
                }
                _ => {}
            }
        }

        for stmt in &program.body {
            if let Statement::VariableDeclaration(var_decl) = stmt {
                for declarator in &var_decl.declarations {
                    self.try_detect_module_wrapper(declarator);
                }
            }
        }

        self.determine_bundle_type();

        eprintln!(
            "[ESBUILD] Summary: {} helpers, {} object aliases, {} module wrappers, {} support helpers, bundle type: {:?}",
            self.helpers.len(),
            self.object_aliases.len(),
            self.module_wrappers.len(),
            self.support_helpers.len(),
            self.bundle_type
        );
    }
}

pub fn annotate_esbuild_modules(code: &str, collector: &EsbuildHelperCollector) -> String {
    let helpers = collector.get_helpers();
    let module_wrappers = collector.get_module_wrappers();

    if helpers.is_empty() && module_wrappers.is_empty() {
        return code.to_string();
    }

    let commonjs_helpers: Vec<&str> = helpers
        .iter()
        .filter(|(_, info)| matches!(info.kind, EsbuildHelperKind::CommonJs))
        .map(|(name, _)| name.as_str())
        .collect();

    let esm_helpers: Vec<&str> = helpers
        .iter()
        .filter(|(_, info)| matches!(info.kind, EsbuildHelperKind::Esm))
        .map(|(name, _)| name.as_str())
        .collect();

    let mut result = String::with_capacity(code.len() + 8192);

    // Extract hashbang if present at start — must stay first for valid JS
    let (hashbang, code_after_hashbang) = if code.starts_with("#!") {
        match code.find('\n') {
            Some(newline_pos) => {
                let hashbang_line = &code[..=newline_pos];
                let rest = &code[newline_pos + 1..];
                (Some(hashbang_line), rest)
            }
            None => (Some(code), ""),
        }
    } else {
        (None, code)
    };

    // Hashbang MUST come first (ECMAScript spec requirement)
    if let Some(hashbang_line) = hashbang {
        result.push_str(hashbang_line);
    }

    // Header goes after hashbang
    result.push_str(&generate_helper_header(collector));

    let mut count = 0u32;

    for line in code_after_hashbang.lines() {
        let trimmed = line.trim_start();

        for helper in &commonjs_helpers {
            let pattern = format!(" = {helper}((");
            if let Some(rest) = trimmed.strip_prefix("var ") {
                if let Some(name_end) = rest.find(&pattern) {
                    let module_name = &rest[..name_end];
                    if !module_name.is_empty()
                        && module_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                    {
                        result.push_str("// ═══════════════════════════════════════\n");
                        result.push_str("// esbuild CommonJS Module: ");
                        result.push_str(module_name);
                        result.push('\n');
                        result.push_str("// ═══════════════════════════════════════\n");
                        count = count.wrapping_add(1);
                    }
                }
            }
        }

        for helper in &esm_helpers {
            let pattern = format!(" = {helper}({{");
            let pattern_alt = format!(" = {helper}((");
            if let Some(rest) = trimmed.strip_prefix("var ") {
                let found = rest.find(&pattern).or_else(|| rest.find(&pattern_alt));
                if let Some(name_end) = found {
                    let module_name = &rest[..name_end];
                    if !module_name.is_empty()
                        && module_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                    {
                        result.push_str("// ═══════════════════════════════════════\n");
                        result.push_str("// esbuild ESM Module: ");
                        result.push_str(module_name);
                        result.push('\n');
                        result.push_str("// ═══════════════════════════════════════\n");
                        count = count.wrapping_add(1);
                    }
                }
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    if count > 0 {
        eprintln!("[ESBUILD] Annotated {count} esbuild modules");
    }

    result
}

fn generate_helper_header(collector: &EsbuildHelperCollector) -> String {
    let helpers = collector.get_helpers();
    let bundle_type = collector.get_bundle_type();
    let support_helpers = collector.get_support_helpers();

    if helpers.is_empty() {
        return String::new();
    }

    let mut header = String::with_capacity(2048);

    header.push_str("// ╔═══════════════════════════════════════════════════════════════════╗\n");
    header.push_str("// ║                    ESBUILD BUNDLE ANALYSIS                        ║\n");
    header.push_str("// ╠═══════════════════════════════════════════════════════════════════╣\n");

    match bundle_type {
        BundleType::Anthropic => {
            header.push_str("// ║  Bundle Type: ANTHROPIC-MODIFIED ESBUILD                         ║\n");
            header.push_str("// ║  (Claude Code / Anthropic CLI build system)                     ║\n");
        }
        BundleType::Standard => {
            header.push_str("// ║  Bundle Type: STANDARD ESBUILD                                   ║\n");
        }
        BundleType::Unknown => {
            header.push_str("// ║  Bundle Type: UNKNOWN                                            ║\n");
        }
    }

    header.push_str("// ╠═══════════════════════════════════════════════════════════════════╣\n");
    header.push_str("// ║  Helper Mapping:                                                  ║\n");

    for (name, info) in helpers {
        let semantic_name = match info.kind {
            EsbuildHelperKind::CommonJs => "__commonJS",
            EsbuildHelperKind::Esm => "__esm",
            EsbuildHelperKind::Export => "__export",
            EsbuildHelperKind::ToEsm => "__toESM",
            EsbuildHelperKind::ToCommonJs => "__toCommonJS",
            EsbuildHelperKind::CopyProps => "__copyProps",
            EsbuildHelperKind::ReExport => "__reExport",
            EsbuildHelperKind::ObjectAlias(kind) => match kind {
                ObjectAliasKind::DefineProperty => "Object.defineProperty",
                ObjectAliasKind::GetOwnPropertyNames => "Object.getOwnPropertyNames",
                ObjectAliasKind::GetOwnPropertyDescriptor => "Object.getOwnPropertyDescriptor",
                ObjectAliasKind::GetPrototypeOf => "Object.getPrototypeOf",
                ObjectAliasKind::Create => "Object.create",
                ObjectAliasKind::HasOwnProperty => "Object.prototype.hasOwnProperty",
                ObjectAliasKind::PropertyIsEnumerable => "Object.prototype.propertyIsEnumerable",
            },
            EsbuildHelperKind::SupportHelper(kind) => match kind {
                SupportHelperKind::GetterBind => "getterBind (Anthropic)",
                SupportHelperKind::Identity => "identity (Anthropic)",
                SupportHelperKind::SetterBind => "setterBind (Anthropic)",
            },
            EsbuildHelperKind::WeakMapCache(kind) => match kind {
                WeakMapCacheKind::ToEsmNodeMode => "WeakMap cache (toESM node)",
                WeakMapCacheKind::ToEsmBrowserMode => "WeakMap cache (toESM browser)",
                WeakMapCacheKind::ToCommonJs => "WeakMap cache (toCommonJS)",
            },
            EsbuildHelperKind::ImportDefault => "__importDefault",
            EsbuildHelperKind::ImportStar => "__importStar",
            EsbuildHelperKind::CreateBinding => "__createBinding",
            EsbuildHelperKind::ExportStar => "__exportStar",
            EsbuildHelperKind::AddDisposableResource => "__addDisposableResource",
            EsbuildHelperKind::DisposeResources => "__disposeResources",
            EsbuildHelperKind::Awaiter => "__awaiter",
            EsbuildHelperKind::Generator => "__generator",
            EsbuildHelperKind::AsyncGenerator => "__asyncGenerator",
            EsbuildHelperKind::Async => "__async",
            EsbuildHelperKind::PrivateGet => "__privateGet",
            EsbuildHelperKind::PrivateSet => "__privateSet",
            EsbuildHelperKind::PrivateAdd => "__privateAdd",
            EsbuildHelperKind::PrivateMethod => "__privateMethod",
            EsbuildHelperKind::PrivateIn => "__privateIn",
            EsbuildHelperKind::PublicField => "__publicField",
            EsbuildHelperKind::PrivateWrapper => "__privateWrapper",
            EsbuildHelperKind::Pow => "__pow",
            EsbuildHelperKind::Name => "__name",
            EsbuildHelperKind::Require => "__require",
            EsbuildHelperKind::Glob => "__glob",
            EsbuildHelperKind::RestKey => "__restKey",
            EsbuildHelperKind::ObjRest => "__objRest",
            EsbuildHelperKind::SpreadValues => "__spreadValues",
            EsbuildHelperKind::SpreadProps => "__spreadProps",
            EsbuildHelperKind::Rest => "__rest",
            EsbuildHelperKind::Spread => "__spread",
            EsbuildHelperKind::SpreadArrays => "__spreadArrays",
            EsbuildHelperKind::SpreadArray => "__spreadArray",
            EsbuildHelperKind::Extends => "__extends",
            EsbuildHelperKind::Assign => "__assign",
            EsbuildHelperKind::Values => "__values",
            EsbuildHelperKind::Read => "__read",
            EsbuildHelperKind::Await => "__await",
            EsbuildHelperKind::AsyncDelegator => "__asyncDelegator",
            EsbuildHelperKind::AsyncValues => "__asyncValues",
            EsbuildHelperKind::YieldStar => "__yieldStar",
            EsbuildHelperKind::ForAwait => "__forAwait",
            EsbuildHelperKind::SuperGet => "__superGet",
            EsbuildHelperKind::SuperSet => "__superSet",
            EsbuildHelperKind::SuperWrapper => "__superWrapper",
            EsbuildHelperKind::Template => "__template",
            EsbuildHelperKind::MakeTemplateObject => "__makeTemplateObject",
            EsbuildHelperKind::ToBinary => "__toBinary",
            EsbuildHelperKind::ToBinaryNode => "__toBinaryNode",
            EsbuildHelperKind::PropKey => "__propKey",
            EsbuildHelperKind::SetFunctionName => "__setFunctionName",
            EsbuildHelperKind::EarlyAccess => "__earlyAccess",
            EsbuildHelperKind::Decorate => "__decorate",
            EsbuildHelperKind::Param => "__param",
            EsbuildHelperKind::EsDecorate => "__esDecorate",
            EsbuildHelperKind::Metadata => "__metadata",
            EsbuildHelperKind::DecorateClass => "__decorateClass",
            EsbuildHelperKind::DecorateParam => "__decorateParam",
            EsbuildHelperKind::DecoratorStart => "__decoratorStart",
            EsbuildHelperKind::DecoratorMetadata => "__decoratorMetadata",
            EsbuildHelperKind::RunInitializers => "__runInitializers",
            EsbuildHelperKind::DecorateElement => "__decorateElement",
        };

        let modified = if info.is_anthropic_modified() {
            " [MODIFIED]"
        } else {
            ""
        };

        header.push_str(&format!("// ║    {name} → {semantic_name}{modified}\n"));
    }

    if !support_helpers.is_empty() {
        header.push_str("// ╠═══════════════════════════════════════════════════════════════════╣\n");
        header.push_str("// ║  Anthropic Support Helpers:                                       ║\n");
        for (name, kind) in support_helpers {
            let desc = match kind {
                SupportHelperKind::GetterBind => "function(q) { return this[q] }",
                SupportHelperKind::Identity => "(q) => q",
                SupportHelperKind::SetterBind => "function(q, K) { this[q] = ... }",
            };
            header.push_str(&format!("// ║    {name}: {desc}\n"));
        }
    }

    header.push_str("// ╚═══════════════════════════════════════════════════════════════════╝\n");
    header.push('\n');

    header
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use oxc_traverse::{ReusableTraverseCtx, traverse_mut_with_ctx};

    fn run_collector(code: &str) -> EsbuildHelperCollector {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut collector = EsbuildHelperCollector::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
        let mut ctx = ReusableTraverseCtx::new(state, scoping, &allocator);

        traverse_mut_with_ctx(&mut collector, &mut program, &mut ctx);
        collector
    }

    #[test]
    fn test_detect_object_alias() {
        let code = r#"
            var __defProp = Object.defineProperty;
            var __getOwnPropNames = Object.getOwnPropertyNames;
        "#;
        let collector = run_collector(code);
        assert!(collector.object_aliases.contains_key("__defProp"));
        assert!(collector.object_aliases.contains_key("__getOwnPropNames"));
    }

    #[test]
    fn test_detect_commonjs_helper() {
        let code = r#"
            var B = (q, K) => () => (K || q((K = {exports: {}}).exports, K), K.exports);
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("B"));
        assert_eq!(collector.helpers.get("B").unwrap().kind, EsbuildHelperKind::CommonJs);
    }

    #[test]
    fn test_detect_esm_helper() {
        let code = r#"
            var L = (q, K) => () => (q && (K = q(q = 0)), K);
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("L"));
        assert_eq!(collector.helpers.get("L").unwrap().kind, EsbuildHelperKind::Esm);
    }

    #[test]
    fn test_detect_export_helper() {
        let code = r#"
            var G8 = (q, K) => {
                for (var _ in K) __defProp(q, _, {get: K[_], enumerable: true});
            };
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("G8"));
        assert_eq!(collector.helpers.get("G8").unwrap().kind, EsbuildHelperKind::Export);
    }

    #[test]
    fn test_detect_identity_helper() {
        let code = r#"
            var sz5 = (q) => q;
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("sz5"));
        assert_eq!(
            collector.helpers.get("sz5").unwrap().kind,
            EsbuildHelperKind::SupportHelper(SupportHelperKind::Identity)
        );
        assert!(collector.support_helpers.contains_key("sz5"));
    }

    #[test]
    fn test_detect_getter_bind_helper() {
        let code = r#"
            function MH7(q) {
                return this[q];
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("MH7"));
        assert_eq!(
            collector.helpers.get("MH7").unwrap().kind,
            EsbuildHelperKind::SupportHelper(SupportHelperKind::GetterBind)
        );
        assert!(collector.support_helpers.contains_key("MH7"));
    }

    #[test]
    fn test_detect_setter_bind_helper() {
        let code = r#"
            function tz5(q, K) {
                this[q] = K;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("tz5"));
        assert_eq!(
            collector.helpers.get("tz5").unwrap().kind,
            EsbuildHelperKind::SupportHelper(SupportHelperKind::SetterBind)
        );
        assert!(collector.support_helpers.contains_key("tz5"));
    }

    #[test]
    fn test_detect_anthropic_export() {
        let code = r#"
            var G8 = (q, K) => {
                for (var _ in K) HI6(q, _, {
                    get: K[_],
                    enumerable: true,
                    configurable: true,
                    set: tz5.bind(K, _)
                });
            };
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("G8"));
        let helper = collector.helpers.get("G8").unwrap();
        assert_eq!(helper.kind, EsbuildHelperKind::Export);
        assert!(helper.is_anthropic_modified());
        assert!(
            helper
                .anthropic_modifications
                .contains(&AnthropicModification::ConfigurableExport)
        );
    }

    #[test]
    fn test_bundle_type_standard() {
        let code = r#"
            var B = (q, K) => () => (K || q((K = {exports: {}}).exports, K), K.exports);
            var L = (q, K) => () => (q && (K = q(q = 0)), K);
        "#;
        let collector = run_collector(code);
        assert_eq!(collector.get_bundle_type(), BundleType::Standard);
    }

    #[test]
    fn test_bundle_type_anthropic() {
        let code = r#"
            var sz5 = (q) => q;
            function MH7(q) {
                return this[q];
            }
        "#;
        let collector = run_collector(code);
        assert_eq!(collector.get_bundle_type(), BundleType::Anthropic);
    }

    #[test]
    fn test_annotation_header_generation() {
        let code = r#"
            var B = (q, K) => () => (K || q((K = {exports: {}}).exports, K), K.exports);
            var sz5 = (q) => q;
            function MH7(q) {
                return this[q];
            }
        "#;
        let collector = run_collector(code);
        let annotated = annotate_esbuild_modules(code, &collector);

        assert!(annotated.contains("ESBUILD BUNDLE ANALYSIS"));
        assert!(annotated.contains("ANTHROPIC-MODIFIED ESBUILD"));
        assert!(annotated.contains("Helper Mapping"));
        assert!(annotated.contains("__commonJS"));
        assert!(annotated.contains("identity (Anthropic)"));
        assert!(annotated.contains("getterBind (Anthropic)"));
    }

    #[test]
    fn test_detect_import_default() {
        let code = r#"
            function __importDefault(mod) {
                return (mod && mod.__esModule) ? mod : { "default": mod };
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__importDefault"));
        assert_eq!(
            collector.helpers.get("__importDefault").unwrap().kind,
            EsbuildHelperKind::ImportDefault
        );
    }

    #[test]
    fn test_detect_create_binding() {
        let code = r#"
            function __createBinding(o, m, k, k2) {
                if (k2 === undefined) k2 = k;
                Object.defineProperty(o, k2, { enumerable: true, get: function() { return m[k]; } });
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__createBinding"));
        assert_eq!(
            collector.helpers.get("__createBinding").unwrap().kind,
            EsbuildHelperKind::CreateBinding
        );
    }

    #[test]
    fn test_detect_export_star() {
        let code = r#"
            function __exportStar(m, exports) {
                for (var p in m) if (p !== "default") __createBinding(exports, m, p);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__exportStar"));
        assert_eq!(
            collector.helpers.get("__exportStar").unwrap().kind,
            EsbuildHelperKind::ExportStar
        );
    }

    #[test]
    fn test_detect_generator() {
        let code = r#"
            function __generator(thisArg, body) {
                var _ = { label: 0, sent: function() {}, trys: [], ops: [] };
                return _;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__generator"));
        assert_eq!(
            collector.helpers.get("__generator").unwrap().kind,
            EsbuildHelperKind::Generator
        );
    }

    #[test]
    fn test_detect_private_add() {
        let code = r#"
            function __privateAdd(obj, state, value) {
                if (state.has(obj)) throw new TypeError("Cannot add same private member more than once");
                state.set(obj, value);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__privateAdd"));
        assert_eq!(
            collector.helpers.get("__privateAdd").unwrap().kind,
            EsbuildHelperKind::PrivateAdd
        );
    }

    #[test]
    fn test_detect_private_get() {
        let code = r#"
            function __privateGet(receiver, state, kind, f) {
                if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
                return kind === "m" ? f : state.get(receiver);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__privateGet"));
        assert_eq!(
            collector.helpers.get("__privateGet").unwrap().kind,
            EsbuildHelperKind::PrivateGet
        );
    }

    #[test]
    fn test_detect_private_set() {
        let code = r#"
            function __privateSet(receiver, state, value, kind, f) {
                if (kind === "m") throw new TypeError("Private method is not writable");
                state.set(receiver, value);
                return value;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__privateSet"));
        assert_eq!(
            collector.helpers.get("__privateSet").unwrap().kind,
            EsbuildHelperKind::PrivateSet
        );
    }

    #[test]
    fn test_detect_private_in() {
        let code = r#"
            function __privateIn(state, receiver) {
                return typeof state === "function" ? receiver === state : state.has(receiver);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__privateIn"));
        assert_eq!(
            collector.helpers.get("__privateIn").unwrap().kind,
            EsbuildHelperKind::PrivateIn
        );
    }

    #[test]
    fn test_detect_dispose_resources() {
        let code = r#"
            function __disposeResources(env, suppressionError) {
                var fail = typeof SuppressedError === "function" ? SuppressedError : function(e, s) { return e; };
                return fail;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__disposeResources"));
        assert_eq!(
            collector.helpers.get("__disposeResources").unwrap().kind,
            EsbuildHelperKind::DisposeResources
        );
    }

    #[test]
    fn test_detect_extends() {
        let code = r#"
            function __extends(d, b) {
                extendStatics(d, b);
                d.prototype = Object.create(b.prototype);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__extends"));
        assert_eq!(
            collector.helpers.get("__extends").unwrap().kind,
            EsbuildHelperKind::Extends
        );
    }

    #[test]
    fn test_detect_decorate() {
        let code = r#"
            function __decorate(decorators, target, key, desc) {
                var c = arguments.length, r;
                for (var i = decorators.length - 1; i >= 0; i--) r = decorators[i](r);
                return r;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__decorate"));
        assert_eq!(
            collector.helpers.get("__decorate").unwrap().kind,
            EsbuildHelperKind::Decorate
        );
    }

    #[test]
    fn test_detect_param() {
        let code = r#"
            function __param(paramIndex, decorator) {
                return function(target, key) { decorator(target, key, paramIndex); };
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__param"));
        assert_eq!(collector.helpers.get("__param").unwrap().kind, EsbuildHelperKind::Param);
    }

    #[test]
    fn test_detect_metadata() {
        let code = r#"
            function __metadata(metadataKey, metadataValue) {
                if (typeof Reflect === "object" && typeof Reflect.metadata === "function")
                    return Reflect.metadata(metadataKey, metadataValue);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__metadata"));
        assert_eq!(
            collector.helpers.get("__metadata").unwrap().kind,
            EsbuildHelperKind::Metadata
        );
    }

    #[test]
    fn test_detect_pow() {
        let code = r#"
            function __pow(base, exp) {
                return Math.pow(base, exp);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__pow"));
        assert_eq!(collector.helpers.get("__pow").unwrap().kind, EsbuildHelperKind::Pow);
    }

    #[test]
    fn test_detect_prop_key() {
        let code = r#"
            function __propKey(x) {
                return typeof x === "symbol" ? x : "".concat(x);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__propKey"));
        assert_eq!(
            collector.helpers.get("__propKey").unwrap().kind,
            EsbuildHelperKind::PropKey
        );
    }

    #[test]
    fn test_detect_early_access() {
        let code = r#"
            function __earlyAccess(name) {
                throw new ReferenceError("Cannot access '" + name + "' before initialization");
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__earlyAccess"));
        assert_eq!(
            collector.helpers.get("__earlyAccess").unwrap().kind,
            EsbuildHelperKind::EarlyAccess
        );
    }

    // ========== NEW TESTS FOR NEWLY-WIRED DETECTION FUNCTIONS ==========

    #[test]
    fn test_detect_awaiter() {
        let code = r#"
            function Ql7(O, A, $, w) {
                return new ($ || ($ = Promise))(function(H, J) {
                    H();
                });
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("Ql7"));
        assert_eq!(collector.helpers.get("Ql7").unwrap().kind, EsbuildHelperKind::Awaiter);
    }

    #[test]
    fn test_detect_async_generator() {
        let code = r#"
            function __asyncGenerator(thisArg, _arguments, generator) {
                var i = {};
                i[Symbol.asyncIterator] = function() { return this; };
                return i;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__asyncGenerator"));
        assert_eq!(
            collector.helpers.get("__asyncGenerator").unwrap().kind,
            EsbuildHelperKind::AsyncGenerator
        );
    }

    #[test]
    fn test_detect_await() {
        let code = r#"
            function __await(v) {
                return this instanceof __await ? (this.v = v, this) : new __await(v);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__await"));
        assert_eq!(collector.helpers.get("__await").unwrap().kind, EsbuildHelperKind::Await);
    }

    #[test]
    fn test_detect_async_delegator() {
        let code = r#"
            function __asyncDelegator(o) {
                var i = { next: null, throw: null };
                return i;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__asyncDelegator"));
        assert_eq!(
            collector.helpers.get("__asyncDelegator").unwrap().kind,
            EsbuildHelperKind::AsyncDelegator
        );
    }

    #[test]
    fn test_detect_async_values() {
        let code = r#"
            function __asyncValues(o) {
                if (!Symbol.asyncIterator) throw new TypeError("Symbol.asyncIterator is not defined.");
                var m = o[Symbol.asyncIterator], i;
                return m ? m.call(o) : i;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__asyncValues"));
        assert_eq!(
            collector.helpers.get("__asyncValues").unwrap().kind,
            EsbuildHelperKind::AsyncValues
        );
    }

    #[test]
    fn test_detect_values() {
        let code = r#"
            function __values(o) {
                var m = o && o[Symbol.iterator];
                return m ? m.call(o) : null;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__values"));
        assert_eq!(
            collector.helpers.get("__values").unwrap().kind,
            EsbuildHelperKind::Values
        );
    }

    #[test]
    fn test_detect_read() {
        let code = r#"
            function __read(o, n) {
                var ar = [];
                try { while (true) ar.push(o.next().value); }
                catch (e) {}
                return ar;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__read"));
        assert_eq!(collector.helpers.get("__read").unwrap().kind, EsbuildHelperKind::Read);
    }

    #[test]
    fn test_detect_rest() {
        let code = r#"
            function Il7(O, A) {
                var $ = {};
                for (var w in O)
                    if (A.indexOf(w)) $[w] = O[w];
                return $;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("Il7"));
        assert_eq!(collector.helpers.get("Il7").unwrap().kind, EsbuildHelperKind::Rest);
    }

    #[test]
    fn test_detect_spread() {
        let code = r#"
            function __spread() {
                var ar = [];
                return ar.concat(arguments);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__spread"));
        assert_eq!(
            collector.helpers.get("__spread").unwrap().kind,
            EsbuildHelperKind::SpreadArray
        );
    }

    #[test]
    fn test_detect_assign() {
        let code = r#"
            function __assign(t) {
                for (var p in arguments) t[p] = arguments[p];
                return t;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__assign"));
        assert_eq!(
            collector.helpers.get("__assign").unwrap().kind,
            EsbuildHelperKind::Assign
        );
    }

    #[test]
    fn test_detect_template() {
        let code = r#"
            function __template(cooked, raw) {
                cooked.raw = raw;
                return cooked;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__template"));
        assert_eq!(
            collector.helpers.get("__template").unwrap().kind,
            EsbuildHelperKind::Template
        );
    }

    #[test]
    fn test_detect_name() {
        let code = r#"
            function __name(target, value) {
                return (Object.defineProperty(target, "name", { value: value }), target);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__name"));
        assert_eq!(collector.helpers.get("__name").unwrap().kind, EsbuildHelperKind::Name);
    }

    #[test]
    fn test_detect_to_binary() {
        let code = r#"
            function __toBinary(data) {
                var decoded = atob(data);
                return decoded;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__toBinary"));
        assert_eq!(
            collector.helpers.get("__toBinary").unwrap().kind,
            EsbuildHelperKind::ToBinary
        );
    }

    #[test]
    fn test_detect_super_get() {
        let code = r#"
            function __superGet(cls, self, key) {
                return Reflect.get(cls, key, self);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__superGet"));
        assert_eq!(
            collector.helpers.get("__superGet").unwrap().kind,
            EsbuildHelperKind::SuperGet
        );
    }

    #[test]
    fn test_detect_super_set() {
        let code = r#"
            function __superSet(cls, self, key, value) {
                return Reflect.set(cls, key, value, self);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__superSet"));
        assert_eq!(
            collector.helpers.get("__superSet").unwrap().kind,
            EsbuildHelperKind::SuperSet
        );
    }

    #[test]
    fn test_detect_public_field() {
        let code = r#"
            function __publicField(obj, key, value) {
                obj[key] = value;
                return value;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__publicField"));
        assert_eq!(
            collector.helpers.get("__publicField").unwrap().kind,
            EsbuildHelperKind::PublicField
        );
    }

    #[test]
    fn test_detect_private_method() {
        let code = r#"
            function __privateMethod(obj, member, method) {
                __accessCheck(obj, member, "access private method");
                return method.bind(obj);
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__privateMethod"));
        assert_eq!(
            collector.helpers.get("__privateMethod").unwrap().kind,
            EsbuildHelperKind::PrivateMethod
        );
    }

    #[test]
    fn test_detect_require() {
        let code = r#"
            function __require(x) {
                return new Proxy({}, {
                    get: function() { return require; }
                });
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__require"));
        assert_eq!(
            collector.helpers.get("__require").unwrap().kind,
            EsbuildHelperKind::Require
        );
    }

    #[test]
    fn test_detect_glob() {
        let code = r#"
            function __glob(map) {
                var modules = {};
                return modules;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__glob"));
        assert_eq!(collector.helpers.get("__glob").unwrap().kind, EsbuildHelperKind::Glob);
    }

    #[test]
    fn test_detect_decorate_class() {
        let code = r#"
            function __decorateClass(decorators, target, key, kind) {
                var result = target;
                for (var i = decorators.length - 1; i >= 0; i--) {
                    var decorator = decorators[i];
                    if (decorator) result = decorator(result) || result;
                }
                return result;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__decorateClass"));
        assert_eq!(
            collector.helpers.get("__decorateClass").unwrap().kind,
            EsbuildHelperKind::DecorateClass
        );
    }

    #[test]
    fn test_detect_run_initializers() {
        let code = r#"
            function __runInitializers(thisArg, initializers, value) {
                for (var i = 0; i < initializers.length; i++) {
                    value = initializers[i].call(thisArg, value);
                }
                return value;
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__runInitializers"));
        assert_eq!(
            collector.helpers.get("__runInitializers").unwrap().kind,
            EsbuildHelperKind::RunInitializers
        );
    }

    #[test]
    fn test_detect_decorator_start() {
        let code = r#"
            function __decoratorStart() {
                return [,,];
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__decoratorStart"));
        assert_eq!(
            collector.helpers.get("__decoratorStart").unwrap().kind,
            EsbuildHelperKind::DecoratorStart
        );
    }

    #[test]
    fn test_detect_decorate_element() {
        let code = r#"
            function __decorateElement(array, flags, name, decorators) {
                switch (flags) {
                    case 1: return array;
                    case 2: return array;
                    case 3: return array;
                    default: return array;
                }
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__decorateElement"));
        assert_eq!(
            collector.helpers.get("__decorateElement").unwrap().kind,
            EsbuildHelperKind::DecorateElement
        );
    }

    #[test]
    fn test_detect_set_function_name() {
        let code = r#"
            function __setFunctionName(target, name, prefix) {
                if (typeof name === "symbol") name = "[" + name.description + "]";
                return Object.defineProperty(target, "name", { value: name });
            }
        "#;
        let collector = run_collector(code);
        assert!(collector.helpers.contains_key("__setFunctionName"));
        assert_eq!(
            collector.helpers.get("__setFunctionName").unwrap().kind,
            EsbuildHelperKind::SetFunctionName
        );
    }
}
