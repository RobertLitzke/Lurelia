# Routes

Lines through a world — roads, lanes, fences, boardwalks, guardrails,
elevated bridges, skywalks, cave passages — and the settlements that grow
along them.

All of them are a polyline and a cross-section, so one engine builds them. A
road can run at any angle, a fence can follow a diagonal, a bridge is as long
as the gap it crosses, and a village's houses front onto its streets.

## The three pieces

**Kits** (`library/kits/*.json`) describe a cross-section and what repeats
along it: a surface width and material, continuous rails, posts every so many
metres, a deck with piers, pavements, headroom. One kit is a fence, another a
highway with steel guards, another a rope span. Kits are library content, like
structures and plants, so a world adds its own.

**Ribbons** (`world/route/ribbon.rs`) are the geometry. A ribbon is a 3D
polyline plus a kit; its voxels are decided per column at chunk time, exactly
as `world/city.rs` decides street surfaces and bridge decks. That is
what lets a ribbon run at any angle and any elevation — the quarter-turn
limit on placed structures does not apply. Where a kit only grades the land
(a dirt road), the work happens at chronicle time on the height field
instead, through `grade_strip`.

**Networks** (the `network` event, routed by `world/route/cost.rs`) are
nodes and links. A link is routed across the terrain by A\* over a cost field
(slope, water, cliff, taken ground), so the world builder drops two points and
the road finds the pass.
Links meeting at a node are trimmed back and the junction is aproned, so
crossings look built rather than overlaid. Links carry a rank — highway,
street, lane, path — and a rank change is where one kit hands over to
another: a road meets a lane, a lane meets a footpath at a door.

A link that crosses a gap wider than it can grade becomes a span: the deck
stays on the smoothed grade line, piers go down to the ground where the drop
is deep enough, and the kit's `span` names what to build instead across the
hole.

## The kit file

```json
{
  "name": "highway_guard",
  "ground": "grade",
  "width": 9.0,
  "mat": "road_asphalt",
  "shoulder": "road_gravel",
  "margin": 6.0,
  "rails": [{"side": "both", "out": 0.3, "from": 0.45, "to": 0.75, "thick": 0.2, "mat": "bridge_steel"}],
  "posts": [{"side": "both", "out": 0.3, "every": 4.0, "size": [0.25, 0.75, 0.25], "mat": "rust_iron"}],
  "span": "steel_span"
}
```

`ground` is `grade` (cut and fill the land to the line), `deck` (a free-standing
deck at `lift` m over whatever is below, for boardwalks and skywalks),
`follow` (leave the land alone and sit members on it, for fences) or `none`.

`side` is `left`, `right`, `both` or `centre`. `out` is metres outward from
the surface edge, negative pulling in. `from` and `to` are heights above the
surface. Posts repeat by arc length along the whole route, not per segment,
so a fence's posts stay evenly spaced across a corner.

## Events

| Event | What it does |
|---|---|
| `ribbon` | one polyline, one kit — a fence line, a skywalk, a guardrail |
| `route` | A\* from one place to another with a kit; builds spans where it must |
| `network` | named nodes and links, routed together, junctions resolved |
| `village` | a layout (grid, radial, organic, linear) as a network, with buildings on its frontages and footpaths to their doors |
| `watercourse` | a channel that finds its own way down, and the distributaries that leave it |

## Water

A channel is a route with a different idea of cost. `watercourse` turns the
search round: only the rise of a step is paid for, falling is free, and a step
that climbs more than a few tens of centimetres is refused, so the line that
comes back is one water could take. `via` points keep the geography a world is
built round — the bend a village stands on, the gap the river comes through —
while the line between them is found rather than drawn.

Branches are what make a delta. Each leaves the trunk a fraction of the way
along it and goes to its own mouth, paying three times over for ground the
trunk already took, so it diverges instead of falling straight back in. A
channel stops when it reaches water deeper than itself, and its last quarter
is cut in two shallower, narrower steps: a distributary runs out rather than
ending in a blunt trench.

The carving and the water are the `river` event's own — `watercourse`
registers what it cut under its name, so `settle_water` fills it, an
`avulsion` can break it out later, and everything that works on a `river`
works on it.

## Validating a change

Overhead first, then the ground. `--map` draws the whole world from above,
which is where a network's shape shows; `--headless --ground X,Z --look` puts
the camera at a junction. A layout change that reads well from above and
badly at a crossing is not done.
