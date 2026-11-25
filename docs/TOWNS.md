# Towns

Towns and cities grown from a few inputs: where, how big, which roads come
in from the country, and what kind of place each part is. The generator is
`src/world/town/` and knows nothing of voxels or the library; the `town`
chronicle event (`world/chronicle/events/town.rs`) turns its plan into a
world. Buildings are drawn from their footprints by the same code as an
imported city (`world/city.rs`), so a town costs only its polygons.

## How a town grows

The steps run in the order a town grows in.

1. **Main roads.** Each gate (a point out in the country) is joined to the
   centre by A* over the land (`grow::main_roads`): it climbs as little as it
   can, crosses water where it is narrowest, and joins a road already found
   rather than running beside it, so the roads gather before they reach the
   middle. Inside the town's ragged edge they are high streets, outside it
   country roads. With no gates given, three or four come in from wherever
   the land allows. A centre in water or on a slope moves to the nearest dry,
   level ground.
2. **Where the town reaches** (`extent.rs`). Not a circle: every patch of
   ground is scored, distance from the centre counting against it and
   nearness to water and to the main roads counting for it, and the town is
   the best-scoring ground joined to the centre, as much of it as a circle of
   the radius would cover. A town by the sea stretches along the shore, and
   every town strings out along its roads. A town with wide water (a sea, a
   lake, an estuary) within reach has its market moved to the waterfront
   first. Everything after asks how far inside or outside this shape a point
   is, so the core, the districts, the thinning plots and the farmland all
   follow it. `waterfront` and `ribbon` (both default 1, 0 for none) set how
   hard water and roads pull.
3. **Districts.** The sectors between the main roads, where they cross the
   middle of the town, are the districts; the core is where the score is
   lowest, out along the water and the roads like the town itself. A big
   town (over 200 m) or a walled one has a ring road round its core; a small
   town's edge is its back lanes. Each district has a style (below).
4. **A bridge street** from the market over the shortest crossing nearby,
   when no main road already crosses near the middle.
5. **Quays**: a street at the water's edge where the town faces wide water
   (or anywhere water runs through a town over 250 m), with a handful of
   jetties near the market.
6. **Each district's streets**, in its style. Streets that grow stop 15 to
   45 m short of the town's edge, each its own distance (most near 15), so
   their plots fit inside it and their ends don't line up along one contour.
7. **Clean-up.** Dead ends shorter than 26 m go; a dead end with a street
   within 60 m ahead is carried on to meet it; dead ends out at the edge are
   joined in pairs by a lane along the edge, and any left out there go. A
   high street becomes a country road at its last junction, not partway
   along an open road.
8. **Squares, plots and buildings** (`lots.rs`), then **fields**
   (`country.rs`).

The street graph (`graph.rs`) stays planar throughout: a crossing is always a
junction. A line either cuts through what it crosses (`insert_line`, for a
surveyed grid) or stops at the first street it meets (`grow_one`, for a lane
that grew). A line that would run alongside an existing street within a few
metres is dropped there (`uncrowded`), so two streets never lie on top of
each other and junctions never have fences or kerbs across them.

## District styles

| Style | Streets | Building |
|---|---|---|
| `grid` | lines every 80-120 m one way and 45-58 m the other, square to the nearest main road, cut to the district, each end out at the town's edge cut back a little further by its own amount; a back alley down most long blocks | closed blocks of terraces |
| `radial` | rings round the centre crossed by spokes; in an outer sector, arcs of the same circles | closed blocks, taller |
| `maze` | lanes laid one at a time through the emptiest point left, each leaving a street at a right angle and running on until it meets another, bending to climb as little as it can; passages through the bigger blocks; in a small town, back lanes 42 m behind the main streets first, for long narrow plots | closed blocks of narrow plots |
| `suburb` | curving collectors, closes 70-90 m apart with turning circles, loops | detached houses in hedged gardens, set back, most with a garage and a drive and a tree on the lawn |
| `village` | a few lanes off the through road | rows of cottages along them |
| `ribbon` | none of its own | cottages strung along the roads already there, as a town grows out of its gates |

Ground within 40 m of water is built up as closed blocks whatever its
district, so the waterfront is the densest part of a port.

A town that grew thins toward its edge: near the edge, away from the roads
in and the water, more and more plots are left empty, so building clusters
on the roads out and fades into the fields (`gradient`, default true). A
suburb is built out to its edge. In a town under 170 m radius the houses of
the closed blocks stand a little apart (`gaps`, default 1.6 m there, else 0)
and are mostly two storeys.

## Plots

Every metre of the town is given to the street whose kerb is nearest it, on
one side or the other, and remembers how far along that street it lies.

In the dense districts the band along every frontage, a plot's depth deep, is
cut into strips square to the street, a plot's width apart; a strip is the
cells whose nearest street is that one and whose foot falls in that stretch.
Plots therefore meet their neighbours exactly and fill a block of any shape
all round, leaving a yard in the middle; a deep yard may have a workshop or
coach house at its back. The two part-strips that meet at a
corner join into one corner building, which fills its corner whatever its
shape (`"irregular": false` cuts it back to a rectangle square to its
street). Each footprint is the hull of its strip. A row keeps near one height,
its ridges run along the street, and its roofs are pitched low with short
eaves so neighbours' roofs line up. The edges of a square count as frontage,
so a square is lined with buildings facing it.

Elsewhere (suburbs, villages, ribbons) a house stands detached in a plot cut
along the frontage, set back, with a gap each side. In a suburb the garden
runs back to the plot behind, hedged along the side and the back (one hedge
for two back-to-back gardens), and a garage stands beside most houses with a
drive to the street.

## Notable places

Each is the caller's choice:

- `market` (default true): a square at the centre, long along the main road.
- `squares`: more, at the busiest junctions of the dense districts.
- `churches` (default 1): the first on the market, the rest on the squares.
- `parks` (default none under 180 m radius, one to 280 m, two beyond).
- `wall`: a crenellated wall a plot's depth outside the ring, following the
  hill's contour, with towers along it and a pair at every gate. Houses back
  onto it; the ring inside it is a lane.
- `alleys` (default true) and `yards` (default true, fences between back
  yards).
- `landmarks`: library structures, each `{"structure": ..., "near": ...}`
  with `near` one of `market`, `square`, `edge`, `water`, `high` (the
  highest ground, for a church or a castle on its hill), `anywhere`. A
  landmark on the market may stand in the square. One near the edge or
  anywhere stands on a street and faces it, on a street that runs near a
  compass line if there is one (structures turn only by quarter turns).
  `"poi"` names it on the map.
- `houses`: library structures for the detached houses of the village,
  ribbon and suburb plots, picked in turn: a world's own cottages instead of
  footprint houses. Each stands on its plot facing the street, where the
  street runs within 20 degrees of a compass line; plots on other streets
  get footprint houses, so give the kit materials that match.
- `green`: the market is a village green, left as grass on the lie of the
  land with the lanes running across it, not a levelled, paved square.
- `infill`: up to this many more of `houses` on the ground the street plots
  leave (back plots, corners), nearest the streets first, each turned to
  face its street, kept 2.5 m in from the town's edge, and only where a
  footpath can reach it. For a small top where few streets run square to the
  compass (Kitehold).
- `paths`: a footpath 1.4 m wide (the kit's `path` street, else its `lane`)
  from the door of every library structure to the nearest street, square or
  path, routed round the buildings and sharing the paths already laid.
  Later infill houses keep these paths clear.
- `footprints` (default true): false leaves the plots a library house can't
  take empty, so the town is all library structures; with `infill`, those
  plots are free ground for it. The town's structures
  are registered under its `name` like a `settlement`'s, so an `abandon`
  event can age them (Glassfall's village).
- `walkways`: a town on stilts over shallow water. Every street is laid with
  this route kit (a boardwalk such as `bayou_walk`) instead of being paved;
  water up to `stilt_depth` m deep (default 2) counts as ground for streets
  and plots, deeper channels stay open and the walks span them. Nothing is
  graded or paved, only library structures are built (use stilt houses for
  `houses`), and each gets a gangway from its door to the nearest walk.
  Put the gates due north, south, east and west of the centre so the walks
  run on compass lines and the houses can face them (Lowmouth).

## Fields

`fields` metres of farmland go round the town, out to a ragged limit. They
are laid out from the roads the way strip fields were: each side of every
road out of town is cut into strips square to the road's general line
(bends smaller than a quarter field are straightened out first, so a lane
that zigzags down to a ford doesn't fan the walls out), a field's width
apart, in rows whose backs run parallel to it, and further back the fields
widen (every other cut, then every third) so the cuts behind still meet the
ones in front. Each patch of ground belongs to the road whose verge is
nearest it, as each plot in town belongs to its street, so where two roads'
fields meet they meet along a line. A boundary is found once for each pair
of neighbouring fields: two fields never each build a wall along the same
line, every field is closed, and boundaries stop short of the roads. Near the
town and on high ground the boundaries are heaped stone walls, further out
and lower down hedges with an oak here and there, and the odd paddock has
rails. A farmhouse and barn stand by the road every 250 m or so. `crops`
names regions painted on the fields, for biomes with `"crisp": true` so the
crop stops at the wall.

`farmland`, an outline, replaces the ragged limit: the fields fill it
(outside the town) and nothing past it, for a town whose farms have to stay
where an older map had them.

Roads the world already has count too. Every road the routing engine lays
with a surface wider than a fence goes on a list (`route::RoadLines`), and a
town adds its own roads out; a later town or `fields` event hangs fields off
all of them and keeps its walls clear of them. Ground something already holds
(a building, a yard, a path) is left alone by streets, plots and crops.

### Fields on their own

The `fields` event lays the same farmland in an outline away from any town,
off whatever roads run near it:

```json
{"year": 1790, "event": "fields", "area": [[930, 520], [1078, 401], ...],
 "crops": ["pasture", "hay", "pasture"], "kit": "harbor_town",
 "field_size": 70, "farmsteads": true}
```

It needs a road near the area to start from, and comes after the events
that lay those roads.

### Field sheds

Each field big enough (about 0.1 ha) has a corner away from the road, and by
the event's `shacks` rate a shed stands there, its back to the corner and
its door to the field, stepped further in where the corner is narrow and
left out where the ground is taken or steep. `shacks` is one chance for
every field (`0.2`), or a chance by crop region with a `default`
(`{"default": 0.15, "pasture": 0.3, "orchard": 0}`); 0 is none. Left out it
is 0.15. A `town` takes the same `shacks` for its own fields. The kit's
`shacks` list names the structures (`field_shed` and `field_shack` where it
names none; Kitehold's uses `field_bothy`). The summary says how many stood
and how many corners were crowded or steep.

## The kit

What a town is made of comes from the `town` section of a city kit
(`library/cities/`, named by `"kit"`): wall and roof materials by district
style, trims, street surfaces by rank (`main`, `street`, `lane`, `quay`, ...,
or `<style>_<rank>`), the plaza, flat roofs, the church, street and park
trees, route kits for field walls, rail fences, hedges, garden fences and
the town wall, and the sheds for field corners (`shacks`). `old_town` is a European town, `harbor_town` a New England
one, `cotswold_village` a limestone village with dry-stone walls (made for a
town that stands its world's own cottages on its plots), `settler_village` a
frontier settlement of dirt lanes, `cane_town` a Gulf-coast cane town of dirt
streets, white siding and tin roofs.

## The event

```json
{"year": 1200, "event": "town", "name": "Lynford", "at": "market", "radius": 190,
 "core": "maze", "core_frac": 0.4, "districts": ["maze", "grid", "village", "maze"],
 "gates": ["a_north", "a_west", "fork", "hill_west"], "fields": 150,
 "crops": ["pasture", "hay", "barley"], "kit": "old_town", "region": "town",
 "squares": 1, "landmarks": [{"structure": "market_cross", "near": "market"}]}
```

Other keys: `bounds`, an outline the town stays inside (the ground an older
settlement held); `jetties` (default true), false where the world has its
own wharves; `waterfront` and `ribbon` (0 to 1, default 1), how hard the town
is drawn to water and how far it strings out along the roads in; `gaps`,
metres between the houses of the closed blocks and at least this between
library houses; `gradient` (default true),
plots thinning toward the edge except along the roads in; `irregular`
(default true), corner buildings filling odd corners.

## Replacing an older settlement

Gannet Harbor, Shad Landing, Saurischia's old town, Hemlow, Chepworth,
Glassfall's village, Lowmouth and Mire were each hand-placed buildings,
paths and walled fields (or a `village` or `settlement` event) before a town event
replaced them. The steps:

1. Back up the chronicle. List the events in and around the old settlement:
   its buildings, its lanes and paths, its regions, its field regions and
   their wall ribbons, its signs.
2. Decide what stays. Bridges, mills, wharves and anything the story names
   stay as they are, and the town grows round them: ground they hold is
   taken, so streets and plots keep off it. Put the town event after them.
3. Remove the rest, and give the town `bounds` (or a radius that matches the
   old footprint) and `farmland` (an outline round the old fields), so the
   town and its farms cover the ground the old ones did.
4. Gates: one where each road or path came in. A gate set on a steep bank or
   against a bridge sends the road round the hill to reach it; set it a
   little short, and lay the last stretch with a `path` event.
5. Civic buildings the story names become `landmarks` with a `"poi"`; a
   world's own houses become `houses`. A town that later events build
   roads out of (Chepworth's lanes at 1300) can come just after them,
   with its gates where those roads end, so it grows round them. Walled
   fields away from the town become a `fields` event after the roads.
6. A world whose world.json says `"seeds": 1` keys each event by its place
   in the sorted chronicle, so removing or adding events moves every later
   event onto a new seed and reshuffles half the world. There, mark the new
   events `"late": true`, and give each later event that has no `seed` of its
   own a `"seed"` equal to its old place. Compare `--gazetteer` before and
   after: only the replaced settlement's lines should change.
7. `--validate`, then shoot it overhead and from a low angle; overhead shots
   from high up show coarse geometry, so judge the buildings from the low
   ones.

## Looking at one

`VG_TOWN_SKETCH=dir` writes each town's plan flat, a pixel a metre, as
`dir/<name>_plan.png` while the world builds: districts tinted, streets grey,
buildings red with their fronts yellow, library structures purple, walls
brown, hedges green.
`local/worlds/townlab` holds three towns for comparing changes, and
`tools/towns/shots.sh DIR` takes an overhead and a low-angle shot of each;
`tools/towns/JUDGE.md` is the fixed brief a reviewer scores them against.
