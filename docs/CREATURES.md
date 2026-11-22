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
| `skulk` | same; hunts what it lists in `hunts` | fox, silver fox, coyote, lizard |
| `wade` | same, steering back toward river or coast. A wader whose rig has wings and whose `fly_height` is above 0 takes off instead of running when it bolts (below): it flies 25 to 50 m on slow beats (`wingbeat`) to open ground by the water and lands (`Air` in `walk.rs`) | heron, sandhill crane, faerie heron (fly); capybara (walks) |
| `haulout` | same, close to the sea; lies out on rock or sand, and when it bolts (below) humps to the water and dives with a splash, swimming on in the water form its entry's `in_water` block describes. Seals at sea climb back out onto a low bank about once a minute | harbour seal, sea lion |
| `plod` | same, slow; instead of running it stops and draws head, legs and tail into its shell while the player is within `wary`, and comes out slowly once you leave (`Pose.tuck`, `motors::retract`; legs that stick out sideways fold back under the rim) | tortoise, desert tortoise, moss tortoise |
| `bask` | `plod` on a beach, plus: about once every two and a half minutes (when not hiding) it crawls back to the sea at `flee_speed` and swims off in its `in_water` form; swimmers haul out again like seals. Swimmers with legs beat their front flippers and trail the hind pair | sea turtle |
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
| `nocturnal` | after dark | owls, bats, badgers, hedgehogs, frogs and toads, crocodiles, hippo, jaguar, lion, the water snake, and the lights: fireflies, the marsh light in both its coats (the wisp and the corpse candle) |
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
- the rig is inherited, so a child animates like its parent for free. Extra ops tagged with a
  bone's `part` move with that bone (the elk's mane moves with the neck, the white deer's horn
  with the head).

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

The black bear and the humpback are rare through `spawn_chance` (0.001 and 0.0008 per
half-second check while a biome round you wants one): on paper a bear every 8 minutes in its
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
upright; the cats, dogs, bears, badgers, boar, monkeys and seals do.

Skeletons are derived from the creature's own voxels rather than authored, so a new creature
gets one for nothing, and one too thin to read is dropped at load. The ages, the skeleton and
the ragdoll that poses it are all in [ANIMATION.md](ANIMATION.md), along with predation.

Plants vary per tree in their own way: `height`, `trunk_radius` and `crown_radius` are ranges,
`lean` is a maximum, and `leaf_alt: ["material", chance]` gives a fraction of trees a second
leaf colour (only the flowering cherry uses it). Autumn colour is not per tree: a plant's
`autumn` leaf is worn by every tree of the species when the world's `season` is "autumn", and
the default season is summer, because green and orange trees mixed tree by tree read as messy
from a distance. Plant children inherit all of those.

## From file to world

1. `src/worldload.rs::used_entries` collects the creatures named by the biomes that the built
   world actually contains, plus anything those creatures hunt, plus the rare morphs of
   anything collected. Only these are loaded.
2. `worldload::creatures` runs each one's ops into a 32^3 grid and meshes it into one `Model`
   (and one part model per bone if it has a rig). Its colours go into a palette block of its
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
   that lists the species (behind and beside the player, never in the 140 degrees ahead),
   rolls each member against the species' rare morphs, and despawns anything beyond 130 m once
   it is out of view (more than 75 degrees off where the player faces), or beyond 360 m even in
   view (whales 260 and 600 m). Thinning for the hour and the weather also takes only what is
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
`--anim-sheet NAME:family` to look at it.

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
