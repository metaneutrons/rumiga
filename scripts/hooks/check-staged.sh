#!/bin/sh
set -eu

max_bytes=${MAX_STAGED_BYTES:-5242880}
status=0
fail() {
    printf 'Commit rejected: %s\n' "$1" >&2
    status=1
}

branch=$(git symbolic-ref --quiet --short HEAD 2>/dev/null || echo '')
default=$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null | sed 's#^origin/##')
default=${default:-main}
if [ "$branch" = "$default" ] && [ "${ALLOW_COMMIT_ON_DEFAULT:-}" != '1' ]; then
    fail "direct commit to '$default'"
fi

while IFS= read -r file; do
    [ -f "$file" ] || continue
    size=$(wc -c < "$file" | tr -d ' ')
    [ "$size" -le "$max_bytes" ] || fail "$file exceeds the staged-file size limit"
done <<EOF
$(git -c core.quotePath=false diff --cached --name-only --diff-filter=AM)
EOF

if git -c core.quotePath=false diff --cached --name-only |
    grep -Eq '(^|/)(node_modules|target|dist|build|\.next|coverage)/'; then
    fail 'a build or dependency directory is staged'
fi

exit "$status"
