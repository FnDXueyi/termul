#!/usr/bin/env bash
# Hourly fast lane: new-issue triage + issue->dev PR work ONLY.
# Never overlaps another routine run: flock on a shared lock file.
# The daily 18:30 full routine (nightly QA, chat-agent QA) is separate.
set -uo pipefail

# cron PATH is minimal; omp + gh live in ~/.local/bin / usr/bin
PATH="$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
export PATH

REPO="/root/termul-routine-automation"
LOG_DIR="$REPO/goal-routine-runs/_automation"
RUN_ID="fast-$(date -u +%Y%m%d-%H%M%S)"

# Single-flight lock: shared with the daily runner (same lock file).
exec 9>"$REPO/.routine-lock"
if ! flock -n 9; then
  echo "$(date -u +%FT%TZ) another routine run holds the lock; skipping" \
    >> "$LOG_DIR/skips.log"
  exit 0
fi

# Sync the automation clone to the internal routine branch.
git -C "$REPO" fetch --quiet origin chore/agent-routine-skill
git -C "$REPO" reset --hard --quiet origin/chore/agent-routine-skill
git -C "$REPO" clean -fdq -- goal-routine-runs/_automation
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/$RUN_ID.log"

MARKER="$REPO/goal-routine-runs/_automation/.fast-lane-marker"
SINCE="$(cat "$MARKER" 2>/dev/null || date -u -d '1 hour ago' +%FT%TZ)"

# Bound each hourly run to 45 minutes so the next slot is never blocked.
omp -p \
  --model 'sepintu/zai/glm-5.3' \
  --thinking high \
  --auto-approve \
  --no-session \
  --max-time 2700 \
  --cwd "$REPO" \
  --append-system-prompt "You are the Termul routine FAST LANE agent. Your scope is deliberately narrow, do not run the full routine. Only two things: (1) FAST TRIAGE: fetch open issues updated since $SINCE (gh issue list -R gnoviawan/termul --state open --search \"updated:>$SINCE\" --json ...) and add missing labels per the skill's Pass 1 state table. Labels only, NEVER comments (comments belong to the daily full run, hourly repeats would spam). (2) ISSUE->PR: find dev-ready BUG issues, ones that reproduce on dev and have no existing open PR or routine/* branch fixing them, then per the skill's Unattended runs policy: FIRST open a DRAFT PR immediately (title conventional-commit suffixed with [skip ci] so CI and CodeRabbit spend nothing on the skeleton, body from the template with Closes #<n> and a WIP note naming the picked-up issue) so every other agent sees the claim and filters the issue out, THEN branch routine/<issue>-<slug> from origin/dev, fix, run the full local gate, and mark the PR ready for review. Max ONE PR per fast-lane run. Never feature issues without to-agent (never add that label). Never force-push, never merge, never close issues, never touch the routine branch itself. Everything else (nightly QA, chat-agent QA, housekeeping comments, stale sweeps) belongs to the daily run, skip it." \
  "Fast lane run. New-issue labels plus at most one bug-fix PR to dev. Follow the skill's Unattended runs section." \
  > "$LOG" 2>&1
echo "--- omp exited $? at $(date -u +%FT%TZ) ---" >> "$LOG"

# Advance the marker only after a completed run; a crashed run re-scans
# next hour (idempotent labels make that safe).
date -u +%FT%TZ > "$MARKER"

ls -1t "$LOG_DIR"/fast-*.log 2>/dev/null | tail -n +25 | xargs -r rm -f
exit 0
