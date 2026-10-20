#!/usr/bin/env bash
# Hourly fast lane: new-issue triage + issue->dev PR work ONLY.
#
# Parallel worker design: each hourly tick runs ONE dispatcher plus up to 3
# issue->PR workers. Every worker gets its own git worktree carved from the
# automation clone (shared object store, isolated checkout + index), so three
# fixes can run their gates simultaneously without fighting over one index.
# Worktree naming: .wt/worker-<n>. The dispatcher itself is the triage worker.
#
# Locking:
#   - .routine-lock        guards the dispatcher against the daily full run
#   - .fast-lane-worker-<n> per-worker lock, held for the worker's lifetime,
#                          so a long-running worker survives into the next
#                          hourly tick and the next tick's slot stays empty
#                          rather than stacking two agents in one worktree.
set -uo pipefail

# cron PATH is minimal; omp + gh live in ~/.local/bin /usr/bin
PATH="$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
export PATH

REPO="/root/termul-routine-automation"
WORKERS="${TERMUL_FAST_WORKERS:-3}"
LOG_DIR="$REPO/goal-routine-runs/_automation"
RUN_ID="fast-$(date -u +%Y%m%d-%H%M%S)"

# Dispatcher single-flight lock: shared with the daily runner (same lock file).
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
mkdir -p "$LOG_DIR" "$REPO/.wt"
LOG="$LOG_DIR/$RUN_ID.log"

# The worker prompt is shared; each worker discovers its own issue by
# claiming: draft PR first (skip ci), branch routine/<issue>-<slug>.
worker_prompt () {
  cat <<PROMPT
You are Termul routine fast-lane WORKER $1 (worktree .wt/worker-$1). Your ONLY job is one issue->dev PR, nothing else, no triage, no comments on other issues, no nightly QA.

Claim protocol, in this exact order:
0. FIRST check whether your worktree already holds a WIP routine/* branch from a previous tick (a wip: commit, uncommitted changes, or an open draft PR with only claim commits). If it does, FINISH THAT FIRST: implement, commit, then invoke the finisher. Never claim a new issue while one of your own drafts is unfinished.
1. Run: gh issue list -R gnoviawan/termul --state open --limit 60 --json number,title,labels --jq ...
2. Filter to dev-ready candidates: BUG-labeled issues, OR any issue carrying the to-agent label. Then EXCLUDE any issue that already has an open PR or a live routine/* branch on origin. Never double-claim.
3. Pick exactly ONE issue, highest priority label first, then oldest.
4. ATOMIC claim FIRST, before any branch or PR: git push origin HEAD:refs/claims/<issue-number> is rejected if another worker already claimed (git push is atomic). Push an empty commit on a temp branch or use: git push origin origin/dev:refs/claims/<issue-number>. If the push is rejected, the issue is taken — pick the next candidate. Only after a SUCCESSFUL claim push: checkout -b routine/<issue>-<slug> from origin/dev, commit the claim, push the branch, and open a DRAFT PR titled with the conventional-commit subject suffixed with [skip ci], body from .github/PULL_REQUEST_TEMPLATE.md with Closes #<n> and a WIP claim note. The claim ref stays until the PR merges or the draft is closed not-planned.
5. Then implement the fix and COMMIT it locally (conventional-commit message). Do NOT run the gate commands yourself and do NOT push: the deterministic finisher owns that.
6. Invoke the finisher: bash scripts/routine/finish-pr.sh routine/<issue>-<slug> --push (run it from your worktree; it gates in its own worktree, pushes, strips [skip ci], marks the PR ready, and comments the links). If it fails, read goal-routine-runs/_automation/gate-failures/<branch>.log, fix ONLY what the log shows, commit, and re-invoke the finisher. Repeat until green or your budget ends; never bypass the finisher.

Hard rules: never force-push, never merge, never close issues, never add to-agent, never touch branches other than your own routine/<issue>-<slug>, never edit files outside your worktree. If no claimable issue exists, exit silently. If the repro fails or the gate fails, comment that on the issue and close your own draft PR as not planned.
PROMPT
}

# --- Dispatch triage (labels only) + workers in parallel ------------------
# Triage rides with the dispatcher: one agent, labels only, no comments.
SINCE="$(cat "$REPO/goal-routine-runs/_automation/.fast-lane-marker" 2>/dev/null || date -u -d '1 hour ago' +%FT%TZ)"
DISPATCH_LOG="$LOG_DIR/$RUN_ID-triage.log"

dispatch () {
  omp -p \
    --model 'sepintu/zai/glm-5.3' \
    --thinking high \
    --auto-approve \
    --no-session \
    --max-time 1200 \
    --cwd "$REPO" \
    --append-system-prompt "You are the Termul routine FAST LANE triage dispatcher. Labels ONLY on open issues missing priority/area labels per the skill's Pass 1 state table. NEVER comments, NEVER PRs, NEVER issue claims. One pass over issues updated since $SINCE, then exit." \
    "Triage pass. Labels only." \
    > "$DISPATCH_LOG" 2>&1
}

worker () {
  local n="$1"
  local wt="$REPO/.wt/worker-$n"
  local wlog="$LOG_DIR/$RUN_ID-worker-$n.log"
  # Per-worker lock: if this worker from a previous tick is still running,
  # skip its slot this tick (no stacking in one worktree).
  exec 8>"$REPO/.fast-lane-worker-$n"
  if ! flock -n 8; then
    echo "$(date -u +%FT%TZ) worker $n still busy from an earlier tick; slot empty" >> "$wlog"
    return 0
  fi
  # Salvage: commit-and-push any uncommitted state in the worktree so a
  # previous tick's half-built fix is never destroyed by the re-carve.
  if [ -d "$wt" ]; then
    (
      cd "$wt" || exit 0
      if [ -n "$(git status --porcelain)" ]; then
        branch="$(git branch --show-current)"
        if [ -n "$branch" ] && [ "${branch#routine/}" != "$branch" ]; then
          git add -A
          git -c user.name="routine" -c user.email="routine@users.noreply.github.com" \
            commit -m "wip: salvage before worktree re-carve (worker $n)" --quiet
          git push origin "$branch" --quiet 2>/dev/null || true
          echo "$(date -u +%FT%TZ) salvaged WIP onto $branch" >> "$wlog"
        else
          echo "$(date -u +%FT%TZ) dirty tree on non-routine branch $branch; discarding" >> "$wlog"
          git reset --hard --quiet
          git clean -fdq
        fi
      fi
    ) 2>/dev/null
  fi

  # Carve/refresh the worktree at the internal routine branch so the worker
  # has the skill + scripts; it branches its own routine/<issue>-<slug> from
  # origin/dev when it claims.
  git -C "$REPO" worktree remove --force "$wt" 2>/dev/null || true
  git -C "$REPO" worktree add --quiet --force "$wt" origin/chore/agent-routine-skill 2>/dev/null || {
    echo "worker $n: worktree carve failed" >> "$wlog"
    return 0
  }
  (
    cd "$wt" || exit 0
    omp -p \
      --model 'sepintu/zai/glm-5.3' \
      --thinking max \
      --auto-approve \
      --no-session \
      --cwd "$wt" \
      --append-system-prompt "$(worker_prompt "$n")" \
      "Claim one issue and deliver one PR. Follow your instructions exactly." \
      > "$wlog" 2>&1
    echo "--- worker $n omp exited $? at $(date -u +%FT%TZ) ---" >> "$wlog"
  ) &
}

# Triage dispatcher + workers, all in parallel.
dispatch &
for n in $(seq 1 "$WORKERS"); do
  worker "$n" &
done
wait

echo "--- fast lane run $RUN_ID complete at $(date -u +%FT%TZ) ---" >> "$LOG"
date -u +%FT%TZ > "$REPO/goal-routine-runs/_automation/.fast-lane-marker"

ls -1t "$LOG_DIR"/fast-*.log 2>/dev/null | tail -n +25 | xargs -r rm -f
exit 0
