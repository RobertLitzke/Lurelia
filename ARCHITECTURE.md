# Architecture

How the code is split and where a new thing goes. GUIDE.md covers what the
game does in detail; this file covers where to change it.

## Layers

Each layer uses the ones below it and never the ones above.

| Layer | What | Where |
|---|---|---|
| App | window, screens (title, launch, settings, loading, playing, pause menu, help), what a run was launched with, the games a world can be played as, key table, CLI, dev tools | `main.rs`, `menu.rs`, `launch.rs`, `games.rs`, `input.rs`, `pad.rs`, `cli.rs`, `tools/` |
| Sound | the device, the mixer and its four buses, the ambient voices, the one-shot effects and the animal calls, all synthesised; what the ear gets from the world, worked out by `bed.rs` and `watch.rs`, the two files with game knowledge | `sound/`; see GUIDE.md, "Sound" |
| Editing | the LOOK page (F7) and the EDIT page (F8): a creature or plant in a studio with its library entry beside it, written back to `library/`; both drawn as one form, with one kind of knob | `form.rs`, `lookpage.rs`, `editor/`; see `docs/LOOK.md`, `docs/EDITOR.md` |
| On-screen type | distance-field text (Cinzel, Cormorant Garamond Italic, OFL) and the Lurelia logo, compiled in; rebake the atlas with `library/fonts/bake_atlas.py` | `render/font.rs`, `render/hud.rs`, `library/fonts/`, `library/logo/` |
| Modes | walking, bike, glider, climbing, rope, kayak, four wheels (`vehicle/`); the camera rig every mode hands the renderer; the camera (film, tasks, scrapbook); screenshot mode, the shooter's rules and a race (`race/`, set up by `racemenu.rs`) as activities | `modes.rs`, `ride.rs`, `ride/`, `vehicle/`, `race/`, `racemenu.rs`, `rig.rs`, `camera/`, `screenshot.rs`, `shooter/`, `player.rs` |
| Combat | guns, shots, shells, bursts and damage, with no rules about why | `combat.rs`, `combat/`, `library/weapons/`; see `docs/SHOOTER.md` |
| Game | the session: one world, its player, creatures and saves, a fixed 60 Hz step; the lifts and portals step through `WorldParts`, which the tournament drives too | `game.rs`, `world_parts.rs` |
| Creatures | living things and how they move and act | `entities.rs` and friends; see `docs/CREATURES.md` |
| Content | the shared library and the world builder | `library/`, `inherit.rs`, `variety.rs`, `worldload.rs`, `world/chronicle/`, `world/fields.rs`, `world/flora.rs`, `world/canopy.rs`, `world/city.rs`, `world/route/`, `world/town/`, `world/seam.rs` |
| Engine | voxels, meshing, streaming, physics, rendering | `world/store.rs`, `world/gen.rs`, `world/stamp.rs`, `world/material.rs`, `world/matlib.rs`, `mesh.rs`, `mesh/`, `stream.rs`, `physics.rs`, `models.rs`, `render/`, `shaders/` |
| Shared | small helpers with no game knowledge | `util.rs` (Fx hash, blur, compass bearing), `world/clock.rs`, `world/weather.rs`, `world/season.rs`, `world/noise.rs` |

## Engine seams

- **Voxel queries.** `Store::cell(x, y, z)` says how a voxel behaves for
  movement (open, passable, foliage, solid); `Store::solid`, `top_below` and
  `ray` (distance, face normal, material) build on it. The world height comes
  from the world, not a constant.
- **Foliage shapes.** `world/canopy.rs` holds the four shapes a crown is built
  from (ball, filled dome, flat plate, sleeve along a limb) and measures each
  one as a single normalised distance, so `world/stamp.rs` rasterises all four
  through one loop. `world/flora.rs` decides which shapes a growth form reaches
  for; nothing else knows the difference.

- **Stamps.** `world/stamp.rs` holds everything that presses itself into a
  chunk's voxels: a trunk with its branches and leaf blobs, a hanging vine, a
  crust of moss or a shelf of fungus. `world/gen.rs` hands each one the chunk
  (`Stamp::press`) and never asks what kind of thing it is, so a new kind of
  feature is a function there and an arm in `Stamp for Tree`, not another
  branch in the generator.

- **Lines through the world.** `world/route/` turns a polyline and a kit
  (`library/kits/*.json`) into geometry: `cost.rs` finds the line over the
  land, `mod.rs` grades it and cuts it into segments, `ribbon.rs` decides
  each voxel column from the kit's cross-section, and `plan.rs` lays out a
  settlement's streets and the plots on them. Roads, fences, bridges,
  boardwalks and bored passages all go through it, so none of them is a
  structure with a fixed length or a quarter-turn bearing. See
  `docs/ROUTES.md`.

- **Generated land.** `world/chronicle/hydro.rs` routes water over a coarse
  copy of the height field (every cell to the sea, hollows filled to their
  lips), counts what drains through each cell, cuts where a lot does, and
  turns the streams into lines. `terrain` makes the land from a sketch,
  `drainage` cuts it with that water, and `glaciate` puts a `trough` down
  every stream that collects enough ice. None of them draws a line an author
  typed, which is why their maps do not look ruled. What they make is named
  (`EventCtx::seat`), and later events build from those names (`Pos::Rel`,
  the `seat` event), so a village follows its fjord when the land changes.
  Worlds made before this (`landmass` and stamped landforms) are unchanged.

- **Neighbours.** A world that names `neighbours` in its world.json is
  built to meet them (`world/seam.rs`). `worldload::edges_for` builds each
  neighbour first, with its own library, and takes a strip off its edge;
  `build_world_with` then blends the land into the strip after the
  chronicle, leans the coast and river distances toward the neighbour's
  after `Fields::derive`, and drops trees that would stand in the
  neighbour's crowns. The neighbour is built before the world's own
  library is installed, since building one installs its material table.
  The older world never changes.

- **Seeds.** Everything a world generates is seeded by a key that stays put
  when the world is edited: an event by its kind, its year and which of that
  kind in that year it is (`world/chronicle/mod.rs` `event_keys`), a biome by
  its name and one of its plants, features or collectibles by its own name
  (`world/flora.rs` `name_key`, `use_key`). Keying by a place in a list means
  adding an entry in the middle moves everything after it, which would
  reshuffle land that is already built. A world that says `"seeds": 1` in its
  `world.json` keeps the older list-position keys, which every shipped world
  does so its land stays as built; a new world leaves the field out.

- **Towns.** `world/town/` grows a town from a few inputs and returns a
  plan (streets with ranks, building footprints, squares, walls, fields)
  without touching voxels or the library; it reads the land through its
  `Ground` trait. The `town` event hands the footprints and streets to the
  city code below as if they had been imported, grades the streets, and lays
  the walls and fences as route ribbons. See `docs/TOWNS.md`.

- **Cities.** An imported city (`city.json`) is drawn with a *kit*:
  `library/cities/<name>.json`, named by the `city` event's `"kit"`. The kit
  says what each part of a city is made of by job (road, pavement, deck,
  doors, plinths) rather than by name, and may remap names the import file
  chooses for itself. A theme is a kit with `"like": "modern"`, a remap and a
  little ivy -- the same entry either way, so nothing a world sets there is
  out of another world's reach.

- **Materials.** `world/matlib.rs` holds one table: faces, smoothing,
  rounding, passable, and behaviour flags (`flag::GLINTS`, `WATER`,
  `FOLIAGE`, the tint flags). Library JSON sets them; the shaders read the low
  byte. Saves store material names, not ids.
- **Packed quads.** `mesh/quad.rs` defines the 16-byte quad; the shaders get
  the same field positions through the generated prelude.
- **Shaders.** `render/shaders.rs` assembles every WGSL module and prepends
  constants generated from Rust (shadow size, fade distances, wind, quad
  layout, material flags). `cargo test` parses and validates every module.
- **Renderer.** `Renderer::new` needs only the device and screen;
  `set_world`, `set_mist`, `set_models` (with the palette), `set_look` and
  `set_grade` give it a world, so switching worlds keeps the device resources.
- **The grade.** `render/grade.rs` holds a style's three gradient strips and
  its nineteen knobs. The strips go to the GPU as one 256x3 texture (row 0 by
  distance for the haze, the low sky and the shadowed light; row 1 the
  luminance tint; row 2 the gradient map) and the knobs as one small uniform,
  both re-sent each frame, so nothing about the grade is baked into a shader.
  `lookpage.rs` is the LOOK page (F7) that edits them live, with a colour
  picker for the stops, and writes the result back to the look file;
  `docs/LOOK.md` explains every value.
- **The studio.** The EDIT page (`editor/`, F8) stands one library entry in
  front of the camera to be looked at and tweaked. Two seams carry it:
  `Generator::studio_floor` lays its grey grid platform as generated ground
  (so nothing of it reaches a save, and the plant being previewed is drawn
  after it), and `Generator::studio` is a list of trees `trees_near` appends
  from, which is how a plant preview is drawn by the world's own code. A
  creature subject is appended after everything the world loaded -- models,
  species, palette colours -- so dropping it is a truncate.
  `docs/EDITOR.md` has the rest.
- **The camera rig.** `rig.rs` holds the `Camera` every mode hands the
  renderer and the `Chase` that any of them may have. A first-person view is
  `Camera::looking`; a third-person one is a `Chase`, which keeps the follow
  point that trails the body between steps (`begin_step`, `follow`) and turns
  it into a camera when the frame is drawn, pulled back out of the ground and
  aimed so the screen's centre is where the subject is looking. `Game::camera`
  picks whose camera this frame is. `src/camera/` is a different thing with a
  similar name: the photographic camera (film, tasks, the scrapbook).

- **The body and what it rides.** `Player` is a body that walks; `Ride` is
  what it is riding, and `Game` holds the two side by side. A ride steers the
  body it carries (`Ride::step` takes `&mut Player`), which is why the ride
  cannot live inside the player, and a shooter's bots each own a `Player` that
  would otherwise carry a bike, a wingsuit and a rope they never use. A bot
  steps with `Player::walk`; `Player::update` is the walk with a ride in front
  of it. Both stamp `begin_step` first, so the frame is drawn from where the
  body began even when a ride ends mid-step.

- **Sound.** `sound/` sits beside the renderer and has the same shape:
  `Game` is surveyed into a row of numbers (`Heard`, by `sound/bed.rs`) and
  the window hands it to the device once a frame with the listener's facing,
  the way it hands the renderer a `FrameParams`. One-shots go the other way
  round: anything may post one with `sound::emit(Noise)` -- a kind and a
  world position, no more -- and the window takes them once a frame and
  places them against the listener. That queue is the one place sound
  reaches down the layers, and it is a no-op until a `Sound` (or a
  `--sound-bake`) is listening, so a headless match or a tournament pays a
  relaxed atomic load a shot. Combat posts its gunfire because only the
  trigger knows which fire mode went off; everything a match does besides is
  heard by `sound/watch.rs` comparing the player's and the bots' state from
  one frame to the next, so the shooter's rules know nothing about sound.
  Nothing in the audio callback allocates, locks or blocks: the moment
  crosses over through a `try_lock` and every number glides.

- **The weather.** `world/weather.rs` holds one global kind of day and turns
  it into a row of numbers (`Sky`): cover, overcast, haze, mist, rain, snow,
  heat, wind, and shifts to the grade. It knows nothing of the renderer or the
  creatures; three places read it. `render/daylight.rs` `weather` puts it over
  the hour's light, after `apply`, so it changes only the `Look`, the `Sun`
  and the `Air` the time of day already decided. The shaders get it as the
  `weather` field of the frame's globals (cover, rain, snow, wind) and in the
  post uniform (the falling rain and snow). The spawner multiplies each
  species' quota by `Sky::abroad`, which a `Taste` answers for. Fair weather
  is every multiplier 1 and every addition 0, so a sunny world draws exactly
  as it would with no weather. See GUIDE.md, "Weather".

- **The seasons.** `world/season.rs` holds which part of the year the world
  is in and splits into two halves, because a season is partly voxels. What is
  read every frame is a row of numbers (`Phase`: the green multiply, the
  straw, the grade shifts, the weather it leans toward, how much of each build
  of animal is abroad), and it crossfades over the first half of a season. What
  is baked into the mesh -- leaf colour, bare branches, blossom -- goes through
  `Turn`, which the generator holds in one atomic: every tree asks it which
  season it is wearing and answers from its own seed, so a turn of the year
  walks through a wood instead of crossing it. `flora::dress` is the one place
  a tree is put into those clothes, on its way out of `Generator::trees_near`;
  `Streamer::set_turn` is what has the loaded columns built again. Summer is
  every multiplier 1 and every addition 0, so a summer world draws exactly as
  it would with no seasons. See GUIDE.md, "Seasons".

- **What the weather leaves.** `world/damp.rs` is the weather's history: a
  few world-wide numbers (wet, standing water, frost in shade and in sun,
  fog in the hollows) stepped on the game clock beside the weather and the
  year. The shaders get them as the `damp` field of the frame's globals and
  decide where each shows from what they already know about a point (its
  slope, its hollow below the smoothed ground, its shadow), so there is no
  map of them; the fog goes in as more mist floor. Dry ground is every
  value 0. See GUIDE.md, "What the weather leaves".

- **Marks and senses.** `creatures/sign.rs` reads a ground material for how
  loud a step is and whether it keeps a print (`Tread`, by the material's
  key), and keeps the prints and trampled patches walkers and the player
  leave (`Marks`, worn by the weather). Walkers report footfalls through
  `Entities::steps`; the game lays them. `render/signs.rs` draws the prints
  as multiplied quads after the land; the trampled patches reach the sprite
  shader as a small map round the camera (`render/foliage.rs`). The same
  `Tread` and the weather feed the player's `Threat` (`game.rs`
  `player_threat`), which `creatures/startle.rs` sums by sense: sight,
  noise, closeness, scent. See GUIDE.md, "Stalking, prints and trampled
  grass".

- **Big bodies.** Walkers are capsules from rump to snout as well as boxes
  at their feet (`creatures/crowd.rs`, sized by `Model::reach`, measured
  from the voxels at load). `crowd::update` pushes apart any two that
  overlap after each step, herd.rs spaces members by the gap between their
  spines, and anything over 1.5 m tall turns at a limited rate (walk.rs
  `turn_rate`). Big walkers report shrubs in their way through
  `Entities::crush` instead of steering round them; `game.rs`
  `trample_shrubs` flattens them through `Streamer::edit_passing` and
  `creatures/trample.rs` stands them back up minutes later. Heavy footfalls
  carry a `Step::weight`, which `game.rs` `feel_steps` turns into a small
  jolt of the camera. See docs/CREATURES.md, "Big bodies in a herd".
  Large animals (`Species::large`) are spawned and kept twice as far out
  (`spawn::LARGE_REACH`), walk the generator's ground past full detail
  (`terrain::far_ground`), and are remembered where they were dropped and
  put back on the player's return (`creatures/tracked.rs`, held by
  `Entities::tracked`).

- **Lamps.** `render/lamps.rs`: point lights a world places (`lamp` event),
  the only light anything with a roof over it gets. `Game::prepare_frame`
  picks the nearest to the camera; `lamp_light` in `common.wgsl` adds them in
  `light()`, unshadowed and untouched by the sky-openness map.
- **Under the land.** `under_land` in `scene.wgsl`, folded into
  `far_shadow`: the sun is stopped at the land's height field, because rock
  casts no shadow. The mesher only makes a face where a voxel meets air, so
  the inside of a hill is in no shadow map, and without this a slanting ray
  that travels through it would light the walls of anything buried (a cave,
  a cellar) in patches that crawl as the sun moves. A point below the field is inside the hill, unless the land
  within a few metres along the sun lies lower than the point itself, which
  is how a mesa's face or a canyon wall (carved under the plateau the field
  remembers) keeps its sun. VOXEL_SUNK_SHADOW=0 for comparisons.
- **Overlay.** `render/overlay.rs`: lines a mode pushes each frame
  (`Game::overlay`, or `Activity::overlay`), drawn as ribbons in the world,
  depth-tested or on top.

## Adding things

- **A material, plant, structure, creature with existing verbs, biome or
  collectible:** JSON in `library/` (see `library/README.md`).
- **A city kit or theme:** JSON in `library/cities/`, named by a world's
  `city` event. `"like": "modern"` starts from the plain one.
- **A kind of weather:** a `weather::Kind` variant with its `sky` row, how
  long a spell of it lasts, what may follow it and how common it is. Every
  match on it is exhaustive, so the compiler lists the places to fill in.
- **A season** (a monsoon, a dry season under its own name): a `season::Season`
  variant with its `phase` row and what the trees wear in it (`leaf`). The
  year goes round the variants in order, visiting the ones a world has, and
  every match is exhaustive. Nothing else needs touching: the renderer, the
  generator and the spawner all read the row.
- **A look:** `library/looks/<style>.json`, or tune it in the game on the
  LOOK page (F7) and EXPORT, which writes that same file. A new style (a third
  set of shader paths) is a code change in `render/` and the shaders.
- **A chronicle event:** one file in `world/chronicle/events/` and one line in
  its registry (".claude/skills/worldgen/reference.md", "Adding an event").
- **A creature behaviour:** see `docs/CREATURES.md`.
- **A movement mode:** a `ride::Mode` variant. Its camera, HUD name and
  per-mode tables are exhaustive matches, so the compiler lists every place
  to fill in. Add a `ModeId` so worlds can allow or forbid it, and check its
  entry with `Ride::can`.
- **A vehicle:** a file in `library/vehicles/`. A subtype says
  `"like": "sand_rail"` and the numbers it changes (`beach_buggy.json` is the
  example); every key is in `vehicle/spec.rs`, with its default. A new body
  shape is a builder in `vehicle/draw.rs` named by the spec's `model`. A new
  way of handling (karts) is a `spec::Handling` variant and its tyre function
  beside `grip::slip`. A world lists what it has in `"vehicles"`, and one that
  lists none has the ATV (`fields::default_vehicles`). Nothing in
  `vehicle/` reads keys: `ride/drive.rs` turns them into `vehicle::Controls`,
  and a bot or a race would fill the same struct. `vehicle/bot.rs` is one:
  a `Driver` that follows a `Track` (a closed centreline and a width), aiming
  at a point ahead and planning its speed from the bends; its `Style` is what
  a race would vary per opponent.
- **A race:** a `.track` file built by `tools/course/track_file.py`
  (`tools/course/FORMAT.md`) into the world's `track.json`, the road frozen a
  sample a metre with its race furniture, which `chronicle/race_track.rs`
  stamps after the chronicle so the world round it can't move it;
  world.json's `race` block names it
  (`worlds/peregrine`, `worlds/vermilion`). The older `circuit` event lays a road over the land
  without that guarantee. `race/` is the activity; it reaches the
  player's car through `Activity::race_mut`, which `Game` hands the ride to
  after each fixed step, since `ActivityCtx` carries no ride.
- **An activity** (a race, another kind of shooter): implement
  `modes::Activity` (tick, camera, HUD, overlay lines, which actions it
  takes), add a `ModeId`, and start it from `Game::tick`. Screenshot mode is
  the small example and `shooter/` the full one, which also shows how
  `Game::park_activity` keeps an activity aside while another borrows the
  slot.
- **A weapon:** JSON in `library/weapons/`; its shape in the hand is a
  function in `combat/viewmodel.rs` and a name in `SHAPES`.
- **A light indoors:** a `lamp` event in the world's chronicle.
- **A different set of shooter rules** (one life, no respawn): a second
  `Activity` beside `shooter/` over the same `combat` layer. The camera is not
  an activity: its film, tasks and scrapbook outlive putting it away, so
  `Game` keeps it (`Game::camera`, `camera_actions`).
- **A photo task need:** a `camera::tasks::Need` variant with its `line`,
  how it is checked (`check` for the whole photo, `holds_for` for the
  subject) and where `make` may offer it. If it needs something the photo
  doesn't measure, add it to `camera::scene::Seen` or `Shot`.
- **A key:** a row in `input.rs` `BINDINGS`, and a `HELP` row if players
  should see it; the help screen is built from the table.
- **A world's allowed modes:** `"modes"` in its `world.json`.
- **The games a world offers:** `"games"` in its `world.json`, its own first
  (`launch::games`). The launch screen offers those and no others.
- **A game, or a variant of one:** a file in `library/games/` and its name in
  a world's `"games"`. A variant says `"like": "deathmatch"` and the rules it
  changes (`instagib.json`). `games.rs` holds every key and is the one place
  the map's rules, the game's and the launch screen's choices are put
  together (`GameDef::shooter_rules`, `race_rules`); `Game` starts the
  activity from the result without knowing which game it is. A new kind of
  game is a `launch::Mode` variant, an `Activity`, and its arm in `games.rs`.
- **A dev tool or CLI flag:** `cli.rs` for the flag, `tools/` for the code.

`docs/API.md` is the working guide to these seams: the signatures, the units
and the traps.

## Checks

```bash
cargo test --release
```

runs the unit tests, validates every shader, checks the library (unknown keys
included) and builds every world up to 2 km.

Two switches run a slower path, to prove the faster one draws the same world:

```bash
VOXEL_COLUMN_CULL=0   # walk every loaded chunk instead of testing columns first
VOXEL_BUILD_THREADS=1 # run the world build's per-cell stages as one pass
```

The draw counts in `--stats` and a `--map` image should not move by a byte
when either is flipped. Build with `--locked`: three crates in `Cargo.lock`
are pinned for the GNU toolchain (GUIDE.md, "Build and run"), and
`cargo update` breaks the build.
