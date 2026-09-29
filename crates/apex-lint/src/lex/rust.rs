use super::{Kind, Lexer, at, non_blank_start, starts_with};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Code,
    BlockComment {
        depth: u32,
    },
    Str,
    RawStr {
        hashes: usize,
    },
}

const DOC_ATTRIBUTES: [&str; 2] = ["#[doc", "#![doc"];

#[derive(Debug, Default)]
pub(crate) struct Rust {
    state: State,
    doc_string_open: bool,
}

impl Rust {
    fn scan(&mut self, line: &[char]) {
        let mut i = 0;
        while i < line.len() {
            i = match self.state {
                State::Code => self.step_code(line, i),
                State::BlockComment { depth } => self.step_block_comment(line, i, depth),
                State::Str => self.step_string(line, i),
                State::RawStr { hashes } => self.step_raw_string(line, i, hashes),
            };
        }
    }

    fn step_code(&mut self, line: &[char], i: usize) -> usize {
        match line[i] {
            '/' if at(line, i + 1) == Some('/') => line.len(),
            '/' if at(line, i + 1) == Some('*') => {
                self.state = State::BlockComment { depth: 1 };
                i + 2
            }
            '"' => {
                self.state = State::Str;
                i + 1
            }
            '\'' => after_char_literal_or_lifetime(line, i),
            'r' if starts_raw_string_prefix(line, i) => {
                let hashes = line[i + 1..].iter().take_while(|c| **c == '#').count();
                if at(line, i + 1 + hashes) == Some('"') {
                    self.state = State::RawStr { hashes };
                    i + 2 + hashes
                } else {
                    i + 1
                }
            }
            _ => i + 1,
        }
    }

    fn step_block_comment(&mut self, line: &[char], i: usize, depth: u32) -> usize {
        if starts_with(line, i, "/*") {
            self.state = State::BlockComment { depth: depth + 1 };
            i + 2
        } else if starts_with(line, i, "*/") {
            self.state = match depth {
                1 => State::Code,
                _ => State::BlockComment { depth: depth - 1 },
            };
            i + 2
        } else {
            i + 1
        }
    }

    fn step_string(&mut self, line: &[char], i: usize) -> usize {
        match line[i] {
            '\\' => i + 2,
            '"' => {
                self.state = State::Code;
                i + 1
            }
            _ => i + 1,
        }
    }

    fn step_raw_string(&mut self, line: &[char], i: usize, hashes: usize) -> usize {
        let closes = line[i] == '"' && (1..=hashes).all(|offset| at(line, i + offset) == Some('#'));
        if closes {
            self.state = State::Code;
            i + 1 + hashes
        } else {
            i + 1
        }
    }
}

impl Lexer for Rust {
    fn next_line(&mut self, line: &[char]) -> Kind {
        let kind = match self.state {
            State::BlockComment { .. } => Kind::Comment,
            State::Str | State::RawStr { .. } if self.doc_string_open => Kind::Comment,
            State::Str | State::RawStr { .. } => Kind::Code,
            State::Code => {
                let start = non_blank_start(line);
                if start.starts_with(&['/', '/']) {
                    return Kind::Comment;
                }
                self.doc_string_open = is_doc_attribute(start);
                if start.starts_with(&['/', '*']) || self.doc_string_open {
                    Kind::Comment
                } else {
                    Kind::Code
                }
            }
        };
        self.scan(line);
        if self.state == State::Code {
            self.doc_string_open = false;
        }
        kind
    }

    fn is_neutral(&self) -> bool {
        self.state == State::Code
    }
}

fn is_doc_attribute(start: &[char]) -> bool {
    let text: String = start.iter().filter(|c| !c.is_whitespace()).collect();
    DOC_ATTRIBUTES
        .iter()
        .any(|attribute| text.starts_with(&format!("{attribute}=")))
}

fn is_identifier(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn starts_token(line: &[char], i: usize) -> bool {
    i == 0 || !is_identifier(line[i - 1])
}

fn starts_raw_string_prefix(line: &[char], i: usize) -> bool {
    starts_token(line, i)
        || (i > 0 && matches!(line[i - 1], 'b' | 'c') && starts_token(line, i - 1))
}

fn after_char_literal_or_lifetime(line: &[char], i: usize) -> usize {
    if at(line, i + 1) == Some('\\') {
        return line
            .get(i + 3..)
            .unwrap_or(&[])
            .iter()
            .position(|c| *c == '\'')
            .map_or(line.len(), |close| i + 4 + close);
    }
    if at(line, i + 2) == Some('\'') {
        return i + 3;
    }
    i + 1
}
