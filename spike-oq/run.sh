#!/bin/sh
# usage: run.sh <mode> <name> [max_seconds]
EV=/c/Users/ginan/.claude/orchestrate/termul-browser-v3/findings/1-1
rm -f $EV/$2.jsonl $EV/$2.stdout.log
SPIKE_MODE=$1 SPIKE_OUT="$EV/$2.jsonl" SPIKE_PROFILES="E:/open-source/PecutAPP/termul/.claude/worktrees/v3-1-1/spike-oq/profiles" SPIKE_EVIDENCE="$EV" /e/open-source/PecutAPP/termul/src-tauri/target-v3-1-1/debug/spike-oq.exe > $EV/$2.stdout.log 2>&1 &
sleep 5
n=0
while [ $n -lt ${3:-150} ]; do
  grep -q '"done"' $EV/$2.jsonl 2>/dev/null && break
  tasklist | grep -qi spike-oq || break
  sleep 1; n=$((n+1))
done
sleep 2
taskkill //F //IM spike-oq.exe >/dev/null 2>&1 && echo "killed after ${n}s"
echo "lines: $(wc -l < $EV/$2.jsonl)"
