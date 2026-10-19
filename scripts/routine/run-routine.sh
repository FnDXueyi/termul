#!/usr/bin/env bash
# Unattended termul-routines runner (cron entry point).
# Runs the routine headless through omp on glm-5.3, auto-approved, in a
# dedicated clone pinned to dev so it never touches interactive branches.
set -uo pipefail

REPO="/root/termul-routine-automation"
LOG_DIR="$REPO/goal-routine-runs/_automation"
RUN_ID="$(date -u +%Y%m%d-%H%M%S)"

# Sync the automation clone to the internal routine branch. This stack is
# internal-only (never merged to the public dev); the clone exists only for
# this runner and carries no local work.
git -C "$REPO" fetch --quiet origin chore/agent-routine-skill
git -C "$REPO" reset --hard --quiet origin/chore/agent-routine-skill
git -C "$REPO" clean -fdq -- goal-routine-runs/_automation
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/$RUN_ID.log"

# Single-flight lock: shared with the hourly fast lane (same lock file).
exec 9>"$REPO/.routine-lock"
if ! flock -n 9; then
  echo "$(date -u +%FT%TZ) another routine run holds the lock; skipping" \
    >> "$REPO/goal-routine-runs/_automation/skips.log"
  exit 0
fi

# Kill anything a previous run left behind (a leaked server pins a core: #1391).
pkill -9 -f 'termul-nightly-audit-.*termul-server' 2>/dev/null || true

omp -p \
  --model 'sepintu/zai/glm-5.3' \
  --thinking high \
  --auto-approve \
  --no-session \
  --max-time 5400 \
  --cwd "$REPO" \
  --append-system-prompt "You are the unattended Termul routine agent. Read .claude/skills/termul-routines/SKILL.md in the repo and run the full routine now: Pass 1 (issue triage and housekeeping), Pass 3 (nightly QA via scripts/routine/nightly-audit.mjs, solved-today check, regression hunt), Pass 4 (chat-agent QA with the real opencode agent). Pass 2: auto-open PRs to dev for dev-ready BUG issues that reproduce and have no existing open fix PR, per the Unattended runs section of the skill. Never for feature issues without the to-agent label, which the agent never adds itself. Branch routine/<issue>-<slug> from origin/dev, run the full local gate before pushing, and comment the PR link on the issue. GitHub mutations via gh are authorized and expected. Never force-push, never merge PRs, never close issues, never add the to-agent label. File new issues for new findings per the skill's rules. Write the run report to goal-routine-runs/<run-id>/run-report.md and keep every artifact under goal-routine-runs/." \
  "Run the termul routine now. Follow the skill exactly." \
  > "$LOG" 2>&1
status=$?
echo "--- omp exited $status at $(date -u +%FT%TZ) ---" >> "$LOG"

# Keep the last 14 automation logs; older runs live on as their run dirs.
ls -1t "$LOG_DIR"/*.log 2>/dev/null | tail -n +15 | xargs -r rm -f
exit 0
