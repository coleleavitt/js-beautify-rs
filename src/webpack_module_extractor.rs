use crate::BeautifyError;
use crate::Result;
use crate::token::{Token, TokenType};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(debug_assertions)]
macro_rules! trace_webpack {
    ($($arg:tt)*) => {
        eprintln!("[WEBPACK] {}", format!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
macro_rules! trace_webpack {
    ($($arg:tt)*) => {};
}

#[derive(Debug, Clone)]
pub struct WebpackModule {
    pub id: usize,
    pub start_pos: usize,
    pub end_pos: usize,
    pub dependencies: Vec<usize>,
}

pub struct ModuleExtractor {
    modules: HashMap<usize, WebpackModule>,
}

impl ModuleExtractor {
    pub fn new() -> Self {
        trace_webpack!("initializing ModuleExtractor");
        Self {
            modules: HashMap::new(),
        }
    }

    pub fn extract_modules(&mut self, tokens: &[Token]) -> Result<()> {
        trace_webpack!("=== EXTRACTING WEBPACK MODULES ===");
        trace_webpack!("total tokens: {}", tokens.len());

        self.find_module_map(tokens)?;

        trace_webpack!("found {} modules", self.modules.len());
        Ok(())
    }

    fn find_module_map(&mut self, tokens: &[Token]) -> Result<()> {
        let mut i = 0usize;

        while i < tokens.len() {
            if let Some(map_start) = self.try_find_module_map_at(tokens, i)? {
                trace_webpack!("found module map at position {}", map_start);
                self.parse_module_map(tokens, map_start)?;
                return Ok(());
            }

            i = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
        }

        trace_webpack!("no module map found");
        Ok(())
    }

    fn try_find_module_map_at(&self, tokens: &[Token], i: usize) -> Result<Option<usize>> {
        if i.checked_add(10)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?
            >= tokens.len()
        {
            return Ok(None);
        }

        if tokens[i].token_type != TokenType::Equals {
            return Ok(None);
        }

        let next = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

        if tokens[next].token_type != TokenType::StartBlock {
            return Ok(None);
        }

        let after_brace = next
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

        if after_brace >= tokens.len() {
            return Ok(None);
        }

        if tokens[after_brace].token_type == TokenType::Number {
            let colon_pos = after_brace
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

            if colon_pos < tokens.len() && tokens[colon_pos].token_type == TokenType::Colon {
                return Ok(Some(next));
            }
        }

        Ok(None)
    }

    fn parse_module_map(&mut self, tokens: &[Token], start: usize) -> Result<()> {
        debug_assert!(tokens[start].token_type == TokenType::StartBlock);

        let mut i = start
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

        while i < tokens.len() {
            if tokens[i].token_type == TokenType::EndBlock {
                break;
            }

            if tokens[i].token_type == TokenType::Number {
                if let Some(module) = self.try_parse_module(tokens, i)? {
                    self.modules.insert(module.id, module);
                    i = i.checked_add(1).ok_or_else(|| {
                        BeautifyError::BeautificationFailed("overflow".to_string())
                    })?;
                    continue;
                }
            }

            i = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
        }

        Ok(())
    }

    fn try_parse_module(&self, tokens: &[Token], i: usize) -> Result<Option<WebpackModule>> {
        if tokens[i].token_type != TokenType::Number {
            return Ok(None);
        }

        let module_id = tokens[i].text.parse::<usize>().ok();
        if module_id.is_none() {
            return Ok(None);
        }
        let module_id = module_id.unwrap();

        let colon_pos = i
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

        if colon_pos >= tokens.len() || tokens[colon_pos].token_type != TokenType::Colon {
            return Ok(None);
        }

        let func_pos = colon_pos
            .checked_add(1)
            .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

        if func_pos >= tokens.len() {
            return Ok(None);
        }

        if tokens[func_pos].token_type == TokenType::Reserved && tokens[func_pos].text == "function"
        {
            let func_end = self.find_function_end(tokens, func_pos)?;
            if func_end.is_none() {
                return Ok(None);
            }
            let func_end = func_end.unwrap();

            trace_webpack!(
                "parsed module {}: positions {}..{}",
                module_id,
                func_pos,
                func_end
            );

            return Ok(Some(WebpackModule {
                id: module_id,
                start_pos: func_pos,
                end_pos: func_end,
                dependencies: Vec::new(),
            }));
        }

        Ok(None)
    }

    fn find_function_end(&self, tokens: &[Token], start: usize) -> Result<Option<usize>> {
        debug_assert!(tokens[start].token_type == TokenType::Reserved);
        debug_assert!(tokens[start].text == "function");

        let mut i = start;
        let mut depth = 0usize;
        let mut found_first_brace = false;

        while i < tokens.len() {
            if tokens[i].token_type == TokenType::StartBlock {
                depth = depth.checked_add(1).ok_or_else(|| {
                    BeautifyError::BeautificationFailed("depth overflow".to_string())
                })?;
                found_first_brace = true;
            } else if tokens[i].token_type == TokenType::EndBlock {
                if depth > 0 {
                    depth = depth.checked_sub(1).ok_or_else(|| {
                        BeautifyError::BeautificationFailed("depth underflow".to_string())
                    })?;

                    if found_first_brace && depth == 0 {
                        return Ok(Some(i));
                    }
                }
            }

            i = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("index overflow".to_string()))?;
        }

        Ok(None)
    }

    pub fn write_modules(&self, tokens: &[Token], output_dir: &Path) -> Result<()> {
        trace_webpack!("=== WRITING MODULES ===");
        trace_webpack!("output directory: {}", output_dir.display());

        fs::create_dir_all(output_dir).map_err(|e| {
            BeautifyError::BeautificationFailed(format!("failed to create output directory: {}", e))
        })?;

        for (id, module) in &self.modules {
            let filename = format!("module_{}.js", id);
            let output_path = output_dir.join(&filename);

            let module_tokens: Vec<Token> = tokens[module.start_pos..=module.end_pos]
                .iter()
                .cloned()
                .collect();

            let module_code = self.tokens_to_string(&module_tokens);

            fs::write(&output_path, module_code).map_err(|e| {
                BeautifyError::BeautificationFailed(format!("failed to write module {}: {}", id, e))
            })?;

            trace_webpack!("wrote module {} to {}", id, filename);
        }

        trace_webpack!("wrote {} modules", self.modules.len());
        Ok(())
    }

    fn tokens_to_string(&self, tokens: &[Token]) -> String {
        tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn extract_dependencies(&mut self, tokens: &[Token]) -> Result<()> {
        trace_webpack!("=== EXTRACTING DEPENDENCIES ===");

        for (id, module) in self.modules.clone().iter() {
            let deps = self.find_dependencies_in_range(tokens, module.start_pos, module.end_pos)?;

            if let Some(m) = self.modules.get_mut(id) {
                m.dependencies = deps.clone();
                trace_webpack!("module {} has {} dependencies", id, deps.len());
            }
        }

        Ok(())
    }

    fn find_dependencies_in_range(
        &self,
        tokens: &[Token],
        start: usize,
        end: usize,
    ) -> Result<Vec<usize>> {
        let mut deps = Vec::new();
        let mut i = start;

        while i <= end && i < tokens.len() {
            if tokens[i].token_type == TokenType::Word && tokens[i].text == "t" {
                let paren = i
                    .checked_add(1)
                    .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;

                if paren < tokens.len() && tokens[paren].token_type == TokenType::StartExpr {
                    let num_pos = paren.checked_add(1).ok_or_else(|| {
                        BeautifyError::BeautificationFailed("overflow".to_string())
                    })?;

                    if num_pos < tokens.len() && tokens[num_pos].token_type == TokenType::Number {
                        if let Ok(dep_id) = tokens[num_pos].text.parse::<usize>() {
                            if !deps.contains(&dep_id) {
                                deps.push(dep_id);
                            }
                        }
                    }
                }
            }

            i = i
                .checked_add(1)
                .ok_or_else(|| BeautifyError::BeautificationFailed("overflow".to_string()))?;
        }

        Ok(deps)
    }

    pub fn generate_dependency_graph(&self, output_path: &Path) -> Result<()> {
        trace_webpack!("=== GENERATING DEPENDENCY GRAPH ===");

        let mut graph = String::from("digraph webpack_modules {\n");
        graph.push_str("  rankdir=LR;\n");
        graph.push_str("  node [shape=box];\n\n");

        for (id, module) in &self.modules {
            graph.push_str(&format!("  module_{} [label=\"Module {}\"];\n", id, id));
        }

        graph.push_str("\n");

        for (id, module) in &self.modules {
            for dep in &module.dependencies {
                graph.push_str(&format!("  module_{} -> module_{};\n", id, dep));
            }
        }

        graph.push_str("}\n");

        fs::write(output_path, graph).map_err(|e| {
            BeautifyError::BeautificationFailed(format!("failed to write dependency graph: {}", e))
        })?;

        trace_webpack!("wrote dependency graph to {}", output_path.display());
        Ok(())
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }
}
