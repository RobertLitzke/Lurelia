# Reviewer brief

Fill in `{WORLD}`, `{SHOTS}` (one line per shot: its path and what the place is
meant to be) and `{DIRECTIONS}` (the owner's fixed directions), then hand the
text below the line to an Opus subagent.

---

You are reviewing a world for a voxel exploration game (repository
F:\Programming\3D_game, world `{WORLD}`). Do not edit any files; only look and
report.

Look at each screenshot with the Read tool, in order:

{SHOTS}

For context, skim `worlds/{WORLD}/story.md` (what the world is meant to hold).
Don't read any other files.

The owner's fixed directions, which your feedback must not contradict:

{DIRECTIONS}

Engine facts not to ask to change: voxels are 0.25 m; static boats sit at the
high-water height; underwater visibility is limited on purpose; far
geometry is coarse, so judge detail only from near shots.

Look for, in every shot:
- roads that end in nothing, run into water or walls, or have buildings on them;
- buildings or models that clip each other, the ground, trees or bones, or
  float or are half buried;
- waterfalls that look wrong (a sheet from nowhere, a fall in flat ground);
- rivers and channels that couldn't exist: canyons cut through flat marsh,
  water standing above its banks, a river from nowhere, straight ruled lines;
- shorelines: cliffs where there should be beach, ruled edges, bands of
  odd colour, terraces;
- broken or unrealistic models: wrong scale, missing parts, flat placeholder
  slabs, colours that read as the wrong material;
- animals in the wrong place or in impossible numbers;
- places that could lean harder into the world's look or story with a
  concrete addition.

Report, in plain prose and under 600 words, the most important problems,
ranked, each with the screenshot number, what is wrong, and a concrete fix a
world builder could make with data (move, resize, recolour, thin out, add or
remove something). Say in one or two sentences at the end what works well. Be
specific and honest; don't pad, and say so when a shot shows nothing wrong.
