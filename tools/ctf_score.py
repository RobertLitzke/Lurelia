#!/usr/bin/env python3
"""Score a round of the CTF tournament (tools/ctf_round.sh).

Reads shots/ctf/rounds/<name>.txt, which holds four bench tables in order
(Sluice average, Sluice adept, ctfbox average, ctfbox adept), and prints the
plan's record in each: won-drawn-lost against the frozen base, the fixed
control and the rush, its captures a minute, and one number to compare
rounds by:

    score = sum over the four settings and the three opponents of
            (won - lost) / matches

so +12 is every match won everywhere, 0 level, -12 every match lost.

    python tools/ctf_score.py r00 [r01 ...]
"""

import os
import re
import sys

SETTINGS = ["sluice avg", "sluice adept", "ctfbox avg", "ctfbox adept"]


def parse(path):
    tables, cur = [], None
    for line in open(path, encoding="utf-8"):
        if line.startswith("coach v coach"):
            cur = {}
            tables.append(cur)
            continue
        m = re.match(r"\s*(plan v \w+|\s*\w+ side)\s+(\d+)\s+(\d+)\s+([\d.]+)\s+([\d.]+)", line)
        if m and cur is not None:
            cur[m.group(1).strip()] = (int(m.group(2)), int(m.group(3)), float(m.group(4)), float(m.group(5)))
    return tables


def score(name):
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    tables = parse(os.path.join(root, "shots", "ctf", "rounds", name + ".txt"))
    total, rows = 0.0, []
    for setting, t in zip(SETTINGS, tables):
        cells = []
        for opp in ("base", "fixed", "rush"):
            us = t.get(f"plan v {opp}")
            them = t.get(f"{opp} side")
            if not us or not them:
                cells.append("   --   ")
                continue
            won, drawn, caps = us[0], us[1], us[2]
            lost = them[0]
            n = won + drawn + lost
            total += (won - lost) / max(n, 1)
            cells.append(f"{won:>2}-{drawn:>2}-{lost:<2} {caps:4.2f}")
        rows.append(f"  {setting:<13}" + "   ".join(cells))
    return total, rows


if __name__ == "__main__":
    print(f"  {'':<13}{'v base  w-d-l caps':<21}{'v fixed  w-d-l caps':<21}{'v rush  w-d-l caps'}")
    for name in sys.argv[1:]:
        total, rows = score(name)
        print(f"{name}: score {total:+.2f}")
        for r in rows:
            print(r)
