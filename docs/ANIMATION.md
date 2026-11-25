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
- `"rigid": true` on a bone keeps it from ever turning on its joint, whatever the motors do. It
  is for a piece split off only so that every part fits its 32^3 window: the salt tortoise's
  shell is quartered, the root keeping one quarter and three rigid `spine` bones claiming the
  others, which would otherwise swing apart with the sprawler's spine. Leave the root a piece of
  its own: a root that owns no voxels fails the slicer's test.
- `"spread": [pitch, yaw, roll]` on a bone is its pose in flight, for a flier whose rest pose
  stands on the ground. See **Pterosaurs** below.
- The root's joint defaults to the body's centre; every other bone needs one. Parents must be
  listed before their children.

The slicer (`src/anim/slice.rs`) runs at load: it runs the ops a second time with each op's
material replaced by its bone, applies the claims and overlaps, splits the grid into one
grid per bone and meshes each, so each part is its own `Model` with its pivot at the joint.
The unsliced whole model is kept too. A creature with a `grid` bigger than 32^3 (the
sauropods, the tyrannosaur) has no whole mesh: each part is cut from a 32^3 window round its
own voxels (`Sliced::origins`) and its pivot is the joint less the window's corner, so the
parts pose and draw exactly as a 32^3 creature's do. `--validate` checks the rig
(`anim::validate`): known roles and plans, unique bone names, parents before children, joints
inside the grid, every op `part` naming a bone, and on a big grid that every part fits its
window.

### Smooth parts

Each rigged creature is also baked at load into two decimated triangle meshes of its whole body,
split by bone into its parts, and drawn as those unless the player picks voxel creatures
(docs/CREATURES.md "Model resolution and decimation"). Because the body is one surface, a part
and its parent share a ring of vertices where they meet. In the part's mesh that ring follows the
parent's rotation and the ring next to it a third of the way (the seam weight, eight bits in the
vertex), so a bent knee or a turned neck stretches the surface instead of opening a gap or
showing two rounded ends. The parent's rotation rides in the part's instance (`glow[1..]`, set in
`push_parts_at`) and `entity_local_seam` in `common.wgsl` blends the two, in the scene and the
shadow pass. Parts that touch without being parent and child (legs side by side) keep their own
rotations, so they can still part a little. A part that lies flat against others at rest and swings away from them (a pterosaur's wing panel, the triceratops frill over its shoulders) has no surface where they touch, so it opens holes; give its bone `"loose": true` and it and its children are baked as a closed surface of their own, with no seam to the parent. Bone boundaries are held through the decimation like
colour boundaries, so a joint's ring stays at the joint. `overlap` only matters in voxel mode. The sloped parts that used to do this job (`"slope": true`,
`src/anim/slope.rs`) are gone.

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
rides on a sagging flank and a half-soft head stays on its neck. Limp bodies, swimmers and
skeletons have no sag or jiggle. The walrus has `flesh` 1 on its body and 0.6 on its head; the harbour seal
(and the sea lion, which inherits it) 0.7 and 0.4.

### Body plans

`plan` picks the posture table and the default activity table; the motors themselves are
picked by roles.

| Plan | Creatures | Notes |
|---|---|---|
| quadruped | deer, elk, bighorn, pronghorn, fox, coyote, marmot, boar, horse, bear and their children; the dinosaurs | walk, trot, gallop; sits if its table includes `sit`. A rig with only two legs (a tyrannosaur, a raptor) takes them in turn at every pace, so its run is a fast walk rather than a hop, and rises and falls once a step. A chain of tail bones shares the swish and the alarm lift between them, so a long tail curves instead of whipping up |
| hopper | rabbit, pale hare, jackrabbit | always bounds |
| sprawler | lizard, tortoise, sea turtle, harbour seal | legs swing sideways, the tail swings against them; never gallops; lies flat to bask |
| bird | starling and its children, gull, heron | flight motor in the air; the heron, geese and woodpeckers also use the ground modes below |
| fish | herring, trout | posed by `swim.rs`: body pitch with the climb, side-to-side tail |
| cetacean | humpback | posed by `swim.rs`: up-and-down tail |
| ray | manta ray, sky ray, kite ray | wings are three-bone chains that ripple (`motors::undulate`) |
| serpent | rattlesnake and its children | no motors at all: the body is posed along a path (below) |
| crab | bone crab | eight two-bone legs out to the sides and two two-bone claws (`arm`); walks sideways (below) |
| octopus | giant octopus | a root and eight chains of `arm` bones, each posed along a path of its own (below) |
| biped | humanoid and its children (`bot` and `zombie` and their builds) | walk, jog and run on two legs: knees that bend only backwards, elbows only forwards, ankles that keep the feet flat and point at toe-off, arms counter-swinging, hips dropping and rolling and turning against the shoulders |

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
- `"sit_up"` on the rig (radians, next to `plan`) makes `sit` a lookout: past about 0.5 the
  body tips back that far, the hind legs fold flat under it, the fore paws come up to the chest
  and the head and tail turn back level (`motors::sit_up`). The prairie dog uses 1.35 and sits
  for much of its day.
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

### Pterosaurs

The quetzalcoatlus and the pteranodon are modelled standing, plan `quadruped`, and open their
wings only in flight. Each side has three bones:

- `leg_fl` (`leg`, pair 0): the forelimb from shoulder to hand, with the inner membrane as a
  sheet behind it. On the ground it walks as a front leg.
- `wing_l` (`wing`, child of the forelimb, hinged low on it): the wing finger, folded up and
  back along the side, with the part of the outer membrane that lies between it and the arm.
- `panel_l` (`wing`, child of the finger, same hinge): the rest of the outer membrane, stored
  behind the folded finger inside the inner membrane's outline, which fans out behind the
  finger in flight.

Every bone that moves in flight has a `spread` angle: the forelimb rolls out level, the finger
pitches down past the hand into line with it and swept back a little, the panel pitches on
round behind it, and the neck, head, body and hind legs take their flight attitude. In the
air `Flight::compose_spread` (flight.rs) eases each bone from rest to its spread angle as the
wings open, beats the forelimb's roll on top as a bird's inner wing, flexes the finger as a
tip, and lets the panel ride on the finger. Landing eases everything back to rest, which is
the standing pose, so there is no separate fold. `fold_wings` leaves bones with `spread` alone.

The membrane geometry is laid out as the wing is in flight (one planform tapering from the
inner membrane's width at the hand to the finger tip) and folded back to rest by each piece's
spread angle, so the pieces meet without gaps when open. The layers sit one voxel apart across
the side of the body, so the open wing is stepped by a voxel or two. A voxel of membrane that
falls inside another bone's op (a thigh) moves with that bone and leaves a hole in flight.
The panels are `loose` (Smooth parts above): sandwiched between the finger's membrane and the
inner membrane at rest, joined into one surface they had no faces of their own and showed as
slits in the open wing.

A walker with wings and a `fly_height` takes off when it bolts (`Air` in walk.rs), so the
quetzalcoatlus (`graze`) runs a few steps' worth of alarm and flies 25 to 50 m. The pteranodon
(`glide`) flies all the time as a `Flyer` and stands when it comes down on high ground.
`creatures::birds::animate` and the `moves` sheet treat a rig with `spread` bones as winged.

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
- `coil`, `drape` and `settled` (limp.rs) are paths and poses of their
  own: a ring that rises as it goes round with the head lifted over it, a
  body lying along a branch with its front curving off and hanging, and a
  limp body for a snake that has let go of one.
- `JOINT_RATE` holds every joint to 6 radians a second. A path can ask for
  more than a body can give — a hairpin travelling down the length as a coil
  unwinds — and the lag that costs reads as weight.

### Long necks and tails

`src/anim/trail.rs` bends a sauropod's neck and tail out of line with its body. A tail of
three bones or more (two for a sprawler: the crocodile, the alligator, the lizard) is a rope
dragged behind the hips with the snake's `Spine::lead`: its root follows the body, every point
behind is pulled up to its own distance from the one ahead, and `spine::pose_limb` lays the
bones along it. So a turn swings the tail wide and walking on straightens it. The rope runs on
the flat, each bone keeps its own droop from the model, no joint turns more than 0.45 rad from
the one before, and a tail left standing eases back straight. Lying down blends it out.

A neck of three bones or more browses: it swings well out to one side or the other while the
animal grazes, and leads into a turn while it walks, more toward the head. Neither counts in
the crowd: `crowd::trunk_reach` cuts the walker's capsule at the root of the neck and of the
tail, so a herd of diplodocus spaces by their trunks rather than by 25 m of neck and tail.

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

## Ages, bones and limp bodies

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
skull. Ears get nothing. The parts come out on the creature's own rig, so a skeleton poses
and lies through the same code as the animal.

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

### Limp bodies

`anim/limp.rs` is an animal with nothing holding it up: the dead, the carried, and an animal
asleep on its side. It replaced a ragdoll that turned joint angles about a body pinned at its
feet. That one had no floor, only a rule for how far down to put the body afterwards, so a
skeleton came to rest rolled part way over with its legs swung back under it and the lowest
joint, a hoof, on the ground: a carcass propped up on four straight legs. Nothing could hang
from a mouth either, since the body was posed about its own feet.

A limp body is points. Every part of the animal is a rigid cluster: its own joint, the joints
of the parts that hang off it, and up to fifteen voxels on its outside (`Bone::hull`, from
`slice::hulls`, the furthest voxels from the joint in the six box directions, the eight
corner directions and outright). Each step:

- every point falls under gravity, plus any `push` the caller gives (a moving frame's own
  acceleration turned round)
- six times over: a point under the ground is put on it, and one that moved less than
  `GRIP_M` along it this step is put back where it was (without that a lying body creeps
  round, because putting a part back into shape after lifting a point turns it a hair, always
  the same way); every part is matched back into its own shape, turned to fit its points
  best (shape matching, with the rotation found by Müller's 2016 iteration from last step's
  answer); and any joint bent past its role's limit from rest is turned back about the joint.
  A point on the ground counts eight times over in the matching and a held point sixty times,
  so parts move to the floor and to the hand rather than the other way round
- a point on the ground loses most of its slide once a step and does not bounce

The parts share their joints, and nothing else holds the body together. Each part's turn
becomes the bone's angle in its parent's frame and the root joint's place becomes
`Pose::root`, so a limp body is drawn by the same `push_parts` as a living one. Between two
steps the angles are unwrapped onto the last ones, and where a joint jumps further than
drawing between them could make sense of (a turn passing through the pole of the angles) the
new pose is drawn without blending (`limp::follow`).

It runs in the animal's own frame: x and z turned with the yaw it had, y up, the origin at
`Look::pos`. The caller says where the ground is in that frame, and may move the frame
(`Limp::shift`) to keep `Look::pos` with the body.

What uses it:

- the deceased that a world starts with settle into `limp::settled` on flat ground, from
  standing, with a roll about their own length (`Limp::topple`) seeded per individual, so no
  two skeletons in a field lie the same way. A skeleton's rig has its own hull from the bone
  voxels (`bones::hulls`), so bones lie on bone
- anything can be held at one point (`Limp::hold`): a `Grip` is the scruff (the head's
  joint), the middle of the back, or any point of the body by number. The held point counts
  sixty times over and the rest hangs from it, swinging as the hand moves. A kill is held at
  the point nearest the hunter's mouth as the catch is made, on its neck, head or trunk
  (`Limp::nearest`, `carry::grab`), or nearest the talons on its trunk, so a rabbit taken
  from behind dangles by the rump with its head down, and one taken from the side hangs by
  the back. A deer, longer than a cougar's mouth is high, lies along the ground behind it and
  is hauled. Nothing chooses between carried and dragged
- a carcass on the ground is stepped until it is still (`creatures/carcass.rs`), and again
  each time something eating it tugs at it, and once more when it goes to bones, which lie
  lower than the body did
- a carcass thrown by a blast rides in the frame of the box `physics::tumble` moves
  (`creatures::flung`), with the box's acceleration turned round as `push`: in free fall the
  limbs float, and the landing throws them down onto the ground
- an animal whose entry says `sleeps_on_side` lies in `limp::on_side`: the body put over a
  quarter turn and every limb let down onto the ground, worked out once the first time it
  lies down
- a snake that drops out of a tree is held by the head at the frame's origin, which falls
  with the head, so the body trails above it

The shooter's bots still use the older `anim/ragdoll.rs` for the moment between dying and
their corpse (`shooter/corpse.rs`, points and sticks) taking over.

### Predation

A hunter that chases its prey (`walk.rs`, from the library's `hunts`) takes it at `CATCH_M`:
the prey loses its `Critter`, `Body` and any other mover, goes limp in the pose it was in,
and gains a `Carried`, a `Carcass` and `Age::fresh_kill`. It is held at the hunter's mouth as
the mouth is actually drawn (`carry.rs`).

The hunter walks it off for `CARRY_S`, then puts it down where it stands and eats it
(`Mood::Eat`): it stands over the carcass, faces it and holds its head down (`Act::Graze`),
and pulls at it now and then, for up to `EAT_S` or until the meat runs out. After that it is
not hungry again for `FULL_S`.

A `Carcass` holds its meat as seconds of one animal eating, 60 a metre of the animal's
height. Any walker that hunts that species and is hungry treats a carcass nobody is carrying
as prey that does not run: it walks over and eats when its snout is over it, beside whoever
else is already there. While something eats, the carcass does not rot to bones; when the meat
is gone it goes to bones at once (or, for an animal that leaves none, is gone), and one
nobody touches goes to bones after `CARCASS_S` as before. A shot animal is a carcass too, so
predators clean up after the player. A bird of prey holds its kill in its talons for
`4 × CARRY_S` and lets it drop; it does not come down to eat it, but a walker that hunts that
species may.

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
  and, in `limp.rs`, `every_creature_settles_in_one_piece` (no joint past its limit, nothing
  drawn into the floor), `skeletons_lie_down`, `held_by_the_scruff_it_hangs`,
  `held_anywhere_it_hangs_from_there`,
  `dragged_it_trails` and `asleep_on_its_side_the_legs_are_down`; in `slice.rs`, `parts_add_up_to_the_whole` (no voxel
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
`bot` builds into models of their own so a bot looks the same in every world). A `Figure` is display
only: it reads the body and never writes to it, and nothing that decides anything may read
it. See `docs/BOTS.md`.

One thing a bot wears is not in its library entry: the lamps that carry its health, a bar across
the top of the chest and another between the shoulder blades. Health is state rather than shape,
so `avatar.rs` draws them as their own instances and the `bot` entry holds only the dark bands
they sit in (rows 74 to 78). A bot is drawn as one of three children of `bot` (`bot_courier`,
`bot_hardsuit`, `bot_walker`, picked from its seed); `avatar.rs` finds each build's chest surface
on its voxels, so the bars follow a deeper or flatter chest without anyone moving an offset. The
builds are on the humanoid's 40x96x40 grid at 18.5 mm, so `avatar.rs` slices each part into a
32^3 window and bakes each rig (the whole body for a corpse, and the uniform and the rest for a
living bot) into smooth meshes with `models::bake_with`, as the loader does for a library
creature. `build` skips the bake for callers that only want the rig; `build_drawn` is for the
renderer. The walker's legs are reverse-kneed by shape alone: thigh forward to a knee in front
of the hip, shin back to a raised ankle, a long foot bone; the biped stride and crouch bend
them the same way as anyone's.

The zombie builds (`zombie_office`, `zombie_hiker`, `zombie_labourer`) come after the bots in
`Avatar::bodies` (`avatar.bots` says where), each whole, in a palette block of its own and with no
lamps (`Body::lit`); `Avatar::zombie_build(seed)` picks one. How a zombie walks is
`Figure::walk` (`anim/undead.rs`): `Shamble` (short dragging steps, head hung to one side, arms
dead, one foot scuffed), `Lurch` (one stiff knee swung round from a hitched hip, the stride bent so
that leg goes through fast, arms held out ahead), `Runner` (hard lean, head thrust out, arms
clawing in turn, a twitch) and `Crawler` (face down, dragging itself on its forearms, legs
trailing). Each is a pass over the stride, plus a step length and cadence the stride itself uses,
so the brain only sets velocity. The zombie round gives a crawler a 0.7 m collider
(`Player::stand_h`) and moves its hitbox forward over the head. `--zombie-sheet`
draws all four, standing and moving, as `--bot-sheet` does.

The upper arms and thighs of every people rig are `loose` bones (docs/CREATURES.md): baked as one
surface, the pelvis had no skin where a thigh met it, and a thigh swung forward showed the hole.
The consequence is worth knowing before you reach for the obvious tool: **`--edit bot_courier`
cannot show them.** The EDIT page draws the library entry, so it shows two dark bands and
nothing lit, and the one page that lets you turn a robot round is blind to the one part of it
that changes. Judging a lamp means a bot in a match, which means the headless recipe in
`docs/BOTS.md`.

They are high on the chest because nothing on a fighter's torso can be read: a weapon held in
both hands lies across the middle of the chest and the supporting arm crosses what is left.
Anything added later that a player is meant to read off a body has the same problem.
`brain::avatar::tests::the_lamps_sit_on_the_body` pins the placement, since a lamp a few
voxels out is simply inside the chest, drawing nothing, and nothing else would catch it.
