# tools/course

Kart tracks written as `.track` text files and built into race worlds. The
format is in `FORMAT.md`, which is also the brief to hand an agent that is to
design one.

```bash
python -I tools/course/track_file.py tools/course/maps/peregrine.track --check   # report only
python -I tools/course/track_file.py tools/course/maps/peregrine.track           # build
cargo run --release -- --world peregrine
```

`track_file.py` resolves a layout into `worlds/<name>/track.json`: the road a
sample a metre (height, bank, width, walls, run-off, decks), the race
furniture (pickup rows, boost pads, jumps, checkpoints), and the `add` and
`carve` shapes as voxels. The engine stamps it after the world's own
chronicle (`src/world/chronicle/race_track.rs`), so the track is exact
whatever the world does round it. The tool also writes `ground.bin`, the land
eased to meet the road, and the first time a starter world (world.json,
chronicle.json, story.md, look.json) for the style in `styles/`.

`shapes.py` rasterises the arena format's shapes (`box`, `prism`, `section`,
`pyramid`, `stairs`, `ramp`) and cuts them into box ops.

`maps/` holds the tracks: `peregrine.track`, Peregrine Ridge on the chalk
downs; `vermilion.track`, Vermilion Mesa, a figure-eight in red rock country
with a long tunnel under its own road; and `ribstrand.track`, Ribstrand, a
lap of a grey northern beach and its dunes.
