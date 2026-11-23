# New structure brief

The brief given to an agent asked to add new buildings to a world's towns. It
shares the rules of `structure_rework.md` and adds placement. Fill in the world,
the town event and the buildings (`{WORLD}`, `{TOWN}`, `{BUILDINGS}`) and hand
over the text below the line. One agent per world, since it edits that
world's chronicle.

---

Add new voxel buildings to `{WORLD}` in this repository for more
variety in its towns: {BUILDINGS}. They go in {TOWN}.

**Read only these:** `library/schema/structure.md` (the format and the review
checklist), `worlds/{WORLD}/story.md` if it exists (skim for period and
materials), the town's event in `worlds/{WORLD}/chronicle.json`, and two
existing buildings from that town in `library/structures/` as a guide to its
materials and slots. Don't read `library/CATALOGUE.md` or the JSON schema
whole; grep the catalogue for a material name, and check a material exists
with `ls library/materials/<name>.json`. Use existing materials only.

**Each new file:** `name` as given (check `library/structures/<name>.json`
doesn't exist), `"version": 1`, `"introduced_by": "{WORLD}"`, slots named like
the town's other buildings (`wall`, `roof`, `timber`, `floor`, ...) so the
town's slot overrides apply, tags like theirs, `decays: true`. Build it with
a real interior: a floor, an open door and rooms you can walk into.

**Scale:** voxels are 0.25 m; the player is 1.75 m. Doors at least 2.25 m by
1 m, storeys about 3 m, railings 1.1 m. Only a real door leaf uses a
`door_*` material. Footprint about the size of the town's existing buildings,
so they fit its plots.

**Decay:** `roof`, `cloth`, `glass` fail in sections, `floor` never decays,
everything else crumbles from the top at one rate. Draw it whole and give
every op a part.

**Notes:** one or two plain sentences with real dimensions, which way it
faces (front is local -Z) and what it carves.

**Placing:** {PLACING}

**Loop, at most three rounds per building:**
1. Write the file through libfmt: `python -c "import sys,json; sys.path.insert(0,'tools'); import libfmt; d=json.load(open(SRC)); open('library/structures/NAME.json','w',encoding='utf-8',newline='\n').write(libfmt.render(d,'structures'))"`.
2. `python tools/check_structure.py library/structures/NAME.json` until 0 errors and no warnings you can't justify.
3. `python tools/shoot_structure.py NAME --max 1` and look at the one contact sheet it prints. Don't open the single views. Take a close shot only if one detail needs it, at small size:
   `./target/release/lurelia --headless --world {WORLD} --size 800x450 --pos X,Y,Z --look YAW,PITCH --no-hud --screenshot FILE`.
4. Fix what the sheet shows, then stop.

Run `./target/release/lurelia --world {WORLD} --validate` after the chronicle
edit and at the end. Touch no other file, make no commits, don't build with
cargo.

**Report** in under 150 words per building: what it is, size, the check
output, the sheet path, and the chronicle change.
