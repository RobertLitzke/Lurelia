# The look: every value on the LOOK page

F7 in a world, or LOOK AND COLOUR in the pause menu. Everything here is live:
the frame behind the page answers on the next frame, and EXPORT writes the
result to `library/looks/<style>.json`. There is one of these files per style
(`saturated`, `classic`), and loading a world reads it again, so **export
before you switch worlds**.

This file is the long version. GUIDE.md has the short one, and
`src/render/grade.rs` is where the values live in code.

## Getting around

| | |
|---|---|
| Up, down | Move between lines |
| Left, right | Change the value on the line (held, it keeps winding) |
| Enter | Open the line, or pick the colour on it |
| Esc | Back a page, or out of the picker |
| Click | Open the line, or nudge a number up — click the `‹` to nudge it down |
| Right click | Nudge a number down |

Out over the world, beside the list, a small box follows the cursor with the
**range to whatever it is over** and the colour the distance ramp gives there.
That is the quickest way to put a stop where a ridge is: point at the ridge,
read the metres, drag the stop to them.

It answers the same way the ramp does, from two passes with the nearest
winning: the voxel world, which has everything that is drawn but only out to
the streamed radius, and the generated terrain height, which has the ground at
any range but nothing standing on it. So a trunk ten metres away reads ten
metres, and a ridge two kilometres out still reads two kilometres. Entities —
creatures and placed models — are not in either pass, so the cursor reads
through them.

## The three strips

Three of the five pages are 1D colour strips. A strip is a short list of stops with straight lines
between them, baked to a 256-sample texture the shaders read. What differs is
**what each strip is read by** and **what it does with the answer**.

| Page | Read by | Does |
|---|---|---|
| DISTANCE RAMP | how far away the surface is | colours the haze in front of it, and optionally the low sky and the shadows |
| TINT | how bright the pixel already is | multiplies, so every local hue survives |
| GRADIENT MAP | how bright the pixel already is | replaces the hue, keeping only the brightness |

They stack: the distance ramp works in the scene, before the picture is
tone-mapped; the other two work on the finished frame, tint first, then map.

On any strip page: left and right move a stop, Enter picks its colour, ADD A
STOP puts one in the widest gap (up to eight), TAKE A STOP OUT removes the
middle one nearest halfway. The two ends always stay. Past the last stop the
strip holds that colour flat — it does not wrap or fade out.

Opening a stop on the **distance ramp** gives the colour picker a fourth
control: a distance bar across the bottom of the panel, painted in the ramp's
own colours, running from 0 to twice RAMP REACH. Drag the handle to move the
stop, or step the DISTANCE line. The apricot tick on the bar is whatever the
cursor is pointing at, so you can drag a stop straight onto a ridge you have
picked out.

### Distance ramp vs gradient map

They sound alike and do opposite things.

The **distance ramp** is about *where a thing is*. Two hillsides of the same
green, one at 200 m and one at 900 m, come out different colours. This is
aerial perspective, art-directed: you choose what 900 m looks like instead of
accepting what a fog equation gives you. It is what makes ridges recede
through a designed sequence of hues, and it is the one that can produce
rows of flat colour.

The **gradient map** is about *how bright a thing is*, and it ignores distance
entirely. Every dark pixel in the frame takes the strip's dark colour, every
bright one takes its bright colour, whatever they were before. Push it and the
whole picture collapses onto four or five hues — the poster-flat grade. Only
the stops' hue matters here, because each sample is put back at the pixel's
own brightness, so you can make them as saturated as you like.

The **tint** is the gentle version of the gradient map: same strip shape, same
reading, but multiplied rather than substituted, so a green tree stays a green
tree and only leans warm or cool. The default is a split tone — teal in
the shadows, gold in the highlights.

## LIGHT

The colours the world is lit by, before any grade. The clock moves all of them
through the day (`src/render/daylight.rs`), so what you set here is the look
the day is shifted *away from*, not what is on screen at this hour.

- **SKY ZENITH** — straight overhead.
- **SKY HORIZON** — at eye level. The distance ramp is held to this colour's
  brightness while HELD TO SKY VALUE is 1, so it also sets the value of
  everything far away.
- **GROUND BOUNCE** — light coming back up off the ground into the undersides
  of things. Keep it dark and in the soil's family; it is not a second sky.
- **AMBIENT FILL** — the sky's fill on everything the sun does not reach. This
  is what shadows are made of, and the single biggest lever on the mood. A
  deep slightly teal fill is what gives the saturated style its shadows; a
  lifted blue-grey makes everything look like an overcast afternoon.
- **SUN TINT** — a multiplier on the key light's colour, so the sun can be
  warmed without moving the sky.
- **SUN GAIN** — how bright the key light is. Everything else is judged
  against it, so this moves the whole exposure.
- **FOG DISTANCE** (m) — how far you see before the air has taken the colour
  over. Lower is hazier. HAZE DENSITY scales it again on the ramp page.
- **SUN-SIDE WARMTH** — haze warmer looking into the sun, cooler looking away.
  Separates a sunlit slope from a shaded one at the same distance.
- **SUN GLOW** — a broad warm bloom around the sun, in the sky and the haze.
  Off by default. Not the sun rays, which are their own pass in `post.wgsl`.
- **TIME OF DAY TINT** — how much of the clock's own warm and cool shift
  reaches the grade. 0 holds one colour all day.

## DISTANCE RAMP

The strip, plus the knobs that decide where it lands in the world and how much
of it you ever see.

Stop positions read in **metres**, worked out from RAMP REACH and RAMP CURVE
together — a stop is a position along the strip, so changing either of those
moves every stop's metres. `far` means the eased curve only reaches that stop
at infinity.

- **RAMP REACH** (m) — the distance the whole strip is spread over. 300 m
  suits a forest; 1000 to 1500 m open country.
- **RAMP CURVE** — the shape distance is mapped on.
  - **1** eases: `1 - exp(-d / reach)`. The strip is spent on the first few
    hundred metres and everything beyond crowds into its far end, so distant
    ridges all come out much the same colour. This is the default.
  - **0** is straight: a stop sits at the distance it says, and ridges a few
    hundred metres apart land on different parts of the strip.
- **DISTANCE SLABS** — cut the view into this many slabs of distance, each
  taking one colour and one thickness of haze right across it. **This is what
  turns ridges into flat cards.** A stepped ramp colour on its own is not
  enough: the *amount* of haze still rises smoothly, so the steps blend away.
  Five or six slabs over a 1200 m reach is a reasonable start. Off by default.
- **HELD TO SKY VALUE** — at 1 only each stop's hue is used and its brightness
  follows SKY HORIZON. Drop it toward 0 and the strip sets brightness too,
  which is what separates one band from the next.
- **HAZE FROM RAMP** — how much of the haze colour comes from the strip rather
  than from the sky behind it. At 0 the ramp does nothing to the haze.
- **HAZE DENSITY** — thickness of the ramped haze, on top of FOG DISTANCE. The
  ramp only shows where the haze has taken over, so this decides how much of
  the strip is ever visible.
- **HAZE HEIGHT** (m) — the height the haze thins over. The air at this height
  above the sea holds about a third of what the air at the sea does, and the
  thickness between any two points is integrated along the ray, so a valley
  floor lies under more air than the crest behind it. This is what stacks the
  distance into layers instead of one flat wash, and what makes DISTANCE SLABS
  cut along the land rather than straight across it. 0 gives air of one
  thickness at every height.
- **SKY FROM RAMP** / **SKY REACH** — pull the far end of the strip into the
  low sky, and how far up it climbs. Distant ridges then dissolve into a sky
  of their own colour instead of standing against it. **These two only work
  together**: at the default SKY REACH of 0.26 the colour reaches a thin band
  just above the horizon, which terrain usually hides, so raising SKY FROM
  RAMP on its own looks like it does nothing. Raise SKY REACH with it. It is
  the strip at its far end, so the stop it uses is the last one.
- **SHADE FROM RAMP** — tint shadowed and indirect light by the strip at that
  distance, so a far hillside's shadows sit in the same family as the haze in
  front of it.
- **CROWN SHADING** — leaves darker near the ground and lighter toward the top
  of the crown, by height above the terrain. A canopy depth the lighting alone
  misses. Off by default.

### Getting flat cards

In order of how much each matters:

1. **Composition.** Rows of flat colour need ridgeline behind ridgeline with
   sky between them. A forest, or a plateau seen from above, has nothing to
   band, whatever these knobs say.
2. **RAMP CURVE 0** and a reach that matches the view, so ridges at different
   distances land on different parts of the strip.
3. **DISTANCE SLABS** on, so the haze steps as well as its colour.
4. **HELD TO SKY VALUE** low, so the bands differ in brightness, not only hue.
5. **HAZE DENSITY** high enough that the far bands are mostly haze.

## TINT

The luminance strip, multiplied in.

- The stops run from 0 (the darkest pixels) to 1 (the brightest), not metres.
- **TINT STRENGTH** — how strongly it multiplies. It keeps each pixel's own
  hue, so it stays subtle however far you push it.

Colours here sit near white, because they are multipliers: `#EBFFFF` is a
faint teal cast, not a teal. A `+` after the hex means a channel is brighter
than white, which is how a stop lifts rather than tints.

## GRADIENT MAP

The same kind of strip, read the same way, but it replaces the hue.

- **GRADIENT MAP** — how much of it is applied. At 0 it does nothing; at 1
  every pixel takes the strip's colour for its brightness.
- Only hue and chroma matter: each sample is normalised and put back at the
  pixel's own brightness, so the picture never gets darker or brighter, only
  flatter. Make the stops bold.

The default runs deep blue-teal through a warm middle to pale gold.

## GRADE

What is left after the strips, in this order: exposure, the time-of-day shift,
the tint, the gradient map, saturation, contrast, vignette.

- **EXPOSURE** — brightness going into the tone map (ACES). Raising it both
  lifts the picture and pushes more of it into the shoulder, where highlights
  flatten.
- **SATURATION** — colour against grey. 0 is black and white.
- **CONTRAST** — an S-curve darkening the lower half and lifting the upper.
  Small amounts: the tone map has already rolled both ends off, so this bites
  quickly.
- **VIGNETTE** — darkening toward the corners.
- **SITE LUSHNESS** — how strongly the plant site fields show in grassy
  ground: greener, taller grass in wet, sheltered hollows, ochre and shorter
  on dry, sunny, wind-cut ground. 1 as tuned, 0 off, up to 3. The colour
  follows at once, the grass when the land round you is built again a moment
  after you let go (docs/PLANTS.md, "Ground cover").

## The look file

EXPORT writes every value above, keeping the file's `notes`. READ THE LOOK
FILE AGAIN throws the session's changes away.

`VOXEL_GRADE=legacy` turns the ramp off for a comparison, and the tokens
`fog2`, `sun1`, `sky2`, `glow1`, `tree2` and `shade1` each set a group of
knobs, on top of whatever the file says.

## A world's own look

`worlds/<name>/look.json` is a patch over the style file, read when the world
loads and applied to both styles before the `VOXEL_GRADE` switches. Every field
is optional and named as in the look file, with the grade knobs gathered
under `knobs` by their names in `render/grade.rs`; `styles` holds a further
patch for one style alone. The order is: style file, the world's shared
patch, the world's patch for this style, then the environment switches.

The two EXPORT TO THIS WORLD lines write it: the page's state minus the style
file, so the file holds only what the world made its own, and a world that
has gone back to the plain style loses its file. Because the world's values
are the world's, EXPORT TO THE LOOK FILE writes the style around them and
reports how many it left alone — change a knob a world owns and the style
will not take it until the world gives it up.

Nine worlds ship with one. To change a world's, open the ramp page in that
world, move the stops and export to the world.
