mod classify;
mod lex;
mod report;
mod rules;
mod scope;

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use classify::classify;
use lex::Kind;
use report::Finding;

const USAGE: &str = "\
Usage: apex-lint comments [--json] [PATH...]
       apex-lint strip FILE...

comments  Check comment blocks and comment density (docs/CONTRIBUTING.md) in every
          git-tracked file in scope, or in each PATH (a directory: its git-listed
          files). Prints path:line: rule: message and exits 1 on any violation.
strip     Print each FILE without its comment lines: diff the output before and after
          a comment cleanup to see that only comments changed.
";

const EXIT_VIOLATIONS: u8 = 1;
const EXIT_USAGE_OR_ERROR: u8 = 2;

#[derive(Debug)]
struct Target {
    shown: String,
    on_disk: PathBuf,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("apex-lint: {error}");
            ExitCode::from(EXIT_USAGE_OR_ERROR)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    match args.split_first() {
        Some((command, rest)) if command == "comments" => comments(rest),
        Some((command, rest)) if command == "strip" => strip(rest),
        Some((flag, _)) if flag == "-h" || flag == "--help" => {
            emit(USAGE)?;
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(format!("expected a command\n\n{USAGE}")),
    }
}

fn comments(args: &[String]) -> Result<ExitCode, String> {
    let json = args.iter().any(|arg| arg == "--json");
    let paths: Vec<&String> = args.iter().filter(|arg| *arg != "--json").collect();
    if let Some(unknown) = paths.iter().find(|path| path.starts_with('-')) {
        return Err(format!("unknown option {unknown}\n\n{USAGE}"));
    }
    let targets = if paths.is_empty() {
        tracked_files()?
    } else {
        named_files(&paths)?
    };
    let mut findings = Vec::new();
    for target in targets {
        for violation in check_file(&target)? {
            findings.push(Finding {
                path: target.shown.clone(),
                violation,
            });
        }
    }
    if json {
        emit(&report::json(&findings))?;
    } else {
        emit(&report::text(&findings))?;
    }
    if findings.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!(
            "apex-lint: {} comment violations; the rule and how to fix them: docs/CONTRIBUTING.md",
            findings.len()
        );
        Ok(ExitCode::from(EXIT_VIOLATIONS))
    }
}

fn check_file(target: &Target) -> Result<Vec<rules::Violation>, String> {
    let Some(text) = read_if_present(&target.on_disk)? else {
        return Ok(Vec::new());
    };
    if scope::exclusion(&target.on_disk, &text).is_some() {
        return Ok(Vec::new());
    }
    Ok(scope::syntax(&target.on_disk, &text)
        .map(|syntax| rules::check(&classify(syntax, &text)))
        .unwrap_or_default())
}

fn strip(files: &[String]) -> Result<ExitCode, String> {
    if files.is_empty() {
        return Err(format!("strip needs at least one FILE\n\n{USAGE}"));
    }
    let mut stripped = String::new();
    for file in files {
        let path = Path::new(file);
        let text = read_if_present(path)?.ok_or_else(|| format!("{file}: no such file"))?;
        let syntax =
            scope::syntax(path, &text).ok_or_else(|| format!("{file}: no known comment syntax"))?;
        let classified = classify(syntax, &text);
        stripped.extend(
            text.split_inclusive('\n')
                .zip(&classified.kinds)
                .filter(|(_, kind)| **kind != Kind::Comment)
                .map(|(line, _)| line),
        );
    }
    emit(&stripped)?;
    Ok(ExitCode::SUCCESS)
}

fn tracked_files() -> Result<Vec<Target>, String> {
    let root = PathBuf::from(git(None, &["rev-parse", "--show-toplevel"])?.trim_end());
    let listing = git(Some(&root), &["ls-files", "-z"])?;
    let targets: Vec<Target> = listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| Target {
            shown: path.to_owned(),
            on_disk: root.join(path),
        })
        .collect();
    if targets.is_empty() {
        return Err("git ls-files listed no files; run apex-lint inside the repository".to_owned());
    }
    Ok(targets)
}

fn named_files(paths: &[&String]) -> Result<Vec<Target>, String> {
    let mut targets = Vec::new();
    for path in paths {
        if Path::new(path.as_str()).is_dir() {
            let listing = git(
                None,
                &[
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "--",
                    path.as_str(),
                ],
            )?;
            targets.extend(
                listing
                    .split('\0')
                    .filter(|listed| !listed.is_empty())
                    .map(|listed| Target {
                        shown: listed.to_owned(),
                        on_disk: PathBuf::from(listed),
                    }),
            );
        } else {
            targets.push(Target {
                shown: (*path).clone(),
                on_disk: PathBuf::from(path.as_str()),
            });
        }
    }
    Ok(targets)
}

fn git(directory: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    if let Some(directory) = directory {
        command.arg("-C").arg(directory);
    }
    let output = command
        .args(args)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim_end()
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("git {} printed non-UTF-8", args[0]))
}

fn read_if_present(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn emit(text: &str) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
            Err(format!("cannot write to stdout: {error}"))
        }
        _ => Ok(()),
    }
}
