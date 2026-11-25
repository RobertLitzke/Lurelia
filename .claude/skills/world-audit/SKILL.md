---
name: world-audit
description: Audit a finished or nearly finished 3D_game world for the faults a player notices: roads attached wrong or buildings standing on them, buildings and models that clip each other or the ground, ugly waterfalls, rivers and channels that couldn't exist, bad shorelines, broken or unrealistic models, animal numbers that crash or explode, predators that never hunt, and missed chances to lean into the world's aesthetic. Runs the engine's automated checks and an animal soak, shoots 10-25 places of interest, has an independent subagent review the shots against the world's brief, then verifies and fixes what holds up. Use when asked to audit, review, check over or polish a world, or after building one.
---

# Auditing a world

An audit has four parts: what the engine can check by itself, what an animal
soak shows, what a reviewer sees in screenshots, and the fixing. The reviewer
is a separate subagent that hasn't seen your reasoning, because the builder
stops seeing their own world's faults. Its report is a lead, not a verdict:
verify each claim on the shot before acting.

Use one build for the whole audit (`target/<name>`, kept until the audit is
done; see the project CLAUDE.md). `BIN` below is that binary.

## 1. Automated checks

Run and read, don't skim:

```bash
$BIN --world W --validate          # must say "no problems"; also lists every waterfall
$BIN --world W --gazetteer > gaz.txt
$BIN --world W --seam seam.png     # only for a world with neighbours
```

In the gazetteer look for:
- `warn`/`problem` lines, and `town ...: no site for landmark`.
- `road gap to bridge` and routes with `0 segments` (a road that found nothing).
- town lines: `refused ... steep/shallow` counts far above the built count mean
  the town's ground is wrong for it.
- `budgets: N of 238 material ids`: within 5 of the limit is a finding.
- every structure in the structure list whose `h` (ground under it) is below
  sea level but whose `base` isn't `sea` or `bed`: a building in the water.
- `place` events dated after the `town` or `village` that surrounds them: the
  town grew first, so the later building may stand on its street. Put a
  town's own works (quays, mills, landmarks the story names) before it.

## 2. Animals

```bash
python tools/world_audit.py W --bin $BIN --soak 480
```

Twenty game days of the herd and pack records. Findings: a species that
crashes or explodes, predators with no kills, a common herd animal nothing
hunts, a predator listed where its prey isn't (check `hunts` in its entry),
and counts that don't fit the ground (a hundred elk on a strip of dunes). Fix
by biome counts, by adding the predator a habitat lacks, or by moving a
species to the biome that suits it. Live behaviour (a display, a chase) needs
the windowed game or `--eco-pin`; say so rather than claim it.

## 3. Screenshots

Pick 10-25 views. Each must be a place a player will see, from where a
player sees it:
- the spawn view, exactly as the game opens (a view named `spawn`);
- every town: one oblique from 60-100 m and one at street level (`ground`);
- every waterfall the validate list names, every river mouth, the main
  coast, every harbour or quay;
- every named landmark and point of interest in the story;
- underwater places, from 3-6 m above the bed, not from inside rock;
- one or two aerials over the whole world;
- a seam with each neighbour, from the ground.

Overhead shots from high up show coarse distant geometry, so judge buildings
and models from low obliques. Platforms (lifts, ferries) are placed only once
the spawn floor streams in, so a view far from the spawn never shows them;
move the spawn temporarily (and restore it) to shoot one. Write the views as
a JSON list (format in `tools/world_audit.py`) and shoot them:

```bash
python tools/world_audit.py W --bin $BIN --views views.json --out shots/audit/W
```

Look at every shot yourself before handing them on; a shot of the inside of
a rock or of open sky is wasted on the reviewer, so fix the camera first.

## 4. Independent review

Launch one subagent (Opus) with the brief in `reviewer.md` in this folder,
filled in: the shot list with one line on what each place is meant to be, the
world's `story.md`, and the owner's fixed directions from the conversation
(what the user has asked for, which the reviewer must not contradict). It
only reads and reports.

## 5. Verify, fix, record

- Check each finding against its shot. Reviewers misread scale and miss what
  is outside the frame; drop what doesn't hold up and say why.
- Fix what is data: biome counts and plants, materials, structure slots,
  chronicle order and positions, models (through `model-rework` for big
  jobs). Never change a library entry another world uses without making a
  `_v2` or asking.
- What needs engine work, or what the owner's directions rule out, goes in
  `F:\Programming\BACKLOG.md` (one row per world, kept current) instead.
- Re-shoot every place you changed and look again.
- Report: what the automated checks found, the soak table, the review's
  findings with what you did about each (fixed, dropped and why, backlogged),
  and what remains unverified.
