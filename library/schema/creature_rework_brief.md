# Creature rework brief

The brief given to an agent asked to improve the model of one library creature.
Fill in the creature (`{NAME}`), a world it lives in (`{WORLD}`), the entries
that inherit from it (`{CHILDREN}`) and what to look at (`{FOCUS}`), and hand
over the text below the line. One agent per creature family, since children
inherit the parent's ops.

---

Improve the voxel model of the creature `{NAME}` in this repository
(`library/creatures/{NAME}.json`). {FOCUS}

**Read only these:** `docs/CREATURES.md` down to "The hours they keep" (the
entry format), the rig section of `docs/ANIMATION.md` for its rig plan, the
entry itself and its children: {CHILDREN}. Don't read the Rust source or
`library/CATALOGUE.md` whole.

**The format:** the model is drawn on a 32^3 grid of voxels `voxel` metres on
a side, facing -Z, its pivot the bottom centre (16, 0, 16). Ops are those of
structures (`box`, `ellipsoid`, `cylinder`, `log`, `disc`, `segment`, `set`)
plus `recolour_below` and `recolour`; later ops overwrite earlier ones and
`mat` names a colour in `colors`. Every voxel that moves with a bone carries
that bone's name in `part`; untagged voxels move with the root.

**Keep:**
- `voxel`, `half_w`, `height`, `behaviour`, `vary`, `ages` and the rig plan,
  so it moves, collides and behaves as before;
- every bone's name and role, and a `part` on every op of a moving piece.
  Move a bone's `joint` to where that piece now hinges on the body;
- every colour name, since children recolour them by name. You may add
  colours; a child that doesn't name one inherits yours;
- the place its children's `extra_ops` sit (a mane on the neck, moss on the
  shell): if the body moves, the child's additions must still sit on it.
  Change a child only if that can't be kept, and then only its `extra_ops`.

**Aim for** the animal's real proportions and the features that make it read
at a glance from 5 to 10 m, in a few colours with clear patches rather than
noise. Round shapes (`ellipsoid`, `cylinder`, `log`) over stacked boxes; thin
pieces at least one voxel wide so they don't vanish. Put the detail where the
player looks: head, legs, outline.

Set `"version"` one higher and rewrite `notes` if what it says is no longer
true. Keep the original at `notes/replaced_creatures/{NAME}_v<old>.json`
before the first edit.

**Loop, at most four rounds:**
1. Write the file through libfmt: `python -c "import sys,json; sys.path.insert(0,'tools'); import libfmt; d=json.load(open(SRC)); open('library/creatures/{NAME}.json','w',encoding='utf-8',newline='\n').write(libfmt.render(d,'creatures'))"`.
2. `./target/release/lurelia --world {WORLD} --validate` until it reports
   nothing about this entry or its children.
3. Look at it:
   - the studio, side and three-quarter: `./target/release/lurelia --headless --world {WORLD} --edit {NAME} --size 960x540 --screenshot FILE` (the model stands in front, the entry is on the right);
   - the family: `--anim-sheet {NAME}:family` with the same flags, to check the children;
   - the postures and gaits: `--anim-sheet {NAME}:moves`, to check the bones still move the right pieces (a leg swinging must take its whole leg and nothing else).
   Crop or zoom a shot with PIL if the model is small in it.
4. Fix what the shots show, then stop.

Touch no file but the entry, its children's `extra_ops` if needed, and the
copy under `notes/replaced_creatures/`. Make no commits, don't build with
cargo.

**Report** in under 200 words: what changed, the rig changes (joints moved,
bones added), anything a child needed, the validate output, and the paths of
your final shots.
