# Reviewing a structure file

A short form of `structure.schema.json` for an agent reviewing a file in
`library/structures/`, followed by what to check by hand. The schema is the
authority when the two disagree.

## Before reading

Run the checks a machine can do, so the review is spent on the rest:

- Validate the file against `structure.schema.json` with any JSON Schema
  (draft 2020-12) validator. It catches unknown keys, wrong types, unknown
  ops, a bad `base` or `axis`, and arrays of the wrong length.
- `lurelia --world NAME --validate` checks names: every slot's material and
  every op's `mat` must be a slot, a material, `air` or `carve`.

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
| `base` | `sea` (y = 0 one metre under sea level), `rim` (anchor at the highest ground, for spans), `bed` (sunk into the sea bed) |
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
| `segment` | `points` (two or more), `radius`: rods, ropes, braces at any angle |
| `dish` | `vertex`, `radius`, `depth`, `thick` (1.5), `tilt`, `yaw` |
| `repeat` | `count`, `step`, `ops`: run k is shifted by k * `step` |
| `recolour`, `recolour_below` | creature models only |

`mat` is a slot, a material, `air` (empties the structure's voxels, leaves
the terrain) or `carve` (removes the terrain too). `part` decides decay:
`roof`, `cloth`, `glass` fail in sections; `timber`, `frame`, `wall`,
`stone`, `metal` crumble from the top; `floor` mostly stays. Any other part,
or none, never decays.

## What to check

- Everything drawn lies inside `size`, including the top of the roof:
  clipping is silent.
- `anchor` is inside `size`. For `base: "rim"` its y is the row that rests
  on the highest ground under the footprint (a bridge's deck).
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
