import json
import sys
from collections import Counter


def counts(log):
    c = Counter()
    for e in log:
        c[f"{e['t']}{'' if e.get('tr') else '(untrusted)'}@{e.get('id')}"] += 1
    return dict(c)


def page(d):
    if not isinstance(d, dict):
        return d
    out = {k: v for k, v in d.items() if k not in ("log", "ua")}
    if "log" in d:
        out["events"] = counts(d["log"])
        wheels = [(e.get("dy"), e.get("dm")) for e in d["log"] if e["t"] == "wheel"]
        if wheels:
            out["wheel_deltas"] = wheels
        moves = [e for e in d["log"] if e["t"] == "mousemove"]
        if moves:
            out["mousemove_buttons"] = sorted({e.get("b") for e in moves})
    return out


for path in sys.argv[1:]:
    print(f"===== {path}")
    for line in open(path, encoding="utf-8"):
        line = line.strip()
        if not line.startswith("{"):
            continue
        r = json.loads(line)
        who, data = r["who"], r["data"]
        if who in ("page_a", "page_b"):
            data = page(data)
        print(f"{r['label']:>20} {who:>13} {json.dumps(data, separators=(',', ':'))}")
