pub struct Output {
    lines: Vec<String>,
    current_line: String,
    indent_level: usize,
    indent_string: String,
    newline_count: usize,
    at_line_start: bool,
}

impl Output {
    pub fn new(indent_string: String) -> Self {
        Self {
            lines: Vec::new(),
            current_line: String::new(),
            indent_level: 0,
            indent_string,
            newline_count: 0,
            at_line_start: true,
        }
    }

    pub fn add_token(&mut self, text: &str) {
        if self.at_line_start && !text.is_empty() && !text.trim().is_empty() {
            self.current_line.push_str(&self.get_indent());
            self.at_line_start = false;
        }
        self.current_line.push_str(text);
        self.newline_count = 0;
    }

    pub fn add_template_literal(&mut self, text: &str) {
        let mut lines_iter = text.split('\n');

        if let Some(first_line) = lines_iter.next() {
            if self.at_line_start && !first_line.is_empty() && !first_line.trim().is_empty() {
                self.current_line.push_str(&self.get_indent());
                self.at_line_start = false;
            }
            self.current_line.push_str(first_line);
        }

        for line in lines_iter {
            self.lines.push(self.current_line.clone());
            self.current_line.clear();
            self.current_line.push_str(line);
            self.at_line_start = false;
        }

        self.newline_count = 0;
    }

    pub fn add_space(&mut self) {
        if !self.current_line.is_empty() && !self.current_line.ends_with(' ') {
            self.current_line.push(' ');
        }
    }

    pub fn add_newline(&mut self) {
        if !self.current_line.is_empty() || self.newline_count == 0 {
            self.lines.push(self.current_line.clone());
            self.current_line.clear();
            self.at_line_start = true;
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

    pub fn current_line_length(&self) -> usize {
        self.current_line.len()
    }

    pub fn line_exceeds_length(&self, max_length: usize) -> bool {
        self.current_line.len() > max_length
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
