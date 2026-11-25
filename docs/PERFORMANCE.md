# How the renderer stays fast

Written for a reader who knows programming but not graphics. It covers how a
frame is drawn, the three techniques that cost the most to get right, and how
to measure the game. GUIDE.md's "Performance" section is
the short version with the current numbers.

## How a frame is drawn

The world is made of voxels: cubes 25 cm on a side, stored in chunks of
32 x 32 x 32. Before anything can be drawn, each chunk is turned into a
*mesh*: a list of the flat rectangles (quads) where solid voxels meet air.
Nobody can see the inside of a hill, so only the surface becomes quads. Each
quad is two triangles, and the graphics card (GPU) draws triangles.

Drawing one frame happens in stages, roughly:

1. The CPU decides which chunks are in front of the camera at all (*frustum
   culling*: anything outside the pyramid-shaped field of view is skipped)
   and writes a list of draw commands.
2. The GPU runs a small program, the *vertex shader*, on every corner of every
   quad, to work out where that corner lands on the screen.
3. The GPU works out which screen pixels each triangle covers
   (*rasterisation*) and runs another small program, the *fragment shader*,
   on each of them to pick its colour.
4. A *depth buffer* holds, for every pixel, how far away the nearest thing
   drawn there so far is. A new pixel that is farther than what's already
   there is thrown away. That is how near things hide far things, whatever
   order they are drawn in.
5. Grass, water, the sky and a final colour-grading pass go on top.

At 300 frames a second the whole thing has 3.3 ms. The CPU and GPU work at
the same time (the CPU prepares frame N+1 while the GPU draws frame N), so
the slower of the two sets the frame rate.

This renderer is bound by the vertex shader, not by colouring pixels: render
the same view at a sixteenth of the resolution and it still costs about three
quarters as much. The cost is in the number of quads, and 85-90% of them are
tree crowns, because a smoothed crown makes one small quad per voxel face and
those cannot be merged into bigger rectangles the way flat ground can. Every
technique below follows from that.

## Don't draw what's hidden

Standing in a village and looking at the forest, the nearest trees form a
wall. Everything behind it is drawn and then thrown away pixel by pixel by
the depth buffer, but by then the expensive part (running the vertex shader
on every corner and setting up every triangle) has already been paid.

*Occlusion culling* (`render/occlusion.rs`) skips those chunks before they are
drawn. The question for each chunk is: is its box entirely behind things
already known to be in front of it? Answering that needs a depth buffer, and
the trick is to use last frame's.

At the end of each frame the GPU shrinks the depth buffer into a *pyramid*:
level 0 is half the screen's width and height, and each texel holds the
farthest depth among the pixels it covers. Each next level halves again, down
to a single texel. A texel high up the pyramid answers "what's the farthest
thing drawn anywhere in this big patch of screen?" in one read.

At the start of the next frame, a GPU program takes each visible chunk's box,
works out where it would have appeared on last frame's screen and how near
its nearest corner is, picks the pyramid level where that patch is a few
texels across, and reads them. If even the farthest thing drawn there is
nearer than the box's nearest corner, the box is wholly hidden, and the
program sets that chunk's draw commands to draw zero copies. The CPU never
waits for this; the GPU changes its own command list in place.

Why last frame, and why is it safe? The pyramid is only valid from the
camera that drew it, so boxes are projected with *that* camera, not the
current one. A chunk is skipped only if it was completely hidden from where
the camera stood a frame ago:

- Turning the head can't wrongly hide anything. Hiding depends only on where
  the eye is, not which way it looks, and anything that was off the old
  screen counts as visible.
- Walking can reveal something that was hidden a frame ago. It then appears
  one frame late. At walking speed and 100+ fps that's a sliver a few pixels
  wide for 5-10 ms.
- A jump (a teleport, loading a new world, resizing the window) skips the
  culling for one frame and draws everything.

It is checked by rendering the same views with culling on and off and
comparing the images: zero pixels differ. Facing a dense forest at a 256 m
detail radius it took the GPU frame from 13.7 ms to 8.9 ms, and an open view
from 6.0 to 3.1 ms. Building the pyramid costs about 0.12 ms.
`VOXEL_OCCLUSION=0` turns it off for comparisons.

## Tree crowns are meshed with bigger pieces

The world is stored in 25 cm voxels, but nothing says the mesh has to follow
them exactly. For leaves the mesher looks at the world in 50 cm cells
(2x2x2 voxels). A cell with at least five leaf voxels counts as leaves, and
the crown's surface is built from that coarser grid, its corners rounded the
same way the fine one's are. One 50 cm face replaces four 25 cm ones, so a
crown needs about a quarter of the quads. In the band of terrain 128-256 m
away, already stored at 50 cm, the cells are 1 m.

Three details make it work:

- The coarse surface doesn't have to line up with the bark. Bark is meshed as
  if the leaves weren't there, so limbs and leaves are two separate closed
  surfaces that pass through each other, and no crack can open where a limb
  enters the crown.
- Every number a cell's corners are worked out from comes from voxels both
  neighbouring chunks hold, so a corner on the border between two chunks
  comes out the same in both and the surface has no seams.
- The leaf cards (flat textured sprites) around each crown make its outline.
  Cards that would sit inside the coarser surface, where nobody can see them,
  are not generated at all.

Each leaf face is also lit by the crown's overall shape (the direction in
which the density of leaves falls off, worked out over a few cells) instead
of by its own tilt. That costs nothing at draw time: rounded faces are never
merged, so the bits that would hold a merged face's width and height are free
to hold this direction.

With the coarse cells, the GPU frame at a 256 m detail radius is 5.2 ms facing
a dense forest and 5.1 ms in a managed wood.

## The material table is published, not locked

Chunks are generated and meshed on eight background threads, so the world
keeps loading while you move. The mesher asks one lookup table about every
voxel of every chunk: for each material, whether it grows grass or leaf
sprites, or is a plant drawn only as sprites.

That table must not be behind a lock, not even a reader-writer lock. Taking
even a read lock writes to a counter inside the lock, and each write has to
move the 64-byte *cache line* the counter lives in from one CPU core's cache
to the next. Asked millions of times a second across eight threads, the cores
spend most of their time waiting on each other: with a lock, eight threads
took 125 ms per column of chunks and never finished a view, where one thread
took 8.8 ms and did. More threads being slower than one almost always means
exactly this.

The table changes only when a different world loads, so it is built once per
world, never freed, and published through a single atomic pointer. Readers
load the pointer, which needs no writes and so no contention. The same view
loads in 2.6 seconds rather than over 25.

## What else is in place

- Each quad is drawn as 4 corners with a shared list of how to join them into
  triangles, instead of 6 separate corners. Since the vertex shader is the
  bottleneck, that is worth about 30%.
- Flat faces are merged into large rectangles (*greedy meshing*).
- Faces pointing away from the camera are grouped per chunk, so whole groups
  can be skipped.
- Opaque things are drawn nearest first, so the depth buffer rejects hidden
  pixels before their colour is worked out. They are put in that order by
  counting them into buckets four metres deep rather than sorting them.
- Grass and leaf sprites are lit once per corner rather than once per pixel,
  and thin out with distance.
- Distant land is meshed at lower detail (*level of detail*): voxels 50 cm,
  then 1 m, and so on, the farther away.
- The slow-moving outer parts of the shadow maps are redrawn in slices across
  several frames rather than all at once, centred ahead of a moving player.
- Building the draw list tests a column's whole footprint before any of its
  chunks. At a 256 m radius on Lowmouth (1920x1080, shadows on, 7,800 columns
  loaded for 3,400 visible chunks) that takes about 0.6 ms, against 1.2 ms
  walking every chunk (`VOXEL_COLUMN_CULL=0`), with identical draw counts.
- Unloading frees voxel chunks on a background thread; freeing a few thousand
  32 KB chunks at once stalls the main thread for tens of milliseconds.
- Full detail stops at 128 m (`stream::FULL_DETAIL_M`) whatever the view
  distance; the view distance still sets where the coarse rings end. A 25 cm
  voxel 200 m away is under a pixel at 1080p. On Glassfall from 812,95,700 at
  1920x1080, a 256 m view drew 3.58M quads and 108k cards at 14.0 ms (median);
  with full detail held to 128 m it draws 2.32M quads and 79k cards at 9.7 ms,
  against 8.2 ms for a 128 m view. Past 256 m the picture is identical; the
  128-256 m band looks as it does at a 128 m view. `--full-detail 256` keeps
  full detail out to 256 m for comparing.

## Measuring it yourself

```bash
# GPU time per stage, printed as GPUPROF lines
VOXEL_GPU_PROFILE=1 target/release/lurelia --headless --world lowmouth \
  --size 1920x1080 --ground 430,700 --look 45,-5 --hour 12 --view 256 \
  --no-shadows --frames 180 --wait 200 --stats out.json
```

Headless runs are the repeatable ones: no window, the same rendering into an
image in memory, a fixed camera and a fixed hour. `VOXEL_TRACE=1` logs frames
over 16 ms with their CPU breakdown, and `--bench` flies a fixed route and
reports frame-time percentiles.

Three things make a measurement a lie, all of them easy to miss:

- **Something else is using the GPU.** Another copy of the game in the
  background holds it at 100% and every timing is shared with it. Measure on
  an otherwise idle machine.
- **The world hasn't finished loading.** A run that waits a fixed time and
  measures anyway measures a world with holes in it, which looks fast. Always
  check `load_s` in the stats file is below `--wait`: one forest view went
  from an apparent 8 ms to a real 13.7 ms once it had actually loaded.
- **The settings aren't the ones being played.** The documented defaults are
  2560x1440 with a 128 m detail radius; a player's saved settings may be
  1920x1080 at 256 m with shadows off, which is a different workload. Pass
  the settings you mean to measure.

`--threads 1` shows how generation scales, and `VOXEL_PACING=0`, `VOXEL_OCCLUSION=0`,
`VOXEL_FRINGE=0`, `VOXEL_GRAIN=0` and `VOXEL_COLUMN_CULL=0` turn individual
features off for comparisons. `VOXEL_BUILD_THREADS=1` runs the world build's
per-cell stages as a single pass.

A fourth thing to hold in mind when reading a headless number: **the headless
loop waits for the GPU every frame.** It builds a frame, submits it and polls
until it is done, so its frame time is CPU *plus* GPU. A windowed frame
overlaps the two, so it is about `max(CPU, GPU)`. A headless 8 ms frame made
of 3 ms CPU and 5 ms GPU is a 5 ms frame on screen, not an 8 ms one -- which
is why the two stages are worth reading separately (`render_cpu_ms` and
`drawlist_ms` in the stats file, `VOXEL_GPU_PROFILE=1` for the GPU).

A windowed frame under vsync has one more catch: **the loop's intervals are
not the display's.** On a 60 Hz screen with the frame queue two deep, the loop
runs a couple of frames ahead in 7 ms each and then waits 30-50 ms for a
buffer, while the display still shows one frame every 16.7 ms. A `--bench`
run's `p95_ms` of 45 ms there is that bunching, not a slow frame. The world
used to move by those raw intervals, so motion juddered at a steady 60 fps,
and a faster build juddered more because it got further ahead before each
wait. `pacing.rs` now averages the last twelve intervals and snaps the step
to the refresh. The windowed bench reports the steps too: `step_jump_ms` is
the mean change in step from one frame to the next, and `VOXEL_PACING=0`
turns the smoothing off. On redrock at 1080p it went from 22 ms to 0.3 ms,
with every step between 16.5 and 17.2 ms.

Under vsync the swapchain now queues one frame, not two (`frame_latency` in
main.rs; `VOXEL_FRAME_LATENCY=N` overrides it). Each queued frame is another
refresh between moving the mouse and seeing it, and with the GPU frame at 3-6
ms nothing is gained by queueing more. On redrock the worst loop waits went
from 47 ms to 34 ms and the average stayed 60 fps. Without vsync it is still
two.

## Finding hitches in play

The headless runs miss what only happens in a window: sound, real input, a
vehicle, animals up close. The window keeps its own account of each frame:
the frame loop and the tick time their parts (`trace.rs`: tick, and inside it
creatures, player, platforms, activity, aim; then stream, models, hud, sound,
render), and the wait for the display is kept apart from the work. A frame
with over 16 ms of work, or a wait over 50 ms (the GPU still busy, not
vsync), is a hitch. The stats panel (F3) shows the worst of the last five
seconds as `HITCH 21 MS  STREAM`, naming its biggest part, or GPU for a long
wait. With `VOXEL_TRACE=1` every hitch is printed with its parts and the
streamer's own (accept, plan, visibility, unload, queue). Headless
`VOXEL_TRACE=1` lines carry the tick's parts too, in brackets.

The flight (`--bench`) is fast and high: up to 100 m/s, which no one plays at,
and it never sees an animal close. `--bench-route field` walks once round a
loop 50 m across at eye height instead, looking from side to side, from the
spawn or `--ground`. On Hemlow's spawn (the West Field, about a hundred
animals in view, sheep, horses and cattle among them) that is the work of
playing on foot:

```bash
target/release/lurelia --world hemlow --bench-route field --view 256 --no-shadows
```

At 1080p with no MSAA, headless frames there are 5.0 ms at p50 and 7.7 ms at
p99. The window used to catch one hitch a run, 130-240 ms on the first frame
after loading. That frame painted every material's texture (`textures::build`,
250-300 ms of CPU), because the renderer starts in the classic style and the
world loads in another, and with new materials and a new style in the same
frame it could paint them twice. The loading thread now paints them
(`textures::prebuild`) and the renderer takes them if they are for the
material table and style it needs, and a frame paints at most once. The first
frame is now under 20 ms in the window. Headless, the first frame still waits
about 250 ms on the GPU: headless loading never submits, so every chunk
uploaded while loading is copied then.

`--bench-route race`, on a world with a race (Ribstrand, Peregrine,
Vermilion), drives the race on autopilot to the finish with the race's own
camera. A race is the most streaming the game does: the kart goes round a
768 m world at 100 km/h, and the columns behind it unload and stream in again
next lap, so the world is still loading for over half the race. The report
adds the work under the 16 ms the tracer looks for: `work_p50_ms`,
`frames_over_12ms`, `wait_over_8ms` (frames held up by the display or the
GPU, not by work), and `worst`, the sixteen slowest frames with their parts.
At 300 fps a 12 ms frame shows, though the tracer never names it.

```bash
VOXEL_TRACE=1 target/release/lurelia --world ribstrand --bench --bench-route race
```

That race found the main thread losing its core. Frames with 1 ms of work
waited 10-40 ms; a streamer step that takes 0.5 ms took 50; command encoding
took 79 ms once. None of it showed in the GPU timestamps (no frame's passes
over 8 ms). The chunk workers ran below normal priority, but most likely
Windows lifted them for a moment each time they woke from a wait, which puts a
below-normal worker level with the main thread, and the main thread then sat
behind one for a whole quantum. Three
laps, 1920x1080, shadows on, no MSAA, vsync off, i5-13500 with 8 workers:

| | frames over 12 ms | over 16 ms | waits over 8 ms | p99 |
|---|---|---|---|---|
| workers below normal | 134 | 53 | 10 | 9.5 ms |
| main thread above normal | 6 | 4 | 2 | 7.2 ms |
| workers lowest | 1 | 2 | 1 | 6.4 ms |
| both (now) | 1 | 2 | 0 | 5.4 ms |
| both, 14 busy processes beside it | 11 | 5 | 0 | 7.5 ms |

The frame left over 12 ms is the race putting the karts on the grid, once,
during the countdown. The workers now run at the lowest priority and the main
thread above normal (`util::set_thread_priority`), so a build running beside
the game no longer takes the main thread's core either. The sound device's
thread is time-critical, above both. Columns load at about the same rate.

The karts' own physics was two thirds of a race's tick. Each wheel reads the
ground at a dozen points every substep, and each point is the drawn slope
(`mesh::ground_height_h`), whose four corners each read sixteen columns of
the store: about 50,000 voxel reads a kart a step, each a hash lookup. Now
`Vehicle::step` copies the voxels the step can reach into a flat box first
(`store::Window`), and the window remembers each ground corner it has worked
out, which the points under one wheel and the four substeps mostly share.
The kart lab (`vehicle/lab.rs`, three laps, four karts, headless) times
`Vehicle::step` and prints a fingerprint of where the karts end up, which
did not change by a bit:

| | Ribstrand, kart | Vermilion, heavy kart |
|---|---|---|
| store reads | 0.42 ms a kart-step | 0.53 ms |
| window | 0.33 ms | |
| window and corners | 0.09 ms | 0.07 ms |

```bash
WORLD=ribstrand TUNE=kart cargo test --release kart_lab -- --ignored --nocapture
```

What shadows and antialiasing cost, from `VOXEL_GPU_PROFILE=1` at 1920x1080,
256 m view (GPU ms a frame, average over each run):

| | Redrock flight | Hemlow field |
|---|---|---|
| no shadows, no MSAA | 3.5 | 3.1 |
| shadows | 3.7 | |
| 4x MSAA | 5.8 | |
| both | 6.1 | 4.8 |

Shadows are about 0.2 ms; 4x MSAA is about 2.3 ms, nearly all of it terrain.

## What a column costs

Headless stats report `gen_stage_ms`: the milliseconds a column spends in
each stage, averaged over the run's columns, from timers summed across the
workers (`gen::GEN_NANOS`, `stream::WORK_NANOS`, `mesh::MESH_NANOS`). On the
redrock flight a column took 11.1 ms, and 7.4 ms of it was meshing; of that,
4.4 ms was the leaning stems (`mesh/lean.rs`), where a column hands the
mesher about 300 threads (juniper and pinyon sticks). Two things were
quadratic or wasted there: each thread looked for the threads joined to its
ends by checking every other thread, and every piece of every face was
worked out in every chunk the thread crossed, only for `owned` to drop the
ones another chunk draws. Thread ends are now found through a small grid of
cells (`Ends`), and a piece whose line misses the chunk is skipped. A column
is 9.0 ms, the leaning stems 2.8 ms; the meshes are the same, quad for quad,
and so are the pictures.

What is left of a redrock column: voxel fill 2.4 ms, the six face sweeps
2.0 ms, leaning stems 2.8 ms (of which about 0.9 ms is each chunk refining
the same thread's pieces again), the rest under 0.6 ms together. Eight
workers is about right on a 14-core i5-13500: sixteen generated 24% more
columns, each a third slower, since past six they share the hyperthreads
and the efficiency cores.

Most of what was left of the leaning stems was `anchor`, which finds the
plain voxel face a tube's quad hangs from and the turn of its corners that
needs the least move. It worked out each turn's cost several times over
inside `min_by`, and was called in full where only part was wanted: `owned`
needs only the face's voxel, and the refining of a thread's pieces (`fits`)
only whether some turn fits, which it can stop at. The test below meshes the
49 columns round a spot on one thread and hashes every quad; the hashes are
the same before and after, on Saurischia, Hemlow, Redrock and Kitehold.

| Columns, single thread | lean before | after | meshing before | after |
|---|---|---|---|---|
| Saurischia marsh (470, 735) | 6.25 ms | 3.25 ms | 8.67 ms | 5.48 ms |
| Saurischia Fernwood (330, 600) | 1.50 ms | 1.01 ms | 3.60 ms | 3.13 ms |
| Hemlow spawn | 0.30 ms | 0.20 ms | 1.32 ms | 1.27 ms |

```bash
WORLD=saurischia AT=470,735 cargo test --release lean_world -- --ignored --nocapture
```

In the marsh what is left of the lean stage is drawing the threads' faces
(about 1.3 ms, each face hung by `anchor` in full), refining them (0.7 ms,
each chunk a thread crosses refining it again) and finding their voxels and
joins (0.5 ms).

## Building a world

`--gazetteer` prints what each chronicle event cost and then the stages after
it, which is where the time goes on a big world. On an imported city 5.8 km
across, the `city` event takes 3.3 s as one pass, and the per-cell stages
split across cores:

| Stage | One pass | Split by rows |
|---|---|---|
| biome classify | 4,890 ms | 490 ms |
| fields derive | 1,920 ms | 760 ms |

Biome classification is 33 million cells against up to ninety biomes, each
decided on its own, so it splits by rows. `Fields::derive` splits its slope
pass the same way and runs its three distance transforms side by side (sweeps
cannot be split by rows, but they do not read each other).

The rule that keeps this safe: **split the part that only reads, and keep the
part that writes in one piece, in its sequential order.** The expensive half
of most of these stages is a pure function of a spot and a seed, and the half
that has to stay sequential is cheap.

| Stage, and what was split | One thread | All of them |
|---|---|---|
| Jadewater's 40 `tower` events: scanning down for the skin over each cell | 5,109 ms | 1,297 ms |
| The city's 90,000 street trees: growing them | 1,358 ms | 492 ms |
| Glassfall's plants: growing what the scan decided on | 62 ms | 36 ms |

A tower writes the heightfield cell by cell after the scan, so two towers can
still be told apart by which rose first. The plant scan itself cannot be
split at all: every plant placed blocks the ground around it for the ones
after it (`taken` in `flora::place_plants`), so which plants there are
depends on the order they are decided in. Only the growing runs in parallel,
which is why the city's plant stage gains little from threads (1.4 s of it is the scan
rejecting candidates, not growing them).

Every world's `--map` image is identical with `VOXEL_BUILD_THREADS=1` and
without, which is the check to run after touching any of this.

**The binary in `target/` may not be the one you think.** Another session
building from the same working tree replaces it, so a "before and after" can
be the same code twice. Compare `VOXEL_BUILD_THREADS=1` against the default on
one binary instead of keeping an old one around, on an idle machine.
