"""One round of the kart brain tournament: 6 karts, 3 brains with 2 karts
each, 9 races on each course (Peregrine and Ribstrand unless KART_COURSES
names others, comma-separated).

usage: python tools/kart_brains/run.py ROUND BRAINS.json [OUT_DIR]

The JSON is {"order": ["a", "b", "c"], "brains": {"a": {...}, ...}}, each
brain's fields those of `Brain` in src/race/brain.rs; "original" and
"default" are the built-in brains. It needs a release build in target/release
(or KART_EXE set to one). Each race's lineup and log, and the round's table,
go to OUT_DIR (default: kart_brains/ in the system temp dir).

Seat s (0 the player's kart, driven by the autopilot from the back of the
grid; 1-5 rivals) has kart type s % 3 (kart, heavy, quick) and tier
TIERS[s], for each of the three rotations of GGNNBB. In race r, seat s gets
brain (s + r) % 3, so over the 9 races on a course each brain drives every
kart type in every tier and the back seat three times: 18 results per brain
a course. The races are deterministic: the same lineup gives the same result.
"""
import json, os, statistics, subprocess, sys, tempfile
from concurrent.futures import ThreadPoolExecutor

GAME = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EXE = os.environ.get("KART_EXE", os.path.join(GAME, "target", "release", "lurelia.exe"))
OUT = sys.argv[3] if len(sys.argv) > 3 else os.path.join(tempfile.gettempdir(), "kart_brains")
TIERS = ["GGNNBB", "BBGGNN", "NNBBGG"]
COURSES = os.environ.get("KART_COURSES", "peregrine,ribstrand").split(",")
POINTS = [10, 7, 5, 3, 2, 1]


def race(rnd, job, order, brains):
    course, tiers, r = job
    karts = [order[(s + r) % 3] for s in range(6)]
    tag = f"it{rnd}_{course}_{tiers}_r{r}"
    path = os.path.join(OUT, f"{tag}.json")
    json.dump({"brains": brains, "karts": karts, "tiers": tiers}, open(path, "w"))
    # The whole world loaded (view and full detail past its 512 m), or karts
    # far from the player's run as ghosts along the centreline.
    cmd = [EXE, "--world", course, "--headless", "--ride", "race", "--ride-secs", "420", "--rivals", "5", "--laps", "3", "--chase", "--view", "700", "--full-detail", "700"]
    out = subprocess.run(cmd, cwd=GAME, env=dict(os.environ, KART_BRAINS=path), capture_output=True, text=True)
    log = out.stdout + out.stderr
    open(os.path.join(OUT, f"{tag}.log"), "w").write(log)
    rows = []
    for line in log.splitlines():
        if line.startswith("RESULT"):
            _, place, seat, brain, tier, kart, *t = line.split()
            time = 999.0 if t[0] == "dnf" else float(t[0])
            rows.append(dict(place=int(place), seat=int(seat), brain=brain, tier=tier, kart=kart, time=time))
    return dict(race=tag, rows=rows, ghosts=log.count("m~"))


def main():
    os.makedirs(OUT, exist_ok=True)
    rnd = int(sys.argv[1])
    spec = json.load(open(sys.argv[2]))
    order, brains = spec["order"], spec["brains"]
    jobs = [(c, t, r) for c in COURSES for t in TIERS for r in range(3)]
    with ThreadPoolExecutor(3) as ex:
        races = list(ex.map(lambda j: race(rnd, j, order, brains), jobs))
    score = {b: dict(points=0, places=[], times=[]) for b in order}
    for rc in races:
        if len(rc["rows"]) != 6:
            print(f"race {rc['race']}: {len(rc['rows'])} results")
        for row in rc["rows"]:
            s = score[row["brain"]]
            s["points"] += POINTS[row["place"] - 1]
            s["places"].append(row["place"])
            s["times"].append(row["time"])
        line = ", ".join(f"{x['place']}.{x['brain']}({x['seat']}{x['tier']} {x['time']:.1f})" for x in sorted(rc["rows"], key=lambda x: x["place"]))
        print(f"race {rc['race']}: {line}" + (f"  ghost lines {rc['ghosts']}" if rc["ghosts"] else ""))
    table = []
    for b in order:
        s = score[b]
        table.append(dict(brain=b, points=s["points"], mean_place=statistics.mean(s["places"]), mean_time=statistics.mean(s["times"]), wins=s["places"].count(1)))
    table.sort(key=lambda x: (-x["points"], x["mean_time"]))
    print("brain | points | mean place | mean time | wins")
    for x in table:
        print(f"{x['brain']} | {x['points']} | {x['mean_place']:.2f} | {x['mean_time']:.1f} | {x['wins']}")
    json.dump(dict(round=rnd, races=races, table=table), open(os.path.join(OUT, f"it{rnd}_result.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
