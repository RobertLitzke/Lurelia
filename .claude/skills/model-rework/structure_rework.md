# Rework brief

The brief given to an agent asked to rework existing structures. Fill in the
names and worlds (`{STRUCTURES}`) and hand over the text below the line. It
is kept short on purpose: every file an agent reads and every picture it
looks at is paid for again on each later step, so the brief names only what
the work needs and caps the rounds.

Measured on 2026-11-20: Opus spent 150k-190k tokens per
structure with an open-ended brief, Sonnet about 95k. Sonnet is the default
for this brief; use Opus for a landmark that needs real design work.

---

Rework these voxel structures in this repository so they are richer
and better-looking at the right human scale: {STRUCTURES}.

**Read only these:** `library/schema/structure.md` (the format and the review
checklist), each structure's current file in `library/structures/`, and its
place event in `worlds/<world>/chronicle.json` (`age`, `bearing`, `slots`).
Don't read `library/CATALOGUE.md` or the JSON schema whole; grep the
catalogue for a material name if you need one, and check that a material
exists with `ls library/materials/<name>.json`.

**Keep:** `name`, the slot names (add slots freely, use every one, existing
materials only), `base`, `level`, `decays`, `tags`. If you change `size`,
keep the `anchor` on the same physical spot. Set `"version": 2`. If a `sign` event in the chronicle paints text on it, keep that board's size and place and leave it plain.

**Scale:** voxels are 0.25 m; the player is 1.75 m. Doors at least 2.25 m by
1 m, storeys about 3 m, railings 1.1 m. Only a real door leaf uses a
`door_*` material.

**Decay:** `roof`, `cloth`, `glass` fail in sections, `floor` never decays,
everything else crumbles from the top at one rate. Draw it whole unless the
current file draws the damage, and give every op a part.

**Notes:** one or two plain sentences with real dimensions, which way it
faces (front is local -Z) and what it carves.

**Loop, at most three rounds per structure:**
1. Write the file through libfmt: `python -c "import sys,json; sys.path.insert(0,'tools'); import libfmt; d=json.load(open(SRC)); open('library/structures/NAME.json','w',encoding='utf-8',newline='\n').write(libfmt.render(d,'structures'))"`.
   Back up the original first to `notes/replaced_structures/NAME_v1.json` if that copy doesn't exist.
2. `python tools/check_structure.py library/structures/NAME.json` until 0 errors and no warnings you can't justify.
3. `python tools/shoot_structure.py NAME --max 1` and look at the one contact sheet it prints (four views in one image). Don't open the single views. Take a close shot only if one detail needs it, at small size:
   `./target/release/lurelia --headless --world W --size 800x450 --pos X,Y,Z --look YAW,PITCH --no-hud --screenshot FILE`.
4. Fix what the sheet shows, then stop. Three rounds is the limit; say what you would do with more.

Run `./target/release/lurelia --world W --validate` once at the end. Touch no
other file, make no commits, don't build with cargo.

**Cost:** once at the end, `python tools/bench_entry.py structures NAME` (about ten minutes; it compares your version with the one under `notes/`, a hundred of each on a flat test world). Give the quads drawn, shadow quads and frame p50 rows of its `walk` table in your report. If a count grew more than 20%, say what the extra buys, or take some of it back.

**Report**, with the cost rows, in under 200 words per structure: what changed, new size and
notes, the check output, the sheet path, anything for the user to decide.
