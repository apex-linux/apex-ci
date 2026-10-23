#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

usage() {
    cat >&2 <<'EOF'

Assert that the rustc on PATH is the release pinned in rust-toolchain.toml.

  rust-toolchain-check.sh              fail on a mismatch (CI)
  rust-toolchain-check.sh --warn-only  report a mismatch as a warning (an RPM build)

rust-toolchain.toml is read from the git checkout of the current directory.
EOF
    exit 2
}

warn_only=0
case ${1:-} in
    "") ;;
    --warn-only) warn_only=1 ;;
    *) usage ;;
esac

require rustc sed

toolchain_file="$(checkout_root "$PWD")/rust-toolchain.toml"
fedora=$(. /etc/os-release 2>/dev/null && printf '%s' "${VERSION_ID:-}")
[[ -f $toolchain_file ]] || die "missing $toolchain_file"

pinned=$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$toolchain_file")
[[ -n $pinned ]] || die "no channel = \"...\" line in rust-toolchain.toml"

actual=$(rustc -vV | sed -n 's/^release: //p')
[[ -n $actual ]] || die "could not read the release from 'rustc -vV'"

if command -v rustup >/dev/null 2>&1 && [[ -z ${RUSTUP_TOOLCHAIN:-} ]]; then
    log "rustup found: rustc is a proxy and follows rust-toolchain.toml"
fi

log "rust-toolchain.toml channel: $pinned"
log "rustc on PATH ($(command -v rustc)): $actual"

if [[ $actual != "$pinned" ]]; then
    msg="rustc is $actual but rust-toolchain.toml pins $pinned. Fedora ${fedora:-this release} ships the \
compiler that builds the RPM; set channel = \"$actual\" in rust-toolchain.toml (and \
review rust-version in Cargo.toml) in the same change that moves to the new Fedora rust."
    if ((warn_only)); then
        warn "$msg"
        summary "Toolchain: rustc $actual differs from rust-toolchain.toml ($pinned); built anyway."
        exit 0
    fi
    die "$msg"
fi

if command -v rpm >/dev/null 2>&1; then
    summary "Toolchain: \`$(rpm -q rust cargo 2>/dev/null | tr '\n' ' ')\` matches rust-toolchain.toml ($pinned)."
fi
