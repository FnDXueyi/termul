#!/usr/bin/env bash
# Deterministic PR finisher for routine worker branches.
#
# Takes a routine/<issue>-<slug> branch that holds a fix commit and does the
# part the model reliably fails to finish: run the full local gate, then
# ship or record the failure. No model in this loop.
#
#   finish-pr.sh <branch> [--push]
#
# - Gate: biome ci, typecheck (3 tsconfigs), vitest, cargo clippy, cargo test.
# - On any failure: writes goal-routine-runs/_automation/gate-failures/<branch>.log
#   with the failing step + tail, and exits 1. The next worker turn reads that
#   log as its TODO.
# - On success with --push: pushes the branch, strips "[skip ci]" from the
#   open draft PR title, marks the PR ready, comments the link on the issue.
# - Works in the branch's own worktree (independent checkout), never in the
#   automation clone itself, so running gates never collides with dispatch.
set -uo pipefail

PATH="$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
export PATH

BRANCH="${1:-}"
PUSH="${2:-}"
REPO="/root/termul-routine-automation"
WT="$REPO/.wt/finish-${BRANCH//\//-}"
FAIL_DIR="$REPO/goal-routine-runs/_automation/gate-failures"
LOG="$FAIL_DIR/$(echo "$BRANCH" | tr '/' '-').log"
mkdir -p "$FAIL_DIR"

if [ -z "$BRANCH" ] || [ "${BRANCH#routine/}" = "$BRANCH" ]; then
  echo "usage: finish-pr.sh <routine/issue-slug> [--push]" >&2
  exit 2
fi

step () { echo "=== $1 $(date -u +%FT%TZ)" | tee -a "$LOG"; }
fail () {
  echo "GATE FAILED at: $1" | tee -a "$LOG"
  exit 1
}

echo "gate run for $BRANCH started $(date -u +%FT%TZ)" > "$LOG"

# Independent worktree at the branch tip.
git -C "$REPO" worktree remove --force "$WT" 2>/dev/null || true
git -C "$REPO" fetch --quiet origin "$BRANCH"
git -C "$REPO" worktree add --quiet --force "$WT" "origin/$BRANCH" 2>/dev/null || {
  # local-only branch (worker never pushed): use the local ref
  git -C "$REPO" worktree add --quiet --force "$WT" "$BRANCH" 2>/dev/null || {
    echo "cannot carve worktree for $BRANCH" | tee -a "$LOG"
    exit 2
  }
}
cd "$WT" || exit 2

# Rebase onto current dev first: claim branches are cut hours earlier and
# pre-existing dev flakes must not fail the gate. A clean rebase also keeps
# the PR mergeable. Conflicts abort the gate with a clear log line (the
# worker resolves them next turn).
git fetch --quiet origin dev
if ! git rebase origin/dev >> "$LOG" 2>&1; then
  git rebase --abort >> "$LOG" 2>&1 || true
  echo "GATE FAILED at: rebase onto origin/dev has conflicts; resolve and re-run" | tee -a "$LOG"
  exit 1
fi

# Node deps: link the clone's node_modules (bun install is content-addressed;
# a fresh install would re-download everything for identical deps).
[ -d node_modules ] || ln -s "$REPO/node_modules" node_modules
( cd src-tauri && [ -d node_modules ] || ln -s "$REPO/node_modules" node_modules ) 2>/dev/null

step "biome ci"
bun run ci >> "$LOG" 2>&1 || { tail -40 "$LOG"; fail "bun run ci"; }

step "typecheck"
bun run typecheck >> "$LOG" 2>&1 || { tail -40 "$LOG"; fail "bun run typecheck"; }

step "vitest"
bun run test >> "$LOG" 2>&1 || { tail -40 "$LOG"; fail "bun run test"; }

step "clippy"
( cd src-tauri && CARGO_TARGET_DIR="$REPO/.cargo-target" cargo clippy --all-targets -- -D warnings ) >> "$LOG" 2>&1 \
  || { tail -60 "$LOG"; fail "cargo clippy"; }

step "cargo test"
( cd src-tauri && CARGO_TARGET_DIR="$REPO/.cargo-target" cargo test ) >> "$LOG" 2>&1 \
  || { tail -60 "$LOG"; fail "cargo test"; }

step "gate green"
echo "ALL GATES GREEN: $BRANCH" | tee -a "$LOG"

if [ "$PUSH" = "--push" ]; then
  git push origin "$BRANCH:refs/heads/$BRANCH" >> "$LOG" 2>&1 || { fail "git push"; }
  ISSUE="${BRANCH#routine/}"; ISSUE="${ISSUE%%-*}"
  PR_JSON="$(gh pr list -R gnoviawan/termul --head "$BRANCH" --state open --json number,isDraft,title --jq '.[0]')"
  PR_NUM="$(echo "$PR_JSON" | jq -r '.number // empty')"
  if [ -n "$PR_NUM" ]; then
    TITLE="$(echo "$PR_JSON" | jq -r '.title' | sed 's/ \[skip ci\]//')"
    gh pr edit "$PR_NUM" -R gnoviawan/termul --title "$TITLE" >> "$LOG" 2>&1
    gh pr ready "$PR_NUM" -R gnoviawan/termul >> "$LOG" 2>&1 || true
    gh pr comment "$PR_NUM" -R gnoviawan/termul --body "Gate green: biome, typecheck, vitest, clippy, cargo test all pass. Log: goal-routine-runs/_automation/gate-failures/$(echo "$BRANCH" | tr '/' '-').log" >> "$LOG" 2>&1
    gh issue comment "$ISSUE" -R gnoviawan/termul --body "Fix ready for review: #$PR_NUM" >> "$LOG" 2>&1 || true
    echo "SHIPPED: PR $PR_NUM marked ready" | tee -a "$LOG"
  fi
fi

git -C "$REPO" worktree remove --force "$WT" 2>/dev/null || true
exit 0
