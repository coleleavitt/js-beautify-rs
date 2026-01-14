pub struct Output {
    lines: Vec<String>,
    current_line: String,
    indent_level: usize,
    indent_string: String,
    newline_count: usize,
}

impl Output {
    pub fn new(indent_string: String) -> Self {
        Self {
            lines: Vec::new(),
            current_line: String::new(),
            indent_level: 0,
            indent_string,
            newline_count: 0,
        }
    }

    pub fn add_token(&mut self, text: &str) {
        self.current_line.push_str(text);
        self.newline_count = 0;
    }

    pub fn add_newline(&mut self) {
        if !self.current_line.is_empty() {
            self.lines.push(self.current_line.clone());
            self.current_line.clear();
        }
        self.newline_count += 1;
    }

    pub fn add_indent(&mut self) {
        self.indent_level += 1;
    }

    pub fn remove_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    pub fn get_indent(&self) -> String {
        self.indent_string.repeat(self.indent_level)
    }

    pub fn to_string(&self) -> String {
        let mut result = self.lines.join("\n");
        if !self.current_line.is_empty() {
            result.push('\n');
            result.push_str(&self.current_line);
        }
        result
    }
}
