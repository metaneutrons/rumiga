#!/bin/sh
# Checks a commit message against Conventional Commits and forbids attribution
# trailers from AI tools.
set -eu

msg_file=${1:?path to the commit message is missing}
msg=$(cat "$msg_file")
body=$(printf '%s\n' "$msg" | sed -e '/^#/d' -e '/^diff --git /,$d')
subject=$(printf '%s\n' "$body" | sed -e '/^[[:space:]]*$/d' -e 1q)

fail() {
    printf '\033[31mCommit rejected:\033[0m %s\n' "$1" >&2
    shift
    for line in "$@"; do printf '  %s\n' "$line" >&2; done
    exit 1
}

case "$subject" in
    "Merge "*|"Revert \""*|"fixup!"*|"squash!"*|"amend!"*) exit 0 ;;
esac

types='feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert'
if ! printf '%s' "$subject" |
    grep -Eq "^($types)(\([a-z0-9._/-]+\))?!?: .+"; then
    fail 'The subject line does not follow Conventional Commits.'
fi

if [ "${#subject}" -gt 100 ]; then
    fail "The subject line is ${#subject} characters long, 100 are allowed."
fi

if printf '%s\n' "$body" | grep -Eiq '^[[:space:]]*co-authored-by:.*(claude|anthropic|noreply@anthropic\.com)'; then
    fail 'The message contains an AI attribution trailer.'
fi
if printf '%s\n' "$body" | grep -Eiq 'generated with .*claude code|🤖 generated with'; then
    fail 'The message contains an AI generation line.'
fi
