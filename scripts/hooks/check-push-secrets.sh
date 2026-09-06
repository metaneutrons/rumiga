#!/bin/sh
set -eu

command -v gitleaks >/dev/null 2>&1 || {
    printf 'Push rejected: gitleaks is missing (brew install gitleaks).\n' >&2
    exit 1
}

range=''
if upstream=$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null); then
    range="${upstream}..HEAD"
elif head_ref=$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null); then
    range="${head_ref}..HEAD"
fi

if [ -z "$range" ]; then
    printf 'gitleaks: no remote reference, full scan.\n' >&2
    exec gitleaks git --redact --verbose .
fi

if [ -z "$(git rev-list --max-count=1 "$range" 2>/dev/null || true)" ]; then
    printf 'gitleaks: no new commits in %s.\n' "$range" >&2
    exit 0
fi

exec gitleaks git --redact --verbose --log-opts="$range" .
