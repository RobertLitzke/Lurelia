# Shooters

A second kind of game on the same engine: guns, damage, and rules about what
happens when you die. The first one is an arena deathmatch on Slagdeck
(`worlds/slagdeck`), laid out as a classic arena. A later one might be an
open-world sniper game, so the split below keeps the rules out of the weapons.

## Three layers

| Layer | What it knows | Where |
|---|---|---|
| Weapons | one gun: its fire modes, damage, ammo, how it looks and sounds | `library/weapons/*.json`, `src/combat/weapon.rs` |
| Combat | firing a shot and what it hits: rays, flying shells, splash, push, damage to the player and to creatures | `src/combat.rs`, `src/combat/shot.rs`, `src/combat/fx.rs` |
| Rules | health, armour, respawn, score, the loadout a world hands out, the HUD | `src/shooter.rs` (a `modes::Activity`), `"shooter"` in `world.json` |

Combat never reads the rules, and the rules never reach inside a weapon. A
sniper game writes a different `Rules` (one life, no respawn, long sight
lines) and reuses both layers below it; an arena map writes only
`world.json`.

## A weapon

`library/weapons/<name>.json`, a library entry like any other. Two fire
modes, `fire` (left button) and `alt` (right), each one of:

- `hitscan` — a ray, `pellets` of them inside `spread_deg`, falling off over
  `range`. The minigun, the sidearm, the sniper rifle and the Digger's blast.
- `shell` — a body that flies at `speed`, drops at `gravity`, and bursts on
  what it touches: `splash` damage inside `radius`, `push` on whoever it
  catches (the player included, scaled by `self_damage` and `self_push` —
  that is the rocket jump), and `bounces` for scrap shards and razor discs.
  `bounce_keep` is the share of its speed it keeps through each bounce (0.45
  unless stated; the skipjack disc keeps 0.72 and has no fuse, so it ends on
  a body or on the wall after its last bounce, never in the air).
- `beam` — a ray held on a target rather than fired: `damage` is per second,
  the line is resolved afresh every step the trigger is down, and the store
  runs down at `rpm` rounds a minute while it is live. Both of the pulse
  gun's buttons, and the bore.

Ammo is per weapon, `ammo_max` and `per_shot`; `ammo_max: 0` is a weapon with
nothing in it to run out of, which is the Digger. Guns that name the same
`ammo_type` take one kind of round out of one store: the sidearm and the
minigun are both `bullets`, so what either fires is gone from both and a pad
for either fills both, while each keeps its own magazine. Guns of one type must
agree on `ammo_max` (the validator checks). `rpm` sets the
cadence, `spin_up` the seconds before a minigun reaches it, `kick` how far the
view climbs, `mag` and `reload_s` the magazine.

The remaining fields are each one gun's own trick, and each is a few lines of
engine behind a number in a file:

| Field | What it does | Whose |
|---|---|---|
| `charge_s`, `charge_mult` | the shot is held back while the button is down and is worth up to `charge_mult` when it goes, which is when the button comes up; a full charge stays loaded until then | goo gun's alt |
| `stick_s` | how long a shell waits where it landed before it goes off; it goes sooner if somebody comes within reach. With a `goo_r` it is the glob's fuse | goo gun |
| `goo_r` | the shell lands as this many metres of real sludge voxels, which cling, slump, stack and set each other off | goo gun |
| `head_mult` | a ray through the top 18% of a body is worth this much more | sniper rifle, sidearm |
| `zoom` | the right button narrows the view to this share of it and fires nothing | sniper rifle |
| `bore_r` | the ray cuts a tunnel this many metres wide through the world as it goes, and does not stop on a wall | the bore |
| `burst`, `burst_gap_s` | fires `burst` rounds at `rpm`, then waits `burst_gap_s`; a burst that has started finishes even if the button comes up | sidearm's alt |
| `splash.curve` | how the burst's damage falls from centre to rim: (1 - x) to this power, 1 a straight line; above 1 the centre and the reach are the same and the damage dies away sooner between (1.22 is about 10% less in all) | rocket launcher |
| `splash.crater` | the burst takes a ball of this radius in metres out of the world where it goes off; 0.33 is about four voxels of floor, and under 0.25 is the one solid voxel nearest the burst | rocket launcher, minigun's alt |
| `burst_spread_deg` | degrees added to the spread for each round already fired in a burst | sidearm's alt |
| `ring` | the pellets go out evenly round the rim of the `spread_deg` cone, all at one speed, instead of scattered inside it | minigun's alt |
| `pierce_m` | metres of solid the round goes through, hollowing the way; on a charging mode, scaled by the charge | sniper rifle's alt |
| `guide` | radians a second a shell turns toward its owner's crosshair | rocket launcher's alt |
| `ammo_type` | the store the gun's rounds come out of, shared with every gun naming the same type | sidearm, minigun |

A gun with a `zoom` may have an `alt` only if the alt charges: holding the
button scopes and winds the shot up, letting go fires it.

## The arsenal

Ten guns, on the digits 1 to 9 and 0, each one a different question about
range and timing:

| Slot | Gun | Left | Right |
|---|---|---|---|
| 1 | Digger | a three-metre blast that takes none of its own (nozzle to the floor and ride the shove onto the gantry), and a spade that bites the wall while the handle is squeezed | a weaker squeeze that throws them much further |
| 2 | sidearm | a P90-style bullpup, 40 rounds along the top, half again for a head shot; takes the minigun's bullets | bursts of six that open out as the muzzle climbs |
| 3 | goo gun | lobbed globs that land as real sludge: they cling, stack and go off in a chain | hold to grow one glob, up to three times the burst |
| 4 | bore | a cutting beam sixty metres that takes a metre of wall with it | the same thing wide and short: a doorway through a partition |
| 5 | pulse gun | a beam held on a target, damage by the second | the same stream focused: half again the reach, steadier |
| 6 | skipjack | razor discs that bounce four times, out of a bright plastic toy | one that bursts where it lands |
| 7 | minigun | a wall of bullets after a spin-up | all six barrels at once: a ring of burning flechettes at the scrap gun's pace, each biting a voxel out of what it hits |
| 8 | scrap gun | nine shards you can watch fly | one shell lobbed whole |
| 9 | rocket launcher | a rocket that bursts on what it meets and takes a few voxels out of it | a guided rocket that follows the crosshair with a lag, a fifth weaker |
| 0 | sniper rifle | one shot, no falloff, twice the damage in the head | the sight, and a held charge worth half again that goes through 1.25 m of wall; the left button fires it, and letting the sight down drops it |

## The bench

`lurelia --world duelbox --duel 8 --duel-seconds 45` (src/tools/duel.rs).

The bench is how the numbers in the weapon files are tuned. A live match on
Slagdeck cannot settle one: two bots on a map with a pit and three
galleries spend most of a round out of each other's sight, the map hands out
all ten guns, and what comes out is which gun the bots *chose*. So
`worlds/duelbox` is one empty room -- fifty-eight metres by thirty-four, flat,
roofed, no cover, no height, no pickups, two spawns at opposite ends -- and
the bench forces one weapon per fighter with `Shooter::arm`. Both bots are the
same person with the same aim, so skill cannot be the difference, and the
player is parked outside the room, because a body standing still in the middle
of a weapons bench is a free kill that goes to whichever gun reaches it.

Two passes. The **bench** gives both bots the same gun and reports how it
behaves: kills a minute, what share of rounds land, what a round is worth when
it does, damage a minute, the range the fight happens at, and how much of the
firing is on the right button. The **table** fights every gun against every
other, both ways round so the spawn is not what wins, and scores net kills.
A round of eight takes about a minute, and the same arguments give the same
table every time -- there is no wall clock in the loop.

It runs on any world with a `shooter` block, and should be run on two. On a
map with pads it drops the weapon pads and any ammunition for a gun nobody in
the match carries, so the only thing on the floor is health, armour and
rounds for the two guns being asked about.

**The standing is a property of the map, and one map is not an answer.** The
same ten guns, the same bots, the same session: the minigun is first in the
empty room and near the bottom on Slagdeck, and the skipjack is last in the
room and in the middle on Slagdeck. Hitscan wins where there is nothing to
hide behind and loses where there is, and a weapon whose whole use is a
corner cannot show it in a room with none. Slagdeck is the arena, so Slagdeck
is the authority on the numbers; the box is a control that isolates the gun
from the map, which is what makes it the right instrument for a *mechanism*
and the wrong one for a balance pass. `shots/guns/duel_slagdeck.txt` and
`duel_duelbox.txt` are both tables as they stand.

What the box cannot see at all: `WANT_FAR` pins the fight at about sixteen
metres, so neither the sniper rifle's four hundred nor the scrap gun's cone at
touching distance is ever the question.

## What they look like

Every weapon is a voxel model in a 32 cube (`src/combat/viewmodel.rs`), drawn
as an ordinary entity rather than a HUD overlay, so it takes the light of the
room it is in. `--gun-sheet` lays all ten out side by side, turned through
half a revolution, which is the only way to see a model that is otherwise
only ever viewed from the one place it is held.

The cube is a convenience for drawing them: ten guns built to fill the same
box would come out the same size. `SCALE` gives each shape a size of its own,
from the sidearm at 0.62 to the sniper rifle at 1.18, and
`the_guns_are_not_all_the_same_box` measures the arsenal in centimetres and
fails if they come out too alike. The sidearm is 32 cm long and the sniper
rifle 63.

Two of them are not iron. The Digger is a tool off a shelf in the works with
the cartridge swapped for something that goes off and a spade welded under
the nozzle, and the skipjack is bright orange plastic with a blue drum -- the palette carries four plastic
colours for them (slots 18..22). A thing that is plainly a toy and plainly
lethal reads across a hall in a way no amount of grey does.

## A weapon that writes to the world

The bore, the Digger, the goo gun and the rocket's crater change the map. An
activity is handed the store to **read** and not to write, because the store,
the streamer and the chunks the mesher is rebuilding are one thing and `Game`
owns all of it.

`ActivityCtx::edits` is the seam that lets a weapon write. A weapon pushes
`([x, y, z], material)` onto it, `Game` applies the lot with `Streamer::edit` at the end of the tick,
and the mesher rebuilds what changed. Two rules fall out of where the seam
sits, and both are deliberate:

- **Nothing an activity writes this tick can be read back this tick.**
  Anything that places voxels has to keep its own account of what it put
  where, which is the right discipline anyway.
- **The floor rule is `Game`'s.** Nothing more than `BURIED_LIMIT` below the
  generated ground may be taken away, because the world keeps no detail down
  there and a hole into it is a hole out of the world. `Game::dig` knows
  that; the bore does not have to, and should not.

`Combat::bore` walks the ray in quarter-metre steps -- the voxel edge -- and
takes every voxel within `bore_r` of the *line*, not of the step, or the
tunnel comes out as a string of beads. The ray itself does not stop on the
wall it is eating: it reaches its full `range` and hits whoever is behind it.

### The spade

The Digger is the other way round: it takes one ball out of the first wall it
reaches and stops there. `dig` on the weapon (not on a fire mode) gives it a
`radius`, an `rpm` and a `range`, and `Combat::bite` cuts the same ball
`Game::dig` cuts out in the world -- 0.45 m at 430 bites a minute, which is
the rate a shovel works at.

The clock is the weapon's own and not the fire mode's: the blast goes off at
84 rounds a minute and the spade bites at 430, so one squeeze of the handle
does both at its own pace. `Held::pull` reports it as `Pulled::dig`, which is
why a bot gets it for nothing -- the player's path and `bots_fire` both read
the same field, and a bot at the bottom of a borer hole can cut its way out
of it.

### Sludge is matter

The goo gun is the harder half: the bore only takes
voxels away, and a glob has to put them somewhere and then live there.

A shell with a `goo_r` that finds a body still bursts on it. One that finds a
wall, a floor or the end of its fuse lands as a ball of sludge voxels
(`library/materials/sludge.json`) laid wherever there is air for it, and
`combat::sludge` keeps it from there: its own list of voxels, its fuse, its
burst and whose it was.

What a blob does each tick is all there is to the physics, and it is enough:

- **It clings to any face it touches** -- a wall, a ceiling, the floor, or
  another blob. That is what sticks a glob to a wall, and it is also
  what makes globs stack, so a doorway can be filled or a step built.
- **With nothing adjacent at all it slumps**, a voxel at a time, five metres
  a second and no acceleration: sludge oozes. Bore away what a stack stands
  on and the stack comes down.
- **It goes off** when its fuse runs out or somebody comes within reach, and
  its burst lights the fuse of every blob it reaches rather than setting
  them off inline. Each link of a chain is its own bang, 0.08 s apart, and a
  chain of forty cannot run away down the call stack.

Two things in there are forced by the seam rather than chosen:

- The store answers for the world and a `claimed` set answers for the
  sludge, because a blob laid this tick is not in the store until `Game` has
  applied the edits. Neither is asked about the other.
- **A burst waits one step.** The voxels go back to air on the queue, and the
  burst happens on the next tick, when they are gone. A burst resolved in
  the same tick goes off in the middle of its own glob, and `burst` checks
  that it can see each body it hurts -- so it would see nothing and touch
  nobody. One step is 1/60 s and reads as the same bang.

The cap is forty blobs on the map at once; past it the oldest is given a
zero fuse, so a held trigger reads as a chain going off rather than as a glob
vanishing.

## The arena rules

`"shooter"` in `world.json`:

```json
"shooter": {
  "weapons": ["minigun", "rocket_launcher", "scrap_gun"],
  "health": 100, "armour": 0,
  "respawn_s": 5.0,
  "spawns": [[x, z, bearing], ...],
  "fall_damage": false,
  "give_all": true
}
```

`respawn_s` is how long the dead wait: 5 s by default and in every world the
game lists (duelbox, a bench instrument, keeps 2 s). `give_all` hands out every weapon full at spawn. Without it a respawn hands
back the lowest gun that takes ammunition and everything that takes none --
the sidearm and the Digger -- and the rest are on the floor as pads.
An ammunition pad for a gun you are not carrying still hands its rounds over
(`ammo_without_gun`, on unless a world's `shooter` block says `false`): they
are kept for the gun and given with it when you find it, and lost at a death,
as in the older arena shooters.
A weapon with no ammunition is never taken away, because there is nothing for
a pad to give it back and a slot that could never be filled again is worse
than a weapon everybody has.

Slagdeck has weapon pads but runs with `give_all` on, so for now its weapon pads
only top up ammunition. With it off, a bot that ran its guns dry and had nothing
to swap to stood about unarmed; the floor becomes the whole economy again once
that case is handled.

## Two sides

The launch screen offers TEAM DEATHMATCH on any arena and CAPTURE THE FLAG on
a world whose `shooter` block has a `ctf` block (`launch::Mode`). Either one sets
`ShooterDef::teams`, and `shooter/teams.rs` deals the sides: a bot the world put
on a side stays there (`BotDef::team`), the rest go to the shorter side, and
the player joins the side the launch screen named or the shorter one. Side 0 is
cyan and side 1 magenta, the first and third of `avatar::TEAM_COLOURS`, each
shaded a little per bot. A world's own bot colour is not used in a team match.

The rest is one rule applied in a few places:

- A teammate's round does `friendly_fire` of its damage (0 unless the world
  says otherwise) and all of its push, so a rocket still lifts a teammate. A
  blow from anything that can also hurt its own firer (a burst with
  `self_damage`, a shell with `self_hit`: rockets, goo, scrap, the skipjack)
  lands in full whatever `friendly_fire` says. It
  scores no hit, sets no blame and does not count as a blow on a bot
  (`Shooter::settle`).
- A teammate killed by one is nobody's kill (`down`).
- A body comes back at its own side's spawns (`SpawnPoint::team`), as far as
  it can from the other side.
- In team deathmatch a side's kills between them reach the target
  (`round::Goal::TeamKills`). The scoreboard groups the sides.

## Capture the flag

```json
"ctf": {
  "flags": [{"team": 0, "at": [x, z]}, {"team": 1, "at": [x, z]}],
  "captures": 3,
  "time_s": 600,
  "return_s": 25,
  "need_home": true
}
```

`shooter/flag.rs` owns the flags the way `pickup.rs` owns the pads, and
decides every move a flag makes:

| From | What happens | To |
|---|---|---|
| Home | an enemy comes within 1.3 m of the stand | Carried |
| Carried | the carrier dies | Dropped where they fell, on the floor under it |
| Dropped | one of its own side comes within reach | Home, a return |
| Dropped | an enemy comes within reach | Carried |
| Dropped | `return_s` runs out, or it lies in burning ground or the void | Home |
| Carried | the carrier reaches their own stand with their own flag home | Home, a capture |

Everything else listens to the `flag::Event`s that come out of a step. The
scores count grabs, captures and returns, the feed gets a line, the player gets
an alert across the top of the screen worded for their side (`Shooter::heard_flag`),
and the team brain re-plans at once (docs/BOTS.md, "The team"). The match ends
at the capture count or on the clock, a level match on the clock being a draw
(`round::Goal::Captures`). The command line plays a world's own count and
clock; the launch screen overrides them.

The HUD in a team match has the sides' scores top middle with the clock
between them and, under each, where its flag is: HOME, TAKEN and the carrier's
name, or DOWN and the seconds before it goes home. A player carrying a flag
gets a pulsing border in its colour. `Activity::marks` puts markers on the
screen through walls, pinned to the edge when off it (`modes::draw_marks`,
projected by `Game`, which has the camera). The shooter marks a flag lying
anywhere, a teammate carrying one, our own flag on an enemy's back while one
of our bots can see them, and home while the player carries. These are the
same things a bot's side may know.

A flag is a lit pole and cloth in the side's colour (`avatar::FLAG_M`), on its
stand, lying where it fell, or across a carrier's back. A stand is a lit ring
on the floor (`STAND_M`), there whether or not the flag is. A flag away from
its stand has a tall column of light over it.

`--validate` checks a CTF layout (`teams::check`): a flag for each side within
3 m of a nav node, two spawns a side, a route from each side's spawns to the
other flag and from there home, and the two sides' runs within a tenth of
each other.

The two CTF worlds are written by their scripts, which lay the structure, the
nav graph, the spawns, the flags and the pads together. `worlds/ctfbox/make_box.py`
writes the bench room. `worlds/sluice/make_map.py` writes Sluice, which is
mirrored about its middle by construction (every box is laid twice): two pump
yards across a drained spillway, joined by a gantry three metres up along the
north wall, the spillway floor two metres down with three gate pillars in it,
and a roofed culvert with a dog-leg along the south wall. Neither world issues
the bore or the Digger, which cut routes the nav graph does not know.

## How a body moves in an arena

The arena's movement is `src/player.rs`, behind one flag. `Player::arena` is
set off the world's modes for the player and by the shooter for a bot, which
walks and never rides, and it turns on four things at once:

- **The running pace is a sprint.** 8.3 m/s with nothing held, where a
  walking world does 5. An arena is a place you cross, not a place you hike.
- **Shift is a boost, not a sprint**: 15% over the pace, out of a meter that
  empties in 1.6 s and refills in 4.5 -- `Ride`'s bargain for the bike, down
  to the lock that will not let a boost restart on the dregs. A key you can
  hold forever is not a decision; a meter is. It is the lowest bar in the
  bottom-left corner of the HUD.
- **A direction key tapped twice is a dodge**: 12.2 m/s that way and a little
  lift, which is hard enough that the air-control branch keeps it rather than
  steering out of it. Two presses inside 0.26 s, with a release between them,
  so holding a key is never a dodge.
- **Space inside 0.34 s of a dodge is a dodge-jump**, which lifts 9.1 m/s
  against a standing jump's 7.6 and carries the dodge's run with it. It
  stands in for the ground jump rather than coming on top of it.

`air_jumps` is 3 in an arena and 1 out in a world, where a double jump would
undo every climb and drop the terrain is laid out for. An air jump *sets* the
climb rather than adding to it, so falling fast and jumping still gets you up
and rising fast gains nothing by it.

A world may set its own gravity (`"gravity": {"g": 0.3}`, GUIDE.md "Gravity").
Starfall does: every jump goes about three times as high, and a body in the
air there is steered by its keys however fast it is going.

A bot is the same body with the same flag, so it boosts and triple-jumps for
nothing, and it dodges too -- by tapping the keys, not by being handed a move
the player has not got. `steer::Tapper` is the hand; `Duel::may_dodge` is the
habit of keeping moving and `Duel::struck_dodge` the reaction to a blow
landed, which gets it off the line it was hit along. docs/BOTS.md has why a
dodge needs a hand rather than a flag, and what it is worth.

## The dead

A fighter who is down is no longer driven: the body keeps its box and is moved
by gravity and the walls alone (`physics::tumble`), with whatever killed it
still in it. Every hit carries a `fling` beside its `push` -- a shot tips a man
over along the line it came in on, a burst throws him -- and the two are
separate numbers because `push` is a living fighter's footing, which the rocket
jump is tuned against, and a corpse should go further than a rocket jump is
worth. Under `FLUNG` metres a second the body simply drops where it stood.

A bot a burst kills (`Hit::blast`: a burst, or a shell that bursts) comes
apart instead (`shooter/gibs.rs`): every bone's part and the two chest lamp bars
fly off from the pose it died in, bounce and settle, and are gone when it
respawns. Every dead bot's lamps flash white and gutter out over most of a
second (`lamp_dying`), with sparks off them at the moment of death.

What the player sees of their own hits: a hit marker of four diagonal dashes,
longer for a bigger hit and red on a head shot; a red and white X on the
killing blow; and a gold KILLED NAME banner at 30% of the screen's height, with
DOUBLE / TRIPLE / MULTI KILL under it for kills less than three seconds apart.
A death in the slag within half a second of another fighter's hit is that
fighter's kill (`Fighter::blame`).

## Where you come back

Of four spawn points drawn at random, a body takes the one standing farthest
from everybody still up. Walking the list in turn would hand out the same
corner first in every match, and whoever shot you would know exactly where
you are about to be.

A match also stands the player on a spawn point when it begins, rather than
wherever `Game` put them (with a `progress.json` in the save folder, the spot
they quit from). The placement is retried for a few seconds, because the floor under a spawn
is not there until the world has streamed in (the same wait as the nav graph
and the pads), and it is dropped the moment the player walks off under their
own steam. In a deathmatch the saved position is not read at all; exploring
still comes back to where you were.

## The arena mends

A match is played in the arena as it was built. What the guns take out of it
belongs to that match, so a deathmatch starts from an empty edit map
(`edits.bin` is not read) and never writes one, and playing again mends the
holes in place: the shooter asks through `Activity::wants_world_clean`, and
`Game` empties the edit map and regenerates every loaded column, which is
the same machinery a turn of the season uses (`Streamer::clear_edits`,
`set_turn`). A column is built from the generator and the edit map is
reapplied to it, so emptying the map is the whole of the undo -- and because
the same job refills the store, the collision comes back with the mesh
rather than a step behind it.

Exploring is the other way round: a world you are walking is the one place
where a hole you dug is worth keeping, so nothing here touches what walk
mode saves. A match does not overwrite that file either -- it leaves it
exactly as exploring left it.

## What the engine provides

- `ActivityCtx` carries `&mut Player` and the creatures, so an activity
  can push the player (splash, rocket jump), move them (respawn) and hurt an
  animal.
- `ModeId::Shooter`, which a world allows in `"modes"`. The activity starts
  itself on spawn when the world has a `"shooter"` block, rather than from a
  key, since in an arena the gun is never put away.
- Tracers, shells and bursts are drawn as `render::overlay` lines, which are
  ribbons a fixed width in metres: everything they draw has to start clear of
  the camera or it fills the screen.
- `Activity::instances` lets an activity draw models. The gun in hand goes
  through it rather than onto the HUD, so it is lit by the room it is in.
- `render/lamps.rs` and the `lamp` chronicle event. An arena has a roof, and
  every other light in the engine comes from the sky.

## What is on the floor

`src/shooter/pickup.rs`. Four kinds -- health, armour, ammo, a weapon -- and
they are Rust rather than library files, because each one is a thing the rules
do to a `Fighter` and there are only four. What is data is where they stand,
how much they give and how long they take to come back, which lives in the
world's `shooter.pickups`.

Three rules do most of the work:

- **A pad over somebody who has no use for it stays standing.** Walk over a
  health pack at full health and nothing happens, which has to be true or the
  player learns to edge around their own pickups.
- **The clock is per pad, not per kind.** That is what lets the rocket
  launcher be the slow one, and it is most of what makes a map a map.
- **The pad rates itself for whoever is asking.** `Pad::good` takes a
  `Fighter` and answers how much good it would do them; `brain::Lure` is that
  answer plus a point in the world, and it is the whole of what a bot is told.
  The brain never learns what a health pack is, so a fifth kind of pickup
  needs no bot code -- the same arrangement as `brain::desire`, where a gun
  rates itself for the range rather than the bot knowing what a scrap gun
  is. A bot subtracts the walk (a third of a point per ten metres) and needs
  to want the thing four times as badly to break off a fight for it as to
  fetch it while idle.

### Three sizes of health, and being able to see them

Health comes in a vial (+10), a pack (+25) and a crate (+50). The amount is
the only thing a world states; `pickup::Size::of` reads the size off it, and
the size decides how long it takes to come back (12, 20 and 40 seconds). So
a map asks for a crate by writing `"amount": 50.0`, and a pad with no amount
is the middle one. How big it is drawn is `Pad::scale`, which is not in
steps: the amount over the middle one (25 health, 50 armour) to the power
0.55, held between 0.6 and 2.2. Armour is sized the same way, so a 30-point
plate is smaller than the 50 and a 100-point one is bigger.

`Pad::good` multiplies the bot's urgency by how much of the gap the pad
actually closes, or the three sizes would be three looks and one decision: a
bot at ten health would want a vial exactly as much as the crate across the
hall and take whichever was nearer. The term starts at 0.4 rather than 0, so
a vial still beats nothing.

What is on a pad turns over it, see-through (`Instance::glass`, drawn after
everything solid with no depth written and no shadow cast), so it reads as
something floating there to be taken rather than a solid in the way:

- **Health** is a cross and **armour** a curved breastplate, both in the
  lamp's emissive slot and tinted green and blue, the size of what they
  give. Neither is flat, because the pad turns on the spot and a flat shape
  spends a quarter of every turn edge-on.
- **Ammunition** is the rounds themselves, one look per kind
  (`viewmodel::AMMOS`): a can of bullets, two rockets, a hopper of scrap, a
  jar of sludge, the bore's lit core, a pulse cell, a stack of razor discs,
  three sniper rounds. The look follows the gun's `ammo_type` where it has
  one, else its `view`, so guns that share a store share a look.
- **A gun** is the gun.

Each pad stands on a **platform**: a square of half-height slab a metre
across (wider under a pickup drawn bigger), in the kind's colour for health
and armour and the gun's own colour under a gun and under its ammunition.
The rules lay it, not the world (`Pickups::platforms`, pushed through the
activity's voxel edits once a pad has found its floor), so any world with
pickups gets them and the world file says nothing about them. Only level
floor round the pad is covered, so a platform at the foot of a wall does not
climb it. A pad with less than two metres of air over the platform or the
half metre round it gets none: stepping up onto it lifts the head an eighth
of a metre, and in a tunnel built to body height that hits the ceiling, so
the platform would be a wall. The materials are `pickup_pad_health`, `pickup_pad_armour` and
`weapon_pad_<gun>` (`PickupDef::pad_material`), and `worldload::reach` gives
them ids in a world whose shooter has pads.

Two more marks say what is on the floor from a distance:

- **The platform**, which stays when the pickup has been taken, so an empty
  pad still says what comes back there.
- **A column of light** over every pad that is standing, in the pad's own
  colour (`Shooter::beacons`, drawn into the overlay). Depth-tested, so a
  wall still hides one: it says where a thing is from across a room, not
  through the floor. It fades out with height, and it is not drawn within
  two and a half metres, where it would be in your face and say nothing you
  do not know.

Pads do not wait for the nav graph, which on Slagdeck never fully settles
because a few of its nodes never find a floor. They go down on the first
pass at the height the world states, and `settle_pads` asks again for the
ones whose column had not loaded.

`nothing_the_avatar_draws_is_empty` checks every model the avatar hands out,
the pad blob, the cross and the plate included, because a hand-built shape that comes out empty draws
nothing without complaint, which a library one does not.

An arsenal holds every weapon the world issues whether or not you have
found it (`Held::have`), because a slot has to be a slot either way and a gun
you pick up again has to remember the rounds you had for it. `give_all` is the
world's choice of respawn: with it a respawn hands over
everything, without it a respawn hands back the sidearm and the guns are where
the map keeps them. Keeping what you were carrying would be the worst of both,
since whoever was winning would stay armed through death.
`worlds/slagdeck/make_arena.py` places Slagdeck's pads on nav nodes rather
than at coordinates of their own, which gets three things for free -- the
floor is known to be standable, nothing is inside a pillar, and a bot can
always route to the thing it wants, because the thing it wants *is* a node.

## Slagdeck's floor

The weapon pads are placed by the range each gun is for: the sniper rifle at
the blind end of the west gallery, the bore on the east walkway over the
hall, the pulse gun in the west shop and the skipjack in the east, and the
goo gun down the sump ramp. The four health pads are one pack on the open
deck, the crate on the far corner where standing still costs something, and
a vial on each wing. The Digger and the sidearm are on nobody's floor.
`worlds/slagdeck/make_arena.py` writes them.

## Water you can get into

The world's own water is a field and not voxels -- `game::water_over` reads
a surface and a current out of `Fields`. A pool inside a structure is made
of voxels instead, in a material flagged `water`:

- `Store::cell` answers `Cell::Water` for a voxel whose material carries the
  `WATER` flag. `Store::solid` still counts it solid, so creatures, the bike
  and the lifts behave exactly as they did.
- `physics::hits_wet` is `hits` with water let through: the collision a body
  that can swim uses. The player and the shooter's bots move with it.
- `physics::water_voxel_top` gives the surface over a point inside water, by
  running up the column to the first voxel that is not water.
- `game::water_at` is `water_over` with that counted in, and is what anything
  stepping a body asks. A voxel pool wins over the field, which under an
  arena is usually the sea, far below.

So the swim, the wade, the current, the dive and the underwater view all come
out of the code that handles rivers; the pool only has to answer the same
question. One move is its own: a tank with vertical sides would hold a
swimmer forever, because the float keeps the eyes at the waterline and that
leaves the feet a body's length below the rim. Swimming into a wall with a
floor just above the surface pushes off hard enough to land on it
(`Player::haul_out`).
