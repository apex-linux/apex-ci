use super::{Kind, Lexer, after_single_line_string, at, non_blank_start, starts_with};

const KEYWORDS_BEFORE_AN_EXPRESSION: [&str; 10] = [
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    BlockComment,
    Template,
    Interpolation { open_braces: u32 },
}

#[derive(Debug)]
pub(crate) struct JavaScript {
    contexts: Vec<Context>,
    slash_starts_regex: bool,
}

impl Default for JavaScript {
    fn default() -> Self {
        Self {
            contexts: Vec::new(),
            slash_starts_regex: true,
        }
    }
}

impl JavaScript {
    fn scan(&mut self, line: &[char]) {
        let mut i = 0;
        while i < line.len() {
            i = match self.contexts.last().copied() {
                None | Some(Context::Interpolation { .. }) => self.step_code(line, i),
                Some(Context::BlockComment) => self.step_block_comment(line, i),
                Some(Context::Template) => self.step_template(line, i),
            };
        }
    }

    fn step_code(&mut self, line: &[char], i: usize) -> usize {
        let c = line[i];
        if c.is_whitespace() {
            return i + 1;
        }
        if c.is_alphanumeric() || c == '_' || c == '$' {
            let end = i + line[i..]
                .iter()
                .take_while(|c| c.is_alphanumeric() || **c == '_' || **c == '$')
                .count();
            let word: String = line[i..end].iter().collect();
            self.slash_starts_regex = KEYWORDS_BEFORE_AN_EXPRESSION.contains(&word.as_str());
            return end;
        }
        let (next, slash_starts_regex) = match c {
            '/' if at(line, i + 1) == Some('/') => return line.len(),
            '/' if at(line, i + 1) == Some('*') => {
                self.contexts.push(Context::BlockComment);
                return i + 2;
            }
            '/' if self.slash_starts_regex => (after_regex(line, i + 1), false),
            '\'' | '"' => (after_single_line_string(line, i + 1, c, true), false),
            '`' => {
                self.contexts.push(Context::Template);
                (i + 1, false)
            }
            '{' => {
                if let Some(Context::Interpolation { open_braces }) = self.contexts.last_mut() {
                    *open_braces += 1;
                }
                (i + 1, true)
            }
            '}' => {
                match self.contexts.last_mut() {
                    Some(Context::Interpolation { open_braces: 0 }) => {
                        self.contexts.pop();
                    }
                    Some(Context::Interpolation { open_braces }) => *open_braces -= 1,
                    _ => {}
                }
                (i + 1, false)
            }
            ')' | ']' => (i + 1, false),
            _ => (i + 1, true),
        };
        self.slash_starts_regex = slash_starts_regex;
        next
    }

    fn step_block_comment(&mut self, line: &[char], i: usize) -> usize {
        if starts_with(line, i, "*/") {
            self.contexts.pop();
            i + 2
        } else {
            i + 1
        }
    }

    fn step_template(&mut self, line: &[char], i: usize) -> usize {
        match line[i] {
            '\\' => i + 2,
            '`' => {
                self.contexts.pop();
                self.slash_starts_regex = false;
                i + 1
            }
            '$' if at(line, i + 1) == Some('{') => {
                self.contexts
                    .push(Context::Interpolation { open_braces: 0 });
                self.slash_starts_regex = true;
                i + 2
            }
            _ => i + 1,
        }
    }
}

impl Lexer for JavaScript {
    fn next_line(&mut self, line: &[char]) -> Kind {
        let kind = match self.contexts.last() {
            Some(Context::BlockComment) => Kind::Comment,
            Some(Context::Template) => Kind::Code,
            None | Some(Context::Interpolation { .. }) => {
                let start = non_blank_start(line);
                if start.starts_with(&['/', '/']) {
                    return Kind::Comment;
                }
                if start.starts_with(&['/', '*']) {
                    Kind::Comment
                } else {
                    Kind::Code
                }
            }
        };
        self.scan(line);
        kind
    }

    fn is_neutral(&self) -> bool {
        self.contexts.is_empty()
    }
}

fn after_regex(line: &[char], from: usize) -> usize {
    let mut in_class = false;
    let mut i = from;
    while i < line.len() {
        match line[i] {
            '\\' => i += 1,
            '[' => in_class = true,
            ']' => in_class = false,
            '/' if !in_class => return i + 1,
            _ => {}
        }
        i += 1;
    }
    line.len()
}
