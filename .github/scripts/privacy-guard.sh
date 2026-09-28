#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The CI guard against biometric data in the repository (ADR 0006, THREAT-MODEL T6,
# docs/hardware/FINGERPRINT.md "Never commit, attach or paste"). A fingerprint image or
# template that reaches the public history can never be taken back, so this fails on:
#   - files named *.pgm (images), *.variant (enrolled prints), *.ioctl and *.umockdev
#     (umockdev recordings, which contain images), *.pcap and *.pcapng (usbmon
#     captures), in any letter case;
#   - anything under a var/lib/fprint directory (fprintd's stored prints);
#   - a line with a run of 128 or more hex bytes (optionally 0x-prefixed, separated by
#     a space, a colon or a comma), in any text file: what FP_DEBUG_TRANSFER=1 prints
#     for a sensor frame, or a frame pasted into C;
#   - a hex-dump line in a *.log or *.txt file: one that ends in ": " and five or more
#     hex bytes, `: ([0-9A-Fa-f]{2} ?){5,} *$`;
#   - a line of 400 or more characters (bytes) in a text file, where dumps, base64
#     images and umockdev records end up. Exempt are the files in LONG_LINE_EXEMPT,
#     whose format puts long lines there, and Markdown table rows (a line of a *.md
#     file that starts and ends with '|') without a run of 120 or more non-blank
#     characters; the three rules above still apply to them.
# Vendored files kept byte-identical to their source (VENDORED_EXEMPT) get only the
# name rules; each entry says why.
# Binary files are not read (git grep -I); only their names are checked.
# Findings name the file and line, never the content: CI logs are public too.
#
#   privacy-guard.sh              check the files git tracks (CI: the checked-out commit)
#   privacy-guard.sh --untracked  also check untracked files that .gitignore does not
#                                 hide (before `git add`)
#   privacy-guard.sh --dir DIR    check the git repository at DIR instead of this one
#
# Exit status 0 clean, 1 findings (each printed as PATH or PATH:LINE with the reason).

set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

# Bytes, not characters, and the same regex behaviour on every host.
export LC_ALL=C

readonly MAX_LINE=400
readonly FORBIDDEN_SUFFIXES=(pgm variant ioctl umockdev pcap pcapng)
# 128 or more hex bytes in a row (each at least two hex digits, optionally 0x-prefixed,
# optionally followed by one separator). ERE, as git grep -E takes it.
readonly HEX_RUN='((0[xX])?[0-9A-Fa-f]{2}(, ?|[ :])?){128,}'
# A hex-dump line (docs/hardware/FINGERPRINT.md), checked in *.log and *.txt files.
readonly HEX_DUMP_LINE=': ([0-9A-Fa-f]{2} ?){5,} *$'
readonly HEX_DUMP_FILES=(':(glob,icase)**/*.log' ':(glob,icase)**/*.txt')
# A Markdown table row with a run this long is not exempt (a pasted blob).
readonly TABLE_ROW_MAX_TOKEN=120
# Files whose format puts lines of MAX_LINE or more there (git pathspec globs). Each
# entry says why; the hex rules still apply to them.
readonly LONG_LINE_EXEMPT=(
    # Lockfiles: written by cargo from registry metadata.
    '**/Cargo.lock'
    # insta snapshots: written by the tests from Apex's own output.
    '**/snapshots/*.snap'
    # bootc --progress-fd captures: JSON Lines, one event per line.
    'crates/apex-bootc/tests/fixtures/progress/*.jsonl'
    # A .flatpakrepo carries the remote's public key as one base64 line (GPGKey=).
    'image/system_files/usr/share/flatpak/remotes.d/*.flatpakrepo'
    # A bootc status JSON document in a shell string (owner follow-up: wrap it, then
    # drop this entry).
    'crates/apex-bootc/src/runner/tests.rs'
)

# Vendored files kept byte-identical to their source (git pathspec globs): only the
# name rules apply to them. Each entry says why and what was checked.
readonly VENDORED_EXEMPT=(
    # The owner's design prototype (design/SPEC.md): minified JS, long SVG paths, and
    # C2PA content credentials in the icons (signer, claim and hashes; no personal or
    # biometric data, checked 2026-09-28).
    'design/mockup/**'
)

# finding MESSAGE: report one finding.
FINDINGS=0
finding() {
    FINDINGS=$((FINDINGS + 1))
    if in_github_actions; then printf '::error::privacy guard: %s\n' "$*" >&2; else printf 'privacy guard: %s\n' "$*" >&2; fi
}

git_() { git -c safe.directory="$REPO" -C "$REPO" "$@"; }

# check_names: every path (NUL-separated on stdin) against the forbidden names.
check_names() {
    local path lower suffix
    while IFS= read -r -d '' path; do
        lower=${path,,}
        for suffix in "${FORBIDDEN_SUFFIXES[@]}"; do
            if [[ $lower == *."$suffix" ]]; then
                finding "$path: *.$suffix files hold biometric data or captures and never belong in the repository"
            fi
        done
        if [[ /$lower/ == */var/lib/fprint/* ]]; then
            finding "$path: fprintd's stored prints (var/lib/fprint) never belong in the repository"
        fi
    done
}

# scan OUT ERE PATHSPEC...: the lines matching ERE in the text files of PATHSPEC (binary
# files are skipped: -I) as PATH NUL LINE NUL TEXT LF records in OUT. Fails on a git
# grep error, which must never pass as clean.
scan() {
    local out=$1 ere=$2 rc=0
    shift 2
    git_ grep -z -n -I -E "${GREP_ARGS[@]}" -e "$ere" -- "$@" >"$out" || rc=$?
    ((rc <= 1)) || die "git grep failed (exit $rc)"
}

# is_exempt_table_row PATH TEXT: a Markdown table row without a long unbroken run.
is_exempt_table_row() {
    [[ ${1,,} == *.md ]] || return 1
    [[ $2 =~ ^[[:space:]]*\|.*\|[[:space:]]*$ ]] || return 1
    ! [[ $2 =~ [^[:space:]]{$TABLE_ROW_MAX_TOKEN} ]]
}

# line_finding PATH LINE MESSAGE: report PATH:LINE, once (the first rule that matches).
declare -A REPORTED=()
line_finding() {
    [[ -z ${REPORTED["$1:$2"]:-} ]] || return 0
    REPORTED["$1:$2"]=1
    finding "$1:$2: $3"
}

check_lines() {
    local path line text
    local -a exclude=() vendored=()
    for path in "${LONG_LINE_EXEMPT[@]}"; do exclude+=(":(exclude,glob)$path"); done
    for path in "${VENDORED_EXEMPT[@]}"; do vendored+=(":(exclude,glob)$path"); done

    scan "$WORK/hex-runs" "$HEX_RUN" . "${vendored[@]}"
    while IFS= read -r -d '' path && IFS= read -r -d '' line && IFS= read -r text; do
        line_finding "$path" "$line" "a run of 128 or more hex bytes (a sensor dump?)"
    done <"$WORK/hex-runs"

    scan "$WORK/hex-dumps" "$HEX_DUMP_LINE" "${HEX_DUMP_FILES[@]}" "${vendored[@]}"
    while IFS= read -r -d '' path && IFS= read -r -d '' line && IFS= read -r text; do
        line_finding "$path" "$line" "a hex-dump line (': ' and five or more hex bytes) in a log or text file"
    done <"$WORK/hex-dumps"

    scan "$WORK/long-lines" ".{$MAX_LINE}" . "${exclude[@]}" "${vendored[@]}"
    while IFS= read -r -d '' path && IFS= read -r -d '' line && IFS= read -r text; do
        is_exempt_table_row "$path" "$text" && continue
        line_finding "$path" "$line" "a ${#text}-character line (a dump or an encoded image?); wrap it, or add the file to LONG_LINE_EXEMPT in .github/scripts/privacy-guard.sh with the reason"
    done <"$WORK/long-lines"
}

main() {
    local untracked=0
    REPO=$APEX_ROOT
    while (($#)); do
        case $1 in
            --untracked) untracked=1; shift ;;
            --dir) REPO=$2; shift 2 ;;
            *) usage ;;
        esac
    done
    require git
    git_ rev-parse --git-dir >/dev/null 2>&1 || die "$REPO is not a git repository"

    local -a ls=(ls-files -z --cached)
    GREP_ARGS=()
    if ((untracked)); then
        ls+=(--others --exclude-standard)
        GREP_ARGS+=(--untracked)
    fi
    WORK=$(mktemp -d)
    # shellcheck disable=SC2064 # expand now: the path is fixed
    trap "rm -rf '$WORK'" EXIT
    check_names < <(git_ "${ls[@]}")
    check_lines

    if ((FINDINGS > 0)); then
        die "privacy guard: $FINDINGS finding(s); remove the files from the change (and from history if they were pushed), see docs/hardware/FINGERPRINT.md"
    fi
    log "privacy guard: no biometric files, hex dumps or long lines in $(git_ "${ls[@]}" | tr -cd '\0' | wc -c) files"
}

# selftest.sh sources this file to test its functions; only a direct run starts main.
if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    main "$@"
fi
