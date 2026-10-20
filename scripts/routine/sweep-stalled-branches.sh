#!/usr/bin/env bash
# Sweep: run the deterministic finisher over every routine branch that has a
# commit beyond its claim but whose PR is still a draft. Serialized (one gate
# at a time — the cargo target dir is shared).
set -uo pipefail
PATH="$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin:$PATH"; export PATH
REPO="/root/termul-routine-automation"
cd "$REPO" || exit 1
git fetch --quiet origin dev 'routine/*' 2>/dev/null
for b in $(git ls-remote --heads origin 'routine/*' | awk '{print $2}' | sed 's|refs/heads/||'); do
  ahead=$(git rev-list --count "origin/dev..origin/$b" 2>/dev/null || echo 0)
  # skip claim-only branches (ahead <= 1) and already-ready PRs
  draft=$(gh pr list -R gnoviawan/termul --head "$b" --state open --json isDraft --jq '.[0].isDraft' 2>/dev/null)
  if [ "$ahead" -gt 1 ] && [ "$draft" = "true" ]; then
    echo "=== finishing $b (ahead=$ahead)"
    bash scripts/routine/finish-pr.sh "$b" --push 2>&1 | tail -3
  fi
done
