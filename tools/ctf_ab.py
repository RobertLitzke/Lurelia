#!/usr/bin/env python3
"""Score role changes against the baseline team (tools/ctf_ab.sh).

Reads shots/ctf/roles/<variant>.txt: four bench tables (Sluice average,
Sluice adept, ctfbox average, ctfbox adept), each one row of the changed side
("ab v plan") and one of the baseline ("plan side"). Prints, per setting,
won-drawn-lost and the two sides' captures, grabs, share of carries scored,
returns and kills a minute, then two totals:

    score  = sum over settings of (won - lost) / matches
    margin = sum over settings of (captures a minute, changed - baseline)

    python tools/ctf_ab.py none attack1 [...]
"""

import os
import re
import sys

SETTINGS = ["sluice avg", "sluice adept", "ctfbox avg", "ctfbox adept"]
ROW = re.compile(r"\s*(ab v plan|plan side)\s+(\d+)\s+(\d+)\s+([\d.]+)\s+([\d.]+)\s+(\d+)%\s+([\d.]+)\s+(\d+)m\s+([\d.]+)\s+([\d.]+)")


def parse(path):
    tables, cur = [], None
    for line in open(path, encoding="utf-8"):
        if line.startswith("coach v coach"):
            cur = {}
            tables.append(cur)
            continue
        m = ROW.match(line)
        if m and cur is not None:
            g = m.groups()
            cur[g[0]] = dict(won=int(g[1]), drawn=int(g[2]), caps=float(g[3]), grabs=float(g[4]), scored=int(g[5]), carry=float(g[6]), fell=int(g[7]), rets=float(g[8]), kills=float(g[9]))
    return tables


def score(name):
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    tables = parse(os.path.join(root, "shots", "ctf", "roles", name + ".txt"))
    total = margin = 0.0
    rows = []
    for setting, t in zip(SETTINGS, tables):
        a, b = t.get("ab v plan"), t.get("plan side")
        if not a or not b:
            rows.append(f"  {setting:<13} --")
            continue
        n = a["won"] + a["drawn"] + b["won"]
        total += (a["won"] - b["won"]) / max(n, 1)
        margin += a["caps"] - b["caps"]
        rows.append(
            f"  {setting:<13}{a['won']:>3}-{a['drawn']:>2}-{b['won']:<3}"
            f" caps {a['caps']:.2f}/{b['caps']:.2f}  grabs {a['grabs']:.2f}/{b['grabs']:.2f}"
            f"  scored {a['scored']:>3}%/{b['scored']:<3}%  rets {a['rets']:.2f}/{b['rets']:.2f}  kills {a['kills']:.1f}/{b['kills']:.1f}"
        )
    return total, margin, rows


if __name__ == "__main__":
    for name in sys.argv[1:]:
        total, margin, rows = score(name)
        print(f"{name}: score {total:+.2f}  margin {margin:+.2f} caps/min")
        for r in rows:
            print(r)
