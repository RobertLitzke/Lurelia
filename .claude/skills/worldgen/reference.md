# World and library reference

Everything here is checked by `--validate`. Source of truth: `src/library/` (one file per kind)
(entry formats), `src/world/chronicle/events/` (events, one file each), `src/world/flora.rs` (tree
forms, placement), `src/creatures/locomotion.rs` (movement verbs), `src/render/recipes.rs`
(texture patterns), `src/world/shape.rs` (shape ops, decay). Glassfall
(`worlds/glassfall/`) is a complete worked example.

Units: metres for world positions; voxels (0.25 m) inside structures; compass
bearings in degrees, 0 = north (-Z), 90 = east (+X). World x runs east, z south.

## world.json

```json
{
  "name": "glassfall", "size_m": 1024, "height_m": 128, "sea_level_m": 24, "seed": 7,
  "rock": [["basalt", 0.55], ["tuff", 0.3], ["stone_dark", 0.15]],
  "spawn": [812, 641], "spawn_bearing": 280,
  "default_biome": "mixed_forest", "seabed_biome": "seabed"
}
```

On generated land give the spawn as a found place, `"spawn_at": {"from":
"fjord_head", "by": [12, 0], "as_if": 90}`, which wins over `spawn`;
`spawn_bearing` turns with it.

## Positions

Wherever an event takes a position it may be `[x, z]`, a name, or an offset
from a name: `{"from": "fjord_head", "by": [30, -12]}` is 30 m east and 12 m
north of that place. Names come from the chronicle's `places` table, from a
`seat` event, or from `glaciate`, which names what it made. A found place may
face a way (a fjord's head faces up its valley, a shore inland, a cove out to
open water). With `"as_if": B` the offset is read as drawn for a place facing
bearing B and turned in quarter turns to suit the way it came out; a `place`
turns its structure, and `vent` and `lamp` their points, the same amount. Lay
a village out once, for the shore on whichever side you like, and it keeps the
water at its feet however the land regenerates.

Keep land between sea level and `height_m` (the voxel volume's top).

Optional fields:

- `biomes`: the library biomes this world may use. Every library biome
  competes for every cell, so always give the list; without it a world takes
  every biome, including ones later worlds add.
- `strata`: rock formations by height, `[[material, base_m], ...]` lowest
  first, each running up to the next base. Bare rock (cliffs, canyon walls,
  steep slopes) then shows level bands that line up across the whole world;
  without it `rock` gives random 1.3 m bands. A thin formation (1 m) makes a
  level line on every cliff that reaches its height (Redrock's high-water mark).
- `weathering` (1): multiplies the age of abandoned buildings for decay. A
  desert keeps its timber: Redrock uses 0.2.
- `moss` (1): how much moss old stonework grows; 0 in a desert.
- `season` ("summer"): the one season a world is always in. `seasons` says
  it better; this is the older way of writing `"seasons": ["autumn"]`.
- `water` ({"shallow": [r, g, b], "deep": [r, g, b]}, sRGB bytes): the sea's
  colour; the engine's jade-teal if left out. `"still": true` makes it still,
  murky water (a swamp, a bayou): no foam along the shore, a dim reflection,
  and the bed lost a hand down (Lowmouth). A biome may give its own
  `water` too (at most two biomes per world): open water over it, and any
  inland water (a cenote, a lake not joined to the sea) whose shore is mostly
  that biome, takes it.
- `blurb`: one sentence on what the place is, shown under the launch
  screen's list with the world picked. Write one for every world; without it
  the first sentence of story.md is used. `title` is the name the menus show
  where the folder name in capitals is wrong ("Deck 16"), and
  `"listed": false` keeps a bench or a test off the list.
- `start_hour` (15): the hour a new game starts at, 5.2 to 18.8 (Hemlow
  opens at 17.6, golden hour). A world with a short or long day should pick
  the hour on its own sun path: at latitude 64 in winter the sun is up from
  about 8 to 16.
- `sun` ({}): the sun's path (`render/daylight.rs` `SunPath`; GUIDE.md,
  "Time of day and light"). `latitude` (42): the summer sun stands
  90 - latitude + 14 degrees up at noon and the winter one 28 lower, so read
  it off the place, gently (Thornpan 30, Hemlow 48, Kaldvik 64). The clock
  keeps every day the same real length whatever the latitude and season;
  only the sun's height and the hours on the clock change. `lift_deg`
  (3.44): lighting and shadows never use a sun lower than this, so a low
  winter sun lights the snow and casts shadows of a sensible length
  (Kaldvik 20). `shadow_soft` (1): how much wider the shadow filter spreads.
- `snow_cover` (0): snow on whatever faces up (tree tops, ledges, roofs)
  that never goes, 0 to 1, for a snowbound world (Kaldvik 0.85); the
  season's own lying snow adds to it. Off the trees it only lies where the
  land is near level, so cliffs stay dark.
- `glide` ({}): the glider's air. `thermals` scales the engine's thermals
  (1; Kaldvik 0.2, since snow and ice make none worth having),
  `ridge_from` is the bearing the ridge-lift wind blows from and
  `ridge_speed` its strength (7 m/s): set them where the engine's own wind
  runs along a world's cliffs instead of into them (Kaldvik: 185 and 9, so
  the fjord's north wall works).
- `diamond_dust` (0): ice crystals sparkling in the air round the player on
  clear days with the sun up (Kaldvik 1).
- `aurora` (0): the strength of the northern lights on the best night
  (`render/aurora.rs`). Each night draws its own share and a fifth are
  quiet. 1 is a full display; leave it 0 anywhere south of the Arctic.
- `weather` ({}): the climate, as kind name to weight -- `sun`, `heat`,
  `clouds`, `rain`, `snow`, `fog`, `storm` (`src/world/weather.rs`; GUIDE.md,
  "Weather"). A kind left out of a block that has anything in it never
  happens there, so a desert gives `{"sun": 1.5, "heat": 1.0, "clouds": 0.5,
  "fog": 0.1}` and never sees snow or rain; a rainforest leans on rain and
  fog and leaves snow out. Left out entirely, every kind turns up at its own
  default, which suits a temperate world. **Every shipped world states one**
  (a test pins that), so write one for a new world too, read off its own
  story rather than its name. Worth copying: Thornpan's flood arrives in the
  dry season, so it is `{"sun": 1.6, "heat": 1.3, "clouds": 0.3, "rain": 0.1,
  "fog": 0.15}` and never snows; Windbarrow leaves `heat` out of the list
  because heat is a windless day and the wind on those downs never stops;
  Slagdeck leaves `fog` out because a deathmatch should not turn on how far
  you can see; the duel bench lists `{"sun": 1.0}` alone, and since nothing
  sun can turn into is allowed there the weather simply holds. A kind a
  world asks for that nothing is called is reported by `--validate`.
- `start_weather`: the kind of day a new game opens on. Left out, it opens
  on the commonest kind the climate allows. Every shipped world names the sky
  its `look.json` was graded under, so the first frame of a world is the one
  it was tuned as, and the drift takes over from there.
- `seasons`: the parts of the year this world has, as a list of
  `"spring"`, `"summer"`, `"autumn"` and `"winter"` in any order (the year
  always goes round in that order, visiting the ones named). See GUIDE.md,
  "Seasons". Left out, a world is summer all year and never turns. Write one
  from the place:
  all four for a temperate world, `["spring", "summer", "autumn"]` where
  there is no winter to speak of, `["spring", "summer"]` for the tropics
  (a wet season and a warm one, nothing ever bare), and one season alone
  for an arena, where a match should be played under the same sky every
  time. The delta is worth copying: Thornpan has `["summer", "winter"]` for
  its wet and dry, and because its climate has no snow in it, its winter is
  a dry season with bare trees and no snow at all. A season a world asks for
  that nothing is called is reported by `--validate`.
- `start_season`: the one a new game opens in; the first its list has,
  going round from summer, if left out. A world that opens in winter opens
  in the middle of one, snow already down.
- `noise_at` ([0, 0]): added to every point before `landmass` and `subside`
  read their noise. A world trimmed after it was made (Lowmouth lost 152 m
  off the west: every x moved by -152, `noise_at` [152, 0]) keeps the same
  coast and islands under its moved features.
- `mirage` (0): how strongly far flat ground mirrors the sky in the heat of
  the day, 0 to 1 (GUIDE.md, "Sheet floods, reflections and the mirage").
  Saltmirror's salt pan is 1.
- `snow_line_m` (72): ground above this height turns to snow. A warm world
  with tall towers sets it out of reach (Jadewater: 1000).
- `tide` (none): `{"range": 2.5}` for a sea that goes out 2.5 m and back
  in 18 game hours; `high` (6) is the hour of the first high water and
  `period_h` (18) the hours from one high to the next.
  `sea_level_m` is high water, so the tide only uncovers seabed and never
  floods the land. Only open sea reaching the map's edge moves, plus the
  lower reaches of rivers running into it; lakes and sealed hollows stay
  still (GUIDE.md, "Tides"). Leave it out for a lake, an arena or a
  sheltered sea.
- `surf` (none): `{"height": 1.2}` for breakers about a metre high on a fair
  day; `length_m` (16) and `period_s` (7) set the wave spacing. Lagoons and
  coves are calmer by themselves. 1.2 is an open beach, 0.3 a sheltered
  bay; leave it out for a still sea (GUIDE.md, "Surf"). It also brings a
  swell on the open sea, half the breaker height unless `swell` says
  otherwise (0 for none), its longest wave `swell_length_m` (40) long.

The sea is the only water, so a world is always an island in it; a dry world
can call it a lake or a reservoir (Redrock).

## chronicle.json

```json
{ "places": { "village": [585, 655], ... },
  "events": [ { "year": -150, "event": "settlement", "cause": "...", ...params } ] }
```

`year` is negative (years before now); events run oldest first. Any position
parameter (`at`, `from`, `to`, `centre`, `breach`, list items in `path`) takes
`[x, z]` or a name from `places`. Every event needs `cause`.

Each event's noise is seeded by its kind, its year, and which event of that
kind in that year it is, so adding an event leaves the others' land where it
was. An event may still set `seed` to any number to take that noise instead.

Some older worlds say `"seeds": 1` in their `world.json`, which keeps an
earlier scheme: there an event is seeded by its place in the
year-sorted list, so adding an older event reshuffles every event after it
(islands, meanders), and `seed` pins that place, 1-based -- which is how
Lowmouth holds its land's events after events were added ahead of them. Leave
`seeds` out of a new world, and never add it to an old one: it would reshuffle
the land that is already there.

| event | params | does |
|---|---|---|
| `terrain` | base (m), hills, ridges, rough (m), vary (0.6), warp (60 m), scale (1), relief [{at, radius, height, stretch [bearing, factor]}], courses [{path, width, depth, depth_end, walls}], sea [{edge, reach, bays, rise (60), skerries}], shelf (14) | the land from a loose sketch, in place of `landmass`: rises and basins (`relief`, height above or below `base`), broad sags for water to find (`courses`; `walls` toward 1 makes one a block let down between two faults, with a level floor and a scarp each side), and sea along whichever edges are named (none: land to every horizon). A course's `width` is its half-width. A deep walled course that leaves the map at both ends and goes under the sea is a drowned river canyon with land on its far side (Redrock's Lake Corwin: width 64, depth 42 from a base of 46 over a sea at 10, walls 0.75), which reads far better than sea along an edge. Everything is read through a warp, so nothing comes out as drawn, and `vary` makes hills, ridges and knobbly ground strong in some quarters and faint in others. Follow it with `drainage`. Keep the highest ground it reports under `height_m` |
| `drainage` | name, points (9), sinks [pos], iters (24), strength (0.25), exponent (0.45), diffuse (0.12), wander (1.6 m), source (2500 m2), inflow [{at, area}], edges (false), step (2) | running water over the land as it stands: every stream cuts toward the sea, big ones fastest, so valleys branch and wind where the water went. It cuts back from the sea: few `iters` leave gorges near the coast and plateau behind. `inflow` is a river arriving from beyond the map (`area` its catchment, 1e6 = a square kilometre), put on the edge where a `course` begins. `edges` lets water leave over the world's edge, for a country with no coast. A hollow below sea level that does not reach the edge is a lake, not sea; with no sea at the edge at all the water ends in whatever lies below sea level (an inland delta: Thornpan). `name` names the biggest river: `<name>_source`, `_1`..`_9`, `_mouth` (faces the way it arrives); give those to a `watercourse` as `via` (or to a `river` as its `path`) and it runs down the valley this cut; ask for more `points` on a long winding river or the line cuts its corners. `sinks` are places water ends above the sea, a lake with no outlet: the river that reaches one stops there (Windbarrow's Saltmere, with `edges` on for everything else). An `inflow` need not be on the edge: a spring is one |
| `glaciate` | name ("fjord"), inflow, min_area (40000), width, depth, side (each [least, most]), rim, shape (2), ledge, floor_mat, floor_min (1), sill {top, length, inset}, cirque_above, valleys [{name, near, poi, cirque {radius, ice, ice_m, poi}}], poi | an ice age: a glacier down every valley that collects `min_area`, each cutting a `trough` sized by its ice, so the trunk goes far deeper than what joins it, tributaries hang, and where the trunk's floor is under the sea there is a fjord. Where the head of the fjord falls depends on the land before it: raise the upper `course` (less `depth`) to bring it west. A second big `inflow` gives a second arm. Names `<name>_head` (faces up the valley), `_valley`, `_mouth`, `_sill`, `_1`..`_9` down the water; and per valley `<v>_head`, `_1`..`_3`, `_above`, `_lip`, `_out` (an `icefall` runs from `_above` toward `_out`) |
| (escarpment trap) | | an `escarpment` does not lift ground that is more than a metre under the sea, so a hollow or a deep `course` in low base land stays behind as a pit in the plateau, and a lake put there fills at sea level (Hemlow's first try). Keep base land above the sea wherever a scarp will lift it |
| `strandline` | levels [m, ...], band (4), strength (0.8) | the shore of water that stood at a height for a long time and is gone: a bench cut into every slope at each level with a low cliff behind it, following the contour round every hill, broad in places and faint in others. Thornpan's old lake. Put it before `drainage` so the streams cut through it |
| (rivers) | `river` and `watercourse` both take rise (0 m), from_edge (false); `river` also curve (false, see `canyon`) | **A river must come from somewhere.** With neither, it starts full-grown at its first point, which reads as a river from nowhere. `rise` grows it from a trickle to full width and depth over that many metres (150 to 300 for one that rises at a spring; mark the spring with rocks, wet ground and a name, as Windbarrow does). `from_edge` carries a river that arrives from beyond the map back to the edge. A river that runs out of a lake needs neither |
| `seat` | name, near, within, rule, elev [lo, hi], size (12) | finds a place within `within` m of `near` and names it: `flat`, `high`, `low`, `shore` (faces inland), `cove` (sea with land round it; faces open water), `sea` (open water). `elev` is metres above the sea. Use it for anything that must sit on a kind of ground: a hot pool on a shelf, a wreck in a cove, stacks off a headland |
| `landmass` | centre, radius, roughness (0.5), hills (m), ridges (m), shelf (14), edge_fall (140) | a round island (for anything else use `terrain`): warped coast, rolling hills; sets base moisture. `shelf` is the sea floor's depth off the coast: 3 makes a shallow lagoon you can see the sand through (it still falls to 14 m over the last `edge_fall` m before the world's edge; Lowmouth's 1.4 m sound uses 60) |
| `volcano` | at, radius, height, crater_radius, crater_depth, flows | cone with radial gullies, summit crater, lava tongues ending in sea cliffs |
| `ashfall` | from, thickness, reach, depth, mat | buried band `depth` m down, thinning with distance |
| `impact` | at, radius, depth, rim, from_bearing, strewn_length, strewn_width, fragments, floor (mat), item (kind), tell (mat, optional), strangeness | crater with glassy floor, rim thrown downrange, strewn field of buried items (biggest furthest), a `<item>_mother` at the far end, strangeness field |
| `bloom` | at, radius, strength | adds strangeness around a point |
| `river` | name, path [...] (source first), width, depth, bed (mat), bank (mat, optional), meander, floor (optional) | carves a channel to the sea or into a lake; a path that starts inside a lake runs out of it at the lake's level (a pool on a tower top spilling over the edge). Where its water drops 2.5 m or more at a slope of at least 0.6 it goes over a waterfall: a falling sheet, spray and mist, drawn without any structure (see `--validate`, which lists every fall). After all events it gets water in pools and rapids, about `depth` x 0.8 deep in the middle (1.4 to 2.4 m, swimmable), a rounded bed of `bed` with `bank` along the waterline. The bed must fall a little at every step, which across flat country adds up to a gorge: `floor` (m) is the deepest it may be carved under the sea (Lowmouth: 2.6). `water` (mat) from older worlds is read and ignored |
| `lake` | name, at, radius, depth, bed (mat), bank (mat, optional), ragged (0.3), stretch (1), bearing, steep (false) | digs a basin `radius` m round (shore wandering by `ragged`, `stretch` times longer along `bearing`) and fills it to 0.3 m below the lowest ground round its shore; still water with banks like a river's. A river whose path ends in it runs in at its level. Put it in a hollow: on a slope the uphill shore is cut back, unless `steep` (the ground round it is left as it is: a plunge pool at the foot of a fall, a tarn in rock). A lake at sea level is drawn by the sea's own surface |
| `avulsion` | river, breach, path [...], old_bed (mat), silt (mat), silt_thick | river breaks out at `breach` on a new course; old bed below dries and half fills |
| `subside` | at, radius, drop [lo, hi] or level [lo, hi], grain (60), edge (40), ragged (0.25), stretch (1), bearing, islands (0), island_size (28), island_rise (0.7), floor | land that sinks (a lobe starved of silt, a settling marsh): inside a ragged region the ground drops by `drop` m, or down to `level` m under the sea (deeper ground, an old channel, stays), varying in patches `grain` m across and easing out over `edge` m. Under the sea it floods. `islands` (0-0.6) keeps that share standing as low knolls `island_rise` m over the sea (hammocks, keys); `floor` caps the water's depth. Lowmouth's swamp, bay, flats and mouth |
| `bayou` | path [...], width, depth, bank (5), meander (8) | a slow channel of sea water `depth` m deep through low ground, from one stretch of water to another (a crevasse, a slough). No source and no current; floods end to end. A `vale` can't do this: its floor stays above the sea |
| `earthwork` | parts [{from, to, width, top}], arcs [{centre, radius, from, to, width, top}], slope (1.8), relative, mat, poi | earth heaped up by people: ridges along segments and arcs (bearings `from` to `to`, clockwise) with flat crests `width` m across standing `top` m over the sea (over the ground with `relative`), sides at `slope` run per rise; overlapping pieces join, the higher wins. Lowmouth's bird effigy mound (body, head, wings, tail as parts) and the Poverty Point half rings (arcs). `mat` lays everything it raises in that material instead of the biome's ground: Gannet Harbor's breakwater is a granite earthwork with the `breakwater` kit along its crest |
| `deposit` | at, radius, thickness, mat, shape, depth (1) | raises the ground with a layer, burying what was there (walls too). `shape: "dome"` eases it out to nothing at the rim with no step: a knoll, a low hill, an islet in a lake (run after the lake). `depth` is the deepest water it may build up out of, so a bank can stand out of a lagoon floor three metres down (Jadewater's bay hills) instead of stopping at the shore |
| `settlement` | name, at, radius, houses, buildings [structure...], extras [{structure, count, near: centre/river/edge}], slots {slot: material}, path (mat) | picks flat dry sites, levels pads, faces buildings to the centre, paths to the centre; paints region `name` |
| `place` | structure, at, bearing or face (pos), slots, age, poi, height, when (hours, far, near, moon) | one structure (landing, shrine, anomaly). `height` hangs it with its anchor voxel at that height above sea, whatever the ground: a rope bridge strung between two rock walls at mid-height, its ends buried in the rock (run a `passage` along the same line for the tunnels it lands in). On a passage floor, use the floor's height, not the passage's (see `passage`). `when` makes it stand only some of the time, the rest of the time its plot empty: `{"hours": [from, to]}` (wrapping past midnight, [23, 3]), `"far": m` (seen only from further than that: gone when you get there), `"near": m` (only within that), `"moon": true` (only while the moon is out, the sun more than 5 degrees down, so it follows dusk and dawn through the year; false: only while it is not); all given must hold. The game flips it and redraws the ground under it (Lowmouth's juke joint, the man at the crossroads, the ghost steamer, the light in Ribshore's titan's eye). Give a hung structure `base: "rim"` too, or it is dropped onto the ground with the logs and boulders |
| `spires` | structures (list), at, radius, count, spacing (8), poi | tall landmarks picked at random from the list, turned at random, at least `spacing` m apart; base "sea" ones stand in water 1.5 to 16 m deep, the rest on dry free ground. Usually one to a few (a sea stack or two, a spire, a tower); a field of dozens (hoodoos) should be very rare, at most one per world. Library: `sea_stack`, `sea_stack_low`, `glass_spire`, `ruined_tower`, `hoodoo`, `hoodoo_tall`, `hoodoo_squat` |
| `abandon` | settlement | sets that settlement's age from the event's year: decay |
| `fire` | at, radius, wind_bearing, stretch, char (mat) | burn mask stretched downwind; sets burn year (biomes use burn age) |
| `region` | name, at + radius, or poly [...] | paints a named region for biome rules: a ragged disc, or with `poly` exactly the inside of that outline (a walled field: give the same points to a `ribbon` with a wall kit, as Hemlow does) |
| `escarpment` | from, to (or path [...], region), height, width (10), reach | a long rock face: ground right of the line (facing from -> to) stands `height` m higher, cliff with overhangs between, fading past the ends. Without `reach` only a circle round the line's middle (half its length + 4 widths + 40 m) rises, so the raised side ends in a circular step; for cliff lines that cross a world give `reach` (metres the raised side extends; 2000 = all of it). **Prefer `path`**: five or more points not in a row, started and ended off the map, and the face follows a curve through them with noses and bays of its own and a height that varies; the ground on its right, facing along the path, is the high side. `region` names that high side so a biome covers exactly the plateau (Windbarrow's Flint Down), which a `region` disc never does. Put `drainage` after it and streams cut combes into the face |
| `vale` | path [...] (head first), width (floor), depth, depth_end, side (m), meander (6), fall | a soft river valley: a floor `depth` m under the ground round it, falling gently downstream, and grassy sides easing up to the old ground over `side` m, no rim and no bare rock (the lowland `canyon`). `fall` caps the floor's steepness (run per metre of drop) so where it meets a scarp the floor is cut back and the combe opens out at the foot; leave it out for a river valley that goes over a fall. Run a `river` along the same path afterwards |
| `canyon` | path [...] (head first), width (floor), depth, depth_end, wall (0.6 m run per m), bench (8 m), meander (6), floor (mat), curve (false), poi | a gorge: flat floor falling downstream, never steeper than 1 in 7 (where the ground drops at a cliff line the floor upstream is cut deeper, making a slot), walls climbing in benches at fixed heights (they line up with `strata` and between canyons). `depth_end` tapers the depth along the path. Short canyons into a cliff line break it up; a deep wide short one is an amphitheatre. Floors below sea level drown, never deeper than 3 m under the sea: a very large `depth` (300) on a short path makes a round flooded sinkhole, a cenote. Run a `river` along the same path afterwards for water. `level` cuts the floor to that height above the sea all along in place of `depth` (a pit that must meet a cave network; a canyon shorter than 30 m never reaches its `depth` otherwise). `curve` bends the path into a smooth curve through its points first: use it, on the canyon and its river both, whenever the path is a `drainage` river's named points, which otherwise give a row of straight stretches with a corner at each (Redrock's Sorrel: canyon `meander` 0 so the two keep to one line, river `meander` 2). `depth` is below the ground as it stands, so after `drainage` has cut a valley ask for less |
| `trough` | path [...] (head first), width, width_end, floor, floor_end (absolute m), rim (absolute m), side (m), shape (2.5), ledge (0 m), meander (4), floor_mat, sill {at, top, length (40)}, poi | a glacial U valley: a flat floor at an absolute height (under the sea for a fjord) and walls that steepen as they climb toward `rim` over `side` m, with avalanche chutes notched in and snow-holding couloirs. `ledge` steps the walls in lava-flow shelves that hold snow (Kaldvik 7-9). It only lowers, so cut a tributary with a higher floor first and the main trough after: the tributary is left hanging at a lip high on the main wall (a hanging valley, where its waterfall goes). `sill` is the moraine ridge across a fjord's floor near the mouth. Lower `shape` (1.7) gives a steeper foot, as a fjord's walls go straight into deep water |
| `cirque` | at, radius, floor, rim (absolute m), opens (bearing the bowl opens toward), ice (mat), ice_m (6), poi | an armchair bowl with a near-sheer headwall round three sides, drawn out toward `opens`; with `ice` a domed glacier in it with crevasses where it bends toward the mouth. Put its floor above the head of the trough it feeds, or the trough has already cut below it and the bowl carves nothing |
| `dip` | at, radius, bearing, grade (drop per m), warp (m) | tilted beds: the strata over a region slant down toward `bearing` (0.04 is about 2 degrees), fading to level over the last third of `radius`. Only the beds move (`Fields::lift`), not the ground; `canyon` benches with `irregular` and `trough` ledges cut afterwards follow them, so put it early. Use it so not every cliff in a world is ruled level (Redrock's west and east, Kaldvik's outer fjord) |
| `fell` | above (m), at, radius, wind (bearing), scour (0-1), rock, tors (0-1), tor_height (4), polygons (0-1), gravel, drifts (m) | what wind and frost make of high snowy ground: patches blown bare to `rock`, black knobs (tors) standing through the snow, frost-polygon patterned ground with `gravel` in the cracks, and sastrugi ridges across the wind. Leaves cliffs and anything already surfaced alone |
| `spring` | at, radius, ring (10), sinter (mat), stain (mat), vents (5), steam (6 a s), fx (steam) | a hot spring or fumarole field: snow-free sinter terraces stained orange in patches, and steam vents that breathe particles all the time (`World.vents`, emitted within 220 m of the camera). Put a `lake` at the same spot first for a pool |
| `sea_ice` | at, radius, ice (mat), young (mat), snow (mat), above (0.3 m), leads (0-1), poi | a walkable ice sheet over a sheltered bay: old ice with snow drifts in the middle, young grey ice toward the open water, broken floes at the edge, `leads` of black water across it. Covers only what was under the sea |
| `icefall` | from, toward, width, ice (mat), blue (mat), stream [path], poi | a frozen waterfall built as voxels: walks from `from` toward `toward` to find the lip, then plasters ice bulges and icicles down the face to the sea, with an ice cone at the foot; climbable. `stream` is the frozen stream on the valley floor above. Needs the `icefall` library structure (it has `base: rim`, or the gravity pass drops it to the lowest ground under its footprint) |
| `vent` | at, points [[east, south, up]], fx, rate (2 a s), spread (m), on_sea | places that breathe particles all the time (chimney smoke, sea smoke over open water with `on_sea`); emitted within 220 m of the camera |
| `mesa` | at, radius, height or top, warp (0.25), stretch [bearing, factor], skirt (m), talus (0.3), cap (mat), poi | a flat-topped block standing `height` m over the low fifth of the ground round it, sheer sides then a talus skirt. Small radius (4-6 m) and tall = a spire; `stretch` [0, 8] with radius 4 = a north-south fin; radius 30, height 3, cap slickrock = a rock bench. `top` instead of `height` sets the flat top's height above sea level whatever the ground, for towers that must match (a `base: "rim"` bridge between two). Blocks packed close make a tower-karst maze whose gaps are the gorges (Jadewater's Old Wall); that avoids crossing `canyon`s, whose floors never rise again after a junction |
| `road` | path [...], width, mat, shoulder (mat), smooth (80 m) | a graded road on a smooth curve through the points: ground cut and filled to heights averaged over `smooth` m, shoulders, cut feathered back. Where the ground is more than 3 m under the grade (a canyon) it is left alone and the gazetteer lists a "road gap to bridge" with the deck height: place a `base: "rim"` structure there (`arch_bridge`). Use a smaller `smooth` (40) where the road climbs a wash |
| `ribbon` | path [...], kit, closed, curve (true), meander (0), smooth (40), height, poi | one line built to a kit (`library/kits/`): a fence, a wall, a boardwalk, a skywalk, an adit. The kit holds the whole cross-section; this says only where the line goes. `closed` joins the last point back to the first (a pasture, a ring road), `height` holds it at a fixed height above sea level whatever the ground (a bored passage, a walkway strung between two towers). Drawn per voxel column, so it takes any angle and any length: use a kit, never a `thing_20`/`thing_40`/`thing_60` family of structures |
| `route` | from, to, kit, terms {grade, water, wade, taken, step}, smooth (60), poi | a line that finds its own way: an A* search over the land where climbing past `grade`, open water, cliffs and ground an earlier event took all cost it distance. `wade` is the deepest water it will simply walk through: shallower than that a crossing is a ford, the track going into the water and out the far side, which across a delta in flood is what a track does. Both ends are always standable, so a route may begin or end on a landing. Where the land falls more than 3 m below the grade it builds the kit's `span` across the hole and puts piers under it, so a road over a gorge comes out as a bridge and not as a note in the gazetteer. Report lines give each gap's position, length and deck height |
| `network` | nodes {name: pos}, links [{from, to, kit, terms}], terms, smooth (60), height, wander (6) | several routes laid together, widest first. Each line laid makes its own ground dearer, so the next joins it at a junction rather than running beside it; where two or more meet, both are trimmed back and the crossing is aproned. A link's `from`/`to` names a node or a chronicle place. **Every cave is a `network` with a `height`**: all links are laid level at that height above the sea, and with a `bore` kit (`cave_passage`, or Hemlow's paler `lantern_passage`) that is a cave system, open wherever a link leaves the rock (a mouth in a cliff, a pit). Underground the links are not searched for; each runs node to node swinging up to `wander` m, so the nodes give it its shape: join them in loops, leave a dead end or two, and put a node outside the cliff for each mouth. Chambers are a short fat `passage` at a node (`height` = the network's + 0.55 x radius, a `lining`, a `shaft` for daylight); a pit open to the sky is a `canyon` with `level`. A cave is black without a `lamp` event (use its `level`): one lamp per node and one per link's middle is enough to walk by. Keep at least 6 m of rock over the roof: a river bed or a valley floor closer than that breaks in. Hemlow's Lantern Hole is the example |
| `village` | name, at, radius, layout, street (kit), lane (kit), houses, buildings [...], bearing, block (46), spacing (16), setback (3), plot (5), walk (mat), edge (kit), extras, slots | a settlement laid out on its own streets. `layout` is `grid`, `radial`, `organic` or `linear`, turned by `bearing` (a quarter turn suits a grid or a linear plan: the buildings then sit square to their street). Plots come off each street edge every `spacing` m, alternating sides, `setback` m back on a square of ground `plot` m either side (small on a sand island where a round house is all that fits, large for a farmstead); each building faces the street it is on and gets a `walk` path from its door to the kerb. `edge` rings the whole thing in a kit (a wall, a hedge). Prefer this to `settlement`, which scatters houses in a disc all facing the middle |
| `watercourse` | name, from, to, via [...], width, depth, bed (mat), bank (mat), meander (14), floor, branches [{name, to, at, width, depth, meander}], terms {downhill, max_climb, step, taken} | a channel that finds its own way down: only the rise of a step is paid for, falling is free, and a step climbing more than `max_climb` (0.4 m) is refused, so the line is one water could take. `via` points keep the geography a world is built round (the bend a village stands on) while the line between them is found. Each branch leaves the trunk `at` a fraction along it and goes to its own mouth, paying three times over for the trunk's own ground so it diverges: that is a delta in one event. A channel stops where the water is already deeper than itself, and its last quarter is cut shallower and narrower so it runs out instead of ending in a blunt trench. What it cuts is registered as a river under `name`, so `settle_water` fills it and an `avulsion` can break it out later. Prefer this to `river` for anything that branches or crosses country |
| `track` | path [...] (a closed loop), width, mat, scale (1), poi | a dirt-bike course: graded, berms banked on the outside of every turn tighter than about 28 m radius, and jumps by straight length: 46 m+ a tabletop, 30 m+ a double (kicker, gap, landing), 16 m+ rollers. Give 10-12 points, with tight corners between straights |
| `lava_tube` | path [...], radius, depth, skylights | a walkable tunnel `depth` m down along `path`, opening at both ends, with collapsed holes in its roof |
| `passage` | path [...], height, radius, lining (mat), wall (mat), shafts, wander (0), poi | a level cave passage `height` m above sea along `path`, open wherever it leaves the rock: through towers, ridges, round a cenote at the waterline (an undercut overhang). `lining` turns the rock over its roof to a material (a glowing ceiling), only where there is rock above and to both sides; `wall` turns all the rock round it, floor included, to another (dark wet stone), and with a wall the lining grows in patches on the top of the roof; `shafts` chimneys up to the surface. A very short path deep under the sea floor with one shaft is a blue hole. `height` is the tunnel's centre line; its floor is flat at `height - 0.55 * radius`, which is the `height` to give anything placed on it (a den, a find, a bone bed): at the passage's own height it floats, and at `height - radius` it is buried under the floor |
| `tower` | at, radius, height or top, bulge (0.5), flute (0.1), taper (random 0.25-0.55), lean, crown, stretch, rock (mat), mantle (mat), mantle_share (0.8), apron (mat), rubble [...], rubble_count, ledge_plants [...], ledge_vine, ledges (2 per 100 m), poi | a free-standing rock tower drawn as a volume, not a height: it narrows, bulges and overhangs like a giant hoodoo or a karst tower. `crown` is "dome", "knobs", "slope", "point", "flat" or "any" (default, one of the first four); "flat" with `top` gives towers that can carry a `base: "rim"` bridge. Ledges where a bulge sticks out under a waist grow `ledge_plants` and hang `ledge_vine` curtains. `rock` gives a tower its own stone instead of the strata (a dark or pale one among the rest); `mantle` covers `mantle_share` of its skin in another material (a tower wrapped in vines). Standing in shallow water, `apron` builds a sand fan round the foot, a beach on some sides and a bar on others; `rubble` structures (fallen blocks, slot "stone" takes the tower's rock) are scattered round the foot. Towers that overlap merge. Costs build time: each tower's columns are filled voxel by voxel |
| `skeleton` | name, path [...] (head first), kind ("toothed" or "baleen"), skull (0.2 of the length), vertebrae (50), ribs (14), rib_height (0.12 of the length), rib_span (0.04), broken (0.35), sink (0.6), skull_sink (m), belly ("right": the side the flipper falls to), flipper_at, scapula_at, sand (mat), bone, marrow, pits, lichen, lichen_share (0.3), wet, wet_depth (5), deep, teeth (mat), poi | the bones of a giant lying on its belly along `path`, drawn as volumes (`world/bones.rs`), not as a structure, so it may be any length and lie at any angle. The body is as long as the path. Vertebrae lie half sunk and worn flat on top, with `sand` drifted level into the gaps so the spine can be walked end to end (open water in a river is left alone); ribs stand up from both sides like tusks, each its own height, reach, lean and wobble, `broken` of them snapped off with the piece lying beside; a hollow skull upright at the head with eye sockets, a blowhole and a way in at the back; open jaws with teeth (or baleen plates on an upturned snout); a fallen flipper and a dish-shaped shoulder blade. Broken ends show `marrow` with `pits`; bone open to the sky grows `lichen`; bone near the waterline is `wet`, deeper bone `deep`. Names `<name>_skull`, `_blowhole`, `_jaw`, `_ribs`, `_flipper`, `_scapula`, `_hips`, `_tail`, `_spine_1`..`_9`. Run it after the land is shaped: the spine follows the ground under it. Ribshore |
| `erode_rims` | depth (3), reach (10), gullies (0.4), min_drop (4) | weathers the top of every cliff in the world as it stands: rounds the lip into a shoulder up to `depth` m deep easing back `reach` m, cuts gullies back into the tops, varies along the edge. Run after the landforms and before anything built (Redrock, Jadewater's Well block). Don't run it after `tower`s: it would lower their heights but not their rock |
| `path` | from, to, width, mat | a track (surface material, no plants) |
| `moisture` | at, radius, amount | wetter or drier ground |
| `heightmap` | file | the ground from a height grid in the world folder (f32 little-endian, one value per 1 m cell, rows north to south, world metres), instead of `landmass` |
| `area` | name, kind, at + radius (wobble 0.12) or poly [...], inside | names a stretch of ground: the HUD names it in the corner once you are `inside` m past its edge (a tenth of the way across by default), and holds the name that far back out again. Areas may nest; the smallest one you are properly inside wins, so a green inside a village inside a valley names the green. Name the parts worth referring to, not the whole map |
| `sign` | at, text [lines], style, bearing or face (pos), slots, age, centre_m | a board with a line or two on it. `style` is a `library/signs/` entry: `timber_sign` and `painted_board` on posts, `carved_stone` cut into a waist-high stone, `street_blade` on a pole, `highway_board` on two legs, `iron_plate` on a post. `bearing` is the way the front looks, which is the direction a reader stands in. The board and its letters are drawn in world space, not as voxels, so small signs read; only the post is voxels. Two or three short lines beat one long one, and `--validate` says when the words come out too small. A people who don't write get `area`s and no signs |
| `lamp` | at, points [[east, south, up], ...], level, reach (14), colour ([255,236,198]), strength (1.8), after_dark | point lights, measured from `at`, `up` being metres above the ground there. Every other light in the engine comes from the sky, so a room with a roof on it is black until lamps are hung in it (`render/lamps.rs`); outdoors they do nothing worth having. `reach` is where the light falls to nothing and `strength` is how bright it is at the lamp (1 is about open daylight on a grey day, 2 a working light in a shed). One event holds a whole building's lighting. The lamp is light only: build the fitting it hangs from into the structure, and give the fitting's voxels a material with `glow` so it looks lit. At most 64 lamps light any one frame, the nearest first. `level` measures `up` from that height above the sea instead of from the ground, for lamps in a cave. `after_dark` lights them only from dusk to dawn (street lanterns, a market's lights); pair them with `night_glow` materials on the fittings |
| `city` | file, kit | a city from map data (`city.json` in the world folder; no importer ships with the game): streets, pavements and planting strips, parks/woods/commercial land as regions `city_park`/`city_wood`/`city_commercial`, buildings levelled and drawn from their footprints, bridge decks and piers, street trees, street name blades and named places. `kit` names a city kit in `library/cities/` (material roles, name swaps, ivy, more street trees); without one the city is as the importer meant it (`modern`) |

After all events lakes, then rivers, get their final shape and their water
(`settle_water` in `events/river.rs`): a river's level only falls toward the
sea or its lake and stands a hand below its lower bank, so where it crosses
land lower than its bed it gets a low lip of bank. Where it drops over a cliff
the water above runs level to the lip over a shallowing ledge and the water
below starts again at the foot; put a `lake` (with `steep`) or the sea where
the fall lands, or the river digs its own pool. Ground under buildings and
paths beside it is left alone.

Placement facts: `place` with a `level: false` structure settles it on the
lowest ground under its footprint and keeps plants off the footprint;
`base: "rim"` hangs it from the highest ground instead, with the structure's
`anchor[1]` voxel at that height (bridges). Events in the same year run in file
order. Settlements avoid ground already taken, so place a town's works (mill,
headframe, adit) in events before the `settlement`.

### Adding an event

Each event kind is one file, `src/world/chronicle/events/<event>.rs`, named
after its `event` value. The file holds a params struct (`Deserialize`, its
doc comments describe the fields) and `impl Process` with
`fn apply(&self, cx: &mut EventCtx) -> String`, which changes the world and
returns the gazetteer line. To add one:

1. Copy a similar file (`deposit.rs` is the smallest that resolves a place and
   a material) to `events/<event>.rs` and rename the struct.
2. Add `<event>: <Struct>,` to `registry!` in `events/mod.rs`.
3. Add a row to the table above.
4. If a new key names a material, add it to the material key list in
   `world_uses` in `tools/libfmt.py` so the catalogue counts it.

`EventCtx` (`chronicle/ctx.rs`) has the fields (`cx.f`), the year, cause
number and seed of the running event, `cx.pos()` and `cx.mat()` to resolve
names (they warn on unknown ones), `cx.place()` for structures, and the
lists events add to (`placed`, `items`, `pois`, `warnings`). State one event
leaves for a later one goes in `cx.state.get::<T>()`, keyed by a type the
event file defines: `river.rs` keeps `Rivers` for `avulsion`, `settlement.rs`
keeps `Settlements` for `abandon`. Shared geometry (`each_near`, `densify`,
`spline`, `grade_strip`, `stamp_path`) is in `chronicle/geom.rs`. Don't
change how an event's seed is made or the order it visits cells in: worlds
must rebuild the same. An event may still be split across threads where the
expensive half only reads -- `tower` scans for its skin over every cell in
bands and then writes the heightfield in one pass, in the old order -- but the
result has to be identical with `VOXEL_BUILD_THREADS=1` and without: compare
`--map` images byte for byte.

## Library entries (one JSON file each, file name = name)

Common fields: `name`, `version` (1), `introduced_by` (world name), `notes`.

### materials

```json
{ "id": 31, "name": "fieldstone", "smooth": false,
  "texture": { "pattern": "coursed", "base": [138,132,122], "alt": [168,160,146], "scale": 4, "contrast": 1.0,
               "calm": { "base": [178,164,144], "alt": [192,178,156] } },
  "top": { ... optional separate top-face recipe }, "bottom": { ... optional } }
```

`glow` (0 to about 1.5) gives a material its own light, drawn bright in shade,
caves and at night: Jadewater's `lantern_film` cave ceilings (1.1) and
`lumen_moss` (0.35).

Biomes may also set `water` (see world.json above) for their water's colour.

`tube: true` draws a material as thin stems: the mesher bevels the edges
along a one-voxel stem into an octagon instead of rounding it, so a slim
straight pole stays one piece where rounding would break it into floating
blobs. The leaf of a vine and the bark of `bamboo` and `canes` are
tubes unless the material says `tube: false`. It suits straight, upright
stems (Lowmouth's `silver_deadwood` snags, bottle-tree branches, the
crossroads figure's cane); on leaning or arching ones (mangrove prop roots,
palm trunks) it leaves stair-steps that rounding hides, so leave those rounded.

`crisp: true` on a ground material keeps its border with other ground sharp
instead of blended with noise (road surfaces, paving).

`floods: true` makes flat ground of this material flood from edge to edge
after rain, an inch of water over the whole of it, instead of gathering
puddles: a salt pan, a playa (Saltmirror's `salt_plates`).

`underwater: true` lets a ground material's grass sprites grow under the sea as
well as over it. Land grass stops at the waterline; a drowned meadow
(`eelgrass`) doesn't, so a biome with `underwater` in its rules and a grass
surface comes out as fields of grass on the sea floor.

`floating: true` draws a material's sprites as leaves lying flat on the sea's
surface over wherever they grow, whatever the depth (`water_lily`). Use it on
the leaf of a `herb` plant with `standing_water`: the root sits on the bed and
the pads float above it, bobbing on the swell, with now and then a bloom.
Only the sea, which includes a flooded swamp or lagoon; a river or a lake
carries its own level, which a sprite never sees.

`id` is legacy: leave it out. Voxels are a byte, so the game gives ids per
world at load (`Library::assign_ids`): materials the world reaches keep the id
they ask for (17..254, unique, 255 is `carve`) or take a free one, and a
material the world cannot reach gets no id and no texture layer, so the
library costs a world only what it draws. Saves name materials, not ids.

`was` (a list of names) is what a material was called before others were
merged into it: `cattail_head` has `"was": ["reedmace_head"]`. A save, a
local world or a city file that still names an old one gets this material,
and a world that reaches an old name reaches it. When you merge two
near-identical materials, keep the one the engine names or more files use,
delete the other's file, rewrite the references and list the old name here,
and keep the survivor's `sprites` the one both drew with.

`sprites` says what grows on a material as sprites, whatever it is called:
`"grass"`, `"needles"`, `"fern"`, `"cattail head"`, `"vine"`, `"flower"`,
`"leaves"` (plain leaves shared by many species, one shape per grove) or
leaves of a shape, `{"rule": "leaves", "shape": "broad"}` (pointed, broad,
lobed, narrow, heart, maple). The rules are `SPRITE_RULES` in
`mesh/sprites.rs`; `"none"` grows nothing. Leave it out and nothing grows
(ground with a `grass` block still grows short dry tufts), and `--validate`
asks for one wherever the name reads like a rule (`_leaves`, `_grass`,
`_needles`), since that is usually forgotten rather than meant. Names used to
decide this; they no longer do.

`like` makes a material a variant of another, as for creatures and plants:
it starts from the parent and overrides what it gives. Its `texture`, `top`
and `bottom` merge key by key, and so does each one's `calm`, so a recolour
gives only `base`, `alt` and the calm pair, and a same-texture copy with a
flag of its own gives only the flag (`"like": "dish_paint", "facet": true`).
`shade` (a brightness multiple) and `hue` (warmer above 0, cooler below)
repaint every colour the child takes from its parent and leave the ones it
gives itself: `"like": "quartzite", "shade": 0.78` is a darker bed of the
same rock. The legacy `id` and `was` are never inherited; `sprites` is, so a
variant that should grow nothing says `"none"`. Use it for families (the
`ice_panel_*` colours, `door_*`, `siding_*`, the karst beds) rather than for
two materials that merely look alike.

`grass` ({"sparse": 0.1, "dense": 0.85, "short": 1.0}): how grass grows on a
ground material that grows grass (`sprites` a grass rule). `sparse` and `dense` are the share of
columns carrying a tuft where the meadow noise is lowest and highest (0.04 and
0.3 is a thin steppe, 0.5 and 0.95 a crop); `short` is the height of the
shortest grass, 1.0 being the default, so 3.0 makes short grass three
times as tall. Long grass adds the same 1.7 on top whatever `short` is. `smooth: true` for ground (the mesher slopes
it). Plant bark and leaf materials are rounded automatically. Built-in names
usable anywhere: grass, dirt, sand, stone, stone_dark, stone_light, bark, leaves,
pine, planks, gravel, birch_bark, leaves_autumn, snow, birch_leaves, brick, air.

Patterns: flat, noise, speckle, strata, columns, cracks, blotch, planks,
coursed, glass, ripples, grain, rings, thatch, gravel. `scale` is pattern cells per
metre tile (for gravel, the larger stones across a metre). The `calm` block may also give
`pattern` (a different verb for the calm style only) and `contrast` (scales the
calm contrast, crack lines included); see `library/materials/scree.json`.

### plants

```json
{ "name": "spiral_pine", "form": "spiral", "height": [9,14], "trunk_radius": [0.2,0.3],
  "crown_radius": [2.2,3.0], "bark": "pale_bark", "leaf": "pale_needles",
  "autumn": "leaves_autumn", "squash": 1.0, "max_slope": 0.6 }
```

`autumn`: the leaf material this plant wears in autumn. It is also what makes
a plant deciduous: one with an `autumn` colour stands bare in winter, keeping
its trunk and limbs, and one without is evergreen and keeps its leaf all year.
So give it to every broadleaf that should lose its leaves, and to nothing that
should not. Don't use `leaf_alt` for autumn colour: it gives a random share of
trees the second colour for ever, and green and orange trees mixed tree by
tree read as messy from a distance. `leaf_alt` is for the rare plant whose
individuals really differ (cherries not yet in blossom).

`flower_seasons` (default all): the seasons a plant's `flower` shows in, e.g.
`["spring"]` for ocotillo. Out of season the plant has the same shape, no
flowers. A world that names no `season` counts as summer.

`flower` on a branching tree lays flat heads of that material on the sunlit top
of the crown, over the green leaves (elder, rowan, hawthorn). Give the flower
its own material; don't make it the whole `leaf`, which paints the tree one
colour.

`curve`, `canopy`, `fill`, `leader`, `dome`, `spread`, `stems` (form branching):
the knobs that tell one broadleaf from another. `curve` is the shape of a limb
— "arch" (bows out and levels off, the default), "sweep" (out then up, a
maple's S), "straight", "droop" (over and down) or "kinked". `canopy` is how the
foliage is massed — "lobes" (one lobe of leaves over each big limb with the
notches between them part bridged, the default), "vase" (the lobes stood taller
and pulled in over the trunk, a beech), "layers" (flat tiers, an elder or a
cypress), "cloud" (every notch bridged under one dome, a live oak) or "tufts"
(leaves on the twigs only, an open crown). `fill` (0-1, default 0.55) is how
often a notch between two lobes is bridged, `leader` (0-1, default 0.35) how
much of the trunk carries on up through the crown instead of being spent in the
lowest fork (0 an oak's open vase, 0.9 a cypress's single spar), `dome` (0-1)
how many small domes ride over the top, `spread` (0-1) how far out the first
limbs reach, and `stems` [lo, hi] several stems from one root instead of a
trunk (an elder, a hazel). A plant's `crown_radius` is the radius of one lobe,
not of the whole crown: the crown comes out roughly one and a half to two times
it, so a tree as wide as it is tall wants a `crown_radius` of about a third of
its `height`.

`standing_water` ([lo, hi] metres, default none): a land plant that stands in
the sea's shallows (bald cypress, mangroves, sawgrass): the depth of water the
ground may lie under, negative for above the sea. Without it a land plant
needs ground 0.6 m over the sea. It grows at full height from the bed (unlike
`water_depth`, which is for kelp).

`buttress` (metres, default 0): root flare for big old trees, extra trunk
radius at the ground in lobes like buttress roots, fading over about four times
that height. Big trees look wrong on thin straight trunks: give them a thick
`trunk_radius` (0.5-0.85) and a buttress of 0.4-0.7 (great_oak, giant_fir,
bigleaf_maple).

`crown_base` (conifer, columnar and sapling forms, default 0.2): where the
needles start, as a fraction of the height.

Forms: mangrove (a trunk on a ring of `stems` arching prop roots that bow out
and down into the mud, a low wide crown, drop roots from some limbs; give it
`standing_water`), broad, birch, conifer, columnar, spiral, shrub, snag (no leaves),
umbrella (use squash 0.5), weeping, sapling, branching (a trunk shedding limbs
up its length that fork again, each clothed in leaves, under a crown of lobes:
every broadleaf tree. `curve` and `canopy` above shape it; `limb` is what the
trunk has left where the first limbs leave it, as a share of its radius at the
ground, default 0.88 — below that the limbs are sized by area conservation),
canes (ocotillo: `stems` [lo, hi] whip-like canes from one root crown,
`cane_lean` the most a cane leans at its root in radians, about 0.54 unless set,
`trunk_radius` their thickness, `flower` a material for the tips; with
`canopy: "tufts"` a shrub of thin stems drawn as smooth tubes instead: sea
buckthorn, hazel, bramble, broom, rabbitbrush. Then `cane_bend` how far a cane
arches over toward its tip (default ~0.07, 0 straight), `cane_spurs` the
chance of a side spur per segment (0.4), `cane_leaves` the chance of a leaf
tuft per upper joint (1; low for near-leafless switches), `flower` berries
along the stems or, with `flower_tips: true`, clusters at the tips. Every
leaning stem is a thread, about 15-20 quads a metre: a sugar-cane field at
0.1 x 6-10 stems is about 1.2M quads in view, so keep stems x density modest.
Give the stems their own bark material, set `passable` on knee-high ones),
`bole` (branching, 0.15-0.85): the trunk's clear length to the first limbs as a
share of the height, unset 0.46 less 0.12 x `spread`; about 0.6 puts a flat
crown on top of a tall bare column (baobab).
`thin_limbs` (any woody form, metres): limbs thinner than this are sticks,
one thin line of voxels meshed as a smooth tube, whatever the bark. Twiggy and
gnarled trees, mangrove prop roots and deciduous crowns use 0.12-0.2; a
deciduous tree in leaf skips the tubes (its twigs are hidden), so the cost
falls in winter. `cane_umbel` (canes with tufts): a mop of thin rays this long
at each tip (papyrus). Canes may take `water_depth` (sea whips). An umbrella
with `stems` splits its trunk into that many forked limbs under the crown
(acacia); a kelp with a `flower` is a bull kelp (bare stipe, that float),
cactus
(saguaro: a ribbed column with arms that turn up, more with height; optional
`flower`), kelp (a water plant: give `water_depth` [lo, hi] metres of sea it
grows under; `height` is then a share of the high-water depth, ~1 so the strand
reaches the surface; `stems` stipes per holdfast; drawn as loose strands that
ride the tide, not voxels, so `trunk_radius` is the stipe's width and
`crown_radius` the blade size), bamboo (a clump of `stems` culms
spread over `crown_radius`, each one straight vertical stalk of even
thickness with a short nod at the tip, leaf sprays above `crown_base` (0.55);
`trunk_radius` is one culm's, 0.17 or more or the rounding makes it wobble; give the
culm material `passable` for a thicket you push through). Herbs whose leaf material's
`sprites` is a sprite plant (fern, fireweed, heather, glassbloom, rabbitbrush,
cattail leaves, sawgrass, palmetto, cane) are drawn
entirely as sprites, and a `stalk` of `cattail_head` stands a cattail's brown
head over them. Sprite plants have no collision, so the kayak slides through
them, and with `standing_water` they grow up out of the water (grass doesn't
grow under the sea). A plant whose leaf is `palm_frond` (`sprites` "palm
frond") gets fan sprites round its crown.

`crust` (form "crust"): a plant that grows on something else rather than in
the ground. `hosts` is "tree" (trunks) and/or "rock" (steep faces of bare
ground); `crust` is "bracket" (shelf fungus: half-shelves standing out from a
trunk, stacked up one face) or "patch" (a one-voxel skin of moss or lichen
hugging the surface); `shaded` (default true for patches, false for brackets)
keeps it to the side that faces away from the midday sun, which is north.
`crown_radius` is one patch's radius (kept under about the trunk's own, or a
patch sleeves right round it) and `height` the band of trunk, metres above the
ground, a colony keeps to. In a biome the entry's `density` is the chance a
host carries a colony: each tree, and each 4 m cell of steep ground. Like
vines, crusts are placed after their hosts. Give the material `passable` so
you walk through the moss instead of into it (`shelf_fungus`, `tree_moss`,
`rock_moss`, `rock_lichen`).

Conifers and columnar trees can be very tall
(giant_fir is 22-28 m, redwood 34-46 m: mind the world's `height_m`).

### structures (shape programs)

The complete format (every key and op, types, defaults, the allowed `base`
and `axis` values) is the JSON Schema `library/schema/structure.schema.json`.
It is generated from the loader's own types in `src/library/shape.rs`, and a
test fails when it falls behind them, so trust it over this summary.
`library/schema/structure.md` is a checklist for reviewing a structure file.

```json
{ "name": "settler_house", "size": [24,27,28], "anchor": [12,0,0], "foundation": 6,
  "slots": { "wall": "fieldstone", "roof": "thatch" }, "tags": ["settler","house"],
  "decays": true, "level": true, "base": null,
  "ops": [ { "op": "walls", "from": [0,1,0], "to": [24,11,28], "mat": "wall", "thick": 2, "part": "wall" }, ... ] }
```

Voxel units, y = 0 is ground level, the front faces local -Z (placement turns the
front toward `bearing`/`face`). `anchor` is the voxel on the placement point.
`foundation` fills down under solid footings. `level: false` and
`decays: false` for natural features (boulders, logs). `base: "sea"` puts y = 0
one metre under sea level (piers, landings); `base: "bed"` sinks it half a
metre into the lowest sea bed under it (wrecks); `base: "rim"` see above. `mat`
names a slot or a material; `"air"` empties the structure's own voxels but
leaves the terrain, `"carve"` removes the terrain too (alcoves, adits, arches
worn through a fin: `great_window`, `mine_portal`, `cliff_granaries`). Later
ops override earlier ones, so carve after anything that must not fill the hole.

Ops: `box` (from, to exclusive), `walls` (thick), `gable` (from, to with the
same y as eaves, axis x/z = ridge direction, rise 1 = 45 degrees, 2 = steeper),
`ellipsoid` (centre, radius[3]), `cylinder` (base, radius, height), `log`
(base, radius, length, axis, yaw), `disc` (centre, radius, thick, axis, yaw;
`yaw` turns either that many degrees from +x toward +z), `set` (at),
`segment` (points [[x,y,z], ...], radius: a rod, rope or spar at any angle,
drawn as a smooth tube where it leans; use it for rigging, stays, spokes,
braces and leaning masts instead of stepped boxes or cylinder stacks),
`dish` (vertex, radius, depth, thick (1.5), tilt, yaw: a paraboloid shell,
the bowl's bottom at `vertex`, its rim `radius` across and `depth` up the
axis, the axis leaning `tilt` degrees from straight up toward `yaw`; radio
dishes and reflectors at any angle: Saltmirror's array),
`repeat` (count, step, ops), `recolour_below` (y, from, to; creatures),
`recolour` (at, until, from, to: swap one colour for another inside a box;
stripes, collars).

Parts (for decay): roof, cloth, glass (fail in whole sections), timber, frame,
wall, stone, metal (crumble from the top of each column), floor (stays).
After decay, `gen::standing_map` applies gravity per placed structure: a voxel
stands if it joins (face or edge) its lowest row or the terrain. Loose pieces
first get the intact voxels straight under them back, then the cheapest path
of intact voxels to what stands if that costs at most half the piece; the rest
fall. Parts that touch nothing in the intact shape (a gable a voxel above its
beam) count as standing. `VOXEL_GRAVITY=0` turns the pass off for comparing.
Stone and wall parts grow moss (`mossy_stone`) with age.

### kits (cross-sections for routed lines)

One file per kit in `library/kits/`; `docs/ROUTES.md` is the full account.
A kit is what a `ribbon`, `route`, `network` or `village` builds along its
line, and it is the right answer whenever something runs: a road, a lane, a
fence, a wall, a bridge, a boardwalk, a walkway, a bored passage.

```json
{"name": "highway_guard", "version": 1, "introduced_by": "<world>",
 "notes": "Two lanes and a shoulder with a steel guard down both edges.",
 "ground": "grade", "width": 9.0, "mat": "road_asphalt",
 "shoulder": "road_gravel", "margin": 7.0,
 "rails": [{"side": "both", "out": 0.4, "from": 0.5, "to": 0.8, "thick": 0.25, "mat": "bridge_steel"}],
 "posts": [{"side": "both", "out": 0.4, "every": 4.0, "size": [0.25, 0.5, 0.25], "mat": "rust_iron"}],
 "span": "steel_span", "tags": ["road"]}
```

- `ground`: `grade` cuts and fills the land to the line; `deck` builds a
  free-standing deck `lift` m over whatever is below; `follow` leaves the land
  alone and sits the members on it (fences, walls); `bore` hollows a passage
  out of rock at the line's own height; `none` draws nothing on the ground.
- `width`/`mat`/`shoulder`/`margin`: the surface and how far beside it a
  graded cut is feathered back. `width: 0` makes a line with no surface.
- `walk {width, gap, mat, kerb, side}`: a footway each side, past a planting
  gap, with a raised kerb.
- `rails [{side, out, from, to, thick, mat}]`: members running the whole
  length. `side` is `left`, `right`, `both` or `centre` (a centred one
  straddles the line: that is how a wall is written). `out` is metres outward
  from the surface edge, `from`/`to` heights above the surface.
- `posts [{side, out, every, size [along, up, across], phase, base, mat}]`:
  members repeated by arc length along the whole route, so spacing holds
  across a corner.
- `deck {thick, mat, under}` and `piers {every, radius, mat, min_drop}`: the
  slab a line rides on where it leaves the ground, and what holds it up.
- `sag`: metres the deck drops at mid-span (rope bridges).
- `span`: the kit to hand over to where the line flies; `max_span` the
  longest crossing the router will attempt.
- `clear`: metres of air kept above the surface (a tunnel's headroom).

### creatures

```json
{ "name": "heron", "voxel": 0.0417, "half_w": 0.2, "height": 1.1,
  "colors": { "heron": [128,140,152], "beak": [240,190,40], "glow": [170,120,255,1.8] },
  "ops": [ ... shape ops on a 32x32x32 grid, mat = colour name ... ],
  "behaviour": { "movement": "wade", "walk_speed": 0.5, "flee_speed": 4, "wary": 14, "flightiness": 1.3,
                 "flees_from": ["player"], "hunts": [], "fly_height": 0 } }
```

The model grid is 32 voxels a side, the model faces -Z, feet at y = 0. A fourth
colour value is emissive strength. Movement: graze, hop, splash (hops and keeps
to the water's edge: frogs and toads), skulk, wade (keeps near
water), circle (flies circles: gulls, crows), flutter (small low circles: moths),
flock (a flying flock that wheels, lands together and flushes: starlings), soar
(a lone bird circling wide and high: an eagle), glide (a few big seabirds
sweeping wide and low, roosting together on the highest dry ground near:
albatrosses), still, plod (slow; hides in
its shell instead of running: tortoises), bask (plod on a beach, and now and
then crawls back to the sea to swim as its `school` child: sea turtles), undulate (slow wide circles a few
metres up, wings rippling: flying rays, rig plan `ray`), school, breach,
haulout (seals: lie by the sea, dive when startled), paddle (waterfowl on the
sea near shore or on rivers, ashore to graze, off in a V when startled), cling (woodpeckers on trunks), creep (an octopus on the sea floor, its arms
draped over what is under it; rig plan `octopus`), buzz (bees
at plants that afford `nectar`), dart (dragonflies over water). A
`wade` creature with a winged rig and `fly_height` takes off and flies a short
way when startled (herons, cranes). `flees_from` takes
creature names or "player"; `hunts` takes creature names. For flock and soar,
`walk_speed` is the cruising speed and `flee_speed` the top speed.

`wary` is the flee distance in metres; `flightiness` is how readily it actually
runs once you are inside it -- 0 a manta ray that ignores you, 0.2 a crocodile,
1 an ordinary wild animal, 1.8 a hare -- and `juvenile_flightiness` is a
juvenile's relative to an adult's, below 1 where the young stand and stare
(fawn 0.55, bear cub 0.5) and above 1 where they are jumpier (piglet 1.25).
Travel mode, approach speed, which way the animal faces and cover at close
range all move the likelihood too; see docs/CREATURES.md, "Running from the
player".

`blink` (flashes per second) in a behaviour makes its glowing colours flash on and off, each one on its own rhythm (firefly 0.45, marsh_light a slow 0.12 pulse); `spawn_chance` below 1 makes a species rare (great_horned_owl 0.08).

`morphs`: the named coats a creature or plant comes in, each a `name`, a
relative `weight`, optional `shades`, and `colors` mapping the entry's own
colour names (a plant's slots `bark`, `leaf`, `flower`, `autumn`, `stalk`) to
one colour or the two ends of a range:

```json
"morphs": [
  {"name": "violet", "weight": 6, "shades": 3, "colors": {"leaf": [[132, 104, 196], [162, 134, 222]]}},
  {"name": "white", "weight": 1, "colors": {"leaf": [[230, 228, 238], [246, 246, 250]]}}
]
```

A morph carries no shape, which is what separates it from a `like` child: a
variant that needs a horn or a plume is a species with `rarity`, not a morph.
Coats are not inherited through `like`. A biome picks them with `morph` on the
creature or plant it lists, either one name or a map of weights. A plant may
name another material in a slot instead of a colour, which is what `leaf_alt`
says the older way, and then nothing is generated.

A **plant's** coat is a material of its own, generated at load from the
plant's own one (`<plant>_<slot>@<coat>`, plus `.0`, `.1` for shades), so it
can be any colour at all, and a glow. It costs a voxel id and a texture layer
per shade, and a world has 238 ids, so keep `shades` small: wisteria's 3+2+2
is seven materials where it had one. One coat per patch of ground, not per
plant: `morph_blob_m` (60 m by default) sets how wide a patch is, so a cluster
of wisteria is one colour and the next along another, and a downland's
tussocks lie in drifts. Worked examples: `wisteria` (violet, lilac, a rare
white, in jadewater) and `feather_tussock` (tawny, silver, sage in 35 m
drifts, on the Windbarrow downland).

A **creature's** coat is drawn, and it is one affine map over every colour in
the model at once, fitted from the colours the coat names. Write coats against
that: name two or more colours for a real repaint (one colour gives a plain
multiply, so the silver fox names `fox` dark *and* `white` unchanged to keep
its tail tip); a coat may take every colour lighter, darker or warmer together
but cannot send two of them different ways (no blue body to red while a green
thorax goes brown -- that is a species); it recolours a glow with the rest, and
a fourth number on a glowing colour sets how bright it burns (one gain over
every glowing colour: the glassmoth's `lantern` coat, `[255, 210, 120, 2.4]`
over wings of 1.8), but it cannot light a colour that does not glow, so the
faerie wood's recolours that glow where their parent does not stay species;
and it carries no size and no temperament. One draw per group, so a herd is of a piece. `--validate`
reports a coat that cannot reach a colour it declared or that drives an
undeclared colour past white; `VG_COATS=1` prints what each group got. The
worked creatures are fox (red/silver), trout (rainbow/golden) and humpback
(grey/white). See docs/CREATURES.md, "Coats".

`drift` on a material: the two colours it drifts between across the land,
tens of metres at a time, in place of the one ramp every material in its
`tint` class shares. `"drift": [[r,g,b],[r,g,b]]` for both styles, or
`{"calm": [...], "classic": [...]}` with a style left out keeping its class
ramp. Declared as colours and shipped as the multiply that takes the
material's own texture to each end, so write the colours you want to see.
Declaring one also turns the drift on for a material whose `tint` class gives
it none. `downland_grass` is the worked example (cool khaki to dry gold on the
Windbarrow chalk). `--validate` catches two ends that look the same and an end
the texture cannot reach.

`rarity` (0 to 1) with `like` makes an entry a rare morph of that parent rather
than a species of its own: it stands in for one of the parent's spawns at that
chance, keeps the parent's place in the biome's quota, and is never listed in a
biome itself (unicorn 0.01 of deer, old_pale 0.03 of alligator, sky_manta 0.05
of wind_skate). Any way of
moving may draw one: walkers roll per individual as a group is placed, so a
herd can hold one, and everything else rolls as it is spawned
(`spawn::roll_morphs`). A morph may differ from its parent in any way an entry
can — the sky manta flies higher than the skates it stands among.

`in_water`: a second behaviour for a species that crosses between land and
water (crocodile, harbour seal, sea turtle), giving only what changes once it
is swimming; it must move with `school`. The block is laid over the creature's
own `behaviour`, so its temperament carries over, and the engine builds a
second form of the species from it. The two forms share a model, colours and
one place in a biome's quota: a biome lists the animal once, and where an
individual lands decides which form it is. See docs/CREATURES.md, "Land and
water".

`awash` (school): swims at the surface with its back and eyes out instead of
under it (crocodiles). `shallows` (school, metres): the least water it keeps
to and is found in; default 2 at sea and 1.2 in rivers, a marsh swimmer 0.6.

Boids: add `"group": { "size": [min, max], "spacing": 1.5, "range": 8,
"separation": 1, "alignment": 1, "cohesion": 1 }` to the behaviour. With
movement flock it sets the flock size and how tight it flies (starlings:
size [25, 70], spacing 0.9); with any walking movement it spawns family groups
that keep together and bolt together (deer: size [1, 4], spacing 2.5). Most
species should not group: a biome's `count` is individuals, so a flock of 40
wants a count near 40.
Colours are counted per model, not per world: each model gets its own palette block of up to 192 colours (`models::PALETTE_SLOTS`), so there is no need to reuse RGB values from other entries.

### collectibles

```json
{ "name": "settler_coin", "display": "settler's coin", "voxel": 0.0266,
  "colors": { "brass": [200,160,70,0.5], "dark": [36,28,24] },
  "ops": [ ... shape ops on a 32^3 grid ... ], "spin": false }
```

Small finds the player picks up by touch; the HUD counts them by display name.
Biomes scatter them with `collectibles` (below); chronicle events can also make
them (an impact's `item` and `<item>_mother` are collectibles, buried). Keep them
about 0.2-0.5 m across and give metal a little emissive so it glints. They share
the creatures' colour slots.

### biomes

```json
{ "name": "prairie", "priority": 8, "rules": { "burn_age": [0,150] },
  "surface": "prairie_grass", "subsurface": "dirt", "rock": null,
  "plants": [ { "plant": "aspen", "density": 0.03, "placement": { "verb": "edge", "params": { "width": 22, "inner": 0.03 } } } ],
  "features": [ { "structure": "boulder", "density": 0.0003 } ],
  "creatures": [ { "creature": "rabbit", "count": 8 } ],
  "collectibles": [ { "item": "pine_cone", "density": 0.0005, "placement": { "verb": "scatter" } } ],
  "map_color": [200,172,90] }
```

Each cell takes the highest-priority biome whose rules all match. Rules (each a
[min, max] range): elev (above sea), slope, moist (0..1), coast (distance to the
coast, negative at sea), river (distance to flowing water), burn_age (years since
fire; never-burned cells fail it), strange (0..1); plus `region` (name) and
`underwater` (bool). A biome without `underwater` covers land only, unless
`amphibious: true`: then the sea's shallows in its region match too (a swamp
standing in water). Features whose structure has `base: "sea"` (cypress knees,
a floating mat) are placed in 0.2 to 2.5 m of water instead of on land. Boundaries are warped by noise automatically.

`like` makes a biome a variant of another: it keeps the parent's rules,
priority and everything else it does not give. Lists (`plants`, `features`,
`creatures`, `collectibles`) are replaced whole, so a child that changes one
gives all of it. The faerie city's biomes are the city's with other ground,
plants and creatures (`"like": "city_park"`), and Kaldvik's fjord is the
seabed with its own whales. A parent still competes only where a world lists
it.

Edges also feather: a biome's ground reaches `fringe` metres (default 3) across
its boundary, so two shades of grass break into interlocking patches instead of
meeting on a line, and the grass growing on the ground fringes with it. Set
`"fringe": 0` where the edge is a real one and should stay sharp -- a ploughed
field, a burn scar, a paved square, a salt pan; a biome with 0 is never mixed
into its neighbours either. Shores and river banks never feather.

Placement verbs: scatter; clustered (size, min, max: patchy groves); edge
(width, inner: near other biomes; `field` "a,b" only near those biomes, as a
beach's driftwood keeps to where the dune grass starts); riparian (width: near rivers); gradient
(`field`: strange/moist/elev/slope/coast, min, max); ring (x, z, radius, width);
lone (spacing). Any verb also takes `spacing`. Features (structures) also take
`seat` (metres): the structure's long axis (local X) is turned along the
contour, and spots where the ground rises more than `seat` across the
footprint are skipped, so arches stand level instead of half-buried in a slope
(Redrock's arches use 2.5).

A natural feature made only of `log` and `disc` ops (driftwood, fallen trunks,
bone shards) lies at any bearing in 15-degree steps, not just the four quarter
turns. With `along` (degrees) it lies along the shore, across the line to the
sea, give or take that many degrees, as the tide leaves a log.

### signs

```json
{ "name": "timber_sign", "version": 1, "introduced_by": "glassfall",
  "notes": "A sawn board on two posts at the edge of a settlement.",
  "post": "sign_posts_timber", "board": [2.4, 0.8], "centre_m": 1.55,
  "offset_m": 0.02, "thickness": 0.08,
  "face": [134, 104, 68], "edge": [76, 56, 34], "ink": [40, 28, 16],
  "cap_m": 0.17, "border_m": 0.0, "letters": "caps", "two_sided": true }
```

How a `sign` event's board looks. `post` is the structure placed under it (a
pair of posts, a pole, a stone); leave it out for a board hung on something
else. `board` is the board's width and height in metres and `centre_m` how
high its centre stands above the ground; `offset_m` is how far it stands clear
of the post's face. `face`, `edge` and `ink` are sRGB bytes for the board, its
edges and painted border, and the lettering. `cap_m` is the capital height the
words are set at, smaller where the lines need the room; `border_m` paints a
keyline that far in from the edge; `letters` is "caps" (Cinzel) or "italic"
(Cormorant Garamond Italic); `two_sided` letters the back as well. `bare`
drops the board altogether and cuts the letters into whatever holds them, for
a name stone or a wall.

The board and its lettering are plates in world space (`src/render/plates.rs`),
not voxels: they are lit, fogged and hidden by what is in front of them, and
they cast no shadow. Boards stop drawing past 140 m and lettering past 64 m.
