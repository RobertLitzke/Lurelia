# Town judge brief

Given to a fresh subagent once per iteration, with the iteration's directory
filled in. Keep it fixed so scores are comparable.

---

You are judging the output of a procedural town generator for a voxel game.
Three places, each seen from straight overhead (1800x1800 px, roughly 0.3 to
0.45 m per pixel, north up) and from a low angle:

- `{DIR}/portmouth.png` and `{DIR}/portmouth_low.png`: a port city of about
  330 m radius where a river meets the sea, with farmland round it.
- `{DIR}/lynford.png` and `{DIR}/lynford_low.png`: a small market town of
  about 150 m radius on a river, with farmland round it.
- `{DIR}/oakfield.png` and `{DIR}/oakfield_low.png`: a modern suburb of about
  170 m radius, detached houses each in its own garden, with farmland round
  it.

The low-angle shots show detail the overhead ones can't: from overhead the
renderer draws everything at a coarse level of detail, so roofs look stepped
there. For a sense of what good looks like, also look at the reference maps
and photos in `shots/towns/refs/`: `nyc_1807.png`, `rome.png`, `gent.png`
(street patterns), `stone_wall.png` and `rail_fence.png` (field boundaries).
These are references for plausibility and character, not targets to copy.

Score each place from 1 to 10 on:

- **Plausibility**: would a planner or historian believe this place grew or
  was laid out this way, for what it is (a port, a market town, a suburb)?
  Where it sits against the water and the roads, how density changes from
  the centre and along the roads to the country, street hierarchy, how
  buildings meet streets, how the roads lead out, whether fields make sense.
- **Beauty**: is it pleasing to look at, overhead and low? Rhythm, density,
  variety without noise, clear structure, colour.

Be specific and concrete. Point at places in the image. Name the three
changes that would most improve each place, most important first. Do not
suggest things the screenshots can't show. Be a demanding critic: a 7 should
mean a place that would pass in a shipped game.

Reply in exactly this form, and nothing else:

```
PORTMOUTH plausibility N/10, beauty N/10
- remark
Fixes: 1) ... 2) ... 3) ...

LYNFORD plausibility N/10, beauty N/10
...

OAKFIELD plausibility N/10, beauty N/10
...

OVERALL: one or two sentences: what the generator does well and its single
biggest weakness.
```
