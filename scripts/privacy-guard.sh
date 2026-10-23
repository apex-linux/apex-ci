#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

usage() {
    cat >&2 <<'EOF'

Fail on biometric data in the repository: fingerprint images, print templates, umockdev
recordings, usbmon captures, fprintd's stored prints, hex dumps and long lines.

  privacy-guard.sh              check the files git tracks (CI: the checked-out commit)
  privacy-guard.sh --untracked  also check untracked files that .gitignore does not
                                hide (before `git add`)
  privacy-guard.sh --dir DIR    check the git repository at DIR (default: the current one)

A repository adds its own long-line exemptions to .github/privacy-guard.exempt, one glob
per line, each followed by "# " and the reason.

Exit status 0 clean, 1 findings (each printed as PATH or PATH:LINE with the reason).
EOF
    exit 2
}

# Bytes, not characters, and the same regex behaviour on every host.
export LC_ALL=C

readonly MAX_LINE=400
readonly FORBIDDEN_SUFFIXES=(pgm variant ioctl umockdev pcap pcapng)
readonly HEX_RUN='((0[xX])?[0-9A-Fa-f]{2}(, ?|[ :])?){128,}'
readonly HEX_DUMP_LINE=': ([0-9A-Fa-f]{2} ?){5,} *$'
readonly HEX_DUMP_FILES=(':(glob,icase)**/*.log' ':(glob,icase)**/*.txt')
readonly TABLE_ROW_MAX_TOKEN=120
LONG_LINE_EXEMPT=(
    '**/Cargo.lock' # written by cargo from registry metadata
    '**/snapshots/*.snap' # insta snapshots, written by the tests from Apex's own output
)
readonly EXEMPT_FILE=.github/privacy-guard.exempt

load_exemptions() {
    local line glob
    [[ -f $REPO/$EXEMPT_FILE ]] || return 0
    while IFS= read -r line || [[ -n $line ]]; do
        [[ $line =~ ^[[:space:]]*(#|$) ]] && continue
        [[ $line == *' # '?* ]] || die "$EXEMPT_FILE: '$line' gives no reason after ' # '"
        glob=${line%% # *}
        LONG_LINE_EXEMPT+=("${glob%"${glob##*[![:space:]]}"}")
    done <"$REPO/$EXEMPT_FILE"
}

# Byte-identical vendored files get only the name rules; each entry says why and what was checked.
readonly VENDORED_EXEMPT=()

# A finding names the file and line, never the content: CI logs are public.
FINDINGS=0
finding() {
    FINDINGS=$((FINDINGS + 1))
    if in_github_actions; then printf '::error::privacy guard: %s\n' "$*" >&2; else printf 'privacy guard: %s\n' "$*" >&2; fi
}

git_() { git -c safe.directory="$REPO" -C "$REPO" "$@"; }

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

scan() {
    local out=$1 ere=$2 rc=0
    shift 2
    git_ grep -z -n -I -E "${GREP_ARGS[@]}" -e "$ere" -- "$@" >"$out" || rc=$?
    ((rc <= 1)) || die "git grep failed (exit $rc)"
}

is_exempt_table_row() {
    [[ ${1,,} == *.md ]] || return 1
    [[ $2 =~ ^[[:space:]]*\|.*\|[[:space:]]*$ ]] || return 1
    ! [[ $2 =~ [^[:space:]]{$TABLE_ROW_MAX_TOKEN} ]]
}

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
        line_finding "$path" "$line" "a ${#text}-character line (a dump or an encoded image?); wrap it, or add a glob and the reason to .github/privacy-guard.exempt"
    done <"$WORK/long-lines"
}

main() {
    local untracked=0
    REPO=
    while (($#)); do
        case $1 in
            --untracked) untracked=1; shift ;;
            --dir) REPO=$2; shift 2 ;;
            *) usage ;;
        esac
    done
    require git
    REPO=$(checkout_root "$REPO")
    load_exemptions

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
        die "privacy guard: $FINDINGS finding(s); remove the files from the change (and from history if they were pushed), see https://github.com/apex-linux/apex/blob/main/docs/hardware/FINGERPRINT.md"
    fi
    log "privacy guard: no biometric files, hex dumps or long lines in $(git_ "${ls[@]}" | tr -cd '\0' | wc -c) files"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    main "$@"
fi
