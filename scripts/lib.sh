# shellcheck shell=bash
# SPDX-License-Identifier: GPL-3.0-or-later

in_github_actions() { [[ ${GITHUB_ACTIONS:-} == true ]]; }

log() { printf '%s\n' "$*" >&2; }

warn() {
    if in_github_actions; then printf '::warning::%s\n' "$*" >&2; else printf 'warning: %s\n' "$*" >&2; fi
}

die() {
    if in_github_actions; then printf '::error::%s\n' "$*" >&2; else printf 'error: %s\n' "$*" >&2; fi
    exit 1
}

require() {
    local missing=() c
    for c in "$@"; do
        command -v "$c" >/dev/null 2>&1 || missing+=("$c")
    done
    ((${#missing[@]} == 0)) || die "missing required tools: ${missing[*]}"
}

summary() {
    if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
        printf '%s\n' "$@" >>"$GITHUB_STEP_SUMMARY"
    else
        printf '%s\n' "$@" >&2
    fi
}

retry() {
    local attempts=$1 delay=$2 n=1 rc
    shift 2
    while true; do
        rc=0
        "$@" || rc=$?
        ((rc == 0)) && return 0
        if ((n >= attempts)); then
            warn "giving up after $n attempts (exit $rc): $*"
            return "$rc"
        fi
        log "attempt $n/$attempts failed (exit $rc), retrying in ${delay}s: $*"
        sleep "$delay"
        n=$((n + 1))
        delay=$((delay * 2))
    done
}

# The caller's checkout: the composite actions run these scripts from the action's own copy.
checkout_root() {
    local dir=${1:-${GITHUB_WORKSPACE:-$PWD}}
    git -c safe.directory='*' -C "$dir" rev-parse --show-toplevel 2>/dev/null ||
        die "$dir is not inside a git checkout"
}
