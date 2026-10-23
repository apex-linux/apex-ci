# apex-ci

The CI tooling every [apex-linux](https://github.com/apex-linux) repository shares. The
organisation profile and the security policy live in
[apex-linux/.github](https://github.com/apex-linux/.github).

| Path | What it is |
|---|---|
| `default.json` | The Renovate preset every repository extends (`"extends": ["local>apex-linux/apex-ci"]`) |
| `actions/install-tool` | Installs pinned release binaries (actionlint, shellcheck, cargo-deny, cargo-auditable, cosign) after checking their SHA-256 |
| `actions/privacy-guard` | Fails on biometric files, hex dumps and long lines in the checked-out repository |
| `actions/fedora-rust` | In a `fedora` container job: Fedora's Rust toolchain, checked against `rust-toolchain.toml` |
| `actions/comments` | The comment guard, `apex-lint comments`, built from the action's own commit |
| `apex-lint/` | The comment guard itself: code explains itself, so long comment blocks and comment-heavy files fail the build ([CONTRIBUTING](https://github.com/apex-linux/apex/blob/main/docs/CONTRIBUTING.md)) |
| `scripts/` | The scripts behind those actions; each runs locally too (`scripts/privacy-guard.sh --dir ../apex-image`) |

A workflow uses an action by commit, never by branch or tag:

```yaml
- uses: apex-linux/apex-ci/actions/privacy-guard@<commit> # main
```

Run the comment guard locally on any checkout:

```bash
cargo install --locked --git https://github.com/apex-linux/apex-ci apex-lint
apex-lint comments            # every git-tracked file in scope
apex-lint comments --json
```

A repository with files that legitimately hold long lines lists them in
`.github/privacy-guard.exempt`, one glob per line, each followed by `# ` and the reason.

## Checks

`scripts/selftest.sh`, shellcheck, actionlint, and apex-lint's fmt, clippy and tests run in
CI with the privacy and comment guards. Licensed under the GPL-3.0-or-later (`LICENSE`).
