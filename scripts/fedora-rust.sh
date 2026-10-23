#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

usage() {
    cat >&2 <<'USAGE'

Install Fedora's Rust toolchain from the release's own repositories, then check it
against the checkout's rust-toolchain.toml. Runs as root in a fedora container.

  fedora-rust.sh [PACKAGE...]   also install the extra PACKAGEs
USAGE
    exit 2
}

[[ ${1:-} != -* ]] || usage
((EUID == 0)) || die "fedora-rust.sh installs packages; run it as root in a fedora container"
require dnf5
retry 3 10 dnf5 -y --setopt=install_weak_deps=False --repo=fedora --repo=updates install \
    rust cargo clippy rustfmt gcc "$@"
"$(dirname "${BASH_SOURCE[0]}")/rust-toolchain-check.sh"
