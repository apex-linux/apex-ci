#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

GUARD="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/privacy-guard.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
FAILS=0

check() {
    local name=$1 want=$2 got=$3
    if [[ $want == "$got" ]]; then
        printf 'ok: %s\n' "$name"
    else
        printf 'FAIL: %s: expected %q, got %q\n' "$name" "$want" "$got"
        FAILS=$((FAILS + 1))
    fi
}

guard_rc() { local rc=0; "$GUARD" --dir "$1" >/dev/null 2>&1 || rc=$?; printf '%s' "$rc"; }

repo() {
    local dir=$TMP/$1
    mkdir -p "$dir"
    git -C "$dir" init -q
    printf 'fine\n' >"$dir/README.md"
    printf '%s\n' "$dir"
}

commit() { git -C "$1" add -A && git -C "$1" -c user.name=t -c user.email=t@example.invalid commit -q -m t; }

clean=$(repo clean)
commit "$clean"
check "a clean repository passes" 0 "$(guard_rc "$clean")"

pgm=$(repo pgm)
printf 'P5\n' >"$pgm/print.PGM"
git -C "$pgm" add -f print.PGM
check "a fingerprint image fails, in any letter case" 1 "$(guard_rc "$pgm")"

long=$(repo long)
mkdir -p "$long/data"
printf '%500s\n' '' | tr ' ' x >"$long/data/blob.txt"
commit "$long"
check "a 500-byte line fails" 1 "$(guard_rc "$long")"

mkdir -p "$long/.github"
printf 'data/*.txt # one generated line per record\n' >"$long/.github/privacy-guard.exempt"
commit "$long"
check "the repository's exemption file lets it pass" 0 "$(guard_rc "$long")"

printf 'data/*.txt\n' >"$long/.github/privacy-guard.exempt"
commit "$long"
check "an exemption without a reason is refused" 1 "$(guard_rc "$long")"

hex=$(repo hex)
for _ in $(seq 130); do printf '0a '; done >"$hex/dump.c"
commit "$hex"
check "a run of 128 or more hex bytes fails, exemption or not" 1 "$(guard_rc "$hex")"

((FAILS == 0)) || die "selftest: $FAILS failure(s)"
log "selftest: all checks passed"
