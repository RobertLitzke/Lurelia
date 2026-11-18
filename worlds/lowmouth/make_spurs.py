"""Give every stilt house in Lowmouth its own short walk to the nearest
boardwalk, and turn the ones that face away from it.

Unlike make_chronicle.py this one is kept: rerun it from the repo root
whenever the `bayou_walk` ribbons move or houses are added, and it rebuilds
the spurs from the chronicle as it stands. It is idempotent -- it drops the
spurs it made last time before making new ones -- and it edits
chronicle.json in place, so diff the result.
"""
import json
import math

P = 'worlds/lowmouth/chronicle.json'
SPUR = "the walk from the door to the boardwalk"
d = json.load(open(P, encoding='utf-8'))
ev = d['events']

ev[:] = [e for e in ev if e.get('cause') != SPUR]

# Every boardwalk line, sampled at a metre.
lines = []
for e in ev:
    if e['event'] == 'ribbon' and e['kit'] == 'bayou_walk':
        pts = [tuple(p) for p in e['path']]
        for a, b in zip(pts, pts[1:]):
            lines.append((a, b))


def near(p):
    """Nearest point on any walk, and how far."""
    best = (1e9, None)
    for a, b in lines:
        abx, abz = b[0] - a[0], b[1] - a[1]
        l2 = abx * abx + abz * abz or 1e-6
        t = max(0.0, min(1.0, ((p[0] - a[0]) * abx + (p[1] - a[1]) * abz) / l2))
        q = (a[0] + abx * t, a[1] + abz * t)
        dd = math.hypot(q[0] - p[0], q[1] - p[1])
        if dd < best[0]:
            best = (dd, q)
    return best


def dir_deg(b):
    r = math.radians(b)
    return (math.sin(r), -math.cos(r))


spurs = 0
turned = 0
missed = 0
for e in ev:
    if e['event'] != 'place' or e['structure'] not in ('stilt_house', 'stilt_camp', 'net_shed'):
        continue
    at = e.get('at')
    if not isinstance(at, list) or not all(isinstance(v, (int, float)) for v in at[:2]):
        continue
    if not (280 <= at[0] <= 620 and 530 <= at[1] <= 840):
        continue
    dist, q = near((at[0], at[1]))
    if q is None or dist > 46:
        missed += 1
        continue
    toward = math.degrees(math.atan2(q[0] - at[0], -(q[1] - at[1]))) % 360
    b = e.get('bearing', 0) % 360
    off = abs((toward - b + 180) % 360 - 180)
    if off > 80:
        # It faces away from its own walk: turn it to the quarter turn that
        # looks at it, which is what a porch on a boardwalk does.
        e['bearing'] = int(round(toward / 90.0) * 90) % 360
        b = e['bearing']
        turned += 1
    # The porch sits about 5 m in front of the anchor.
    dx, dz = dir_deg(b)
    porch = (round(at[0] + dx * 5.0, 1), round(at[1] + dz * 5.0, 1))
    dist2, q2 = near(porch)
    if q2 is None or dist2 > 46:
        missed += 1
        continue
    if dist2 < 1.2:
        continue
    ev.append({"year": -35, "cause": SPUR, "event": "ribbon", "kit": "bayou_walk",
               "curve": False, "path": [[porch[0], porch[1]], [round(q2[0], 1), round(q2[1], 1)]]})
    spurs += 1

json.dump(d, open(P, 'w', encoding='utf-8', newline='\n'), indent=1)
print('%d spurs, %d houses turned to face their walk, %d left unconnected' % (spurs, turned, missed))
