---
name: worldgen
description: Create a new world for the 3D_game voxel engine from a premise or a bit of inspiration ("abandoned 1930s Utah desert town", "a drowned forest after a dam failed"). Writes the story, the chronicle of events that shapes the land, and any new library entries (materials, plants, structures, creatures, biomes), then validates, maps and screenshots the result and iterates. Use when asked for a new world, a new map, or new library content for one.
---

# Making a world from a premise

A world is three files in `worlds/<name>/` plus whatever it adds to the shared
`library/`. The engine builds everything else. Read `reference.md` in this
folder for every format, event kind and behaviour name before writing JSON.

```
library/            shared by every world; each world adds to it
  materials/ plants/ structures/ creatures/ biomes/   one JSON file per entry
worlds/<name>/
  world.json        size, sea level, seed, bedrock, spawn, default biomes
  story.md          a few pages of prose: the canon
  chronicle.json    dated events (processes) derived from the story
  look.json         optional: this world's own colour over the style's look
```

Behaviours are Rust code ("verbs") and entries are data ("nouns"). You can write
any number of nouns. A world that needs a verb that doesn't exist (a new process,
movement style, placement rule, texture pattern or tree form) needs a Rust change:
say so to the user and propose it rather than faking it with data. A new event kind
is one file plus a registry line ("Adding an event" in `reference.md`).

## Steps

1. **Premise to story.** Write `story.md` first, two to four pages. Decide what
   happened here, in order, and who was here. Every visible thing in the world
   should trace to something in the story. Give it a few points of interest that
   reward exploring and one to three things left unexplained (the anomaly budget),
   all tied to the story's theme so they don't read as random. Write plainly.
   Strangeness should have a source and fade with distance from it.

2. **Story to events.** List the story's events and map each to a process from
   `reference.md` (landmass, volcano, impact, river, avulsion, settlement,
   fire, deposit, place, abandon, region, path, ...). Every event needs a `cause`
   in the story's words. If the story needs a process that doesn't exist, choose
   the nearest existing one and note the missing verb for the user.

3. **Check the library before adding to it.** Read `library/README.md` (how
   to work in it) and `library/CATALOGUE.md` (every entry, its notes, which
   worlds use it). Reuse entries where they fit. New entries go in as new files
   with `"introduced_by": "<world>"` and `"version": 1`. Never edit an entry a
   previous world uses; make `name_v2` or a new name instead. New materials
   leave out `id`: the game gives each world its own ids at load, since
   voxels are a byte and the library holds more materials than fit at once.
   Creature and collectible colours share 128 palette slots per world: reuse
   exact RGB values of existing colours where you can. List
   your biomes in `world.json` `biomes`, or every library biome competes in
   your world (and yours in the others that lack the list). When done, run
   `python tools/libfmt.py --catalogue` so the files are canonical and the
   catalogue is current.

4. **Write `world.json` and `chronicle.json`.** Start with the landmass and
   large landforms, run `--gazetteer` and `--map` to see where things landed,
   then place rivers, settlements and single structures by named `places`. You
   are bad at guessing coordinates blind: build often and read the map. The
   gazetteer ends with a height table every 64 m; `VG_PROBE=x,z,2` before
   `--gazetteer` adds a 2 m grid round one point, which is how to put a
   structure against a canyon wall or a cliff foot. For a world with many
   repeated events (a ring of hoodoos, a row of fins, side canyons), write a
   small `make_chronicle.py` in the world folder that writes the JSON, as
   Redrock does; the JSON stays the source the game reads.

4b. **Grade it (optional).** A world may carry its own colour in
   `worlds/<name>/look.json`: a patch over whichever style it is seen in, with
   every field of a look file optional and the grade knobs under `knobs` by
   their names in `render/grade.rs` ("A world's own grading" in GUIDE.md).
   The distance ramp and the haze are what carry a world's climate; write it
   from the story (dust, wet air, cold sea light) and check it in a wide shot
   from 100 m up, not at head height. Leave it out and the world is drawn in
   the plain style, which is a fine answer.

5. **Build and look.** From the repository root (a separate `--target-dir`
   keeps this build from fighting any other build running at the same time):

   ```bash
   cargo build --release --target-dir target/world
   ./target/world/release/lurelia --world NAME --validate
   ./target/world/release/lurelia --world NAME --gazetteer
   ./target/world/release/lurelia --world NAME --map shots/NAME_map.png
   ```

   `--validate` must say "no problems". The gazetteer lists every event's result,
   points of interest, built structures with positions, peaks, biome shares and
   warnings. Read the map PNG: biome colours, rivers (blue), old beds (tan),
   structures (red), items (cyan), spawn (yellow).

6. **Screenshot the important places** and read the PNGs. Yaw is minus the
   compass bearing (yaw 0 looks north, -90 looks east):

   ```bash
   B=./target/world/release/lurelia; S="--world NAME --save ${TEMP:-${TMPDIR:-/tmp}}/vg_NAME --style calm --frames 8 --no-hud"
   $B --headless $S --screenshot shots/NAME_spawn.png
   $B --headless $S --ground X,Z --look YAW,PITCH --screenshot shots/NAME_place.png
   $B --headless $S --pos X,Y,Z --look YAW,-30 --view 160 --screenshot shots/NAME_aerial.png
   ```

   Check at least: the spawn view, each settlement or structure group up close,
   each strange place, one aerial. Look for floating or buried structures, plants
   growing where they shouldn't, biome edges that are straight lines or perfect
   circles, colours that are too saturated or too pale in the calm style, and
   structures placed in water.

7. **Iterate** until the views hold up. Typical fixes: move `places`, change
   event radii, adjust biome `rules` and `priority`, tone colours, change plant
   densities or placement verbs.

8. **Record it.** Add a short section to `GUIDE.md` under "Worlds and the library" (premise,
   what's in it, its anomalies), list new library entries in the commit or
   summary, and tell the user which verbs were missing.

## Rules of thumb

- Variety comes from different histories, not from jitter. Two worlds should
  differ in their event lists, not just their numbers.
- Every structure, anomaly and strange creature should cite an event.
- Calm-style colours: keep greens near the built-in grass (about 100,130,60);
  very pale or very saturated surfaces read as snow or plastic.
- Plant densities are per square metre: 0.02 is a wood, 0.005 scattered trees,
  0.0005 a lone tree every 45 m.
- Decay: set an `abandon` event or a `place` `age`; walls crumble from the top,
  roofs go in sections, moss grows with age. Natural features (`decays: false`)
  never decay. A gravity pass then makes sure nothing hangs: an upper storey
  over a caved-in roof gets the roof under it back, and small loose bits
  fall. `--gazetteer` logs how many voxels fell and were put back.
- Long cliff lines need `escarpment` with `reach`, or the raised side ends in
  a circular step. Break them up with short `canyon`s cut back into them;
  a straight wall a kilometre long looks artificial.
- For layered rock, give `strata` and let canyons, mesas and escarpments cut
  through it; benches line up with the bands by themselves.
- Leave no gap between a decision in `story.md` and the built world: after the
  last change, reread the story against the gazetteer (heights, which side of
  something a place is, what direction a fire ran) and fix whichever is wrong.
- Water: the sea, rivers and lakes are real water you can swim in (and
  paddle on: the kayak). Ground let down under the sea floods (`subside`,
  `bayou`); land plants can stand in it with `standing_water`.
- Large worlds take longer to build at startup (1024 m takes about 1.4 s).
