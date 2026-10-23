use std::collections::VecDeque;

use super::{Kind, Lexer, at, for_syntax, non_blank_start};
use crate::scope::{Syntax, by_shebang, interpreter_syntax};

const WORD_SEPARATORS: [char; 8] = ['(', ')', ';', '|', '&', '=', '`', '!'];
const SURROUNDING_QUOTES: [char; 2] = ['"', '\''];
const CONTAINERFILE_SHELL_INSTRUCTION: &str = "RUN";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    SingleQuotes,
    AnsiCQuotes,
    DoubleQuotes,
    Backticks,
    CommandSubstitution { open_parens: u32 },
    Arithmetic { open_parens: u32 },
    ParameterExpansion,
}

impl Context {
    fn is_literal(self) -> bool {
        matches!(
            self,
            Self::SingleQuotes | Self::AnsiCQuotes | Self::DoubleQuotes | Self::Backticks
        )
    }
}

#[derive(Debug)]
struct HereDocument {
    terminator: String,
    script: Option<Box<dyn Lexer>>,
    at_first_body_line: bool,
}

impl HereDocument {
    fn ends_at(&self, line: &[char]) -> bool {
        line.iter().collect::<String>().trim() == self.terminator
    }

    fn body_line(&mut self, line: &[char]) -> Kind {
        let first = std::mem::replace(&mut self.at_first_body_line, false);
        if first {
            let text: String = line.iter().collect();
            if let Some(syntax) = by_shebang(&text) {
                self.script.get_or_insert_with(|| for_syntax(syntax));
                return Kind::Code;
            }
        }
        self.script
            .as_mut()
            .map_or(Kind::Code, |script| script.next_line(line))
    }
}

#[derive(Debug)]
pub(crate) struct Shell {
    contexts: Vec<Context>,
    here_documents: VecDeque<HereDocument>,
    quotes_span_lines: bool,
    in_containerfile: bool,
    continued_command: String,
}

impl Shell {
    pub(crate) fn script() -> Self {
        Self {
            contexts: Vec::new(),
            here_documents: VecDeque::new(),
            quotes_span_lines: true,
            in_containerfile: false,
            continued_command: String::new(),
        }
    }

    pub(crate) fn line_by_line() -> Self {
        Self {
            quotes_span_lines: false,
            ..Self::script()
        }
    }

    pub(crate) fn containerfile() -> Self {
        Self {
            in_containerfile: true,
            ..Self::line_by_line()
        }
    }

    fn interpreter_of(&self, command: &str) -> Option<Syntax> {
        let mut words = command
            .split(|c: char| c.is_whitespace() || WORD_SEPARATORS.contains(&c))
            .map(|word| word.trim_matches(SURROUNDING_QUOTES.as_slice()))
            .filter(|word| !word.is_empty());
        let first = words.next()?;
        if self.in_containerfile && first.eq_ignore_ascii_case(CONTAINERFILE_SHELL_INSTRUCTION) {
            return Some(Syntax::ShellScript);
        }
        std::iter::once(first)
            .chain(words)
            .filter(|word| !word.starts_with(['-', '<', '>', '$']))
            .find_map(interpreter_syntax)
    }

    fn scan(&mut self, line: &[char]) -> Vec<String> {
        let mut announced = Vec::new();
        let mut i = 0;
        while i < line.len() {
            i = match self.contexts.last().copied() {
                None | Some(Context::CommandSubstitution { .. }) => {
                    self.step_code(line, i, &mut announced)
                }
                Some(Context::SingleQuotes) => self.step_until(line, i, '\'', false),
                Some(Context::AnsiCQuotes) => self.step_until(line, i, '\'', true),
                Some(Context::Backticks) => self.step_until(line, i, '`', true),
                Some(Context::DoubleQuotes) => self.step_double_quotes(line, i),
                Some(Context::Arithmetic { .. }) => self.step_arithmetic(line, i),
                Some(Context::ParameterExpansion) => self.step_parameter(line, i),
            };
        }
        announced
    }

    fn enter(&mut self, context: Context, next: usize) -> usize {
        self.contexts.push(context);
        next
    }

    fn leave(&mut self, next: usize) -> usize {
        self.contexts.pop();
        next
    }

    fn step_code(&mut self, line: &[char], i: usize, announced: &mut Vec<String>) -> usize {
        match line[i] {
            '\\' => i + 2,
            '#' if starts_word(line, i) => line.len(),
            '\'' => self.enter(Context::SingleQuotes, i + 1),
            '"' => self.enter(Context::DoubleQuotes, i + 1),
            '`' => self.enter(Context::Backticks, i + 1),
            '$' => self.step_dollar(line, i, true),
            '(' if at(line, i + 1) == Some('(') && starts_word(line, i) => {
                self.enter(Context::Arithmetic { open_parens: 2 }, i + 2)
            }
            '(' => {
                if let Some(Context::CommandSubstitution { open_parens }) = self.contexts.last_mut()
                {
                    *open_parens += 1;
                }
                i + 1
            }
            ')' => {
                if let Some(Context::CommandSubstitution { open_parens }) = self.contexts.last_mut()
                {
                    *open_parens -= 1;
                    if *open_parens == 0 {
                        self.contexts.pop();
                    }
                }
                i + 1
            }
            '<' if at(line, i + 1) == Some('<') => here_document(line, i, announced),
            _ => i + 1,
        }
    }

    fn step_dollar(&mut self, line: &[char], i: usize, ansi_c_quotes_open: bool) -> usize {
        match (at(line, i + 1), at(line, i + 2)) {
            (Some('\''), _) if ansi_c_quotes_open => self.enter(Context::AnsiCQuotes, i + 2),
            (Some('('), Some('(')) => self.enter(Context::Arithmetic { open_parens: 2 }, i + 3),
            (Some('('), _) => self.enter(Context::CommandSubstitution { open_parens: 1 }, i + 2),
            (Some('{'), _) => self.enter(Context::ParameterExpansion, i + 2),
            _ => i + 1,
        }
    }

    fn step_until(&mut self, line: &[char], i: usize, closing: char, escapes: bool) -> usize {
        match line[i] {
            '\\' if escapes => i + 2,
            c if c == closing => self.leave(i + 1),
            _ => i + 1,
        }
    }

    fn step_double_quotes(&mut self, line: &[char], i: usize) -> usize {
        match line[i] {
            '\\' => i + 2,
            '"' => self.leave(i + 1),
            '`' => self.enter(Context::Backticks, i + 1),
            '$' => self.step_dollar(line, i, false),
            _ => i + 1,
        }
    }

    fn step_arithmetic(&mut self, line: &[char], i: usize) -> usize {
        if let Some(Context::Arithmetic { open_parens }) = self.contexts.last_mut() {
            match line[i] {
                '(' => *open_parens += 1,
                ')' => *open_parens -= 1,
                _ => {}
            }
            if *open_parens == 0 {
                self.contexts.pop();
            }
        }
        i + 1
    }

    fn step_parameter(&mut self, line: &[char], i: usize) -> usize {
        match line[i] {
            '\\' => i + 2,
            '}' => self.leave(i + 1),
            '\'' => self.enter(Context::SingleQuotes, i + 1),
            '"' => self.enter(Context::DoubleQuotes, i + 1),
            '$' => self.step_dollar(line, i, false),
            _ => i + 1,
        }
    }
}

impl Lexer for Shell {
    fn next_line(&mut self, line: &[char]) -> Kind {
        if let Some(body) = self.here_documents.front_mut() {
            if body.ends_at(line) {
                self.here_documents.pop_front();
                return Kind::Code;
            }
            return body.body_line(line);
        }
        let inside_literal = self.contexts.last().is_some_and(|c| c.is_literal());
        if !inside_literal && non_blank_start(line).starts_with(&['#']) {
            return Kind::Comment;
        }
        let text: String = line.iter().collect();
        let command = std::mem::take(&mut self.continued_command) + &text;
        let terminators = self.scan(line);
        let script = self.interpreter_of(&command);
        self.here_documents
            .extend(terminators.into_iter().map(|terminator| HereDocument {
                terminator,
                script: script.map(for_syntax),
                at_first_body_line: true,
            }));
        if let Some(continued) = command.trim_end().strip_suffix('\\') {
            continued.clone_into(&mut self.continued_command);
        }
        if !self.quotes_span_lines {
            self.contexts.clear();
        }
        Kind::Code
    }

    fn is_neutral(&self) -> bool {
        self.contexts.is_empty() && self.here_documents.is_empty()
    }
}

fn starts_word(line: &[char], i: usize) -> bool {
    i == 0 || line[i - 1].is_whitespace() || matches!(line[i - 1], ';' | '&' | '|' | '(' | ')')
}

fn here_document(line: &[char], i: usize, announced: &mut Vec<String>) -> usize {
    if at(line, i + 2) == Some('<') {
        return i + 3;
    }
    let mut j = i + 2;
    if at(line, j) == Some('-') {
        j += 1;
    }
    while matches!(at(line, j), Some(' ' | '\t')) {
        j += 1;
    }
    let (start, end, next) = match at(line, j) {
        Some(quote @ ('\'' | '"')) => {
            let Some(close) = line[j + 1..].iter().position(|&c| c == quote) else {
                return i + 2;
            };
            (j + 1, j + 1 + close, j + 2 + close)
        }
        Some('\\') => {
            let end = word_end(line, j + 1);
            (j + 1, end, end)
        }
        _ => {
            let end = word_end(line, j);
            (j, end, end)
        }
    };
    if start == end {
        return i + 2;
    }
    announced.push(line[start..end].iter().collect());
    next
}

fn word_end(line: &[char], from: usize) -> usize {
    from + line[from..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
        .count()
}
