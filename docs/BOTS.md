# Bots

The arena opponents: how each layer works, how they are drawn, and how to
watch and measure them without a window. The code is `src/brain/`; the match
rules that host them are `src/shooter/` (`docs/SHOOTER.md`).

## The claim

A bot is a thing that presses the keys. `Player::update` takes a `MoveInput`
and knows nothing about where it came from, so a bot is a `Player` body whose
`MoveInput`, yaw, pitch and trigger are filled in by code. It walks, falls,
collides, swims, dodges and dies through the function the human drives, and
fires through the same `Arsenal`.

So a bot cannot take a corner the player could not, stop dead, or shoot
through a wall, and every difficulty knob is on perception and judgement, not
health, damage or speed. `shooter::tests::a_bot_moves_exactly_as_the_player_does`
holds the line; anything that moves a bot by writing its position breaks it.

## Layers

| Layer | Question | File |
|---|---|---|
| Senses | what may this body know? | `senses.rs` |
| Steering | which keys get me toward that point? | `steer.rs` |
| Navigation | where are the points? | `nav.rs` |
| Decision | what do I want? | `duel.rs` |

Each layer works without the one above it; only the decision layer is
per-game. `person.rs` holds the scalars every layer reads, so a difficulty is
a row in a table rather than a branch. `aim.rs` and `desire.rs` serve the
decision layer.

## The seam

`Pilot` is everything a body can be told in one step (`mv`, `yaw`, `pitch`,
`trigger`, `want_slot`, `dodge`), and `Brain::think(me, cx, out, dt)` fills it;
the rest of the trait is counters for the headless report. `Me` is the bot's
own state, including `struck` (who landed a blow since it last thought, seen
once). `Ctx` is read-only: store, nav graph, clock, every fighter as a `Foe`,
the `Lure`s on the floor and recent `Noise`s. `Fighter` (health, armour,
respawn, score, arsenal) is shared by the player, who is fighter 0, and the
bots, 1.. in the same list.

`Shooter::step` runs once per fixed step (1/60 s, from `Game::fixed_step`):
settle the graph and pads, think, step the bodies, push overlaps apart,
collect pickups, fire. Only the player's keys are taken per frame.

## Senses

`senses::can_see` tests range (`sight`), then the cone (`peripheral`, a
cosine), then a voxel raycast. The raycast is the expensive part, so
`Memory::look` runs at most every `LOOK_EVERY` (0.1 s), staggered per bot.

What a bot sees becomes a `Contact`: last position and velocity, when last
seen, and when this run of seeing began (losing sight and finding a body again
is a fresh acquisition, which matters for aim). A contact decays rather than
vanishing and is forgotten after `REMEMBER_S` (6 s); `Contact::guess` carries
it forward along its last velocity for at most 0.6 s. A bot cannot know how
hurt anyone else is.

Hearing: `Shooter::heard` keeps one `Noise` per shooter (the newest shot
replaces the last), audible for 40 m and kept 3 s. A noise tells a bot where
to go, never who is there. A roaming or hunting bot re-routes toward a noise
somebody else made (`Duel::toward_noise`), once per noise, then holds that
destination for `HEARD_HOLD_S` (2 s). Without the hold, three bots firing
re-route each other several times a second and nobody arrives anywhere.

## Navigation

The graph is baked. A world states it in the `nav` block of its `shooter`
rules: nodes (`at` as `[x, z]`, `up`, `cover`, `poi`) and links (`a`, `b`,
`reach`, optional `cost`, `platform`, `portal`). Slagdeck's is written by
`worlds/slagdeck/make_arena.py`, the script that writes the deck, so the two
cannot drift apart. Imported maps get theirs from the importer.

`reach` is `walk`, `jump`, `drop`, `lift` or `portal`. `Graph::route` (A\*)
filters links against `Able`, what this body will do, so a bot never plans a
move it would not make; a timid, hurt bot will not take an optional drop.
Drops and portals are one way (a portal pair is two links). A link costs its
length unless it states a `cost`; a lift costs its length plus 14 m so a bot
does not ride one to save three steps, and a portal costs 3 m.

- Node heights are metres above the floor the world's spawn point stands on,
  read from the voxels. `level` and `foundation` move a structure after the
  world file is written, so absolute heights would be wrong.
- The graph cannot be built when the match starts, because only the chunks
  round the spawn have streamed in. `Shooter::settle_nav` rebuilds it every
  `NAV_RETRY_FRAMES` frames up to `NAV_TRIES` times, keeping the best so far;
  bots are stood on real ground once it settles. Pads go down on the first
  pass and `settle_pads` re-asks their floors.
- Each node records whether its floor burns. Routing never enters a burning
  node from dry ground but may cross them from one, so a bot in a pool can
  walk out. A graph where every node burns is treated as a bad reading.
- Nodes closer than about 1.5 m make bots stick on them. Keep nodes sparse.

`Graph::audit(from, able)` lists the nodes that burn, the ones a body at
`from` can never reach, and the ones it can reach and never leave. The last
kind is the dangerous one: a bot that wanders in asks for impossible routes
for the rest of the match. The headless report prints it as `nav_audit`.

### Paths and reach

`Path` is a list of nodes plus an optional `tail`, a point off the graph to
finish on. Pads on an imported map are mostly further from any node than the
1.3 m pickup reach, so a fetch routes to the nearest node and walks the last
stretch straight at the pad.

- A node counts as reached at 1.8 m across the floor, height forgiven. The
  node a route leaves through a portal needs 0.5 m, because stopping short is
  outside the portal's trigger. `advance` looks two steps ahead, so a body
  coming out of a portal picks the route up at the far end.
- `Path::goal()` returns `None` once the path is done. Callers ask
  `goal() != Some(x)` to mean "am I already heading for x", and answering with
  the last node of a finished path freezes a bot where it arrived.

Each bot keeps a flood fill of what it can reach (`Graph::reachable_from`,
using `Duel::able` so survey and route agree) and picks destinations only from
that. It refreshes every 4 s, since a lift arriving changes the answer, and
within 0.5 s after a refused route. A refused node is avoided for `AVOID_S`
(20 s). A bot that can reach `STRANDED` (4) nodes or fewer walks straight at
what it wants instead. `a_reachability_flood_is_cheap_enough_to_keep_current`
asserts a flood stays under a millisecond.

A lift link is the only link that is a wait: near the plate's node the bot
presses nothing and lets the plate carry it. A wait looks like being stuck, so
the ride has its own clock: after `RIDE_PATIENCE_S` (12 s) the bot gives up
and refuses lifts for `NO_LIFTS_S` (25 s).

## Steering

`steer::Want` is a direction plus `sprint`, `jump`, `strafe` and `dodge`.
`steer::drive` turns it into the four direction keys a person would hold, so a
bot moves on the eight headings a player has. Wants add before they become
keys, which is where flocking would plug in.

`Duel::move_feet` passes each want through, in order:

- `steer::around`, the wall slide. If the line is blocked at step, waist or
  head height it tries a widening fan along the wall, nearest the wanted
  heading first, the last pair leaning back so a body can leave a corner. If
  every way is blocked it returns the original want, never zero: a zero want
  reads as a bot that chose to stand still, and the jam watch ignores those.
- `Duel::unjam`. A bot that has asked to move for 0.8 s and covered under
  0.6 m counts a jam and, if `Style::unjam` is on, shoves sideways (toward a
  side with floor) and jumps for 0.9 s, then drops its path. The watch runs
  whether or not the shove is on, so `jams` measures the fault, not the cure.
- The ledge and fire checks. A bot never steps off a fall over `LEDGE_DROP`
  (2.6 m) it did not choose, checked 1.4 m ahead, and looks 2.8 m ahead for
  burning ground. If the way is bad, `steer::aside` picks the nearest of eight
  headings that is neither; a bot standing in burning ground walks out
  whatever it wanted. On a `drop` link the floor check is skipped, since that
  fall is one the graph chose; the fire check still applies.

## Dodges

A dodge is a direction key pressed twice inside a quarter of a second
(`Player::double_tap`), read off a bot's keys by the same rule as a player's.
There is no dodge flag on `MoveInput`, which would hand bots a move no player
has. The brain sets `Pilot::dodge`, and `step_bodies` takes it and steps a
`steer::Tapper` once per body step: release, press, release, press.

- The habit, `Duel::may_dodge`, rolls every second against `tactics * 0.6`.
  In a fight it dodges the way it was circling, or back if somebody is in its
  face; breaking off, along its way out (`Dodge::toward`, since it is still
  facing its enemy). A novice never dodges.
- The reaction, `Duel::struck_dodge`, fires on `Me::struck` at the full
  `tactics`, at most every 0.5 s, across the line from whoever hit it (or
  across its own facing if it does not know who). It beats the habit.
- Both check floor and burning ground 4 m out, since a dodge covers far more
  ground than a step, and ignore walls: a dodge into one just stops early.

## The duel

`Duel` picks one of six acts in `decide`, a short `if` with no planner.

| Act | When |
|---|---|
| `Roam` | no contact, or one lost more than `HUNT_S` (3 s) ago |
| `Hunt` | a contact out of sight for under `HUNT_S` |
| `Break` | in sight, and health below the bot's `nerve` |
| `Close` | in sight, and beyond the band's far edge |
| `Fight` | in sight, inside the band |
| `Falling` | off the ground and falling faster than 9 m/s |

`decide` runs every `0.1 + 0.35 * (1 - tactics) + roll * 0.15` s, capped at
`0.20 + 0.18 * (1 - tactics)`, so better bots think more often and nobody goes
half a second without noticing a change. When a path runs out between
decisions, `move_feet` asks for a decision at once.

- Roam: a noise first, then a pad worth fetching, then `somewhere`, which
  samples eight reachable, safe nodes at least 5 m off and scores each by time
  since the bot was there (capped at 45 s), plus distance, plus a bonus for a
  named node. Distance counts in favour: charging for it keeps a bot in the
  room it started in.
- Hunt: route to the node nearest where the contact probably is. If that
  cannot be routed to, roam. Hunts sprint.
- Break: run for a pad that mends it if one is worth the trip, otherwise to
  the reachable node furthest from the enemy, with node `cover` in its favour.
- Close: beyond `CLOSE_BY_GRAPH_M` (12 m) route through the graph; inside it,
  steer straight at the body. The aim is turned by `shoot`, not by the feet,
  so a routed close still fires.
- Fight: push in when beyond the band, back off inside it, and circle with
  uneven legs (`steer::lurch`) and a per-bot phase, so two bots do not hold a
  fixed bearing on each other.

The band is 5 to 16 m, with the far edge pulled in to the gun's reach
(`desire::carry`) so a bot with a melee weapon walks in. 16 m sits under
Slagdeck's sightlines on purpose: the aim cone is a fixed angle, so range
decides more than anything whether a bot hits.

Fetching: the rules decide what a pad is worth (`Pad::good`) and the brain
sees only a `Lure`, a point and a number. `Pad::good` must ask what `give`
asks, or bots pace between pads they cannot use
(`nothing_wants_a_pad_that_would_do_nothing`). The walk costs an eighth of a
point per ten metres, and a fetch gives up after 5 s plus the walk at 2.5 m/s.
A bot content to do nothing looks like a broken one, which is why the fetch,
the jam watch and the ride each have a clock; anything new that waits needs
one too.

`duel::Style` switches off `wall_slide`, `unjam`, `circle`, `fetch`,
`hearing` or `dodge`, per bot through `BotDef::without`; the tournament uses
it to measure the shipping code with one part missing.

## Weapons and buttons

`desire.rs` scores guns from their library files, so a new weapon needs no
bot code. `desire::rating` is the better of a gun's two modes at a range,
given the bot's accuracy, so a poor shot prefers splash and a good one the gun
that rewards aim. It reads real cadence (including reloads), pellets landing
at that range, a shell's reach (fuse times speed), flight time and drop.

- A bot swaps weapon at a 0.3 margin and button (`desire::button`) at 0.25, so
  a charging or spinning mode keeps what it has built. With nobody in mind it
  picks for 14 m. A mode with a sight (`zoom`) has no second mode for a bot.
- `shoot` never fires a mode rated 0 at this range, or a burst that would hurt
  the bot at its own feet.
- Bank shots: a mode with `bounces` may be fired at a wall when the target is
  out of sight. `shot::bank` flies a shell through the world with the target
  carried forward, and `Duel::find_bank` tries twelve headings either side,
  taking the nearest pass under 0.7 m. It needs `leads`, a contact under 2 s
  old and over 35% of the magazine left, and halves the aim error.

## Aim and difficulty

`aim::error_rad` multiplies a base of 2.2 degrees for a ray and 4.5 for a
shell:

```text
err *= 11 - 10 * dot(to_target, ahead)       crossing the view
err *= max(3.6 - 3.2 * (aim + roll * 0.15), 0.25)
err *= 1 + (1 - held / settle)               while held < settle
err *= 1.25 if hurt recently, 1.4 if airborne
```

`ahead` is where the target will be 1.25 s along its course, seen from where
the shooter is heading. It must not be the lead solution: a ray is never led,
so the dot would be exactly 1 for every bullet. The crossing term is why
strafing works and standing still is fatal. `settle` is reaction time done as
bad aim on a fresh target rather than as a timer.

The error is held for a correction cycle (0.55 s down to 0.18 s by `aim`) and
the view turns at `1.8 + 5.2 * aim` rad/s (`aim::turn_head`); an error rolled
fresh every step would be filtered out into perfect aim. The trigger needs the
view within about ten degrees of `want_dir`, where the bot meant to put the
gun, not the true direction, which would turn misses into pauses. A bot fires
on a step with probability `cadence`, and leads projectiles only if `leads`.

`person::LEVELS` has seven rows: novice, easy, average, skilled, adept,
masterful, godlike. Each sets `aim`, `settle`, `peripheral`, `sight`,
`tactics`, `nerve`, `cadence` and `leads`, and gets harder down every axis
(`the_table_only_gets_harder`). Only godlike cheats: its `peripheral` is below
zero, a field of view past 180 degrees. A world's `BotDef` picks a row by
`skill` and adds character with `bold` (more `tactics`, less `nerve`) and
`steady` (more `aim`, shorter `settle`).

## How bots are drawn

A bot is the library creature `robot`, a `humanoid` in different paint,
compiled on demand by `brain::avatar` so it looks the same in every world.
`anim/figure.rs` poses it from what the body is doing. A `Figure` is display
only: it reads the body and never writes to it, and nothing in `brain/` reads
one, or a gait could start deciding how fast a bot moves.

Health is three emissive lamps, amber whole to red nearly out, on the collars
and between the shoulder blades, because the weapon and supporting arm cover
the chest. Their offsets match recesses in `library/creatures/robot.json`;
move one and move the other. Plates take a team colour from
`avatar::TEAM_COLOURS` by bot index unless `BotDef::colour` overrides it. No
team colour is in the lamp's amber band, and slot zero is cold cyan, so lamps
can be measured on the first bot without counting plate. A corpse's plates
wash to grey; a body killed by a big enough blow is thrown
(`physics::tumble`) and settles through `anim/ragdoll.rs`.

Bodies are solid to each other: they may overlap for one step and
`Shooter::unstack` pushes them apart, which keeps `Body::update` free of other
fighters. A bot's hitbox is widened by how far it moved last step, because
bodies are drawn interpolated up to a step behind where shots test them; the
player's is not, since bots fire on the step. `put_at_spawn(.., heal)`
separates a respawn, which heals, from standing bots on ground that has just
streamed in, which does not.

## Watching bots headlessly

A world with a `shooter` block starts a match, and a headless run's `--stats`
JSON gets a `shooter` key: `match_s`, the scores, `nav_audit`, and per bot
counters (act, gun, `jams`, `dodges`, rounds per button, `banks`,
pads, routes). Headless runs set `Game::repeat`, so they repeat exactly.

```text
lurelia --world slagdeck --headless --frames 5400 --stats out.json --no-player
```

- Advance the match with `--frames N` (steps of 1/60 s, default 30, which is
  half a second). `--wait` is only the load timeout. Check `match_s` before
  drawing any conclusion.
- `--no-player` benches the player (`Shooter::bench_player`). Otherwise they
  stand at the spawn, a free kill and a reason for bots to stop roaming.
- To get a bot on camera, run once, read its position, then run again with
  `--ground X,Z --look YAW,PITCH`: `yaw = atan2(-dx, -dz)` in degrees (0 is
  north, -Z) and the chest is at pitch `atan2(bot_y + 1.1 - (camera_y + 1.62),
  distance)`. Bots react to the player, so re-probe for each camera spot.
- Check line of sight before blaming the aim. On Slagdeck the hall's long
  walls are at `z = 166` and `z = 216` and its side walls at `x = 173` and
  `x = 211`.
- `--fire N` holds the trigger for the last N steps, so raising `--frames`
  with N fixed moves the firing window.
- Measure colour rather than squint: `python tools/warm.py shot.png x0 y0 x1
  y1` counts bright, clearly red pixels in a box (`--cool` for cold ones).

## The tournament

`lurelia --world slagdeck --tourney N [--tourney-seconds S]` runs with no
window or GPU (`src/tools/tourney.rs`). Lifts and portals run through the
game's own `world_parts::WorldParts`. Rounds come from `Shooter::repeatable`,
which seeds the spawn draw from the match seed; the game itself draws spawns
off the clock so matches open in different corners. The player is benched
unless `VOXEL_TOURNEY_PLAYER=1`.

The field is the difficulties, `easy bold` and `easy timid`, the average bot
with one `Style` switch off at a time, and `twin`, an exact copy of the
shipping bot. Three brackets:

- Moving: one bot alone, nobody shooting. `cover` is the share of nodes come
  within 3 m of; also `spread`, `climb` (vertical range), `idle` (pressing no
  direction), `jams`, `rides`, `tele`, `took`, routes asked and failed.
- Fighting: each entrant against the shipping bot with aim forced flat, each
  seed from both spawn slots, scored as damage dealt less taken per minute,
  differenced seed by seed against the shipping bot playing itself. It plays
  one round twice first and warns if they differ. An arrow marks a margin two
  standard errors clear, and the last line says how many duels would separate
  the leader. `nose` is time inside 4 m. There is no accuracy column because
  `Score::hits` counts damage events.
- Three bots: each entrant in every one of three slots against two shipping
  bots. This is the game as played; a duel keeps a contact alive nearly all
  the time, so roaming and hearing barely matter in one.

`twin` must print `0 +-0`; anything else means a wall clock has got into the
loop. A number that is exactly zero, or a table where every row agrees, is a
bug in the instrument until shown otherwise. The bars are not run-to-run noise
(there is none) but how much a result depends on which spawns the seeds drew,
so compare only within one run. In the moving bracket, entrants that differ
only in fight behaviour print identical rows, `took` is 0 on a give-all map,
and `idle` counts passengers waiting on a lift. The fight bracket is a
small-arena instrument: on a large map bots rarely meet.

## The weapon bench

`lurelia --world duelbox --duel N [--duel-seconds S] [--duel-weapon NAME]`
(`src/tools/duel.rs`, `docs/SHOOTER.md` "The bench") puts two average bots,
aim forced to 0.5, in one empty room with one gun each. The bench gives both
the same gun; the table plays every gun against every other, both ways round.
Check any change to `desire.rs` here.
