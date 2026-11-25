# Reviewing a structure file

A short form of `structure.schema.json` for an agent reviewing a file in
`library/structures/`, followed by what to check by hand. The schema is the
authority when the two disagree.

## Before reading

Run the checks a machine can do, so the review is spent on the rest:

- `python tools/check_structure.py FILE` validates against the schema (when
  `jsonschema` is installed) and checks materials, unused slots, ops drawn
  outside `size`, the anchor, door materials and leaf heights, sizes in the
  notes against what the file measures, and canonical form. Given a file
  outside the library, it compares it with the library entry it would
  replace and lists what changes.
- `lurelia --world NAME --validate` checks names across the whole world.
- `python tools/shoot_structure.py NAME` photographs every placement of the
  structure in the worlds that use it, from three sides and above, into
  `screenshots/structures/NAME/`. For a feature a biome scatters (rocks,
  beds, logs) pass `--world`: the chronicle doesn't name it, but the
  gazetteer's "Natural features" list says where it stands. The release binary reads the library at
  load, so it shows an edited file without a rebuild.

What no tool can judge: whether it looks right, whether its scale fits the
player (1.75 m) and the buildings round it, and whether its place in the
world suits it. That is what the pictures are for.

If the file has `"_generated_by"`, it is written by that script: review the
script, not the file, since an edit to the file is lost the next time the
script runs.

## Format

Units are voxels, 0.25 m each (4 per metre). y is up and y = 0 is ground
level. The front faces local -z; placement turns it toward the event's
`bearing` or `face`. Integer boxes are `from` inclusive, `to` exclusive.
Float ops measure to voxel centres, which sit at +0.5.

Top-level keys (required: `name`, `size`, `ops`):

| key | meaning |
|---|---|
| `name` | same as the file name |
| `version`, `introduced_by` | 1, and the world that added it |
| `_generated_by` | the script that writes the file, if one does; otherwise the file is the source |
| `notes` | one sentence: what it is, which way it faces, what it carves |
| `size` | bounding box [x, y, z]; anything drawn outside is clipped |
| `anchor` | the voxel that lands on the placement point (default origin) |
| `foundation` | voxels under y = 0 filled down from the bottom row where the ground falls away |
| `slots` | slot name -> material; ops name slots so a world can reskin |
| `ops` | the shape program, run in order; a later op overwrites earlier voxels |
| `tags` | labels; the engine reads `ruin` and `anomaly` |
| `decays` | weathers with age (default true); false for rock and logs |
| `level` | ground levelled under it (default true); false settles it into the lowest ground |
| `base` | `sea` (y = 0 one metre under sea level), `rim` (anchor at the highest ground, for spans; a bridge is seated on its banks, see below), `bed` (sunk into the sea bed) |
| `vines` | 0 to 1, how readily vines cover its walls |

Ops, by their `op` value. All but the recolours and `repeat` take `mat` and
an optional `part`.

| op | fields |
|---|---|
| `box` | `from`, `to` |
| `walls` | `from`, `to`, `thick` (1): the four sides only, no floor or lid |
| `gable` | `from`, `to` (same y, the eaves), `axis` (ridge direction, x or z), `rise` (1 = 45 degrees, 2 steeper) |
| `ellipsoid` | `centre`, `radius` [x, y, z] |
| `cylinder` | `base` (centre of the bottom), `radius`, `height` |
| `log` | `base` (centre of one end), `radius`, `length`, `axis`, `yaw` |
| `disc` | `centre`, `radius`, `thick`, `axis`, `yaw` |
| `set` | `at`: one voxel |
| `segment` | `points` (two or more), `radius`: rods, ropes, braces at any angle. Fills only air, so it never cuts what it touches, and its tube hides only the voxels it drew: a brace may share the pole's material |
| `dish` | `vertex`, `radius`, `depth`, `thick` (1.5), `tilt`, `yaw` |
| `repeat` | `count`, `step`, `ops`: run k is shifted by k * `step` |
| `recolour`, `recolour_below` | creature models only |

`mat` is a slot, a material, `air` (empties the structure's voxels, leaves
the terrain) or `carve` (removes the terrain too). `part` decides decay:
`roof`, `cloth`, `glass` fail in sections; `floor` never decays; everything
else (`timber`, `frame`, `wall`, `stone`, `metal`, any other name, or none)
crumbles from the top of each column, all at the same rate. So an op that
must survive age whole (an iron kettle set in brick) needs `floor`.

## Bridges

A structure with `base: "rim"` and the tag `bridge` is seated on its banks
by the `place` event (`chronicle/events/place.rs`, `fit_bridge`) instead of
hung from the highest ground under its footprint:

- Its long axis is local X or Z, whichever is longer, and the deck is
  centred on the anchor across that axis. The tips are the first and last
  slice along it.
- The walking surface at each tip is read from the model: the first air
  above the first solid voxel in the tip column on the anchor's line. So that
  column must be solid from the bottom up to the deck (an abutment or the
  deck itself), with air above the deck.
- Without a `height` on the `place` event, the deck goes to the mean of the
  ground just off the two tips. With one, it stays there (rope bridges, a
  chain of `bridge_bay`s).
- The ground at each tip is cut or filled to the deck from 2.5 m under the
  abutment out along a ramp of about 1 in 5 (6 to 24 m), as wide as the deck
  plus a 3 m feather. An end more than 5 m off its ground (a cliff, a canyon
  rim) is left alone.
- The nearest graded road (a `network`, `route` or `ribbon` kit) or town
  street on each bank within 45 m of a tip is carried on to the tip in its
  own surface, easing from the road's height to the deck's. A road on the
  far side of the water, or one whose join would cross a house, is not taken.
- The deck and 8 m off each tip are marked taken, so a later town builds no
  house across the bridge head and later walls and fences leave a gap there.

So make the bridge long enough to land on dry ground at both ends with a
few metres to spare: a tip standing in the river gets no ground and no
road. In the chronicle, end the road's links at the two tips (or within
45 m of them) rather than routing the road across the water beside it.

## What to check

- Everything drawn lies inside `size`, including the top of the roof:
  clipping is silent.
- `anchor` is inside `size`. For `base: "rim"` its y is the row that rests
  on the highest ground under the footprint (a bridge's deck). A bridge
  follows the rules in "Bridges" below.
- Order: an `air` or `carve` op comes after the solids it cuts, and nothing
  after it fills the hole again.
- Doors and windows cut the full wall thickness: an opening in `walls` with
  `thick: 2` is at least 2 voxels deep.
- The door is on the local -z side, or the notes say where it is.
- A `mat` that is meant to be a slot is listed in `slots`. A misspelt slot
  that happens to match a material name passes `--validate` and draws that
  material.
- Every slot in `slots` is used by some op.
- Built things carry a `part` on every op that should weather. Natural
  features have `decays: false` and `level: false`.
- A `gable`'s `from` and `to` share their y, and its span fits on the walls
  under it.
- Several near-copies at different lengths (`thing_20`, `thing_30`) should
  be one kit (`library/README.md`, "When a kit, not a structure").

A file that passes all of this can still look wrong; render it before
calling it done (worldgen skill, screenshots).
