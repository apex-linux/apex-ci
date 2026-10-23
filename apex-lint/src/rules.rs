use std::fmt;

use crate::classify::Classified;
use crate::lex::Kind;

const LONGEST_COMMENT_BLOCK: usize = 2;
const DENSITY_APPLIES_FROM_NON_BLANK_LINES: usize = 40;
const DENSITY_LIMIT_PERCENT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rule {
    Block,
    Density,
    Unterminated,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Block => "block",
            Self::Density => "density",
            Self::Unterminated => "unterminated",
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Violation {
    pub(crate) line: usize,
    pub(crate) rule: Rule,
    pub(crate) message: String,
}

pub(crate) fn check(classified: &Classified) -> Vec<Violation> {
    let mut violations = Vec::new();
    if let Some(line) = classified.unterminated_from {
        violations.push(Violation {
            line,
            rule: Rule::Unterminated,
            message: "the file ends inside the string, block comment, here-document or spec-edit \
                      block opened here, so its lines cannot be classified"
                .to_owned(),
        });
    }
    violations.extend(comment_blocks(&classified.kinds));
    violations.extend(density(&classified.kinds));
    violations
}

#[derive(Debug, Clone, Copy)]
struct Block {
    first_line: usize,
    comment_lines: usize,
}

fn comment_blocks(kinds: &[Kind]) -> Vec<Violation> {
    let mut blocks = Vec::new();
    let mut open: Option<Block> = None;
    let mut file_header_may_end_at_a_blank = true;
    for (index, kind) in kinds.iter().enumerate() {
        match kind {
            Kind::Comment => {
                open.get_or_insert(Block {
                    first_line: index + 1,
                    comment_lines: 0,
                })
                .comment_lines += 1;
            }
            Kind::Blank if open.is_some() && file_header_may_end_at_a_blank => {
                blocks.extend(open.take());
                file_header_may_end_at_a_blank = false;
            }
            Kind::Blank | Kind::Directive => {}
            Kind::Exempt => blocks.extend(open.take()),
            Kind::Code => {
                blocks.extend(open.take());
                file_header_may_end_at_a_blank = false;
            }
        }
    }
    blocks.extend(open);
    blocks
        .into_iter()
        .filter(|block| block.comment_lines > LONGEST_COMMENT_BLOCK)
        .map(|block| Violation {
            line: block.first_line,
            rule: Rule::Block,
            message: format!(
                "{} comment lines with no code between them, at most {LONGEST_COMMENT_BLOCK}: let \
                 names say it and keep only a why the code cannot",
                block.comment_lines
            ),
        })
        .collect()
}

fn density(kinds: &[Kind]) -> Option<Violation> {
    let non_blank = kinds.iter().filter(|kind| **kind != Kind::Blank).count();
    let comments = kinds.iter().filter(|kind| **kind == Kind::Comment).count();
    let too_dense = non_blank >= DENSITY_APPLIES_FROM_NON_BLANK_LINES
        && comments * 100 > non_blank * DENSITY_LIMIT_PERCENT;
    too_dense.then(|| Violation {
        line: 1,
        rule: Rule::Density,
        message: format!(
            "{comments} of {non_blank} non-blank lines are comments ({}%), at most \
             {DENSITY_LIMIT_PERCENT}%",
            comments * 100 / non_blank
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify;
    use crate::scope::Syntax;

    fn violations(lines: &[String]) -> Vec<(usize, Rule)> {
        violations_in(Syntax::ShellScript, lines)
    }

    fn violations_in(syntax: Syntax, lines: &[String]) -> Vec<(usize, Rule)> {
        check(&classify(syntax, &(lines.join("\n") + "\n")))
            .into_iter()
            .map(|violation| (violation.line, violation.rule))
            .collect()
    }

    fn owned(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    fn code_with_comments(non_blank: usize, comments: usize) -> Vec<String> {
        (0..non_blank)
            .map(|index| {
                if index % 2 == 1 && index / 2 < comments {
                    "# why".to_owned()
                } else {
                    "echo".to_owned()
                }
            })
            .collect()
    }

    #[test]
    fn two_comment_lines_pass_and_three_fail_at_the_first() {
        assert_eq!(violations(&owned(&["# a", "# b", "echo"])), []);
        assert_eq!(
            violations(&owned(&["echo", "# a", "# b", "# c"])),
            [(2, Rule::Block)]
        );
    }

    #[test]
    fn blank_lines_do_not_split_a_block() {
        assert_eq!(
            violations(&owned(&["echo", "# a", "# b", "", "# c", "echo"])),
            [(2, Rule::Block)]
        );
        assert_eq!(
            violations(&owned(&["echo", "# a", "", "", "# b", "echo"])),
            []
        );
    }

    #[test]
    fn only_the_file_header_ends_at_its_first_blank_line() {
        assert_eq!(
            violations(&owned(&["# header", "", "# why a", "# why b", "echo"])),
            []
        );
        assert_eq!(
            violations(&owned(&["# a", "# b", "", "# c", "# d", "", "# e", "echo"])),
            [(4, Rule::Block)]
        );
    }

    #[test]
    fn tool_directives_neither_count_nor_split_a_block() {
        assert_eq!(
            violations(&owned(&[
                "echo",
                "# shellcheck disable=SC2034",
                "# why a",
                "# why b",
                "echo"
            ])),
            []
        );
        assert_eq!(
            violations(&owned(&[
                "echo",
                "# a",
                "# shellcheck disable=SC2034",
                "# b",
                "# c",
                "echo"
            ])),
            [(2, Rule::Block)]
        );
    }

    #[test]
    fn the_spdx_line_and_its_blank_comment_line_are_not_part_of_a_block() {
        let header = [
            "#!/bin/sh",
            "# SPDX-License-Identifier: GPL-3.0-or-later",
            "#",
        ];
        let mut two = owned(&header);
        two.extend(owned(&["# a", "# b", "echo"]));
        assert_eq!(violations(&two), []);
        let mut three = owned(&header);
        three.extend(owned(&["# a", "# b", "# c"]));
        assert_eq!(violations(&three), [(4, Rule::Block)]);
    }

    #[test]
    fn a_justfile_recipe_doc_is_not_part_of_the_why_above_it() {
        let recipe = owned(&[
            "check:",
            "    true",
            "",
            "# why a",
            "# why b",
            "# Doc of build, which just --list prints.",
            "[positional-arguments]",
            "build *args:",
            "    # body a",
            "    # body b",
            "    # body c",
            "    make",
        ]);
        assert_eq!(violations_in(Syntax::Justfile, &recipe), [(9, Rule::Block)]);
    }

    #[test]
    fn density_applies_from_forty_non_blank_lines_above_eight_percent() {
        assert_eq!(violations(&code_with_comments(39, 10)), []);
        assert_eq!(violations(&code_with_comments(40, 3)), []);
        assert_eq!(violations(&code_with_comments(40, 4)), [(1, Rule::Density)]);
    }

    #[test]
    fn an_unterminated_here_document_is_a_violation() {
        assert_eq!(
            violations(&owned(&["echo", "cat <<EOF", "# body"])),
            [(2, Rule::Unterminated)]
        );
    }
}
