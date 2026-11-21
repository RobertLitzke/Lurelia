# The engine, by its seams

What you call to build something new on this engine, with the signatures that
matter and the traps that cost time. `ARCHITECTURE.md` says where code goes;
`GUIDE.md` says what the game does; this says how to talk to it.

It is thickest on the paths the shooter uses (`src/combat`, `src/shooter/`,
`docs/SHOOTER.md`). Where it is thin, read the module's own header comment:
every file has one.

## Units and frames

- **Metres** everywhere in gameplay code. **Voxels** (0.25 m) inside the
  world store and inside structure shape programs. `V = 4` is the conversion.
- **x east, z south, y up.** A compass bearing of 0 looks north, which is
  `-z`; 90 looks east. `yaw = -bearing.to_radians()`, and
  `Player::forward()` is `(-sin(yaw)cos(pitch), sin(pitch), -cos(yaw)cos(pitch))`.
- **The step is fixed at 1/60 s** (`game::STEP`). `Game::tick` is the
  simulation; per-frame work belongs in `prepare_frame` and
  `frame_params`, which interpolate by `Game::alpha`. Anything that changes
  the world from a draw path is a bug.

## The voxel world: `world::store::Store`

```rust
fn get(&self, x: i32, y: i32, z: i32) -> Option<u8>   // None = not loaded
fn cell(&self, x: i32, y: i32, z: i32) -> Cell        // Open | Passable | Foliage | Solid
fn solid(&self, x: i32, y: i32, z: i32) -> bool
fn loaded(&self, x: i32, y: i32, z: i32) -> bool
fn top_below(&self, x: i32, z: i32, from_vy: i32) -> Option<(i32, u8)>
fn top_m(&self) -> f32                                // the world's ceiling, metres
fn ray(&self, origin: Vec3, dir: Vec3, max_m: f32, stop: impl Fn(u8) -> bool) -> Option<Hit>
```

`Hit` is `{ voxel, prev, t, normal, mat }` with **`t` in metres** and `normal`
the face the ray entered through. The usual call is
`store.ray(from, dir, range, |m| m != AIR)`.

Traps:

- `get` returns `None` for unloaded chunks and `ray` **walks straight through
  them**. A ray fired at an unstreamed part of the world travels its whole
  length and hits nothing. Treat "no hit" as "no hit yet".
- `cell` is the movement answer, `solid` the physical one. Foliage is solid to
  a walker and not to a bike; don't reimplement that with `get`.
- Chunks are 32 voxels (8 m) across; `CHUNK`, `M`, `MY`, `PAD` live in
  `world/gen.rs`. A test that builds a `Store` by hand must insert the chunks
  it writes into (`s.chunks.insert((cx, cy, cz), Voxels::Uniform(0))`) before
  `s.set`.

For a surface height in metres from gameplay code, use
`creatures::terrain::voxel_ground(store, x, z, under) -> Option<(f32, u8)>`,
which searches **down from `under`** — that is how you find the floor of a
room with another floor above it.

## Moving a body: `physics`

```rust
struct Body { half_w: f32, height: f32, vel: Vec3, on_ground: bool, .. }
fn move_body(store, pos: &mut Vec3, b: &mut Body, delta: Vec3, step_up: f32) -> MoveResult
fn hits(store, p: Vec3, half_w: f32, height: f32) -> bool
fn snap_down(store, pos, b, max: f32) -> f32   // walk off a ledge without falling
fn free_body(store, pos, hw, h, max, hits) -> bool  // push a body out of rock
fn drawn_ground_lift(store, pos) -> f32        // the eye rides the drawn slope
```

`step_up` is 0.5 m for the player, which is what makes a quarter-metre
staircase walkable without a stair system. Collision blocks every move out of
solid rock, so anything that may end up inside the world (a teleport, a
respawn, a load) should `free_body` first — `Player::update` does.

## Materials and the look of a surface

`world/matlib.rs` is one table indexed by the voxel byte: faces, `smooth`,
rounding, passable, flags. Library JSON fills it at load, per world
(`Library::assign_ids`), so **a material's id is not stable between worlds** —
save and compare by name.

Two flags decide how built geometry reads:

- `"smooth": true` makes the mesher slope the material's top surface. A
  staircase of quarter-metre steps is drawn as one clean incline **when it
  rises a quarter-metre per metre of run** (also 1 per 1 and 2 per 1; see the
  "Slopes" note at the top of `mesh.rs`). At one voxel per two columns it
  still reads as steps. Lay ramps out from their rise at a supported grade.
- `"crisp": true` stops a ground material's border being blended with noise —
  what you want for paving and a works floor, not for grass.
- `"slab": true` makes every voxel of the material a half-height block, like a
  slab: the mesher draws it half a voxel high (`matlib::slab`), physics stands
  a body on its middle and steps onto it by half voxels, and
  `creatures::terrain::voxel_ground` reports its top. Ray casts still treat it
  as a whole voxel. The shooter's pickup platforms are made of it.

`"pattern": "flat"` is a single colour with no texture at all, which is how
you get a clean painted wall. The full list of patterns is in
`render/recipes.rs`.

## Drawing into the world without touching the renderer

`render::overlay::Overlay` takes camera-facing ribbons, pushed every frame and
cleared every frame:

```rust
fn line(&mut self, a: Vec3, b: Vec3, width: f32, color: [f32; 4])  // depth-tested
fn line_on_top(&mut self, a: Vec3, b: Vec3, width: f32, color: [f32; 4])
```

`width` is **metres**, so a 5 cm ribbon half a metre from the eye fills the
screen. Anything fired from the camera (a tracer, a thrown object) must start
clear of the eye; `combat::NEAR_CLEAR` (0.9 m) is how the shooter does it. The
cap is `MAX_LINES` (4096) a frame; the rest are dropped.

For solid things, push `(model_index, models::Instance)` onto the list
`Game::prepare_frame` builds. Models are built once
(`models::build_all`, then `Game::add_ride_models` appends more and records
the base index). A model's colours come from the palette, starting at its
`block`: 0 for the fixed block the built-in models, ride, guns and bots share
(`models::FIXED_SLOTS`), or a block of its own for a library model.

## Light

All of the engine's own light comes from the sky: the sun, a hemisphere fill,
and a baked sky-openness map that darkens whatever stands under something
else. `light()` in `scene.wgsl` is the whole model, and it is shared by
terrain and entities. Two consequences worth knowing before building
anything enclosed:

- **A roofed room is black.** Nothing in the daylight model lights an
  interior, and a material's `glow` lights only itself -- it is emissive, not
  a light source.
- **Lamps are the answer** (`render/lamps.rs`). A world places them with the
  `lamp` chronicle event; they carry a position, a reach and a strength, cast
  no shadow and ignore the sky map. `Game::prepare_frame` picks the nearest
  `lamps::MAX` (64) to the camera into a storage buffer and the shader loops
  over them with a half-Lambert falloff, so a surface facing away from a lamp
  is dim rather than black.

A lamp is light only. Build the fitting it hangs from into the structure and
give that fitting a material with `glow`, or the room is lit by nothing
visible. Lamps are not occluded by walls, so place them where the room they
light is the room they are in.

## The HUD: `render::hud::Hud`

```rust
fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4])
fn label(&mut self, x: f32, y: f32, s: f32, color: [f32; 4], text: &str) -> f32
fn text_width(text: &str, s: f32) -> f32
```

Pixels from the top left, `s` the scale handed to you. Colours are linear, so
a mid grey is about 0.2, not 0.5. There is no immediate-mode layout: measure
with `text_width` and place it yourself. `screenshot.rs` and `shooter/hud.rs`
are the two worked examples, one text-heavy and one not.

## Creatures: `hecs`, with a twist

Components live in an `hecs::World` at `Entities::world`. **`iter()` yields
the query's item tuple, not `(Entity, item)`** — to get the entity, ask for it
as a component:

```rust
for (e, look, age) in world.query::<(hecs::Entity, &Look, Option<&Age>)>().iter() { .. }
```

Getting this wrong is a type error that reads as if the tuple arity is wrong.

Useful pieces:

- `Look { model, pos, prev_pos, yaw, flash, .. }` — where a thing is drawn.
  `flash` tints it bright for a moment and decays in `walk.rs`; set it to 1.0
  to show a hit.
- `creatures::age::Age` with `Stage::Dead`, `Age::fresh_kill(now)` and
  `Age::old_bones(now, jitter)`; `now` is `Entities::clock`.
- To kill something, follow `creatures::carry::take`: strip `Critter`, `Body`,
  `herd::Kin` and `Anim`, then insert `Age::fresh_kill` and an `Anim` holding
  an `anim::ragdoll::Ragdoll` built from the model's `rig`. A model with no
  rig has nothing to go limp and just stops. `combat::shot::kill` is a
  smaller copy of the same recipe.
- `Entities::identify(models, origin, dir, max) -> Option<(f32, String)>`
  names what is under the crosshair but does not give you the entity;
  `combat::shot::nearest_creature` is the version that does.

## A new way of playing: `modes::Activity`

One activity runs at a time, layered over walking. Implement:

```rust
fn id(&self) -> ModeId;
fn tick(&mut self, cx: ActivityCtx, input: &MoveInput, actions: &mut Actions, dt: f32);
fn pauses_world(&self) -> bool { false }
fn camera(&self, player: &Player) -> Option<rig::Camera> { None }
fn zoom(&self) -> f32 { 1.0 }
fn hud(&self, hud: &mut Hud, w: f32, h: f32, s: f32, fov: f32) {}
fn overlay(&self, player: &Player, out: &mut Overlay) {}
fn instances(&self, player: &Player, alpha: f32, base: usize, models: &[Model],
             out: &mut Vec<(usize, Instance)>) {}
fn note(&mut self) -> Option<String> { None }
fn shutter(&mut self) {}
fn exit(&mut self, player: &mut Player) {}
```

`ActivityCtx` carries `&mut Player`, `&Store`, `&mut Entities`, `&[Model]` and
the screenshot flag, so an activity may move the player, push them, and hurt
an animal. `instances` is how it draws solid things; `base` is where its own
models start, which `Game::add_ride_models` records when it appends them.

Anything held in the camera's frame goes through `instances` rather than onto
the HUD, so it is lit like the rest of the world. Placing one means working
in the shader's own terms: an instance is turned by `rot` in model space and
then by `pos_yaw.w` about world Y, so model +x is the level right, +y is up
and +z is level *backward*. Pitch rides in `rot`; a part that must turn about
its own axis (a minigun's barrels) is a second model with its `pivot` moved
onto that axis. `shooter::hold::Grip` is the worked example, and it exists because
the point a shot leaves from has to be the muzzle that is drawn.

Then: add a `ModeId` (and its `name()`, which is the word a world writes in
`"modes"`), and start it from `Game::tick`. Screenshot mode starts from a key;
the shooter starts itself when the world has rules for it
(`Game::start_shooter`), and `Game::park_activity` keeps it aside while
something else wants the slot, so a detour does not reset the game.

**Actions are a mailbox, not a state.** `Actions` fields are set by the window
and cleared by whoever takes them (`std::mem::take`). An activity that does
not take `dig` will find the player digging the level apart. Mouse buttons
arrive as `dig` / `place` held; digits 1-9 as `select`; the wheel as
`brush_delta`.

Movement modes (bike, glider, rope) are different: they are `ride::Mode`
variants with exhaustive matches, so the compiler lists every place to fill
in.

## Library content

Each kind is a folder of one-JSON-per-entry under `library/`, loaded by
`Library::load` into a `Vec` plus a name index. To add a kind:

1. A `Deserialize` struct in `library/<kind>.rs` (`version`, `introduced_by` and
   `notes` on every entry).
2. A field on `Library`, a line in `Library::load`, a line in the `Index`, and
   a lookup method.
3. Checks in `Library::validate` — it returns a `Vec<String>` of problems and
   `cargo test` fails on any of them.
4. The kind's name in `KINDS` and `ORDER` in `tools/libfmt.py`, and, if a
   world reaches the entries through some new key, a line in `world_uses` so
   the catalogue counts them.

`python tools/libfmt.py` reformats everything to canonical form and
`--catalogue` rewrites `library/CATALOGUE.md`. Both are checked.

## World content

`world.json` is `world::fields::WorldDef`; every new field is `#[serde(default)]`
so older worlds keep loading. A chronicle event is one file in
`world/chronicle/events/` plus a line in `registry!`; see
`.claude/skills/worldgen/reference.md`, which is the full reference for every
event and every library format.

For built geometry there are two routes, and the choice matters:

- **A `ribbon`/`route` with a kit** for anything that *runs*: roads, fences,
  walkways, bridges. Takes any angle and any length.
- **A structure** (a shape program in voxels) for anything with a fixed shape.
  Ops are `box`, `walls`, `gable`, `ellipsoid`, `cylinder`, `log`, `disc`
  (both with an optional `yaw`),
  `segment` (a rod or rope through points, any angle, drawn as a smooth tube),
  `set`, `repeat`, `recolour`. `"air"` empties the structure's own voxels;
  `"carve"` takes the terrain with it. Later ops override earlier ones.

Some things are not voxels at all and have to be placed beside the structure
that implies them: a lamp's light, a sign's board, an area's name. Where a
script writes the structure, have it write those events into the chronicle
too (`worlds/slagdeck/make_arena.py` writes its own `lamp` events), or the
two drift apart the first time anything moves.

A structure may be large: `worlds/slagdeck` is a whole 84 x 68 m arena as one
entry, so nothing can drift out of line between its parts. Write a big one
from a Python script in the world folder (as `make_arena.py` does) that emits
ops in metres through `libfmt.render`, and treat the JSON as build
output. Building a floor on a plinth, with the pit hollowed out of it, avoids
carving terrain entirely.

## Running it without a window

```bash
cargo build --release --target-dir target/alt      # a second session uses its own dir; rm -rf it when done
B=./target/release/lurelia.exe
S="--world NAME --save $TEMP/vg_NAME --fresh --frames 8 --no-hud"
$B --world NAME --validate          # must say "no problems"
$B --world NAME --gazetteer         # every event's result, heights, biome shares
$B --world NAME --map shots/NAME.png
$B --headless $S --ground X,Z --look YAW,PITCH --screenshot shots/a.png
$B --headless $S --pos X,Y,Z --look YAW,-30 --view 160 --screenshot shots/air.png
```

Yaw is minus the compass bearing. Drop `--no-hud` to see the HUD in the shot.
`--fire N` holds the trigger for the last N steps and `--weapon N` takes out a
weapon first, which is how the shooter is checked. `--catalog DIR` renders a
review page of a world's content. Read the PNGs; the gazetteer will not tell
you that something is ugly.

`cargo test --release` runs the unit tests, validates every shader, checks the
whole library including unknown keys, and builds every world to 2 km. Build
with `--locked`: three crates are pinned for the GNU toolchain and
`cargo update` breaks the build.

## House style for new code

- A module header comment that says what the file is for and what it is not.
- Tests that assert behaviour in the domain's own words
  (`a_rocket_jump_hurts_and_lifts`), not implementation details.
- Comments explain *why*; the code says what. Where a constant was measured or
  chosen, say what it was chosen against.
- Data in `library/`, verbs in Rust. If a world needs a verb that does not
  exist, say so rather than faking it with data.
