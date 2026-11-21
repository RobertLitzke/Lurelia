# Structure viewer

A browser page that renders a `library/structures/*.json` file. Drop a file on
it to load it; drop another to replace it. Left-drag spins, wheel zooms,
right-drag pans.

The voxelizer is a port of `src/world/shape.rs`, so what you see is what the
engine compiles before placement (no rotation, decay, levelling or foundation).
Colours are each material's `texture.base`, from `palette.js`. Rerun
`python tools/structure_viewer/build_palette.py` after adding materials.

In Chrome or Edge the page watches the dropped file and reloads it when it
changes on disk, keeping the camera where it was. Firefox has no file handles,
so there you drop it again.

The overlay shows the `size` box, the ground at y = 0, the front (local -z) as
an orange arrow and the `anchor` as a blue dot. `carve` voxels are faint red.
"Cut height" hides everything above a row so you can see inside.

Open `index.html` directly, or serve the repo with
`python -m http.server 8765` and go to
`http://localhost:8765/tools/structure_viewer/` (the `.claude/launch.json`
entry `structure-viewer` does this).
