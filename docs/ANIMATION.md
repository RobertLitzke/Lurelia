# Creature animation

How creatures move their bodies: legs that walk, heads that look, birds that flap, glide, soar,
land and take off, and creatures that stand, graze, sit and lie down in an unscripted loop. The
system aims for variety and speed of authoring over fidelity; small gaps at joints and feet
that float a little on slopes are accepted. The code is in `src/anim/`, driven from the movers
in `src/creatures/`. [CREATURES.md](CREATURES.md) covers how creatures are defined and spawned.

## Approach

Rigid parts on a bone tree, posed by procedural motors, chosen by a small activity scheduler.
There is no skinning, no keyframed clips and no animation file. Every part is a voxel mesh the
entity shader can already draw at any orientation.

```
mover (src/creatures/)  ->  Life or Flight  ->  motors  ->  forward kinematics  ->  one instance per part
 where to go, how fast,      activity, gait,      bone angles   bone world transforms     entity pass
 threats                      look target
```

Motors key on bone roles, never on species. A deer is a body with four legs, a neck, a head
and a tail; nothing in the motors knows what a deer is, so a new creature with the same roles
animates without new code.

## The rig

### Bones and parts

A library creature may carry a `rig` block (`RigDef` in `src/anim/mod.rs`). Bones form a tree;
each has a name, a role, a parent and a joint position in the model's voxel coordinates. The
creature's `ops` still build the whole body, and each voxel goes to a bone in one of two ways:

- By the op that made it: a shape op's `part` field names its bone, so
  `{"op": "box", ..., "part": "leg_fl"}` puts that box in the front-left leg. Later ops win, as
  they do for colour. This is the normal way, since legs, heads and tails are usually their
  own ops.
- By a `claims` box on the bone, for when one op spans two bones (an ellipsoid that is both
  body and neck). A claim takes voxels from the root or the bone's parent.

Voxels nothing assigns belong to the root (the first bone).

```json
"rig": {
  "plan": "quadruped",
  "bones": [
    { "name": "body",   "role": "root" },
    { "name": "neck",   "role": "neck", "parent": "body", "joint": [16, 18, 11], "claims": [[[14, 16, 7], [19, 23, 12]]] },
    { "name": "head",   "role": "head", "parent": "neck", "joint": [16, 22, 8] },
    { "name": "leg_fl", "role": "leg",  "parent": "body", "joint": [14, 13, 12], "side": "l", "pair": 0 },
    { "name": "leg_fr", "role": "leg",  "parent": "body", "joint": [19, 13, 12], "side": "r", "pair": 0 },
    { "name": "leg_hl", "role": "leg",  "parent": "body", "joint": [14, 13, 23], "side": "l", "pair": 1 },
    { "name": "leg_hr", "role": "leg",  "parent": "body", "joint": [19, 13, 23], "side": "r", "pair": 1 },
    { "name": "tail",   "role": "tail", "parent": "body", "joint": [16, 17, 25] }
  ]
}
```

- Roles: `root`, `spine`, `neck`, `head`, `jaw`, `tail`, `leg`, `arm`, `wing`, `ear`.
- `side` (`l` or `r`) and `pair` (0 front, 1 hind) place legs and wings for the gait and
  flight motors.
- A leg is one bone (hip only) or a chain: a `leg` bone whose parent is a `leg` is a lower leg
  and bends as a knee. Rabbits and lizards use one bone; deer, foxes and birds use two.
- Chains (neck, wing) are consecutive bones with the same role.
- `"overlap": n` on a bone grows its part n voxel layers into its parent's voxels at the
  joint, which hides gaps when the joint bends. Off by default.
- `"flesh": f` on a bone (0 to 1, default 0) makes the part soft: see **Soft bodies** below.
- The root's joint defaults to the body's centre; every other bone needs one. Parents must be
  listed before their children.

The slicer (`src/anim/slice.rs`) runs at load: it runs the ops a second time with each op's
material replaced by its bone, applies the claims and overlaps, splits the 32^3 grid into one
grid per bone and meshes each, so each part is its own `Model` with its pivot at the joint.
The unsliced whole model is kept too. `--validate` checks the rig (`anim::validate`): known
roles and plans, unique bone names, parents before children, joints inside the grid, and every
op `part` naming a bone.

### Soft bodies

A walrus or a seal drawn as rigid parts looks like a potato. Bones with `flesh` above 0 are soft,
and the entity vertex shader moves every vertex of them by one field over the whole body
(`src/anim/flesh.rs`, `flesh_field` in `common.wgsl`, the same in `shadow.wgsl`):

- Sag: the top settles toward the ground and the low flanks bulge out, by up to `SQUASH` (a
  third) of the body's height. How far is the pose's `sag`, which `Life::compose` sets from the
  posture: 0.6 standing (the belly still carries the weight), 1 lying.
- Jiggle: a damped spring per animal (`Jiggle`, in `Anim`), thrown by how the body accelerates,
  from its travel over the ground and from its own pose (the gait's bob and pitch), moves the upper
  flesh against the frame. It rings at 2.6 Hz and is held to 5% of the body's height, so it reads
  as weight rather than wobble.

The field has its origin at the bottom centre of the soft bones' voxels and acts in the creature
frame after the bones are posed, so parts that met before it meet after it. A part with less flesh
than its parent takes its parent's movement at its joint and its own past it, so a rigid flipper
rides on a sagging flank and a half-soft head stays on its neck. Ragdolls, swimmers and skeletons
have no sag or jiggle. The walrus has `flesh` 1 on its body and 0.6 on its head; the harbour seal
(and the sea lion, which inherits it) 0.7 and 0.4.

### Body plans

`plan` picks the posture table and the default activity table; the motors themselves are
picked by roles.

| Plan | Creatures | Notes |
|---|---|---|
| quadruped | deer, elk, bighorn, pronghorn, fox, coyote, marmot, boar, horse, bear and their children | walk, trot, gallop; sits if its table includes `sit` |
| hopper | rabbit, pale hare, jackrabbit | always bounds |
| sprawler | lizard, tortoise, sea turtle, harbour seal | legs swing sideways, the tail swings against them; never gallops; lies flat to bask |
| bird | starling and its children, gull, heron | flight motor in the air; the heron, geese and woodpeckers also use the ground modes below |
| fish | herring, trout | posed by `swim.rs`: body pitch with the climb, side-to-side tail |
| cetacean | humpback | posed by `swim.rs`: up-and-down tail |
| ray | manta ray, sky ray, kite ray | wings are three-bone chains that ripple (`motors::undulate`) |
| serpent | rattlesnake and its children | no motors at all: the body is posed along a path (below) |
| crab | bone crab | eight two-bone legs out to the sides and two two-bone claws (`arm`); walks sideways (below) |
| octopus | giant octopus | a root and eight chains of `arm` bones, each posed along a path of its own (below) |
| biped | humanoid and its children (robot) | walk, jog and run on two legs: knees that bend only backwards, elbows only forwards, ankles that keep the feet flat and point at toe-off, arms counter-swinging, hips dropping and rolling and turning against the shoulders |

`flutterer` is an accepted name with no motors behind it; `--validate` warns that such a
creature would move as a quadruped. Creatures without a rig (the glassmoth, bees,
dragonflies) draw as one rigid model.

A biped driven by an activity table walks through `life.rs` like any other creature. One
driven by a body -- a player's keys, or a brain filling in the same input -- goes through
`figure.rs` instead; see **A piloted biped** below.

## Motion

### Pose

A `Pose` is a root offset and a (pitch, yaw, roll) triple per bone in its parent's frame; bone
0's rotation tilts the whole body. Poses blend by linear interpolation. A pose may also carry a
per-bone `tuck`, which pulls a joint toward the root's joint: a tortoise drawing its head and
legs into the shell.

For a walking creature, `Life::compose` (`src/anim/life.rs`) builds the pose in this order:

1. Posture: the plan's key pose at the current posture level (0 stand, 1 sit, 2 lie),
   blended between levels. Angles scale with the creature's own hip height, so the same "lie"
   fits a marmot and a bighorn.
2. Gait: leg swing, weighted by how fast the creature is moving and zero when it stands or
   lies.
3. Grazing: neck and head down to the ground, nibbling.
4. Look: head and neck turned toward the look target, damped while grazing; raised a little
   when alert.
5. Tail, ear flicks and breathing.
6. Ground tilt: body pitch from the ground height under the front and hind hips, clamped to
   0.35 rad.
7. `fold_wings` for a bird on the ground, and `retract` for a creature hiding in its shell.

### Motors

Motors (`src/anim/motors.rs`) are plain functions that write bone angles into a pose, keyed by
role.

- Gait. Each upper leg swings as a sine of the stride phase plus its gait offset; a lower leg
  bends only on the forward swing. Gaits are walk (lateral sequence), trot (diagonal pairs),
  transverse gallop, and bound (hind legs together, then the fore legs). Phase advances by
  distance travelled divided by stride length, and stride comes from hip height, so feet slide
  little at any speed. The gait is chosen by Froude number (v^2 / (g * hip height)): walk
  below 0.6, trot below 2.5, gallop above, so a fox trots while a deer walks at the same speed.
  Hoppers always bound and sprawlers never gallop. A gait change waits for the end of a stride
  and cross-fades over 0.3 s; switching phase offsets at once made legs jump.
- Look. The head turn is split about 60% into the neck bones and 40% into the head (or all
  into whichever exists).
- Graze. Neck and head down, with a nibble.
- Tail. Sway from the stride plus a slow wave of its own; raised when alarmed (a deer's
  white flag).
- Ears. A flick every few seconds, forward when alert.
- Breathing. A slow rise and fall of the body, faster after a fright.
- `fold_wings`. Wings folded back along the body.
- `retract`. Head, legs and tail tucked into the shell and the shell let down onto the
  ground; legs that stick out sideways fold under the rim.
- `undulate`. A wave running out along each wing chain, for rays.

### Activities

An activity (`Act` in `life.rs`) is one of `stand`, `walk`, `graze`, `look`, `sit`, `lie` and
`drink`. When the mover's timer runs out it asks `Life::choose` for the next one: a weighted
pick from the current activity's row, with a duration drawn from the row's time range. Each
individual's `habits` (from `vary`, see CREATURES.md) scale the weights, so one deer grazes
more and another lies down more.

The default table comes from the plan and the movement verb's `Routine` (graze, hop, skulk or
still; `VERBS` in `src/creatures/locomotion.rs` names one per verb). A rig's `activities`
block overrides rows by name; the marmot uses it to sit up.

```json
"activities": {
  "stand": { "time": [2, 6],   "next": { "walk": 4, "graze": 3, "look": 2, "lie": 0.5 } },
  "lie":   { "time": [20, 90], "next": { "stand": 1 } }
}
```

- The posture level moves toward the activity's level at a fixed rate, so a lying deer gets up
  before it walks. `Anim::mobility` is 0 until it is standing, and the mover scales its speed
  by it.
- When the creature is alarmed or the mover wants to move, the level changes at about three
  times the normal rate: a lying deer scrambles up and bolts.
- A table with no `sit` goes straight from standing to lying.
- Fleeing and chasing are decided by the mover, not the table; `Life` sees them as `moving`
  and `alarm` in its `Input`.
- `drink` is chosen like any other activity; `walk.rs` finds the water and walks there, and
  `Life` lowers the head once the creature stops. How often drinks are kept follows the time
  of day (CREATURES.md).
- Look target: the player when within 1.6 times `wary` and not fleeing; otherwise every 1 to
  4 s a new random glance, wider during `look`. The head follows by exponential smoothing,
  faster when alert, with yaw clamped to 1.4 rad and pitch to -0.5..0.6 rad. The body does not
  turn to follow.

### Birds

Flight paths stay in the movers (`flock.rs`, `circle.rs`, `flit.rs`, `fowl.rs`,
`woodpecker.rs`, `runner.rs`, `hummer.rs`); `src/anim/flight.rs` only reads the bird's velocity, whether it is perched,
whether it is landing and whether it is clinging. `creatures::birds::animate` gives every
rigged `Flyer`, `Gull`, `Flit`, `Paddler`, `Clinger`, `Runner` and `Hummer` a flight `Anim` and steps it each tick
from the position change.

- Wings are two-bone chains each side (inner and tip). A beat is a shoulder roll plus a
  delayed tip roll, with a faster downstroke than upstroke.
- Patterns (`behaviour.flight`, or the verb's default in `VERBS`):
  - `flap`: steady beats
  - `flap_glide`: a few beats, then a glide (flock, circle)
  - `bound`: a burst of beats, then wings folded and a dip (flit, cling)
  - `soar`: wings held in a shallow V, a rare beat (soar)
  - `sail`: wings held out flat, a beat or two in half a minute (albatross)
  - `hover`: fast shallow beats (flutter, undulate)
- Wingbeat rate is `behaviour.wingbeat`, or comes from the wingspan in the rig
  (`flight::beat_for`, 1.5 to 12 a second), so a starling beats fast and an eagle slowly. A
  rate under 3.5 a second beats shallower, so big wings don't hinge through 80 degrees.
- Climbing faster than 2 m/s always beats (except when soaring). The threshold is high because
  a flock's own jostling would otherwise keep starlings beating nearly all the time.
- Banking: roll from turn rate times speed, clamped to 0.9 rad. Pitch follows the climb.
- Take-off: a 0.8 s burst of faster beats. Landing (a flock in `Land` mode within 5 m of its
  goal, or the last part of a sparrow's hop): the body pitches up, the wings come forward and
  the legs reach forward.
- Perched: wings folded back along the body, quick head turns and the odd peck. The rig's
  `floor` (height of the lowest voxel) puts a perched bird on its feet.
- Clinging (woodpeckers): the body pitched upright and bursts of quick drumming pecks.
- Walking or paddling (geese ashore or afloat): wings folded, legs striding with the pace and
  the body rolling onto each foot in turn.
- Running birds (`scurry`, `patter`, `dash`: runner.rs) are animated as birds standing on
  the ground: `Flight::stride_m` (about the bird's height, from 6 cm) sets the stride, so a
  sanderling's legs turn over up to eight times a second, and `peck_every` keeps the surf birds
  and plovers probing every half second or so. Hummingbirds (`hum`) set `Flight::upright`: hovering
  slowly the body hangs tilted up to 0.75 rad, standing nearly upright climbing straight up.
- Birds that walk (`wade` with a bird rig: heron, crane) run `Life` on the ground with
  `fold_wings`, and switch to `Flight` for the short flight when flushed (`Air` in `walk.rs`).
- Rays that fly (`undulate`) ripple their wings instead of beating them; rays at sea are
  rippled by `swim.rs`.

### Serpents

A serpent is the one plan with no motors. Its rig is a single chain of bones,
the head at the root and the tail tip at the far end (`--validate` insists on
it), and `src/anim/spine.rs` poses the whole body along a path: one world
point per bone, head first.

```
mover (creatures/snake.rs)  ->  Spine (a path)  ->  pose_along  ->  one instance per part
 leads the head                 the body follows     bone angles
```

- `Spine::lead` moves the head and pulls every point behind it back to its own
  distance from the one ahead, so the body follows the head's track.
- `Spine::settle` eases each point onto the ground under it, which is what
  puts a snake over a rock rather than through it, and `tighten` takes the
  stretch that leaves back out.
- `pose_along` turns the path into bone angles: each bone's rotation is the
  one that turns its rest direction onto the step from its own point to the
  next, converted into the parent's frame. The chain is anchored at the head,
  so the head lands exactly where the mover put it.
- `add_wave` lays a travelling sine down the joints — radians per joint, not
  metres sideways. Offsetting the path itself by anything near a bone's own
  length folds it back on itself wherever the path is already curved (a snake
  crawling out of its own coil), and a folded path throws the parts about.
  The phase runs backwards as the snake advances, so the wave stands still on
  the ground and the body slides through it.
- `coil`, `drape` and `settled` (the ragdoll) are paths and poses of their
  own: a ring that rises as it goes round with the head lifted over it, a
  body lying along a branch with its front curving off and hanging, and a
  limp body for a snake that has let go of one.
- `JOINT_RATE` holds every joint to 6 radians a second. A path can ask for
  more than a body can give — a hairpin travelling down the length as a coil
  unwinds — and the lag that costs reads as weight.

### Crabs

A crab's model is drawn turned a quarter: its right side faces -Z, the way every walker is led,
so `walk.rs` moves it sideways without knowing it is a crab. Its legs reach out along ±Z and its
claws point along -X. The crab motors in `motors.rs` key on which side of the body a bone is on
(`crab_lead`: the side it walks toward, or the side it walks away from) rather than on `side`
and `pair`:

- The scuttle is an alternating tetrapod: each leg is half a cycle out of step with the legs
  either side of it and with the one opposite, so four feet are down at any time. A leg on the
  leading side reaches out (the lower leg opens) as it swings and pulls the body after it; a
  leg on the trailing side comes in as it swings and pushes. Stride length still comes from hip
  height, so the feet slide little.
- Resting, the body is let down onto the sand and the legs splay round it; there is no sitting.
- `motors::claws` raises the claws a little while it walks, picks at the ground and carries
  each claw to the mouth in turn while it feeds (the graze weight), and holds them up and wide
  when it is alarmed.

The activity table for the plan has it feeding most of the time. Look does nothing, since a
crab has no neck or head bone.

### Octopuses

An octopus has the serpent's machinery eight times over, with no motors. Its rig is a root for
the head and mantle and eight chains of `arm` bones, each pointing out from the body at rest.
`creatures/octopus.rs` keeps one `Spine` per arm: a path from where the arm leaves the body to
its tip.

- `spine::pose_limb` lays a chain that hangs off a parent along a path. Unlike `pose_along` it
  does not assume the chain runs down +Z at rest: each bone is turned from the way it points in
  the model onto the way the path runs, then into its parent's frame. `a_limb_lands_on_its_path`
  pins it.
- Resting or creeping, each arm eases toward a spread path round the body that curls slowly at
  the tip (and, creeping, reaches and pulls in turn), then `settle` lets it down onto the bed.
  That is the drape: an octopus on a rock or a rib has its arms running down the sides of it.
- Jetting, nothing is eased: `lead` pulls each arm after its base, so the arms stream behind
  the body as it goes off mantle first, pitched over so the mantle leads.
- Every heading is in the root bone's yaw, and `look.yaw` stays 0, as for snakes. The pose is
  held to `JOINT_RATE` like a snake's.
- It pales as anything comes near: the mover writes its own `Morph` tint and shift, blending
  from the tint it was rolled with toward the sand colour (`PALEST` of the way), quickly going
  pale and slowly coming back. Nothing else in the drawing changes.

`--anim-sheet NAME:moves` on an octopus shows it spread, curled and at four points of a creep
below, and draped over a mound and jetting above, through the same path code as the mover.

## Rendering

Each part is its own `Model` and its own entity instance (`anim::push_parts`, called from
`Entities::instances`):

- position: the joint's world position after forward kinematics
- yaw: the creature's yaw
- rot: the part's orientation in the creature's frame, as the quaternion the entity shader
  applies before the yaw
- info: the part's voxel size (times the individual's size) and its pivot at the joint

Parts with no voxels are skipped. The fixed-step update keeps the previous pose next to the
current one (`Anim.prev`, `Anim.pose`), and `Entities::instances` blends them by `alpha` as it
does position and yaw. A rigged creature with no `Anim` component draws at rest.

## Ages, bones and ragdolls

An animal is one of four ages (`creatures/age.rs`): juvenile, adult, aged or deceased. The
stage is rolled once when the individual spawns and never changes, and it is laid over the
individual's own roll of size, coat and habits (`variety::Morph::age`):

- juvenile: body scale from the library's `ages.juvenile` (0.62 by default), head, jaw and
  ears 1.15 times larger again (`push_parts`'s `head`), quicker to startle, and its collision
  box shrinks with it. In a family group the animal that chose the spot is the adult and each
  of the others is a juvenile about a third of the time, so fawns come with does.
- aged: a greyer, lighter coat from the species' `aged_tint` (one tint aimed at the mean of
  the model's colours, since a single instance tint cannot grey each colour exactly), 0.8
  speed, and `Life::supple` 0.7, which shortens the stride and quiets the ears.
- deceased: no mover and no collision box. It is drawn with `Model.skeleton` instead of
  `Model.rig`, in the bones' own colours, and it is removed after half a day
  (`age::BONES_LAST_S`, 750 seconds of `Entities::clock`).

Which ages a creature has follows from its `height` unless its entry says otherwise:
knee-high and up has young and old, half a metre and up leaves bones. `"ages": {"juvenile":
0, "aged": false, "bones": "none"}` turns any of them off.

### The skeleton

Nothing about a skeleton is authored. `anim/bones.rs` walks the `owner` grid the slicer
already built and keeps, for each bone, a thin structure inside the flesh: a strut down a leg,
a neck or a tail; a ribcage every third voxel along the body with a spine over it; a hollow
skull. Ears get nothing. The parts come out on the creature's own rig, so a skeleton poses,
lies and ragdolls through the same code as the animal.

Every voxel it keeps is one that was solid in the living animal and was *not* on its surface.
The core is defined as
the voxels with no face on the outside world, and the thinning only ever removes from it, so
the property holds by construction; `skeleton_stays_inside_the_animal` asserts it for every
rigged creature in the library.

A skeleton under `bones::MIN_BONE_VOXELS` reads as litter rather than as an animal, so the
loader drops it (`worldload::creatures`) and that species leaves nothing, however tall it is.
A thin creature can clear the height rule and still fail this one, which is why a condor
leaves no bones. An entry that asked for `"bones": "full"` and fails gets a `--validate`
complaint naming the voxel count.

### Ragdolls

`anim/ragdoll.rs` is a body with nothing holding it up. Each bone gets an angular velocity;
gravity pulls whichever way the bone's own long axis is not already pointing, the joint
clamps at a per-role limit, and friction takes the motion out of it. Once every joint is
still the pose is held and costs nothing further. The body itself keeps the roll it was given,
since the ground, not a joint, is holding it up, and it is lifted back up by `FLANK` of its
hip height because it rolls about the model's feet.

Which way a bone points at rest comes from the plan: down the body for most
animals, back along the chain for a serpent, so a snake that lets go of a
branch flails instead of standing to attention.

Five things use it:

- the deceased settle into it (`ragdoll::settled`), seeded per individual, so no two skeletons
  in a field lie the same way
- prey in a predator's mouth keeps stepping, with the carrier's pace fed in as `shake`, which
  is the jiggle
- an animal whose entry says `sleeps_on_side` tips over into `ragdoll::on_side` as it lies
  down, worked out once the first time it lies and then kept, because settling a ragdoll is
  not something to do every frame
- a snake that drops out of a tree is limp all the way down, with the fall fed
  in as `shake`, and the mover holds its head where the fall has got to
- a body thrown by a blast (`Ragdoll::hurl`; `physics::tumble` moves it, for a
  fighter from `shooter` and for an animal from `creatures::flung`) leaves the
  ground upright and tumbles. While it is `aloft` the root turns past its joint
  stop and keeps its spin, nothing is lying on the floor, nothing is brought
  down onto it, and **no bone has any weight**: a body in free fall is
  weightless, and gravity on each bone through the fall would pin every joint
  at its stop and land a corpse arched backwards with its arms out. On the way down
  the root is wrapped back into a half turn, the floor turns the body's own
  axis toward horizontal (`TOPPLE`, measured off the axis so a body that lands
  on its head is dealt with too), and its joints ease back toward rest
  (`RELAX`) -- the only thing that straightens a bent limb, since a flat body's
  limbs are held by the floor rather than by gravity. A body that was dropped
  rather than thrown feels none of this: nothing of it is bent

### Predation

There is no pounce and no eating (`creatures/carry.rs`). A hunter that already chases its prey
(`walk.rs`, from the library's `hunts`) takes it at `CATCH_M`: the prey loses its `Critter` and
`Body`, gains a limp ragdoll and a `Carried`, and is pinned to the hunter's jaw or head joint
as that joint is actually drawn. Prey at least 0.55 of the carrier's height is dragged with its
feet on the ground and trailing; anything smaller is carried clear of it. A rabbit is carried,
a deer is dragged.

The hunter walks it off for `CARRY_S` and drops it, then is not hungry again for `FULL_S`. What
it drops stays a body for a minute and is bones after that (`Age::fresh_kill`), or, if the
creature leaves no bones, it is gone. A corpse has no `Critter`, and every mover's query asks
for one, so nothing chases a corpse and nothing flees one.

## Checking

- `--anim-sheet NAME` draws a contact sheet: the rest pose and each bone bent in turn, to catch
  wrong joints and parts.
- `--anim-sheet NAME:moves` on an octopus: see Octopuses above. Crabs and octopuses are
  spaced wider on every sheet, since their legs and arms fill the grid.
- `--anim-sheet NAME:moves` on a serpent draws the crawl at four phases of its wave, a
  turn and a swim below, and stretched out, coiled, coiled with its head up, climbing,
  draped and limp above.
- `--anim-sheet NAME:moves` draws postures, gaits and activities. It uses the species' own
  `wingbeat` and `flight`, shows rays' ripples and the hiding poses of plodders, and shows
  both walking and flight for a walking bird.
- `--anim-sheet NAME:family` and `NAME:herd` are covered in CREATURES.md.
- `--anim-sheet NAME:ages` draws the ages side by side: each living stage standing and
  mid-stride, the side-sleeping pose if it has one, and the skeleton settled from three
  different falls.
- `--catalog` gives each creature an `ages` row and a shot of its stages in a line.
- `VG_DEAD_SHARE=0.3` fills a field with skeletons to look at them; `VG_JUVENILE_SHARE` and
  `VG_AGED_SHARE` do the same for the living stages.
- Unit tests over every rigged creature: `skeleton_stays_inside_the_animal` (in `bones.rs`)
  and `every_creature_settles_and_stays_within_its_joints` (in `ragdoll.rs`); in `slice.rs`, `parts_add_up_to_the_whole` (no voxel
  lost or doubled) and `rest_pose_reproduces_the_whole_model` (every part's voxels land where
  they were in the whole model).
- In `spine.rs`, `bones_land_on_the_path` (every bone is drawn on the point the path put
  it on, at any yaw) and `leading_the_head_keeps_the_body_together`. In `snake.rs`, five
  minutes of a rattlesnake crawling and resting on real ground without coming apart, a
  rattlesnake that coils and buzzes rather than leave, a quiet snake that leaves, and a
  tree snake that drops on someone standing under it and crawls away.
- `a_deer_lives_smoothly` in `life.rs` simulates ten minutes with a bolt every two minutes and
  checks at least four activities, no movement while still down, and no joint moving more than
  0.3 rad in one 1/60 s step.
- Headless `--stats` reports `creature_activities`.

```bash
cargo run --release -- --headless --anim-sheet deer --no-hud --hour 10 --size 1600x900 --look -90,14 --screenshot bones.png
cargo run --release -- --headless --anim-sheet deer:moves --no-hud --hour 10 --size 1600x900 --look -90,14 --screenshot moves.png
```

`--look -90,14` from the Glassfall spawn puts sky behind the sheet; elsewhere trees get in the
way. Bighorn, coyote, pronghorn and lizard need `--world redrock`.

## A piloted biped

`anim/figure.rs` animates a body that something else is already driving. `life.rs` is the
other way round: there, the creature picks an activity and the pose follows from it. A
fighter has no say -- its keys are already pressed -- so a `Figure` holds no activity at all,
only continuous quantities smoothed toward what the body is doing: which way it is moving
relative to the way it is facing, whether its feet are on the ground, how far it is
crouched, where it is looking, how fast it is turning, whether its weapon is up, and what
the last shot did. That is what lets a bot strafe out of a sprint into a jump and read as
one movement rather than as three clips.

On top of the stride it adds what only a fighter needs: the side step (legs out and across
instead of fore and aft), the backpedal (the same cycle run backwards), the crouch, the jump
tuck and the landing the knees take, the lean into a turn, the weapon held still in both
hands over a moving body, the recoil, and the death, which is `ragdoll.rs`.

The gun is placed first and the arms are solved onto it (`figure::weapon`, `two_bone`): the
stock in the pocket of the right shoulder, the barrel along the aim (or down at the floor
ahead at a sprint), the right wrist on top of the grip and the left under the barrel.
`Figure::grip` hands back that same placement, so the gun drawn, the hands on it and the line
a bot's shot leaves along all agree. The arms used to be set to fixed angles with the gun
hung off the hand, which left the gun level whatever the aim and the forearms low.

A trunk lean goes on the spine bones (`figure::lean`), never the root, because the legs hang
off the root. On a bone that points up a positive x turn tips it backwards, the opposite of a
hanging limb; every trunk lean once had this the wrong way round and the bots ran leaning
back. `a_runner_leans_forward` pins it.

To judge any of this without a match: `lurelia --headless --world glassfall --pos 0,90,0
--look 0,0 --hour 12 --bot-sheet --no-hud --frames 8 --size 2400x1000 --screenshot X` draws
ten states (idle, walk, run, sprint, backpedal, strafe, aim up, aim down, jump, firing) side
on above and three-quarters below, through the same `shooter::draw_bot` a match uses.

The deathmatch bots are the only users (`brain/avatar.rs`, which compiles the library
`robot` into models of its own so a bot looks the same in every world). A `Figure` is display
only: it reads the body and never writes to it, and nothing that decides anything may read
it. See `docs/BOTS.md`.

One thing a bot wears is not in its library entry: the lamps that carry its health, a bar across
the top of the chest and another between the shoulder blades. Health is state rather than shape,
so `avatar.rs` draws them as their own instances and the `robot` entry holds only the dark bands
they sit in. A bot is drawn as one of three children of `robot` (`robot_brawny`, `robot_heavy`,
`robot_lean`, picked from its seed); `avatar.rs` finds each build's chest surface on its voxels,
so the bars follow a deeper or flatter chest without anyone moving an offset.
The consequence is worth knowing before you reach for the obvious tool: **`--edit robot`
cannot show them.** The EDIT page draws the library entry, so it shows two dark bands and
nothing lit, and the one page that lets you turn a robot round is blind to the one part of it
that changes. Judging a lamp means a bot in a match, which means the headless recipe in
`docs/BOTS.md`.

They are high on the chest because nothing on a fighter's torso can be read: a weapon held in
both hands lies across the middle of the chest and the supporting arm crosses what is left.
Anything added later that a player is meant to read off a body has the same problem.
`brain::avatar::tests::the_lamps_sit_on_the_body` pins the placement, since a lamp a few
voxels out is simply inside the chest, drawing nothing, and nothing else would catch it.
