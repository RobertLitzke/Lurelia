# Creature migration brief (voxel rule and decimation)

The brief for moving one creature family onto the process in docs/CREATURES.md
"Model resolution and decimation". Before the agent starts, run
`python tools/creature_rescale.py ROOT VOXEL` yourself (the voxel from the rule
in that section), so the agent begins from a correctly sized model. Fill in the
root (`{NAME}`), a world it lives in (`{WORLD}`), its children (`{CHILDREN}`),
what the rescale did (`{RESCALE}`: old and new voxel and grid, and any
`--validate` problems it left) and anything particular to look at (`{FOCUS}`).
`{EXE}` is the engine binary the agent should run. One agent per family.

---

Remodel the voxel creature `{NAME}` (`library/creatures/{NAME}.json`) so it
looks good once decimated into a triangle mesh. {RESCALE} The voxel size is
final; don't change `voxel`. {FOCUS}

**Read only these:** in `docs/CREATURES.md`, the table under "A creature
entry" and the section "Model resolution and decimation"; the rig section of
`docs/ANIMATION.md`; the entry and its children: {CHILDREN}. Don't read the
Rust source or `library/CATALOGUE.md`.

**The format:** ops are in grid voxel coordinates, the model faces -Z, its
pivot is the bottom centre of the grid. Ops: `box`, `ellipsoid`, `segment`
(points and a radius), `cylinder`, `log`, `disc`, `set`, `recolour`,
`recolour_below`; later ops overwrite earlier ones; `mat` names a colour in
`colors`. Every voxel that moves with a bone carries that bone's name in
`part`. A grid bigger than 32^3 draws only the rig's parts, each in a 32^3
window, so every bone's voxels must fit 32 a side.

**Keep:** `voxel`, `half_w`, `height`, `behaviour`, `vary`, `ages`; every
bone's name and role, with `part` on every op of a moving piece (move a
`joint` to where its piece now hinges; add bones only to split one that
`--validate` says is too big); every colour name; and the places children's
`extra_ops` sit. Change a child only in its `extra_ops`, and only if the body
moved from under them.

**Aim for**, following "Modelling for decimation" in that section: the
animal's real proportions in few, large, overlapping shapes; `segment`s for
legs, necks and tails; no boxes on curved surfaces (feet, hooves, beaks,
ears become flattened ellipsoids); patterns by `recolour`; thin parts two
voxels thick where they join; eyes and markings two voxels or more. The
model now has more voxels than it was drawn for, so use them on the head,
legs and outline. Eyes are their own colour (add an `eye` colour if the coat colour would hide them), drawn as small ellipsoids set into the head surface, not as a `recolour` (which only finds voxels that happen to be there).

Set `"version"` one higher and rewrite `notes` if it is no longer true. The
original is already under `notes/replaced_creatures/`.

**Loop, at most four rounds:**
1. Write the file through libfmt: `python -c "import sys,json; sys.path.insert(0,'tools'); import libfmt; d=json.load(open(SRC)); open('library/creatures/{NAME}.json','w',encoding='utf-8',newline='\n').write(libfmt.render(d,'creatures'))"`.
2. `{EXE} --world {WORLD} --validate` until it reports nothing about this
   family. Other entries' problems are not yours.
3. Look at the decimated bake: `python -I tools/creature_lod/preview.py {NAME}`
   writes `shots/creature_lod/p_{NAME}.png` (voxel, 1/2 and 1/4 meshes;
   three-quarter, side, head, 64 px, 32 px). Judge the decimated columns: lumps,
   creases, bricks and parts that vanish. Run it for a child too
   (`preview.py CHILD`) if its `extra_ops` sit on something you changed.
4. Once in the last two rounds, check the rig:
   `{EXE} --headless --world {WORLD} --anim-sheet {NAME}:moves --size 960x540 --screenshot FILE`
   (a leg swinging must take its whole leg and nothing else).
5. Fix what the pictures show, then stop.

Touch no file but the entry and its children's `extra_ops`. Other agents share your scratch folder, so name your scratch files after your creature (`NAME_*.py`). Never run git (no checkout, restore or stash: the file's uncommitted rescale is the starting point, and other work in the checkout is not yours). Make no commits,
don't build with cargo, don't run `tools/bench_entry.py`.

**Report**, in under 150 words: what changed in the shapes, rig changes,
anything a child needed, the triangle counts `preview.py` printed, the
validate output, and the path of your final preview.
