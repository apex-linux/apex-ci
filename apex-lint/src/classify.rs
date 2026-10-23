use crate::lex::{self, Kind};
use crate::scope::Syntax;

const SPDX_TAG: &str = "SPDX-License-Identifier:";
const TOOL_DIRECTIVES: [&str; 6] = [
    "shellcheck ",
    "eslint-disable",
    "eslint-enable",
    "renovate:",
    "pylint:",
    "prettier-ignore",
];
const CONTAINERFILE_PARSER_DIRECTIVES: [&str; 3] = ["syntax", "escape", "check"];
const JUSTFILE_NON_RECIPE_KEYWORDS: [&str; 5] = ["set ", "alias ", "export ", "import ", "mod "];

#[derive(Debug)]
pub(crate) struct Classified {
    pub(crate) kinds: Vec<Kind>,
    pub(crate) unterminated_from: Option<usize>,
}

pub(crate) fn classify(syntax: Syntax, text: &str) -> Classified {
    let lines: Vec<&str> = text.lines().collect();
    let mut lexer = lex::for_syntax(syntax);
    let mut kinds = Vec::with_capacity(lines.len());
    let mut open_since = None;
    for (index, line) in lines.iter().enumerate() {
        let was_neutral = lexer.is_neutral();
        kinds.push(if line.trim().is_empty() {
            Kind::Blank
        } else {
            lexer.next_line(&line.chars().collect::<Vec<_>>())
        });
        if lexer.is_neutral() {
            open_since = None;
        } else if was_neutral {
            open_since = Some(index + 1);
        }
    }
    exempt_shebang(&lines, &mut kinds);
    exempt_spdx_header(&lines, &mut kinds);
    mark_tool_directives(&lines, &mut kinds);
    match syntax {
        Syntax::Containerfile => exempt_parser_directives(&lines, &mut kinds),
        Syntax::Justfile => exempt_recipe_docs_and_shebangs(&lines, &mut kinds),
        _ => {}
    }
    Classified {
        kinds,
        unterminated_from: open_since,
    }
}

fn exempt_shebang(lines: &[&str], kinds: &mut [Kind]) {
    if kinds.first() == Some(&Kind::Comment) && lines[0].starts_with("#!") {
        kinds[0] = Kind::Exempt;
    }
}

fn exempt_spdx_header(lines: &[&str], kinds: &mut [Kind]) {
    for index in 0..lines.len() {
        if kinds[index] == Kind::Comment && lines[index].contains(SPDX_TAG) {
            kinds[index] = Kind::Exempt;
            let next = index + 1;
            if kinds.get(next) == Some(&Kind::Comment) && is_bare_marker(lines[next]) {
                kinds[next] = Kind::Exempt;
            }
        }
    }
}

fn is_bare_marker(line: &str) -> bool {
    line.trim()
        .trim_matches(|c| matches!(c, '#' | '/' | '*' | '!'))
        .is_empty()
}

fn mark_tool_directives(lines: &[&str], kinds: &mut [Kind]) {
    for (index, line) in lines.iter().enumerate() {
        if kinds[index] == Kind::Comment && is_tool_directive(line) {
            kinds[index] = Kind::Directive;
        }
    }
}

fn is_tool_directive(line: &str) -> bool {
    let text = line
        .trim_start()
        .trim_start_matches(['#', '/', '*'])
        .trim_start();
    TOOL_DIRECTIVES
        .iter()
        .any(|directive| text.starts_with(directive))
}

fn exempt_parser_directives(lines: &[&str], kinds: &mut [Kind]) {
    for (index, line) in lines.iter().enumerate() {
        if kinds[index] != Kind::Comment || !is_parser_directive(line) {
            break;
        }
        kinds[index] = Kind::Exempt;
    }
}

fn is_parser_directive(line: &str) -> bool {
    line.trim_start_matches('#')
        .split_once('=')
        .is_some_and(|(key, _)| {
            CONTAINERFILE_PARSER_DIRECTIVES.contains(&key.trim().to_ascii_lowercase().as_str())
        })
}

fn exempt_recipe_docs_and_shebangs(lines: &[&str], kinds: &mut [Kind]) {
    for header in (0..lines.len()).filter(|&index| is_recipe_header(lines[index])) {
        let body = header + 1;
        if kinds.get(body) == Some(&Kind::Comment) && lines[body].trim_start().starts_with("#!") {
            kinds[body] = Kind::Exempt;
        }
        let mut top = header;
        while top > 0 && lines[top - 1].starts_with('[') {
            top -= 1;
        }
        if top > 0 && kinds[top - 1] == Kind::Comment {
            kinds[top - 1] = Kind::Exempt;
        }
    }
}

fn is_recipe_header(line: &str) -> bool {
    let starts_like_a_name = line
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '@');
    starts_like_a_name
        && !JUSTFILE_NON_RECIPE_KEYWORDS
            .iter()
            .any(|keyword| line.starts_with(keyword))
        && line
            .split_once(':')
            .is_some_and(|(_, after)| !after.starts_with('='))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_map(syntax: Syntax, lines: &[&str]) -> String {
        let text = lines.join("\n") + "\n";
        classify(syntax, &text)
            .kinds
            .iter()
            .map(|kind| match kind {
                Kind::Blank => '_',
                Kind::Code => 'c',
                Kind::Comment => '#',
                Kind::Directive => 'd',
                Kind::Exempt => 'x',
            })
            .collect()
    }

    fn unterminated_from(syntax: Syntax, lines: &[&str]) -> Option<usize> {
        classify(syntax, &(lines.join("\n") + "\n")).unterminated_from
    }

    #[test]
    fn shell_shebang_and_spdx_header_are_exempt() {
        let script = [
            "#!/bin/sh",
            "# SPDX-License-Identifier: GPL-3.0-or-later",
            "#",
            "# why",
            "echo hi # trailing",
            "",
            "    # indented",
        ];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "xxx#c_#");
    }

    #[test]
    fn shell_here_documents_of_data_hold_no_comments() {
        let script = [
            "cat <<EOF",
            "# body",
            "EOF",
            "cat <<-END >\"$out\"",
            "\t# tabbed",
            "\tEND",
            "diff <<\"A\" - <<B",
            "# first",
            "A",
            "# second",
            "B",
            "cat >\"$d/python.err\" <<'EOF'",
            "# data, though python appears in a path",
            "EOF",
            "# after",
        ];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "cccccccccccccc#");
    }

    #[test]
    fn shell_here_documents_an_interpreter_runs_are_lexed_as_its_code() {
        let script = [
            "x=$(python3 - \"$a\" <<'PY'",
            "# python comment",
            "print('# not one')",
            "PY",
            ")",
            "if python3 - \"$a\" \\",
            "    >\"$out\" <<'PY'; then",
            "    # after a continued line",
            "PY",
            "fi",
            "ssh host \"bash -s\" <<EOF",
            "  # remote shell comment",
            "EOF",
        ];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "c#ccccc#ccc#c");
    }

    #[test]
    fn shell_here_documents_that_write_a_script_are_lexed_by_its_shebang() {
        let script = [
            "cat >\"$bin/fake\" <<'EOF'",
            "#!/bin/sh",
            "# fake comment",
            "exit 0",
            "EOF",
            "# after",
        ];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "cc#cc#");
    }

    #[test]
    fn shell_here_strings_and_arithmetic_shifts_open_no_here_document() {
        let script = [
            "read -r x <<<\"$y\"",
            "((mask & (1 << bit))) && echo",
            "echo $((1 << width))",
            "# comment",
        ];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "ccc#");
    }

    #[test]
    fn shell_hash_inside_words_strings_and_urls_starts_no_comment() {
        let script = [
            "echo \"${#list[@]}\" '#' a#b $# ${x#pre} https://example.com/#top",
            "echo 'it'\"'\"'s",
            "# inside a single-quoted string",
            "'",
            "# comment",
        ];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "cccc#");
    }

    #[test]
    fn shell_multi_line_strings_hide_hash_lines() {
        let script = [
            "awk '",
            "# awk comment",
            "{ print }' file",
            "jq --arg k \"$1\" \"",
            "# jq comment $(printf \"%s\" \")\")",
            "\"",
            "printf $'a\\'",
            "# ansi-c",
            "'",
        ];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "ccccccccc");
    }

    #[test]
    fn shell_comment_inside_a_multi_line_command_substitution_counts() {
        let script = ["x=$(", "    # why", "    date", ")", "# after"];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "c#cc#");
    }

    #[test]
    fn shell_apostrophe_in_a_comment_opens_no_string() {
        let script = ["# Fedora's", "# don't", "echo"];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "##c");
    }

    #[test]
    fn python_triple_quoted_strings_hide_hash_lines() {
        let script = [
            "#!/usr/bin/env python3",
            "# SPDX-License-Identifier: GPL-3.0-or-later",
            "\"\"\"Docstring",
            "# not a comment",
            "\"\"\"",
            "x = \"#\"  # trailing",
            "y = '''one line''' + r'\\''",
            "# comment",
        ];
        assert_eq!(kind_map(Syntax::Python, &script), "xxccccc#");
    }

    #[test]
    fn toml_literal_strings_have_no_escapes() {
        let config = [
            "# comment",
            "path = '''C:\\'''",
            "# comment",
            "url = \"https://example.com/#x\"",
            "text = \"\"\"",
            "# inside",
            "\"\"\"",
        ];
        assert_eq!(kind_map(Syntax::Toml, &config), "#c#cccc");
    }

    #[test]
    fn yaml_and_spec_quotes_end_at_the_line_end() {
        let workflow = [
            "# SPDX-License-Identifier: GPL-3.0-or-later",
            "name: Fedora's ci",
            "# comment",
            "      - run: |",
            "          # shell comment in a run block",
            "          cat <<EOF",
            "          # here-document body",
            "          EOF",
        ];
        assert_eq!(kind_map(Syntax::ShellEmbedded, &workflow), "xc#c#ccc");
        let spec = [
            "Summary:  Apex's tools",
            "# why",
            "%install",
            "cat >x <<'EOF'",
            "# body",
            "EOF",
        ];
        assert_eq!(kind_map(Syntax::ShellEmbedded, &spec), "c#cccc");
    }

    #[test]
    fn justfile_recipe_docs_and_recipe_shebangs_are_exempt() {
        let justfile = [
            "# SPDX-License-Identifier: GPL-3.0-or-later",
            "#",
            "# Header.",
            "",
            "set shell := [\"bash\", \"-c\"]",
            "# About the profile.",
            "profile := \"a:b\"",
            "",
            "# Doc of check.",
            "check:",
            "    cargo test",
            "",
            "# Section.",
            "# Doc of ssh.",
            "[positional-arguments]",
            "ssh *args:",
            "    #!/usr/bin/env bash",
            "    # body comment",
            "    echo '# not a comment'",
            "",
            "# Doc of build.",
            "build pkg=\"all\":",
            "    make {{ pkg }}",
        ];
        assert_eq!(
            kind_map(Syntax::Justfile, &justfile),
            "xx#_c#c_xcc_#xccx#c_xcc"
        );
    }

    #[test]
    fn containerfile_parser_directive_leads_and_run_here_documents_are_shell() {
        let containerfile = [
            "# syntax=docker/dockerfile:1",
            "# comment",
            "FROM x",
            "RUN <<EOF",
            "# body",
            "EOF",
            "# syntax=only-a-comment-here",
        ];
        assert_eq!(kind_map(Syntax::Containerfile, &containerfile), "x#cc#c#");
    }

    #[test]
    fn spec_edit_block_text_is_spec_content() {
        let edit = [
            "# SPDX-License-Identifier: GPL-3.0-or-later",
            "# why",
            "after ^%autosetup",
            "\t# a comment of the edited spec",
            "\tmkdir x",
            "end",
            "# why",
        ];
        assert_eq!(kind_map(Syntax::SpecEdit, &edit), "x#cccc#");
        assert_eq!(
            unterminated_from(Syntax::SpecEdit, &["replace ^Release:", "\tRelease: 1"]),
            Some(1)
        );
    }

    #[test]
    fn tool_directives_are_not_comments() {
        let script = [
            "# shellcheck disable=SC2034 # read by the caller",
            "# renovate: datasource=github-releases depName=x/y",
            "# why",
            "# shellcheck-like prose is a comment",
        ];
        assert_eq!(kind_map(Syntax::ShellScript, &script), "dd##");
        let module = [
            "    // eslint-disable-next-line no-await-in-loop",
            "    await step();",
        ];
        assert_eq!(kind_map(Syntax::JavaScript, &module), "dc");
    }

    #[test]
    fn systemd_units_and_lists_have_hash_lines_only() {
        let unit = [
            "# SPDX-License-Identifier: GPL-3.0-or-later",
            "[Unit]",
            "# why",
            "ExecStart=/bin/sh -c 'echo # not at the line start'",
        ];
        assert_eq!(kind_map(Syntax::HashLines, &unit), "xc#c");
    }

    #[test]
    fn rust_doc_comments_count_and_strings_hide_slashes() {
        let source = [
            "//! Crate doc.",
            "/// Item doc.",
            "fn main() {",
            "    let url = \"https://example.com\"; // trailing",
            "    let raw = r#\"",
            "// inside a raw string",
            "\"#;",
            "    let s = \"multi",
            "// inside a string \\\" still",
            "\";",
            "    let c = '\"';",
            "    let l: &'static str = \"x\";",
            "    /* block",
            "       continues */",
            "    code(); /* opens",
            "    still comment */",
            "}",
        ];
        assert_eq!(kind_map(Syntax::Rust, &source), "##cccccccccc##c#c");
    }

    #[test]
    fn rust_doc_attributes_are_doc_comments() {
        let source = [
            "#[doc = \"one\"]",
            "#![doc = \"two",
            "three\"]",
            "#[doc(hidden)]",
            "#[derive(Debug)]",
            "let doc = \"#[doc = x]\";",
        ];
        assert_eq!(kind_map(Syntax::Rust, &source), "###ccc");
    }

    #[test]
    fn rust_block_comments_nest_and_raw_identifiers_open_no_string() {
        let source = [
            "/* a /* b */",
            "   still */",
            "let r#type = br##\"x\"##;",
            "let e = '\\'';",
            "// comment",
        ];
        assert_eq!(kind_map(Syntax::Rust, &source), "##cc#");
    }

    #[test]
    fn javascript_templates_and_regexes_hide_slashes() {
        let source = [
            "// SPDX-License-Identifier: GPL-3.0-or-later",
            "//",
            "/**",
            " * Doc.",
            " */",
            "const url = 'https://x'; // trailing",
            "const t = `",
            "// inside a template ${a + `nested`}",
            "`;",
            "const re = /\\/\\/[/`]*/g, half = total / 2;",
            "// comment",
        ];
        assert_eq!(kind_map(Syntax::JavaScript, &source), "xx###ccccc#");
    }

    #[test]
    fn stylesheet_loud_comments_reach_the_compiled_css() {
        let sheet = [
            "// SPDX-License-Identifier: GPL-2.0-or-later",
            "//",
            "// why",
            "/*! loud marker */",
            ".a { background: url(http://x/y.png); }",
            "/* block",
            "   end */",
        ];
        assert_eq!(kind_map(Syntax::Stylesheet, &sheet), "xx#cc##");
    }

    #[test]
    fn an_unclosed_literal_is_reported_where_it_opened() {
        assert_eq!(
            unterminated_from(Syntax::ShellScript, &["echo", "cat <<EOF", "body"]),
            Some(2)
        );
        assert_eq!(unterminated_from(Syntax::Rust, &["/* open"]), Some(1));
        assert_eq!(
            unterminated_from(Syntax::ShellScript, &["echo 'a", "b'"]),
            None
        );
    }
}
