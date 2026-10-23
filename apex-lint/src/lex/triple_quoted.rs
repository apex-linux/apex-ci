use super::{Kind, Lexer, after_single_line_string, at, non_blank_start, starts_with};

#[derive(Debug, Clone, Copy)]
struct OpenString {
    quote: char,
    escapes: bool,
}

#[derive(Debug)]
pub(crate) struct TripleQuoted {
    single_quotes_are_literal: bool,
    open: Option<OpenString>,
}

impl TripleQuoted {
    pub(crate) fn python() -> Self {
        Self {
            single_quotes_are_literal: false,
            open: None,
        }
    }

    pub(crate) fn toml() -> Self {
        Self {
            single_quotes_are_literal: true,
            open: None,
        }
    }

    fn escapes(&self, quote: char) -> bool {
        !(self.single_quotes_are_literal && quote == '\'')
    }

    fn scan(&mut self, line: &[char]) {
        let mut i = 0;
        while i < line.len() {
            i = match self.open {
                Some(open) => self.step_inside(line, i, open),
                None => self.step_code(line, i),
            };
        }
    }

    fn step_inside(&mut self, line: &[char], i: usize, open: OpenString) -> usize {
        let closing: String = [open.quote; 3].iter().collect();
        if open.escapes && line[i] == '\\' {
            i + 2
        } else if starts_with(line, i, &closing) {
            self.open = None;
            i + 3
        } else {
            i + 1
        }
    }

    fn step_code(&mut self, line: &[char], i: usize) -> usize {
        match line[i] {
            '#' => line.len(),
            quote @ ('\'' | '"') => {
                let escapes = self.escapes(quote);
                if at(line, i + 1) == Some(quote) && at(line, i + 2) == Some(quote) {
                    self.open = Some(OpenString { quote, escapes });
                    i + 3
                } else {
                    after_single_line_string(line, i + 1, quote, escapes)
                }
            }
            _ => i + 1,
        }
    }
}

impl Lexer for TripleQuoted {
    fn next_line(&mut self, line: &[char]) -> Kind {
        if self.open.is_none() && non_blank_start(line).starts_with(&['#']) {
            return Kind::Comment;
        }
        self.scan(line);
        Kind::Code
    }

    fn is_neutral(&self) -> bool {
        self.open.is_none()
    }
}
