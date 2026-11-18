# Slagdeck

A works on a granite shelf off a cold coast. One building, one floor worth
standing on, and nothing else for a hundred metres in any direction.

## What it was

The shelf was chosen for the rock, not the weather. Granite takes a heavy
plant without settling, and the deep water came close enough inshore that
scrap could be landed at the foot of the island and carried up. Nobody lived
here; the shift came in by boat and went out again.

The building is a smelter, raised five metres on a plinth so the tapping floor
could be reached from under it as well as from the deck. Everything else is
arranged round that floor: the hall over the pit, open to the weather because
nobody wanted the fume under a roof; the west shop where moulds were cast,
roofed, with a gallery over it for the men who watched the pour; the east shop
where scrap came in, laid out the same way the other way about; a walkway ring
at nine metres so a man could cross the hall without crossing the floor; and a
platform over the north walk where the crane driver sat, reached by two ramps,
with the whole hall under him.

The corridors behind the hall, north and south, were how you got from one shop
to the other without going near the pour. They are the reason the deck reads
as a loop rather than a room: there is no dead end on it anywhere, and no two
ways between two places are the same length.

## What happened

Nothing dramatic. The scrap stopped coming, the boat stopped, and the works
was left where it stood. The sea air has not had long enough to do much — the
sheet along the top of the walls has gone chalky, the rails have rusted, the
paint on the edges is still legible — and the pit is still warm, which nobody
has a good explanation for. That is the one strange thing here and it is left
unexplained on purpose: everything else in the view is a building doing what a
building does.

## What it is now

A deathmatch map, laid out as a classic arena: the pit is the map's low
ground, the walkway ring is its high ground, and the platform over the north
walk is the place worth fighting for and the hardest place to hold. Ten
weapons, nine spawn points, and bots to fight.

The rules are in `world.json` under `shooter`; the building is one structure
written by `make_arena.py` in this folder. See `docs/SHOOTER.md`.

## Rules of the place

- No plants, no weather, no wildlife on the deck. A few squirrels live on the
  ballast outside and can be seen, and shot, through the high windows.
- Nothing decays (`weathering: 0`), because a wall that crumbles changes the
  map, and a map that changes is not a map.
- The only warm colour in the world is the slag. A tracer should be the
  second.
