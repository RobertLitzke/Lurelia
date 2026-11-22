# Kart course format (.course)

A `.course` file describes a kart circuit and the land it sits on, in a few
dozen lines of plain text. `course_file.py` turns it into a world with a race
in it: three laps against bots, launched as RACE. It is the kart-racing
counterpart of the arena format (`tools/arena/FORMAT.md`), and like that
format it is meant to be written by hand or handed to another agent as the
whole brief.

```bash
python -I tools/course/course_file.py tools/course/maps/kestrel.course          # build worlds/kestrel
python -I tools/course/course_file.py tools/course/maps/kestrel.course --check  # report on the layout only
cargo run --release -- --world kestrel
```

One line per setting or point; `#` starts a comment. Units are metres. **x runs
east and z runs south** from the world's north-west corner, as in the arena
format; heights are metres up.

## Lines

| line | what |
|---|---|
| `course NAME size S style ice\|downs` | Required, first. The world's folder name, its width (a square S metres across: 512 suits a circuit of 1 to 1.5 km), and the dressing (below). |
| `title TEXT` | The name the launch screen shows. Default: NAME in capitals. |
| `blurb TEXT` | A sentence for the launch screen and story.md. |
| `race laps N rivals R vehicle V` | Laps (default 3), bots racing the player (1 to 3, default 3), and the library vehicle everyone drives (default `kart`, `library/vehicles/`). |
| `road W [walls H] [bank K]` | Road width (default 12), wall height either side (default 0, no walls), and how much the turns are banked (default 1, 0 for flat). |
| `land BASE [HILLS]` | The ground's height (default 30) and the size of its gentle rolling (default 4). |
| `hill X Z RADIUS HEIGHT` | A rise centred at X, Z fading out over RADIUS; a negative HEIGHT is a hollow. Any number. This is where the climbs come from. |
| `point X Z [RISE]` | The centreline, **in the order the race runs**. The first point is the start line, and the race heads toward the second. RISE lifts the road above the ground there (default 0; negative cuts down): crests, humps, dips, embankments. |
| `hour H` | The hour the race is set at (default 14). |

## The road

The points are joined by a smooth closed curve, so they are control points,
not corners: put one every 30 to 60 m, closer through a bend. The road follows
the land, smoothed over 40 m, plus each point's RISE, eased between points.
Kerbs (1 m) edge both sides. With `walls`, a wall 1 m thick stands outside
each kerb, so a car can't leave the road; without walls the verges are the
style's ground, slower and slipperier than the road.

What makes a good course:

- **Length.** 1 to 1.5 km is a lap of 50 to 70 s, three laps about three
  minutes. The kart tops out near 108 km/h.
- **Bends.** Nothing under 15 m radius; 20 to 30 m is a good tight corner and
  60 m or more is taken flat out. One hairpin a lap is plenty. A run of
  alternating bends 40 to 60 m apart makes an S.
- **Straights.** One long one (150 m or more) for the start and for
  overtaking; drifts charge boost in the long bends before a straight.
- **Climbs.** Use `hill` for the big ones (10 to 20 m over 100 m or more) and
  RISE for the details. Keep grades within 15%: steeper and a kart slows hard
  and goes light over the top. Two hills of opposite sign close together (a
  hollow beside a knoll) make the steepest grades; RISE can fill the foot of
  such a climb. `--validate` measures the built road and names any steeper
  stretch. Keep RISE within 2.8 m of the ground: higher, and the
  road leaves a gap under it that nothing fills. A crest or hump on the way
  into a bend throws a kart wide; put it on a straight or after the turn.
- **Clearance.** Two stretches of road must stay apart by the road width plus
  room for both kerbs and walls (the report says how much). The course can't
  cross itself: there are no bridges.
- **Edges.** Keep every point 20 m or more inside the world.

## Styles

| style | what |
|---|---|
| `ice` | The look of the ice arenas: a road of packed blue-grey ice (it grips like rock), blue and white kerbs, glacier ice for the walls and the ground under them, packed snow beside the road, and the cold blue air of `worlds/kaldvik/look.json`. Set in summer so no snow lies on the road. Best with `walls`. |
| `downs` | Chalk downs under feather grass: asphalt, red and white kerbs, open verges. |

A new style is an entry in `STYLES` in `course_file.py`: materials for the
road, kerb and wall, the biomes, rock, weather, and optionally a world to
borrow a look.json from.

## What it writes

`worlds/NAME/`: `world.json` (with a `race` block and RACE among its modes),
`chronicle.json` (a `terrain` event for the land and hills, a `circuit` event
for the road), `story.md`, `report.txt`, and for `ice` a `look.json`. The
spawn is on the start line facing the second point. Rebuilding overwrites all
of them, so edit the `.course`, not the world.

`report.txt` (also printed) gives the lap length, the tightest bends with
where they are, the closest approach of two stretches, and an estimate of the
race time, then either `no problems` or a `PROBLEM:` line for each: two
stretches too close, a bend under 12 m, a point too near the edge, a RISE
that leaves a gap.

## Checking it in the game

```bash
cargo run --release -- --world NAME --validate                      # must say no problems
lurelia --world NAME --headless --ride race --ride-secs 200 --chase --screenshot out.png
```

`--validate` also measures the road as built: a grade over 15% is a problem,
with the distance round the lap and the place. `--gazetteer` gives the lap
length, the climb from lowest to highest, and the steepest grade.

The headless race puts a bot in the player's kart and prints the standings
each second: the place, lap, distance and how far off the centreline each car
is. A car more than half the road off for long is somewhere the bots can't
take; widen that bend or move its crest.

## A worked example

`maps/kestrel.course`, the Kestrel Ice Ring: 21 points, 1.3 km, walls 2.5 m,
a climb of 20 m over a hill and a dip through a hollow. `maps/peregrine.course`
is a second, on the downs with open verges: 27 points, 1.4 km, three hills.

```
course kestrel size 512 style ice
title KESTREL ICE RING
race laps 3 rivals 3 vehicle kart
road 12 walls 2.5
land 30 4
hill 300 190 150 15     # the hill the course climbs over
hill 105 320 75 -5      # a hollow to its west
point 130 420           # start / finish, heading east
point 250 428
point 360 420
point 428 388
...
point 338 196 2.0       # a crest
...
point 90 405            # the last bend onto the straight
```
