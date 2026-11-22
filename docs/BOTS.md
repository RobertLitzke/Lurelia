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
| Team | what does my side need? | `team.rs` |

Each layer works without the one above it; only the decision and team
layers are per-game, and the team layer only runs in capture the flag. `person.rs` holds the scalars every layer reads, so a difficulty is
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

A lift link is the only link that is a wait. The bot walks to within 0.6 m of
the plate's own node (the one the link leaves from, not the landing it
arrives at, which can be beside the plate), presses nothing until it has
been carried to the landing's height, then walks off. A wait looks like being
stuck, so the ride has its own clock: after `RIDE_PATIENCE_S` (12 s) the bot
gives up and refuses lifts for `NO_LIFTS_S` (25 s). Going down a lift it does
not wait at the top: a plate that starts when stood on rests at the bottom, so
the bot walks into the shaft as it would take a drop (`Duel::down_shaft`).

A route that starts behind the bot (it is already past the first node toward
the second, on a walk link) starts from the second node.

`Duel::progress` watches the distance to the step being walked. If it has not
shrunk by 0.4 m in 1.1 s, or the bot has covered under 1.5 m in 2.5 s while
running (pacing round a knot of close nodes, which resets the step watch at
every node), the link is routed round for 30 s at a cost of 60 m
(`Graph::route_with`), and a first node that could not be reached is skipped
as a start (`nearest_seen_except`); a pad's tail that cannot be reached
avoids the pad, and the goal is left alone for `AVOID_S`, as it is after a
jam. This catches a bot running flat out and getting nowhere,
which the jam watch below cannot see because the body is moving. A bot jumps
when the next node is more than 0.45 m up and under 2.5 m away, and when the
floor 0.6 m ahead rises more than the body's 0.5 m step.

## Steering

`steer::Want` is a direction plus `sprint`, `jump`, `strafe` and `dodge`.
`steer::drive` turns it into the four direction keys a person would hold, so a
bot moves on the eight headings a player has. Wants add before they become
keys, which is where flocking would plug in.

`Duel::move_feet` passes each want through, in order:

- With nobody to aim at, the keys come from `steer::drive_running`: forward,
  steering with the head, and a side key only past 45 degrees off the facing.
- `steer::around_keeping`, the wall slide. If the line is blocked at step,
  waist or head height it tries a widening fan along the wall, nearest the
  wanted heading first, the last pair leaning back so a body can leave a
  corner. The side it slid to is kept for `SLIDE_KEEP_S` (2.5 s), and the ledge
check's turn along an edge keeps the same side, and every
  heading on that side is tried before the other, or a body at a doorframe
  slides back and forth several times a second. If
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

The same rule catches keys the brain did not mean to tap: a wanted heading
that wobbles across `drive`'s threshold presses, releases and presses again,
and the body dodges. Before the tapper guarded against it the league counted
38 dodges a minute against five asked for. Outside a tap, the `Tapper` will
not press a key pressed less than 0.3 s ago (`GUARD`), nor a side key within
0.3 s of the opposite side key going down, which is a sidestep reversed
before it began.

- The habit, `Duel::may_dodge`, rolls every second against
  `tactics * DODGE_ODDS` (0.24), about one dodge per ten seconds of fight for
  an average bot.
  In a fight it dodges the way it was circling, or back if somebody is in its
  face; breaking off, along its way out (`Dodge::toward`, since it is still
  facing its enemy). A novice never dodges.
- The reaction, `Duel::struck_dodge`, fires on `Me::struck` at
  `tactics * HIT_DODGE_ODDS` (0.4), at most every 0.5 s, across the line from whoever hit it (or
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
  cannot be routed to, roam. Hunts sprint. Arriving there ends the hunt
  (`searched`): the bot roams on rather than standing on the spot until
  `HUNT_S` runs out. With nobody in sight the head turns along the path, at
  twice the aiming rate, except for `GLANCE_S` (1 s) after losing sight of
  somebody, when it keeps looking where they went. Under `MEND_FIRST` (55% health) a bot with nobody in sight
  goes for a pad worth fetching before it hunts or follows a noise.
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
sees only a `Lure`, a point, a number and, for a gun, which arsenal slot it
is. The brain re-weighs a gun by its own rating of it loaded at 14 m against
the best gun it could have (`0.5 + 0.7 * like`), so a bot goes for the gun it
fights best with rather than the highest slot. A pack's worth rises as
`short^1.5`, and a gun or rounds are worth `0.2 + 0.8 * whole` of their value,
so below about a third of its health a bot wants a pack more than a gun. A
`Lure` also says whether it heals (`mends`): under `MEND_FIRST` (55%) a pad
that does not is worth `HURT_ELSE` (0.3) of itself, the walk to a pack costs
three times as much (nearest first), and a bot with nobody in sight does not
hunt at all. A bot whose best gun in hand is under `ARMED` (0.99) of its
favourite fetches before it hunts or follows a noise, with the walk to a gun
priced the same way. `Pad::good` must ask what `give`
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
- A mode with `bounces` and a `self_hit` is not fired when `shot::comes_back`
  (the same flight, counted from the first bounce) brings the round back
  within the bot's half-width plus `DISC_BACK_M` (2.5 m) of where it stands
  (`CTF_OFF=comeback` turns that off).

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

Health is two emissive bars, one across the top
of the chest and one between the shoulder blades, because the weapon and
supporting arm cover the rest of the chest. They lie in the dark bands at
y 24-25 in `library/creatures/robot.json`; `avatar::bars` finds the chest's
front and back on each build's voxels. Every bot is one of three builds
(`avatar::BUILDS`: brawny, heavy, lean) on the same bones and the same
collision box as the player. The team colour goes on a uniform, not the whole
body: the chest and shoulder plates and the kneepads are their own colour
(`uniform`) and each body part is sliced twice, uniform and the rest, so only
the uniform half is tinted (`avatar::PAINT`, `Body::paint`). The tint is taken
most of the way into linear light first (`avatar::plate`); handed over as raw
bytes it lifted every weak channel and every bot came out pastel. It takes a colour from
`avatar::TEAM_COLOURS` by bot index unless `BotDef::colour` overrides it, or
in a team match their side's colour. The lamps are that colour too, brighter
and more saturated (`avatar::lamp_colour`; in a team match the side's base,
not the per-bot shade), and run to red as the bot is hurt. Slot zero is cold
cyan, so a hurt bot's red can be measured on the first bot without counting
plate. A corpse's plates
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

Stock is the default errand: health above full is worth up to 0.4 to top up
to the cap (`OVERHEAL_WORTH`), and a roaming bot fetches anything worth
`STOCK_FIRST` (0.3) before it follows a noise. A fight or a hunt still comes
first. A roaming bot fetches any pad worth over `FETCH_IDLE` (0.05), and while roaming
or hunting with nobody in sight it steps over to any useful pad it can see
within `PASS_M` (5 m) of where it is (`Duel::in_passing`).

## Shells and goo it can see

`Ctx::threats` is every shell in the air and `Ctx::mines` every goo blob
lying about. A brain acts only on what is inside its own view cone
(`peripheral`), within `sight` and in line of sight: nothing behind it.

- `Duel::incoming` flies each shell forward 1.5 s with its gravity. One that
  will pass within its blast plus 0.9 m of the body, later than the bot can
  react (0.08 s plus 0.15 of `settle`), is stepped off sideways for 0.45 s,
  still shooting; under 0.9 s out it is dodged if a `tactics` roll passes and
  the `dodge` switch is on. `Style::evade` switches it off. Measured in the
  single-gun league it helps against the lobbed goo and shows nothing
  against rockets or scrap, which mostly arrive faster than a bot reacts.
- `Duel::mine_ahead` steers 2 m clear of a visible blob within 5 m of the way
  it is going (a blob goes off at 1.1 m).
- `shoot` will not fire a blast that can hurt its owner into a wall within
  1.2 times its radius, nor a disc that comes back off walls at a wall within
  4 m. A gun that spins up holds its trigger rather than rolling `cadence`
  every step, which let the barrels run down.

## The league

`lurelia --world transmission --league N [--tourney-seconds S]`
(`src/tools/league.rs`) puts five bots of one difficulty (`skilled`) on the
map at once, free for all, with their own aim, and plays every entrant in
every slot on each of N seeds, 90 s a match by default. Four seeds is 20
matches and takes a few seconds on an arena that size. The entrants differ by
`BotDef::tune`, numbers on `duel::Tuning` (`dodge`, `weave`, `hunt`,
`greed`, `nerve`), so a code change moves every row and the spread between
rows says which way a knob wants to go.

It counts what reads as aimless from the other end of a gun:

- objectives: guns picked up a minute and seconds from spawn to the first,
  health taken while under half, how hurt spells end, and how often a lost
  enemy is found again inside 6 s;
- movement: time walking backwards with nobody in sight (`back%`; `fback%`
  is the same in a fight, where it is fair), two-second stretches with nobody
  in sight that covered under 1.5 m (`lost%`), time pressing nothing, dodges
  a minute taken against those asked for (`meant`), sidestep reversals a
  minute, time faster than a boosting player, the average range to a foe in
  sight (`range`), and routes asked for and refused a minute (`rte/m`,
  `ref/m`);
- while hurt: the share of hurt spells that end in a death (`died%`) and the
  metres to the nearest health pad standing when each began (`pack m`);
- a score a minute alive: 8 a kill, -6 a death, damage over 25, 3 a gun, 4 a
  pack taken while hurt, 1.5 armour, 2 a lost enemy found, less 0.4 a percent
  of `back%`, 0.5 of `lost%`, 0.3 of idle, 1 a dodge over six a minute and
  0.3 a reversal over twelve.

`--league-weapon NAME` (or `all`, each gun in turn) plays the league with one
gun: everybody spawns holding it and every gun or ammunition pad hands out
rounds for it. It prints the field's kills, damage, deaths nobody was
credited with, the part of those its own round did (`own/m`, from
`Score::own_deaths`), rounds and damage a round, and shells evaded, a
bot-minute. `LEAGUE_WITHOUT=evade` switches a `Style` part
off for every entrant, and `LEAGUE_WITHOUT_FOR=house` for one only, which is
the paired test: in a mirror match a change moves the whole field.

The league also prints coverage, one bot in one match: nav nodes come within
3 m of, 8 m areas (3 m of height a floor) stood in, pads taken from, pads that
were of use to it at some moment, pads stood on, and the same by spot (pads
within 2.5 m are one place). `LEAGUE_STASH=0` turns
`ShooterDef::ammo_without_gun` (on by default) off for the run.

The score's fight half swings about five points an entrant between runs that
share their seeds once the code under it has changed, so compare the
movement and objective columns, and the field's average, more than the
order of the rows. `LEAGUE_FLIP`, `LEAGUE_LOST`, `LEAGUE_IDLE` and
`LEAGUE_HURT` print a line per event with `Brain::note`, which says what a
bot is walking toward and why; `LEAGUE_TRACE` prints every bot's act,
position and note four times a second, and `LEAGUE_SELF` a line per death by
a fighter's own round.

Running with nobody to aim at, a bot will not put the other side key down
within `RUN_FLIP_S` (1.2 s) of one going down; it holds W and lets the head
turn instead (`CTF_OFF=runflip` turns that off). Changes the league kept
behind a name can be switched off with `CTF_OFF` in the deathmatch too
(`brain::team::trying`).

## The weapon bench

`lurelia --world duelbox --duel N [--duel-seconds S] [--duel-weapon NAME]`
(`src/tools/duel.rs`, `docs/SHOOTER.md` "The bench") puts two average bots,
aim forced to 0.5, in one empty room with one gun each. The bench gives both
the same gun; the table plays every gun against every other, both ways round.
Check any change to `desire.rs` here.

## The team

Capture the flag puts a layer over the decision one: `team.rs`, one
`TeamBrain` per side, run by the rules before the bots think, once a second
and at once whenever a flag moves. It hands each bot an `Order` (a `Role`, a
`Goal`, an urgency, and optionally a lane to go by and a fighter to mark), and
the bot reads it as `Me::order`. A team deathmatch has no team brain: its bots
play the duel with their own side left out of `Memory` and out of the line of
fire (`friend_in_line`).

What a side may know is what the player's HUD shows them. Where a flag lies is
public. Where the enemy carrying our flag is, only while one of our bots has
them in sight (`Shooter::side_sees`), and the last place they were seen after
that. `Brain::sightings` pools each bot's `Memory`, which is what a side on
voice comms has and no more than any one of them saw.

The plan is a list of needs dealt to the bots nearest first:

| Need | When | How many |
|---|---|---|
| Carry | it has their flag | whoever holds it |
| Return | its flag is lying somewhere | one |
| Retrieve | they have its flag | half the side, at least one; in a standoff (both flags out) everybody but the carrier |
| Pick up | their flag is lying somewhere | one |
| Escort | it has their flag, and its own is home | one, two in a side of five or more |
| Defend | otherwise | one in a side of two to four, two above; none while chasing its own flag in a small side, or behind with a minute left. With no enemy seen within 30 m of the flag for 10 s (and at the start) it holds midfield, 40% of the way to the enemy stand, and drops back to the home posts when one is (`CTF_OFF=stance` keeps it home). When the side has seen every enemy die in the last 10 s, the defender attacks too (`CTF_OFF=reset`) |
| Attack | everybody left | each by a different lane, at urgency 0.85, so they do not break off or chase; within 15 m of the flag it stops fighting and runs for it, shooting on the move (`CTF_OFF=commit` turns that off) |

A bot keeps a job for `HOLD_S` (6 s) unless a flag moves, and the deal favours
the job a bot already has by 25 m of walk. Without the hold, a side's bots swap
jobs every plan. Lanes are nav nodes about as far from one stand as the other,
no two within 8 m (`team::lanes`). Posts are nodes 5 to 16 m from the stand,
nearest the enemy first (`team::posts`).

In `Duel`, an order comes in through two places, not a branch in every act:

- `obey` changes the act. Roaming becomes the job (`Act::Errand`). A carrier is
  always on the errand: it shoots whoever it sees but never closes, hunts or
  breaks off toward a pad. Attackers and escorts stop chasing anybody further
  than `CLOSE_BY_GRAPH_M`. A defender fights inside its leash (22 m) and walks
  back. A retriever keeps hunting the bot it was told to mark.
- `errand` picks the destination: through the lane first, then the point, the
  carrier to follow (3 m to one side, 2.5 m behind), or the post. Inside 4 m of
  a point it steers straight at it. At a post it moves to a new spot within
  2.5 m every 3 to 7 s and sweeps its head either side of the way the enemy
  comes.

A carrier's route charges 40 m for every node within 10 m of somebody it has
seen in the last 4 s, so it goes the long way round a fight. `MARK_WORTH` (0.6)
on the marked fighter is enough to pick the enemy carrier out of two in sight.

`Coach` switches a side to one of the bench's controls: `Fixed` (every third
bot defends, the rest attack, for life), `Rush` (everybody attacks), and `Base`,
the team brain as it was before the CTF tournament, frozen in `team_v0.rs`.
The first two still carry a flag home.

There is one more job, `Role::Overwatch`: a bot holds the best vantage on its
own half (1.5 m or more above its stand, `team::vantages`) with a 6 m leash,
shoots whatever crosses the middle and picks its gun for 40 m. It lost its
round in the tournament (a bot on the gantry is a bot not attacking or
defending), so the plan does not hand it out unless `CTF_ON=overwatch`.

The tournament's changes are each behind a name (`team::trying`). One that is
kept can be switched off with `CTF_OFF=name,...`; one that lost
(`team::REJECTED`: overwatch, react, cutoff, lead, hide) can be switched on with
`CTF_ON`. "What the bench found" below has what each one is and what it scored.

## The CTF bench

`lurelia --world ctfbox --ctf N [--ctf-seconds S] [--ctf-size K]`
(`src/tools/ctf.rs`) plays the team brain against its frozen former self, the
fixed control and the rush, K bots a side (default 3), each seed twice with the sides
swapped, with no window or GPU. It reports matches won and drawn, captures and
grabs a minute, the share of carries that score, how long a carry lasts, how
far from home carriers die, returns, kills, and each side's time by job.
Matches are played to the clock, not to a capture count. With every
tournament change switched off (`CTF_OFF`), the `plan v base` row is the twin
check: the two sides are then the same brain, and on a mirrored map its record
must come out level.

`--ctf-skill NAME` sets the bots' difficulty. `tools/ctf_pair.sh ROUND NAME`
runs the bench at average and adept on both maps with change NAME on and off
from one build, side by side, and `tools/ctf_score.py` scores the tables. Pair
every comparison: the bench repeats exactly from one build, but anything else
that changes between builds moves its numbers by several matches a row.

`CTF_AB=attack1,carry2` plays a single row instead: the plan with those role
changes (`brain::TWEAKS`, carried on each order as `Order::tweaks`) against the
plan without them. Both sides deal jobs the same way, so only how the jobs are
played differs; `CTF_AB=none` must come out level. `tools/ctf_ab.sh` runs
several variants at once over the four settings and `tools/ctf_ab.py` scores
them; `CTF_SEED0` starts the seeds elsewhere for a check on unseen matches.
"What the bench found" below has the role changes tried and what they scored.

`worlds/ctfbox` is the instrument (a flat roofed room with two pillars a half)
and `worlds/sluice` the map (three lanes, mirrored). Run both: the standings
differ between them in the same way the weapons do between duelbox and
Slagdeck.

### What the bench found

The score is the sum over the bench's rows of (won - lost) / matches, each
change played on and off from one build, 20 matches a row.

The tournament tried ten changes to the team plan (`team::trying`). Two were
kept:

- `standoff`: with both flags out, every bot but the carrier retrieves, and
  nobody escorts until the thief is dealt with.
- `press`: attackers' urgency is raised to 0.85, over the bar for breaking off
  and for chasing.

The five that lost and are still in the code behind `CTF_ON` are `overwatch`
(a bot parked on high ground is one not attacking or defending), `react`
(defenders leave their posts too soon), `cutoff` (heading the thief off
arrives where it is not yet), `lead` (an escort in front dies first) and
`hide` (better on ctfbox, worse on Sluice). Gathering attackers before a push
and a first post closer to the stand also lost and were taken out. Head to
head, the final plan beat the one it started from 27-38-15 over
80 matches.

Of the 24 role changes (`brain::TWEAKS`), one is clearly better: `attack1`
(within 15 m of the flag an attacker stops trading and runs for it, shooting
on the move). It is now on by default as `commit`. `carry2` (a carrier dodges
while it sees an enemy) and `escort2` (an escort prefers enemies near the
carrier) scored well at first and fell to noise on unseen seeds. The defend,
retrieve and return changes were level or worse. Of the stance changes,
`stance` (the defender holds midfield while home is quiet) and `reset` (the
defender attacks too after a wipe) were level to slightly positive and are on
by default because they make a more open game against a player. `group` and
`lane` lost badly: attackers spread over the lanes are also the side's screen
in the middle.

ctfbox at adept is a stalemate whatever the plan: a skilled side in one open
room kills every attacker before the flag.
