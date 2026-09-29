use std::fmt::Write as _;

use crate::rules::Violation;

#[derive(Debug)]
pub(crate) struct Finding {
    pub(crate) path: String,
    pub(crate) violation: Violation,
}

pub(crate) fn text(findings: &[Finding]) -> String {
    let mut lines = String::new();
    for Finding { path, violation } in findings {
        let _ = writeln!(
            lines,
            "{path}:{}: {}: {}",
            violation.line, violation.rule, violation.message
        );
    }
    lines
}

pub(crate) fn json(findings: &[Finding]) -> String {
    let objects: Vec<String> = findings
        .iter()
        .map(|Finding { path, violation }| {
            format!(
                "  {{\"path\": {}, \"line\": {}, \"rule\": \"{}\", \"message\": {}}}",
                json_string(path),
                violation.line,
                violation.rule,
                json_string(&violation.message)
            )
        })
        .collect();
    if objects.is_empty() {
        "[]\n".to_owned()
    } else {
        format!("[\n{}\n]\n", objects.join(",\n"))
    }
}

fn json_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for c in value.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(quoted, "\\u{:04x}", u32::from(c));
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Rule;

    fn finding(path: &str, message: &str) -> Finding {
        Finding {
            path: path.to_owned(),
            violation: Violation {
                line: 3,
                rule: Rule::Block,
                message: message.to_owned(),
            },
        }
    }

    #[test]
    fn text_is_one_path_line_rule_message_per_line() {
        assert_eq!(
            text(&[finding("a.sh", "too long")]),
            "a.sh:3: block: too long\n"
        );
    }

    #[test]
    fn json_escapes_quotes_backslashes_and_control_characters() {
        assert_eq!(json(&[]), "[]\n");
        assert_eq!(
            json(&[finding("dir/\"q\".sh", "a\\b\nc\u{1}")]),
            "[\n  {\"path\": \"dir/\\\"q\\\".sh\", \"line\": 3, \"rule\": \"block\", \
             \"message\": \"a\\\\b\\nc\\u0001\"}\n]\n"
        );
    }
}
