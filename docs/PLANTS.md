# Plant placement

How a world decides where each plant grows. A biome's `plants` list says which
plants may grow in it, how dense and by which placement verb
(`.claude/skills/worldgen/reference.md`, "biomes"). On top of that, every
world measures what each patch of its ground offers a plant (the site fields),
and a world on placement v2 puts each plant on the ground that suits it. The
code is `src/world/site.rs` (the fields) and `src/world/flora.rs`
(`place_plants`, `place_by_list`, `place_by_site`).

## Choosing a placement

world.json `plants` picks it:

- `"v1"`, or left out: by list. Each biome present walks its `plants` list in
  order, and every plant placed blocks the ground round it for those after,
  so the plant listed first takes a contested spot. Site blocks are ignored.
- `"v2"`: by site, described below. Biomes still say which plants may grow
  where and how many; the site fields decide where inside the biome each one
  stands.

Any other value counts as v1, and building the world warns about it, as it
does for a `fill` outside 0 to 1. Every shipped world with plants is on v2; the
zoo and the arenas leave the field out.

`fill` (0 to 1, default 0) adds plants a biome doesn't list; it only does
anything under v2. See "Fill" below.

```json
{ "plants": "v2", "fill": 0.2 }
```

## Site fields

Built for every world the chronicle builds, whatever its placement, after
decay and gravity and just before the plants (`Site::build`). They read the
finished land, the structures and the roads, never the plants. The grid is
the world survey's, 4 m cells (`survey::cell_index`), one byte a cell per
field, read as 0 to 1 and bilinearly between cell middles. They are not
saved: a 1536 m world builds them in well under a second.

| Field | What it measures |
|---|---|
| `wet` | Water in the ground. Open water and the sea are 1. Elsewhere the soft-or of the climate (up to 0.45 from the world's moisture) and three local terms: the water table (a cell standing little above the drainage its water runs to, reaching 0 at 5 m above it; drainage is open water, the sea, or a valley floor collecting about 1.3 ha of flow), convergence (the flow a cell collects from upslope against its own slope), and depth below the mean ground within 20 m (full at 2.5 m), for divots and washes too small for the flow |
| `wet_table`, `wet_flow`, `wet_hollow` | The three local terms of `wet` on their own, for the map. A site block can't name them |
| `soil` | Acid and poor (0) to base-rich (1), from the biome's `soil` (0.5 if unset), graded linearly toward the midpoint over 15 m either side of a biome border |
| `sun` | Warmth of the ground by slope and aspect against the sun's path at the equinox: 0.5 on flat ground, lower on faces turned from the sun, higher on faces turned to it |
| `exposure` | Height over the mean ground within 100 m and within 20 m, plus how squarely the slope faces into the world's wind |
| `cold` | 0.15 at sea level up to 1 at 220 m above it, the same scale in every world; colder at higher latitudes (world.json `sun.latitude`, 42 if unset), on north faces and in hollows where cold air pools, milder within 150 m of the sea |
| `rock` | Steep slopes, cliffs and bare stone surface materials, less where loose deposits (sand, ash, silt) lie deep over the rock |
| `disturbed` | Fires in the last 80 years (fading with age), the ground within 12 m of paths, roads, routed lines and circuits, within 20 m of built structures (those that decay), and cinders, ash or lava crust at the surface (0.6) |
| `shade` | The land's own shade: three quarters the share of the equinox day the sun is behind the land or the ground is turned from it, one quarter the sky cut off. Placement v2 adds crown shade as it goes |

Each cell also keeps its nearest other biome and the distance to that border
(`Site::other`, `Site::border`), which soil grading and border zones use.

To look at them, open the ecology map (backquote, then P) and step through its
layers with `[` and `]`, or draw one headless with `--eco-map wet` (any field
name). The map also has `fit` (each plant coloured by how well its ground
suits it, shade left out; grey for plants with no site block), `border`
(grey plants belong to the biome they stand in, magenta ones came from the
neighbour) and `realm`.

## A plant's site block

`site` on a plant entry (`library/plants/*.json`) gives a soft range
`[lo, hi]` on any of `wet`, `soil`, `sun`, `exposure`, `cold`, `rock`,
`disturbed` and `shade`:

```json
"site": { "soil": [0.0, 0.45], "wet": [0.0, 0.8], "shade": [0.0, 0.3] }
```

A field's fit is 1 inside the range, falls linearly to 0.03 over 0.15
outside it and stays at 0.03 beyond, so a plant grows on ground outside its
range rarely, not never (`site::fit`). A plant's suitability at a point is the
product of the fits of the fields it names (`Site::suits`); a field left out
doesn't matter to it. `shade` is read as the land's shade with the crowns of
taller plants already placed (below). A plant with no site block suits
everywhere and grows wherever its biome's verb puts it.

Rough values: `wet` 0.1-0.3 is a dry ridge, 0.5-0.8 a gully floor, 1 open
water; `soil` 0.15-0.2 heath and bog, 0.85-0.9 chalk. Read the real spread
in the biomes a plant is listed in with `--site-report` before setting a
range.

`--validate` reports a range outside 0 to 1 or with `lo` above `hi`, and a
site block on a vine or crust (they grow on their hosts, so it does
nothing). An unknown field name fails to load. Water plants, skeleton
entries and the two generic saplings go without one.

## A biome's soil

`soil` on a biome entry (`library/biomes/*.json`), 0 to 1: acid and poor
(heath, bog, pine sand) to base-rich (limestone, chalk, rich meadow). Left
out it is 0.5, or the parent's with `like`; `--validate` reports a value
outside 0 to 1. Only the `soil` site field reads it.

## Placement v2

`place_by_site`, in order:

1. Every plant use with a density above 0 in each biome present is an entry,
   plus the uses fill adds. Vines and crusts are set aside and placed after
   their hosts, as in v1.
2. Entries go in by layer, tallest first, by the mean of their `height`:
   canopy (8 m and up), sub-canopy (3 m), shrub (0.8 m), herb.
3. In a layer each entry bids on its own jittered grid. The grid is made
   denser by the plant's mean suitability over its biome (floored at 0.2),
   and each spot is kept with chance verb weight times suitability times its
   border share (below). So a plant keeps about its listed density in its
   biome but moves onto the ground there it suits best, and one that suits
   nowhere stays thin. The per-spot checks are v1's: slope, water, decks,
   structures, `standing_water` and `water_depth`.
4. The layer's bids are taken in a race keyed `-ln(u) / suitability`, so the
   better suited plant usually takes a contested spot and the worse one now
   and then. Trunks keep apart and a verb's `spacing` holds, as in v1.
5. Each plant placed darkens a 2 m shade map under its crown: a disc of its
   crown radius, pushed away from the noon sun by 0.6 of its height, darker
   for conifers than for birches, lighter for crowns with a high `open` or a
   low `fill`. The next layer's `shade` reads it.

Bids are made on all cores, a plant per task; the result does not depend on
the thread count.

### Border zones

Within 15 m either side of a biome border the two biomes' plants share the
ground (`zone_share`). A plant's border share is 1 deep in its own biome,
0.5 at the line and 0 at 15 m into the neighbour, so the neighbour's plants
take over across the zone in proportion and suitability decides which plant
gets each spot. The border stays hard where either biome has `fringe` 0 or
lists no plants. `VG_BORDER=0` turns the zones off.

### Stand structure

Woods get ages, gaps, saplings and dead trees. `VG_STANDS=0` turns all of it
off.

- Ages: each canopy and sub-canopy tree of a woody form takes a size from its
  hash: about a quarter young (0.6 to 0.75 of grown), over half grown (0.9 to
  1.05), a fifth old (1.04 to 1.15, only where the room overhead allows). Its
  crown shade follows its size.
- Gaps: a slow noise thins the canopy layer in patches 30 to 60 m across,
  about a tenth of a wood. Most trees that would have stood there are gone;
  35% of them stand or lie dead instead. Elsewhere 5% of canopy and
  sub-canopy trees are dead.
- Dead trees: only trees of the forms that raise saplings (conifer, spiral,
  columnar and the broadleaf forms) and grow at least 5 m tall. Each is drawn
  with a skeleton entry (a plant with `skeleton`: `standing`, `broken`,
  `stump` or `log`) in that tree's size and bark: about 30% standing, 25%
  snapped, 15% stumps and 30% fallen, and half fallen in a gap. A generic
  snag, stump or log that a biome lists is drawn as one of that biome's own
  trees, dead, picked by their density; a biome with no such trees keeps the
  generic entry as it is. Named dead trees (`cypress_snag`, `dead_juniper`
  and so on) are ordinary entries and placed like any other.
- Saplings: after the canopy and again after the sub-canopy, each tree just
  placed raises up to two saplings (three round an old tree), one to two and
  a half crowns from its trunk. Each is kept by its border share, the
  parent's suitability there (shade aside) and the light (1 - 0.6 x shade).
  They are the generic `sapling_conifer` or `sapling_broadleaf` worn in the
  parent's colours. Saplings a biome lists by name (`sapling_fir`) are
  placed as ordinary entries.

### Fill

World.json `fill` (0 to 1; `VG_FILL` overrides it) lets plants a biome
doesn't list join it where its habitat suits them (`fill_uses`). At 0, the
default, biomes grow their lists only.

- The pool is every plant some biome in this world lists, that has a site
  block, grows on land and is not a vine, crust or skeleton.
- Each biome needs a `habitat` (`library/habitats/`), which gives baseline
  values for `wet`, `soil`, `shade`, `rock` and `disturbed`. A pool plant
  suits a habitat by the product of its fits on the fields both give
  (`HabitatDef::suits`), and joins only at 0.5 or more.
- Fill only adds to layers the biome already has: a grassland that lists no
  trees takes none. In each layer the newcomers share `fill` times the
  biome's listed density in that layer, by how well they suit.
- A plant listed only in field, garden or settlement biomes here is a crop
  or a garden plant and joins only those.
- Realms: a newcomer must share a realm with the biome (world.json
  `realms`, a plant's `realms`, `real` if it names none), and each realm that
  comes through a biome also brings its own cast for that habitat
  (`habitats` on a `library/realms/` entry) at the densities written there.
  Every fill bid is weighted by its plant's realm share of the cell.

Fill uses are then placed by v2 like listed ones.

## Ground cover

The site fields also set how lush the ground cover grows (`Site::lush`, 0 to
1, 0.5 ordinary ground): wetter ground, north faces and sheltered dips
lusher; dry ground, south faces, wind-cut ridges and thin rocky ground
drier. Grass sprites follow it (a meadow's tall share and the tuft count lean
with it, `mesh/sprites.rs`), and so does the colour of grassy ground (meadow
and understory materials), tilted along its usual green-to-ochre blend
(`site_lean` in `scene.wgsl`).

This does not depend on the placement: v1 worlds and the arenas have site
fields too, so their grass and its colour follow the site as well.

SITE LUSHNESS on the LOOK page's GRADE page scales it (0 off, 1 as tuned, up
to 3). The colour follows at once, the grass a moment after the knob stops,
when the land round you is built again. `VG_LUSH=0` turns it off entirely.

## Comparison switches

Environment variables, for comparing a world with and without each part:

| Variable | Default | Effect |
|---|---|---|
| `VG_PLANTS` | world.json `plants` | `v1` or `v2` forces that placement on any world |
| `VG_BORDER` | on | `0` keeps each biome's plants inside it (no border zones) |
| `VG_FILL` | world.json `fill`, else 0 | a share 0 to 1 overrides the world's fill |
| `VG_STANDS` | on | `0` drops ages, gaps, saplings and dead trees |
| `VG_LUSH` | on | `0` leaves grass and ground colour as they were without site fields |

`VG_BORDER`, `VG_STANDS` and `VG_FILL` only change v2.

## The site report

```bash
cargo run --release -- --world kitehold --site-report
VG_PLANTS=v1 cargo run --release -- --world kitehold --site-report
```

`--site-report` builds the world, prints, then exits:

- the grid size and how long each part of the site build took;
- for each biome by area: its cell count, hectares and `soil`, then each
  field's min, quartiles and max over its dry land;
- the placement in use, then for each plant with a site block that stands in
  the world: its count, mean suitability (shade left out), the share standing
  where it suits at least half, and for each ranged field the median at its
  feet against its range and the share where that field alone fits under
  half; then a line for all of them together;
- every plant's count by the biome it stands in;
- under v2, the layer counts, saplings, dead trees and fill uses.

Running it under `VG_PLANTS=v1` and `v2` and comparing the per-plant lines is
how to check a site block: a plant whose median sits outside its range, or
whose count drops sharply under v2, has a range that misses its biomes.
