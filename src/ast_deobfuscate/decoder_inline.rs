//! Decoder function detection and inlining pass
//!
//! Detects decoder functions that wrap string array access:
//! ```js
//! var _0x1234 = ["hello", "world"];
//! function _0xdec(a) { return _0x1234[a - 291]; }
//! console.log(_0xdec(291)); // Inlined to: console.log("hello");
//! ```

use oxc_ast::ast::*;
use oxc_span::SPAN;
use oxc_traverse::{Traverse, TraverseCtx};
use rustc_hash::FxHashMap;

use crate::ast_deobfuscate::state::{DecoderInfo, DecoderType, DeobfuscateState, OffsetOperation};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rc4::{
    Key, KeyInit, Rc4, StreamCipher,
    consts::{U8, U16, U32},
};

pub type Ctx<'a> = TraverseCtx<'a, DeobfuscateState>;

pub struct DecoderInliner {
    detected_decoders: FxHashMap<String, DecoderInfo>,
    changed: bool,
}

impl DecoderInliner {
    pub fn new() -> Self {
        Self {
            detected_decoders: FxHashMap::default(),
            changed: false,
        }
    }

    pub fn has_changed(&self) -> bool {
        self.changed
    }

    fn detect_decoder_function<'a>(
        &mut self,
        func: &Function<'a>,
        func_name: &str,
        ctx: &Ctx<'a>,
    ) -> Option<DecoderInfo> {
        eprintln!("[AST] Analyzing function: {}", func_name);

        if func.params.items.len() != 1 {
            eprintln!(
                "[AST]   Function has {} params, need exactly 1",
                func.params.items.len()
            );
            return None;
        }

        let param_name = match &func.params.items[0].pattern {
            BindingPattern::BindingIdentifier(ident) => {
                eprintln!("[AST]   Parameter: {}", ident.name);
                ident.name.as_str()
            }
            _ => {
                eprintln!("[AST]   Parameter is not simple identifier");
                return None;
            }
        };

        let body = func.body.as_ref()?;
        eprintln!(
            "[AST]   Function body has {} statements",
            body.statements.len()
        );

        let (array_name, offset, offset_op, decoder_type) =
            self.analyze_function_body(&body.statements, param_name, ctx)?;

        eprintln!(
            "[AST] ✓ Detected decoder: {} for array {} (offset: {:?} {}, type: {:?})",
            func_name, array_name, offset_op, offset, decoder_type
        );

        Some(DecoderInfo {
            function_name: func_name.to_string(),
            array_name,
            offset,
            offset_operation: offset_op,
            decoder_type,
        })
    }

    fn analyze_function_body<'a>(
        &self,
        statements: &[Statement<'a>],
        param_name: &str,
        ctx: &Ctx<'a>,
    ) -> Option<(String, i32, OffsetOperation, DecoderType)> {
        eprintln!(
            "[AST]   Analyzing body with {} statements",
            statements.len()
        );

        let mut array_name = None;
        let mut offset = 0i32;
        let mut offset_op = OffsetOperation::None;
        let mut decoder_type = DecoderType::Simple;

        for stmt in statements {
            match stmt {
                Statement::ReturnStatement(ret) => {
                    eprintln!("[AST]     Found return statement");
                    if let Some(arg) = &ret.argument {
                        if let Some((arr, off, op, dec_type)) =
                            self.extract_array_access_with_decoder(arg, param_name, ctx)
                        {
                            eprintln!(
                                "[AST]       extract_array_access returned: array={}, offset={}, op={:?}, decoder={:?}",
                                arr, off, op, dec_type
                            );
                            array_name = Some(arr);
                            decoder_type = dec_type;
                            if offset_op == OffsetOperation::None {
                                eprintln!(
                                    "[AST]       No assignment offset found, using inline offset"
                                );
                                offset = off;
                                offset_op = op;
                            } else {
                                eprintln!(
                                    "[AST]       Already have assignment offset ({:?} {}), keeping it",
                                    offset_op, offset
                                );
                            }
                        }
                    }
                }
                Statement::ExpressionStatement(expr_stmt) => {
                    eprintln!("[AST]     Found expression statement");
                    if let Some((off, op)) =
                        self.extract_offset_assignment(&expr_stmt.expression, param_name)
                    {
                        offset = off;
                        offset_op = op;
                        eprintln!("[AST]       Extracted offset assignment: {:?} {}", op, off);
                    }
                }
                _ => {
                    eprintln!("[AST]     Found other statement type");
                }
            }
        }

        if let Some(arr) = array_name {
            if ctx.state.string_arrays.contains_key(&arr) {
                eprintln!("[AST]     Array {} is a known string array", arr);
                Some((arr, offset, offset_op, decoder_type))
            } else {
                eprintln!("[AST]     Array {} not found in string arrays", arr);
                None
            }
        } else {
            eprintln!("[AST]     No array access found");
            None
        }
    }

    fn extract_array_access_with_decoder<'a>(
        &self,
        expr: &Expression<'a>,
        param_name: &str,
        _ctx: &Ctx<'a>,
    ) -> Option<(String, i32, OffsetOperation, DecoderType)> {
        if let Some(decoder_type) = self.detect_decoder_call(expr) {
            eprintln!("[AST]       Detected decoder call: {:?}", decoder_type);
            if let Expression::CallExpression(call) = expr {
                if let Some(arg) = call.arguments.first() {
                    if let Some(arg_expr) = arg.as_expression() {
                        if let Some((arr, off, op)) =
                            self.extract_simple_array_access(arg_expr, param_name)
                        {
                            return Some((arr, off, op, decoder_type));
                        }
                    }
                }
            }
        }

        if let Some((arr, off, op)) = self.extract_simple_array_access(expr, param_name) {
            return Some((arr, off, op, DecoderType::Simple));
        }

        None
    }

    fn detect_decoder_call(&self, expr: &Expression<'_>) -> Option<DecoderType> {
        if let Expression::CallExpression(call) = expr {
            if let Expression::Identifier(func_id) = &call.callee {
                let func_name = func_id.name.as_str();
                eprintln!(
                    "[AST]         detect_decoder_call: func_name = {}",
                    func_name
                );

                if func_name == "atob" {
                    eprintln!("[AST]         Detected atob (base64)");
                    return Some(DecoderType::Base64);
                }

                if func_name.contains("xor") || func_name.contains("XOR") {
                    eprintln!("[AST]         Function contains 'xor', trying to extract key");
                    if let Some(key) = self.extract_xor_key(call) {
                        eprintln!("[AST]         Extracted XOR key: {:?}", key);
                        return Some(DecoderType::Xor { key });
                    } else {
                        eprintln!("[AST]         Failed to extract XOR key");
                    }
                }

                if func_name.contains("rc4") || func_name.contains("RC4") {
                    eprintln!("[AST]         Function contains 'rc4', trying to extract key");
                    if let Some(key) = self.extract_rc4_key(call) {
                        eprintln!("[AST]         Extracted RC4 key: {:?}", key);
                        return Some(DecoderType::Rc4 { key });
                    } else {
                        eprintln!("[AST]         Failed to extract RC4 key");
                    }
                }

                eprintln!(
                    "[AST]         No decoder pattern matched for function: {}",
                    func_name
                );
            } else {
                eprintln!("[AST]         Callee is not an identifier");
            }
        } else {
            eprintln!("[AST]         Expression is not a call expression");
        }
        None
    }

    fn extract_xor_key(&self, call: &CallExpression<'_>) -> Option<Vec<u8>> {
        eprintln!(
            "[AST]           extract_xor_key: args len = {}",
            call.arguments.len()
        );
        if call.arguments.len() >= 2 {
            if let Some(arg) = call.arguments.get(1) {
                eprintln!("[AST]           Got second argument");
                if let Some(expr) = arg.as_expression() {
                    eprintln!("[AST]           Argument is expression");
                    if let Expression::StringLiteral(lit) = expr {
                        let key = lit.value.as_bytes().to_vec();
                        eprintln!("[AST]           String literal key: {:?}", key);
                        return Some(key);
                    }
                    if let Expression::NumericLiteral(lit) = expr {
                        let key = vec![lit.value as u8];
                        eprintln!("[AST]           Numeric literal key: {:?}", key);
                        return Some(key);
                    }
                    eprintln!("[AST]           Argument is not string or numeric literal");
                } else {
                    eprintln!("[AST]           Argument is not expression");
                }
            } else {
                eprintln!("[AST]           No second argument");
            }
        } else {
            eprintln!("[AST]           Not enough arguments (need at least 2)");
        }
        None
    }

    fn extract_rc4_key(&self, call: &CallExpression<'_>) -> Option<Vec<u8>> {
        if call.arguments.len() >= 2 {
            if let Some(arg) = call.arguments.get(1) {
                if let Some(expr) = arg.as_expression() {
                    if let Expression::StringLiteral(lit) = expr {
                        return Some(lit.value.as_bytes().to_vec());
                    }
                }
            }
        }
        None
    }

    fn apply_decoder(&self, value: &str, decoder_type: &DecoderType) -> Option<String> {
        eprintln!(
            "[AST]   apply_decoder called with value: {:?}, type: {:?}",
            value, decoder_type
        );
        let result = match decoder_type {
            DecoderType::Simple => Some(value.to_string()),
            DecoderType::Base64 => self.decode_base64(value),
            DecoderType::Xor { key } => self.decode_xor(value, key),
            DecoderType::Rc4 { key } => self.decode_rc4(value, key),
        };
        eprintln!("[AST]   apply_decoder result: {:?}", result);
        result
    }

    fn decode_base64(&self, value: &str) -> Option<String> {
        eprintln!("[AST]     decode_base64 input: {:?}", value);
        let result = BASE64_STANDARD
            .decode(value.as_bytes())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok());
        eprintln!("[AST]     decode_base64 output: {:?}", result);
        result
    }

    fn decode_xor(&self, value: &str, key: &[u8]) -> Option<String> {
        eprintln!("[AST]     decode_xor input: {:?}, key: {:?}", value, key);
        if key.is_empty() {
            eprintln!("[AST]     decode_xor: empty key, returning None");
            return None;
        }

        let bytes = value.as_bytes();
        let decoded: Vec<u8> = bytes
            .iter()
            .enumerate()
            .map(|(i, &byte)| byte ^ key[i % key.len()])
            .collect();

        let result = String::from_utf8(decoded).ok();
        eprintln!("[AST]     decode_xor output: {:?}", result);
        result
    }

    fn decode_rc4(&self, value: &str, key: &[u8]) -> Option<String> {
        eprintln!(
            "[AST]     decode_rc4 input: {:?}, key len: {}",
            value,
            key.len()
        );
        if key.is_empty() {
            eprintln!("[AST]     decode_rc4: empty key, returning None");
            return None;
        }

        let mut data = BASE64_STANDARD.decode(value.as_bytes()).ok()?;
        eprintln!("[AST]     decode_rc4 base64-decoded bytes: {:?}", data);

        match key.len() {
            8 => {
                eprintln!("[AST]     decode_rc4: using 8-byte key");
                let key_arr = Key::<U8>::from_slice(key);
                let mut cipher = Rc4::<_>::new(key_arr);
                cipher.apply_keystream(&mut data);
            }
            16 => {
                eprintln!("[AST]     decode_rc4: using 16-byte key");
                let key_arr = Key::<U16>::from_slice(key);
                let mut cipher = Rc4::<_>::new(key_arr);
                cipher.apply_keystream(&mut data);
            }
            32 => {
                eprintln!("[AST]     decode_rc4: using 32-byte key");
                let key_arr = Key::<U32>::from_slice(key);
                let mut cipher = Rc4::<_>::new(key_arr);
                cipher.apply_keystream(&mut data);
            }
            _ => {
                eprintln!(
                    "[AST]     decode_rc4: unsupported key length: {} bytes",
                    key.len()
                );
                return None;
            }
        }

        eprintln!("[AST]     decode_rc4 output bytes: {:?}", data);
        let result = String::from_utf8(data).ok();
        eprintln!("[AST]     decode_rc4 output: {:?}", result);
        result
    }

    fn extract_simple_array_access<'a>(
        &self,
        expr: &Expression<'a>,
        param_name: &str,
    ) -> Option<(String, i32, OffsetOperation)> {
        match expr {
            Expression::ComputedMemberExpression(member) => {
                eprintln!("[AST]       Found computed member expression");

                let array_name = match &member.object {
                    Expression::Identifier(ident) => {
                        eprintln!("[AST]         Array: {}", ident.name);
                        ident.name.to_string()
                    }
                    _ => return None,
                };

                let (offset, offset_op) = match &member.expression {
                    Expression::Identifier(ident) if ident.name.as_str() == param_name => {
                        eprintln!("[AST]         Index: {} (no offset)", param_name);
                        (0, OffsetOperation::None)
                    }
                    Expression::BinaryExpression(bin) => {
                        eprintln!("[AST]         Index is binary expression");
                        self.extract_offset_from_binary(bin, param_name)?
                    }
                    _ => {
                        eprintln!("[AST]         Index is not identifier or binary");
                        return None;
                    }
                };

                Some((array_name, offset, offset_op))
            }
            _ => {
                eprintln!("[AST]       Return expression is not array access");
                None
            }
        }
    }

    fn extract_offset_assignment<'a>(
        &self,
        expr: &Expression<'a>,
        param_name: &str,
    ) -> Option<(i32, OffsetOperation)> {
        if let Expression::AssignmentExpression(assign) = expr {
            if let AssignmentTarget::AssignmentTargetIdentifier(target) = &assign.left {
                if target.name.as_str() == param_name {
                    if let Expression::BinaryExpression(bin) = &assign.right {
                        return self.extract_offset_from_binary(bin, param_name);
                    }
                }
            }
        }
        None
    }

    fn extract_offset_from_binary<'a>(
        &self,
        bin: &BinaryExpression<'a>,
        param_name: &str,
    ) -> Option<(i32, OffsetOperation)> {
        let left_is_param =
            matches!(&bin.left, Expression::Identifier(id) if id.name.as_str() == param_name);

        if !left_is_param {
            eprintln!("[AST]           Left side is not parameter");
            return None;
        }

        let offset = match &bin.right {
            Expression::NumericLiteral(lit) => {
                eprintln!("[AST]           Right side is number: {}", lit.value);
                lit.value as i32
            }
            _ => {
                eprintln!("[AST]           Right side is not numeric literal");
                return None;
            }
        };

        let offset_op = match bin.operator {
            BinaryOperator::Subtraction => {
                eprintln!("[AST]           Operation: subtract");
                OffsetOperation::Subtract
            }
            BinaryOperator::Addition => {
                eprintln!("[AST]           Operation: add");
                OffsetOperation::Add
            }
            _ => {
                eprintln!("[AST]           Operation: unsupported");
                return None;
            }
        };

        Some((offset, offset_op))
    }

    fn try_inline_decoder_call<'a>(
        &mut self,
        call: &CallExpression<'a>,
        ctx: &mut Ctx<'a>,
    ) -> Option<Expression<'a>> {
        eprintln!("[AST] Checking call for decoder inlining");

        let func_name = match &call.callee {
            Expression::Identifier(ident) => {
                eprintln!("[AST]   Function: {}", ident.name);
                ident.name.as_str()
            }
            _ => {
                eprintln!("[AST]   Callee is not identifier");
                return None;
            }
        };

        let decoder = self.detected_decoders.get(func_name)?;
        eprintln!("[AST]   Found decoder for function {}", func_name);

        if call.arguments.len() != 1 {
            eprintln!(
                "[AST]   Call has {} args, need exactly 1",
                call.arguments.len()
            );
            return None;
        }

        let arg_value = match &call.arguments[0] {
            Argument::NumericLiteral(lit) => {
                eprintln!("[AST]   Argument: {}", lit.value);
                lit.value as i32
            }
            _ => {
                eprintln!("[AST]   Argument is not numeric literal");
                return None;
            }
        };

        eprintln!(
            "[AST]   Decoder offset_operation: {:?}, offset: {}",
            decoder.offset_operation, decoder.offset
        );

        let actual_index = match decoder.offset_operation {
            OffsetOperation::None => {
                eprintln!(
                    "[AST]   No offset, using argument value directly: {}",
                    arg_value
                );
                arg_value as usize
            }
            OffsetOperation::Subtract => {
                let result = arg_value - decoder.offset;
                eprintln!(
                    "[AST]   Applying offset: {} - {} = {}",
                    arg_value, decoder.offset, result
                );
                if result < 0 {
                    eprintln!("[AST]   Negative index after offset");
                    return None;
                }
                result as usize
            }
            OffsetOperation::Add => {
                let result = arg_value + decoder.offset;
                eprintln!(
                    "[AST]   Applying offset: {} + {} = {}",
                    arg_value, decoder.offset, result
                );
                result as usize
            }
        };

        let array_info = ctx.state.string_arrays.get(&decoder.array_name)?;
        eprintln!(
            "[AST]   Array {} has {} strings",
            decoder.array_name,
            array_info.strings.len()
        );

        if actual_index >= array_info.strings.len() {
            eprintln!(
                "[AST]   Index {} out of bounds (array has {})",
                actual_index,
                array_info.strings.len()
            );
            return None;
        }

        let raw_value = &array_info.strings[actual_index];
        eprintln!("[AST]   Raw value from array: {:?}", raw_value);
        eprintln!("[AST]   Decoder type: {:?}", decoder.decoder_type);

        let decoded_value = self.apply_decoder(raw_value, &decoder.decoder_type)?;

        eprintln!(
            "[AST] ✓ Inlining decoder call: {}({}) → \"{}\" (decoder: {:?})",
            func_name, arg_value, decoded_value, decoder.decoder_type
        );

        Some(Expression::StringLiteral(ctx.ast.alloc(StringLiteral {
            span: SPAN,
            value: ctx.ast.atom(&decoded_value),
            raw: None,
            lone_surrogates: false,
        })))
    }

    pub fn finalize(&mut self, state: &mut DeobfuscateState) {
        state
            .decoders
            .extend(self.detected_decoders.drain().map(|(k, v)| (k, v)));
    }
}

impl Default for DecoderInliner {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Traverse<'a, DeobfuscateState> for DecoderInliner {
    fn enter_statement(&mut self, stmt: &mut Statement<'a>, ctx: &mut Ctx<'a>) {
        if let Statement::FunctionDeclaration(func_decl) = stmt {
            let func_name = func_decl
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .unwrap_or("");

            if !func_name.is_empty() {
                if let Some(decoder) = self.detect_decoder_function(func_decl, func_name, ctx) {
                    eprintln!(
                        "[AST] Storing decoder: {} -> array={}, offset={}, op={:?}",
                        decoder.function_name,
                        decoder.array_name,
                        decoder.offset,
                        decoder.offset_operation
                    );
                    self.detected_decoders
                        .insert(decoder.function_name.clone(), decoder);
                }
            }
        }
    }

    fn enter_expression(&mut self, expr: &mut Expression<'a>, ctx: &mut Ctx<'a>) {
        if let Expression::CallExpression(call) = expr {
            if let Some(new_expr) = self.try_inline_decoder_call(call, ctx) {
                *expr = new_expr;
                self.changed = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_deobfuscate::{DeobfuscateState, StringArrayInliner, StringArrayRotation};
    use oxc_allocator::Allocator;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;

    fn run_decoder_inline(code: &str) -> (String, DecoderInliner) {
        let allocator = Allocator::default();
        let source_type = SourceType::mjs();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let mut program = ret.program;

        let mut rotation = StringArrayRotation::new();
        let state = DeobfuscateState::new();
        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = oxc_traverse::ReusableTraverseCtx::new(state, scoping, &allocator);

        oxc_traverse::traverse_mut_with_ctx(&mut rotation, &mut program, &mut ctx);

        let mut state = DeobfuscateState::new();
        rotation.finalize(&mut state);

        let scoping = SemanticBuilder::new()
            .build(&program)
            .semantic
            .into_scoping();
        let mut ctx = oxc_traverse::ReusableTraverseCtx::new(state, scoping, &allocator);

        let mut inliner = StringArrayInliner::new();
        oxc_traverse::traverse_mut_with_ctx(&mut inliner, &mut program, &mut ctx);

        let mut decoder = DecoderInliner::new();
        oxc_traverse::traverse_mut_with_ctx(&mut decoder, &mut program, &mut ctx);

        let output = Codegen::new().build(&program).code;
        (output, decoder)
    }

    #[test]
    fn test_simple_decoder() {
        let code = r#"
            var _0x1234 = ["hello", "world", "test"];
            function _0xdec(a) {
                return _0x1234[a];
            }
            console.log(_0xdec(0));
            console.log(_0xdec(1));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed(), "Should have inlined decoder calls");
        assert!(
            output.contains("\"hello\""),
            "Should contain inlined 'hello'"
        );
        assert!(
            output.contains("\"world\""),
            "Should contain inlined 'world'"
        );
        assert!(
            !output.contains("_0xdec(0)"),
            "Should not contain decoder call"
        );
    }

    #[test]
    fn test_decoder_with_offset() {
        let code = r#"
            var _0xabcd = ["foo", "bar", "baz"];
            function _0xdecode(a) {
                return _0xabcd[a - 100];
            }
            console.log(_0xdecode(100));
            console.log(_0xdecode(101));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed());
        assert!(output.contains("\"foo\""), "Should inline with offset");
        assert!(output.contains("\"bar\""), "Should inline with offset");
    }

    #[test]
    fn test_decoder_with_assignment_offset() {
        let code = r#"
            var _0x1234 = ["a", "b", "c"];
            function _0xdec(a) {
                a = a - 291;
                return _0x1234[a];
            }
            console.log(_0xdec(291));
            console.log(_0xdec(292));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed());
        assert!(output.contains("\"a\""));
        assert!(output.contains("\"b\""));
    }

    #[test]
    fn test_non_constant_arg() {
        let code = r#"
            var _0x1234 = ["hello", "world"];
            function _0xdec(a) {
                return _0x1234[a];
            }
            var i = 0;
            console.log(_0xdec(i));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(!decoder.has_changed(), "Should not inline variable arg");
        assert!(
            output.contains("_0xdec(i)"),
            "Should preserve variable arg call"
        );
    }

    #[test]
    fn test_decoder_with_rotated_array() {
        let code = r#"
            var _0x1111 = ["a", "b", "c"];
            (function(_0x2222, _0x3333) {
                var _0x4444 = function(_0x5555) {
                    while (--_0x5555) {
                        _0x2222.push(_0x2222.shift());
                    }
                };
                _0x4444(2);
            })(_0x1111, 0x123);
            function _0xdec(a) {
                return _0x1111[a];
            }
            console.log(_0xdec(0));
        "#;

        let (output, decoder) = run_decoder_inline(code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed());
        assert!(output.contains("\"c\""), "Should use rotated array");
    }

    #[test]
    fn test_base64_decoder() {
        use base64::{Engine, engine::general_purpose::STANDARD};

        let plain = "hello world";
        let encoded = STANDARD.encode(plain.as_bytes());

        let code = format!(
            r#"
            var _0xstr = ["{}"];
            function _0xdec(a) {{
                return atob(_0xstr[a]);
            }}
            console.log(_0xdec(0));
            "#,
            encoded
        );

        let (output, decoder) = run_decoder_inline(&code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed(), "Should have decoded base64");
        assert!(
            output.contains(&format!("\"{}\"", plain)),
            "Should contain decoded string"
        );
    }

    #[test]
    fn test_xor_decoder() {
        let plain = "HELLO";
        let key = b"\x01";
        let encoded: String = plain
            .chars()
            .map(|c| {
                let xored = (c as u8) ^ key[0];
                char::from(xored)
            })
            .collect();

        let code = format!(
            r#"
            var _0xstr = ["{}"];
            function _0xdec(a) {{
                return xorDecode(_0xstr[a], "\x01");
            }}
            console.log(_0xdec(0));
            "#,
            encoded
        );

        let (output, decoder) = run_decoder_inline(&code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed(), "Should have decoded XOR");
        assert!(
            output.contains(&format!("\"{}\"", plain)),
            "Should contain decoded string"
        );
    }

    #[test]
    fn test_rc4_decoder() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use rc4::{Key, KeyInit, Rc4, StreamCipher, consts::U8};

        let plain = "secret";
        let key = b"password";
        let key_arr = Key::<U8>::from_slice(key);
        let mut cipher = Rc4::<_>::new(key_arr);
        let mut encoded = plain.as_bytes().to_vec();
        cipher.apply_keystream(&mut encoded);

        let encoded_b64 = STANDARD.encode(&encoded);

        let code = format!(
            r#"
            var _0xstr = ["{}"];
            function _0xdec(a) {{
                return rc4Decode(_0xstr[a], "password");
            }}
            console.log(_0xdec(0));
            "#,
            encoded_b64
        );

        let (output, decoder) = run_decoder_inline(&code);
        eprintln!("Output:\n{}", output);

        assert_eq!(decoder.detected_decoders.len(), 1);
        assert!(decoder.has_changed(), "Should have decoded RC4 and base64");
        assert!(
            output.contains(&format!("\"{}\"", plain)),
            "Should contain decoded string"
        );
    }
}
