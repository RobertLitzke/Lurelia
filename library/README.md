# The shared library

Every world builds from this one library, and each new world adds to it. An
entry is a noun (data); what it can do is limited to the verbs written in Rust.
The formats and the verb names are in `.claude/skills/worldgen/reference.md`;
this file is how to work in the folder. `schema/` holds JSON Schemas generated
from the loader's types (structures, materials, plants, creatures, biomes and
kits); `cargo test` fails when one falls behind the code, and
`WRITE_SCHEMA=1 cargo test schema` rewrites them. A kind that takes `like`
asks for its required keys only of an entry without one.
`schema/structure.md` is the short form of it plus a review checklist, for an
agent asked to review a structure file.

```
materials/     one block material each: texture recipe, flags, what grows on it
               (`sprites`); a variant is `like` another
plants/        a growth form (a verb) and its sizes, bark and leaf materials,
               and optionally what it `affords` animals (docs/CREATURES.md)
structures/    shape programs in 0.25 m voxels: buildings, arches, boulders
kits/          cross-sections for lines through the world: roads, fences,
               bridges, boardwalks, bored passages (docs/ROUTES.md)
cities/        what an imported city is made of, and what to make of it
               instead: material roles, name swaps, ivy, more street trees
creatures/     a 32^3 shape program plus a movement verb
collectibles/  small finds, also 32^3 shape programs
biomes/        rules for where a biome applies, and what grows and lives there;
               a variant is `like` another
signs/         how a sign's board and lettering look, and the post under it
fx/            particle effects: dust, splashes, chips, falling leaves (src/fx)
sprites/       PNG overrides for the detail sprites (see its own README)
looks/         the light each style is drawn in: sky, fill, sun, haze
CATALOGUE.md   generated: every entry, who added it, which worlds use it
```

## Before adding anything

Read `CATALOGUE.md`. It lists every entry with its notes and the worlds that
reach it, so you can reuse what exists instead of adding a near copy. The
worlds it counts are those in `worlds/` (and `local/worlds/`, if you keep
private worlds there), plus the materials the engine names in its own code,
so an entry listed as unused is one no world can get at. It is generated: run `python tools/libfmt.py --catalogue` after you add or change
entries, and never edit it by hand.

## How a world picks entries

- **Biomes** compete for every cell, so each world lists the biomes it allows in
  `world.json` (`"biomes": [...]`). A world without the list gets all of them,
  and every biome a later world adds would leak into it.
- Everything else is reached by name: plants, features, creatures and
  collectibles through the world's biomes; structures, materials, sign styles
  and items through chronicle events, structure slots and `strata`.
- Each creature and collectible model gets a palette block of its own, so a
  world has no limit on how many colours its entries use, and there is no need
  to reuse exact RGB values from other entries. One model can use up to 192
  colours (a creature with a skeleton spends two of them on bone);
  `--validate` reports an entry that wants more.
- A biome may say what it sounds like with a `sound` block, where the
  defaults are wrong for it (GUIDE.md, "Sound"). Every field is optional:
  `wind`, a multiple on how much wind reaches the ground (0.6 a hollow or a
  closed wood, 1.3 a down the wind never leaves); `leaves`, a multiple on the
  rustle of the leaf voxels near the listener (0.7 for needles, which hiss,
  1.3 for bamboo or big loose leaves); `grass`, 0 to 1, how much grass there
  is to stir (otherwise from the surface material); and `ground`, what rain
  lands on (`soft`, `grass`, `leaves`, `stone` or `water`; otherwise from the
  surface material's name). Most biomes need none.
- A world can limit how it is played with `"modes"` in `world.json` (walk,
  bike, glide, climb, rope, screenshot, camera; see `src/modes.rs`). Leaving it
  out allows them all.
- A world says which games it is built for with `"games"`, each a name in
  `library/games/`, its own first. The launch screen offers those and opens on
  the first; `cargo test` fails a world that lists a game that is not there,
  or one it has no `shooter`, `ctf` or `race` block for. Leaving it out is a
  world to explore.
- A game (`library/games/<name>.json`) is a `kind` (explore, deathmatch, team,
  ctf or race), a `title` and `help` for the launch screen, and the rules it
  states: `weapons`, `health`, `health_max`, `armour`, `armour_max`,
  `respawn_s`, `give_all`, `ammo_without_gun`, `friendly_fire`,
  `"pickups": false` for a bare floor, `target`, `captures`, `return_s`,
  `need_home`, `laps`, `rivals`. A rule left out is the map's own. A variant
  says `"like": "deathmatch"` and what it changes; `instagib.json` is the
  example. See `src/games.rs`.

## Rules

- New entries carry `"introduced_by": "<world>"` and `"version": 1`.
- Never change an entry another world uses (check "used by" in the catalogue);
  add `name_v2` or a new name. You may change entries only your own world uses,
  and delete ones no world uses.
- `id` is legacy: leave it out of anything new. Voxels are a byte, so one
  world has room for 238 materials, and the library holds more than that. At
  load each world gets its own ids (`Library::assign_ids`): the materials it
  reaches keep the id their file asks for (older entries have one, 17 to 254;
  255 is `carve`), the rest take free ids, and the grain copies of its woods
  (plant barks and materials named like bark or wood, two each, along X and Z)
  take what is left. Saves name materials by key (`dead_wood@x`), never by
  number, so the ids in the older files no longer buy anything.
- A material the world cannot reach gets no id and no texture layer at all, so
  the library can grow without costing every world: Windbarrow installs 83
  layers of the 245 materials. A material the engine names in its own code
  must therefore be listed in `worldload::ENGINE_MATERIALS`, or it would draw
  as air in every world that does not name it; the test
  `materials_named_in_code_are_reached_by_every_world` checks that.
- Write `notes` on every entry: one sentence on what it is and, for
  structures, how to place it (which way it faces, what it carves).
- Keep files in canonical form: `python tools/libfmt.py` reformats everything
  (one key per line, short arrays inline, one op per line, floats to three
  decimals) and `--check` lists files that aren't.
- The JSON file is the source of an entry: write and edit it directly. A
  script that writes entries rewrites them every time it runs, so any edit
  made to the JSON since is lost without a word. Keep a script only for an
  entry that is computed rather than designed (a whole arena or imported map,
  geometry that is mostly arithmetic and will be regenerated with new
  numbers). Such an entry names its script in `"_generated_by"`, which the
  loader ignores; edit the script, never the file. The script writes through
  `libfmt.render` so its output is canonical. The worlds' old
  `make_library.py` scripts were retired for this reason: their entries had
  been edited as JSON since, and running them would have undone about
  seventy of those edits.

## When a kit, not a structure

A structure is a fixed block of voxels placed on a quarter turn. Anything
that runs — a fence, a wall, a road, a bridge, a walkway, a passage — is a
kit instead: it is drawn per voxel column along a polyline, so it takes any
angle, any length and any height without a new file. `rope_bridge_24` through
`rope_bridge_80` are eight files holding the same bridge at eight lengths;
`rope_span` is one kit that is any length. If you find yourself writing
`thing_20`, `thing_30`, `thing_40`, you want a kit.

`docs/ROUTES.md` has the field list and the events that lay a kit down.

## City kits

A world that imports a city (`city.json`) names a kit in `library/cities/`
from its `city` event (`"kit": "faerie"`, else `modern`). The kit says what
each part of a city is made of by its job -- `road`, `pavement`, `deck`,
`rail`, `chimney`, `doors`, `plinths` -- rather than leaving those names in
the drawing code, and `remap` swaps the names the import file chooses for
itself, building by building (a roof's shingle, a street tree's species).

A theme is the same thing: `"like": "modern"`, a remap, and `ivy`, `ivy_mat`
and `tree_boost`. Keeping it here rather than inline in a chronicle means a
second world can wear the same city, and the materials it reaches are found
by the same name chase every other entry uses.

## What a material does

Besides its texture, a material can say how it behaves. Each of these is
optional:

- `"passable": true`: bodies pass through it (hanging vines).
- `"water": true`: water drawn as solid voxels; walking creatures keep out.
- `"glow": 1.1`: gives its own light (0 to about 1.5): bioluminescent moss,
  glowing cave ceilings.
- `"night_glow": true`: the glow is a lamp behind glass, lit only from dusk
  to dawn, the glass dark by day (Kaldvik's windows, fairy lights, lanterns).
- `"glints": true`: single texels flash in the sun. Defaults to true when the
  name contains "glass".
- `"foliage": true`: the bike brushes through and a rope won't hold. Defaults
  to true for plant materials named like leaves, needles or fronds.
- `"tint": "meadow" | "leaves" | "needles"`: broad colour fields across the
  land, as on built-in grass and leaves.

The flags live in `src/world/matlib.rs` (`flag`); the shaders read the same
bits.

## Checking your entries

```bash
cargo build --release
./target/release/lurelia --world NAME --validate
./target/release/lurelia --world NAME --gazetteer
VG_PROBE=x,z,2 ./target/release/lurelia --world NAME --gazetteer
```

`--validate` catches unknown names, bad verbs, clashing ids, the colour
budget, and keys no entry type knows (a misspelt key would otherwise silently
take its default; `notes` and keys starting with `_` are comments).
`cargo test --release` runs the same checks over the whole library and builds
every world up to 2 km. The gazetteer ends with a height table every 64 m, and `VG_PROBE`
adds a close-up height grid round one point: use it to put a structure
against a cliff or on a rim instead of guessing. Then screenshot it (see the
worldgen skill): a shape program that validates can still look wrong.

For a structure, two tools do most of a review (`schema/structure.md` has the
checklist they leave for a person):

```bash
python tools/check_structure.py library/structures/NAME.json   # or a replacement file
python tools/shoot_structure.py NAME                           # screenshots/structures/NAME/
```
