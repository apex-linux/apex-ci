mod hash_lines;
mod javascript;
mod rust;
mod shell;
mod spec_edit;
mod stylesheet;
mod triple_quoted;

use crate::scope::Syntax;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Blank,
    Code,
    Comment,
    Directive,
    Exempt,
}

pub(crate) trait Lexer: std::fmt::Debug {
    fn next_line(&mut self, line: &[char]) -> Kind;
    fn is_neutral(&self) -> bool;
}

pub(crate) fn for_syntax(syntax: Syntax) -> Box<dyn Lexer> {
    match syntax {
        Syntax::ShellScript => Box::new(shell::Shell::script()),
        Syntax::ShellEmbedded | Syntax::Justfile => Box::new(shell::Shell::line_by_line()),
        Syntax::Containerfile => Box::new(shell::Shell::containerfile()),
        Syntax::SpecEdit => Box::new(spec_edit::SpecEdit::default()),
        Syntax::Python => Box::new(triple_quoted::TripleQuoted::python()),
        Syntax::Toml => Box::new(triple_quoted::TripleQuoted::toml()),
        Syntax::HashLines => Box::new(hash_lines::HashLines),
        Syntax::Rust => Box::new(rust::Rust::default()),
        Syntax::JavaScript => Box::new(javascript::JavaScript::default()),
        Syntax::Stylesheet => Box::new(stylesheet::Stylesheet::default()),
    }
}

fn at(line: &[char], index: usize) -> Option<char> {
    line.get(index).copied()
}

fn starts_with(line: &[char], index: usize, pattern: &str) -> bool {
    pattern
        .chars()
        .enumerate()
        .all(|(offset, expected)| at(line, index + offset) == Some(expected))
}

fn non_blank_start(line: &[char]) -> &[char] {
    let start = line
        .iter()
        .position(|c| !c.is_whitespace())
        .unwrap_or(line.len());
    &line[start..]
}

fn after_single_line_string(line: &[char], from: usize, quote: char, escapes: bool) -> usize {
    let mut i = from;
    while i < line.len() {
        if escapes && line[i] == '\\' {
            i += 2;
        } else if line[i] == quote {
            return i + 1;
        } else {
            i += 1;
        }
    }
    line.len()
}
