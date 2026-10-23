use super::{Kind, Lexer, non_blank_start};

#[derive(Debug)]
pub(crate) struct HashLines;

impl Lexer for HashLines {
    fn next_line(&mut self, line: &[char]) -> Kind {
        if non_blank_start(line).starts_with(&['#']) {
            Kind::Comment
        } else {
            Kind::Code
        }
    }

    fn is_neutral(&self) -> bool {
        true
    }
}
