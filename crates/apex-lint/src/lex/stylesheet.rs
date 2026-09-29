use super::{Kind, Lexer, after_single_line_string, non_blank_start, starts_with};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Code,
    BlockComment,
    LoudCommentKeptInCompiledCss,
}

#[derive(Debug, Default)]
pub(crate) struct Stylesheet {
    state: State,
}

impl Stylesheet {
    fn scan(&mut self, line: &[char]) {
        let mut i = 0;
        while i < line.len() {
            i = match self.state {
                State::BlockComment | State::LoudCommentKeptInCompiledCss => {
                    if starts_with(line, i, "*/") {
                        self.state = State::Code;
                        i + 2
                    } else {
                        i + 1
                    }
                }
                State::Code => match line[i] {
                    '/' if starts_with(line, i, "//") => line.len(),
                    '/' if starts_with(line, i, "/*!") => {
                        self.state = State::LoudCommentKeptInCompiledCss;
                        i + 3
                    }
                    '/' if starts_with(line, i, "/*") => {
                        self.state = State::BlockComment;
                        i + 2
                    }
                    quote @ ('\'' | '"') => after_single_line_string(line, i + 1, quote, true),
                    _ => i + 1,
                },
            };
        }
    }
}

impl Lexer for Stylesheet {
    fn next_line(&mut self, line: &[char]) -> Kind {
        let start = non_blank_start(line);
        let kind = match self.state {
            State::BlockComment => Kind::Comment,
            State::Code if start.starts_with(&['/', '/']) => return Kind::Comment,
            State::Code
                if start.starts_with(&['/', '*']) && !start.starts_with(&['/', '*', '!']) =>
            {
                Kind::Comment
            }
            State::LoudCommentKeptInCompiledCss | State::Code => Kind::Code,
        };
        self.scan(line);
        kind
    }

    fn is_neutral(&self) -> bool {
        self.state == State::Code
    }
}
