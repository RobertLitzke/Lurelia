# Kart tracks (.track)

A kart track is two things: the track, which is everything a kart touches,
and the world round it. The track is a `.track` file of a few dozen lines of
plain text. `track_file.py` resolves it into `worlds/NAME/track.json`, and
the engine stamps that into the world after the world's own chronicle, so
the road comes out the same, to the voxel, whatever the world does round it
and however the engine changes. The world is an ordinary world folder
(`world.json`, `chronicle.json`), written by hand or by an agent with the
worldgen skill, that may place things beside the road but never on it.

This file is the brief to hand an agent that is to design a track.

```bash
python -I tools/course/track_file.py tools/course/maps/NAME.track --check   # report on the layout only
python -I tools/course/track_file.py tools/course/maps/NAME.track           # write track.json (and a starter world if new)
python -I tools/course/track_file.py tools/course/maps/NAME.track --fresh   # rewrite the starter world too
cargo run --release -- --world NAME --validate
```

One line per setting or point; `#` starts a comment. Units are metres. **x
runs east and z runs south** from the world's north-west corner, as in the
arena format; heights are metres up. **Lap distance** (`S`, `FROM`, `TO`) is
metres round the road from the start line in the direction of the race, as
the race measures it. `--check` prints the lap distance of every `point`, so
you can find S for a place on the map. A stretch `FROM TO` may wrap past the
line (`1400 60`).

## The layout

| line | what |
|---|---|
| `track NAME size S style STYLE` | Required, first. The world's folder name, its width (a square S metres across: 512 suits a lap of 1 to 1.5 km), and the style (below). |
| `title TEXT` | The name the launch screen shows. Default: NAME in capitals. |
| `blurb TEXT` | A sentence for the launch screen and story.md. |
| `race laps N rivals R vehicle V` | Laps (default 3), bots racing the player (1 to 7, default 3), and the library vehicle (default `kart`). |
| `road W [walls H] [bank K]` | Road width (default 12), wall height either side (default 0, none), and how much the turns are banked (default 1, 0 for flat). |
| `materials [road M] [kerb M] [wall M] [deck M] [verge M] [post M] [pad M] [portal M] [light M]` | Materials in place of the style's: the road, its kerbs, the walls and parapets, the deck slabs, the run-off beside an open road, the gantry's posts, the boost pads, the tunnels' portal frames, and their roof lights. |
| `land BASE [HILLS]` | The land's height (default 30) and the size of the rolling the world adds over it (default 4; it goes into the starter world's `terrain`, not the road). |
| `hill X Z RADIUS HEIGHT` | A rise centred at X, Z, a bell about a third of HEIGHT high at RADIUS; a negative HEIGHT is a hollow. Any number. |
| `plateau TOP EDGE x,z x,z x,z ...` | Flat land at height TOP (metres, as `at`) inside the outline, falling to the land round it over EDGE metres outside it: a mesa or a bench with EDGE 3 to 6, a broad rise with 40 or more. Played after the hills, in order, so a later plateau stands on an earlier one. |
| `point X Z [RISE] [at Y]` | The centreline, **in the order the race runs**. The first point is the start line, and the race heads toward the second. RISE lifts the road above the land there (default 0; negative cuts down). `at Y` gives the road's height outright. |
| `hour H` | The hour the race is set at (default 14). |
| `shoulder M` | Land lower than the road is filled up level with it for M m past the run-off (default 10) before it eases away, so the road never has a ditch or a drop beside it; land higher than the road still comes down to it as a bank or a cliff. Decks, tunnels and covered stretches leave the land alone (a pier keeps its water). `shoulder 0` turns it off. |
| `lights EVERY` | Lamps along the road after dark, one every EVERY metres (about 25), alternating sides, each on a post (the gate's `post` material, or `bridge_steel`) with an arm over the kerb and a glass head. Where the ground beside the road is more than 2 m above or below it (a deck, an embankment, a cutting, a tunnel) the light hangs with no post. |
| `grid N` | Grid boxes painted behind the line (default 8). |

### The road

The points are joined by a smooth closed curve, so they are control points,
not corners: put one every 30 to 60 m, closer through a bend. The road's
height comes from the track file alone. A point with `at Y` is at Y; a
point without is the land (`land` and the `hill`s, no noise) plus its RISE.
The heights run through every point as one smooth curve that never
overshoots: a run of points with `at` up a slope is one even climb, the
road is level only where two neighbouring points are at the same height or
at a crest or dip, and between two points with the same `at` it is exactly
level. A climb that leaves a level stretch eases in over its first span,
steeper in the middle of it than the average; put a point a short way up
to keep it within 15%. Between two points without `at` the road also
follows the land's shape; next to a point with `at` it is the curve alone.
Between any two points with `at` it is not banked. Kerbs
(1 m) edge both sides. With walls, a wall 1.2 m thick stands outside each
kerb; without, an 8 m run-off strip of the verge material lies level beside
the kerb, then the ground eases back to the world's over 12 m.

What makes a good track:

- **Length.** 1 to 1.5 km is a lap of 50 to 70 s. The kart tops out near
  108 km/h.
- **Width.** About 14.5 m as a base (`road 14.5`) for a field of six; 12
  m is tight and anything under 10 m is a squeeze. Vary it with `width`
  stretches: 16 to 20 m on the start straight, in hairpins and on a
  landing, so those are forgiving and there's room to pass; 10 to 13 m
  through a technical run of bends, a lane, a bridge or a town, where a
  narrow road is the difficulty. Leave the road under a jump at the width
  `--check` flew it at: a lip much wider than its landing throws karts off
  the outside lanes clear of the road. Butt neighbouring stretches
  together (one's TO is the next one's FROM): a short gap between them
  eases out to the base width and back, a bulge that pinches the road just
  where the next stretch starts. A railless deck wants the road before it
  no wider than itself.
- **Bends.** Nothing under 15 m radius; 20 to 30 m is a good tight corner and
  60 m or more is taken flat out. One hairpin a lap is plenty.
- **Straights.** One long one (150 m or more) for the start and for
  overtaking.
- **Climbs.** Give the points up a climb `at` heights, or let them follow the
  land (`hill`, `plateau`) with RISE for the details. Where the road climbs
  onto higher land, a `ramp` stretch builds the land up to it instead of a
  deck.
  Keep grades within 15%; `--check` measures every 10 m of the road and
  names any steeper stretch. A crest on the way into a bend throws a kart
  wide; put it on a straight or after the turn.
- **Clearance.** Two stretches of road must stay apart by the road width
  plus room for both kerbs and walls, or pass at least 4.5 m one over the
  other (a bridge), or a tunnel's height plus 1.5 m over a tunnel.
- **Edges.** Keep the road 20 m or more inside the world.

### Bridges, tunnels and decks

Where the road stands more than 2.5 m over the land it is a deck: road and
kerbs on a slab 0.75 m thick of the deck material, a parapet each side as
high as the walls (1 m with none), and pillars every 12 m, never on a
stretch of road below. So a road can climb onto a deck and cross over
itself: give the points over the crossing `at` heights that put one road at
least 4.5 m over the other.

A `tunnel` stretch is bored through whatever land is there, the track's or
the world's: a hollow over the road, its kerbs and half a metre more each
side, the road laid on its floor, a metre of lining round it, and a light
panel in the roof every 8 m with a lamp under it (`works_lamp`, or
`materials light M`). The hollow and the road run on 10 m out past each end, so
the hill is cut back from the portal, and each portal has a frame 2 m wide
and 2 m over the roof (concrete, or `portal M`). The land over the tunnel
is left alone, so a road can run across the top of a hill or a mesa while
another goes through its base. Where the land over it is lower than its
roof, the lining stands out of it as a gallery, and nothing grows there;
`--check` says how much of the tunnel the track's own land covers. Give
the points through a tunnel `at` heights. A road 14 to 16 m wide through a
curved tunnel keeps karts off its walls.

For a short covered stretch an arch or a block will do: `add` it over the
road and `carve` its hollow from the road's height up. The road, its kerbs
and its run-off are cut down to the road's height under a shape, so the
block must span all of that. A carve whose floor is below the road digs a
hole in it.

Shapes are played after the road and override it: an `add` that stands even
0.25 m proud of the road surface is a step a kart can't climb. `--check`
names any `add` that stands into the road and isn't cut away by a later
`carve`.

## Stretches

| line | what |
|---|---|
| `surface FROM TO MAT` | The road is MAT on this stretch, on a deck as on the land: a sand stretch, a plank pier, ice. Use a smooth material (`pier_planks`, not `weathered_planks`): one that isn't draws a sloping road as voxel steps and stands a lip where it meets the next. Grip is the material's `surface`: `hardpack`, `rock`, `sand_wet`, `sand_soft`, `grass`, `mud` or `snow`. |
| `width FROM TO W` | The road is W wide on this stretch, eased over about 10 m at each end. |
| `walls FROM TO H` | Wall height on this stretch (0: open), in place of `road`'s. On a deck the rail is as high as the walls, 1 m with none, and none at all with a negative H (a railless boardwalk). Behind a wall a kart is out of bounds: it is put back after 3 s. |
| `verge FROM TO MAT [RUNOFF]` | The run-off on this stretch is MAT (`-` keeps the ground's own), RUNOFF metres wide. |
| `bank FROM TO K` | Banking on this stretch. |
| `bank FROM TO deg A` | A fixed bank of A degrees (up to 45, the steepest a voxel slope draws smooth), the outside of the turn up: a bowl's wall. The road is tipped about its inner edge, rising to it over 50 m either side. Use a smooth road material (`"smooth": true`) or it draws as steps a kart can't climb; and no walls, which stand on the centreline's height. 24 degrees is what the race's drivers handle round a 36 m bowl. |
| `ford FROM TO DEPTH` | The road dips DEPTH under the water's level (its height at the stretch's ends), eased over the first and last quarter, and the race fills the dip with water as the ground loads (race/course.rs). Karts wade (`wade` in the vehicle): water past half a metre slows them hard, and one that floats is drowned and put back. |
| `flood FROM TO D1 [D2 ...] [dip DEPTH]` | Water over the stretch that changes lap by lap: Dk metres over the road's height at the stretch's ends on lap k (negative: under it, none on the road), laid as the leader starts each lap. With `dip` the road dips as for a ford. A kart that drowns in a flood is put back on its far side, the metres skipped counted. |
| `landing FROM TO [V] [GAP]` | A landing hill: from the take-off at FROM (a `jump` there sets the launch slope) the road is shaped to the flight of a kart leaving at V m/s (default 30), GAP m (default 3) under its path, meeting it where it falls at 0.7, on down at that slope and curving level by TO. A long drop lands along the ground, as a ski jumper does. |
| `ramp FROM TO [WIDTH]` | The land is built up or cut down to the road on this stretch, eased out over WIDTH metres either side (default 30), so the road climbs on the land and never on a deck. For a road up onto a plateau. Across flat land it is a causeway; to make a slope of it, put a broad low `plateau` under it. |
| `tunnel FROM TO [HEIGHT] [mat M] [portal M]` | The road is bored through the land here (above), HEIGHT metres clear (default 6), lined in M (default the wall material), its portals framed in the portal material (default concrete). No walls or run-off inside. |

## Race furniture

| line | what |
|---|---|
| `items S [ACROSS] [KIND ...]` | A row of pickups across the road at S: ACROSS of them (default 4, or as many KINDs as given), the KINDs from the left: `gun rocket goo block disc boost shield overdrive repair`. Places without a KIND take the next from the race's cycle. With no `items` lines the race puts up to three rows on the straights. |
| `pad S [left\|centre\|right] [LEN]` | A boost pad starting at S, LEN long (default 6) and 3 m across, in the middle of the road or toward one side. A kart driving over it gets a full drift's kick. Bots with nobody to pass steer for one. |
| `jump S HEIGHT [LEN] [SKEW]` | A lip across the road: the road curves up HEIGHT over LEN metres (default 8) to S, steepening as it goes (a parabola: no kink where it starts, and 2 x HEIGHT / LEN steep at the lip), then falls back down a slip face, 1 in 2 and at least 3 m long, so a kart too slow to fly rolls down it. Give the ramp twice the length a straight one would need. SKEW turns the lip that many degrees from square, its right side further along (negative: its left), so it can lie along a dune crest that crosses the road at a slant; one side of a kart takes off first. S is where the lip crosses the centreline. `--check` works out where a kart at full speed lands and names a landing on a bend; it also flies karts off the lip from every lane across the road at full speed down to 30%, and names a jump where a wheel hits the ramp in flight or a kart comes down harder than 11 m/s square on (a level landing from full speed is about 9): a landing on the rise to the next crest, or into the next ramp. Put the road's dips about 40 m past each lip, so karts land going downhill. The race's drivers take a jump flat out, where any other crest they ease over. |
| `jump S HEIGHT [LEN] [SKEW] boost` | A jump meant to be flown from a pad: `--check` flies it at pad speed (about 137 km/h) and judges the landing only at that speed; a kart without the pad is meant to fall short. |
| `smash S left\|centre\|right OFFSET KIND [LEN] [mat M] [slow K]` | A prop a kart breaks by driving into it: `bale`, `stones`, `gate`, `cane`, `logs`, `crates`, OFFSET metres off the centreline (put it on the inside of a corner). It is stamped in a passable copy of its material, and the race takes its voxels away as a kart reaches it, throws debris and keeps K of the kart's speed: a tight line costs a little on lap 1 and is open after. |
| `wind FROM TO ACCEL BEARING [GUST]` | A side push of ACCEL m/s² toward BEARING (degrees clockwise from north), steady, or in gusts GUST seconds long every three times that. Dust streams across the road 1.2 s before each gust. |
| `gravity FROM TO G` | Karts weigh G times their own on this stretch. |
| `glide FROM TO G` | A kart that leaves the ground here glides until it lands: G of its weight, its fall checked, its path swinging toward its nose and turned by its bank. It is never out of bounds while gliding, and a glide over a loop of road counts as the shortcut it is. |
| `thermal X Z RADIUS LIFT [BASE]` | Rising air at X, Z: a gliding kart within RADIUS is lifted LIFT m/s². Seeds and leaves rise in it from BASE. |
| `herd FROM TO SPECIES N OFF1 [OFF2 ...]` | N animals put down along the stretch as the player comes up to it each lap: OFFk metres either side of the centreline on lap k (0: across the road; negative: none that lap). They graze and scatter, and the player's kart shoves them. Each lap's herd takes the place of the last lap's (a lap with no herd clears it), and a restarted race starts with none. |
| `arch S [SPAN] [HEIGHT] [THICK] [mat M]` | An arch over the road at S, square to it at any angle: its legs just outside the opening (SPAN m across, default the road and kerbs and 4 m), its underside a half ellipse HEIGHT m clear at the middle (default 8), its top squared off 3 m above, THICK m deep along the road (default 5). `--check` gives its clearance over the kerbs and names one under 5 m. Things close to the road and over it fill the chase camera's view: give a course one or two. |
| `checkpoint S` | A checkpoint. Going past one far out of bounds (more than 15 m past the kerb) gains nothing: the kart's progress waits at the checkpoint and it drives what it skipped again. Without any, there are two, at a third and two thirds of the lap. The HUD's splits are taken at them. |

## Shapes and structures

| line | what |
|---|---|
| `add SHAPE ... [mat M] [drive]` | A solid shape (below): `box`, `prism`, `section`, `pyramid`, `stairs`, `ramp`, in the same coordinates as the points, Y in metres. `mat` names its material (default: the wall's). For arches, grandstands, spires, a tunnel's block. `drive` marks one karts are meant to drive on (a shoal, a log ramp up to a boardwalk), which the check leaves alone. |
| `carve SHAPE ...` | The same shape cut out of whatever is there: a tunnel's hollow, a cutting. |
| `structure NAME X Z [BEARING] [at Y]` | A library structure (`library/structures/`), facing BEARING, on the ground or with its anchor at Y. |
| `structure NAME along S left\|right OFFSET [TURN] [at Y]` | The same, OFFSET metres off the centreline at S, facing the road (TURN turns it further, degrees). |

The shapes (`tools/course/shapes.py`):

| shape | what |
|---|---|
| `box X0 Z0 X1 Z1 Y0 Y1` | An axis-aligned box. |
| `prism Y0 Y1 x,z x,z x,z ...` | A polygon on the plan, its corners in order round the edge, with vertical sides from Y0 to Y1: angled walls, octagonal pillars. |
| `section z Z0 Z1 x,y x,y ...` | A polygon drawn in side view (x across, y up), pushed from z = Z0 to Z1: sloped roofs, chamfers, a stair profile. |
| `section x X0 X1 z,y z,y ...` | The same, drawn in z and y, pushed from X0 to X1. |
| `pyramid X0 Z0 X1 Z1 Y ax,az,ay` | A rectangle at height Y and a single apex point. |
| `stairs X0 Z0 X1 Z1 n\|s\|e\|w Y0 Y1` | (with `add`) Solid steps over the rectangle, climbing toward that side from Y0 to Y1, 0.25 m risers spread evenly. |
| `ramp X0 Z0 X1 Z1 n\|s\|e\|w Y0 Y1` | (with `add`) A solid wedge whose top slopes from Y0 to Y1. |

Anything a kart can touch belongs here, not in the world: something on the
road or its run-off, or over the road lower than 6 m. Shapes are written
into `track.json` as voxels; they are not library entries.

## Styles

`style` names a file in `tools/course/styles/`: `road`, `kerb`, `wall` and
`deck` materials, `gate_post`, optionally `pad`, `story` (a phrase for the
blurb), `world` (keys copied into the starter world.json: rock, biomes,
default and seabed biome, seasons, weather and so on) and `look` (a world
whose look.json the starter copies, or null). The verge is set per track
with `materials verge M`. `ice` is the ice arenas' look (best with walls);
`downs` is chalk downs with asphalt and red and white kerbs; `redrock` is red
rock country, a rolled red clay road with oxide and cream kerbs and laid
sandstone walls; `shore` is Ribshore's grey coast, a rolled grey sand road
with basalt and bone kerbs, basalt walls and bone gantry posts. A new theme is
a new file there; `materials` overrides any of its materials for one track.

## What it writes

Every build writes `worlds/NAME/track.json` (the road a sample a metre, its
furniture and shapes; generated, never edited by hand), `ground.bin` (the
land as the track sees it, eased to meet the road, 1 m cells) and
`report.txt`, and sets world.json's `race`, `spawn` and `spawn_bearing` from
the track.

The first build, or `--fresh`, also writes a starter world: `world.json`
from the style, `chronicle.json` with a `heightmap` event reading
`ground.bin` and a `terrain` event in `mode: "add"` rolling the land a
little, `story.md`, and the style's `look.json` if there is none. After
that the world is yours: later builds leave `chronicle.json`, `story.md`
and the rest of `world.json` alone.

## The world round the track

Add to `chronicle.json` as for any world (the worldgen skill's reference
has the events): rivers, a coast (`terrain` `sea`), dunes, a village,
woods, structures, signs. The engine stamps the track after all of it, so:

- The road, kerbs, walls and run-off take their ground outright, and the
  ground past them is eased back to the world's over 12 m.
- A structure the world places whose footprint covers the road or run-off,
  with its foot less than 6 m over the road, is taken out, with a warning
  naming it. `--validate` reports it as a problem. Plants never grow there.
- A river the world runs across the road goes under it.

Any position in an event may be given by lap distance:
`{"along": 640, "left": 45}` is 45 m left of the centreline 640 m round
(`right` for the other side). A `place` given that way faces the road unless
it has a `bearing`. `ribbon` takes `"along": {"from": 200, "to": 480,
"right": 14}` in place of a `path` (a fence or a wall beside the road), and
`region` and `area` take `"along": {"from": 900, "to": 1300, "left": [10,
120]}` in place of a `poly` (a band 10 to 120 m left of the road: a beach, a
named stretch).

## Checking it

```bash
python -I tools/course/track_file.py tools/course/maps/NAME.track --check
cargo run --release -- --world NAME --validate          # must say no problems
lurelia --world NAME --headless --ride race --ride-secs 200 --chase --screenshot out.png
```

`--check` gives the lap length, the lap distance of each point, the
tightest bends, the climb and steepest grade, crossings, decks, jumps and
where they land, the furniture and checkpoints, then `no problems` or a
`PROBLEM:` line for each. `--validate` builds the world and reports grades,
crossings too close, and anything the world stood on the track.

The headless race puts a bot in the player's kart and prints a line each
second: the standings (distance round, or the finish time; `~` marks a
rival carried round out of the loaded world), then `off` (metres off the
centreline, in the order YOU and the rivals as named) and `items` (what each
holds). A car more than half the road off for long is somewhere the bots
can't take; widen that bend or move its crest.

## Worked examples

`maps/peregrine.track`, Peregrine Ridge on the downs with open verges: 27
points, 1.4 km, three hills.

`maps/vermilion.track`, Vermilion Mesa: 48 points, 1.9 km, style
`redrock`. A figure-eight with no walls: the first loop runs along the
canyon floor and climbs two hairpins up Aspen Draw onto a mesa 31 m above
it, the road leaves the mesa down a fan of fallen blocks, and the second
loop comes back through a 200 m tunnel in the mesa's foot, under the road
it drove on the way out. The land is shaped with `plateau`, the climbs onto
and off the mesa are `ramp`s, and the bends the bots ran wide are banked.

```
track vermilion size 768 style redrock
race laps 3 rivals 3 vehicle kart
road 12
materials verge red_sand
land 30 1.5
plateau 62 5 222,305 228,276 ...      # the mesa, 32 m over the canyon floor
point 330 502                         # start / finish, heading east
...
point 655 428 at 31                   # the foot of Aspen Draw
point 648 317 at 40.9                 # hairpin A
...
point 534 385 at 62                   # the top of the draw, onto the mesa
...
point 250 342 at 60.5                 # over the tunnel
...
point 250 342 at 31                   # under the mesa road
...
ramp 345 730 30                       # the climb onto the mesa, built up to the road
tunnel 1642 1838 6
width 1610 1870 16
bank 440 530 2.5                      # hairpin A
```

Walls and shapes go on top of the same lines, for example a ridge with a
tunnel cut through it and a grandstand by the straight:

```
road 12 walls 2.5
add box 226 60 286 90 43 51 mat glacier_ice   # the tunnel's block
carve box 226 64.5 286 85.5 43 49             # and its hollow
add stairs 180 432 300 450 n 30 36 mat packed_snow   # the grandstand
```
