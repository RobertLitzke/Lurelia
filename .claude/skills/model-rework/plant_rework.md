# Plant rework brief

The brief given to an agent asked to improve how library plants look. Fill in
the plants (`{PLANTS}`), a world each grows in (`{WORLD}`) and what to look at
(`{FOCUS}`), and hand over the text below the line. Give one agent the plants
of one form or one biome, two to four of them, so it compares like with like.

A plant is a set of numbers over a form that Rust draws (`src/world/flora.rs`).
This brief changes the numbers only. If a plant needs a shape no form or knob
can give, the agent says so, and that becomes a `feature_rework.md` job.

---

Improve how these voxel plants look in this repository: {PLANTS}. {FOCUS}

**Read only these:** the plants section of the world reference,
`sed -n '/^### plants/,/^### structures/p' .claude/skills/worldgen/reference.md`
(every field and form), each plant's file in `library/plants/`, and one other
plant of the same `form` as a guide. Don't read the Rust source,
`library/CATALOGUE.md` or the JSON schema whole. Grep the catalogue for a
plant's name to see which worlds and biomes use it, and check a material
exists with `ls library/materials/<name>.json`. Use existing materials only.

**Keep:** `name`, `introduced_by`, the placement fields (`max_slope`,
`standing_water`, `water_depth`, `passable` and any other that decides where
it grows), and whether it is deciduous (an `autumn` colour or none). Change
`form` only if another form draws this plant much better, and say why.
Keep `height` within what the real plant reaches. Set `"version"` one
higher and rewrite `notes` if what it says is no longer true. Keep the
original at `notes/replaced_plants/NAME_v<old>.json` before the first edit.

**Aim for** the real plant's outline and habit, readable at 10 to 30 m: the
trunk-to-crown proportion, how the limbs leave the trunk, how the foliage is
massed. Big old trees want thick trunks and a `buttress`. Mind the cost: a
plant grows by the hundred, so keep stems, canes and thin limbs modest (the
reference gives quad counts for canes).

**Loop, at most four rounds:**
1. Write the file through libfmt: `python -c "import sys,json; sys.path.insert(0,'tools'); import libfmt; d=json.load(open(SRC)); open('library/plants/NAME.json','w',encoding='utf-8',newline='\n').write(libfmt.render(d,'plants'))"`.
2. `./target/release/lurelia --world {WORLD} --validate` until it reports
   nothing about these plants.
3. Look at it:
   - three individuals in the studio: `VG_EDIT_ROLL=R ./target/release/lurelia --headless --world {WORLD} --edit NAME --size 960x540 --screenshot FILE` for R = 0, 1, 2 (one tree in front, the entry beside it; each roll is another tree of the same plant). Paste the three side by side with PIL, cropped to the left 540 px, and look at that one image;
   - a deciduous plant once more with `--season winter` on roll 0.
   Take the same three rolls of the original before your first edit, so you
   can compare.
4. Fix what the shots show, then stop. Look at every final shot before you
   report it. Say what you would do with more rounds.

A plant with `"like": "NAME"` inherits every field it doesn't set: grep
`library/plants/` for it, and look at one roll of each such child at the end.

Run `--validate` once at the end on every world the catalogue lists for each
plant. Touch no file but the plants and their copies under
`notes/replaced_plants/`. Make no commits, don't build with cargo.

**Cost:** once at the end, `python tools/bench_entry.py plants NAME` (about ten minutes; it compares your version with the one under `notes/`, a hundred of each on a flat test world). Give the quads drawn, cards drawn and frame p50 rows of its `walk` table in your report. If a count grew more than 20%, say what the extra buys, or take some of it back.

**Report**, with the cost rows, in under 150 words per plant: what changed (old -> new for each
field), the validate output, the paths of your final shots, and any shape the
forms couldn't give.
