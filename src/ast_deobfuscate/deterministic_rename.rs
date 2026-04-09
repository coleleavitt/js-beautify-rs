//! Slot-based variable normalizer for Bun-minified bundles
//!
//! Converts minified variable names to canonical slot-based names (s0, s1, s2, ...)
//! that are stable across different Bun versions regardless of alphabet ordering.

use oxc_ast::ast::{BindingIdentifier, Function, IdentifierReference};
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::FxHashMap;

use crate::ast_deobfuscate::bun_alphabet::{BunAlphabet, extract_alphabet_from_source};
use crate::ast_deobfuscate::state::DeobfuscateState;

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

const JS_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "finally",
    "for",
    "function",
    "if",
    "in",
    "instanceof",
    "new",
    "return",
    "switch",
    "this",
    "throw",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "class",
    "const",
    "enum",
    "export",
    "extends",
    "import",
    "super",
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
    "await",
    "async",
    "of",
    "get",
    "set",
    "null",
    "true",
    "false",
];

const GLOBALS: &[&str] = &[
    "console",
    "process",
    "require",
    "module",
    "exports",
    "__dirname",
    "__filename",
    "arguments",
    "undefined",
    "NaN",
    "Infinity",
    "globalThis",
    "self",
    "window",
    "document",
    "navigator",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "Promise",
    "Array",
    "Object",
    "String",
    "Number",
    "Boolean",
    "Symbol",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
    "AggregateError",
    "JSON",
    "Math",
    "Date",
    "RegExp",
    "Reflect",
    "Proxy",
    "Intl",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "encodeURIComponent",
    "decodeURIComponent",
    "encodeURI",
    "decodeURI",
    "eval",
    "Buffer",
    "atob",
    "btoa",
    "fetch",
    "URL",
    "URLSearchParams",
    "TextEncoder",
    "TextDecoder",
    "AbortController",
    "AbortSignal",
    "Event",
    "EventTarget",
    "Headers",
    "Request",
    "Response",
    "ReadableStream",
    "WritableStream",
    "TransformStream",
    "Blob",
    "File",
    "FormData",
    "WebSocket",
    "crypto",
    "performance",
    "queueMicrotask",
    "structuredClone",
    "Function",
    "ArrayBuffer",
    "DataView",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    "BigInt",
    "SharedArrayBuffer",
    "Atomics",
    "FinalizationRegistry",
    "WeakRef",
    "Iterator",
    "AsyncIterator",
    "Generator",
    "AsyncGenerator",
    "GeneratorFunction",
    "AsyncGeneratorFunction",
    "AsyncFunction",
];

pub struct DeterministicRenamer {
    alphabet: BunAlphabet,
    rename_map: FxHashMap<String, String>,
    skip_names: FxHashMap<&'static str, ()>,
    changed: bool,
    renamed_count: usize,
}

impl DeterministicRenamer {
    #[must_use]
    pub fn new() -> Self {
        let mut skip_names = FxHashMap::default();
        for &kw in JS_KEYWORDS {
            skip_names.insert(kw, ());
        }
        for &g in GLOBALS {
            skip_names.insert(g, ());
        }

        Self {
            alphabet: BunAlphabet::default_alphabet(),
            rename_map: FxHashMap::default(),
            skip_names,
            changed: false,
            renamed_count: 0,
        }
    }

    #[must_use]
    pub const fn has_changed(&self) -> bool {
        self.changed
    }

    #[must_use]
    pub const fn renamed_count(&self) -> usize {
        self.renamed_count
    }

    /// Extracts the alphabet from the source and builds the rename map
    pub fn analyze_source(&mut self, source: &str) {
        self.alphabet = extract_alphabet_from_source(source);
        eprintln!(
            "[SLOT_RENAME] Extracted HEAD: {}",
            &self.alphabet.head[..20.min(self.alphabet.head.len())]
        );
        eprintln!(
            "[SLOT_RENAME] Extracted TAIL: {}",
            &self.alphabet.tail[..20.min(self.alphabet.tail.len())]
        );
    }

    /// Builds the rename map from minified names to slot-based names
    pub fn build_rename_map(&mut self, source: &str) {
        let mut identifier_slots: Vec<(String, usize)> = Vec::new();

        for name in extract_identifiers(source) {
            if self.should_skip(&name) {
                continue;
            }

            if let Some(slot) = self.alphabet.name_to_slot(&name) {
                if !self.rename_map.contains_key(&name) {
                    identifier_slots.push((name, slot));
                }
            }
        }

        for (name, slot) in identifier_slots {
            let canonical = format!("s{slot}");
            self.rename_map.insert(name, canonical);
        }

        eprintln!("[SLOT_RENAME] Built rename map with {} entries", self.rename_map.len());
    }

    fn should_skip(&self, name: &str) -> bool {
        if self.skip_names.contains_key(name) {
            return true;
        }

        if name.starts_with("_0x") {
            return true;
        }

        if name.len() > 4 {
            return true;
        }

        if name.len() > 2 && name.starts_with('_') && !name.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
            return true;
        }

        false
    }

    fn try_rename(&mut self, name: &str) -> Option<&str> {
        self.rename_map.get(name).map(String::as_str)
    }
}

impl Default for DeterministicRenamer {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for DeterministicRenamer {
    fn enter_binding_identifier(&mut self, ident: &mut BindingIdentifier<'a>, ctx: &mut Ctx<'a>) {
        let old_name = ident.name.as_str();
        if let Some(new_name) = self.try_rename(old_name) {
            ident.name = ctx.ast.atom(new_name).into();
            self.changed = true;
            self.renamed_count += 1;
        }
    }

    fn enter_identifier_reference(&mut self, ident: &mut IdentifierReference<'a>, ctx: &mut Ctx<'a>) {
        let old_name = ident.name.as_str();
        if let Some(new_name) = self.try_rename(old_name) {
            ident.name = ctx.ast.atom(new_name).into();
            self.changed = true;
            self.renamed_count += 1;
        }
    }

    fn enter_function(&mut self, func: &mut Function<'a>, ctx: &mut Ctx<'a>) {
        if let Some(ident) = &mut func.id {
            let old_name = ident.name.as_str();
            if let Some(new_name) = self.try_rename(old_name) {
                ident.name = ctx.ast.atom(new_name).into();
                self.changed = true;
                self.renamed_count += 1;
            }
        }
    }
}

fn extract_identifiers(source: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut string_char = ' ';
    let mut prev_char = ' ';

    for c in source.chars() {
        if !in_string && (c == '"' || c == '\'' || c == '`') {
            in_string = true;
            string_char = c;
            prev_char = c;
            continue;
        }

        if in_string {
            if c == string_char && prev_char != '\\' {
                in_string = false;
            }
            prev_char = c;
            continue;
        }

        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            current.push(c);
        } else if !current.is_empty() {
            if current
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
            {
                if current.len() <= 4 {
                    identifiers.push(current.clone());
                }
            }
            current.clear();
        }

        prev_char = c;
    }

    if !current.is_empty()
        && current
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
        && current.len() <= 4
    {
        identifiers.push(current);
    }

    identifiers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_deobfuscate::DeobfuscateState;
    use oxc_allocator::Allocator;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use oxc_traverse::{ReusableTraverseCtx, traverse_mut_with_ctx};

    fn run_rename(code: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut renamer = DeterministicRenamer::new();
        renamer.analyze_source(code);
        renamer.build_rename_map(code);

        let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
        let mut ctx = ReusableTraverseCtx::new(DeobfuscateState::new(), scoping, &allocator);
        traverse_mut_with_ctx(&mut renamer, &mut program, &mut ctx);

        Codegen::new().build(&program).code
    }

    #[test]
    fn test_slot_based_rename() {
        let code = "var q = 1; var K = 2; console.log(q, K);";
        let output = run_rename(code);

        assert!(
            output.contains("s0") || output.contains("s1"),
            "Should have slot-based names"
        );
        assert!(output.contains("console"), "Should preserve globals");
    }

    #[test]
    fn test_preserves_globals() {
        let code = "console.log(JSON.stringify({}));";
        let output = run_rename(code);

        assert!(output.contains("console"));
        assert!(output.contains("JSON"));
        assert!(output.contains("stringify"));
    }

    #[test]
    fn test_preserves_keywords() {
        let code = "function test() { return true; }";
        let output = run_rename(code);

        assert!(output.contains("function"));
        assert!(output.contains("return"));
        assert!(output.contains("true"));
    }

    #[test]
    fn test_skip_0x_prefix() {
        let code = "var _0x1234 = 42;";
        let output = run_rename(code);

        assert!(output.contains("_0x1234"));
    }

    #[test]
    fn test_consistent_slots() {
        let code1 = "var q = 1; var K = 2;";
        let code2 = "var q = 1; var K = 2;";

        let output1 = run_rename(code1);
        let output2 = run_rename(code2);

        assert_eq!(output1, output2, "Same code should produce same output");
    }
}
