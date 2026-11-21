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
   `reference.md` (terrain, drainage, glaciate, volcano, impact, river,
   avulsion, settlement, fire, deposit, place, abandon, region, path, ...). Every event needs a `cause`
   in the story's words. If the story needs a process that doesn't exist, choose
   the nearest existing one and note the missing verb for the user.

3. **Check the library before adding to it.** Read `library/README.md` (how
   to work in it) and `library/CATALOGUE.md` (every entry, its notes, which
   worlds use it). Reuse entries where they fit. New entries go in as new files
   with `"introduced_by": "<world>"` and `"version": 1`. Never edit an entry a
   previous world uses; make `name_v2` or a new name instead. New materials
   leave out `id`: the game gives each world its own ids at load, since
   voxels are a byte and the library holds more materials than fit at once.
   Each creature and collectible model has its own palette block of up to
   192 colours, with no limit per world, so pick colours freely. List
   your biomes in `world.json` `biomes`, or every library biome competes in
   your world (and yours in the others that lack the list). Write library
   entries as JSON files directly, not from a `make_library.py`: the JSON is
   the source, and a script run later would overwrite every hand edit made
   since. Only an entry computed from code (a whole arena, an imported map)
   keeps a script, and it names it in `"_generated_by"` (library/README.md,
   "Rules"). Before editing an entry, check it has no `_generated_by`; if it
   has, change the script instead. When done, run
   `python tools/libfmt.py --catalogue` so the files are canonical and the
   catalogue is current.

4. **Write `world.json` and `chronicle.json`.** Generate the land, do not
   draw it: a `terrain` sketch, then `drainage` (and `glaciate` in ice
   country), as Kaldvik does. Read "Layout" below first. Run `--gazetteer`
   and `--map` to see where things landed, then place what stands on the
   land from names the generator left or a `seat` found (`{"from": name,
   "by": [..]}`), so it follows the land when a setting changes. You
   are bad at guessing coordinates blind: build often and read the map. The
   gazetteer ends with a height table every 64 m; `VG_PROBE=x,z,2` before
   `--gazetteer` adds a 2 m grid round one point, which is how to put a
   structure against a canyon wall or a cliff foot. For many repeated events
   (a ring of hoodoos, a row of fins, side canyons), a throwaway script may
   write them into `chronicle.json` once, but don't leave it in the world
   folder: the JSON is the source, people edit it by hand, and a script run
   later would undo every one of those edits (library/README.md, "Rules").

4b. **Grade it (optional).** A world may carry its own colour in
   `worlds/<name>/look.json`: a patch over whichever style it is seen in, with
   every field of a look file optional and the grade knobs under `knobs` by
   their names in `render/grade.rs` ("A world's own grading" in GUIDE.md).
   The distance ramp and the haze are what carry a world's climate; write it
   from the story (dust, wet air, cold sea light) and check it in a wide shot
   from 100 m up, not at head height. Leave it out and the world is drawn in
   the plain style, which is a fine answer.

5. **Build and look.** From the repository root (a separate `--target-dir`
   keeps this build from fighting any other build running at the same time;
   delete `target/world` with `rm -rf` once the world is finished, since each
   build directory takes 1.5 to 3 GB):

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

## Layout

Worlds laid out by hand read as robotic on the overhead map: a round
island, a few straight lines of features parallel or at right angles to
each other, round lakes, evenly spaced copies. On the ground
they can look fine. The cause is the method: `landmass` (a disc) and then
landforms stamped from coordinates an author typed, with nothing linking
them. So:

- **The `--map` is a deliverable.** Open it at every iteration and judge the
  layout before anything else. A world is not done while its map shows a
  ruled line, two features at a right angle, a circle, or a row of the same
  thing at even spacing.
- **Start from `terrain`, not `landmass`,** unless the story is a small round
  island. Land should run off at least one edge of the map.
- **Let water lay out the valleys.** `drainage` and `glaciate` find their own
  courses and their own tributaries. Steer them with a `course` in the
  sketch and an `inflow`; never draw the river.
- **Do not type a line.** If a feature must be drawn (`escarpment`, `canyon`,
  `vale`, `trough`), give it five or more points that are not in a row, and
  never two points sharing an x or a z.
- **No loops of copies.** A script that places `for k in range(n)` at a fixed
  step makes a comb. Scatter with jitter in position, size and bearing, or
  let the generator place them.
- **Every river has a source.** It rises at a spring and grows (`rise`),
  comes in over the edge (`from_edge`), or runs out of a lake. One that
  starts full width in the middle of a field is the first thing a player
  notices.
- **Pin things to a found river by its points, and re-pin after changes.**
  `sorrel_9` is the ninth point along the river, so anything that changes
  the river's length (a basin at its mouth) moves every point. Check on the
  `--map` where a bridge or a cliff dwelling landed whenever it must sit on
  a particular reach.
- **A wood is several woods.** One biome over a whole forest reads as
  boring however good its trees are. Split it by rule, not by hand: one
  kind on the high ground (`elev`), one in wet hollows (`seat` rule `low` +
  `moisture`), a few `glade` regions, and a core that is plainly different.
  Hemlow's Hollins is the example.
- **Caves are networks.** See `network` in the reference: nodes in loops at
  one `height`, chambers, mouths, a pit, lamps. Never a row of hand-laid
  `passage` tubes, which reads as a drain, not a cave.
- **Vary the relief.** Give the sketch high ground and basins of different
  sizes in different quarters, and leave `vary` up, so one part of the map
  is rugged and another plain.

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
