# Under water

How diving, the underwater look, reef terrain, coral and reef fish work. GUIDE.md "Under water"
is the short version for players and world authors; this file is the longer one, with the
numbers and the reasons behind them.

## Scuba

Scuba is a state of the player (`src/player/dive.rs`, `Player::dive`), not a ride mode, because
floating and swimming already lived on the player. There is no gear to fetch: any water deep
enough works, sea, lake, river or cenote.

### Going under and coming up

Ctrl pressed while floating dives if the column from the surface down 1.5 m (`MIN_DEPTH`) is
open water or air at every quarter metre (`deep_enough`); otherwise the HUD says "TOO SHALLOW TO
DIVE". On the way under (`go_under`) the eye stays where it is and the body under it becomes a
swimmer lying flat: 0.7 m tall (`DIVE_H`) with the eye at 0.42 m (`DIVE_EYE`). Collision is
`physics::hits_wet` (water voxels are not in the way), so a gap between coral heads or a hole in
a voxel pool that a standing body would not fit through can be swum.

The dive ends when the eye comes within 0.08 m of the surface and the body is not sinking
(`come_up`). The body stands up under the eye if the column is clear, and the surface float in
`player.rs` takes over. Rising faster than 2.6 m/s (`LEAP_V`) as the head breaks the surface is a
leap: vertical speed becomes `1.5 v + 1.5` and a `splash` cue fires at the surface.

### Swimming

W/S swim along the view with pitch included, A/D sideways, Space up and Ctrl down. There is no
gravity: velocity eases toward what the keys ask at 1.9 per second (`EASE`), plus the water's
current, so letting go of everything leaves the body hanging where it is and drifting with the
current. Cruise is 2.6 m/s.

Shift is a kick. It adds speed along the swim direction (or the view, with no keys held) up to
5.6 m/s (`KICK`), which then eases back to cruise. A kick 0.45-1.3 s after the last one chains,
up to three, each 22% stronger than the one before (5.6, 6.8, 8.1 m/s); one sooner than 0.45 s
is ignored, so mashing the key does nothing. Each kick and a slow breath every 3.5-5 s push a
`bubbles` cue to `src/fx`.

### Breath

A breath is 60 s under the surface (`BREATH`) and is whole again the moment the head is out.
Nobody drowns. Over the last 10 s (`TIRED`) the swim speed falls toward half, and at zero the
keys stop counting and the body rises at 1.6 m/s until it surfaces. The HUD (`build_breath` in
`game.rs`) shows BREATH over a bar that turns orange-red under 10 s.

### Holding on to big swimmers

Hold E under water near a big swimmer to take hold (`Game::hitch_step`). E is the camera's focus
key, reused; it only grabs while diving and with the camera put away. Anything whose model is at
least 0.32 m half-width or 0.9 m tall qualifies, within `1.4 + max(half_w, height / 2)` metres of
its middle: mantas, turtles, whales, whale sharks. The diver's offset in the animal's own frame
is kept (clamped to 1.6 m), and each step the body is put there. It lets go when E is released,
on a kick, when the animal is gone, or when the place it would carry the body to is rock or not
loaded.

The breath keeps running while holding on, but a held diver is not pushed up when it runs out:
the out-of-breath rise only applies once the diver lets go.

The big animals do not shy from a diver. Mantas, humpbacks, whale sharks and reef sharks have
`flightiness` 0, which turns the startle code off for them; sea turtles keep 0.4.

### The kayak that waits

There is one kayak (`src/ride/kayak.rs`). K in the kayak with no footing in reach rolls the
paddler over the side into deep water and leaves the boat moored where it was
(`Ride::moored`, drawn by `Ride::moored_instance` bobbing and rolling a little in place). K
within 2.6 m of it, afloat with the eye within 1.2 m of the surface, climbs back in. K anywhere
else launches the kayak there, which takes it from wherever it was moored. While it is moored
the HUD adds its distance and compass point beside BREATH, for example "KAYAK 34 M NE".

The kayak's launch test searches 40 m down for a bed; water deeper than that reads as 40 m, so a
launch from a swim far offshore still works.

## The look under water

The renderer fills one uniform, `g.under` (`src/render/mod.rs`): x is the water surface over the
camera, y the visibility in metres, z the sunlight falloff per metre, and w is 1 when the camera
is in inland water standing above the sea. `g.fog.z` is 1 while the camera is under. The shader
functions are in `src/shaders/common.wgsl`.

- `water_top()` is the surface light comes down through: the camera's own lake, river or cenote
  surface when w is set, the sea level otherwise. Everything below that is lit as under water,
  so a cenote standing 30 m above the sea is lit right while you are in it.
- `underwater_color()` mixes the world's `water` palette (`shallow` near the top, `deep` by
  22 m down, engine colours if the world has none) by the camera's depth, darkened at night.
  `haze()` blends toward it by `1 - exp(-dist / visibility)`.
- `water_tint(dist)` takes colour out of every lit surface by distance, red first, then green,
  then blue (extinction ratios 0.75, 0.16, 0.05 per visibility). This is what makes far coral go
  blue-green instead of grey.
- Direct sunlight under the surface falls by `exp(-depth * light)`, takes a slight blue cast,
  and is multiplied by `0.72 + 0.9 * caustics(p)`. The ambient turns toward the water colour as
  the direct light fades (`scene.wgsl`).
- `caustics(p)` traces the point back up the sun direction to the surface and takes two layers
  of ridged value noise drifting different ways, brightest where both ridges cross, fading with
  depth.
- `water_shafts()` marches six samples up to 32 m along the view and adds sun where a slowly
  drifting noise patch of the surface is overhead.
- The surface seen from below (`fs_water` in `scene.wgsl`) shows the sky through Snell's
  window, about 48.6 degrees either side of straight up, rippled and with a bright rim; past the
  window it mirrors the dark water.
- The light motes become marine snow: every mote shows as a pale fleck, not only the ones in a
  sunbeam (`foliage.wgsl`).
- Sprites from materials with the `underwater` flag carry the `WET` bit; below the sea surface
  they sway on a slow surge instead of bending with the wind.
- With the camera under, chunks further than five visibilities are not drawn (99% hidden by
  then). Shadows still use the full test. This is what pays for dense coral.

A world sets its water in world.json with `"underwater": {"visibility": m, "light": per_m}`
(`fields::Underwater`, default 14 and 0.16). Visibility is the e-folding distance of the green
channel. The clearest is Jadewater (30 m, 0.07), then Glassfall (16, 0.13); the murkiest are
Thornpan (5, 0.3) and Lowmouth's delta (4, 0.35).

## Depth and the sea floor

Voxel row 0 is the floor of the world (`Store::cell` answers Solid below it), so the sea can be
at most `sea_level_m` deep: 24 m on Glassfall, 12 m on Jadewater. Reef walls run down to a metre
above that floor and let the fog do the rest; at 16 m visibility a 20 m wall reads as
bottomless.

The sea is one plane drawn everywhere at sea level (`vs_water`), so anything below sea level
shows a sea surface inside it. Inland water has its own level and bed, so deep water inland has
to stand above the sea: the cenote is a hill with a flooded shaft in it, and its bed is kept at
least a metre above the sea.

## Reef terrain

Three chronicle events in `src/world/chronicle/events/` write the height field, the surface
material and a region name that the reef biomes match on. Caves are capsules (`fields::Tube`)
pushed onto the same list lava tubes and passages use, and carved with them.

### The fore reef profile

`coral_reef.rs` holds the profile both reef events share: `Fore`, and `fore_ground(s, along)`,
the height and zone `s` metres seaward of the crest. From the crest out:

| Zone (region) | Shape | `Fore` field (default) |
|---|---|---|
| crest (`reef_crest`) | 2 m at `crest` depth | `crest` (0.5) |
| fore reef (`fore_reef`, `reef_groove`) | slopes from the crest to `wall_top` over `width`; spurs every `spur_spacing` stand `spur_height` over sand grooves, tallest mid-slope | `width` (32), `spur_spacing` (9), `spur_height` (1.6) |
| wall (`reef_wall`) | a smoothstep from `wall_top` to `wall_foot` over `wall_run` metres | `wall_top` (5), `wall_foot` (22), `wall_run` (2.5) |
| apron (`reef_apron`) | rubble easing back to the old bed; its first 10 m count as wall | `apron` (50) |

The drop-off wanders by up to 22% of `width` along the reef, so the wall's edge is not ruled.
The foot is clamped to 1 m above the floor of the world. Grooves and the apron are sand, the rest
reef rock.

`undercuts()` cuts caves into the wall: 3 m capsule sections a little inside the face, radius
0.22 of the wall height (1.6-3.4 m), kept where a noise along the reef falls under `undercut` (the
share of the wall cut under, default 0.35). Walls under 5 m get none.

### `coral_reef`

A fringing reef along a coast. `path` is the crest; the side where the ground 40 m off is lower,
by a vote along the whole path, is the sea. Shoreward of the crest a reef flat `flat` metres
wide (22) is built up to `flat_depth` (1.2 m) only where the bed is lower, so the shore itself
is left alone. Seaward is the fore reef profile. The reef dies out over `taper` metres (40) at
each end, and cells a structure already holds are skipped. `rock`, `sand` and `poi` name the
materials and the map label.

### `atoll`

A ring of reef round a lagoon at `at`, `radius` metres out with the outline wandering by about a
fifth. The ring is `ring` metres wide (24): the crest on its outer edge, a reef flat behind it at
`flat_depth` (0.8), and the fore reef profile outside.

- `motu` (4) low sand islets stand on the ring up to `motu_height` (1.6 m) over the sea, placed
  at bearings clear of the passes; their cells take the `motu` region.
- `passes` (1) channels `pass_width` wide (14) cut through the ring to the lagoon floor and on
  out through the fore reef to the drop-off.
- The lagoon shelves over 14 m from the flat to sand `lagoon_depth` deep (7), with about
  `patches` (8) patch reefs 3-8 m across rising to about 1.4 m under the surface.
- `blue_hole` (radius, 0 for none) puts a round shaft in the lagoon, down to the floor of the
  world, with a reef lip round it and two rings of capsules that open it out below the mouth.

Caves are cut under the outer wall all the way round.

### `cenote`

A karst hill `hill` metres (28) over the ground at `at`, `hill_radius` (75) across its foot, with
a round shaft `radius` metres (15) across in its crown. The hill rises on a smoothstep with
noise lumps, and its foot wanders so it is not a turned dome. The shaft is flooded to
`below_rim` (2.5 m) under the rim, `depth` (30) metres deep unless that would bring the bed
within a metre of the sea, and has a sand floor dished 0.8 m in the middle. Below `bell_from`
(0.35) of the depth, three rings of capsules open the shaft out into a bell up to `bell` (6)
metres wider than its mouth, leaving the mouth as an overhang.

The water is inland water at the cenote's own level. Because `settle_water` rebuilds inland
water from the lakes and rivers, the cenote records its pool (`Cenotes`) and `cenote::settle`
lays it back in afterwards. Cells within the bell's reach take the `cenote_shaft` region.

### Reef biomes

| Region | Biome | What grows and swims there |
|---|---|---|
| `reef_flat` | `reef_flat` | brain, finger and staghorn coral; chromis, tangs, butterflyfish, parrotfish |
| `reef_crest` | `reef_crest` | elkhorn, brain and finger coral; tangs, parrotfish, butterflyfish |
| `fore_reef` | `fore_reef` | staghorn, tables, plates, brain and boulder coral, finger coral, sponges, fans; chromis, anthias, tangs, butterflyfish, angelfish, parrotfish, grouper, turtles |
| `fore_reef` with `strange` >= 0.25 | `glass_reef` | glass staghorn, tables and fans among ordinary coral |
| `reef_groove` | `reef_groove` | sand; rays, garden eels |
| `reef_wall` | `reef_wall` | fans, sponges, plate coral and wall plates; reef sharks, barracuda, jacks, sardine balls, mantas, jellies, whale sharks |
| `reef_apron` | `reef_apron` | rubble and sand; rays, barracuda, garden eels |
| `lagoon` | `lagoon` | sand, scattered brain and finger coral; rays, tangs, chromis, turtles |
| `patch_reef` | `patch_reef` | coral heads in the lagoon; chromis, butterflyfish, angelfish, parrotfish |
| `blue_hole` | `blue_hole` | sand in the dark; grouper, jellies |
| `motu` | `motu` | palms and palmetto; gulls, turtles |
| `cenote_shaft` | `cenote_reef` | coral in bands down the walls, vines and ferns over the rim; chromis, angelfish, grouper, jellies |

Kaldvik's `cold_coral` is not region-based: it takes steep underwater slopes 8-60 m down, with
white Lophelia, red sea fans, cod and jellies.

## Coral as growth

Corals are plant forms grown by `flora::coral` from the same limbs and blobs as trees. Limbs wear
the plant's `bark` (the skeleton), blobs its `leaf` (the living colony), and `flower`, where set,
tips the branch ends with a paler colour. `height` is how tall a colony stands, `crown_radius` how
far it spreads, `trunk_r` how thick its limbs are, `stems` how many limbs or plates.

| Form | Shape | Species |
|---|---|---|
| `coral_branch` | limbs forking up and out from one foot, each fork shorter, blunt pale tips | staghorn, violet staghorn, glass staghorn, Lophelia |
| `coral_antler` | a few thick limbs with rounded sleeve palms and a lobe at each end | elkhorn |
| `coral_table` | a short stalk and a wide flat plate, sometimes a second smaller tier | table, teal table, glass table |
| `coral_mound` | a dome on the bed, sometimes with smaller domes against it | brain, boulder |
| `coral_plate` | tilted plates in a whorl climbing toward the middle | plate coral |
| `coral_finger` | a low cushion bristling with short blunt fingers | finger coral |
| `sea_fan` | a lattice of fine branches in one upright plane; fans on a reef face roughly the same way | sea fan, red fan, glass fan |
| `sponge` | a cluster of fat upright tubes; one alone is a barrel | tube sponge, barrel sponge |

A coral needs `water_depth` and grows only where the water over the cell is within that band,
measured from the lake's or cenote's own surface where there is one. Unlike kelp, whose length
is a share of the water over it, a coral keeps its own height. Where the water is too shallow
for its tallest colony it is shrunk to fit 0.35 m under the surface (crown shrinking with it, to
half at most); where the water is shallower than three quarters of its smallest colony it is
not planted, and something lower grows instead.

`wall_plate_coral` is a `crust` with `water_depth`: it shelves underwater faces of reef walls and
cenotes the way shelf fungus grows on trunks, each patch checking its own depth.

Voxels are 0.25 m, and the mesher keeps corners near the ground square so trunks meet sloped
ground without gaps. A coral's foot is therefore blocky and only what stands higher rounds, so
species are sized tall enough to show it; the low ones (finger, small brain) stay chunky. Elkhorn
palms are rounded sleeves because a flat plate a voxel or two thick drew as a board.

## Kelp

Kelp is not voxels. A kelp plant is one invisible marker voxel on the bed (flora.rs
`Form::Kelp`: the float for a plant with a `flower`, the blade for one without), and the sprite
pass draws it as strands (mesh/sprites.rs kind `STRAND`, `kelp`). Nothing about it is solid, so
the diver, seals, sea lions and whales swim through a kelp forest.

Each holdfast grows `stems` strands. A strand's length is fixed when the chunk is meshed: its
`height` share of the water over the holdfast at high water, plus a canopy of 1.5 to 3 crown
radii. The vertex shader (`kelp_at` in foliage.wgsl) shapes it against the sea as it stands now
(`fog.y`). In water deeper than the strand it stands up through it with a slight bow at the top.
In shallower water the stipe leans out and curves into the surface, and whatever is left of its
length lies along the surface, wandering slowly. So at high water a strand trails a short canopy
on the surface, and as the tide goes out the canopy gets longer. The surge rocks the whole
strand.

A strand is a chain of stipe segments (one per 0.8 m, at most 40), its blades and, for bull kelp,
its float. Every sprite of a strand carries the holdfast as its root and the strand's length and
heading, so each can work out the same curve; word3 holds which part it is and where along the
strand. Giant kelp has a blade about every 0.9 m along the stipe and on along the canopy; bull
kelp has a bare stipe with 6 to 10 long blades streaming from the float. Under water a blade
rises along the stipe and out to its side; on the surface it lies flat and trails with the
canopy, and it is never drawn above the water. The textures (`kelp_stipe`, `kelp_blade`,
`kelp_float`) are grey and take their colour from the plant's bark, leaf and flower materials.

A chunk's culling box grows by the longest strand in it, up and to each side, so a canopy stays
drawn while the holdfast's chunk is off screen. Strands are drawn only on the finest level of
chunks, like the other herbs, and fade with the leaves. They cast no shadows.

## Fish

Reef fish use the `school` movement (`src/creatures/swim.rs`) with a `habit` in their behaviour.
A school has a home it steers toward and picks a new one every so often, at a distance set by the
habit.

| Habit | Behaviour | Species |
|---|---|---|
| `open` | the plain school: boids round a wandering home 8-28 m away | yellow tang |
| `cloud` | a tight cloud within 3 m of the coral head it was born on; startled, it drops into the coral for 3-5 s instead of fleeing | chromis, anthias |
| `hover` | one or two near a home patch; hangs still 3-8 s, cruises 4-10 s, and while still turns to face a diver within 7 m | butterflyfish, angelfish, parrotfish, grouper, cod |
| `patrol` | long passes 25-55 m, keeping to its depth band; ignores the diver | reef shark, barracuda, jack |
| `rest` | on the sand 0.12 m over the bed; every 40-90 s glides 0.6 m up to a spot 5-12 m away | stingray |
| `bait` | a ball that turns round its own middle and opens round a diver within 3.5 m | sardine, herring shoal (Kaldvik) |
| `pulse` | drifts about 0.12 m/s, lifting on each beat and sinking a little between | moon jelly |
| `burrow` | stands in a fixed hole and sinks into the sand when a diver comes within 3.5 m | garden eel |
| `cruise` | one big animal on long lines 40-80 m; ignores the diver | whale shark |

Patrol, cruise, pulse and burrow never use the startle code. A species sets its height with
`above_bed: [lo, hi]` (metres over the bed) or `depth: [lo, hi]` (metres under the surface); each
fish has a lane in that band so a school spreads up and down. Without either it swims at the old
`fish_depth`, at most 2.5 m down.

Two limits are worth knowing. Every rigged creature draws one instance per part, and
`flock::steer` compares every fish in a school with every other, so a bait ball is about 100-160
fish. And the fish read the bed from the height field (`Terrain::bed`), which is wrong under an
undercut, so fish do not know about overhangs and caves.

## Where the reefs are

**Glassfall** is a cool coast with kelp. Its reef grows in a warm plume the old volcano still
vents into the sea off its north foot: a `coral_reef` ("the warm reef") about 400 m long along
the north coast. The wall runs from 5.5 m to 22.5 m. Toward its east end, nearer the strewn field, the `strange` field passes 0.25 and `glass_reef` takes over the fore reef,
with pale, faintly lit glass corals among the ordinary ones. The kelp forest also has cod and
moon jellies.

**Jadewater** is tropical, with 30 m visibility and 12 m of sea.

- An `atoll` 144 m across in the open water to the south-west: four motu, one pass, seven patch
  reefs, a 7 m lagoon of lime sand and a wall from 4.5 m to the floor. It has no blue hole.
- "The Deep Eye", a `cenote` in the bamboo country: a hill 30 m high, a shaft 30 m across with a
  7 m bell, about 31 m of clear water over a bed a metre above the sea. It can be reached on
  foot, so it is usually the first deep dive a player finds. The existing `glow_cenote` biome is
  a different, shallow feature.
- The ordinary shallows (`jade_shallows`) have tangs, rays and jellies.

Other worlds share some of it. The `seabed` biome (Glassfall, Windbarrow, Hemlow) has moon
jellies, a sardine bait ball and jacks; Kaldvik's fjord has `cold_coral` on its deep slopes and
jellies; Lowmouth's delta bed has stingrays and barracuda.

## Currents, hunters, wrecks and the surface

Currents are the sea's thermals (`player/dive.rs sea_current`): scattered by a hash over 140 m
cells, a little under half of them hold a meandering ribbon of water 4-8 m across, 90-160 m long
and 2.5-7.5 m down, running 3-5.5 m/s, only where there is 5 m of water. A diver in one is
carried (it adds to the swim's target velocity), fish ride it at 85%, pale `current_streak`
particles flow along it near the diver, and the HUD says CURRENT. `Dive::currents` turns them
off for the movement tests, whose test sea happens to hold one.

Hunters are species with `hunts` (barracuda, jack, reef shark, grouper, cod). One rests 12-30 s,
then rushes the nearest school of something it eats within 35 m for 6-11 s
(`School::chase`). Prey within 7 m frights as it does from the diver, each prey fish within
4 m darts away (a bait ball splits round the jack), and a rushing hunter that comes within half
a metre of a fish takes it. The swim stats count `chases` and `caught`.

Wrecks: `base: "bed"` sets a structure on the lowest bed under it, half a metre into the sand
(`chronicle/ctx.rs place`). Anything with an `age` that sits under the water crusts over in the
decay pass (`gen.rs draw_structures`): reef limestone in broad patches and one coral colour per
wreck on the thickest of it (a colour per block of voxels read as confetti). Entries
`sunken_hull`, `sunken_ribs`, `jar_spill`; Glassfall has the surveyor's launch on the reef and a
trading ship below the wall, Jadewater a junk and its spilled celadon jars below the atoll. They
lie level: no tilt to the slope and no breaking over a ledge.

The surface: `surface_froth` (scene.wgsl) draws foam lines drawn out downwind and broken into
flecks, more in wind; from below they are pale patches, and the mirror past Snell's window is
banded by the waves. `wave_normal` has a broad swell layer, and the calm style keeps 0.7 of the
ripple slope, not 0.5 (Kaldvik had read as glass). The underwater sky is the haze colour, so
land culled past the fog no longer shows as dark blocks.

Reef life is about 3.5 times what it was, with `reef_manta` (common where the open-sea manta is
rare), and fish now turn up 12-50 m from the player instead of 20-90. Kelp forests have
rockfish, kelp perch, sea otters (`sea_otter`, a floating form of the otter) and seabirds with
`dive_depth` that go under for 12-22 s while they sit on the water.

## Testing it headless

GUIDE.md "Testing it headless" has the camera and spawn switches. The dive tests in
`player/dive.rs` cover diving from the surface, hanging still, running out of breath, the
shallows refusal, kick chaining and the leap; `ride/kayak.rs` tests the kayak left moored and
climbed back into.
