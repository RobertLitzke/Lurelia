---
name: model-rework
description: Rework or add voxel models and landmarks in the 3D_game engine by handing a short, fixed brief to Sonnet subagents. Covers reworking existing structures, adding new buildings to a world's towns, improving a creature's (animal's) model, improving how plants and trees look, and improving landforms that Rust draws from a chronicle event (sea stacks and towers, the titan's bones, mesas, reefs). Use when asked to make any of these richer, more realistic or better-looking, or to add buildings to a town.
---

# Reworking voxel models with subagents

Each kind of job has a brief in this folder. The brief is the whole prompt the
subagent gets: fill in its placeholders, then pass the text below its `---`
line as the agent's prompt, unchanged otherwise.

| Job | Brief | Placeholders | Agents |
|---|---|---|---|
| Rework existing structures | `structure_rework.md` | `{STRUCTURES}` (names and their worlds) | two to four structures per agent |
| Add new buildings to a town | `new_structure.md` | `{WORLD}`, `{TOWN}`, `{BUILDINGS}`, `{PLACING}` | one per world, since it edits that world's chronicle |
| Improve a creature's model | `creature_rework.md` | `{NAME}`, `{WORLD}`, `{CHILDREN}`, `{FOCUS}` | one per creature family, since children inherit the parent's ops |
| Improve plants | `plant_rework.md` | `{PLANTS}`, `{WORLD}`, `{FOCUS}` | two to four plants of one form or one biome per agent |
| Improve a code-drawn feature | `feature_rework.md` | `{EVENT}`, `{FILES}`, `{INSTANCES}`, `{FOCUS}`, `{TARGET}` | one per event kind, since every world shares its Rust file |

The briefs are short on purpose. Every file an agent reads and every picture
it looks at is paid for again on each later step, so they name only what the
work needs and cap the rounds. Don't add reading to them, and don't swap in an
open-ended prompt: measured on 2026-11-20, Opus took 150k-190k tokens per
structure on an open-ended brief, Sonnet on this one about 95k, and Sonnet's
results were better. Plants, first run on 2026-11-22: one Sonnet agent
reworked oak, beech and maple in about 96k tokens and 8 minutes. The oak
improved a lot and the maple some; the beech barely changed, and the agent
looked at few of its own final shots, which the brief now requires. Second
round, one agent each: red_mangrove 67k tokens (clearly better), banyan 77k
(switched to the mangrove form for its roots; +66% quads, and its children
inherit the form), coyote at half-size voxels 109k (5.4x model quads, no
measurable frame cost). The
feature brief hasn't been run yet; note its tokens and quality here.

Structures, creatures and plants are data: those agents edit JSON and never
build. A feature is drawn by Rust, so its agent changes code and builds into
its own `target/{TARGET}`. A plant agent that finds a shape no form can give
reports it, and that becomes a feature job on `src/world/flora.rs`.

## Cost

`tools/bench_entry.py KIND NAME` measures a plant, creature or structure
before and after: it builds a flat test world in a sandbox folder (a copy of
the library, so nothing in the checkout changes), puts about a hundred of the
entry on it (`--count`), and renders three views with each version: eye
level, aerial and the `field` walking loop. The table it prints, and writes
to `shots/bench/<kind>_<name>/report.md` with the shots, has exact counts
(quads, leaf cards, model quads, draw calls) and median frame times over
`--reps` runs. The counts are what to judge by; frame times move a few
percent run to run. Three reps take about ten minutes. The rework briefs
run it once at the end and report the rows that matter for their kind.

First run, oak v1 -> v2 (85 trees): quads drawn +24%, leaf cards +26%,
shadow quads +123%, walking frame p50 3.96 -> 4.21 ms.

## Running it

1. Fill the placeholders from the chronicle and the library:
   - structures: grep `worlds/*/chronicle.json` for where each is placed;
   - creatures: grep `library/creatures/*.json` for `"parent"` to find children;
   - plants: grep `library/CATALOGUE.md` for the worlds and biomes using each;
   - features: the event file is `src/world/chronicle/events/<event>.rs`, plus
     any helper it calls for the shape (`world/bones.rs` for `skeleton`). Grep
     the chronicles for `"event": "<event>"` for the instances. Give
     `{TARGET}` a short name, such as `rework_<event>`.
2. Launch the agents with the Agent tool, `model: "sonnet"`. Use Opus only
   for a landmark or event that needs real design work. Agents working on
   different files can run in parallel; two agents never share a file, a
   chronicle or a build directory. Before a feature agent, check `df -h /f`:
   each build directory is 1.5 to 3 GB.
3. The data agents use `target/release/lurelia`, which must be current before
   they start; build it first if the engine changed.
4. Read each report and look at the shots it names before telling the user
   it's done. Collect what the agents left for the user to decide. Check a
   feature agent deleted its build directory.
5. Afterwards run `python tools/libfmt.py --catalogue` so
   `library/CATALOGUE.md` reflects the new versions and notes.

Originals of data entries are kept under `notes/replaced_structures/`,
`notes/replaced_creatures/` and `notes/replaced_plants/`, so a rework the
user dislikes can be put back. A feature's code change is undone with git.
