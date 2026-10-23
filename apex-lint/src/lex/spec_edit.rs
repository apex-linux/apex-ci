use super::{Kind, Lexer, non_blank_start};

const BLOCK_OPENERS: [&str; 2] = ["after ", "replace "];
const BLOCK_END: &str = "end";

#[derive(Debug, Default)]
pub(crate) struct SpecEdit {
    inside_block: bool,
}

impl Lexer for SpecEdit {
    fn next_line(&mut self, line: &[char]) -> Kind {
        let text: String = line.iter().collect();
        if self.inside_block {
            self.inside_block = text != BLOCK_END;
            return Kind::Code;
        }
        if non_blank_start(line).starts_with(&['#']) {
            return Kind::Comment;
        }
        self.inside_block = BLOCK_OPENERS.iter().any(|opener| text.starts_with(opener));
        Kind::Code
    }

    fn is_neutral(&self) -> bool {
        !self.inside_block
    }
}
