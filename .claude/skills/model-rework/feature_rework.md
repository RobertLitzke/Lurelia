# Feature rework brief

The brief given to an agent asked to improve a landform or landmark that Rust
draws from a chronicle event rather than from a library entry: sea stacks and
karst towers (`tower`), the titan's bones (`skeleton`, `world/bones.rs`),
`mesa`, reefs, volcanoes and the like. Fill in the event kind (`{EVENT}`),
its source files (`{FILES}`), the worlds and instances to look at
(`{INSTANCES}`), what to change (`{FOCUS}`) and a build directory name
(`{TARGET}`), and hand over the text below the line.

One agent per event kind, since every instance and every world shares its
Rust file. Unlike the other briefs this one changes code and builds, so it
costs more; Sonnet is the default, Opus for a new event or a shape that
needs real design.

---

Improve how the `{EVENT}` chronicle event draws in this repository, the
Lurelia voxel engine. {FOCUS}

**Read only these:** the `{EVENT}` row of the event table in
`.claude/skills/worldgen/reference.md` (grep for `` | `{EVENT}` | ``), the
source {FILES}, and the `{EVENT}` events in the chronicles of these worlds:
{INSTANCES}. Grep `worlds/*/chronicle.json` for `"event": "{EVENT}"` to find
every world that uses it; you must not break any of them. Read other Rust
only where these files call into it.

**Data first.** Many looks are reachable by changing an instance's
parameters in its chronicle. Try that before changing code, and change code
only for what no parameter can do.

**When you change code:**
- Keep every existing field and its default, so worlds you weren't asked
  about draw as before unless the change is a fix they need. Put a new look
  behind a new field that defaults off, or say plainly that it changes every
  world.
- Document a new field in the struct's doc comment and in the event's row
  of the reference table.
- Keep it deterministic (seeded from the event, `noise::hash32`), and keep
  build time in mind: say if a world's load got noticeably slower.
- Touch only {FILES}, the chronicles you were given and the reference row.
  Other sessions edit this checkout at the same time; if a file you need
  changed under you, stop and report.

**Build** into your own directory, never `target/release`. First check
`df -h /f` and stop if under 10 GB free. Then
`cargo build --release --target-dir target/{TARGET}`, and run
`./target/{TARGET}/release/lurelia` for everything below. Run the tests
for your files with `cargo test --release --target-dir target/{TARGET} <module>`.

**Look before changing anything:** run `--gazetteer` on each world to find
each instance's point of interest, then take two or three shots of each
(near, from the side, one aerial) with
`--headless --world W --size 960x540 --pos X,Y,Z --look YAW,PITCH --no-hud --screenshot FILE`.
Keep these as the before set and take the after set from the same places.

**Cost:** add `--stats FILE` to the before and after shots of each
instance. The JSON's `quads_drawn`, `shadow_quads` and `frame_ms_p50` from
the same view tell you what the change costs; give them in your report, and
say what any count that grew more than 20% buys.

**Loop, at most four rounds:** change, build, `--validate` every world
that uses the event, shoot, fix what the shots show. Say what you would do
with more rounds.

Make no commits. When done, delete your build directory:
`rm -rf target/{TARGET}`.

**Report** in under 300 words: what changed in code and in each chronicle,
whether worlds you weren't asked about look different, new fields, the
validate and test output, and before/after shot paths for each instance.
