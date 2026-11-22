# tools/course

Kart circuits written as `.course` text files and built into race worlds. The
format is in `FORMAT.md`, which is also the brief to hand an agent that is to
design one.

```bash
python -I tools/course/course_file.py tools/course/maps/kestrel.course
cargo run --release -- --world kestrel
```

`course_file.py` writes `worlds/<name>/` from the layout: the land as a
`terrain` event, the road, kerbs and walls as a `circuit` event
(`src/world/chronicle/events/circuit.rs`), and a `race` block in world.json
that `src/race.rs` runs. `--check` reports on a layout without writing it.

`maps/` holds the courses: `kestrel.course`, the Kestrel Ice Ring, and
`peregrine.course`, Peregrine Ridge on the chalk downs.
