use std::path::{Component, Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    ShellScript,
    ShellEmbedded,
    Justfile,
    Containerfile,
    SpecEdit,
    Python,
    Toml,
    HashLines,
    Rust,
    JavaScript,
    Stylesheet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exclusion {
    MockupExportedByDesignTool,
    TestFixtureData,
    ProseNotCode,
    UpstreamDiff,
    InstaSnapshot,
    CargoResolverOutput,
    JsonHasNoComments,
    MarkupNotCode,
}

const POLKIT_RULES_DIRECTORY: [&str; 2] = ["polkit-1", "rules.d"];

pub(crate) fn exclusion(path: &Path, text: &str) -> Option<Exclusion> {
    if has_directories(path, &["design", "mockup"]) {
        return Some(Exclusion::MockupExportedByDesignTool);
    }
    if has_directories(path, &["tests", "fixtures"]) || has_directories(path, &["tests", "golden"])
    {
        return Some(Exclusion::TestFixtureData);
    }
    if file_name(path) == "Cargo.lock" {
        return Some(Exclusion::CargoResolverOutput);
    }
    match extension(path) {
        "md" => Some(Exclusion::ProseNotCode),
        "patch" => Some(Exclusion::UpstreamDiff),
        "snap" => Some(Exclusion::InstaSnapshot),
        "json" | "jsonl" => Some(Exclusion::JsonHasNoComments),
        "svg" | "xml" => Some(Exclusion::MarkupNotCode),
        _ if text.trim_start().starts_with('<') => Some(Exclusion::MarkupNotCode),
        _ => None,
    }
}

pub(crate) fn syntax(path: &Path, text: &str) -> Option<Syntax> {
    by_file_name(file_name(path))
        .or_else(|| by_extension(path))
        .or_else(|| by_shebang(text.lines().next().unwrap_or_default()))
}

fn by_file_name(name: &str) -> Option<Syntax> {
    match name {
        "Justfile" | "justfile" => Some(Syntax::Justfile),
        "Containerfile" | "Dockerfile" => Some(Syntax::Containerfile),
        "meson.build" => Some(Syntax::Python),
        "spec-edit" => Some(Syntax::SpecEdit),
        "Makefile" | "GNUmakefile" | "series" | "os-release" | ".env" | ".gitignore"
        | ".containerignore" | ".dockerignore" | ".editorconfig" => Some(Syntax::HashLines),
        _ => None,
    }
}

fn by_extension(path: &Path) -> Option<Syntax> {
    match extension(path) {
        "sh" | "bash" | "dash" | "zsh" => Some(Syntax::ShellScript),
        "spec" | "yml" | "yaml" => Some(Syntax::ShellEmbedded),
        "py" => Some(Syntax::Python),
        "toml" => Some(Syntax::Toml),
        "rs" => Some(Syntax::Rust),
        "js" | "cjs" | "mjs" => Some(Syntax::JavaScript),
        "rules" if has_directories(path, &POLKIT_RULES_DIRECTORY) => Some(Syntax::JavaScript),
        "scss" | "css" => Some(Syntax::Stylesheet),
        "service" | "timer" | "socket" | "path" | "target" | "mount" | "conf" | "preset"
        | "env" | "rules" | "txt" | "cfg" | "override" | "preinstall" | "flatpakrepo"
        | "desktop" => Some(Syntax::HashLines),
        _ => None,
    }
}

pub(crate) fn by_shebang(first_line: &str) -> Option<Syntax> {
    let command = first_line.trim_start().strip_prefix("#!")?;
    let mut words = command.split_whitespace();
    let mut interpreter = words.next()?;
    if base_name(interpreter) == "env" {
        interpreter = words.find(|word| !word.starts_with('-'))?;
    }
    interpreter_syntax(interpreter)
}

pub(crate) fn interpreter_syntax(command: &str) -> Option<Syntax> {
    match base_name(command) {
        "sh" | "bash" | "dash" | "zsh" | "ksh" => Some(Syntax::ShellScript),
        "node" | "gjs" => Some(Syntax::JavaScript),
        name if is_python(name) => Some(Syntax::Python),
        _ => None,
    }
}

fn is_python(name: &str) -> bool {
    name.strip_prefix("python")
        .is_some_and(|version| version.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

fn base_name(command: &str) -> &str {
    command.rsplit('/').next().unwrap_or(command)
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

fn extension(path: &Path) -> &str {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
}

fn has_directories(path: &Path, sequence: &[&str]) -> bool {
    let directories: Vec<&str> = path
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    directories
        .windows(sequence.len())
        .any(|window| window == sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn excluded(path: &str) -> Option<Exclusion> {
        exclusion(Path::new(path), "")
    }

    fn syntax_of(path: &str, text: &str) -> Option<Syntax> {
        syntax(Path::new(path), text)
    }

    #[test]
    fn content_not_authored_here_is_excluded() {
        assert_eq!(
            excluded("design/mockup/support.js"),
            Some(Exclusion::MockupExportedByDesignTool)
        );
        assert_eq!(
            excluded("crates/apexctl/tests/fixtures/codec.txt"),
            Some(Exclusion::TestFixtureData)
        );
        assert_eq!(
            excluded("crates/apex-lane/tests/golden/spec-edit/edge.spec"),
            Some(Exclusion::TestFixtureData)
        );
        assert_eq!(excluded("docs/ROADMAP.md"), Some(Exclusion::ProseNotCode));
        assert_eq!(
            excluded("packaging/pkgs/kernel/patches/0001-x.patch"),
            Some(Exclusion::UpstreamDiff)
        );
        assert_eq!(
            excluded("crates/apexctl/src/snapshots/a.snap"),
            Some(Exclusion::InstaSnapshot)
        );
        assert_eq!(excluded("Cargo.lock"), Some(Exclusion::CargoResolverOutput));
        assert_eq!(
            excluded("renovate.json"),
            Some(Exclusion::JsonHasNoComments)
        );
        assert_eq!(
            excluded("fixtures.jsonl"),
            Some(Exclusion::JsonHasNoComments)
        );
        assert_eq!(excluded("desktop/icon.svg"), Some(Exclusion::MarkupNotCode));
        assert_eq!(
            excluded("desktop/schemas/a.gschema.xml"),
            Some(Exclusion::MarkupNotCode)
        );
        assert_eq!(excluded("image/tests/run.sh"), None);
        assert_eq!(excluded("design/SPEC.md"), Some(Exclusion::ProseNotCode));
        assert_eq!(excluded("tools/design/render-mockup.cjs"), None);
    }

    #[test]
    fn markup_behind_a_config_extension_is_excluded() {
        let dbus_policy = "<?xml version=\"1.0\"?>\n<busconfig/>\n";
        assert_eq!(
            exclusion(Path::new("data/io.example.conf"), dbus_policy),
            Some(Exclusion::MarkupNotCode)
        );
    }

    #[test]
    fn syntax_follows_the_file_name_then_the_extension_then_the_shebang() {
        assert_eq!(syntax_of("Justfile", ""), Some(Syntax::Justfile));
        assert_eq!(
            syntax_of("image/Containerfile", ""),
            Some(Syntax::Containerfile)
        );
        assert_eq!(syntax_of("theme/meson.build", ""), Some(Syntax::Python));
        assert_eq!(
            syntax_of("packaging/libfprint/series", ""),
            Some(Syntax::HashLines)
        );
        assert_eq!(
            syntax_of("packaging/pkgs/gnome-shell/pkg.env", ""),
            Some(Syntax::HashLines)
        );
        assert_eq!(syntax_of(".gitignore", ""), Some(Syntax::HashLines));
        assert_eq!(
            syntax_of("a/b.service.d/50-x.conf", ""),
            Some(Syntax::HashLines)
        );
        assert_eq!(
            syntax_of(".github/workflows/ci.yml", ""),
            Some(Syntax::ShellEmbedded)
        );
        assert_eq!(
            syntax_of("packaging/apex.spec", ""),
            Some(Syntax::ShellEmbedded)
        );
        assert_eq!(syntax_of("Cargo.toml", ""), Some(Syntax::Toml));
        assert_eq!(syntax_of("src/main.rs", ""), Some(Syntax::Rust));
        assert_eq!(syntax_of("tools/render.cjs", ""), Some(Syntax::JavaScript));
        assert_eq!(syntax_of("theme/_apex.scss", ""), Some(Syntax::Stylesheet));
        assert_eq!(
            syntax_of("usr/share/polkit-1/rules.d/50-x.rules", ""),
            Some(Syntax::JavaScript)
        );
        assert_eq!(
            syntax_of("usr/lib/udev/rules.d/61-x.rules", ""),
            Some(Syntax::HashLines)
        );
        assert_eq!(
            syntax_of("image/tests/fake/dnf5", "#!/bin/bash\n"),
            Some(Syntax::ShellScript)
        );
        assert_eq!(
            syntax_of("image/tests/fake/rpm", "#!/usr/bin/env python3\n"),
            Some(Syntax::Python)
        );
        assert_eq!(
            syntax_of("probe", "#!/usr/bin/env -S gjs -m\n"),
            Some(Syntax::JavaScript)
        );
        assert_eq!(syntax_of("LICENSE", "GNU GENERAL PUBLIC LICENSE\n"), None);
    }
}
