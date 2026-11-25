# How creatures and plants are defined

How a creature gets its shape, colours and behaviour, how variants such as the pale hare
relate to their originals, how individuals differ, and how it all ends up in a world. The
creature code is in `src/creatures/`; [ANIMATION.md](ANIMATION.md) covers rigs and motion.

## Overview

Every creature and plant is one JSON file in `library/`. A file can name a parent with
`"like"` and then give only what differs: the pale hare is the rabbit with lilac-white fur,
14% larger and calmer, in about ten lines. Some children are rare morphs (`"rarity"`): they
appear in place of the parent wherever the parent spawns, like the white one-horned deer, one
in a hundred deer. Within a species no two individuals are identical: each rolls its own size,
coat shade, speed, wariness and taste for each activity from the species' `vary` spreads.

## A creature entry

`library/creatures/<name>.json`, loaded into `CreatureDef` (`src/library/creature.rs`) after its `like`
chain is resolved (`src/inherit.rs`):

| Field | What it does |
|---|---|
| `name`, `version`, `introduced_by` | Identity. `introduced_by` is the world that added it (`base` for the original island). Entries are not edited once a world uses them; a change is a new version. |
| `like` | The parent this entry starts from (below). |
| `rarity` | Above 0 (and with `like`): a rare morph. That fraction of the parent's spawns are this creature instead, in any biome where the parent lives. |
| `scale` | Multiplies the parent's (or its own) `voxel`, `half_w`, `height` and `fly_height`. |
| `voxel` | Edge of one model voxel in metres. The shape is drawn on a 32^3 grid, so this one number scales the whole animal. |
| `grid` | The grid's size in voxels along x, y and z, 32 to 96 each (default `[32, 32, 32]`). For the big animals: a long-bodied one wastes most of a cube, so a sauropod gets `[32, 56, 92]` at 0.25 m and keeps a head you can see. A grid bigger than 32^3 needs a rig, because only its parts are drawn: each part is meshed in a 32^3 window of its own (a mesh's positions are packed in five bits), so every bone's voxels must fit in 32 a side, and `--validate` names any part that does not (split it into more bones). |
| `half_w`, `height` | The collision box, in metres. |
| `colors` | Colour name -> sRGB, with an optional fourth value for glow (the glassmoth's wings use 1.8). A fourth value below 0 is iridescence instead: the colour is dusky until the sun's reflection off it lines up with the eye, then flashes, brighter than lit paint (`entity_colour` in scene.wgsl); an Anna's hummingbird's gorget uses -1.6. |
| `ops` | The shape program: `box`, `ellipsoid`, `cylinder`, `log`, `disc`, `set`, `walls`, `gable`, `repeat`, `recolour_below` (bellies and socks) and `recolour` (a colour swapped inside a box: zebra stripes, a javelina's collar, a donkey's shoulder cross). Later ops overwrite earlier ones; `mat: "air"` carves. The model faces -Z and its pivot is the bottom centre. The same op language builds structures (`src/world/shape.rs`). |
| `extra_ops` | Ops added after the parent's: longer ears, a mane, a horn, or `air` to carve something away. |
| `behaviour` | `movement` (a Rust verb, below), `walk_speed`, `flee_speed`, `wary` (flee distance: how far out it minds you or a predator), `flightiness` and `juvenile_flightiness` (how readily it actually runs, below), `flees_from` (creature names or `"player"`), `hunts` (both take in the named creature's `like` children at any depth, so hunting rabbits means jackrabbits too), `fly_height`, `group` (family groups for walkers, flock and school sizes and boid weights), `leap` (fish: leaps per fish per minute; whales: the chance a dive ends in a breach), `floats` (seabirds that come down and float on the sea), `plunge` (floating seabirds that come down by plunging, gannets), `swims` (walkers: cross rivers and lakes too deep to wade by swimming, backs at the waterline and carried by the current; deer, bear, boar and horse and their children), `warns` (a snake that holds its ground and buzzes rather than leave), `active` and `off_hours` (the hours it keeps, below), `rain`, `snow`, `heat` and `wind` (the weather it keeps out of, below), `winter` (the share of it about in winter, below), `flight` and `wingbeat` (rigged birds, see ANIMATION.md) and `spawn_chance` (below 1 makes a creature rare: the chance, each half-second check while its biome wants one, that a missing one turns up). |
| `vary` | How much individuals differ (below). |
| `ages` | Which of the four ages its individuals come in (below). |
| `rig` | Optional bones for animation, with ops tagged `part` naming their bone ([ANIMATION.md](ANIMATION.md)). |
| `notes` | Free text for the reader. |

The library follows one rule: nouns are data, verbs are Rust. A creature names its movement;
the verbs are listed once, in `VERBS` in `src/creatures/locomotion.rs`, which says which mover
runs each verb and with what settings. The movers are in `src/creatures/`:

| `movement` | Code | Used by |
|---|---|---|
| `graze` | `walk.rs`: wander, pause, flee | deer, elk, mule deer, bighorn, serow, pronghorn, marmot, boar, javelina, horse, donkey, zebra, fae horse, bear, panda |
| `hop` | same, with a hop impulse on every step | rabbit, pale hare, jackrabbit |
| `skulk` | same; hunts what it lists in `hunts`, noticing it within 14 m or eight times its own height if that is more, and taking it within 1 m or 0.6 of its height (so a tyrannosaur sees a hadrosaur 32 m off and takes it at body length) | fox, silver fox, coyote, lizard, tyrannosaurus, the raptors, compsognathus |
| `wade` | same, steering back toward river or coast. A wader whose rig has wings and whose `fly_height` is above 0 takes off instead of running when it bolts (below): it flies 25 to 50 m on slow beats (`wingbeat`) to open ground by the water and lands (`Air` in `walk.rs`) | heron, sandhill crane, faerie heron (fly); capybara (walks) |
| `haulout` | same, close to the sea; lies out on rock or sand, and when it bolts (below) humps to the water and dives with a splash, swimming on in the water form its entry's `in_water` block describes. Seals at sea climb back out onto a low bank about once a minute | harbour seal, sea lion |
| `plod` | same, slow; instead of running it stops and draws head, legs and tail into its shell while the player is within `wary`, and comes out slowly once you leave (`Pose.tuck`, `motors::retract`; legs that stick out sideways fold back under the rim) | tortoise, desert tortoise, moss tortoise |
| `bask` | `plod` on a beach, plus: about once every two and a half minutes (when not hiding) it crawls back to the sea at `flee_speed` and swims off in its `in_water` form; swimmers haul out again like seals. Swimmers with legs beat their front flippers and trail the hind pair | sea turtle |
| `burrow` | same, kept to a home: a structure tagged `burrow` within 18 m of where its group spawns (each member takes its own, nearest first; `burrow_mound` is one), else the spot it was first stepped at, from which it never wanders more than 7 m. When it bolts it runs back there and goes underground, staying down 8 to 20 s and longer while the danger is still near, then comes up at the mouth (`creatures/walk.rs`, `Critter.home`/`under`). Put it in a biome that scatters `burrow_mound` so its home is among mounds | prairie dog |
| `still` | same, never walks | (none) |
| `circle` | `circle.rs`: a fixed circle 8 to 22 m across at `fly_height`; with `floats`, now and then it glides down to deep water under its circle, floats 15 to 45 s and goes back up (sooner if you come close); with `plunge` too it comes down by folding its wings and dropping straight in (at least 2.5 m of water under it), goes under for a few seconds, floats, and climbs back to do it again within half a minute | gull, puffin, cormorant (float); gannet (plunge); crow |
| `flutter` | `circle.rs` with a 1.5 to 4.5 m circle, slower | glassmoth, lantern moth |
| `paddle` | `fowl.rs`: float on open water 0.6 to 8 m deep near the shore, paddle slowly; now and then walk out onto a low bank to graze and back in; flush together when one is inside the flushing radius (below) and fly low to water 40 to 90 m off, in a V behind the first to take off (followers join their place over the first quarter of the flight). Water is the sea or a river at least a metre from its banks: each bird keeps the level of its water and floats on the river's voxel tops, 0.32 m down where the smoothed water is drawn. On land the legs stride and the body waddles (`Flight.walk`) | goose, mallard, swan |
| `cling` | `woodpecker.rs`: cling upright to a trunk (any placed plant 4 m+ with a trunk), drum in bursts, hitch up and round it, bound to another trunk 8 to 35 m away; farther when startled | woodpecker, pileated, northern flicker, acorn woodpecker |
| `buzz` | `insects.rs`: hover 1 to 4 s at a flower (plants that afford `nectar`) within 14 m of home, then the next; no rig, no animation | bee, bumblebee, glow bee |
| `dart` | `insects.rs`: hover 0.5 to 3 s 0.4 to 1.6 m over water or the bank, then a quick straight dart | dragonfly, meadowhawk, glass dragonfly |
| `undulate` | `circle.rs` with a 7 to 16 m circle, slow; a rig with plan `ray` ripples its wings (`motors::undulate`) | sky ray, kite ray |
| `flock` | `flock.rs` boids: wander, land, perch, flush | starling |
| `soar` | `flock.rs`, a flock of one in wide slow circles. Now and then it comes down on the top of the tallest of a few trees 14 m or more that afford `perch` within 160 m (a Sitka spruce, a drowned snag) and sits 45 s to two minutes. With `hunts`, when hungry it stoops at twice its top speed on the nearest thing it eats below it in the open (in the air, on the ground or the sea, or a fish within 1.3 m of the surface), leading it; at the strike it takes a fish half the time, a bird sitting on the water, the ground or a perch 25%, a flying one 30%. A kill hangs in its talons (`carry::Talons`) for a minute, taken to a tree top if there is one, and is then dropped; it does not hunt again for three minutes. `VG_CATCH_M` prints each stoop, perch and kill | eagle (hunts gulls, herring, mallards, puffins, trout and rabbits), condor |
| `glide` | `flock.rs`, a few big seabirds circling wide (60 m) on held wings like vultures, wandering far; seldom (one time in ten) they come down together on the highest dry ground of ten looks round them (a ridge, a rock, a bone, never a tree crown) and sit there half a minute to a minute and a half | albatross |
| `flit` | `flit.rs`: sit on a shrub or tree top (about 3 m preferred), hop in a short bounding arc to another top 3-14 m away, sometimes drop to the ground to peck, burst 12-24 m away from the player | sparrow, dusk wren |
| `school` | `swim.rs`: boids (the starling rules) in three dimensions under the surface, in sea at least 2 m deep or the deeper middle of a river or lake (a school listed by a river biome spawns in the deepest water within 30 m); scatter from a player in or beside the water; leap clear now and then with a splash. A swimmer 0.6 m tall or more (a seal or walrus at sea, a shark) keeps three quarters of its height under the surface and tilts at most 0.3 rad up or down; a fish tilts up to 1 rad. A rig with plan `ray` ripples its wings instead of wagging a tail | herring, trout, manta ray |
| `slither` | `snake.rs`: a snake on the ground. Crawls, lies coiled, or stretches out to bask; the body is posed along a path and let down onto what it crosses ([ANIMATION.md](ANIMATION.md)). With `warns` it coils, lifts its head and buzzes its tail while anything presses on it instead of leaving | rattlesnake (warns), garter snake |
| `climb` | `snake.rs`: the same snake in trees. Crosses the ground to a trunk, climbs it in a slow spiral, lies along a limb with its front hanging off and swaying, and lets go and falls limp if you stand right under it | gopher snake, green tree snake |
| `swim` | `snake.rs`: the same snake in water. At the surface with its head out and a long wave down its body, ducking under when startled, and hauling out on a bank to lie in the sun | water snake, anaconda |
| `creep` | `octopus.rs`: on the sea floor in at least `shallows` of water (default 2 m). Lies with its arms let down over whatever is under it, creeps every half minute or so to the highest of a few spots 3 to 10 m off, pales toward the sand as you come within two and a half times `wary`, and when pressed jets off mantle first at `flee_speed` with its arms streaming behind ([ANIMATION.md](ANIMATION.md), "Octopuses") | giant octopus |
| `breach` | `swim.rs`: alone offshore in water at least 14 m deep; dives 25 to 50 s, lies at the surface spouting every 6 to 11 s, and some dives end by going deep and driving up out of the water, twisting onto its side and falling back in a big splash | humpback |
| `scurry` | `runner.rs`: a flock on a beach at the edge of the surf. Each bird reads where the swash will be half a second from now (`Terrain::water_edge`: the tide plus `Shore::swash_at`, the same sheet of water `swash` in scene.wgsl draws) along the line down the beach through it, and runs for a spot 0.15 to 1 m up the sand from it, so the flock runs up ahead of each wave and chases the water back down as it drains, at `walk_speed`, or 2.6 times that with water at its feet. It drifts along the beach with its band, keeps `group.spacing` from its neighbours and its legs go in a blur (`Flight::stride_m`). Flushed (or about every two and a half minutes for nothing) the band flies low together, at `fly_height`, out over the surf and back in 25 to 70 m along the beach, each bird keeping its place in the formation. A run blocked by a bank or a log is fluttered over | sanderling |
| `patter` | `runner.rs`: run 1 to 5 m, stop dead, peck, run again, on dry open ground out of the swash; a loose band that flushes together and lands 14 to 40 m off | snowy plover, killdeer, lapwing |
| `covey` | `runner.rs`: a band of quail feeding close together in run-stop-peck (never more than 8 m from where the band settled), one now and then up on a bush top keeping watch. While you are inside its flee distance the whole covey runs off on foot at 3.5 times `walk_speed`; only inside two fifths of that distance does it burst up all at once on whirring wings (`flap_glide`), scatter low and come down 20 to 45 m off | Gambel's quail |
| `dash` | `runner.rs`: alone; trots 4 to 20 m at a time across open ground, sometimes at a sprint, now and then hops up onto a rock or bush top 0.5 to 2.5 m high to look round. With `hunts` it runs down what it eats at `flee_speed` and carries it off in its bill (`carry::Beak`) for half a minute, then is full for three. Startled it mostly sprints off on foot; three times in ten it flies a short way low instead | roadrunner (hunts lizards) |
| `hum` | `hummer.rs`: hummingbirds. Sits on a bare top 1 to 5 m up, darts out to work a run of three to eight flowers, hovering a second or so at each facing in and backing out at the end, and goes back. Flowers are plants that afford `nectar` and are in bloom (`flower_seasons`, so an ocotillo only in spring; `Flora::blooming`) and the drifts of meadow wildflowers (`sprites::bloom_at`). One time in five, leaving its perch, it climbs 18 to 28 m and dives in a narrow U past the perch and up the other side, two to five times, the way a male Anna's courts. Hovering, the body hangs tilted up (`Flight::upright`) | Anna's hummingbird |

Every walker (`graze`, `hop`, `skulk`, `wade` and the shelled and seal verbs) steers the same
way. Each step it tests the way ahead (`walkable` in `walk.rs`, 1.2 to 3.5 m depending on
speed) for trunks, walls, cliffs, low branches at head height, shrub crowns, drops over 1.6 m
and water. If the way is blocked it takes the nearest clear heading, trying first the side it
last went round, so it follows a wall instead of dithering at it. The test is wider than the
collision box (35% of the height each side) because a deer's head and rump reach past its
square box. If no heading is clear it stops and turns round. It jumps only onto a ledge with a
metre of room above it. Walkers spawn only on open ground near the terrain height, never on a
crown or a roof. Headless `--stats` reports `creature_pathing`: steps spent moving, pushing
against something (`blocked`), jumping, standing on foliage, and turning aside.

Waders (`wade`) spawn only within 6 m of a river or the shore, may step into water about knee
deep, head back to the water when they stray more than 3 m from it, and set off for it if they
stand idle more than 8 m away. These ground and water tests are methods of `Terrain`
(`src/creatures/terrain.rs`), one per meaning, shared by every mover. Birds never come down on
water: flocks and sparrows treat river cells, cells within 1.5 m of a river and ground under
half a metre above the sea as wet (the smoothed ground mesh is drawn up to 0.3 m below the
voxel tops, so a bird just above the waterline looks as if it is standing in the water).
`creature_pathing` also counts bird-steps sitting on wet ground (`birds_on_water`), heron time
within 4 m of water, and gull time floating.

Walkers also drink (activity `drink`): when the activity scheduler picks it and a river or
shore is within 35 m, the walker heads for it, stops 2.5 m from the edge and drinks with its
head down (`creature_pathing.drinking_steps`). The head goes down to the water's surface, not
to the bed: `life::Input::water` carries the surface where the animal stands, and the head-down
pose is scaled by how far that is above its feet, so one standing in the shallows dips its mouth
in and one out of its depth doesn't reach down at all. Drinking follows the clock (`Entities.hour`):
animals keep every planned drink in the two and a half hours after dawn and before dusk and
about a third of them at midday, and near dawn and dusk a rest or graze is now and then a drink
instead.

Fish and whales have no physics body: water has no voxels (the sea is a plane at sea level), so
`swim.rs` keeps them between the bed and the surface itself and treats the world's edge as dry
land. Their rigs (`fish`, `cetacean`) are posed by `swim.rs` directly: the body pitches with the
climb and the tail beats (side to side for fish, up and down for the whale). Spouts and splashes
are particles: `swim.rs` pushes a `fx::Cue` (`fish_splash`, `whale_blow`, `big_splash`, `splash`)
onto `Entities::cues` and `Game` throws it into the pool (GUIDE.md "Particles"). Headless
`--stats` reports `swimmers` (fish and whales alive, leaps, spouts, breaches) and
`creature_census` (every creature alive, by species).

## The hours they keep

Every creature names the part of the day it is out in, and the clock thins the world
accordingly (`src/creatures/diel.rs`). `behaviour.active` is one of:

| `active` | Out when | Who |
|---|---|---|
| `diurnal` | daylight | the default, and most of the library: starlings, crows, squirrels, bees, cranes, zebra |
| `nocturnal` | after dark | owls, nightjars, bats, badgers, raccoons, skunks, hedgehogs, kangaroo rats, spotted hyenas, frogs and toads, crocodiles, hippo, jaguar, lion, the water snake, and the lights: fireflies, the marsh light in both its coats (the wisp and the corpse candle) |
| `crepuscular` | dawn and dusk, half out by day | deer and their children, rabbits and hares, fox, coyote, wolf, cougar and the big cats, bear, boar, otter, elephant, rattlesnake, desert tortoise, the dusk wren |
| `any` | no hours of its own | fish, rays and the whale, seals, the sea turtle, the panda |

The curves run off the sun's elevation rather than the hour, so they line up with the light the
renderer draws: full daylight once the sun is up, full dark once it is 9 degrees down, and the
crossover is twilight, where the diurnal and the nocturnal are both half out. A crepuscular
animal peaks at the two edges of the day and never falls to nothing, since it is about by day
as well (at 55% of its peak) and stirs a little at night (30%).

Outside its hours a species is scarce, not absent: `off_hours` is the share of it still abroad
at the wrong end of the clock, 0.08 unless the entry says otherwise. That is the dial for an
animal that keeps loose hours (bear 0.3, hippo 0.35, elephant 0.4, the geese 0.45, so a lake
still has birds on it after dark) or none at all (firefly and the marsh light 0.0, which is why
you only ever see them lit).

Two things follow from the share:

- **How many are about.** The spawner scales each biome's count by it and culls the surplus
  furthest from the player, never within 45 m, so the fireflies come up over the next field at
  dusk rather than in your face. Only species a biome round you still lists are thinned; one
  left behind by walking out of its biome wanders off as it always has.
- **What they are doing.** An animal that sleeps where it stands -- any walker, and snakes --
  mostly stays rather than leaving: 35% of its quota is there whatever the hour, and up to 85%
  of what the scheduler plans for it becomes a long rest instead (`BEDS_DOWN` in `walk.rs`), so
  a meadow at midnight is deer bedded in the grass rather than deer grazing. That share goes by
  the square of how far outside its hours it is, so a deer half asleep at midday beds down now
  and then (17% of its plans) while one at midnight is down nearly always. Birds, insects and
  fish keep no such floor: they simply are not out. A sleeping animal also notices you later
  and is slower to run (`Nerve::dozing`: 40% off its flee distance, 35% off its flightiness),
  which is how you walk up on bedded deer at night -- and it scrambles to its feet as the alarm
  rises, so getting close is not the same as cornering it.

`--catalog` prints each creature's period and its off-hours share, and `--tour SECONDS` with
`--hour H` counts what actually turned up at that hour, which is how the numbers above were
checked.

## The weather they keep out of

The sky thins the world the same way the clock does (`src/world/weather.rs`; GUIDE.md,
"Weather"). Each species has a taste for it: the share of itself still abroad in a downpour, in
falling snow, in the full heat of the day, and in a gale. The default follows from the way it
moves, so most entries need nothing:

| Moves by | In rain | In snow | In heat | In a gale |
|---|---|---|---|---|
| walking or slithering | 0.6 | 0.55 | 0.65 | 0.9 |
| flying (flock, flit, circle, paddle, cling) | 0.15 | 0.25 | 0.8 | 0.3 |
| hovering (insects) | 0.04 | 0.0 | 1.0 | 0.15 |
| swimming (school, breach) | 1 | 1 | 1 | 1 |

A bird's living is made in the air and a fish's is not, which is what the way it moves says: a
wet afternoon empties the sky and leaves the meadow half full, and a gale alone grounds the
birds with no rain falling at all. `behaviour.rain`, `snow`, `heat` and `wind` each override
one, 0 (gone) to 1 (doesn't notice it) -- `heat: 1` for a reptile that comes out in it, `wind:
1` for a seabird, `rain: 1` for a duck.

The weather's share multiplies the clock's, so a swift in the rain at midnight is as scarce as
both make it. What sleeps where it stands keeps its floor: bad weather thins it no further than
the 35% the wrong hour already leaves lying about, because a bedded animal is still lying there
in the rain. And weather beds an animal down the way the wrong hour does -- a downpour or
driving snow reads as up to 0.85 of drowsy whatever the clock says -- so grazers go under the
trees in the middle of a wet afternoon.

## The year they are out in

One more share again, from the season (`src/world/season.rs`, and GUIDE.md "Seasons"). The year
thins a species the way the hour and the weather do, and for the same reasons: what is not
about in winter has either left or gone to sleep.

| Moves by | In winter |
|---|---|
| walking or slithering | 0.7 |
| flying | 0.5 |
| hovering (insects) | 0.1 |
| swimming | 0.9 |

Spring and autumn lie between that and summer, which is all of it. `behaviour.winter` overrides
the default, 0 (sleeps or leaves for the winter) to 1 (about all year): `winter: 0` for a bear
or a marmot, `winter: 1` for a raven or a wolf, `winter: 0.1` for a swallow. Between winter and
summer the year walks from the number stated to all of it, so the one number covers hibernating
and migrating both, and it does nothing at all in summer.

## Inheritance

A child starts from its parent's resolved entry (so chains work: a child of a child) and then:

- any key it gives replaces the parent's;
- `colors`, `behaviour`, `in_water` and `vary` merge key by key, so a child names only the
  colours or behaviour fields it changes;
- `extra_ops` is appended to the parent's ops;
- `scale` multiplies the parent's sizes, unless the child gives a size itself;
- `name`, `version`, `introduced_by`, `notes` and `rarity` are never inherited, and nor are
  `morphs`: a morph's colours are absolute, so a parent's coats would paint a child in the
  parent's colours. A child that comes in coats says so itself;
- `behaviour.voice` is never inherited either, because `like` is mostly about shape: a crow is
  built on the starling and doesn't sing. A child that calls says so itself;
- the rig is inherited, so a child animates like its parent for free. Extra ops tagged with a
  bone's `part` move with that bone (the elk's mane moves with the neck, the white deer's horn
  with the head).

The people are one family: `humanoid` (18.5 mm voxels, 95 tall on a 40x96x40 grid, the
18-bone biped rig) is a plain figure; `bot` and `zombie` are its two classes, and each has
three builds that give their own `ops` (`bot_courier`, `bot_hardsuit`, `bot_walker`;
`zombie_office`, `zombie_hiker`, `zombie_labourer`). A build may give its own `rig` to move
joints, but keeps the bones' names, roles and order, because the shooter poses every bot on
one rig and drops corpses on it. Bot builds share `bot`'s colours and add none (they share one
palette block, `brain/avatar.rs`). No biome lists any of them: bots are drawn by the shooter
and the races, and zombies wait for a zombie mode.

Because the child overrides colour values by name, most variants need no new colour names: the
pale hare sets `"rabbit"` to lilac-white and every op that was brown fur turns pale.

Cycles and unknown parents are load errors (`--validate` reports them). The merge works on the
raw JSON before it becomes a `CreatureDef` or `PlantDef`, so it doesn't need to know their
fields.

## Land and water

An animal that crosses between the two gives a second behaviour in `in_water`, saying only what
changes once it is swimming:

```json
"behaviour": { "movement": "haulout", "walk_speed": 0.17, "wary": 12, "flightiness": 0.2 },
"in_water": { "movement": "school", "walk_speed": 0.35, "flee_speed": 1.3, "wary": 9,
              "awash": true, "shallows": 0.6 }
```

The block is laid over the creature's own `behaviour`, so its temperament carries over (how
readily it bolts, what it runs from, how jumpy its young are) while its locomotion changes.
`in_water` must move with `school`, which is the only mover that wants water under it.

At load the engine builds a second form of the species from it, named `<name>@water`
(`worldload::water_forms`). The two forms share one model, one set of colours, one `vary` and
one place in a biome's quota, so this is one animal with one name:

- a biome lists `alligator` once, and where an individual lands decides which form it is: water
  at least as deep as the water form's `shallows` gives the swimmer, anything shallower or dry
  gives the land form;
- the water form carries its own `spawn_chance`, so seals can be common on the rocks and scarce
  out on the shelf;
- `haulout` and `bask` already move an animal from the land form to the water one when it bolts
  into the sea, and seals climb back out, so the crossing works both ways;
- a rare morph inherits `in_water` with everything else and so gets a water form of its own. The
  two forms' morph lists point at each other's, which is how Old Pale can be met swimming among
  the knees or hauled out on a bank.

`--validate` rejects an `in_water` that does not school, or that repeats the land movement.

Big swimmers (any water form, or a model over 0.6 m tall) turn in arcs, their heading swinging
at most about 1.6 rad/s, and swing aside toward deeper water when they meet rock, instead of
flicking straight back as a fish does: turned like fish, seals and walrus spun on the spot by
the shore. `swim_pitch` in the `in_water` block tips the body nose-down while swimming, so the
walrus, modelled propped up on its foreflippers, lies level in the water.

Fish also spawn under a roof (a sea cave's pool, Gannet's grotto): they read the depth there
from the voxels under the surface rather than the height map, which holds the land overhead.

## The families

| Parent | Child | Kind | What differs |
|---|---|---|---|
| rabbit | pale_hare | subspecies (bloom) | lilac-white, 14% larger, slower, doesn't fear people |
| rabbit | jackrabbit | subspecies (red desert) | 30% larger, tan, long black-tipped ears, bolts at 9 m and runs 7.5 m/s |
| deer | elk | subspecies (conifer slopes, high meadows) | 30% larger, grey-brown with a dark mane, wider rack, herds of 3-7, wary at 14 m |
| deer | mule_deer | subspecies (canyons, pinyon-juniper) | tan, big ears, black-tipped tail |
| deer | mossback_deer | subspecies (faerie wood) | moss along the back, faintly glowing antlers, shyer |
| deer | unicorn | rare morph, 1% | white, antlers carved away and one horn grown, wary at 18 m; otherwise a deer |
| bighorn | serow | subspecies (karst towers) | smaller, dark, pale throat |
| herring | trout | subspecies (reservoirs, rivers, lakes) | 1.5 times the size, olive and pale, groups of 3-6, leaps more |
| bear | moss_bear | subspecies (faerie wood and park) | 15% larger and slower, olive-brown under a moss mantle with glowing pale caps, barely minds people |
| bear | panda | subspecies (bamboo) | black and white, slow, not rare, barely minds people |
| rattlesnake | gopher_snake | subspecies (pinyon-juniper, ponderosa, prairie) | cream with dark saddles, longer, no rattle, climbs trees |
| gopher_snake | green_tree_snake | subspecies (karst jungle, bamboo) | plain leaf-green, slimmer, lives in the canopy |
| rattlesnake | water_snake | subspecies (cypress swamp, sawgrass) | dark olive, no rattle, swims at the surface |
| water_snake | anaconda | subspecies (jade shallows, cenote) | nearly three times the size, darker, rare (`spawn_chance` 0.15) |
| rattlesnake | garter_snake | subspecies (meadows, forest, river banks) | half the size, olive, quick to leave, no rattle |

The black bear and the humpback are rare through `spawn_chance` (0.00133 and 0.0008 per
half-second check while a biome round you wants one): on paper a bear every 6 minutes in its
biome and a whale every 10 near deep water, and either stays until it is 130 m (bear) or 260 m
(whale) away and out of view. In practice a bear is rarer than the paper rate: half an hour of `--tour 1800`
walking a loop through Glassfall's interior, forests included, met no bear, while it met 316
deer and 5 unicorns (1.6%, set to 1%). Rare species of swimmers are rolled per fish or per
whale; coats are not (see Coats).

Walkers and water: an ordinary walker keeps out of river or lake water deeper than 0.3 m, a
wader out of more than 0.6 m (`walk::walkable`); one that `swims` goes in at any depth. Afloat,
it is held with its back at the waterline (80% of its height under), slowed to a paddle and
carried by the current (`swimming_steps` in the pathing stats). Nothing spawns in the water
except a wader in the shallows.

The starling is the base of every perching bird. Its body, head, beak, wing, tail and legs each
have their own colour name, so a child recolours any of them; it also carries the bird rig
(two-segment wings, tail, head, short legs).

| Parent | Children | What differs |
|---|---|---|
| starling | crow | 1.7 times the size, circles alone |
| starling | eagle | 3.6 times the size, dark body, white head, brown wings, yellow beak and feet, soars, perches on tall trees, hunts |
| eagle | condor | 1.45 times the eagle, black with a bare pink-grey head and white bars under the wings, beats at 1.2 a second |
| starling | sparrow | 0.85 times the size, brown with a pale breast and a darker crown, flits between shrubs |
| sparrow | dusk wren | smaller, jewel-coloured with a glowing crown |
| starling | puffin, cormorant | seabirds over the coast (`circle` with `floats`) |
| starling | albatross | its own model at twice the voxel, wings twice the starling shape's length, four and a half metres across, white with pale grey wings; `glide` with the `sail` wingbeat |
| starling | goose (adds a neck, `paddle`) -> mallard, swan | water and land |
| starling | woodpecker (`cling`) -> pileated woodpecker, northern flicker, acorn woodpecker | |

Other families:

| Parent | Children | Notes |
|---|---|---|
| heron (bird rig that walks; wings fold along the body on the ground, `motors::fold_wings`) | sandhill crane, faerie heron | walk and wade, fly off on huge slow wings |
| quetzalcoatlus, pteranodon (each its own model; standing at rest on folded wings, opened by `spread` angles: ANIMATION.md, Pterosaurs) | | the quetzalcoatlus grazes and takes off when it bolts; the pteranodon glides and stands where it lands |
| tortoise (plan sprawler, `plod`) | desert tortoise, moss tortoise, sea turtle (`bask`: own rounded shape, long flippers that fold under the shell) | hide in the shell |
| boar | javelina, capybara (own blunt head, no tail, `wade`) | |
| horse | donkey (long ears, shoulder cross), zebra (`recolour` stripes), fae horse (glowing mane) | |
| manta ray (plan `ray`, three-bone wing chains) | sky ray (flies, `undulate`) -> kite ray | |
| harbor seal (plan sprawler, `haulout`) | sea lion, swimming seal (`school`: the seal at sea), walrus (own shape with tusks, three metres, hardly bothers to move) | land and water |
| reef shark | sixgill shark (three times the size, dark, one dorsal far back, keeps a metre or two over the deep floor) | |
| glassmoth (no rig) | | the lantern moth is its `lantern` coat |
| bee (no rig, `buzz`) | bumblebee, glow bee | |
| dragonfly (no rig, `dart`) | meadowhawk, glass dragonfly | |

The gull keeps its own shape and rig.

Plants:

| Parent | Child | Kind |
|---|---|---|
| birch | pale_birch | subspecies (bloom): pale bark and needles, a little smaller |
| fir | giant_fir | subspecies: 22-28 m against the fir's 12-17 m, thicker trunk and crown |
| pine | lightning_pine | rare morph, 2%: a dead pine burnt black, standing among the living |
| hanging_vine | blossom_vine | subspecies (faerie): longer strands with rose-lilac blossom, trees only |

Vines are the plant form `vine` (`flora::place_vines`). A vine has no ground position of its own:
after every other plant is placed, it looks for hosts of the kinds in its `hosts` list ("tree",
"cliff", "wall"; empty means all three) in the biome:

- Trees: each broadleaf tree carries one with the biome entry's `density` as the chance. 8 to 19
  strands hang from round the crown's edge and underside, and swags are slung to one to three of
  its four nearest broadleaf neighbours: a strand from a low point on one crown's rim to the
  facing rim of the other, 1.5 to 8 m apart, sagging a fifth to a third of the span in a catenary
  (kept 2.2 m clear of the ground), often with a second strand alongside, and short ends dangling
  from its lower part. Each pair of trees is swagged once.
- Cliffs: each 3 m cell along a cliff top (a drop of 3 m or more) carries one with `density` as
  the chance: a curtain `crown_radius` wide falls down the face in metre lengths, each set just
  clear of the rock.
- Walls: each placed structure whose centre is in the biome (not city buildings, whose ivy is
  `city.rs`'s own) carries vines with chance `share * min(1, 0.5 + 4 * density)`, where `share`
  (`flora::wall_vine_share`) is the structure's `vines` key if it sets one, else 1 for a "ruin",
  0 for anything that doesn't decay (rock, arches, hoodoos, the glass spire, rope bridges), and
  0.25 for a kept-up building rising to 1 as it decays. Its walls are cut into patches
  `crown_radius` wide, each covered with chance `0.25 + 0.6 * share`, with a strand every half
  metre or so. A strand hangs from the top of what still stands (the same test as the drawing,
  `gen::still_standing`: decay, then gravity) or, two in five, climbs from the ground; either way it comes down in
  metre lengths, each in the air just outside the wall's face at that height, so it follows
  setbacks, eaves and tapers. Structures standing in the sea are skipped.

`height` is the strand length. Strands are one voxel thick with small leaf clusters
(`library_rule`: names with VINE), bow out to one side as they fall (a sixteenth to a sixth of
their length, at most 1 m, so the short lengths down a cliff or wall stay against the face), sway
a little toward the free end, and are drawn as tree records of kind `Vine`, the swags kept in the
record's `branches` as (one end, the other, sag). Their material is `passable` (a material flag):
drawn like any voxel, but physics never collides with it, so the player walks through a curtain.

Branching trees (`form: branching`) start their limbs at 0.55 of the trunk radius, so a limb is
never thicker than the trunk at the fork. `spread` (0 to 1) tips the first limbs toward the
horizontal, lengthens them by up to 30% and adds limbs, for a wide old crown; the great oak uses 1.

## Running from the player

Two numbers set a species' feeling about people (`src/creatures/startle.rs`). `wary` is the
flee distance: how far out it minds you at all. `flightiness` is how readily it bolts once you
are inside that distance -- 0 for a manta ray, which carries on as if nobody were there, 0.2 for
a crocodile, 1 for an ordinary wild animal, 1.8 for a hare. Whether a walker minds you at all is
still `flees_from: ["player"]`; flightiness grades what happens inside that.

Inside the flee distance you press on the animal at some rate per second, and it bolts once a
second's worth has built up. So walking slowly toward a deer makes it turn and watch you
(ANIMATION.md) and only break when you are close, while the same deer bolts the moment a bike
comes over the rise.

The pressure is the sum of what the animal can sense (`startle::pressure_on`), all scaled by how
close you are and what you are:

| Sense | What it is |
|---|---|
| sight | your movement: nothing while you hold still, about 0.2 walking in, 0.8 running straight at it. A good deal less from behind (0.4 of it), and a quarter as much while its head is down grazing or drinking (`anim/life.rs` `graze_w`). Cover (0.45) where a trunk, boulder, shrub or wall stands between you, looked for only within 6 m, so it costs one short raycast for the few animals at close range (`creature_pathing.cover_rays` counts them) |
| noise | your footsteps (`Threat::noise`): silent standing, 0.5 at a walk on ordinary ground, more at a run. The ground scales it (`creatures/sign.rs` `Tread`): dry leaf litter 1.8, gravel 1.6, grass 0.8, sand 0.6, moss 0.35; wet litter and grass are quieter, lying snow muffles, and rain masks up to 70% of it. A bike rattles whatever the ground, a glider and a kayak are nearly silent |
| closeness | someone standing a few metres off is alarming however still they keep: 0.3 times how far inside the flee distance you are |
| scent | carried downwind of you along `world::WIND` (the way the grass leans), in a cone, out to 2.5 times the flee distance on a steady breeze. Crouching, cover and keeping still do nothing for it. Rain damps it; in the still heat there is no direction and it pools round you over a short reach |

and the whole is multiplied by:

| What | Effect |
|---|---|
| proximity | steeply (as the 1.3 power of how far inside the flee distance you are), and doubled within 2 m. Height counts, so a glider 20 m up is 20 m away even right overhead, and so is someone looking down from a cliff |
| what is coming | the player on foot 1, bike 1.4, climbing 0.55, kayak 0.65, rope 0.8, glider or wingsuit 0.35; a predator 1.6. Crouching (Ctrl) halves it and softens your step |

A pressure under 0.3 a second lets the alarm settle rather than build (`startle::QUIET`), faster
the less is pressing. That is what makes stalking work: move while it grazes, freeze when its head
comes up, and an animal that has seen you and then sees nothing more goes back to feeding. A
crocodile walked past at three metres may never go at all.

The alarm spreads. A walker that bolts is itself a threat to every walker within its flee
distance (`Threat::alarm_of`: menace 0.7, the crash of it loud), and one that runs from a bolting
neighbour runs the way the neighbour went, so a herd goes together.
`creature_pathing.alarmed_by_herd` counts the steps where that was the pressure that won.

The same model runs a prey animal's flight from the predators its `flees_from` names, so a wolf
trotting past at a distance is watched and one coming in at speed is run from. Everything runs
from a predator in the end, however little it minds people: a predator's flightiness is
`0.7 + 0.6 x` the species' own (`Nerve::hunted`), so a manta ray reads 0.7 where it reads 0 about
people, and a hare is still quicker about it than a crocodile. Cover is not asked about there --
a deer under a tree still hears the wolf -- and when both the player and a predator are pressing,
the animal runs from whichever is pressing harder.

Age moves flightiness, not distance. `juvenile_flightiness` is below 1 where the young stand and
stare (a fawn 0.55, a bear cub 0.5, an elephant calf 0.6) and above 1 where they are jumpier than
their parents (a piglet 1.25, a young crocodile 1.4); the aged are always 12% warier than their
prime. Both numbers follow `like`, so a subspecies inherits its parent's nerve unless it says
otherwise.

Flocks, schools and insect swarms flee as one and have no per-individual alarm to build up, so
for them flightiness and your approach move the radius instead (`startle::reach`): a shy flock at
a walking pace lets you to about two thirds of its flee distance, and a bike at speed flushes it
half as far out again.

## Big bodies in a herd

A walker's collision box (`physics::Body`) is a square `half_w` across at its feet. For a deer
that is the animal; for a triceratops it is a fifth of it. So walkers are also capsules on the
flat (`creatures/crowd.rs`): a spine from rump to snout along the heading, `half_w` thick. How far
the body runs back and forward of the feet (`Model::reach`) is measured from the voxels when the
library loads, ignoring a tail tip or claw only a voxel or two across.

- Herds space by body, not feet. herd.rs measures `group.spacing` between spines, so a member
  keeps that much room along the whole of a neighbour, and spawn sets each member down clear of
  the ones already placed, trying up to ten spots.
- Nothing stands in anything. Once a step, after everyone has moved, any two walkers whose
  capsules overlap are pushed apart along the line between their nearest points, the lighter
  giving way more, and never into a wall. A shallow overlap eases out at up to 3 m/s; a deep one
  (two bolting into each other) closes half of itself each step. The player shoulders against
  the same capsule, so you cannot walk through a triceratops's head.
- Turning takes time. Anything over 1.5 m tall turns toward its heading at its own rate and walks
  the way it faces, slowing to a crawl through a sharp turn instead of pivoting or sliding: about
  1.6 rad/s at a walk for a 3 m elephant or triceratops, 0.6 for a 6 m brachiosaur, 1.6 times
  that bolting.
- Big animals walk through brush (`creatures/trample.rs`). A walker at least 2.8 m tall and
  0.75 m `half_w` (an elephant, a mammoth, the big dinosaurs; not a bison or a giraffe) does not
  steer round a shrub below 85% of its height (2.6 m at most, stems no thicker than 12 cm): the
  shrub is flattened to its bottom layer of stems as it reaches it. Each grows back four minutes
  later once nothing big is standing in it, at most 600 lie flattened at once, and none of it is
  saved. Shrubs drawn as sprites (cycads, tussocks) have no voxels and nothing to flatten.
- Their footfalls are felt. A big walker's step within `10 + 12 x weight` metres of the player
  drops and rocks the view by a centimetre or so, settling in a fifth of a second (weight is
  `sqrt(height x half_w / 2.4)`: 1 for an elephant, 2 for a brachiosaur). `VG_THUMP=k` scales it,
  0 turns it off.

## Where a herd goes

In a world with a survey (notes/WORLD_SURVEY_PLAN.md), every prey herd (a walking group whose
species hunts nothing) has a trip, worked out every 2 s in `creatures/roam.rs`:

- The need, in order: away from where it last bolted (for a minute), water at the ends of the
  day (`walk::thirst` 0.75 and up, not again for 15 minutes after drinking), bed when drowsy
  (see "Bedding down" below), else food.
- The goal is the survey anchor of that kind that scores best: what it offers (a grazer reads
  the anchor's `graze`, anything else `graze` or `browse`), less the way's cost / 600 m, less
  twice the live threat on it, with a little jitter so herds spread out. Feed is never chosen
  within 40 m.
- The way is Dijkstra over the large walker's links from the nearest anchor (within 160 m), each
  link costing its length times 1 + 4 x the threat at its middle and its far end. Its path is the
  links' paths joined, as waypoints.
- The herd is only nudged along: members standing about are set walking toward the next waypoint
  (3 in 10 a think while feeding along, 6 in 10 for anything else), and herd.rs pulls walkers
  toward it beside the boid rules. Lying members get up for anything but feeding along, 15 in
  100 a think, so a herd rises over a minute or so. A herd passes a waypoint within 14 m and
  arrives within 22 m, then stays (90 s at water, 4 to 8 minutes feeding, till it wakes in bed).
- A trip is planned again when the need changes, the stay ends, the threat on the rest of the
  way passes 0.5, or it is taking far too long. A bolt drops the trip at once. With no drink
  anchor it can reach, a herd (or pack) goes without, as if it had drunk.

The ecology map draws each herd's way in the colour of its need with a ring on the goal, the
hover box names a herd member's trip, the inspector logs each plan, and `--eco-dump` lines carry
every trip. On Kitehold at 18:18 every herd on the plain heads for its own drink anchor on the
river and closes on it (155 -> 46 m, 580 -> 466 m in 2.5 minutes, at 1x).

## Bedding down

ANIMAL_SIM C2. A herd (or pack) sleeps together at a bed it walked to, the night through:

- It goes to bed when its period makes it drowsier than 0.6 (`diel::drowsy`: nightfall for a
  day animal, daybreak for a lion), and stays until it is under 0.4, so dusk and dawn don't
  flicker. A predator full from a kill lies up the same way, where it ate.
- The bed is the best survey anchor within 200 m (not roofed; feed, refuge, focal or vantage):
  for a grazer that keeps the day's hours (horses, bison, pronghorn, vicuna), open ground with a
  long view (`open` 0.5 and up, scored by `open` and `sightline`); for the rest (deer, elk,
  predators), cover (a refuge, or `cover` 0.35 and up). Last night's bed scores 0.6 more, so a
  herd keeps a bedding ground. A pack beds on its own ground. With no bed in reach, or not there
  90 s after setting off (a night lasts about four minutes of play), it lies where it is.
- Each member carries its herd's night (`walk::Sleep`, set every think): going to bed it does not
  lie down on the way; bedded it lies down 9 times in 10 when it next chooses, for 4 to 12
  minutes, standing now and then for a stretch; otherwise it naps by its own drowsiness as before
  (25 to 95 s, drowsiness squared). A herd met at night is drawn bedded and lies down at once.
- A bolt still gets everyone up and away; the herd goes back to bed once the minute away is over.
- The spawner's night thinning (`diel::abroad`) no longer touches these herds: they are records,
  and at night they are at their beds rather than gone.

`VG_CLOCK_RUN=1` with `--hour` lets the clock run in a headless run, to watch dusk or dawn go by.

## Packs

Predators have trips too (`creatures/roam.rs`), on the same graph and the same 2 s think:

- The need, in order: away after a bolt, lying up when full, water, rest when drowsy (most
  predators keep night or dusk hours), else hunt.
- Hunting, a pack picks a quarry: the nearest herd of something it `hunts` (any herd with a mind,
  recorded or not, a pack of another hunter included: velociraptors take compsognathus) within 600 m and on its own ground. Its goal is the anchor nearest the quarry,
  within 150 m of it, and it plans again when the quarry moves further than that from the goal.
  Within 120 m it makes straight for the quarry's middle: herd.rs pulls walking members there
  (a lone hunter too), and the nudge keeps them walking. The chase and the kill are walk.rs's,
  within its 14 m hunting sight. Not drawn, a pack within 120 m shadows its quarry 25 m short
  of it, and tries for a kill (below). Out of sight a pack of two or more covers ground 40% faster
  than a lone hunter while hunting (about 1.1 m/s against 0.8).
- With no herd of its prey about (compsognathus after lizards, a fox after rabbits, which are not
  records), it prowls the feed anchors on its ground.
- Ground: a pack's home is where it was first thought about. Its ground reaches 450 m from home
  and stops where another pack of its kind's home is nearer by more than 40 m (in that band both
  hunt). A quarry that crosses off its ground is let go ("let herd ... go, it crossed off its
  ground"), and a new one is picked on its own ground.
- After a kill (a member carrying or eating), the pack lies up where it ate for 20 minutes with
  no trip, before it drinks, while the herd it took from runs and goes on; it leaves that herd
  alone for 30 minutes. Every drawn member is kept full that long (`Critter::fed`), so none hunts
  again on its own 3-minute clock, and a pack written back and drawn again is still full. A pack
  that follows one quarry for 5 minutes without a kill gives it up for 15.
- Predators route without the live threat (roam.rs `plan`, `fear` 0) and don't replan on it:
  on their ground it is mostly prey bolting from them.

`VG_ROAM_LOG=1` prints every herd's and pack's decisions, drawn or not, to stderr. The map draws
predator records as red hollow squares and hunts in red.

### Kills out of sight

ANIMAL_SIM C3 (`roster.rs` `encounters`). Every 3 s, a pack not drawn whose quarry is a herd not
drawn and whose middle is within 32 m of the herd's rolls for a kill: 2.5% for a lone hunter,
half as much again for each more in the pack, up to four (a pride of two takes about a minute
and a half of shadowing). On a kill:

- The herd loses its weakest: a young one if it has one, else an old one, else any (a herd
  never drawn has a made-up adult of its kind taken). It is shaken and runs (`Need::Away`), and
  a herd with none left is lost.
- The pack is fed: it lies up at the kill for 20 minutes and leaves that herd alone for 30.
- The kill is kept as a `KillSite` until its bones would be gone (age.rs `CARCASS_S` +
  `BONES_LAST_S`, about 13 minutes). When the player comes within 140 m it is put down as a
  carcass, a third of its meat left, or as bones if it is older than a minute; so the player
  walks up on lions lying by what is left of a pronghorn. A carcass the spawner drops far behind
  is put down again on the way back.
- Prints and trampled grass are not left yet: the marks go down only on loaded ground.

The map draws kill sites as red crosses, `--eco-dump` lines carry them under `roster.kills`
(victim, hunter, place, when, whether put down), and `VG_ROAM_LOG` prints each one.
`VG_KILL_SITE=1` lays a test kill site 20 m east of the player 3 s into a run.

## Wariness

ANIMAL_SIM C4 (`roam.rs` `trouble`). Every herd and pack mind keeps a wariness from -0.3 (used to
people) to 1 (badly spooked), carried by each member (`Critter::wary`) and saved with the herd.

- It rises when trouble comes: 0.12 for a bolt with the player within 80 m, 0.05 for any other
  bolt (a predator, the herd itself), 0.35 for a member lost while drawn (shot or taken), and
  0.15 for a kill out of sight.
- It fades by about two thirds in 1800 s of game clock (about a game day), and quiet time with
  the player within 80 m and little threat wears 0.002 off each 2 s think, down to -0.3.
- It scales every member's flee distance (walk.rs `WARY_REACH`): 1.8 times at 1, 0.76 times at
  -0.3. A herd that has been shot at runs from a long way off; one you have sat beside quietly
  lets you closer.
- Each herd remembers the last four places trouble came to it for 2400 s. Within 120 m of one,
  goals and links count as dangerous, by 0.5 plus its wariness, as live threat does, so a herd
  feeds and travels away from where it was shot at.

The hover box shows a member's herd's wariness (`WARY 0.40`), dump records carry `wary`, and
`VG_ROAM_LOG` prints each trouble.

## Numbers over days

ANIMAL_SIM C5 (`roster.rs` `grow`, every 3 s on the game clock). Each species' target is what
the biome lists seed. Only records not drawn change, so nothing is born or starves in front of
the player.

- Young: a herd has 0.35 young a day per animal, times the room left under its species' target
  (none at or over it), times 0.4 to 1 by the grazing where it stands (survey `graze`). A young
  one is a juvenile of an adult's kind and coat, sized and built when it is drawn. A pack that
  hunts herds has 0.12 a day per animal, only while it has eaten in the last 24 game hours; one
  after prey that is not kept as records (compsognathus after lizards, a fox after rabbits) has
  0.12 a day regardless.
- Hunger: a pack that hunts herds loses one each 60 game hours without a meal.
- Splitting: a herd half again its group's largest (and at least 4) splits; half goes its own way.
- A lost herd is replaced: 6 game hours after a herd is gone (killed off or starved), one of its
  kind comes in just inside the map edge nearest the lost herd's home, out of the player's sight,
  and makes for that home a trip at a time (`Need::Home`, 0.35 m/s unseen, about a game day for
  600 m), through whatever biomes lie between. It keeps going day and night, before drinking or
  bedding; only a bolt or a meal stops it. Where the links don't lead nearer, it goes straight
  across country. Within 120 m of its home it settles and the ground is its own. So each
  territory keeps its herd, and for a while a herd turns up somewhere it doesn't belong.
- A floor: a species under half its target has a herd walk in the same way, toward one of the
  species' seeded homes, at most once in 12 game hours.

`--eco-soak HOURS` runs the records alone, nothing drawn, for HOURS game hours as fast as they
compute (the D4 plan's 100x clock for Tier 0), and prints each species' animals, records and
target, and the births, kills, deaths, splits, walk-ins and replacements so far, as a JSON line
every 6 game hours.

## Herd records

Every prey herd and every predator in a world is kept as a record, drawn or not
(`creatures/roster.rs`, ANIMAL_SIM C1). Which: walkers that don't wade or burrow and either have
a `group` block of two or more and hunt nothing (deer, elk, bison, horses, pronghorn, the grazing
dinosaurs), or hunt anything. A predator whose `group` allows two or more (wolves, hyenas, lions,
the raptors, compsognathus) is one pack record; one that goes alone (cougar, tiger, fox, coyote)
is a record of one. `roster::kind` names them herd, pack or lone, and the logs, the hover box and
the dump use that name. Lone prey, burrowers and waders keep the spawner.

- Seeding: when the world is set, a 64 m grid is walked in a hashed order; each recorded species
  a cell's biome lists adds its count times the cell's area over the circle the spawner fills
  (90 m, or 180 m and half as many again for a large animal). Each time a species' total makes
  up a herd (a size rolled from its `group.size`), one is put at a dry spot a large walker can
  stand on in that cell. A pack may be one animal (a pride of one to three lions is sometimes
  one), a herd is two at the least. Kitehold gets 336 herds (1,488 animals) and 126
  predator records: 31 lion prides (62 lions), 61 compsognathus packs (419, after lizards, which
  are not records), 33 foxes and a coyote. Kitehold's lions were later halved (`kh_plain` count
  0.8) and its compsognathus by 30% (count 7.8): it now seeds 13 prides (23 lions) and 36
  compsognathus packs (231). With packs 40% faster out of sight and hunger after 60 hours, 12
  lions starved in ten game days (62 before) and pronghorn held at 319-327 of 335. The spawner no longer makes these species.
- Drawn: a herd is drawn once its middle is within 90% of its species' reach (spawn.rs `reach`):
  130 m out of view (260 m for a large animal), and in view 250 m (350 m for a large one,
  `HERD_SEEN_M`), nearer than the old far band's 500 m, since a herd much past 250 m hardly
  reads. Nearest first, up to 220 animals of recorded herds at once (`MAX_DRAWN`; about 170
  fill Kitehold's plain) and 3 herds a half second. `VG_HERD_REACH=seen,large` and
  `VG_HERD_CAP=N` override the reach and the cap for measuring. Its members carry `Recorded` and the herd's number as their
  `Kin`, so herd.rs and roam.rs steer them as any herd. The first time, the spawner's own
  `place` and `dress` make the family group.
- Written back: past 105% of its reach (or the furthest herds when over the cap), each living
  member's species, age, sex, build, coat and place in the herd go into the record and the
  animals are taken out. A carcass of the herd is left to its own clock. A herd with no member
  left is lost.
- Not drawn, a herd keeps its trip (roam.rs `unseen`, every 3 s): it thinks as a drawn herd
  does and its middle moves along the way, 0.25 m/s feeding along, 0.8 m/s to water or away.
- Drawn again, the same animals are set down round wherever the middle has got to.

Checked on Kitehold with `VG_AWAY=1300,300,40,160`: the herds round the start are written back at
40 s, move on unseen (one 601 -> 630 m east in two minutes, feeding along), and come back at 160 s
with the same sizes and the same animals (adult, juvenile, adult, adult, adult, juvenile before
and after).

The roster is saved to `herds.json` in the world's save folder when the player saves (F5, the
pause menu) and when the world closes (to the menu, to another world, quitting), not on the
half-minute autosave: drawn herds are written from their animals as they stand, the rest as kept.
Species go by name, and the file carries the world's survey key, so a world that has been rebuilt
since is seeded afresh. Trips are not kept; a loaded herd plans a new one. A fresh start, a
match and the zoo neither read nor write it. Headless: `VG_SAVE_HERDS=1` writes it at the end of
a run, `VG_LOAD_HERDS=1` reads it at the start (with `--save` naming a test folder). Checked on
Kitehold: 329 herds written after 30 s, all 329 read back at the same spots, and the 30 that had
been drawn came back with the same ages and sexes; a file with another key was seeded over.

## Large animals

A walker 2.5 m tall or more (`creatures::LARGE_M`: elephants, bison, giraffes, gallimimus and
the big dinosaurs) is a large animal; `behaviour.large` says so either way (`Species::large`).
Large animals are found, kept and set down twice as far off as the rest (`spawn::LARGE_REACH`),
so a herd can be watched from a ridge:

- The biomes are asked what they want out to 180 m for them, not 45, and each large species is
  wanted half as many again (`LARGE_QUOTA`), since its numbers are spread over four times the
  ground. Newcomers are set down 30 to 180 m out, out of view, except in the first three seconds
  of a world (`FIRST_FILL_S`), so a lookout opens with its herds in view. In view they come from
  the far band (below).
- They are dropped at 260 m out of view and 720 m in view, not 130 and 360.
- Past full detail (128 m, stream.rs `FULL_DETAIL_M`) there are no voxels to walk on, so out
  there a large animal walks the generator's ground (terrain.rs `far_ground`, which is what the
  coarser rings are drawn from), steering off drops of more than 1.6 m, deep water, tree trunks
  and the map's edge (walk.rs `far_walkable`), and the crowd push moves it freely. It steps onto
  the voxels when they load.
- They are remembered (`creatures/tracked.rs`). A large animal dropped for being far away leaves
  a record of where it stood, its species, age, coat and herd; come back within 220 m and it is
  put back there before the biomes are asked for anything new, so the same herd is waiting. A
  record the player has gone more than 600 m from is forgotten, and at most 160 are kept.

`creature_pathing` has `large_about`, `large_far` (past 128 m), `large_remembered` and
`large_restored`. Headless, `VG_AWAY=x,z,from,to` puts the camera at (x, z), facing away, between
two times in seconds and back after, for checking that a herd is still there on the return. On
Thunderfield's south edge about 80 large animals are about, a third of them past 128 m; walking
400 m away and back put 103 back where they were left, and 750 m away forgot them.

`creature_pathing` counts `jostled` (walker-steps spent pushed out of another), `trampled`
(shrubs flattened) and `thumps_felt`. `VG_CROWD=1` prints once a second how many walkers overlap,
the deepest overlap and the fastest any big one is turning; `VG_CROWD=still` measures the same
without pushing anyone, for comparison. On a brachiosaur and diplodocus herd in the Saurischia
parkland, overlaps ran to 3 m without the push and stay under 0.4 m with it.

A hunter's sight and catch reach scale with its height (`walk::hunt_sight`), so a tyrannosaur
sees prey 32 m off and takes it at 2.4 m; animals under about 1.7 m are unchanged. The collision
box is still a square `half_w` round the middle, so the player can walk through a sauropod's
neck or tail.

## Variation within a species

`vary` gives spreads, each a fraction either side of the species' value. Defaults apply to every
creature that doesn't set them, so herds never look cloned:

| Key | Default | Effect |
|---|---|---|
| `size` | 0.06 | body size (drawn scale; the collision box stays the species') |
| `shade` | 0.07 | coat brightness |
| `hue` | 0.04 | warmer (redder) or cooler (bluer) coat |
| `speed` | 0.08 | walking and running speed |
| `wary` | 0.2 | distance it notices the player or a predator |
| `habits` | 0.4 | taste for each activity: one deer grazes more, another lies down more |

Each individual rolls a `Morph` once (`src/variety.rs`) from its entity id and keeps it;
every mover rolls one, fish and whales included. Walkers, flocking and flitting birds and
swimmers move at their own speed; walkers, flitting birds and circlers notice at their own
distance; a whale's size also sets how deep it lies and where it spouts. Shade and hue reach the
shader as the instance `tint`, so variation costs no palette slots. Set a key to 0 to turn it
off (the unicorn keeps `shade` at 0.02 so it stays white).

### Coats

`vary` above is the spread round one colouring. A species that comes in several named colourings
declares them in `morphs`, and each is a **coat**:

```json
"colors": { "fox": [168, 86, 36], "white": [240, 236, 228] },
"morphs": [
  { "name": "red", "weight": 19 },
  { "name": "silver", "weight": 1, "shades": 2,
    "colors": { "fox": [[40, 38, 42], [58, 54, 58]], "white": [[238, 236, 232], [248, 246, 244]] } }
]
```

- The keys of a coat's `colors` are the entry's own colour names; a plant's are the slots
  `bark`, `leaf`, `flower`, `autumn` and `stalk`, and a plant may name another material there
  instead of a colour, as `leaf_alt` does.
- A value is one colour (`[r, g, b]`, with a fourth emissive value as elsewhere) or the two ends
  of a range (`[[r, g, b], [r, g, b]]`). An individual draws from inside its coat's range, so no
  two purple wisteria are quite alike.
- `weight` is relative, not a share: 19 and 1 give one silver fox in twenty. They are normalised
  at load (`Library::creature_coats`, `plant_coats`), and `leaf_alt` compiles into two coats so
  nothing downstream reads it.
- `shades` is how many distinct draws the coat is built from, 1 by default: a continuous draw
  where the renderer can manage one and a single colour where it cannot. A creature's coat is a
  per-instance tint and costs nothing; a plant's is a material and each shade is an atlas layer.
- A biome picks coats with `morph` on the creature or plant it lists: `"morph": "silver"` for one,
  or `"morph": {"pale": 0.5, "wild": 0.5}` to reweight. Left out, the entry's own weights stand.
- Coats are **not inherited** through `like`, since their colours are absolute: a parent's coats
  would paint a child in the parent's colours.

**A coat is not a species.** A morph carries no shape, which is the whole of the difference: a
morph that needed a horn or a plume would be a `like` child with `rarity` instead, and
`--validate` holds a morph to colour alone. So the unicorn keeps its own entry, because a horn
is a horn.

#### What a creature's coat can do

A creature's coat costs no palette slots because it is not a second set of colours: it is one
affine map, `colour * mul + add` in linear RGB, over every colour the model is drawn in, fitted
by least squares from the colours the coat declares (`variety::fit`). It rides the per-instance
tint, which is a multiply-and-add rather than a multiply -- a multiply alone can darken a coat
but not repaint it, since no multiple of a dark brown is a pale lilac.

That one map over the whole model is the constraint to write coats against:

- **Name two or more colours for a real repaint.** With one named colour nothing says how the
  rest of the model should follow, so the fit is a plain multiply. The silver fox names `fox`
  (to near-black) *and* `white` (unchanged), which is what keeps the tail tip white while the
  body goes black; naming `fox` alone would darken the tail with it.
- **A coat takes every colour one way together.** Lighter, darker, warmer, cooler, toward a
  pastel: all fine. Sending two colours different ways is not -- a blue body to red while a
  green thorax goes brown is not one tint, which is why the red meadowhawk is still a dragonfly
  of its own and not a coat.
- **A coat can recolour a glow and make it burn brighter or dimmer, but cannot light a colour
  that does not glow.** A glowing colour's light is the tinted colour times its strength in the
  palette, so the tint already recolours it; a fourth number on a colour the coat paints asks
  for a strength, and the ratio to the model's own becomes one gain on every glowing colour at
  once (`Repaint::glow`, carried to the shader in `Instance::glow`). The glassmoth's `lantern`
  coat paints the wings `[255, 210, 120, 2.4]` over its `[170, 120, 255, 1.8]`, so they turn
  gold and burn a third brighter; the marsh light's `corpse` coat turns the wisp yellow at the
  same strength. A gain cannot make something out of a zero, so the faerie wood's recolours
  that glow where their parent does not (glow_bee, dusk_wren, fae_toad, fae_horse,
  glass_dragonfly) stay species, and `--validate` reports a coat that asks it to.
- **A coat carries no size and no temperament.** The pale hare is slower, larger and unafraid of
  people, so it is a species; a coat of a rabbit would only be a paler rabbit.
- Colours the coat says nothing about follow the same map, which is usually right (a silver
  fox's nose goes to black, a white whale's eye goes pale) and sometimes not. Declare them if it
  matters, but a declared colour pulls on the fit: naming the humpback's dark eye drags the
  whole whale from white to grey.

**One coat per group.** The draw is made where a group is placed, so a herd of elk is of a piece
rather than a scatter of colours, and the individual's own `vary` spread goes over the top of it
(`spawn::dress`, `variety::Morph::wear`). A rare coat therefore turns up as a whole group, which
for a fox or a whale is one animal and for a school of trout is the whole school.

`--validate` also catches a coat too rare to ever be met, two coats the player could not tell
apart, shades spread so finely over a range that the steps cannot be seen, a coat that cannot
reach a colour it declared or the glow it asked for, a glow asked of a colour that has none, and
a coat that drives an undeclared colour past white.

`VG_COATS=1` prints what each group was dressed in and the map it got.

#### What a plant's coat can do

A plant's colour is a texture, not a tint, so a coat that paints a slot gets a **material of its
own**, generated at load from the plant's own one with the recipe's colours carried by the same
multiply: a blotched lilac repainted white stays blotched (`Library::coat_materials`). The
generated material is named `<plant>_<slot>@<coat>` -- with `.0`, `.1` and so on where the coat
asks for several shades -- and it answers to its base material's name everywhere a name is read,
so the sprite rules, the foliage and glints defaults and the wood grain all still find it.

That makes a plant's coats the opposite of a creature's in what they cost and what they can do:

- **A plant coat can do anything a material can**, including a colour nothing else on the plant
  shares, and a glow (a fourth value, as everywhere else).
- **It costs an id and a texture layer, per shade.** A world has 238 voxel ids, so `shades` is
  what to watch: wisteria's 3 + 2 + 2 is seven materials where one plain plant has one. Only the plants a
  world reaches generate anything, and `--validate` reports a world that runs out.
- A coat may name an **existing material** in a slot instead of a colour, and then nothing is
  generated: that is what `leaf_alt` always did.

**One coat per patch of ground, not per plant.** `morph_blob_m` (60 m by default) sets how wide
a patch is; the field is one jittered point per cell, so the edges are irregular rather than a
grid, and every patch draws its own coat and shade (`flora::blob`, `flora::coat_at`). A cluster
of wisteria is one colour and the next cliff along is another; a downland's tussocks lie in
drifts of tawny, silver and sage about 35 m across instead of speckling three greens over each
other. The draw happens where the plant is placed, so the biome's `morph` has its say.

Wisteria's racemes are painted pale and tinted by the strand they hang from, with the
shoulder-to-tip ramp kept as brightness, so each coat flowers in its own colour.

#### The ground's own colour field

A coat is one plant or one animal. The broad colour fields across the land -- the drift a meadow
makes from sage to dry ochre over tens of metres, the patches of drier and lusher growth in the
classic style -- are a material's. By default every material in a `tint` class shares one pair
of ends; a material can declare its own with `drift`:

```json
"tint": "meadow",
"drift": {
  "calm":    [[138, 142, 112], [166, 152, 110]],
  "classic": [[116, 122, 96],  [142, 128, 92]]
}
```

- Two colours, the ends the material drifts between; a bare pair (`"drift": [[..], [..]]`)
  serves both styles, and a style left out keeps the ramp its `tint` class gives it.
- They are declared as **colours** and shipped as the multiply that takes the material's own
  texture to each end -- against the top-face recipe where it has one, and the calm recipe's own
  colours in the calm style -- so an author writes the colours they want to see rather than a
  multiplier against a pattern they cannot picture. The multiplies ride rows 256.. of the
  material buffer, three bytes a colour over a range of 0 to 2.
- Declaring one also **turns the drift on** for a material whose `tint` class gives it none, which
  is how `downland_grass` got a field of its own without joining the meadows.
- The ground and the grass standing on it read the same function (`classic_patch`).
- `--validate` catches two ends the player could not tell apart, and an end the material's own
  texture cannot reach.

`downland_grass` on the Windbarrow chalk is the worked example, cool khaki to dry gold.

### The four ages

Every individual is also one of four ages, rolled once at spawn and never changing:
juvenile, adult, aged or deceased. `ages` says which a creature has, and every field
defaults from its `height`, so almost no entry needs the block:

| key | default | meaning |
|---|---|---|
| `juvenile` | 0.62 at `height` 0.4 and up, else 0 | A juvenile's body scale; 0 means this creature has no juvenile stage. |
| `aged` | true at `height` 0.4 and up | Whether old individuals turn up, greyer and slower. |
| `bones` | `"full"` at `height` 0.5 and up, else `"none"` | Whether it leaves a skeleton. |

5% of a population is juvenile, 5% aged and 2.5% is bones lying in the field, which is 5%
dying a day with bones that last half a day. Juveniles cluster: in a family group the animal
that chose the spot is the adult and each of the others is a juvenile about a third of the
time. `behaviour.sleeps_on_side` makes an animal tip over as it lies down instead of lying
upright; the cats, dogs, wolves, bears, badgers, pigs, horses, elephants, hippos, monkeys,
seals and walruses do, and so does the triceratops.

Skeletons are derived from the creature's own voxels rather than authored, so a new creature
gets one for nothing, and one too thin to read is dropped at load. The ages, the skeleton and
the ragdoll that poses it are all in [ANIMATION.md](ANIMATION.md), along with predation.

### Forms by sex and age

`forms` names the entries whose models a creature's females, males or young are drawn
with, where they look different from the entry's own model:

```json
"forms": {"female": "deer_female", "juvenile": "deer_juvenile"}
```

A juvenile takes the `juvenile` form if there is one, else its sex's form; a form left out
is the entry's own model (`Species::model_for`). Only the model changes: a lioness is a lion
in every other way, with the lion's behaviour, voice and name. A form entry comes along
with its creature in `used_entries`, is never spawned or penned on its own
(`Library::is_form`), and is usually `like` its species so it takes the rig and colours.
`forms` is never inherited, since an elk's hind is an elk and not a deer.

Each horned or antlered species has a `NAME_female` entry drawn the way the real females
look: smaller (scale 0.82 to 0.95), hornless where they are in life (deer, elk, mule deer,
lechwe, saiga, and polled domestic ewes), with the male's horns shrunk toward their bases
where they carry thinner, shorter ones (bison, cow, aurochs, water buffalo, wildebeest,
serow), with short spikes (the bighorn ewe) or short prongless horns (the pronghorn doe),
and the giraffe cow with hair tufts on her ossicones. These were written once by a script
on 2026-11-24 from each species' own ops and are edited by hand since. The young are
hornless `NAME_juvenile` entries, the deer, mule deer and elk spotted and the bison calf
orange-red with no hump yet; the giraffe calf takes its mother's tufted ossicones, and the
hornless species' young use the female. The triceratops has only a juvenile form, with stub
horns, since no difference between its sexes is known. The lion has a maneless
`lion_female` and a spotted, round-eared `lion_juvenile`. Beyond the horned animals: brown
hens for the mallard, eider (barred) and Gambel's quail, whose own models are the drakes
and the cock; a bigger, maned `sea_lion_male` with a crested crown; tuskless elephant and
mammoth calves; a spotted cougar kitten with a ringed tail; a striped boar piglet; and horse
and zebra foals, whose ops and rig were warped once so the legs keep nearly their full
length under a short, shallow body (the zebra foal's stripes brown on cream). `NAME:ages` on `--anim-sheet` shows an adult of each sex where
they differ, and the zoo's sex columns show them too.

Plants vary per tree in their own way: `height`, `trunk_radius` and `crown_radius` are ranges,
`lean` is a maximum, and `leaf_alt: ["material", chance]` gives a fraction of trees a second
leaf colour (only the flowering cherry uses it). Autumn colour is not per tree: a plant's
`autumn` leaf is worn by every tree of the species when the world's `season` is "autumn", and
the default season is summer, because green and orange trees mixed tree by tree read as messy
from a distance. Plant children inherit all of those.

## From file to world

1. `src/worldload.rs::used_entries` collects the creatures named by the biomes that the built
   world actually contains, plus anything those creatures hunt, plus the rare morphs and `forms` entries of
   anything collected. Only these are loaded.
2. `worldload::creatures` runs each one's ops into its grid (32^3 unless `grid` says otherwise)
   and meshes it into one `Model` (and one part model per bone if it has a rig; a bigger grid
   has only the part models, each cut from a 32^3 window round its bone, `anim::slice::Dims`). Its colours go into a palette block of its
   own, appended after the fixed block (`models::FIXED_SLOTS`), and the model's `block` says
   where that starts; its part models and skeleton use the same block, with the two bone
   colours on the end. A world has no limit on its colours, and one model can use up to 192
   (`models::PALETTE_SLOTS`).
3. Each becomes a `Species` (model, behaviour, `flees_from` / `hunts` resolved, its `vary`, its
   `ages` and the tint that greys an old one,
   rare morphs). When the world is set, `creatures::link_families` widens `flees_from` and
   `hunts` to the named creatures' `like` children.
4. A biome lists creatures with a count: `{"creature": "elk", "count": 3}` means "keep three of
   these around the player while near this biome". `src/creatures/spawn.rs` samples the
   biomes around the player every half second, spawns groups 20 to 90 m away inside a biome
   that lists the species (out of view; in view only small animals, 60 to 110 m out, and the
   far band below), rolls each member against the species' rare morphs, and despawns anything
   beyond 130 m once it is out of view (more than 75 degrees off where the player faces), or
   beyond 360 m even in view (500 m for the walkers the far band sets down, whales 260 and
   600 m). Thinning for the hour and the weather also takes only what is
   out of view, so nothing winks out while it is watched. A group never takes the count past the biome's number (a flock of
   starlings is cut to what is missing), and a rare morph joins its parent's family group and
   counts toward the parent's number (`Creature::spawned_for`).
5. Plants: `flora::place_plants` rolls each tree against its plant's rare morphs
   (`Library::plant_morphs`) before growing it. The roll comes from the tree's position, so a
   world builds the same every time.

So where a creature lives is decided by the biome files, and where biomes are is decided by
the world's fields (height, moisture, the `strange` field and so on) through each biome's
`rules` and `priority`. Structures have one more reuse mechanism: `slots` map names to
materials, so one shape program can be built in different stone or wood.

## Seeing animals far off

A walker at least 0.38 m tall or across its collision box (`creatures::SEEN_FAR_M`: a badger,
raccoon, hare, fox, coyote and up, not a rabbit, skunk or marmot) is seen from far off
(`Species::seen_far`). Such walkers turn up in view only in the far band ahead (`spawn::far_band`),
so a plain has its animals on it when the player looks across it. How far out depends on size
(`Species::span`, the larger of height and collision width; `spawn::FAR_START`):

| Size | Set down in view from | Examples |
|---|---|---|
| 0.4 m | 175 m | badger, raccoon, hare |
| 1.5 m | 225 m | deer, horse, cow, zebra |
| 4.1 m and up | 300 m | mammoth, giraffe |

Sizes in between go in step (an elk at 2.3 m about 245 m, an elephant at 3.0 m about 270 m), and
each is set down anywhere from there to 200 m further (`FAR_DEPTH_M`).

- The biomes across the view at 225, 350 and 450 m are asked what they list, and the band keeps twice
  that (`DISTANT_QUOTA`), thinned for the hour like the rest, at most 60 animals in all
  (`DISTANT_MAX`). Its animals carry `spawn::Distant` and are counted apart from the biomes'
  quotas, so they don't thin the animals round the player. Within 150 m (`JOIN_M`) one joins those
  and counts toward its biome's number.
- Out there they walk the generator's ground, as large animals do ("Large animals" above).
- They are kept in view to 500 m (`DROP_FAR_SEEN_M`), and dropped like any other once out of view
  past 130 m. Turning round therefore empties the band behind and fills the one ahead over a few
  seconds.

Anything set down by the spawner dissolves in over 0.6 s (`spawn::APPEAR_S`): the creature
carries `spawn::Appear`, and `Instance::fade` drops a share of its pixels in a fine
screen-space noise (scene.wgsl `fs_entity_fade`, in its own pipeline so the discard doesn't cost
the other creatures their early depth test). The shadow thins with it. An animal walking away
dissolves out over the last tenth of its in-view reach (`EDGE_SHARE`), so it is gone when it is
dropped.

On Kitehold's plain the band filled to its 60 within a few seconds (bison, horses, pronghorn,
jackrabbits, 257 to 437 m out), taking the creatures from 81 to 141. That added about 0.3 ms to
the tick (2.7 -> 3.0 ms) and 0.02 ms of creature drawing on the GPU, on an RTX 3060 at 1600x900.

## Model resolution and decimation

Every creature is modelled at the same detail for its size, then baked into two decimated
triangle meshes that are what the game draws. The rule and the steps below apply to a new
creature and to migrating an old one; Kitehold's animals were the first done this way
(2026-11-23). Backlog #476 has the measurements behind it.

### The voxel rule

Pick the side to count along:

- Use the body height (the entry's `height`), unless the head-and-body length is more than 1.7
  times that or less than 1/1.7 of it; then use the longer of the two. Head-and-body is
  measured as zoologists do: snout to rump, without the tail, wings, bill, antlers or legs.
  A deer (1.4 m tall, 1.8 m long) goes by height; a lizard (6 cm tall, 15 cm head and body),
  a snake or an otter by length; a dragonfly by its body length.
- Put 30 voxels along that side if it is 0.7 m or more, 20 if it is less, and 10 for
  insects and other small invertebrates and for any animal whose side is under 6 cm. So
  `voxel = side / 30` for a deer, fox, goose or heron, `side / 20` for a rabbit,
  squirrel, frog or sparrow, and `side / 10` for a bee, dragonfly, butterfly or mouse.

A long, low animal can cost more than its size deserves at 20; the marmot and rock hyrax are
drawn at 15 per height instead, and say so in their notes. A child with `scale` keeps its parent's count (scale multiplies `voxel` too), so the rule is
applied to a family's root. Crossing 1.7, 0.7 m or 6 cm changes the voxel size by up to 2x; if
two similar species either side of a line look mismatched, blend between the two instead.

### The steps

1. Rescale. `python tools/creature_rescale.py ROOT VOXEL` redraws a family's root at the new
   voxel size with the same shapes: ops, joints, claims and every descendant's `extra_ops`
   are mapped about the pivot, and `grid` is resized to hold them. A descendant with both
   its own `voxel` and its own `ops` keeps its frame (it is given the grid and rig it was
   using); one with only its own `voxel` has that shrunk with the root's. The originals go
   to `notes/replaced_creatures/`. `--dry` prints what it would do. Nothing is added at
   this step, so boxes stay boxes.
2. Model for decimation (below), at the new resolution.
3. Preview the bake: `python -I tools/creature_lod/preview.py NAME` writes
   `shots/creature_lod/p_NAME.png`, the voxel model beside its 1/2 and 1/4 decimated meshes,
   close up and at 64 and 32 px. Judge the decimated columns: those are what is drawn.
4. `--validate`, then `--anim-sheet NAME:moves` to check the bones still carry the right
   pieces. A bigger grid needs a rig, and every bone must fit a 32^3 window: split a long
   body, tail or neck into more bones where `--validate` says so.

### Modelling for decimation

The decimator keeps silhouettes and colour boundaries and smooths everything else, so the
voxel steps disappear but the shapes under them don't. What reads well decimated:

- Few, large, overlapping shapes. A body is one long ellipsoid, or two or three that overlap
  by half their length, not a stack of small ones: each overlap leaves a crease or a bump
  that the decimated mesh shows as lumps (the deer's v5 body did).
- `segment` (a capsule along points) for legs, necks and tails, so the limb runs into the body
  without a step. Taper by giving each segment a smaller radius than the one before.
- No `box` or `set` on a curved surface. A box stays a cube when decimated: hooves, feet,
  beaks and ears look like bricks. Use ellipsoids flattened on one axis instead. Boxes are
  for things that are flat-sided in life.
- Pattern by `recolour`, never by geometry: a spot drawn as a raised box becomes a bump.
- Thin parts (ears, legs, bills, wing edges) at least two voxels thick where they join the
  body, so the decimator has something to keep.
- Single-voxel details vanish at 1/4. Eyes, nostrils and stripes are colour patches of two
  voxels or more.

### Baking and level of detail

Every creature in use is baked at world load (`models::bake_all`, on every core; Kitehold's
take about 340 ms) into two triangle meshes by `src/mesh/decimate.rs`: surface nets over its
voxels smoothed by Taubin passes, then quadric edge collapse that keeps colour boundaries and
the volume around each collapse (so legs and necks don't slim), to half and a quarter of the
square model's triangles. Earlier versions and rejected alternatives are under "What the
bake replaced" below. A rigged creature is baked as one body and split by bone, with the
ring where each part meets its parent weighted to follow the parent, so joints bend without
seams (ANIMATION.md "Smooth parts"); a `loose` bone is baked as a closed surface apart. Each vertex carries a
smoothed normal and an ambient occlusion read from the voxels around it. The same bake also
merges the voxels 2x2x2 for voxel mode's far level. The meshes sit beside the quads in the
entity buffer and draw with `vs_entity_tri`, posed exactly as the quads are.

The renderer picks each instance's level every frame by how many pixels one model voxel covers
(`render::lod_of`), so a bison and a sparrow switch at the same apparent detail:

| Mode | Near | Far | Switch |
|---|---|---|---|
| Smooth (default) | decimated 1/2 | decimated 1/4 | 2 px a voxel (`DECIMATED_SWITCH_PX`) |
| Voxel | square quads | 2x2x2 quads | 1 px a voxel (`VOXEL_SWITCH_PX`) |

Shadows cast from the far level of whichever mode is on. Instances are sorted by model and level
before drawing, so the same part of a whole herd is one draw call. The setting is CREATURES
(Smooth or Voxel) in the settings menu, or `--voxel-creatures` on the command line;
`LOD_TIMES=1` prints how long the bake took.

On Kitehold's plain at the spawn, 619 creature parts in view cost about 28,000 quads' worth of
triangles smooth against 42,000 as voxels.

`tools/creature_lod/` is the same bake in Python, with a software renderer for comparison
sheets (`preview.py`, `lod_sheet.py`); keep the two in step.

### What the bake replaced

The current bake is the second of four rounds (November 2026), each judged on hippo, giraffe,
flamingo, stegosaurus, deer, rabbit, boar and pond turtle at the same triangle counts.

- The first bake placed corners from 3x3x3 solid counts. It read smooth, but the counts blurred
  one- and two-voxel parts inward: giraffe and flamingo legs went to needles at far and antlers
  thinned.
- Crossings at the voxel faces with crease planes on sharp edges kept the limbs but crumpled the
  bodies: the crease planes locked the 45-degree voxel staircases in as ridges. The current bake
  keeps the face crossings and smooths them with Taubin passes instead.
- Stopping at an error threshold doesn't work on summed quadrics (the error at the 50% point ran
  from 14 on the boar to 119 on the giraffe). Normalising by each vertex's summed plane weight
  would be the way to try it again.
- Normals from a blurred solid grid, edge flips after each decimation, and Taubin that slides
  colour boundaries along the surface were all judged worse.
- A blurred field meshed at half resolution gave the smoothest bodies but lost everything under
  two voxels thick (legs, antlers, the flamingo). Catmull-Clark limit positions left rounded
  voxel steps.
- A colour-blind mesh with the voxel colour looked up per pixel looked the same at equal counts.
  Its gain would be cutting triangles two or three times without blurring markings, which was
  never tested; it needs a 3D colour texture.
- Taubin limited by the body's thickness beneath each vertex (0.3 voxels where one thick, up to
  1.25 where four or more) rounded thick bodies but put faint lumps on deer and boar flanks. It
  was only tried together with the rejected normals and edge flips; on its own it is the one idea
  here worth another look.

## Adding a creature

The creature code is in `src/creatures/`: `locomotion.rs` (the verbs), `terrain.rs` (ground and
water queries), `spawn.rs` (biome quotas), the movers `walk.rs`, `herd.rs`, `circle.rs`,
`flock.rs`, `flit.rs`, `fowl.rs`, `woodpecker.rs`, `insects.rs` and `swim.rs`, `birds.rs` (wing
animation), `census.rs` (counts and the creature under the crosshair), and `affordance.rs` and
`plants.rs` (below). `src/entities.rs` owns the hecs world and runs one step in order;
`src/items.rs` is the pickups.

Every living library animal carries a `Creature { species, spawned_for }` component and a
`Look`, plus its mover's own component (`Critter` and a physics `Body`, `Gull`, `Flyer`, `Flit`,
`Paddler`, `Clinger`, `Buzzer`, `Fish` or `Whale`). The census, the biome quotas, despawning,
individual variation and naming read `Creature` alone, so they cover a new mover without
changes.

A creature that moves like an existing one is data only: a file in `library/creatures/` (often
`like` a relative), a line in a biome's `creatures` with a count, then `--validate` and
`--anim-sheet NAME:family` to look at it. Model it by "Model resolution and decimation" above.

A new verb that an existing mover can run is one row in `VERBS` (`locomotion.rs`): its name,
the `Mover` with its settings, the default activity table (`Routine`) and the default flight
pattern. `flutter` is `circle` with a smaller, slower circle; a walker that needs a new habit
gets a new flag on `Mover::Walk`, read in `walk.rs`.

A new kind of motion:

1. Add a `Mover` variant and a row in `VERBS`.
2. Write `src/creatures/<mover>.rs` with its component, a `spawn` that spawns
   `(component, Creature::of(s), Look::new(model, pos, yaw))`, and an `update(world, species,
   dt, player, &Terrain)`.
3. Add an arm to the `match` on `Mover` in `spawn::world_creatures` (the compiler points at it)
   for where it may turn up.
4. Call its `update` from `Entities::update`.
5. If it flies and has a bird rig, add it to `Verb::flies` and to the perched and landing
   `match` in `birds.rs`.

## Displays

A display is something rare a species now and then does: a hummingbird's courtship dive,
two bulls butting heads, a calf nursing, cranes dancing, wolves howling. Each kind is written
once in Rust under `src/creatures/display/`, and a species opts in from its entry:

```json
"behaviour": {
  "displays": [
    {"kind": "head_butt", "holds": 0, "params": {"backoff": [6, 9]}},
    {"kind": "nurse", "season": ["spring", "summer"]}
  ]
}
```

- `kind`: one of the kinds below.
- `rate`: starts an hour per individual. Left out, the kind's default.
- `season`: when it happens. Empty means all year.
- `cast`: who takes part, laid over the kind's default:
  - `from`: `solo`, `same` (its own species near it) or `family` (its own herd, `Kin`).
  - `count`: `[least, most]`.
  - `range`: metres from the first member.
  - `members`: a list of `{"age", "sex"}` filters. The first is the one that starts it, and
    the last repeats. `"adult"` takes in the aged.
- `holds`: a multiplier on how much the player unsettles it while it runs. 0 is bison
  mid-shove, 1 the usual nerves.
- `on_approach`: started by the player coming within the kind's `near` param, in place
  of `rate`. `threat` is by default; the others aren't.
- `params`: numbers the kind reads; each file's header lists them. Give a number, or
  `[low, high]` for a random pick. A few take a word (`call`'s `posture`, `threat`'s
  `style`).

| Kind | What it is |
| --- | --- |
| `sky_dive` | a hummingbird's U-shaped courtship dive |
| `head_butt` | two males square up, charge and clash, a few times |
| `spar` | two males lock horns or antlers and push and twist, no run-up |
| `nurse` | a juvenile nurses at its mother's flank |
| `dance` | cranes in a ring, bowing, leaping, spreading their wings |
| `call` | head up and calling, alone or as a chorus; standing, sitting or lying (wolves howl, lions roar, frogs pump) |
| `roll` | lies down, rolls on its back with legs pedalling, gets up and shakes |
| `sentinel` | sits up tall and looks round; a prairie dog's jump-yip with `yip` |
| `stalk` | fixes on a spot, creeps, freezes, then pounces (`leap`) or stabs (`strike`) |
| `threat` | answers the player coming close: broadside with a tail swing, a raptor squaring up, a skunk's handstand, a hedgehog curling, a crab's claws |
| `tumble` | two circlers (crows, ravens) chase round in the air, rolling over and dropping, now and then together |
| `cache` | comes down, digs, tucks something in, covers it and looks round (crow, raven, Steller's jay) |
| `bob` | on a perch, swivelling and swaying the head (owls) or dipping the body (dipper); a circler flies to a raised perch first |
| `song_flight` | climbs singing in a spiral, hangs at the top, drops with wings up and lands where it started (sparrow, skylark) |
| `rear_strike` | two stallions rear face to face and strike with their fore hooves |
| `groom` | a pair grooming: horses side by side head to tail (`style` "mutual"), monkeys sitting, one working the other's fur ("one") |
| `necking` | two males side by side swinging their necks into each other's flanks |
| `greet` | two of a family meet head to head and rub; `low` and `wag` for wolves |
| `rub` | walks to a tree and rubs on it: back to the trunk standing up (`style` "back", bears) or the flank ("flank") |
| `pronk` | stiff-legged bounds, all four feet together, taken up by those near |
| `play` | the young of a family chase, pounce, roll and face off |
| `box` | two hares rear up and swat with their fore paws, chase, and go again |
| `drum` | stamps a hind foot in quick runs |
| `chase` | two squirrels spiral round each other, round a trunk if one is near |
| `push_up` | a lizard's push-ups, facing the player if near |
| `wash` | a raccoon at the water's edge, sitting up, working its paws, a crayfish at its mouth |
| `fish` | a bear wades into a shallow riffle, watches the water facing upstream, lunges and pins with its fore paws, and now and then carries its `food` (a salmon) to the bank to eat |
| `dig` | head down, scraping fast with the fore feet |
| `march` | flamingos in a tight block walking together, wheeling, heads flagging |
| `pipe` | oystercatchers running side by side, heads down, calling |
| `sweep` | a spoonbill wading forward swinging its bill through the water |
| `wing_stretch` | stands and opens its wings wide, holds, shakes and folds |
| `bask_pose` | a seal on its side with head and hind flippers lifted |
| `float_play` | an otter afloat spinning on its back and rolling over (water only) |
| `gallop` | the whole group bursts into a curving run and pulls up |
| `gesture` | a person or robot waves, stretches, or looks at its hands |
| `shuttle` | a hummingbird swinging side to side like a pendulum in front of its perch |
| `sky_dance` | a raptor's string of steep climbs and stoops (circlers and soaring birds); a pair locks talons and cartwheels down at the end |
| `wing_dry` | a cormorant on a post with its wings held open |
| `long_call` | a gull settles and calls, head bowed then thrown back; others answer |
| `billing` | an auk pair on the water rattling bills together |
| `wing_clap` | a nightjar's looping flight with wing claps and glides |
| `streamer_flight` | a quetzal's steep climb and swoop back to its perch, tail rippling |
| `plunge` | a kingfisher hovers, dives in, and beats its catch on the perch |
| `drum` | also a woodpecker hammering on its trunk |
| `cache` | also an acorn woodpecker hammering an acorn into the bark |
| `wheel` | the whole flock turning as one, the turn rippling through it; shorebirds lift first |
| `sky_point` | an albatross pair on the ground: bills to the sky, bowing, fencing |
| `broken_wing` | a plover drags a wing and leads the player away (on approach) |
| `courtship` | a roadrunner runs up to another, tail up, bows, and circles it |
| `covey_call` | one quail calls from a high point and the covey gathers at its foot |
| `pair_bow` | a duck, goose or swan pair bowing face to face on the water |
| `orientation` | a bee's widening arcs in front of the flowers before it leaves |
| `tandem` | two dragonflies joined, dipping to the water |
| `swarm` | bats in a rising spiral at dusk |
| `sync_flash` | glowing insects falling into step and flashing together (`Out::flash`) |
| `flash_flight` | a morpho's slow deep wingbeats |
| `bait_ball` | a school pulls into a tight milling ball |
| `drop` | a reef school drops into the coral at once and comes back up |
| `leap` | a fish clears the water; mantas somersault (`flip`) |
| `surface_roll` | a gar or catfish rolls at the surface for air |
| `patrol` | a big fish cruising a long oval |
| `bury` | a ray settling and shuffling into the sand |
| `sway` | a garden eel colony swaying together |
| `pulse_bloom` | jellies rising together to the surface |
| `raft` | two sea otters on their backs holding paws |
| `barrel_roll` | a sky ray rolling round its long axis |
| `spyhop` | a whale rises head out and looks round; `slaps` adds tail slaps |
| `snake_dance` | two male snakes raising and twining their fronts |
| `curl` | a millipede or slug curls up (on approach) |
| `colour_flash` | an octopus flashing pale; a lobster's claws up (`style` "claws") |

Displays inherit down `like` like the rest of `behaviour`, so one on `bison` also reaches
cow, aurochs and wildebeest. `--validate` checks the kind, its params, the cast, the bone
roles it needs and that the species' movement is one the display can drive (the match in
`display::validate`). Every mover can be driven now.

How it runs: `display::Shows` in `Entities` is the one scheduler. It starts a display only
where the player could watch it: within 90 m, in front, beyond the animal's `wary` distance,
and with the animal awake. It keeps at most two going, waits between them, and scales every
rate and the gap by one dial (`DIAL`, or `VG_DISPLAY_RATE`). The dial starts at 1 with
fairly high rates so each display is easy to find; turn it down to make them rare.

Each member carries a `Show` while a display runs. Every mover reads its `Drive` and does
what it says, in place of its own choice (walk.rs, hummer.rs, circle.rs, flit.rs,
woodpecker.rs, flock.rs, runner.rs, fowl.rs, insects.rs and swim.rs). Snakes and the
octopus are posed from their own chains, so their displays hold them still and only lay the
overlay on:

- `Stay`, turned to a yaw
- `Go` to a point
- `Back` up without turning (`display::gather` walks a cast to marks and turns them)
- `Place` at a point, flying, for free movers
- `Sit` at a point, perched with wings folded, for fliers

When it ends a circler flies back up to its circle and a flitting bird sits or flies to a
perch. `display::Hop` (an eased flight between two points) and `display::spot` (dry ground
or a raised perch near a point) are shared by the bird displays, and `display::perch` has
coming down to sit and the water's surface. birds.rs lays the overlay on fliers. Besides
the drive and pose, an `Out` can set a glowing creature's brightness (`flash`, fireflies
in step) and blanch it pale (`pale`, an octopus); entities.rs reads both. It can also
say its food is held (`held`): a display entry's `food` names a collectible or creature
(`acorn`, `herring`, `lobster`, `lizard`), and while `held` entities.rs draws that model
across the mouth, at the front of the jaw or head bone's hull, no taller than a third of
the creature, a rigged one in its rest pose. worldload.rs builds the food's model in every
world that has the creature (`used_entries`). `cache`, `wash` (while the paws are at the
mouth), `plunge` and `courtship` hold it.

`display::apply` lays the `Overlay` over the composed pose by bone role: head pitch and turn,
jaw, wings, arms, tail, the whole body rearing or rolling, legs pedalling, and a tuck that
draws the head and legs in, the hind or fore feet stamping (`stamp`, `paw`), and one arm
raised (`wave`). With `plant`, rearing keeps the low end's legs on the ground, and
`display::keep_up` lifts a walker whose overlay rolls, tucks or tips it so its hull stays
above the ground. `Ctx` also
carries the world's plants (`flora`), for displays that need a trunk. A startled walker bolts, and the display ends for all of it.

Debugging:

- The crosshair readout shows sex, age and what the creature is doing, including the display
  and its phase.
- F10 starts a display on the creature under the crosshair, the next kind each press.
- With the animal debug view on (backquote), I pins the creature under the crosshair: its panel shows the display and phase, and its
  decision log says why it left one (src/game/eco.rs; GUIDE.md has the keys).
- `VG_DISPLAY=<kind>` starts that kind whenever a cast can be found, view or not. It does
  nothing for approach displays, which start when the player comes near anyway.
- `VG_DISPLAY_LOG=1` logs starts, phases and ends.
- The headless stats count them under `creature_activities`.

Every individual also has a sex (`creatures/sex.rs`), rolled when it is dressed. A
creature's `forms` may draw the sexes differently ("Forms by sex and age").

To add a kind, write a file with a `KIND` (defaults, params, roles, `ready`, `start`) and a
`Run`, and add it to `display::KINDS`.

A species that should not inherit its parent's kinds sets its own `displays` list: unicorn and
lechwe keep `nurse` without `spar`, raccoon takes `wash` rather than badger's `dig`, kangaroo_rat
`drum` rather than `box`.

### Known limits

- There is no real contact. Two animals meet when their centres are their two snout reaches
  apart, so `necking`, `greet` and `groom` touch in the wrong place; each model will want a touch
  point from a named bone.
- Birds and fish aren't clamped to the ground the way rolled and tipped walkers are.
- Rigged food is stiff, in its rest pose. Food placement was checked from logged positions, not by
  eye.
- `play` needs juveniles in a family group, which a `VG_SPAWN` test doesn't make.
- `rub` and `chase` look for a trunk among the placed plants; in open country `rub` doesn't start.
- Not run end to end: `spyhop` (whales spawn only over 14 m of water) and the frogs' `call` (no
  frog or toad spawns through `VG_SPAWN` since the frog rework).
- Crow, raven and nightjar inherit starling's flock of 6 to 140, though a raven should be a pair.
- Sightings don't go in the logbook, and displays have no sound yet.

## Voices

`behaviour.voice` lists the calls a species makes and what sets each off. The sound is made by
`src/sound/call.rs` (GUIDE.md, "Animal calls"); `src/creatures/voice.rs` decides when, by
watching the creatures, so no mover knows about sound.

```json
"voice": {
  "size": 1.35,
  "calls": [
    {"cry": "howl", "on": "display:call", "at": "jaw", "boost": 8, "chorus": 0.6, "chorus_m": 800},
    {"cry": "howl", "on": "idle", "hours": "night", "every_s": [600, 1800], "boost": 8, "chorus": 0.7}
  ]
}
```

- `cry` is one of the engine's cries (`sound::call::CRIES`). `size` on the voice or on one
  call makes it bigger (lower, slower); a juvenile's voice is 0.6 of an adult's, and each
  individual's differs by up to 8% either way.
- `on`: `idle` (now and then, every `every_s` seconds per individual, default [60, 240]),
  `flee` and `chase` (a walker's mood turning to either), `takeoff` (a bird that was
  sitting, on a perch, the ground, a trunk or the water, flying up, whatever mover flies it),
  `warn` (a snake that `warns` holding its ground; it calls again as each call ends while the
  warning lasts), or `display:KIND` (as the display starts, or with `"at": "jaw"` each time the display
  opens the jaw, so the `call` display's howls come one per raised head).
- Gates: `hours` (`day`, `night`, `dawn`, `dusk`, `twilight`), `season`, `sex`, `age`, and
  `chance`. An idle call needs the animal awake (its period's activity over 0.3) unless it
  names `hours`, and not running, and is quiet in heavy rain unless `"rain": true`. A song's
  timer runs up to four times faster around sunrise, which is the dawn chorus.
- `boost` raises a marquee call, in dB. `chorus` is the chance each other of its species
  within `chorus_m` (300 m) answers a second or a few later; answers don't set off more.
- At most 12 calls sound at once and 4 of any one species (6 counting answers), and an animal
  makes one call at a time. Each call follows its animal while it sounds.

185 species have a voice. The rest make no sound worth hearing in life and are left silent
rather than given one: most fish, sharks, jellies, eels, crabs, lobsters, the octopus, slugs,
millipedes, lizards, moths, butterflies, fireflies, dragonflies, pelicans, people and bots. Fish
and rays that leap only splash, rabbits only stamp, and tortoises only hiss. 71 of the 195 displays make a sound: the calling ones, plus head-butt
clashes, sparring antlers, a horse's squeal, elephants trumpeting in greeting and the like.
`--validate` checks
every voice: the cry, the trigger, that a `display:` trigger names a display the species has,
and the gates.

## Plants creatures can use

A plant entry may list what it `affords`: `browse`, `perch`, `nest`, `cover` or `nectar`
(`AFFORDANCES` in `src/creatures/affordance.rs`; `--validate` reports any other name). When a
world loads, `Entities::plants` (`Flora` in `src/creatures/plants.rs`) holds every placed
library plant as its trunk position, plant index, height, crown radius and affordances, on a
16 m grid. `near(x, z, radius, Some(affordance))` lists the plants within reach and `nearest`
picks the closest.

Four movers use it. Bees (`insects.rs`) look for `nectar`, which a dozen flowering
plants list, and so do hummingbirds (`hummer.rs`), which also ask `Flora::blooming` whether the
plant is in flower this season; woodpeckers find trunks through `Flora` without an affordance.

Walkers read `browse` (`walk.rs`): an animal with nothing on its list of prey, setting off on a
walk, makes for the nearest plant within 22 m that affords browse about half the time rather
than taking a heading out of the hat, and stands and grazes when it arrives. Not every time,
or a meadow would drain into its hedges; and the random island's three animals have no species,
so none of them browses. `Pathing::browse_walks` and `browse_meals` count it in the headless
stats.

Nothing reads `perch` or `nest`. To add a consumer, give some plants the affordance in JSON,
pass `&ents.plants` into the mover's `update` and steer by the answer (`flit.rs` choosing
`perch` tops before bare voxel tops). A new affordance is a variant in `Affordance` and its name
in `AFFORDANCES`.

## The zoo

`--world zoo` is a level lawn with every library creature in a pen of its own, for looking
at animals that are hard to meet: a juvenile lying down, an aged male drinking, a display
that turns up once an hour. Pens are laid out family by family (by the root of `like`) from
the north-west corner; swimmers, waders, seals, ducks and running birds have theirs along the
sea at the south edge. The code is `src/creatures/zoo.rs`; a world opts in with `"zoo": {}`
in its world.json, and the biome spawner stands down there.

In a rigged walker's pen each column is one age and sex (juvenile, adult and aged, female
then male, leaving out the ages the species lacks) and each row holds everyone in it at one
activity, top to bottom: free (its own choices, kept near home), stand, look, graze, drink,
sit, lie, walk (pacing a line three cells long) and run (pacing five cells, as fleeing).
Below the rows each of the species' displays has a stage with a cast built from its `cast`
filters, and plays again three seconds after it ends. Every other way of moving gets one
group put down by the spawner's own `place` and left to itself.

Kept animals are tame: they don't mind the player, hunt, or run from each other, so you can
walk among them. Pens fill within 70 m of the player and empty past 110 m. The top of the
screen names the pen and row you are in; PageDown and PageUp go to the next and previous pen.
The crosshair readout and F10 work as anywhere else.

`VG_ZOO=deer,heron` (or `"zoo": {"only": [...]}`) pens only those creatures and their `like`
descendants, loads only them, and starts the player at the first pen (unless `--pos` pins
the camera). `VG_ZOO_LOG=1` prints where every pen is:

```bash
VG_ZOO=deer cargo run --release -- --world zoo
VG_ZOO=bison cargo run --release -- --headless --world zoo --frames 400 --look 120,-12 --screenshot bison_pen.png
```

Not done yet: no fences, signs or paths on the ground; non-walkers (birds, insects, snakes,
fish) are not held to an activity and can stray from their pen; a sitting row shows standing
animals for a species whose table never sits; there is no sleep apart from lying.

## Checking

```bash
cargo run --release -- --headless --anim-sheet deer:family --no-hud --hour 10 --size 1600x900 --look -90,14 --screenshot family.png
cargo run --release -- --headless --anim-sheet deer:herd --no-hud --hour 10 --size 1600x900 --look -90,14 --screenshot herd.png
```

`:family` shows the creature with every relative loaded in that world (`--world redrock` for
the jackrabbit and mule deer); `:herd` shows twenty individuals and then the rare morphs.

Running birds and hummingbirds have dev hooks of their own. `VG_RUNNER=track` prints one surf
bird's every step: how far seaward of it the water's edge is (negative when a wave has run past
its feet) and whether it is hurrying, which is how the surf-chasing was checked:

```bash
VG_RUNNER=track VG_SPAWN=sanderling cargo run --release -- --headless --world ribshore --ground 640,371 --frames 1500 --screenshot x.png
```

`VG_RUNNER=1` prints a surf bird's reading of the beach now and then (where it stands, its goal,
the water's edge and the sea). `VG_HUM=1` prints where each hummingbird dart ends (a flower, a
perch, the top of a dive) and how high above the ground. `VG_CATCH_M` names the hunter in each
kill, so a roadrunner taking a lizard shows. Headless `--stats` counts them in
`creature_activities` (`runner standing`, `runner running`, `runner sprinting`, `runner
flying`, `runner on a top`, `hummingbird at a flower`, `hummingbird diving` and so on).
`src/inherit.rs` and `src/variety.rs` have unit tests for the merge rules, the spreads and how
often rare morphs turn up.
