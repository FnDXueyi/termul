---
name: termul-routines
description: Run the project's recurring agent routines — issue triage and housekeeping, the to-agent development gate, nightly-build QA with new-issue filing, and chat-agent QA against a real opencode agent. Use when the user says "run the routine", "triage issues", "housekeeping", "QA the nightly", "check issues solved today", or asks to develop a labeled feature issue.
---

# Termul routines

Four recurring passes. Each one is independent, ends in a written artifact, and only then mutates GitHub. Run the ones the user asked for. With no selector, run daily-triage then nightly-qa in that order.

Ordering rule. Triage and housekeeping are reversible (labels, comments), so they proceed without asking. Code PRs, new issues, and merges follow the repo gates. Every pass ends with a one-line-per-item report in the reply. Passes share one working state: a run directory, created before anything else.

```bash
RUN_ID="$(date -u +%Y%m%d-%H%M%S)"
mkdir -p "goal-routine-runs/$RUN_ID"   # local:// run notes also land here
```

Everything a pass writes goes in that directory. Nothing outside it changes except GitHub actions named below.

## Pass 1: Triage and housekeeping

Inputs are all open issues. Fetch them once with `gh issue list -R gnoviawan/termul --state open --limit 200 --json number,title,labels,body,updatedAt`.

Classify each issue into exactly one state. The state machine is the contract; keep one row per issue in `goal-routine-runs/<run>/triage.md`.

| State | Meaning | Action |
|---|---|---|
| `triaged` | Has priority + area labels and a clear repro | none |
| `needs-labels` | Missing `priority-*` or an area label | add the missing labels via `gh issue edit` |
| `needs-info` | Bug without a repro, or question without an answer | ask one concrete question in a comment |
| `stale` | No activity 60+ days and not `priority-critical` | comment the status question, do not close |
| `duplicate` | Same report as another open issue | comment the duplicate link, label `duplicate` |
| `dev-ready` | Bug with a repro, or an accepted enhancement, and you judge it worth developing | see Pass 2 gate |

Rules that keep this honest:

- Every classification cites issue evidence in the report row. A title alone is not evidence for `dev-ready`; read the body.
- Label `bug` issues with a platform only when the body names one (`mac`, `win`, `linux`).
- Never close, lock, pin, or edit a title. Housekeeping is labels and comments.
- Cap at 30 issues per run so a pass stays reviewable. Note the cap in the report.
- One comment per issue per run. If you already asked last run, the issue moves to `stale`, not a second ask.

`dev-ready` output for bugs only, unless the user asked for enhancements too. Feature/enhancement requests get one extra gate before `dev-ready`:

- If the issue carries the `to-agent` label, it is sanctioned for agent development. `dev-ready`.
- If it does not, stop at `needs-labels` with a note. Do not develop it, and do not add `to-agent` yourself. The label is the human's consent signal.

This gate is a hard stop. An unlabeled feature that looks obvious still stops. Say so in the report and move on.

## Pass 2: Develop a dev-ready issue

For each `dev-ready` issue, follow the poteto-mode playbooks on the issue's own merits — Feature playbook for enhancements, Bug fix playbook for bugs. Read the poteto-mode skill (`.claude/skills/poteto-mode/SKILL.md` in the repo) in full before this pass. Branch, verify, and open the PR to `dev` per `.github/PULL_REQUEST_TEMPLATE.md` and `scripts/validate-pr-body.ts`.

Stop conditions for this pass, before any PR opens:

- Repro fails on current dev. Comment the failed repro on the issue and skip.
- The fix is not clearly scoped by the issue. Comment the open question and skip.
- AGENTS.md needs human approval of the complete diff before submission. Build the diff, run the full local gate (`bun run ci`, `bun run typecheck`, `bun run test`, `cargo clippy --all-targets -- -D warnings`, `cargo test` in `src-tauri`), then stop and present the diff for approval. PRs open only after the human approves.

One issue, one PR, one problem. Up to three issues per run; more than that stacks follow-ups instead.

## Pass 3: Nightly QA

Target is the moving `nightly` prerelease. The audit lever is the whole pass's spine:

```bash
bun scripts/routine/nightly-audit.mjs --json \
  > goal-routine-runs/<run>/nightly-audit.json
```

Run it, read the JSON, and only then decide anything. The script is the reviewer-rerunnable evidence trail for the pass. It fetches `latest-nightly.json`, downloads and verifies the `termul-server` binary for this platform, boots it on a scratch port with a scratch state dir, and probes `/health`, the `/ws` authenticate/ping/subscribe/store_write/store_read path, and `list_persisted_sessions`. It kills the server and leaves no state behind. A `verdict` field ends the JSON: `pass` or `fail`.

Script verdict drives the report. A `fail` is a finding; the script's failing check name is the finding's title. Do not re-derive the script's result by hand.

After the lever, the pass has a human half the script cannot do, in this order:

1. **Solved-today check.** `gh issue list -R gnoviawan/termul --state closed --search "closed:>=<Jakarta date>" --json number,title,closedAt` where the Jakarta date is the nightly build's date (`TZ='Asia/Jakarta' date +%F` over the manifest's `pub_date`). For each closed issue that shipped after the nightly's pinned commit, verify the fix by hand on the booted server: reproduce the pre-fix symptom through the probe path, confirm it is gone. A closed issue that cannot be exercised on the server surface is reported `not-verifiable-on-server`, not failed.
2. **Regression hunt.** The known-failure inventory lives at the bottom of this skill. Compare the audit JSON against it. A check that was failing and now passes is a fix worth noting in the report. A check that was passing and now fails is a regression: file a new issue (see below).
3. **Deeper UI QA when the lever passes.** Serve the nightly's web client (the audit script's `--keep-server` flag holds the port open), drive it in a browser per the poteto-mode control skill for browser UIs. One focused sweep, not an open-ended one.

Filing a new issue for anything found:

- Search open AND closed issues for the symptom first (`gh search issues`). A duplicate comment beats a duplicate issue.
- New bug issues get the `bug` label plus `priority-*` from impact, never `to-agent` (that label is only for feature consent).
- Body shape matches the repo's existing reports: environment, version from the manifest, repro, expected, evidence path. Evidence lives under `goal-routine-runs/<run>/`.
- The report lists every filed issue number.

## Pass 4: Chat-agent QA with a real opencode agent

Purpose. Termul's chat agent features must be checked against a real agent process, not mocks. opencode over ACP is that real agent, and the `sepintu` provider is already configured on this host (`~/.config/opencode/opencode.jsonc`, free tier).

Setup facts:

- opencode binary lives at `/root/.local/state/termul/acp-registry-binaries/opencode/opencode` (Termul's own verified `install_acp_agent` put it there). `command -v opencode` is a fallback.
- The ACP entrypoint is the `acp` arg. The server spawns `["<path>", "acp"]`.
- Model: `sepintu/moonshot/kimi-k3` via the configured provider. Costs nothing on the free tier.
- `docs/qa-report-2026-09-15.md` is the prior art for this pass. Read it before running.

Run shape. Boot the nightly server (Pass 3 lever with `--keep-server`, or a locally built `termul-server` for dev HEAD). Over the `/ws` relay:

1. `spawn_agent` with `{ configId: 'opencode-qa', name: 'opencode-qa', command: <binary>, args: ['acp'] }`.
2. `create_session` in a throwaway cwd.
3. `send_prompt` with a deterministic assertion prompt. "Reply with exactly OK and nothing else." Never free-form.
4. Assert on the wire: `message_chunk` events arrived, `prompt_complete` arrived, the concatenated text contains the expected token.
5. `close_session`, then `kill_agent`.


Wire facts learned the hard way (2026-10-19 run), all verified against the live protocol:

- Ledgered `/ws` requests (`spawn_agent`, `create_session`, `send_prompt`, `close_session`) are refused `invalid_command_id` without one. The field is `command_id` (snake_case) on `/ws` but `commandId` (camelCase) on `/terminal/ws`. The host strips nothing; send the right casing per endpoint.
- `send_prompt`'s `command_id` must equal its payload `turnId`, and its reply resolves only after the full agent turn (`payload: "end_turn"`).
- Events arrive as `{sid, seq, type, payload}` frames; `message_chunk` text lives at `payload.content.text` with `payload.role` distinguishing `thought` from `agent` turns. Assert on `role === 'agent'` text only, or thought content pollutes the assertion.
- The first frame after connect is the `auth_required` event, which can land in the same TCP chunk as the WS handshake reply. A probe that drops post-handshake leftovers loses it and every early frame.
- The relay sends `ping` requests every 20 s; answer or the connection dies on the 75 s `pongTimeoutMs`.

Log every frame to `goal-routine-runs/<run>/chat-agent-qa.jsonl`. Assertions are on the log, not on memory of watching it.

Findings file as issues per Pass 3 rules. Two failure classes deserve a second look before filing: greeting-turn latency at `create_session` (known opencode behavior, see the QA report's P2) and idle-agent CPU pinning (open issue #1391) — both already have issues. Do not file duplicates of known findings.

## Unattended runs

`scripts/routine/run-routine.sh` runs this whole skill headless: omp in print mode, `sepintu/zai/glm-5.3`, thinking high, auto-approve, in a dedicated clone at `/root/termul-routine-automation` pinned to the internal `chore/agent-routine-skill` branch. Cron fires it daily at 18:30 UTC (01:30 Jakarta), after the 17:00 UTC nightly build, so the QA target is the finished nightly of that Jakarta date. The runner syncs the clone, kills any leaked audit servers first (a leaked agent pins a core, see `#1391`), and writes logs to `goal-routine-runs/_automation/<run-id>.log`, keeping the last 14. Run artifacts land under `goal-routine-runs/` as in interactive runs.

Two schedules share one lock (`/root/termul-routine-automation/.routine-lock`, `flock -n`), so runs never overlap:

- **Hourly fast lane, parallel** (`scripts/routine/run-fast-lane.sh`, cron at minute 7, `TERMUL_FAST_WORKERS`, default 3): one triage dispatcher (labels only, never comments) plus up to 3 issue->PR workers, each in its own git worktree (`.wt/worker-<n>`, carved from the internal branch, branching `routine/<issue>-<slug>` from `origin/dev` on claim). Per-worker locks make a long-running worker survive into the next tick with its slot left empty rather than stacking. Workers run at `--thinking max` per operator call (2026-10-19; measured 1.8x slower per turn than low, ~64 s vs ~36 s on a repo-read task, quality is bought on purpose) with an 80-minute bound so the slower turns still reach the gate; a deadline-hit worker's uncommitted WIP is salvaged onto its branch as a `wip:` commit by the next tick before the re-carve.
- **Daily full routine** (`scripts/routine/run-routine.sh`, 18:30 UTC): everything in this skill, Pass 3 and Pass 4 included, up to 3 Pass 2 PRs.

Unattended Pass 2 (issue → PR), explicit operator policy, 2026-10-19:

- **Claim-then-build, draft first.** When an issue is `dev-ready`, the run opens a DRAFT PR on `routine/<issue>-<slug>` IMMEDIATELY, before any code exists. The draft title carries `[skip ci]` so PR validation and CodeRabbit spend nothing on the skeleton; strip the suffix when marking the PR ready. The draft is the claim signal: every other agent (fast lane, daily, any future lane) MUST filter out issues that already have an open PR, draft or ready, or a live `routine/*` branch, and never claims an issue twice. After the draft, branch from `origin/dev`, fix, run the full local gate, then mark the PR ready. This is an operator-authorized waiver of AGENTS.md's pre-submission approval for unattended routine runs only; CI, CodeRabbit, and the merge decision remain the safety net.
- **The to-agent gate still holds absolutely.** No `to-agent` label was added by anyone yet; until the operator labels issues, Pass 2 considers bugs only. The agent never adds the label itself.
- One issue, one PR, max 3 per run. If a fix fails its gate or the repro does not hold, comment the failure on the issue and skip, do not open a red PR.
- Post each PR link back to its issue (`Closes #<n>` in the body per the PR template).

## Report shape

Every pass ends its section of the reply the same way: one line per issue or check, state, and the action taken. GitHub mutations name the issue number and the exact mutation. A pass that changed nothing says so and why.

## Known-failure inventory

Checked against the nightly by Pass 3 step 2. Prune entries once they pass two consecutive nightlies.

- `#1391` — standalone `termul-server` pins one full CPU core per live ACP agent on Linux (idle busy-loop). Watch the booted server's CPU while an agent lives.
- Greeting-turn cost on `create_session` with opencode — known agent-side behavior (see `docs/qa-report-2026-09-15.md`). Not a Termul bug; do not file.

## Verification

- Pass 1: the report row cites the issue body evidence. A reviewer re-reads the issue and agrees.
- Pass 2: the PR carries the issue link and the local gate output in its body.
- Pass 3: `nightly-audit.json` is re-runnable by anyone (`bun scripts/routine/nightly-audit.mjs --json`). The verdict field decides.
- Pass 4: the JSONL frame log plus the assertion results are the evidence. Re-run reproduces them.
