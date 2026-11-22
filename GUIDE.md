# Lurelia guide

The detailed guide: every control, mode, world and tool, with the numbers
behind them. [README.md](README.md) is the short introduction and
[ARCHITECTURE.md](ARCHITECTURE.md) says where the code for each part lives.

Lurelia is a voxel engine and exploration game written from scratch in
Rust and wgpu. Voxels are 0.25 m on an edge. Worlds are
built from a short story and a chronicle of events: a 1 km volcanic island, a
red-rock desert and a tower-karst lagoon.
They have synthesized textures, cascaded shadows, a moving sun, water,
creatures with their own behaviour, things to collect, a bike, a glider,
climbing and a rope, a camera with photo tasks, and digging and building.
Every world is deterministic from its files.

## Build and run

Needs Rust (installed with rustup). Then:

```bash
cargo run --release
```

The first build takes about a minute; later builds take 5 to 20 seconds.

The game opens on the title screen (borderless fullscreen, or a maximized
window with fullscreen off): RETURN (the world you were last in, as you left
it), OPEN WORLD, settings, controls, or quit. OPEN WORLD is the one screen
that sets a run up: the game and the world are dropdowns, the lines under them
are what this game and this world put on offer, and GO and BACK stand at the
foot of the screen. The game comes first, and the world list is then the
worlds that offer it: each world lists its games in world.json `"games"`, by
their names in `library/games/` (a world with no list is one to explore).
Picking a game the world on the line does not offer moves to the world that
game was last played in. A list longer than the window scrolls (the wheel, the
arrow keys, or the bar beside it) rather than running under the buttons. EXPLORE is the game as it has always been;
DEATHMATCH is the arena, on a world whose world.json has `"shooter"` rules
(Porch); TEAM DEATHMATCH is the same arena in two sides, CAPTURE THE
FLAG needs those rules to have a `ctf` block (Sluice, Megalith), and INSTAGIB
is a variant of deathmatch with rules of its own (`library/games/instagib.json`). The
options are src/launch.rs: the animals, the day and night and each way of
travelling the world allows, then the match's difficulty, bots and target
score, and in a team match your side (and for capture the flag the captures
and time limit in place of a target). A line that could change nothing is
left out: ANIMALS where no biome of the world has any, the clock and the
weather on a map in space, WEATHER where the climate has one kind of day.
What you pick is kept per world and per game, so each world comes back as you
last played it; a small star before a value says it is not the world's own,
and DEFAULTS puts them all back.

The world list holds the worlds that offer the game, in groups: the last three worlds opened (once there are
two), then places to explore, arenas, and the workshop (`local/worlds/` that
is not an arena). Home, End, Page Up and Page Down move through it. Under the
list the screen says what the highlighted world is: its world.json `blurb`
(or the first sentence of its story.md) and a line of facts counted from its
files (size, biomes, species, seasons). A world's `title` is the name it is
shown by where the folder name is not it, and `"listed": false` keeps a bench
or a test off the list while `--world` still opens it. Behind the menu it cycles through the
pictures in `screenshots/` and `photos/` (yours) and each world's `pictures/`
folder (bundled, any file name), cropped to 16:9 and cross-faded. The title
screen shows every world's; once a world is on the line, OPEN WORLD shows only
that world's, crossfading as you step through them, and falls back to all of
them for a world with none of its own. A world loads behind a loading screen
showing that world's pictures: first the land is built on a worker thread, then the
view around the player streams in (at most 30 s) before play starts.
`--world NAME` (or `--bench`) skips the title screen and loads that world with
the world's own defaults -- the game it is built for, everything in it;
`--mode explore|fps` overrides the game.

Esc in a world opens the pause menu: resume, save, settings, controls, back to
the main menu (saves and unloads the world), exit game (saves). The window
losing focus opens it too. H (controls) and L (worlds) pressed in the game
close straight back to the game; opened from the pause menu, they go back to
it. Menus take W/S or the arrows, Enter, Esc, and the
mouse; left and right (or right click) step a setting back and forth.

The settings are the player's, not a world's, and live in
`saves/settings.json`: fullscreen, style, shadows, sloped ground,
antialiasing, view distance (64 to 256 m, applied at the next world load),
vsync, the performance stats overlay (F3), the debug minimap (F6), sound on
or off with its four levels (VOLUME, AMBIENCE, EFFECTS and MUSIC, 0 to 100
in tens; see "Sound"), and the last world played. Antialiasing is off or 4x; the renderer reads it when
it is made, so a change waits for the next start, and the line says so.
On an RTX 3060 at 1080p, 4x costs about 1.0 ms of frame time over off. 2x is
not offered: it measured no cheaper than 4x on a 3060 and looks worse, and a
settings file that says 2 shows as 4. `--msaa 2` still takes it,
for comparing. The in-game
keys G, T, B, F3, F6 and F11 change the same settings. Flags such as `--style`,
`--no-shadows`, `--blocky`, `--view`, `--msaa` and `--vsync` override them for the run
(and are saved if a setting changes). Headless, `--show-menu` with
`title|settings|worlds|loading|pause` draws a menu screen into `--screenshot`,
`--edit NAME` draws the EDIT page over its studio (see "The EDIT page"),
and `--show-menu look`, `look-light`,
`look-ramp`, `look-tint`, `look-map`, `look-grade` or `look-pick` draws the
LOOK page and its colour picker over the world.

On Windows with Rust's GNU toolchain, three crates in `Cargo.lock` are held
one patch release back (`libloading` 0.8.8, `parking_lot` 0.12.3 with
`parking_lot_core` 0.9.10, `winapi-util` 0.1.9): their newer releases link
Windows DLLs by name, which on the GNU toolchain needs an assembler Rust
doesn't ship. A plain `cargo update` will undo the pins and break that build.
The MSVC toolchain doesn't need them. wgpu is pinned at 25.0.2, and cpal (the
sound device) at 0.16.0 for the same reason: 0.17 and later depend on
`windows` 0.62, which links by name, and fail on the GNU toolchain with
`error calling dlltool`.

## Controls

| Key | Action |
|---|---|
| WASD, mouse | Move and look |
| Space | Jump; in water, swim up or climb out onto a bank; in a deathmatch, up to three times in one airtime |
| WASD tapped twice (deathmatch) | Dodge that way; Space straight after it is a dodge-jump |
| Ctrl (swimming) | Dive; let go to float back up |
| Ctrl (bike) | Tuck: less drag and a harder pull downhill, and it stores a charge the next jump spends |
| Shift | Sprint; a sprint burst on the bike; boost while gliding; in a deathmatch, the boost over the running pace |
| X | Get on or off the mountain bike |
| K | Get into the kayak at the water's edge (or in the shallows), or out of it onto the nearest footing |
| U | Get into the world's vehicle (Ribshore's sand rail), or out of it; far from the parked one, bring it over; held while driving, put it back on its wheels |
| C | Glide: spring about 3.7 m up from the ground and open the wings, open them in the air, fold them in flight |
| Space in a long fall | Open the wings (needs 6 m of air below) |
| Walk into a wall | Climb it |
| Z or a mouse side button | Get the rope out (it is out anyway while climbing); again to throw it at the mark, let go, or throw at the next anchor; at nothing, put it away |
| A D (gliding) | Roll; both together throws a barrel roll |
| Alt (gliding) | Look around without steering |
| V | First person or chase camera while riding, gliding, climbing or on the rope |
| F4 | Free camera (no-clip; Space and Ctrl for up and down) |
| F | Take the camera out or put it away (see "The camera") |
| J | Logbook (the photo tasks) and, a page on, the scrapbook; A and D turn pages |
| R | Load a new roll of film when the roll is finished |
| F2 | Screenshot mode on or off (Esc also leaves it) |
| Enter, F12 | Save a screenshot: in screenshot mode, or F12 at any time for the view without the HUD. With a camera photo waiting, Enter keeps it in the scrapbook |
| Left mouse | Dig (hold to keep digging) |
| Right mouse | Build with the selected block |
| 1 to 9, wheel | Pick a block; change the brush size (1, 2 or 4 voxels) |
| L | Open world: world, game and its settings |
| G | Shadows on or off |
| T | Switch between the classic and calm styles |
| N | Jump to the next time of day: dawn, morning, midday, afternoon, golden hour, sunset, dusk, twilight, night |
| M | The next kind of weather: sun, heat, clouds, rain, snow, fog. It also pins the weather there (see "Weather") |
| Y | The next season this world has. It also pins the year there (see "Seasons") |
| B | Sloped ground on or off |
| F7 | The LOOK page: the light, the colour ramps and the grade (see "The LOOK page") |
| F8 | The EDIT page: a creature or plant in a studio, with its library entry beside it (see "The EDIT page") |
| H or F1 / F3 | Help / stats overlay. Its last line names what the crosshair is on: a creature or find, else the material, its id, distance and voxel, and `IN <name>` when that voxel belongs to a library structure |
| F6 | Debug minimap: the ground 320 m round you in the map colours, north up |
| F5 | Save (also saved on exit and every 30 s after edits) |
| F9 | Sound readout: what the ambience round you is made of, and why (see "Sound") |
| F11 | Fullscreen (saved in the settings) |
| Esc | Pause menu |

Saves go to `saves/<world>/`: `edits.bin` (every voxel you changed) and
`progress.json` (position, finds, film and tasks, the chosen style and whether
slopes are on). `--fresh` starts without edits or progress, and `--save DIR`
uses another folder. The random island (`--world random`) keeps its
parameters in `saves/random/world.json`, written with a new seed the first
time it is played; delete it to roll another island, or edit it to shape one.

## Riding, gliding and climbing

`src/ride.rs` holds the bike and the parts all the modes share; gliding is in
`src/ride/glide.rs` and climbing and the rope in `src/ride/climb.rs`. Every
mode widens the view with speed, leans the camera into turns and blurs the
edges of the screen. Every mode starts in first person; V swaps to a chase
camera with a rider model, which follows a point that trails the rider a
little, more up and down than across, so jumps and swings read as arcs. In
first person you see your gear: the bike's bar, gloves and sleeves at the
bottom of the view, the wing sleeves running out to the lower corners (they
bank with you), your hands on the rock while climbing, and a glove on the
rope. The line above the bottom of the screen names the mode: speed on the
bike, GLIDING with speed, height and climb rate, CLIMBING with height, ROPE
with how much is out. On foot, sprinting is 8.3 m/s, and momentum carried
into a jump (off the rope, a wall or the bike) is kept rather than braked by
the keys. A deathmatch moves on its own terms -- see below.

The mountain bike (X) follows the drawn ground and leaves it wherever the
ground falls away faster than gravity can follow, so any lip or crest taken at
speed is a jump. It rolls along on its own at about 9.4 m/s (34 km/h); W
pedals it to 12.2 m/s and S brakes. On the flat that is its top speed: the
speed worth having comes from a hill, a tuck or a token. It turns toward where you look as fast as
its grip allows (A and D turn the view too, winding up to 2 rad/s over about
a tenth of a second and back down on release, so a tap nudges the view); look
far to the side and it keeps going straight. Shift is a sprint out of the saddle: a burst of about 1.4 s
that takes it toward 90 km/h and refills in 4 s, shown by the bar under the
speed. Downhill it speeds up gently and levels off near 70 km/h on steep
ground. Holding Space loads the suspension (the view sinks) and letting go
pops the bike up; let go just after the wheels leave a lip and the pop adds
to the jump. In the air the bike hangs a moment over the top of the arc and
comes down a little harder. Landing on a downslope after half a second of
air is a clean landing and gives a little speed; landing flat bleeds it.
Bushes slow the bike but don't stop it; trunks and walls do.

Ctrl tucks. Tucked, the bike holds less air drag and the slope pulls about
40% harder, but there is little pedalling in that position, so it is a
downhill move. A tuck down a real descent at speed fills a charge (the HUD
shows TUCK and the percentage) in a couple of seconds, and the charge bleeds
away in five once you sit up. The next time the wheels leave the ground, off
a lip or off a pop, the whole charge is spent at once: a kick up and about a
third less gravity for the length of that jump, which is a noticeably longer,
floatier arc. Landing clears it.

Speed tokens lie along trails across the world, and only show while the bike
is out (`src/ride/boost.rs`). About half of the 384 m cells carry a trail: a
chain of eight to fourteen rings 26 m apart along a wandering line, whose
places come from the world seed, so a trail is where it was last ride. Each
ring waits for the ground under it to stream in before it stands up, and a
spot that turns out to be a cliff, under water or built over is dropped, so
what is left is somewhere a bike can get to. Riding through one fills the
sprint meter, adds about 3 m/s, and the ring comes back a minute later.

Going fast enough (upwards of 13 m/s, and more so sprinting or just after a
token), thin streaks run outward at the rim of the view (post.wgsl, driven by
`Camera::streaks`), on top of the radial blur. They
lighten a dark view and darken a bright one, relative to the pixel under them
and blended between the two, so they show against sky as well as ground
without laying a navy smudge over bright sunset cloud at the rim.

Gliding is the one way to fly. C on the ground springs you about 3.7 m up:
the legs push for a third of a second rather than firing you off in one
frame, and the wings open near the top and swing the climb round into forward
flight over half a second, at just over stall speed;
from the bike, you land back on it. For real height, launch off a cliff or
ride a thermal. C or Space in the air opens the wings and C folds them. The mouse aims the
nose and A and D roll the body: the lift tilts with the roll and the turn
comes out of the bank, at g tan(roll) across the path, so the same bank turns
more slowly the faster you are going and a turn has to be rolled into rather
than snapped. Swinging the aim across with the mouse banks the body as well,
about half as far as a key holds it, so an aimed turn leans into itself.
The roll has mass. The keys push it over against a weak pull
back to level, so it takes about 0.8 s to reach full bank, and letting go
holds the bank for a beat, comes back through level after about a second,
swings 8 degrees past and settles over another second and a half. Rolling the
other way is far quicker than letting go (about a third of a second from full
bank to level), so flying it is responsive while hands off is lazy. Hard over, the wings have little left for pulling up, so a hard
bank and a hard climb are a choice. A and D together throw a barrel roll.
Hold Alt to look around while the aim holds. The flight model is an energy glider: lift (which
grows with the square of airspeed, up to about 5.5 g) cancels gravity's pull
across the path and turns the path toward the aim, while gravity along the
path and drag (parasitic plus induced) set the speed. The best glide is about
7:1 at 47 km/h; aimed lower it goes faster and steeper (about 5.5:1 at
65 km/h, 130+ km/h in a dive, 175 km/h at the very most), and aimed level it slows, stalls near 34 km/h
and drops its nose. Diving and pulling up zooms you back up. W tucks (fast,
steep), S flares (slow, floaty, tighter turns). Shift is a short boost (about
1.4 s, refilling in 4.4 s, a tenth slower than the bike's).

In first person you see only the wing sleeves, so the shadow is cast by the
whole chase-camera flyer drawn into the shadow map alone (`Ride::stand_in`,
`Instance::shift[3]`); the sleeves themselves cast nothing. The bike and the
kayak do the same, so the rider's shadow is there too.

The air rises in thermals: round columns 35-65 m across at 3.5-6.5 m/s,
scattered over about half of each 90 m square of land, from a few metres up
to about 200 m above the ground. Circle slowly in one (S held) to climb. Wind
blows toward the east, and along a slope that faces it there is ridge lift.
The HUD shows speed, height above the ground, climb or sink rate, and THERMAL
or RIDGE LIFT. Close to the ground the wings work less hard (ground effect).

What you collide with while flying is a prone body round your eye, about
0.44 m across and 0.7 m deep, not the standing box you walk around in, so
what clips is about what the viewport shows. Leaves are flown through, at the
cost of a little speed. A contact no more than about 25 degrees into the
surface, at over 8 m/s and without S held, is a graze: the speed into the
surface is gone, what is left slides along it, and the flight carries on.
A graze costs 30% of the remaining speed per second against a wall and much
more against a floor, so a wall can be ridden but a flat belly slide drags to
a halt. Anything squarer, slower, or with S held ends the flight: touching
the ground lands you, flying slowly into a cliff grabs it (a cliff, not a
step: there has to be something to hold at the chest and over the head), and
anything faster knocks you out of the air.

## Scoring a glide

`src/ride/soar.rs` scores flying close to things. A run opens when the wings open and banks on touchdown. Twelve
rays round the flyer find the nearest surface each step; distance flown earns
in proportion to how close that is (full value inside 1 m, nothing past 12 m),
so cruising the open sky on boosts earns nothing however long it lasts. A
multiplier charges to 8x over about six seconds of flying within 8 m, bleeds
back out in the open with a two-second half-life, and resets after three
seconds with nothing within 25 m. Crashing halves it.

Most of a score comes from the lump sums, which pay at the multiplier they
were earned at and then bump it: CLOSE (anything inside 2.5 m), ROLL (a barrel
roll), GRAZE, THREAD (walls within 4 m on both sides), SKIM (a second under
1.5 m of ground above 70 km/h), UNDER (surfaces within 3 m above and below: an
arch, a tunnel, an overhang) and NEEDLE (walls within 2 m on both sides, worth
4,000 before the multiplier). Each has its own cooldown, and firing one holds
every lesser one off for a second and a half, so one tight gap pays once
rather than three times. The HUD carries the running score and the multiplier
over the gliding line, with the multiplier as a second bar under the boost
meter and the call-out across the middle of the view. The best run in each
world is kept in that world's `progress.json`.

To climb, walk into rock (or a trunk) at least head-high. Once your feet have
stopped against it for a quarter of a second you take hold (rock that flares
out at its base, like a hoodoo's, counts); in the air or gliding slowly you
take hold at once. W and S climb up (3.6 m/s) and down, A and D move across,
Space with W lunges up, Space alone pushes off (and Space again opens the
wings), and at the top you pull over the edge on your own. There is no
stamina. The climber follows round, bulging rock and leans out under bulges.

The rope (Z, or a mouse side button): the first press gets it out (while
climbing it is out already), and yellow corner marks then show where it would
catch, on rock or wood up to 48 m away. The marks sit on the crosshair and
only move off it when the aim assist has found a spot a few degrees away. Z
again throws the rope; Z with nothing in reach puts it away, and so does a
moment on the ground on foot after using it, or getting on the bike. On it
you swing as a pendulum, with a nudge at the bottom of each arc that keeps a
swing going. W climbs the rope fast enough to zip up a cliff (climbing into
the rock takes hold of it, and at the top you pull over the edge); S lets it
out to rappel; A and D swing you sideways; Space lets go with a hop; Z again
throws at the next anchor or lets go; C lets go and opens the wings. Leaves
hold no rope, and digging out the anchor drops you.

The kayak (K, `src/ride/kayak.rs`) floats on any water deep enough, the
sea, a river or a lake. K at the water's edge puts you in it on the nearest
water within 3 m that floats it; K again steps you out onto the nearest ground
no deeper than your knees, or over the side into deep water. Like the bike it
goes where you look (A and D turn the view, sweeping the boat round even when
it is still), and it glides on between strokes: W paddles in alternate strokes
at about 8.3 m/s (30 km/h), Shift sprints (about 11 m/s) and S back-paddles
and brakes. It carries its way: from a cruise it takes about 13 s to lose half
its speed. The current carries it. It runs aground where the bed comes within
18 cm of the surface: the HUD says AGROUND for as long as it sits on the bar,
and a note comes up when it first grounds and again every few seconds while
you keep paddling at it. It stops against posts, roots and banks. Your head has to clear whatever
you pass under: anything lower than about a metre over the water stops the
boat, so Lowmouth's boardwalks (1.5 m up) and footbridges (2.5 m in the
middle) are made to be paddled under. In first person you see the bow deck
and the paddle turning over stroke by stroke; the chase camera shows the
paddler.

## Driving

A world that lists `"vehicles"` in its `world.json` can be driven. Ribshore
has a sand rail and an older beach buggy (`library/vehicles/`), and Redrock
has the buggy for the desert floor; the launch
screen picks which one U brings, and its DRIVE switch takes driving out.

U gets in. The first time it sets the car down at your feet, facing the way
you look; after that it is wherever you left it, and U within 5 m of it gets
back in, while U further off brings it over. U again gets out on the
driver's side and leaves it parked; held for most of a second it puts the car
back on its wheels where it last had all four down. The parked car is saved
with the world.

W is the throttle, S brakes and then reverses, Ctrl is the handbrake (the
rear wheels lock and slide), Shift boosts while the wheels are down, and
Space held squats the car on its springs and letting go hops it. In the chase
camera the front wheels turn toward where the view points and A and D turn the
view, as on the bike. In the driver's seat the view is fixed to the car and
turns as it turns, reversing included; A and D steer and the mouse looks round
the cab. In the air W and S pitch the nose and A and D roll; four wheels down at
once off a jump of half a second or more is a clean landing, which gives back
a little speed and some boost. A car on its side or roof and nearly stopped
rights itself after a second. The chase camera is the default (V for the
driver's seat, hands on the wheel).

On Ribshore the wet beach below the tide line is the fast line, and the dune
ridges behind it are 40 to 54 m apart, so at 70 km/h each crest is a jump. The boost-token trails
(`ride/boost.rs`) show for the car as they do for the bike, and a car takes a
token anywhere across its width. Most animals bolt from an engine long before
it arrives; one that doesn't is pushed aside unharmed and the car loses some
speed for it, more for a big one.
Shrubs under 1.8 m (sagebrush, rabbitbrush, small cacti) are flattened as the car
reaches them and cost it a little speed. That isn't saved: the shrub is back
once its ground streams in again. Anything taller, a juniper or an ocotillo,
stops the car or turns it aside.

The surface matters: the wet beach is firm and fast, dune sand bogs a slow
car and planes a fast one, grass and mud have less grip. A material says
which it is with `"surface"` in its JSON (`hardpack`, `rock`, `sand_wet`,
`sand_soft`, `grass`, `mud`, `snow`), else it is guessed from the name
(`src/world/surface.rs`). Water slows the car; past the hubs the engine
stalls, and deeper than that it floats.

The tune is the JSON in `library/vehicles/`: mass, springs, grip, power,
steering lock and every assist. The game reads it again whenever a file
there changes, so you can edit it while driving. F3 shows each wheel's load,
slip and compression at the bottom left, and on the right a tuning panel
with every number in the tune: PageUp and PageDown pick a line, `[` and `]`
step it, `-` and `=` step it ten times as far, Backspace puts it back to the
file's value, and P writes the changed lines into the vehicle's file. Pause
(Esc) and the panel shows every line for the mouse: drag a line left or
right, roll the wheel over it to step it, right-click it for the file's
value, and click the bottom line to save. Edits
not yet saved are lost when any vehicle file changes on disk. To measure a
change without a window:

    lurelia --world ribshore --headless --ride drive --ride-secs 8 --chase --screenshot out.png

prints the position and speed every half second and how rough the ride was.
For numbers to tune against, `TUNE=beach_buggy cargo test --release
vehicle_report -- --ignored --nocapture` reads the vehicle's file and prints
its pace, how far one voxel-high bump pitches and rolls it, how it copes
with rough ground, and whether a handbrake drift holds or spins. It reads
the file at run time, so edit and run again without building.
`vehicle_laps` (same form) puts the bot from `src/vehicle/bot.rs` at the wheel
for three laps of a test circuit, part of it bumpy, and prints the lap times,
how closely it held the line, how far it fell short of the speed the bot
asked for, and any time off the road, spins, flips and resets. The bot's
style stays fixed, so a change in the numbers is the tune's doing. With
`DUMP=file` it also writes the track and the path driven as CSV.

`vehicle_tournament` races three bots in three cars for 20 rounds of three
races, each bot driving each car once a round from each grid slot, with the
cars bumping each other and the bots pulling out to pass. Odd rounds replace
the car with fewest points by a variation on the best one (two numbers moved
up to 15%, kept only if it passes a bump, rough-ground and drift check);
even rounds do the same for the bots' styles. `SEED=` and `ROUNDS=` vary it.
It prints each round's points and what changed, then the final tunes as JSON
lines to paste into a vehicle file and the final bot styles.
The physics is in `src/vehicle/` and the design in `notes/WHEELS_PLAN.md`.

## Kart racing

The Kestrel Ice Ring (`worlds/kestrel`) is a kart circuit in a canyon of ice,
and RACE is its mode on the launch screen (`--world kestrel` opens it that
way). You start at the back of a grid of four karts: three bots, blue, green
and yellow, and you in red. Three laps, 1.3 km round between ice walls, over
the shoulder of Beacon Hill and down through the hollow. W drives, S brakes, A and D steer. Space hops; held with the
steering over at speed it drifts, and every 0.8 s of drift charges a tier
of boost, up to two, that fires when you let go. The HUD shows your place,
the lap, the race clock and your speed. A kart that is stuck, over, or well
off the track for three seconds is put back on it. When you finish, the
results come in as the others do; Enter races again.

A new course is a `.course` text file: the land, its hills, and the
centreline as a list of points, in a few dozen lines. `tools/course/FORMAT.md`
describes it and `tools/course/course_file.py` builds the world from it;
Kestrel is `tools/course/maps/kestrel.course`.

The pieces underneath, for a race built some other way:

- `library/vehicles/kart.json` is the kart. The `drift_` keys turn the hop
  into the drift; any vehicle can have them.
- The `circuit` chronicle event lays a paved loop with kerbs through a
  `path`, graded smooth, banked a little on its turns. `rise` lifts the road
  at each path point (up to about 3 m above the ground, or it leaves a gap),
  and `wall` stands walls that high along both sides. It keeps its centreline
  and heights for the race.
- world.json `race` names the circuit, the laps, how many rivals (1 to 3) and
  the vehicle. The world's `vehicles` should list that vehicle first, since
  the wheel model is sized from it.
- `src/race.rs` runs it. The rivals drive with `vehicle::bot` (`Style::kart`
  for karts, `Style::racer` otherwise), see crests and descents through the
  circuit's heights, pass a slower car ahead, and bump off each other and
  you. A rival out where the world isn't loaded is carried round the
  centreline until the ground is there.

To watch a whole race without a window, with a bot driving your kart:

    lurelia --world kestrel --headless --ride race --ride-secs 200 --chase --screenshot out.png

It prints the standings every second. To tune the kart against the bots on
the test circuit, made smooth: `BUMPS=0 TUNE=kart cargo test --release
vehicle_tournament -- --ignored --nocapture`.

## Under water

[docs/UNDERWATER.md](docs/UNDERWATER.md) goes deeper into how each part works; this is the
short version.

### Scuba

Ctrl while floating in water at least 1.5 m deep dives (`player/dive.rs`). Under water the body
is a swimmer lying flat, 0.7 m tall, so it fits gaps a standing body would not. W/S swim along the
view with pitch, A/D sideways, Space up, Ctrl down; letting go hangs still with the current. Shift
kicks; a kick 0.45-1.3 s after the last one chains, up to three, each stronger, and one sooner is
ignored. A breath is 60 s and refills the moment the head is out; with none left the body rises
on its own, and nobody drowns. Breaking the surface faster than 2.6 m/s leaps.

Hold E near a manta, turtle or whale (anything with a model at least 0.32 m half-width or 0.9 m
tall) to hold on: `Game::hitch_step` carries the body at the place it took hold until E is let
go, a kick, or the animal would drag it into rock. The kayak is one object: K in it over deep
water rolls you over the side and leaves it moored (`Ride::moored`, drawn by
`moored_instance`); K within 2.6 m of it, afloat, climbs back in; K at any shore launches it
there instead. The HUD shows the breath bar and the kayak's distance and bearing.

### The look

Everything under water reads the `under` uniform (x = the surface over the camera, y =
visibility, z = sunlight falloff, w = 1 in inland water). `water_top()` is that surface in a lake
or cenote and the sea level otherwise, so light under a lake above the sea is right while you
are in it. The fog colour comes from the world's `water` palette by camera depth
(`underwater_color`), red is taken out first with distance (`water_tint`), caustics are two
ridged noise layers projected along the sun (`caustics`), shafts are marched along the view
(`water_shafts`), the surface from below has Snell's window (`fs_water`), motes become marine
snow, and wet sprites sway on a surge instead of the wind. Chunks further than five
visibilities are not drawn while the camera is under. A world sets
`"underwater": {"visibility": m, "light": per_m}` in world.json (default 14 and 0.16; Jadewater
30 and 0.07, Lowmouth 4 and 0.35).

### Reefs

Three chronicle events (`chronicle/events/coral_reef.rs`, `atoll.rs`, `cenote.rs`):

- `coral_reef`: `path` is the crest; the lower side is the sea. A flat behind the crest, a fore
  reef of spurs and grooves, a wall, an apron back to the old bed; `fore` holds the seaward
  numbers and `undercut` the share of the wall cut into caves (capsules, `fields::Tube`).
- `atoll`: a ring round a lagoon with motu, passes, patch reefs and an optional blue hole, the
  same fore reef outside.
- `cenote`: a karst hill with a flooded shaft in it whose water is inland water at its own
  level, and a bell carved below. Its bed stays a metre above the sea, because the sea is one
  plane drawn everywhere and would show inside anything below it.

They paint regions the reef biomes read: `reef_flat`, `reef_crest`, `fore_reef`, `reef_groove`,
`reef_wall`, `reef_apron`, `lagoon`, `patch_reef`, `blue_hole`, `motu`, `cenote_shaft`
(-> `cenote_reef`). `glass_reef` is `fore_reef` where `strange` is 0.25 or more.

The voxel floor is y = 0, so the sea is at most `sea_level_m` deep: 24 m on Glassfall, 12 on
Jadewater. Glassfall's wall runs to 22.5 m and the fog does the rest.

### Coral

Plant forms `coral_branch`, `coral_antler`, `coral_table`, `coral_mound`, `coral_plate`,
`coral_finger`, `sea_fan` and `sponge` (`flora::coral`), grown from limbs and blobs like trees.
They take `water_depth`, measured from a lake's or cenote's own surface where there is one, and
keep their own height (kelp alone scales with the water); a coral too tall for the water over it
is shrunk to stand 0.35 m under the surface, and skipped where the water is under three
quarters of its smallest height. Coral limbs are bevelled tubes; the forms are `flora::Form` variants and the test is `form.is_coral()`. The mesher keeps corners within half a metre of
the ground square, so a colony's foot is blocky and only what stands higher rounds: keep
species tall enough to show it. `wall_plate_coral` is a `crust` with `water_depth`, which shelves
under-water faces of reef walls and cenotes.

### Fish

`school` swimmers take a `habit` (`creatures/swim.rs`): `open` (a school in open water), `cloud` (a tight
cloud over one coral head that drops into it when startled), `hover` (one or two near a home,
pausing and turning to look at a diver), `patrol` (long passes in a depth band, minding nobody),
`rest` (rays on the sand), `bait` (a ball that turns on itself and opens round the diver), `pulse`
(jellyfish), `burrow` (garden eels that sink into the sand within 3.5 m of you) and `cruise` (a
whale shark). `above_bed` or `depth` give each species its band. Every rigged creature is one
instance per part, and `flock::steer` is quadratic in school size, so a bait ball is about 150
fish.

### Currents

The sea has currents the way the air has thermals (`player/dive.rs sea_current`): scattered by
a hash over 140 m cells, a little under half of them hold one, a meandering ribbon 4-8 m across
and 90-160 m long, 2.5-7.5 m down, running 3-5.5 m/s, only where there is 5 m of water. A diver
in one is carried along (it adds to the swim's target velocity), fish ride it at 85%, pale
`current_streak` particles flow along it near the diver, and the HUD says CURRENT. The movement
tests turn them off (`Dive::currents`), or the test sea's stream carries the diver away.

### Hunters

A species with `hunts` (barracuda, jack, reef shark, grouper, cod) rests 12-30 s, then picks the
nearest school of something it eats within 35 m and rushes it for 6-11 s at its flee speed
(`School::chase`). Prey within 7 m of a hunter frights as it does from the diver (a cloud drops
into its coral), each prey fish within 4 m darts away from it (a bait ball splits round the
jack), and a rushing hunter that comes within half a metre of a fish takes it. The swim stats
count `chases` and `caught`.

### Wrecks

`base: "bed"` puts a structure on the lowest seabed under its footprint, half a metre into the
sand (`chronicle/ctx.rs place`). Anything placed with an `age` that sits under the water crusts
over in the decay pass (`gen.rs draw_structures`): reef limestone in broad patches, and one coral
colour for the whole wreck on the thickest of it, from whichever of `reef_lime`, `coral_rose`,
`coral_violet`, `coral_gold` and `coral_white` the world has. `sunken_hull`, `sunken_ribs` and
`jar_spill` are the entries; Glassfall has the surveyor's launch on its reef and a ship below its
wall, Jadewater a junk and its jars below the atoll.

### The surface

`surface_froth` in scene.wgsl draws foam lines on open water, drawn out downwind and broken into
flecks, more in wind (`g.weather.w`); from below they show as pale patches, and the mirror past
Snell's window is banded by the waves. `wave_normal` has a broad swell layer under the ripples,
so far water keeps some texture, and the calm style takes the ripple slope down to 0.7, not 0.5.

### Testing it headless

`--pos` pins the camera; `--ground` does not, and over a long `--frames` the body floats up to
the surface. Probe the bed height first (the stats JSON `player.pos` after `--ground`), or a
`--pos` guess ends up inside the reef rock. `VG_SPAWN=reef_chromis,anthias VG_SPAWN_DIST=5`
drops fish beside the camera. `--catalog DIR --only creatures --match shark,jelly` frames the
swimmers; the plants catalogue frames corals badly, so judge those in place.

## The camera

F takes the camera out; the crosshair becomes a small box at the centre of
the screen. The world keeps running. A click raises the viewfinder:
everything outside the frame greys and blurs, and a right click lowers it
again. Tab turns the frame between landscape (3:2), portrait (2:3) and
square, with the camera up or down. The wheel zooms (24 to 200 mm) and Shift
with the wheel opens or closes the aperture (f/1.8 to f/16). Focus is
automatic: it racks to whatever is under the centre box (the sky counts as
far away) and settles in a fraction of a second. Ctrl with the wheel, or
holding Q or E, focuses by hand, nearer or further, and the readout switches
from AF to MF; a middle click, or raising the viewfinder again, goes back to
autofocus. The frame shows the focus distance and the distance to whatever is
under the centre box, which turns green when that is sharp. Blur is the thin-lens circle of confusion on a
24 mm tall sensor, exaggerated four times so depth of field shows at game
distances; the post shader draws it and the photo checks use the same
numbers. A click takes the photo; a right click or Esc lowers the viewfinder.

A roll holds 12 photos. Every three finds (gems, shards, any collectible)
earn a spare roll, and R loads one once the roll in the camera is finished.
A new game starts with a full roll and one spare.

Each photo is measured when the shutter fires (`camera/scene.rs`): every
creature and tree near the frame, where it sits in the frame, how tall it is
there, its distance, the angle up or down to it, whether ground, rock or
leaves hide it, whether it is sharp, what is behind it, and what the animal
was doing (eating, drinking, resting, moving, standing, flying, perched),
plus whether any water shows. The logbook holds five tasks, each a subject
(usually one animal, sometimes a tree, now and then just water) and up to
two more needs: the frame's shape, the time of day, the angle, what the
animal is doing, where it sits (on a thirds point, centred, filling the
frame, small in a wide view), sky behind it, a soft background, or how many
show. The tasks come from a seeded generator over the creatures this world's
biomes hold and the trees near the spawn, so the queue is the same for
everyone; a done task is replaced by the next. A photo that does a task goes
into the scrapbook with a ribbon, the task and its reward. Any other photo
says what the nearest task lacked, and Enter keeps it as a snapshot; Esc or
the next photo lets it go.

The scrapbook keeps its images in `saves/<world>/scrapbook/` and its list in
`saves/<world>/scrapbook.json`; film, tasks and rewards are in
`progress.json`. Rewards add up but don't unlock anything.

## Screenshot mode

F2 pauses the game (creatures, the rider and the clock all stop) and frees a
camera from wherever the view was, with the rider model drawn even if you were
in first person. Everything paused is drawn at the blend between steps it
paused at (`Game::still_alpha`), so nothing caught mid-stride shakes. WASD,
Space and Ctrl move it (Shift faster, slower when
zoomed in), the mouse looks, the wheel zooms (shown as a full-frame focal
length, 17 mm at the normal view, up to about 115 mm), and Q and E tilt the
horizon in 5 degree steps. N, T and G still change the time of day, the style
and the shadows, so a screenshot can wait for the light. The camera stays
within 120 m of the player, because the world only streams in around them,
and it won't go into rock. A thirds grid and the keys show on screen but not
in the image.

Enter or a click saves the view as a PNG at the window's size in
`screenshots/` (next to `saves/`), named `<world>-0001.png` and up. F12 does
the same from normal play, without the HUD. `src/screenshot.rs` renders the
frame a second time into an offscreen texture with an empty HUD, reads it
back and writes the PNG on a background thread; the HUD shows the file name.

## Styles

There are two looks, switched with T or `--style saturated|classic`. Saturated
is the default on load; classic is the fallback.

- Classic: softly detailed textures with smooth filtering, slightly redder soils
  and wood and warm greys, in clear daylight (`render/texsets.rs`).
- Saturated (`--style calm` also opens it): near-flat textures, broad colour
  fields across the land (grass drifts
  between green and gold over tens of metres), a golden sun under a clear
  blue sky, deep slightly teal shadows with softer edges, thin warm haze that
  thickens near the sea, calmer water and a teal-and-gold split-tone grade.
  Steps of one or two voxels cast no shadow, so slopes read as smooth ground;
  trees, cliffs and creatures still do. It is aimed at a painted, poster-like
  landscape: a warm fog ramp by distance, haze warmer toward the sun, shadows
  tinted by the fog and a pushed golden hour (`VOXEL_GRADE=legacy` shows it
  without them).

Both read their colours from `library/looks/<style>.json`, and the LOOK page
below changes them while the game runs.

Both use the same geometry; switching re-uploads the block textures (18 small
layers) and changes the lighting constants and shader branches. Neither costs
measurably more than the other. [docs/ART_VISION.md](docs/ART_VISION.md)
describes the look the art should serve.

## The EDIT page

F8 stands one creature, plant or material in a studio in front of you -- a grey grid
platform, or the world's own ground with one key -- and puts its library entry
down the right of the screen. Stepping a field rebuilds what is standing
there: a colour repaints the model under the cursor, a speed or a wary
distance changes how it behaves on the spot, a plant's crown regrows.
REGENERATE rolls another individual, and AGE shows it as a juvenile, an adult,
an aged animal or its bones. A material stands on the platform as a block, and
its pattern, scale, contrast and colours are rebuilt into the world's textures
as you step them. Rows whose value is a name -- the entry itself, a movement
verb, a growth form, a texture pattern -- drop the whole list open on a click.

WRITE THE FILE puts the entry back in `library/<kind>/<name>.json`, changing
only the values that were edited and leaving the rest of the file's own layout
alone.

Headless, `--edit NAME` opens the page for a screenshot, `--edit NAME:3` on a
group of fields and `--edit NAME:3:world` with the platform off.
[docs/EDITOR.md](docs/EDITOR.md) covers the lot, including what it cannot
edit.

## The LOOK page

F7, or LOOK AND COLOUR in the pause menu, opens the look of the style you are
in. The page sits down the left so the world stays visible on the right, and
every change shows on the next frame. Up and down move, left and right change
a number, Enter opens a colour, Esc goes back. The mouse works too: a click
nudges a number up, a right click down.

Three of the pages are gradient strips, the 1D colour ramps the saturated
look is built on. Each is a list of stops; left and right move a stop along its
strip, Enter opens a picker (drag in the square for saturation and brightness,
on the bar for hue), and ADD A STOP puts one in the widest gap.

- **LIGHT** is the sky, the ground bounce, the ambient fill and the sun: the
  five colours the look file holds, plus sun gain, fog
  distance, the haze warmth toward the sun, the glow round it, and how much
  of the time of day's own warm and cool shift is used.
- **DISTANCE RAMP** is the strip by distance. It colours the haze, and can
  also colour the low sky and the shadowed light. Its stops read in metres,
  worked out from RAMP REACH and RAMP CURVE together. Three knobs decide
  whether ridges read as flat cards:
  - RAMP CURVE 1 is the eased shape the look was tuned on: the strip is spent
    on the first few hundred metres and everything past that crowds into its
    far end, so distant ridges all come out much the same colour. At 0 it is
    straight, so a stop sits at the distance it says and ridges a few hundred
    metres apart land on different parts of the strip. Raise RAMP REACH to
    suit the view (1000 to 1500 m for an open landscape).
  - HELD TO SKY VALUE decides how much of each stop's own brightness
    survives: at 1 only its hue is used and it follows the horizon, which is
    how the look was tuned; drop it toward 0 and the strip sets the value
    too, which is what separates one band from the next.
  - DISTANCE SLABS cuts the view into that many slabs of distance, each
    taking one colour and one thickness of haze across the whole of it. This
    is the step that actually makes cards: a stepped ramp colour under a
    smoothly thickening haze still blends away, because it is the *amount* of
    haze that has to step as well. Five or six slabs over a 1200 m reach is a
    good place to start. Off by default.

  Rows of flat colour are mostly a matter of what is in front of
  the camera: ridgeline behind ridgeline with sky between them. A forest or a
  plateau seen from above has nothing to band, whatever these knobs are set
  to.
- **TINT** is the strip by luminance, multiplied in, so each pixel keeps its
  own hue. This is the teal-shadow gold-highlight split tone, as stops.
- **GRADIENT MAP** is the same idea pushed all the way: the strip's hue
  replaces the image's at each pixel's own brightness. At 0 it does nothing;
  raise it and the picture flattens onto the strip's colours, which is the
  poster-flat grade. Only the stops' hue matters here, so they can
  be bold.
- **GRADE** is exposure, saturation, contrast and the vignette.

Out over the world, beside the list, a small box follows the cursor with the
range to whatever it is over and the colour the ramp gives there: point at a
ridge, read the metres, drag a stop to them. Opening a ramp stop adds a
distance bar to the picker with that range ticked on it.

[docs/LOOK.md](docs/LOOK.md) goes through every value on every page, including
what the distance ramp and the gradient map each do and why they are not the
same thing.

EXPORT TO THE LOOK FILE writes everything on these pages back to
`library/looks/<style>.json`, keeping its notes, so a session of tweaking
becomes the new default. READ THE LOOK FILE AGAIN throws the edits away.
Loading a world also reads the file again, so export before switching worlds.

### A world's own grading

A style is a way of drawing every world; a world may still want a colour of
its own, the way one film is graded against another. `worlds/<name>/look.json`
holds that: a patch over whichever style the world is seen in, with every
field optional, so a world can take the distance ramp and the haze and leave
the rest of the style alone.

```json
{
  "notes": "Dust and red rock: bleached gold near, salmon through the middle
            distance, the far cliffs going dusty violet.",
  "ramp": [{ "at": 0.0, "color": [1.0, 0.86, 0.58] },
           { "at": 1.0, "color": [0.62, 0.56, 0.86] }],
  "knobs": { "ramp_reach": 900.0, "haze_height": 220.0, "saturation": 1.06 }
}
```

The fields are the look file's own: `zenith`, `horizon`, `ground`, `ambient`,
`sun_tint`, `sun_gain`, `fog_distance`, the three strips `ramp`, `tint` and
`map`, and `knobs`, which names any grade knob by the name it has in
`render/grade.rs` (`ramp_reach`, `ramp_curve`, `ramp_bands`, `ramp_value`,
`haze_ramp`, `haze_density`, `haze_height`, `sky_ramp`, `sky_reach`,
`shade_ramp`, `sun_side`, `sun_glow`, `crown_shade`, `exposure`, `tone`,
`map_amount`, `saturation`, `contrast`, `vignette`, `time_tint`). A strip
given at all replaces the style's, since a ramp is a designed sequence rather
than something to average. `styles` holds a second patch for one style by
name, applied over the shared one, for a world that wants to be graded one
way in SATURATED and another in CLASSIC. A knob nothing is called, or a style
that is not a style, is reported by `--validate`.

On the LOOK page, EXPORT TO THIS WORLD writes what differs from the style
file into that world's `look.json`, and EXPORT TO THIS WORLD, THIS STYLE puts
it under the style's name alone. Grading a world back to the plain style
takes the file away again. EXPORT TO THE LOOK FILE then leaves whatever the
world owns as the file has it (on screen those values belong to the world,
not the style) and says how many it left.
`VOXEL_GRADE=legacy` turns the ramp studies off for a comparison.

## Time of day and light

The sun moves. A whole day takes 25 real minutes, but the clock's speed
depends on the sun's elevation (`RATES` in `world/clock.rs`, blended over a
degree or two at each band's edge) so each kind of light gets a set share:
night (sun below -12 degrees) 15%, deep twilight 7%, blue hour 3%, sunrise
and sunset 6.7%, golden hour 15.1%, warm early and late light 16.3%, the
tuned afternoon look 17% and midday 19.9%. The night runs at about 2.7 game
hours per minute, sunset and golden hour at about 0.35. A new game starts at 15:00, the afternoon light both
styles were tuned in. N steps through dawn, morning, midday, afternoon, golden
hour, sunset, dusk, twilight and night; `--hour H` sets the time and stops the clock, for
screenshots (it holds the weather too, which runs on the same clock). The sun rises in the east, crosses the south at
noon and sets in the west.

How high it climbs depends on the world and the season (`SunPath` in
`render/daylight.rs`). Each world has a latitude (world.json `sun`, 42 if left
out) and the season sets the sun's declination: +14 in summer, +4 in spring,
-4 in autumn, -14 in winter, sliding with the rest of the season's numbers
through a turn. At the default latitude the summer sun is 62 degrees up at
noon, the height the styles were tuned under, and the winter sun 34. The
shares above belong to a reference day (the equinox at latitude 28, sunrise
at 6, sunset at 18), and the clock maps every world's day onto it
(`SunPath::solar`/`unsolar`): a short winter day runs its hours faster and
its long night slower, so sunrise, day, sunset and night take the same real
minutes in every season and world. The N presets, the launch screen's hours
and first and last light (`clock::dawn`, `clock::dusk`) follow the same map.
A world can also lift the sun the land is lit from (`lift_deg`, 3.44 by
default) and soften its shadows (`shadow_soft`): Kaldvik's sun stands 12
degrees up at noon, but it lights the snow and throws shadows as if it were
20, and its sky, disk and colours stay those of the true low sun.

The northern lights (`render/aurora.rs`, `aurora` in `scene.wgsl`) show in a
world with world.json `aurora` above 0. Curtains are vertical sheets folded
by slow noise, solved exactly where each sky ray meets them (no marching):
green low with a sharp lower edge and a violet hem, red and magenta at the
top, fine rays shimmering along them. Each night draws its own strength, a
fifth of nights quiet and a few strong enough to arc overhead, building to a
substorm around midnight. They tint the sky's fill a faint green and show in
the water. `VG_AURORA=0..1` holds tonight's share for screenshots.

`src/render/daylight.rs` moves the light away from the style's own look by
sun elevation: blue-hour skies and a dim cool fill with the sun down, a deep
orange sun and a warm band on the horizon at sunset, and a whiter sun, deeper
blue and more fill at midday. Golden hour has a saturated gold sun, warm haze
toward the sun, warm fill in the shade, sun rays that stay on while the sun is
low, and a warmer grade. Once the sun is three degrees down the moon takes
over as the key light. It moves along the sun's path at a steady 1 game hour
per real minute whatever the clock is doing, on the sun's path with the
declination turned over (so a low winter sun means a high moon), centred on
the south halfway through the night, so it is high in the east-south-east at dusk
and fades out high in the west-south-west as the sun comes back: a cool white light that casts shadows,
a small disk, stars, and a grade that drains colour toward blue-violet. In
the night proper (sun 18 degrees down) the sky, fill and grade turn a paler
blue, about a quarter darker than the deepest twilight, the moonlight dims by
half since the moon stands high, and there is more mist. Leaves (foliage voxels, leaf billboards and big plants'
cards) take only part of the shadow on them while the sun is low, falling to
half by the time it is about 3 degrees up (`leaf_shadow` in `common.wgsl`), so
a wood at golden hour keeps its lit canopy instead of going flat and dark. The
sun's afterglow stays on the horizon on the sunset
side. Shadows are cast from at least 3.5 degrees up, so they never smear
across the land at sunset.

Aerial haze thickens with distance and picks up the sun's glow when you look
toward it, and the far distance (90 to 380 m) is lightly blurred in
`post.wgsl`. Biomes can have mist: `"mist": 0..1` in a biome's JSON (coasts,
floodplains, lake shores, conifer slopes, canyon floors). The generator turns
biome mist into a map over the world (`Generator::mist_grid`), which spills a
misty coast's fog out over the sea; the shader integrates a bank about 7 m
deep over the smoothed ground along each ray. Mist is thicker at dawn than at
dusk, and at both a thin mist lies everywhere.

Rivers and lakes are water surfaces at their own levels, one quad per metre
cell (`world/rivers.rs`, `vs_river` in scene.wgsl, the same `fs_water` as the
sea). A river stands in pools joined by short rapids; ripples and patches of
foam drift downstream with its current, and it is clear enough to see trout
and the gravel two metres down. You float with your eyes just above the
surface (Ctrl dives) and the current carries you, about a metre a second on
the rapids; wading slows you. Deer, elk, bears, boar and horses swim across
(behaviour `swims`), backs at the waterline, carried downstream; other
walkers keep out of water deeper than about 0.3 m, waders out of more than
0.6 m. Ducks and geese drift with the current and paddle back. A lake is the
same water with no current.

Where a river's water drops 2.5 m or more over a cliff it is a waterfall
(`world/falls.rs`, found in `settle_water`): a sheet of falling water that
leaves the lip at the current's speed and keeps a hand's breadth off the rock,
streaked white by noise carried down at the water's own speed. Where it lands
the pool is churned white and pushes outward, a few dozen mist puffs roll out
and droplets are thrown up (`vs_spray`, no simulation: each piece is placed
from the time, out of one `Spray` record per fall that the pool's churn reads
too), and the mist map thickens round it. `--validate` lists every
fall. Hemlow's Lantern Spout is one.

Water glitters: small points of sun on the ripples, each lasting a fraction of
a second, make a glitter path toward a low sun (only on real water, not the
film over low shore land). Any material whose name contains GLASS has each
texel tilted its own way, so single texels flash as you move. Sun rays come
and go over a minute or two when the sun is on screen: `post.wgsl` marches
from each pixel toward the sun and gathers the bright sky near it, which
branches break into rays. Dust motes drift in a box round the camera and show
only where they are in sun with shade on three sides, which is inside a shaft
of light under a canopy.

There are three shadow cascades round the camera, 16, 50 and 160 m. Their
texel grids are pinned to the camera as the sun turns, so shadows slide
rather than flicker. Cascade 0 is drawn every frame. Cascades 1 and 2 have
three layers each: a slice of their chunks is drawn into one every frame (4
and 12 frames to a map) while the shader blends between the other two, so
their shadows move smoothly and no frame redraws a whole cascade.

A map is shown for three slice cycles after it is drawn, and it has to be
redrawn whole once the camera strays further from its centre than the margin
its radius allows (6 and 20 m). So the outer cascades are centred not on the
camera but on where it will be in the middle of that life -- one and a half
cycles ahead at its present speed -- which halves how far it strays and
doubles the speed a flight can hold before a whole redraw. Over the 30 s
`--bench` flight there are 54 and 7 whole redraws, against 181 and 59 with
the maps centred on the camera. Standing still the lead is nothing, so
shadows near a walking player are centred on it, and the lead is capped at the margin, so a map never stops
reaching back to its own split. Headless `--stats` counts whole redraws as
`cascade_redraws`, one number per outer cascade.

The maps hold only the faces turned away from the sun (entities included: their
shadow pass culls front faces), so a sunlit face is compared against the far side
of its own solid and needs almost no depth bias. `cascade_shadow` in scene.wgsl
gives each surface its own:

- Tight, a texel and a half plus a normal offset that grows as the sun grazes:
  voxel faces turned to the sun and entities. Contact shadows stay shut: the top
  of a post under a canopy, the foot of a wall under its eaves, an animal's head
  on its neck.
- One voxel (0.24 m along the light): rounded trees and faceted rock, which cast
  from shapes that lean off the faces drawn, tubes, faces turned away that a
  smoothed normal still lights, and sloped quads where the sun grazes them (it
  striped a dome along its shadow line).
- The old bias, 0.24 m and 0.54 m in the saturated style: leaves, which cast
  from 0.5 m leaf cells and streak at less, and everything not drawn as terrain
  or entities.

The old bias everywhere left a lit band that deep under every overhang. One
thing it did on purpose is gone: in the saturated style one-voxel ledges on a
steep wall that slopes don't smooth now shade the face below them. The shadow
pass's own raster bias (`sbias`) stays: without it shadow edges move a few
pixels and leaf crowns speckle. `VOXEL_SHADOW_BIAS=legacy` puts the old bias
and two-sided entity casting back, for comparisons.

### Light past the cascades

The cascades stop at 160 m. With nothing past that, a far hillside would show
the same value on the side facing the sun and the side away from it and the
whole distance would read as flat paper. Three things fill that in. None of them costs any geometry, and none follows the SHADOWS setting:
turning the maps off costs you contact shadows, not the light in the
distance.

**Cloud shadows.** The sky's clouds are a field on a plane 900 m up,
anchored in the world rather than to the camera, so they have parallax and
keep their places as you fly. `cloud_shadow` reads the same field where the
sun's ray crosses that plane, so the shadow crossing a hillside belongs to a
cloud you can look up and see. The one deliberate difference is
`CLOUD_SHADOW_FREQ`: at the sky deck's own scale a cloud is half a kilometre
across, so a world a kilometre and a half wide lies under one of them and the
shadow reads as a flat dimming. The shadow field runs finer, as a lower broken
deck would, and breaks the land into patches you can watch travel.

**The land's own skyline.** `src/render/horizon.rs` bakes, for every cell of a
256-square grid over the world, the elevation of the skyline in each of 16
compass directions. A point is in shadow when the sun stands lower than the
skyline in the sun's own direction: one texture read, at any distance, at any
hour, out to the world's edge, and cheaper per frame than a fourth cascade
would be. It fades in as the cascades fade out, so the two never disagree, and
it fades out above the ground, since a glider well clear of the hills is in
the open whatever the valley below is doing. The bake takes about 70 ms when a
world loads.

**Sky occlusion.** The same scan gives how much sky each cell can see, which
is the large-scale ambient occlusion the per-voxel one cannot reach: hollows
and valley floors sit in less light than the ridges above them. Unlike the
shadow it holds at every distance, because a valley floor is darker whether
you stand in it or look across at it.

`VOXEL_CLOUD_SHADOW=0`, `VOXEL_HORIZON=0` and `VOXEL_SKY_OCCLUSION=0` turn
them off one at a time for comparisons.

## Tides

A world with `"tide": {"range": 2.5}` in world.json has a sea that goes out
and comes back on the game clock (`src/world/tide.rs`). `sea_level_m` is
high water: the beaches, shore plants, jetties and buildings were all placed
against it, so the tide only goes out from there and never floods dry land.
Low water is `range` metres below. One tide, high water to high water, takes
`period_h` game hours (default 18, three quarters of a day), so the sea is out
at its lowest nine hours after each high. The first high water is at `high`
on the first day (default 6). The tide runs on the day count and the hour
together, so each day's tides come six hours later than the last's, and a
given `--hour` on the first day always shows the same tide (06:00 high, 15:00
low, 24:00 high again). Leaving the block out is a still sea, which is right for a lake, an arena or
a sheltered sea.

Only the open sea moves: every cell below sea level joined to the edge of the
map through cells below it (`tide::ocean_cells`). A hollow below sea level the
sea cannot reach keeps its water at high water, drawn by a second plane over
the cells the heightmap's g channel marks (`Fields::basin_grid`, `vs_water`).
A world whose sea reaches no edge gets no tide, and `--validate` says so.

Rivers that run to the sea fall with it near their mouths. Each river cell
has an `ebb` (`Wet::ebb`): 0 for lakes, rivers that end in a lake and water
above a waterfall; for the rest, the channel's depth less 10 cm, so a river
thins over its bed at low water rather than running dry. Water at the sea's
own level falls the whole way; the fall shrinks linearly to nothing a full
range above the sea, so the river never runs uphill (`Tide::river_fall`,
`river_fall` in common.wgsl). With a tide, every river that reaches the
sea runs on to low water: its line is carried past its end, leaning downhill
across the flats (`extend_to_low` in river.rs), its channel cut a little into
them, and the water there stands at the sea's level and drains to 10 cm over
the bed as the tide goes out, so the river follows the sea down instead of
stopping at the old mouth. Those cells draw nothing while the sea covers
them (`vs_river`). Without a tide nothing past the mouth changes, and flora
ignores water at the sea's level, so worlds without one build exactly as
before. Gameplay reads the same numbers through
`Fields::wet_now` and `Fields::sea_now`; `wet` is still the high-water
level, which is what the generator builds against. Falls into the sea
reach down to it, and ground the tide has uncovered stays dark and wet up to
the high-water line (`tide_wet`). No land plant grows below high water
(`drowned` in foliage.wgsl), and seagrass the tide leaves out lies flat on
the sand, and where the water is shallower than a blade is long the rest of
it floats along under the surface. Kelp is loose strands drawn in the shader,
shaped to the sea as it stands now, so a falling tide lays more of each
strand along the surface instead of lifting it into the air (docs/UNDERWATER.md
"Kelp"). Seabirds afloat on the sea ride its level now (`Terrain::sea_at`).

The sea plane moves in 6.25 cm steps, midway between the steps the smoothed
ground is built in, so it never lies flush with a flat beach and flickers.

## Surf

A world with `"surf": {"height": 1.2}` in world.json has waves breaking on
its shores (`src/world/surf.rs`). `height` is the breaker height on a fair
day in metres; the wind scales it, and some waves come bigger than others in
sets. `length_m` (16) is the distance between crests and `period_s` (7) the
seconds between waves. Leaving it out is a sea that only laps, which suits a
lagoon world, a fjord or a bayou. Ribshore has 1.2, Glassfall 0.9, Hemlow
0.7, Jadewater 0.35 and Kaldvik 0.25.

It is drawn in the shaders, nothing moves on the CPU. At load,
`surf::shore_grid` bakes two fields into the heightmap's b and a channels:
how far each point lies seaward of the high-water coast, smoothed over about
10 m so crests bend round the coast rather than copying every notch, and how
open the sea is there, from a wide blur of the same distance, so a lagoon or
a cove behind a spit stays calm. Crests are lines of equal seaward distance
moving in toward the shore (`surf_at` in scene.wgsl). Out at sea they show
only as bands of slope in the water's normal, which steepen as the water
shoals; where the water is about as shallow as the wave is high they break
(`fs_water`): solid white on the face and crest, lace behind it, a little
churn over the inner zone. On the beach, `swash` runs a sheet of water up
the sand after each wave and slides it back, with froth along its front, and
the sand it reaches stays dark. It follows the tide, since all of it is
measured from the sea's level now. The crests' wobble is baked into the
grid too, so the CPU knows the waves exactly: the surf sound's middle train
keeps time with the wave breaking nearest the ear (`Shore::breaker`,
`Surf::run` in sound/voice.rs), its foam coming up as that wave's face goes
white, louder for a set's big ones.

### Swell

The open sea's surface itself rises and falls: three long waves (40 m by
default, `swell_length_m`, with two shorter ones at angles) rolling in from
the side the open coast faces (`swell_heading`, the average seaward slope of
the shore grid). `swell` in the surf block is crest to trough on a fair day,
half the breaker height if left out, 0 for a flat sea; the wind scales it.
It dies away in water under four metres deep, where the surf takes over, and
in sheltered water. With a swell the sea is drawn as a 192 x 192 mesh round
the camera, a metre apart beside it and coarser toward the horizon
(`vs_water`, `SEA_GRID`); a wave shorter than four mesh steps is left out of
the mesh rather than aliased, and the lighting carries every wave at every
distance. `swell` in scene.wgsl and `surf::swell` in Rust are the same
function, fed the same clock and wind (`Game::follow_swell`), so the player
swimming, the kayak, bots, seabirds, fish and whales ride the waves on
screen (`Fields::sea_at`, `game::water_over`, `Terrain::sea_at`). The kayak
also pitches and rolls with the water's slope under it (`Ride::water_slope`).
The high-water plane over a sealed hollow writes its depth, so the
reflections in post.wgsl take it as a pool at its own surface. On an RTX 3060 at 1600 x 900 the
water pass costs 0.31 ms with a still sea, 0.38 with surf and 0.58 with surf
and swell.

## Weather

One kind of day at a time, for the whole world (`src/world/weather.rs`). Seven
kinds: **sun**, **heat**, **clouds**, **rain**, **snow**, **fog** and the dry
**storm**, which only a world that names it in its climate ever sees. Sun is
the fair afternoon everything else in the engine was tuned under, so its row
of numbers is all ones and zeroes and a sunny world draws exactly as the
styles were tuned; the other five are stated as how far they lean away from it.

A kind is nothing but a row of numbers (`Sky`): how much cover to add to the
cloud deck, how flat the light is, multipliers on the haze and the wind, mist
lying everywhere, how much rain or snow is falling, how hot it is, and shifts
to the grade's warmth, saturation and exposure. Two kinds crossfade into one
row over about half a game hour, so every value is continuous and nothing
snaps. What falls is the weather; what it leaves behind -- wet ground,
puddles, frost, fog in the valleys -- is the next section's.

**What each one looks like.** Heat scours the sky clear, thickens the air
pale, pushes the colour and stops the wind dead. Clouds spread the deck to
about half cover and flatten the light. Rain closes the sky, darkens and
greys the frame, doubles the haze and blows hard. Snow is nearly as closed in
but bright rather than dark, and still enough for the flakes to fall almost
straight. Fog leaves the sky mostly alone and does its work with the haze and
the mist, which is the morning it stands for.

**Overcast** is the part that does most of the look. The key light dims by up
to 55% and its colour is drawn toward the flat grey-white of light that has
been through a cloud; the sky fill comes up to take its place, because a cloud
deck is a bright thing however dark the ground under it reads; the horizon and
the zenith come together toward one pale tone; and the warm pool of scattered
light round the sun goes out. All of it runs over whatever the hour already
decided (`weather` in `render/daylight.rs`, after `apply`), so a wet dusk is
still a dusk.

**The cloud deck** is one field (`cloud_cover` in `scene.wgsl`). The weather moves both of its thresholds down that one field
rather than adding a second deck, so the shapes you could see at noon are
still the ones overhead when it closes in; negative cover (the heat) scours
them away. A thicker deck throws a deeper shadow on the land.

**The wind** is one direction for the whole world (`world::WIND`), and its
strength is a multiplier the weather sets: 0 in the heat, 1 on a fair day,
about 1.5 in a squall. It scales the gusts and the steady lean together
(`wind_scale` in `foliage.wgsl`), so a still day is still and a gusty one
bends everything further rather than merely faster. The glider's ridge lift
does not follow it.

**Rain and snow** are drawn in the post pass (`precipitation` in
`post.wgsl`), in front of everything: four layers of falling marks, each
standing for a slab of air at its own distance, so the near ones are long,
fast and sparse and the far ones a fine drift. Rain leans downwind as a thin
pale line lit by the horizon colour; snow wanders down as a soft white dot.
The field is offset by where the camera is looking, so turning sweeps the fall
past rather than carrying it along with you, and it is held off anything
within about two metres, since streaks over your own hands give the whole
thing away. It is weather you look through rather than geometry: one pass over
the pixels, and it reaches as far as the view does.

**Animals.** Each species has a taste for the weather (`Taste` in
`weather.rs`): the share of it still abroad in a downpour, in falling snow, in
the full heat, and in a gale. Each follows from the way it moves unless the
library entry says otherwise -- birds and insects keep out of rain and wind
(insects almost entirely), walkers and snakes mind it less, fish and whales
not at all, and mammals shelter from the heat while a reptile is out in it.
The spawner multiplies that by what the clock already allowed, so a swift in
the rain at midnight is as scarce as both make it; what sleeps where it stands
thins no further than the share the hour leaves lying about. And weather beds
an animal down the way the wrong hour does, so a downpour puts grazers under
the trees in the middle of the afternoon. A library creature sets any of
`rain`, `snow`, `heat` and `wind`, 0 (gone) to 1 (doesn't notice it).

**What each world gets.** Every world states its climate, read off its own
story rather than its name:

| World | Its sky | Never |
|---|---|---|
| Glassfall | cool oceanic: cloud and rain most days, sea mist, a little snow | -- |
| Hemlow | an English valley: long fair evenings, showers that pass, river mist, a little snow in winter | -- |
| Jadewater | tropical karst: hot, wet, often shut in; rain as often as sun | snow |
| Lowmouth | the Gulf coast: heat, afternoon downpours, river fog | snow |
| Redrock | high desert going dry: glare, the short hard rain that cut the cliffs, snow on the rim in winter | -- |
| Thornpan | the flood comes in the dry season, so sun and heat with a little mist | snow, rain in quantity |
| Windbarrow | open downs with cloud driving over, rain in it, fog and winter snow | heat |
| Ribshore | the Oregon coast in the wet half of the year: sea fog, low cloud and rain most days, few clear ones | heat, snow |
| Slagdeck | a cold coast, open to the weather: grey, and wet some of the time | heat, snow, fog |
| Saltmirror | a desert pan: sun and heat, now and then a shower that floods the pan, dry storms | snow |
| Duelbox | one kind of day for ever | everything else |

Two of those are rules about play rather than about the place. Windbarrow
leaves heat out because heat is a windless day and the wind there never stops.
Slagdeck leaves fog out because a deathmatch should not be decided by how far
you can see, and Duelbox has one kind of day so a gun measured today is
measured under the same sky tomorrow. Every world opens on the sky its look
was graded under and drifts from there.

**Where it comes from.** Each spell runs for a few game hours -- fair weather
sits for 4 to 9, rain passes in 1 to 3.5 -- and then the next kind is drawn
from a table of what may follow what, weighted by how common each is in that
world. Rain never breaks straight into sun and sun never opens into rain:
both go by way of cloud, which is the hinge of the table. A world sets its own
climate with `"weather"` in `world.json`, a map of kind name to weight; a kind
left out of a block that has anything in it never happens there, so a desert
lists everything but snow. Where nothing a kind can turn into is allowed, the
weather stays as it is rather than leaving the climate, which is what holds
Duelbox's one sky. `"start_weather"` says what a new game opens on, and
without it the world opens on the commonest kind it has. `--validate` names
any kind a world asks for that nothing is called. The whole thing
runs on the game clock, so stopping the clock holds the sky as well as the
light.

**Dry storms** (`storm`) close the sky in dark and cool and bring the wind
up, but nothing falls: lightning comes out of the deck instead
(`src/world/lightning.rs`). A strike lands somewhere between a few hundred
metres and the horizon, every few seconds in the thick of it, and is one to
four return strokes over half a second. It touches no voxel. The channel is
drawn into the sky colour (`bolt_light` in `common.wgsl`), a jagged line from
the cloud base to the ground with one branch, so everything that mirrors the
sky shows it as well: a wet pan, a puddle, the sea. The flash lifts the sky
and its fill light a cold blue-white for as long as the stroke lasts, so the
whole land blinks with it. There is no thunder. A storm never turns to rain
and only follows sun, heat or cloud; a world with no climate of its own never
sees one. `VG_BOLT=x,z[,flash]` holds a strike there for a headless shot.

**Looking at it.** **M** steps through the seven kinds and pins the weather
where it is put, the way N steps the hour; the stats panel (G) shows the clock
and the kind. `--weather KIND` holds one kind for a headless shot. The launch
screen has a WEATHER switch; off keeps fair weather for the whole run.

Weather reaches the player only through the animals: rain masks your
footsteps and the wind carries your scent (Stalking, below). There is no
slipping, no cold and no visibility rule in the shooter.

## What the weather leaves

The weather is what is falling now; `src/world/damp.rs` is what it has left
on the ground, so it runs on the history rather than the moment. Like lying
snow it is one state for the whole world, a few numbers stepped on the game
clock, and where on the land each shows is the shaders' business.

- **Wet.** Rain and melt water soak the ground; sun, wind and heat dry it,
  but only once the rain stops, so a shower leaves the land dark for a few
  hours after the sky clears. A clear, still night brings dew (damp, never
  soaked) and a foggy day keeps it damp. Wet ground goes darker and deeper
  in colour and takes a faint sheen of the sky; plants darken less and
  leaves take no sheen (`damp_ground` and `damp_light` in `scene.wgsl`, and
  the grass in `foliage.wgsl`). Rock faces and tree trunks wet nearly as
  much as the ground, in vertical streaks where the rain ran down, and the
  underside of an overhang stays drier. Creatures and other things out in
  the open darken too (`entity_colour`), less underneath, but not under the
  sea or under the land.
- **Puddles.** Once the ground is near soaked, standing water fills and then
  drains slower than the wet dries, so puddles outlast the shower by an
  afternoon. They lie only on level bare ground -- paths, earth, rock, never
  grass or leaves -- a little thicker in hollows, grow outward from a noise
  as they fill, and have a soaked dark rim. A puddle is mostly the sky it
  reflects, clouds included, over the dark bed under it, and rain roughens
  it.
- **Frost.** It forms on a clear, still night in a cold season (every winter
  night can, spring and autumn half as hard, summer never, and a climate
  that never snows a quarter as much), whitening grass from the tips down
  and anything that faces up. It is two numbers, frost where the sun has
  been and frost where it has not, and each point picks between them by its
  own shadow, so after sunrise a white edge retreats behind every shadow:
  the sun takes it within the hour, shade keeps it for hours on a winter
  morning. Points of rime glitter in the sun close up.
- **Fog in the hollows.** A clear, calm night over ground with water in it
  thickens a mist that lies lowest in the valleys (it is added to the mist
  floor, which already pools below the smoothed ground); the sun burns it
  off within a couple of hours and any wind clears it.

None of it is saved: a world opens on ground that suits the sky it opens
under. `VG_DAMP=wet,pools,frost_shade,frost_sun,fog` (each 0 to 1, any left
off kept) sets it for a headless shot, for instance
`VG_DAMP=0,0,0.55,0 --hour 7.4 --season autumn` for a frosty morning.

### Sheet floods, reflections and the mirage

A ground material with `"floods": true` (a salt pan, a playa) does not
gather puddles. Its first standing water covers it from edge to edge, an inch
of clear water with the bed bright through it, and it stays a sheet until
the water is nearly gone, breaking into islands of crust only at the end
(`damp_ground` in `scene.wgsl`). Saltmirror's salt and brine mud flood.

Standing water reflects the world, not only the sky. The terrain writes
alpha over 1 where it holds a pool or a sheet, and the water pass adds its
coverage to the bed's alpha instead of blending it, so the post pass can
tell a mirror from everything else (opaque surfaces write exactly 1). For
each mirror pixel `mirror` in `post.wgsl` reflects the view about straight
up from the water's surface (the sea's level for open water, whose own depth
is not in the buffer) and marches it across the depth buffer; what it first
passes behind is what the water shows, weighted by the Fresnel term. What
is off the screen is not reflected, and the sky the scene shader put there
stays. `VOXEL_SSR=0` turns it off for a comparison.

A world may set `mirage` in `world.json` (0 to 1, default 0). In the heat of
the day, with the sun well up and the ground dry, everything below the
horizon from about a hundred metres out shows what stands at four times the
angle above it, upside down: the far flat goes to sky-coloured water and
whatever stands on it floats over its own squeezed reflection. The air at
the horizon shimmers (`mirage` in `post.wgsl`, strength from
`Game::mirage`). Saltmirror sets 1.

The mirage is worked out from each pixel's ray in the world, so it lies
along the true horizon when the view banks. A pixel takes part only where
its ray reaches the bottom two and a half metres of air over the ground
under the camera, more than about a hundred metres out and at a grazing
angle (under about 1.7 degrees): anything nearer or taller hides it, and
from high up the band thins away past the world's edge. `VG_ROLL=degrees`
banks the camera for a headless shot.

## Seasons

The part of the year the whole world is in (`src/world/season.rs`), turning
over about every two days. Four seasons -- **spring**, **summer**, **autumn**
and **winter** -- of which a world has one or more: a world names the ones it
gets and the year goes round only those. Summer is what the looks were tuned
in, so its row of numbers is all ones and zeroes, and a world of nothing but
summer never turns at all.

Seasons and weather are the same shape and different in one way that decides
everything about how they are built. The weather is numbers read every frame,
so it can slide from one kind of day to another in half an hour. A season is
partly **voxels** -- a leaf is geometry, not a tint -- so it cannot slide. The
two halves are handled apart.

**What slides.** The palette (saturation, exposure, warmth), the colour of
every growing thing, which weather the world leans toward, and how much of
each species is abroad: all of these cross from the old season's row to the
new one's over the first half of the new season, which is a good ten minutes
of real time.

**What turns, tree by tree.** Leaf colour, bare branches and blossom are baked
into the mesh when a column is built. So each tree turns on its own: `Turn`
holds the season being left, the one arriving and how far through the change
the world is, and every tree compares its own seed against that. The change
is cut into four steps over the first half of the season, and each step has
the loaded columns built again, nearest first and swapped in place, the way
the slopes switch does. Autumn therefore walks through a wood over about
twenty-five minutes rather than crossing it in a line. A turn between two
seasons whose trees look the same -- and every turn in a one-season world --
costs no rebuilding at all.

**When a season turns.** It lasts two days on average and is not fixed: most
turns are pulled into the small hours, where the leaves can change without
being watched, and the rest are left to fall where they will, so a season can
end at any time of day. The pull picks the night before or the night after in
the proportion that leaves the average exactly two days. A world opening in
the afternoon is therefore about a quarter of the way through its season.

**Dry grass.** world.json `straw` (0 to 1, default 0) holds the grass that far
over to straw whatever the season, for a dry country; a season with more straw
of its own (autumn 0.45, winter 0.8) still takes it further. Megalith, a steppe
arena, sets 0.9: its turf is tan in summer.

**The leaves.** A plant with an `autumn` colour in its library entry is
deciduous: it wears that colour in autumn and stands bare in winter, keeping
its trunk and its limbs. Everything without one is evergreen and keeps its
leaf all year, which is how a world whose trees have no autumn colour can have
an autumn that only changes the light. Blossom comes from the library's
`flower_seasons`: a plant is grown in flower whatever the year
is doing and the blossom is taken off again out of its own season.

**The grass** goes over to straw with the year rather than changing shape: the
season carries a green multiply and a straw amount, and the shaders put them
on leaves and grass (`seasoned` and `strawed` in `common.wgsl`). A leaf takes
only the colour -- it turns or it falls, and both of those are voxels -- while
grass, meadow and the plant sprites go to hay in autumn and winter.

**Snow lies.** What makes snow fall is the weather; what makes it lie is the
season. It builds while snow falls and melts at a rate the season sets, so a
shower in autumn is gone by evening and the same shower in winter stays and
deepens. The world is built for one of three depths -- none, a covering, deep
-- with a little hysteresis, so a flurry either side of a line does not
rebuild the world twice. A covering lies in patches on anything not steep; a
deep fall lies on everything short of a rock face, over whatever that ground
is made of, but never on a street, a laid floor or a lake bed. The permanent
snow line does not move with the season. Winter sees to it that the snow starts
somewhere in its first day rather than leaving a whole winter to the dice --
unless the world's climate has no snow in it at all, which is what makes
Thornpan's winter the dry season it should be. Snow reaches the player only
through the animals, which hear a step on it less: it does not slow walking
and it is not deep enough to wade.

**Animals.** The year thins a species the way the hour does. Insects are
almost entirely gone in winter, half the birds with them (migration), and most
of the walkers are still out. A library creature states its own with
`winter`, 0 (sleeps or leaves) to 1 (out all year), and between winter and
summer the year walks from that to all of it, so one number covers hibernating
and migrating both.

**What each world gets.**

| World | Its year | Why |
|---|---|---|
| Glassfall | all four | a cool oceanic island: everything in its turn |
| Hemlow | all four | an English valley, and the one world with a proper autumn wood |
| Windbarrow | all four | open chalk downs under a northern sky |
| Redrock | all four | high desert: it freezes, and snow on red rock is worth seeing |
| Lowmouth | spring, summer, autumn | the Gulf coast has no winter to speak of |
| Jadewater | spring, summer | the tropics: a wet season and a warm one, and nothing bare |
| Thornpan | summer, winter | the delta's wet and dry; its winter is the dry season, so the trees go bare and no snow falls |
| Slagdeck | summer | a match is played under one sky |
| Duelbox | summer | a gun measured today is measured under the same sky tomorrow |

A world states its own with `"seasons"` in `world.json`, a list of names in
any order (the year always goes round spring, summer, autumn, winter, visiting
the ones it has), and `"start_season"` says which one a new game opens in.
Leaving both out gives summer alone. `--validate` names any season a world asks for that
nothing is called, and any start that is not one of its own.

**Looking at it.** **Y** steps to the next season this world has, arriving in
it at once rather than turning into it over half a season; the stats panel (G)
shows the day, the hour, the season and the weather. `--season NAME` opens a
headless shot in one. The launch screen offers SEASON (a world with more than
one) and TIME: picking either holds it there for the run, and leaving SEASON
on Turning lets the year go round. The season and the day are kept in the
save, so coming back to a world is not always the season it opens in.

## Slopes

By default the ground is not a staircase of voxels: the mesher moves the
corners of grass, dirt, sand, gravel, snow and rock surfaces up or down to the
average height of the ground around them, so steps of up to two voxels a
column become even inclines with flat-shaded facets. Where two ground
materials meet (grass and sand, say), the border is drawn as a smooth, wavy
line rather than in voxel steps. Trees are rounded too: canopies become
faceted blobs and trunks octagons, flaring back to a square foot where they
meet the ground. Planks, brick and creatures keep their square edges, and so
do cliffs, whose risers are taller than the smoothing reaches. B or `--blocky`
switches all of it back to plain voxels; the change regenerates the loaded
world nearest first, swapping each column in place.

A world can bevel materials that are square everywhere else by naming them in
a `"round_edges"` list in its `world.json` (the library flag is
`bevel_steps`). The rule is for built things, not land: a corner where every
face meeting it runs flat for two voxels stays square, so box edges, wall
corners and the foot of a wall keep their lines, while a staggered edge of
one-voxel pieces (a stair, the stepped side of a blast hole, a stepped circle)
is bevelled like faceted rock. A corner never moves along an axis where the
shape runs straight on, so a stepped pillar rounds in plan and the floor round
its foot stays flat (`clean_axes` in `mesh.rs`). The arenas list their walls
and decks, but not pads, lamps, glass or liquids; it adds about 2% to
an arena's quads. Delete the line to turn it off for that world.

On gentle ground the corners come from the generator's true ground height,
not only the voxels: where the four columns round a corner are unedited and
within a voxel of each other, the corner sits at their mean true height
(`corner_raw` in `mesh.rs`; the generator hands each chunk's column heights
to the mesher, and `Store::ground` gives them to feet and wheels). A gentle
hill is then an even ramp instead of 0.25 m steps. The mesh keeps corners to
quarter voxels, so the drawn slope still has faint 6 cm terraces; physics
uses the exact heights. `--ride bike` prints a "ride jolt" figure (RMS change
in vertical speed per tick on the ground), and `VG_ROUND_GROUND=1` turns the
exact heights off for feet and wheels, for comparison.

Collision, digging and building still use the voxels. The eye and the
creatures are drawn on the sloped surface instead (`physics::drawn_ground_lift`),
so feet don't float or sink against it. A single dug voxel shows as a shallow
dip rather than a square hole.

Nearly every face leans a little, so face directions can't be dropped
outright when the camera is behind them. Each chunk keeps its leaning faces
sorted by how far they lean (under 15, 26, 40 and 55 degrees, then the rest),
and a class is skipped once the camera is far enough behind it for none of
them to face it; the shadow pass does the same against the sun. Rounded trees
still draw about a third more quads than square ones, and meshing a chunk with
slopes and rounding takes about 1.8 times as long as plain voxels
(`cargo test --release mesh_bench -- --ignored --nocapture`).

Leaves are drawn coarser than they are stored. Near the camera the mesher
groups leaf voxels into 0.5 m cells, takes a cell with at least five leaf
voxels of its eight as leaves, and rounds that grid into the crown's surface
(`leaf_blocks` in `mesh.rs`); in the next level out the cells are 1 m. Meshed
voxel by voxel, crowns are 85-90% of every quad drawn, and this draws about a
quarter as many for the same crown. Each leaf face is lit by the crown's smoothed shape, not its
own facet, so a crown shades as one soft volume; bark keeps its facets. The
leaf sprites around a crown make its outline, and sprites that would sit
inside the coarser surface are left out. Bark is meshed as if the leaves
weren't there, so a limb runs on inside its crown and no gap can open where
it goes in. Collision, digging and the sprites still use the 0.25 m voxels.
`VOXEL_LEAF_BLOCKS=0` meshes leaves voxel by voxel again, for comparisons.

Trees cast the shadows of their square voxels (leaf cells for leaves), not
their rounded shape: rounded shadows needed about 60% more shadow quads. The
rounded surface lies within a fraction of a cell of the square one, so it
doesn't shadow itself, but a canopy's shadow on the ground has a blocky
outline. Tube faces (mesh/lean.rs, marked by the quad's tube bit) are the
exception: their square faces are only the anchors they hang from, so they
cast from their real corners, and a leaning stem or a rope casts a smooth
shadow. Entity models (the bike, the glider, the guns) get tubes the same
way: `models::Grid::rod` records a thread, and `mesh::mesh_model_t` draws it.

## Detail sprites

Flat sprites stand beside some blocks: grass tufts and wildflowers on grass
(mostly short, tall in meadow patches set by a noise field, flowers in
drifts), sparse dune grass on sand, leaf clusters around broadleaf crowns, and
level, drooping branch sprays out of the sides of pines. Library grounds and
canopies grow them by name (anything called grass, forest floor, needles or
leaves). The wind bends them, with gusts that roll across a meadow and calm
spells between them. They are only drawn: physics never sees them, nothing is
stored, and digging a block remeshes what grew on it.

The mesher writes them as records after a chunk's six face groups
(`src/mesh/sprites.rs`); `src/render/foliage.rs` draws them after the terrain
with alpha to coverage, and `src/shaders/foliage.wgsl` places them and adds
the wind. Tufts shrink away between 32 and 46 m, leaves and branches between
70 and 92 m, and nothing grows below sea level. Sprites cast no shadows.
`VOXEL_FOLIAGE=0` in the environment turns them off for comparisons.

A card grows out of the ground as it fades in. `GROW_CURVE` (0.4) shapes that:
nearly all of the height arrives while the card is still far off and small,
and the last of it comes in slowly, instead of the grass pushing up out of the
dirt a few metres in front of you. `VOXEL_GROW_CURVE=1` gives a straight
ramp.

### The far cover

Past about 40 m the tufts are gone and the ground would be bare colour, which
is most of what made the distance look empty. The far cover fills it: one wide
card of grass per cell, grown in between 26 and 40 m as the tufts shrink away,
and then never shrunk again. Unlike every other sprite it is scattered at
**every** level of detail, not only the full-detail ring, so it carries the
ground's cover out to the edge of the world.

Its cells and its cards are measured in the column's own voxels, so both
double in metres with each coarser level: 1 m cells and 1.6 m cards at full
detail, 2 m and 3.2 m in the next ring out, and so on. The count per ring
therefore stays about the same however far the view reaches, and a card stays
about the same size on screen. Only the height is held back (it grows as the
square root of the step), or far ridges would wear four-metre grass.

Two things it has to get right:

- The cards are **wider than their cells**, so each piece of ground lies under
  two or three of them. Spaced to tile exactly, each card reads as its own
  mark and a hillside comes out stippled with dashes.
- A card is about as wide as it is tall, and turns to face the camera (yaw
  and pitch both), so it leans back when you look down a slope at it. A card
  much wider than tall flattens into a dark stroke lying on the ground as soon
  as you stop looking straight at it, and a field of strokes all turned the
  same way combs the hillside into corduroy.

`VOXEL_GRASS_PATCHES=0` turns the far cover off, which is the quickest way to
see what it is doing.

Prairie grass (Glassfall's prairie, Redrock's high meadow) flowers a little
more than other meadows and in four colours: white, blue, yellow and scarlet
paintbrush. One colour leads in each patch of about 25 m, and here and there a
field of nearly one colour grows denser (`flower` in `src/mesh/sprites.rs`).
Forest shrubs (huckleberry, thimbleberry) are sprite-only herbs like the ferns,
so you walk through them.

A canopy is lit in broad clumps of sun and shade, from a noise field in the
world about 2.5 m across (`canopy_dapple` in `src/shaders/scene.wgsl`). Leaf
cards and the voxels behind them take the same field at the same strength, so a
crown keeps its clumps as its cards shrink away and does not change character
at 92 m. It costs one noise lookup per card vertex and per leaf fragment, and
nothing per tree. `VOXEL_DAPPLE=0` turns it off for comparisons.

The images are PNGs in `library/sprites/`, which override the drawings in
`src/render/sprite_tex.rs`; that folder's README is the spec for revising them.

Wood lying along X or Z (tree limbs lower than 45 degrees, fallen logs) uses a
grain copy of its material, made at startup on an id the library leaves free
(`matlib::grained`; saves name them `<material>@x` and `@z`), and the shader
turns the texture so the grain runs along the wood and puts the end-grain
rings on the log's ends instead of its top. `VOXEL_GRAIN=0` turns it off for
comparisons.

## Particles

Short-lived bits thrown up by something happening: dust and grit off a hard
landing, dust behind the bike's back wheel, snow kicked up underfoot, a splash
where you go into the water, spray off each paddle stroke, your breath on a
cold morning or in winter, smoke and sparks at a muzzle, chips off a wall that
was shot in the wall's own colour, and the smoke and dirt of a burst. In
autumn leaves come down from trees wearing their autumn colour, in spring
petals from trees in blossom, and in summer a few seeds drift on the breeze.
Leaves, petals and grit land and lie a while before they go.

Each effect is a library entry under `library/fx/` (shape, count, speed,
spread, life, size, gravity, drag, wind, colour or the material's colour), so
changing how dust looks is editing a JSON file. Code only says where and when:
`src/fx/hooks.rs` turns the player's steps into bursts by comparing the body
before and after a step, `src/fx/ambient.rs` looks down columns near the
camera for a crown to drop from, and `Combat` records cues (`combat::Cue`)
that `Game::combat_fx` turns into particles. Animals do the same: a fish
leaping, a seal diving in, a whale's blow and breach push a `fx::Cue` onto
`Entities::cues`, which `Game` throws after each step. The pool (`src/fx/mod.rs`) is on
the CPU, up to 8192 at once, stepped with the world and with its own random
stream, so it changes nothing a match, a bench or a seeded run measures.

`src/render/particles.rs` packs the pool into one storage buffer a frame and
draws it twice from `src/shaders/particles.wgsl`: chips and leaves are solid,
write depth and go in with the sign boards; puffs and streaks blend after the
water, sorted far to near, lit once per particle rather than per pixel. A
material's colour comes from the smallest mip of its texture, the way grass
tints come. Puffs fade out near the camera and never grow past half their
distance, so a landing at your feet doesn't fill the screen.
Particles, motes, waterfall spray, sign plates and the sea plane take their
quad corners from one place (`quad_corner` and `billboard` at the end of
`common.wgsl`).
`VOXEL_FX=0` in the environment turns them off for comparisons.

## Sound

Everything you hear is synthesised while it plays; the game ships no
recordings. There are three buses, each with its level in SETTINGS:
**ambience** (the place itself), **effects** (guns, rounds striking, bursts,
footsteps, pickups, the match's signals, and the animals when they have
voices) and **music** (nothing yet). VOLUME is over all three, and the levels
are heard on a curve, so 50 sounds like half. SOUND off lets go of the sound
device altogether. The mix ducks to a third behind the pause menu and goes
silent when the window loses focus. A machine with no output device plays
silently; headphones pulled out are picked up again within two seconds.

### The ambience

The ambience is worked out from the world round you ten times a second and is
quiet on purpose: near the edge of hearing at the default volume. Four things
make it, and the F9 readout lists each number and what set it:

- **The biome**, sampled on two rings round you (12 m and 30 m) so an edge
  crossfades: how much wind reaches the ground there, how much grass there is
  to stir, and what rain lands on. A biome's `sound` block says so where the
  defaults are wrong (`library/README.md`); without one, grass comes from the
  surface material (its `grass`, or a name holding "grass", as the sprites
  have it) and the ground from the material's name.
- **The weather**: the wind drives the gusts and every rustle, rain opens
  the rain, and fog and falling snow hush the whole mix.
- **The water**: rivers and lakes (still water barely laps), every
  waterfall, and the surf where the sea breaks in the shallows, each summed
  over rings of samples into a direction and a spread. A river 50 m off is
  narrow and to one side; standing in it, it is all round. Under water the
  whole mix goes dull and a low rumble takes over.
- **The voxels round your head**: leaf cells near and above you are the
  leaves you hear (so one tree in a meadow rustles and the meadow doesn't),
  five rays up say how much sky you can see (under a roof the near raindrops
  stop and the hiss goes dull; drips go on under leaves for a minute or two
  after the rain), and eight level rays find the walls that make the reverb
  (short and dark in a wood, long in a canyon or a cave).

The voices are filtered noise, crowds of short events (leaf slaps,
raindrops, drips; Cook's PhISEM; grass is a crowd too dense to pick out,
swelling as the sward sways, since sparse sharp ones sound like rain), bubbles for running water (van den Doel's
model: a bubble rings at about 3.26 / radius Hz with a rising pitch) and
three wave trains for the surf. All eleven measure within 4 dB of -32 dBFS at
full strength (`sound::tests::every_voice_is_about_as_loud`; a slow river is
the quiet one), so the numbers in the readout are the mix. Running water is
heard by how much river the rings round the ear find, so a creek on its bank
is about half as loud as a broad river and a lot gentler: slow water is a
few soft bubbles over a murmur, fast water a crowd of them.

### Effects

Each gun's report is worked out from its fire mode (`sound::fx::Report::of`):
a fast hitscan gun is a minigun, a heavy one a sniper rifle, a shell with a
big burst a rocket, one that bounces a disc, and so on, so a new weapon
sounds like something the day it is written. A shot is made of noise, with
no tones in it (a sine falling in pitch is how a drum is made, and the guns
sounded like drums while they had one): a crack of highpassed noise a few
milliseconds long, a broadband blast, a boom of noise lowpassed near 150 Hz,
and a tail, the sum driven through a soft clipper. The sniper rifle's crack
comes twice, the second a few milliseconds behind like an echo off the
ground. Every shot and burst starts on a pressure pulse (the Friedlander
waveform a muzzle blast actually is: a jump, a fall through zero, a shallow
negative phase), a millisecond for the sidearm, three for the rocket tube,
six for a burst; noise alone is heard as "chh" and the pulse as a pop. The
minigun's rounds are over in about 25 ms, so at 1100 rpm they are a
rat-a-tat rather than a roar, and every round is a little louder or softer
than the last. A burst is the same with a bigger boom, lower the bigger it is. A
rocket in flight roars where it is, as a held voice like a beam's, posted
each frame by the watch and pitched up coming on and down going away.
Beams hum while they are held; the bore's is a sawtooth drone at 46 Hz
over a grinding rumble. Rounds striking the world sound like what they struck, and
now and then whine off rock or metal.

A round that goes past you whizzes. `Combat::fire` posts each hitscan
round's line from the muzzle to where it stopped (`Noise::past`), and
`fx::place` finds where that line came nearest your ear: within 5 m, and
only if the round carried on past (one that stopped short of you, or hit
you, is heard as that instead) and the nearest point is not at the muzzle
(your own rounds). The whizz is narrow noise falling in pitch, the Doppler
sweep of something passing a still listener, placed at that nearest point
so it is on the side it went by. The sniper's round is taken to be
supersonic and snaps as well. A scatter gun's pellets go past as one
whizz.

Every effect is placed against you: louder near (inverse distance from
2.5 m), panned by bearing, duller through more air and from behind, and late
by the speed of sound, so a burst 200 m off is a flash and then, half a
second later, a thud.

Code posts a sound with `sound::emit(Noise)` from wherever it happens; the
window takes them once a frame. Gunfire is posted by `Combat::fire` and
`hold_beam`, bounces by the shell, impacts and bursts by `Game` from the
fight's cues, digging and placing by `Game::dig` and `place`. Everything
else is heard by watching (`src/sound/watch.rs`): footsteps by the ground
underfoot (a heel and then a toe 70 ms later, each a soft thud with the
ground's scuff, swish, crunch or slosh over it, well under the guns), jumps, landings, jump pads, teleporters, and in a match the
player hurt or killed or respawned, pickups by kind, reloads, a new weapon in
hand, the hit marker and the killing-blow marker going up, bots hit and
going down, and flags changing hands. Hurt sounds come at most every 0.3 s,
because a beam's damage lands every step.

### Checking it

You cannot hear a test, so sound is checked by measuring and by listening to
files. `--sound-bake FILE --secs N` opens a world with no window, stands the
camera where `--pos`/`--ground`, `--look`, `--hour`, `--weather` and
`--season` say, runs the world (bots fight, the player stands), and renders
what would be heard into a WAV, printing the readout, the level and every
sound posted:

```bash
cargo run --release -- --world glassfall --ground 330,290 --weather rain --sound-bake rain_in_the_pines.wav --secs 30
cargo run --release -- --world slagdeck --sound-bake match.wav --secs 30
```

## Tree crowns

Leaf voxels turn and flip their 1 m texture tile per voxel (one of eight ways
round, `fs_terrain` in scene.wgsl), so far crowns show no rows once their
leaf cards have faded.

A crown is built from four shapes (`src/world/canopy.rs`), not from spheres
alone: a ball, a filled dome (domed over, flatter underneath), a flat plate,
and a sleeve of leaves along a limb. Each measures itself as one normalised
distance, 0 at its middle and 1 at its surface, so `src/world/gen.rs`
rasterises all four in one loop and gives each the same ragged edge, the same
sparse gaps near the skin, and the same broad lumps: its radius swelling and
shrinking by a fifth in a few directions fixed per clump, from two dot products
on a direction the distance already gave, so a crown keeps lobes and notches at
any distance without a noise lookup on every voxel inside it.

The `branching` growth form (`src/world/flora.rs`) builds every broadleaf tree.
The trunk sheds its first limbs at staggered heights over the top quarter of
its clear length, never all from one node, and at each one the wood is shared
by area conservation (a child keeps the parent's cross-section to the power
1/2.3), so the trunk thins as it sheds limbs instead of pinching at a fork.
Each limb is traced as a chain of capsules along a curve (`curve` picks
whether it bows out and levels off, sweeps out and up, runs straight, droops,
or kinks), aimed at the box the crown is meant to fill (so far out, so far up)
and pre-bent by half the curve's turn, so the bow fits inside the crown instead
of being added to it. Its forks end in twigs, and a lobe of foliage is grown
over each limb's own twigs, just big enough to cover them: no wood is left bare
outside the leaves. `canopy` then decides how the lobes are massed: lobes with
the notches part bridged, a beech's upswept vase, an elder's flat tiers, a live
oak's one cloud, or leaves on the twigs only. `fill` sets how often a notch is
bridged, `leader` how much of the trunk carries on up through the crown. A
plant's `flower` lies in flat heads on the sunlit top of the crown, over the
green.

Conifers go through the same shapes, by a different route. A `conifer`,
`columnar` or `sapling` form builds a leader with whorls of branches up it
(`flora::conifer_crown`): a core of sleeves following the cone's profile, a
wide shallow dome at each whorl, and a sleeve of needles along each whorl arm,
with the arm's wood drawn only where the needles are thick enough to cover it.
`sweep` sets how the arms sit ("level" for a fir, the default, "droop" for a
spruce or a redwood, "up" for a pine, or "ragged", all three down one tree),
`taper` how fast the cone narrows, and `whorls` how many tiers.

The thing a conifer has to get right is clothing its own trunk. Leaves are
meshed in cells of 2x2x2 voxels and a cell counts as leaves only when five of
its eight are (see "Slopes"), so it is not enough for the needles to reach the
bark: there has to be room for a whole cell with no bark in it, which is about
0.7 m at full detail. Under that, every cell round the trunk is part wood,
none of them is leaves, and the trunk is plainly visible through a crown that
looks solid. So the trunk stops where the cone narrows past what
can clothe it, and `gen.rs` rasterises a cell of needles round the wood
directly, measured from the trunk's own centre (which is snapped to a voxel
corner, so it stands up to half a voxel off the tree's). Past the full-detail
band the cells are 1 m and 2 m across and can clothe nothing at any price, so
there the trunk simply stops at the foot of the crown: no one can see a
conifer's trunk through its own crown at that range. The whorl limbs are left
out there too: drawn as the chain of coarse voxels their centre line crosses,
they stuck out of their needle sleeves as rust-coloured blocks across every
distant spruce. And the needles run up the axis there as a spine one leaf cell
across, as high as the clumps reach it: the leader's core near the tip is
under 0.3 m solid, which coarse voxels caught in some layers and missed in
others, so the tip came apart into flecks hanging over the crown.

How much clothing is needed falls away with the trunk, and has to: what the
margin buys is room for a cell with no bark in it, so a sapling whose stem is
one voxel across needs almost none, while a redwood's fills cells three deep.
Charged the full margin, and held to the same absolute clump minimums, a
sapling barely a metre wide would come out a flat-topped slab of needles. So
the margin is capped against the bark's own width
(`flora::conifer_clothe`), the clump minimums against the crown's radius, and
the core's minimum draws in toward the tip, which is what lets a 3 m sapling
and a 40 m redwood come out of the same code as cones.

A root flare of 1.5 m or more (`buttress` on the plant, `PLANK_FLARE` in
`stamp.rs`) is drawn as plank buttresses: four to seven thin walls standing
out from the trunk, each with its own bearing, reach and height, sweeping
down from high on the trunk to far out on the ground. A smaller flare is the
old fluted cone. At rainforest sizes a cone reads as a plinth the tree stands
on.

Every trunk and crown clump is rasterised in blocks of 4x4x4 voxels before
any voxel is tested (`raster_into` in `stamp.rs`). A block wholly outside the
shape is skipped and one wholly inside its solid core is filled without a
test, both decided from one sample at the block's middle and a bound on how
fast the shape's distance can change (`Blob::max_rate`, and for a trunk its
taper and flare, which only shrink as they climb). The bounds are exact, so
the voxels come out as they did before: on a grove of 80 m trees it halved
tree drawing, and every world's quad count and a screenshot came out
identical. The ragged edge's noise is looked up only inside the band where
it can change the answer. `Lumps::least` is the subtle part: the lumps
divide by the clump's normalised distance, which is a true length only for
an unsquashed ball, so along a sleeve they can cut much deeper than their
nominal fifth.

The tip is the other end of the same problem. A conifer really does taper to a
point, and nothing at 25 cm voxels can be both that narrow and solid enough to
mesh, so the last metre or so is left thin and its outline is carried by the
leaf cards. Holding every clump wide enough to mesh would turn each tree into
a flat-sided pipe as tall as its crown is thin.

## Performance

At 2560x1440 with 4x MSAA on an RTX 3060, the GPU frame is about 5 ms in the
densest forest, 3.5 ms at the Glassfall spawn view and 2.3 ms on open ground.
What keeps it there:

- Terrain and sprites are drawn indexed: four vertices a quad sharing one
  index buffer, not six. Terrain is bound by vertex work, so this saves
  about 30%.
- Sprites are lit, shadowed and fogged per vertex (`foliage.wgsl`); the
  fragment shader only reads the texture. Past 15 m (25 m for leaves) half the
  grass and 40% of the leaves shrink away one by one, and the rest widen.
- Leaf crowns are meshed as 0.5 m blocks near and 1 m blocks further out
  (see "Slopes"). The Hemlow wood draws 2.0M quads instead of 5.0M and the Lowmouth forest
  view 3.4M instead of 4.7M (128 m detail radius).
- Chunks hidden behind nearer ones are not drawn (`render/occlusion.rs`).
  Each frame ends by turning the depth buffer into a pyramid of farthest
  depths; the next frame tests each visible chunk's box against it, from the
  camera that drew it, and zeroes the hidden chunks' terrain and sprite draws
  on the GPU. Facing the Lowmouth forest at a 256 m view the GPU frame is
  8.9 ms with it and 13.7 ms without, with identical pixels. `VOXEL_OCCLUSION=0` turns
  it off for comparisons.
- The sky is drawn after everything opaque, only where the depth is clear.
- Sun rays are worked out at half resolution in their own pass, skipped when
  there are none.
- The outer shadow cascades are drawn in slices, centred ahead of a moving
  player (see "Time of day and light").
- Draws go to the GPU near to far, so the depth test can reject what is behind
  them before shading. They are put in that order by counting them into
  buckets four metres deep rather than sorting them (`near_to_far`): ten
  thousand draws take 0.07 ms to bucket against 0.34 ms to sort.
- Unloading frees voxel chunks on a background thread; freeing a few thousand
  32 KB chunks at once stalls the main thread for tens of milliseconds.

Underground, chunks entirely below the surface band are never stored (one
"solid" marker each). The world fields keep buried layers only for the top
3 m (`fields::BURIED_LIMIT`), and digging stops 3 m below the generated
ground ("BEDROCK"). Headless stats report `memory` (full and uniform voxel
chunks, field megabytes).

`VOXEL_GPU_PROFILE=1` times each part of the frame on the GPU (shadows,
terrain, creatures, sprites, sky, motes, water, post) with timestamp queries
and prints the averages every 60 frames as `GPUPROF` lines; `VOXEL_TRACE=1`
logs frames over 16 ms with their CPU breakdown. Both work headless.
`--bench` (or `tools/timing.sh --bench`) flies a fixed route and reports frame
time percentiles; run it on an otherwise idle machine. Under vsync the
world moves by a step evened out to the display's refresh rather than by the
loop's bunched intervals (`pacing.rs`, see docs/PERFORMANCE.md).
`--bench-route field` walks a loop at eye height instead, round the spawn or
`--ground`; on Hemlow that is a field of grazing animals. Hitches in play show
on the stats panel (F3) as `HITCH`, with the part of the frame that took the
time, and `VOXEL_TRACE=1` prints each one (docs/PERFORMANCE.md, "Finding
hitches in play").

## Ages, bones and predation

Every animal is one of four ages, and which it is was decided when it
spawned. Juveniles are smaller, with a head and ears big for them, and they
come in family clusters: in a group of deer the doe chose the spot and the
others are often fawns. Old animals are greyer and lighter in the coat, walk
more slowly and move more stiffly. Both are about one animal in twenty.

About one animal in forty is already dead: a skeleton lying where it fell,
which stays for half a day of game time and then is gone. The skeleton is the
bones inside that animal, derived from its own voxels, so it is the right
shape and the right size, and each one has fallen differently. Small animals
leave nothing, because a sparrow's or a lizard's bones do not read at this
size. Look at one and the crosshair names it: `CROCODILE (DECEASED)`, as it
says `DEER (JUVENILE)` or `COYOTE (AGED)`.

Predators catch what they chase. A coyote that gets within a metre of a
rabbit has it, and carries it off in its jaws at a walk for half a minute
before dropping it; something as big as the carrier is dragged instead, feet
trailing on the ground. What it is carrying is limp rather than posed, so it
swings as the coyote walks. A dropped kill lies as a body for a minute and is
bones after that. Cats, dogs, bears, badgers, boar, monkeys and seals lie
down on their sides rather than upright, and tip over to get there.

A carcass is dragged head first, in line with the carrier, so the mouth is at
its shoulder and the body trails behind; small prey is slung across the mouth.
When the carrier is done with it the ragdoll is run to a standstill there and
then, since nothing steps the ragdoll of a body that is not in a mouth.

`VG_DEAD_SHARE=0.3` makes nearly a third of the animals skeletons, which is
how to go and look at them; `VG_JUVENILE_SHARE` and `VG_AGED_SHARE` do the
same for the living stages.

## Stalking, prints and trampled grass

Animals sense the player by sight, noise, closeness and scent (CREATURES.md
"Running from the player" has the numbers). In play that means: approach
from downwind (the grass leans the way the wind blows, and your scent goes
that way); move while an animal has its head down and freeze when it comes
up, which lets its alarm settle; keep off dry leaf litter and gravel, use
moss, wet ground and the cover of rain; crouch. Spook one in a herd and the
rest go with it.

**Prints.** Walkers report a footfall each stride (`walk.rs`, within 90 m of
the player) and the player one every 0.75 m on foot; `src/creatures/sign.rs`
keeps them as marks, up to 4096, oldest going first. What the ground takes
is read from the material's name (`Tread`): snow, mud and sand always take a
print, bare earth once the rain has softened it, grass and low plants are
trampled instead, and rock, gravel, planks and leaf litter take nothing.
The print's shape follows the foot -- two-toed hooves (deer, cattle, pigs),
four toes round a pad (most mammals), a bear's five over a broad pad, a
bird's three forward and one back, the player's lugged boot -- and its size
the body. They are worn away by the weather: rain blurs mud and earth, new
snow fills snow, wind smooths sand, warmth melts it, and a fair day keeps a
snow print for a day. How worn a print is is how old it reads: crisp edges
and a deep dent fresh, soft and shallow old. `render/signs.rs` and
`shaders/signs.wgsl` draw them as flat quads that multiply into the ground
after the land is down, so a dent in snow goes blue-grey and one in mud
dark, whatever the light.

**Trampled grass.** Anything at least half a metre tall walking through
grass presses it down along its way, wider for a bigger body (a deer about
0.7 m across, a bison more); a herd on one line refreshes one trail rather
than laying patches on patches. The grass springs back over about six game
hours. A 64 m map round the camera (`render/foliage.rs` `trample_texels`)
tells the sprite shader how flat each spot is and which way it was laid, and
grass and low plants (not leaves up in a tree) are pressed down, laid over
and show their paler side.

`VG_MARKS=1` lays a short trail of each foot ahead of the player once the
world is in, for judging the prints and the trampling in a headless shot; on
ground that takes nothing it shows nothing.

## Herds

A species with a `group` block spawns in family groups, and the members of a
group steer by the three boid rules on the flat: away from anyone inside their
own `spacing`, toward the middle of the neighbours within `range`, and round
toward the way those neighbours face. `cohesion` sets how tightly they pack
(it also sets the footprint the herd holds: roughly `spacing * (1.2 + sqrt(n)
* 0.45) / cohesion` from the middle), `alignment` how strongly they face the
same way. A grazer that drifts right out of that footprint stops grazing and
walks back in, and when one bolts the rest within reach bolt the same way.
Thirty wildebeest at spacing 3 and cohesion 1.6 hold a herd about twelve
metres across.

Any walker wades to about a third of its own height, so a rabbit keeps to the
water's edge while an elephant crosses a channel; the `wade` verb goes deeper
still and `swims` goes over the head.

## Snakes

Snakes move by none of the above. A snake has no legs to swing and no
collision box; what it has is a path (`src/anim/spine.rs`). The mover leads
the head, every bone behind it keeps its own distance from the one ahead, so
the body follows where the head has been, and the path is let down onto
whatever it crosses: a rattlesnake going over a boulder lies over it rather
than through it. A wave runs down the joints as it goes, standing still on
the ground while the body slides through it, which is what lateral undulation
actually is.

There are three habits, one mover (`src/creatures/snake.rs`):

- On the ground (`slither`): crawling, lying coiled, or stretched out in the
  sun. A rattlesnake `warns` instead of leaving: it winds up tight, lifts
  its head at you and buzzes its tail for as long as you press it, and only
  crawls off if you keep at it. A gopher snake or a garter snake just goes.
- In the trees (`climb`): up a trunk in a slow spiral, then out to lie along
  a limb with the front of it hanging off and swaying. Stand right
  underneath one and it lets go: it falls limp (a ragdoll, the same code
  that settles the dead), lands, and crawls away.
- In the water (`swim`): at the surface with its head out and a long wave
  down its back, ducking under when startled, and hauling out on a bank now
  and then to lie in the sun.

Rattlesnakes are on the red desert, the slickrock and the canyon floor;
gopher snakes in the pinyon-juniper, the ponderosa and the prairie; garter
snakes in meadows, forest and river banks; green tree snakes in the karst
jungle and the bamboo; water snakes in the cypress swamp and the sawgrass;
and an anaconda, rarely, in the jade shallows and the cenote.

## Shooters

A world whose `world.json` carries a `"shooter"` block is played with a gun in
hand. The gun is never put away: `Game` starts the rules as an activity as
soon as the player is on the ground, and parks them rather than dropping them
while screenshot mode borrows the slot, so a look round costs you nothing.

The left button fires, the right is the alternate fire, 1 to 9 and 0 and the
wheel pick a weapon. Health is bottom left, the weapon and its rounds bottom right,
kills and deaths top right. Dying holds the body still for `respawn_s` seconds
and then puts it back at the next spawn point in the list, whole.

The split is three layers, and a second kind of shooter should reuse the lower
two:

- `library/weapons/*.json` is one gun: two fire modes (`fire` and `alt`), each
  a `hitscan` ray (pellets inside a cone, damage falling off with range), a
  `shell` that flies, drops, bounces and bursts, or a `beam` held on a target
  whose damage is by the second. Ammunition, cadence, spin-up, recoil and the
  flat shape the HUD draws are all in here, and so is each gun's one trick:
  `charge_s` holds a shot back while the button is down, `stick_s` makes a
  shell cling where it lands and wait, `head_mult` pays for a hit in the top
  of a body, and `zoom` turns the right button into a sight instead of a second
  mode.
- `src/combat/` fires it and works out what it hit: rays against the world and
  the creatures, shells in flight, bursts that damage and push everything they
  can see -- the shooter included, which is the rocket jump. It knows nothing
  about why anyone is shooting.
- `src/shooter.rs` is one set of rules: health, armour, dying, respawning,
  score, and the HUD. A sniper game would be a second set of rules over the
  same two layers below.

The gun is drawn in the world, not on the HUD: a voxel model held in the
camera's frame (`src/combat/viewmodel.rs`), so it takes the light of the room
it is in. It bobs with the walk, swings after a turn and settles back, drops
to the hip at a sprint, kicks back when it fires, and shows a muzzle flash;
the minigun's barrels are a model of their own and spin up with it. A weapon
file picks its shape with `"view"`, which is one of the shapes that exist in
Rust rather than something a file describes.

There are ten guns (`library/weapons/`): the Digger, sidearm, goo gun,
bore, pulse gun, skipjack, minigun, scrap gun, rocket launcher and sniper
rifle, on digits 1 to 9 and 0. The Digger is a caulk gun with a spade welded
under the nozzle: the left button both lets the blast off and bites the wall
in front of it, at the rate a shovel works out in the world, so anybody down
a borer hole can cut their own way out.

A body in a deathmatch moves faster and lighter than one out in the world,
and `src/player.rs` has all of it. The running pace with nothing held is the
8.3 m/s that is a sprint out in the world, and Shift is 15% over that out of a meter
that empties in 1.6 s and refills in 4.5 -- the bike's bargain, so the boost
is a decision rather than a key to hold. The meter is the lowest bar in the
bottom-left corner. A direction key tapped twice throws the body that way at
12.2 m/s, and Space inside a third of a second of that is a dodge-jump, which
lifts higher than a standing jump and keeps the dodge's run. There are three
jumps in one airtime rather than one, the dodge-jump standing in for the
first. The bots are the same body with the same rules (`shooter::mod`), and they
dodge too: a brain that wants one hands the key to `steer::Tapper`, which lets
go, presses, lets go and presses again over four body steps, so what reaches
the body is the keys a player's hand would be holding. How often is the bot's
own `tactics`, so a novice never dodges and a godlike one does it every other
second of a fight. It also dodges when somebody lands a shot on it, across the
line the shot came along rather than on a timer -- which is worth about a
third of the damage that would otherwise land on it.

`docs/SHOOTER.md` has the design. `--fire N`, `--alt N` and `--weapon N` drive it headless.

The numbers in those files are measured rather than guessed:
`lurelia --world <w> --duel 8` puts two bots in a world with one gun each,
every gun against every other, and prints what each one did and which beat
which. Run it on `worlds/duelbox` (one empty room, the gun on its own) and on
Slagdeck (the gun on a map); they disagree, and `docs/SHOOTER.md` says which
one to believe.

`--gun-sheet` lays every weapon out in a grid, one named row each, turned
through half a revolution across the columns, for one headless screenshot.
It backs the camera up against a wall and hangs the sheet in front of it, so
the backdrop is one flat surface; it wants a frame wider than it is tall,
because the grid is square and the slack either side is where the names go:

```bash
lurelia --headless --world duelbox --mode explore --gun-sheet --no-hud --size 2000x1500 --screenshot shots/guns/sheet.png
```

## Bots

A bot is a body that presses the keys. `Player::update` takes a `MoveInput`
and knows nothing about where it came from, so a bot is a `Player` whose
input is filled in by code (`src/brain/`), and it walks, falls, collides and
jumps through exactly the code you do. It cannot take a corner you could not
take or shoot through a wall, because neither is reachable from the input
struct. That is what forces every difficulty knob to be a knob on perception
and judgement rather than on health or damage.

Four layers, of which only the last is per-game: senses (`senses.rs`),
steering (`steer.rs`), navigation (`nav.rs`) and decision (`duel.rs`). A
racing brain or a stealth guard would be a different decision layer over the
same three. Flocking (boids) is a steering behaviour, and `creatures::herd`
composes it that way.

A world adds bots in its `shooter` block:

```json
"bots": [{ "name": "Rig", "skill": "easy", "colour": [120, 196, 214],
           "bold": 0.4, "steady": -0.2 }]
```

`skill` is one of novice, easy, average, skilled, adept, masterful, godlike,
easiest first (`brain::person::LEVELS`). `bold` and `steady` make a character
of a difficulty, so two bots at one skill fight differently. `colour` is whose
side it is on, washed over its plates.

Bots are drawn as the library's `robot` -- a `humanoid`, which is the root
species anything two-legged inherits from -- carrying the same guns you do, in
its hands rather than floating beside it. It walks, jogs, runs, strafes, backs
off, crouches, jumps, takes the landing in its knees, leans into a turn, lifts
its weapon when it means to use it and drops it at a sprint, and goes down as a
ragdoll where it was killed. How much of it is left is the lamp in its chest,
amber to red: health cannot be a tint on a body that has colours of its own,
because a tint can only darken. `docs/ANIMATION.md` has the rig and the driver
(`src/anim/figure.rs`).

They also need somewhere to walk: a `nav` block of nodes and links, where each
link says whether it is a walk, a jump or a drop, and the search filters by
what the body can do. Slagdeck's is written by `worlds/slagdeck/make_arena.py`
along with the deck itself, so the two cannot drift apart. Node heights are
metres above the floor the world's own spawn stands on, found from the voxels
once the world has streamed in; `docs/BOTS.md` says why that matters.

A world whose spawn point is inside a building needs `spawn_up` (metres above
the bare ground), or the player is pushed up through the building and comes
out on its roof.

### Gravity

A world sets its own gravity in `world.json`:

```json
"gravity": { "g": 0.3, "toward": "plane" }
```

`g` is a share of the engine's own 24 m/s² (1 if left out, and 0 to 4 is
allowed), so 0.3 makes a jump go a little over three times as high and hang
that much longer. Starfall uses 0.3, and the arena builder writes it for
any map in space. It applies to the player and the bots, dead bodies and their
chunks, shells (a weapon's `gravity` is scaled by `g`, and the bots' lead and
dodge prediction with it) and particles. The bike, glider, rope and the
creatures keep their own tuned gravity.

Under 0.6 the world counts as low gravity and a body in the air is steered:
the keys turn it toward where they point at 16 m/s² however fast it is going,
without taking off speed it already has that way, and with no key held it
drifts on. At the engine's own gravity a body thrown faster than a sprint
keeps its momentum and the keys only add to it.

`toward` is what gravity pulls toward. Only `"plane"` exists, which is straight
down everywhere. A point (an asteroid you walk all the way round) or a surface
(a ringworld) would be further values; `Gravity::down` in
`src/world/gravity.rs` is where falling things would read them, but collision
and the camera also assume down is -y and would need the same change. Any
other value is refused when the world loads.

The bots' nav graph is not rebuilt for gravity: its jump and drop links are
the ones the importer found, so in low gravity the bots walk the same routes
and do not use the higher jump.

A route may finish off the graph. A pad is not on a node on any map whose
items and nodes come from different places, so a `Path` carries a `tail`: the
nodes get the bot across the map and the last couple of metres are a walk
straight at the thing. Without it a bot stops at the node, two or three metres
from what it came for, and never picks anything up.

`--headless` reports how a match went under `shooter` in its JSON: the scores,
the node count, what each bot is doing and where, what it made of the pads on
the floor (`Scouting`), and `nav_audit` -- which of the graph's nodes burn,
which cannot be reached and which cannot be left. The last is what to look at
first on a map nobody drew by hand: a one-way pocket is a bot that walks in
and spends the rest of the match asking for routes that cannot exist.

## Lamps

Every other light in this engine comes from the sky -- the sun, the
hemisphere fill, and a baked sky-openness map that darkens whatever stands
under something else. That is right for a world you walk across and useless
the moment one puts a roof on: a roofed room is pitch black.

A lamp is a point light a world places with the `lamp` chronicle event: a
position, a reach and a strength. It casts no shadow of its own and is not
touched by the sky map, so it is the only thing that lights an interior.
`Game::prepare_frame` sends the nearest 64 to the camera and the scene shader
loops over them (`src/render/lamps.rs`, `lamp_light` in `common.wgsl`), with
a half-Lambert falloff so a wall under a lamp is dim rather than black.

A lamp is light and nothing else. The fitting it hangs from is voxels in
whatever structure holds it, and giving that fitting a material with `glow`
is what makes the lamp itself look lit.

## Platforms

Nothing else in this engine moves a piece of the world. Terrain is meshed in
chunks and a chunk that changed has to be remeshed, which is fine for digging
one voxel and hopeless for a lift that climbs eleven metres in under two
seconds. So a platform is not terrain. It is a rigid box of voxels with a
position of its own, drawn as a model instance the way the kayak and the bike
are, and made solid by handing the store a slab (`world::store::Slab`).
Everything that moves asks the store what is solid, so the player, the bots,
the animals and anything shot collide with a platform without knowing
platforms exist.

### Doors

A door is a platform, a leaf whose path runs sideways on a `near` trigger,
and no world has to write one down. In a structure's shape a door is a box of
`door_*` material; the loader finds every such box in the shapes the
chronicle placed and makes a plate of it, the generator leaves those voxels out so the doorway is
an opening, and the leaf stands in it and slides aside for whoever comes.
Hemlow has 14 and Lowmouth 39, found rather than authored.

Two things in the stepping are what make a plate read as a door rather than
as a lift lying on its side. `near` measures from where the plate sits at
**home** and not from where the leaf now is. Measured from the leaf, the
body that called it would fall out of range the moment it moved, the door
would shut on them, which puts them back in range, and it would chatter in
the doorway instead of opening. And at the far end of its path a `near` plate **holds**
while it is still called, where a lift comes back whether or not its rider
got off.

A door on a ruin stays terrain, because a ruin's door does not open.

A leaf is painted as a leaf. The tread-and-rim scheme that makes a lift read
as a plate from above is about a top face, and a door is only ever seen
edge-on: on a leaf one voxel thick every cell is a rim cell, so the whole top
row would go dark and every door would wear a cap. A plate taller than it is thick gets
a frame round its face instead, which is what a door has anyway.

A world states them in `world.json` (`src/platforms.rs`):

```json
"platforms": [{
  "name": "lift_left",
  "at": [120.5, 88.0], "up": 0.0, "size": [3.0, 0.5, 3.0],
  "path": [[0, 10.5, 0]], "speed": 3.5, "wait": 2.0, "trigger": "stand",
  "material": "iron_plate", "colour": [122, 128, 136]
}]
```

`at` and `up` put its top surface at home, `up` measured from the floor the
world's own spawn point stands on -- the same datum nav nodes and pickup pads
use. `path` is where it goes from there, as offsets in metres: it travels the
list in order and comes back the same way, so one entry is the usual lift and
several are a patrol. `trigger` says what sets it off:

- `stand`: a body on it. It climbs, waits `wait` seconds at the far end,
  comes back, and then stays home until somebody stands on it again.
- `near`: a body within `near_m`. A door, or a bridge that lays itself.
- `loop`: never stops.

`material` is what it is solid as, so a pan of slag carries its `hurt`
wherever it goes; rock when left out. `colour` is how it is drawn, because a
model is flat colours rather than textures -- the top face is drawn lighter
and the rim darker, which is what makes a plate read as a plate.

A platform glides: it is drawn and collided where it really is, not where the
grid would put it. Collision is still voxels, since that is what every body
in the game asks about, so the slab handed to the store is the box rounded
*inward* -- only the cells wholly inside the plate are solid. A platform is
then never solid anywhere it is not drawn, which is the error that matters:
an invisible lip you walk into, or a rider's feet swallowed by the floor they
are standing on. The other way round costs at most a quarter metre of plate
edge, and only in flight, because waypoints sit on the grid and a platform at
rest collides exactly as it is drawn.

Riders are carried, not pushed: whoever is standing on one is moved by the
plate's own movement and set down on its surface, their previous position
moved too so they are not drawn trailing behind. A sideways carry nudges
rather than places, so a platform cannot post anybody into a wall, and a body
on its way up is moved without being set down, so a lift does not undo a
jump. The player and the bots ride. Animals do not, and nor does anything a
mode moves itself, such as the bike.

A platform does not turn, and it is at most eight metres across in each
direction, because a model is a 32-voxel grid. The bots ride one they happen
to be standing on but do not route over it.

Jadewater has one: the Choir ferry, a
raft of coral lacquer that crosses the channel between the two islands under
the sea torii, five metres by three and a metre over the water. It is a
`loop`, so it runs whether anybody is aboard or not, which is what a ferry on
a route does.

The headless report has a `platforms` key -- where each one is, how far it
has travelled and how often it has been called -- because a lift that never
moved and a lift that worked perfectly look the same in a screenshot.

## Portals

A portal is a standing cylinder that does something to a body whose feet are
inside it: a teleporter that puts you somewhere else facing a new way, or a
pad that throws you. Both are the same shape of thing, so they are one list
(`src/portals.rs`):

```json
"portals": [
  {"name": "pit", "at": [110.0, 92.0], "up": 0.0, "to": "gallery", "both": true},
  {"name": "gallery", "at": [148.0, 60.0], "up": 11.0, "bearing": 180},
  {"name": "kick", "at": [96.0, 40.0], "up": 0.0, "push": [0, 6.7, 0]}
]
```

`at` and `up` place the foot of it, `up` measured from the same floor
everything else a world states a height for is measured from. `to` names the
portal a body is sent to. `bearing` is the portal's exit direction: the way
a body faces coming out of it, degrees clockwise from north, with its level
speed pointed the same way, so a teleporter you run into at full tilt puts you
out running. Leave it out and the portal faces arrivals the most open way, as
measured from the voxels round it, so nobody lands facing a wall. `push` is
metres a second: the vertical part replaces what the body had, which is what
makes a jump pad a jump pad, and the level part is added.

A portal that is only a destination needs no `to`. `both: true` on the sending
end makes the pair two-way, the far end sending back to it. A portal acts on
a body as it steps in, not while it stands there, so arriving on a two-way
pad does not bounce you back; you have to step off and on again. Bots learn a
way from the nav links, which the arena importer writes in both directions
for a two-way pair; a hand-edited world needs the return link as well. The
player and the bots both go through; the `portals` key in the headless report
counts how often each was used, since a teleporter nobody can reach looks
exactly like one that works.

## Worlds and the library

The game loads a world from `worlds/<name>/` (default `glassfall`; `--world
NAME` or L in the game picks another, `--world random` a noise-generated
island). It also looks in `local/worlds/<name>/`, which git ignores, for
worlds you keep to yourself; a name in both plays the one in `worlds/`. The
random island shows in the world list only when a `local/worlds/random/`
folder exists. A world that fails to build doesn't start: the game returns
to the title screen with a note, and the console has the reason (or run
`--world NAME --validate`).

A world is a short story (`story.md`), a chronicle of dated events
derived from it (`chronicle.json`) and a settings file (`world.json`). At
startup the events run oldest first as processes over 1 m world-wide maps
(`src/world/chronicle/`, one file per event kind in `events/`): landmass,
volcano, ash fall, impact, river and avulsion, deposits, settlements, single
structures, abandonment, fire, regions, paths, spires (sea stacks, hoodoos and
the like, one or a field of them), canyons, mesas (buttes, spires, fins),
routed lines and the settlements laid out on them (see below), and a
dirt-bike track with berms and jumps. Then each cell gets the highest-priority biome whose rules match.
Where two biomes meet, each one's ground reaches a few metres across the line
(its `fringe`, 3 m by default), so the edge breaks into interlocking patches
rather than a clean boundary and the grass growing on the ground fringes with
it; a biome with `"fringe": 0` keeps a hard edge and is never mixed into its
neighbours (a ploughed field, a burn scar, a paved square, a salt pan), and
shores and river banks never feather. `VOXEL_FRINGE=0` turns it off for
comparisons. Then plants and natural features are placed by the biomes' placement rules
(`src/world/flora.rs`), and the chunk generator reads the result: terrain,
buried layers, structures (rotated, decayed by age with a gravity pass so no
piece hangs in the air, mossed, buried by later silt) and trees.

Everything a world uses comes from the shared `library/`, one JSON file per
entry: materials with texture recipes, plants, structures written as shape
programs, creatures (a model program plus a behaviour), and biomes. Entries
are data that choose among behaviours written in Rust (texture patterns, tree
forms, placement rules, movement styles, processes). The worldgen skill in
`.claude/skills/worldgen/` is the procedure for making a world from a premise
with Claude Code, and its `reference.md` lists every format and behaviour.

```bash
cargo run --release -- --validate
cargo run --release -- --gazetteer
cargo run --release -- --map shots/map.png
```

`--validate` checks the library and the world; `--gazetteer` prints what each
event did, the points of interest, every built structure with its position,
peaks, a height table every 64 m and biome shares (with `VG_PROBE=x,z,step`
set it adds a close-up height grid round that point); `--map` draws biomes,
rivers, structures, buried items and the spawn from above.

Voxels are a byte, so only 238 library materials fit in one world. The
library as a whole may hold more: each world gets its own ids at load
(`Library::assign_ids`). The materials a world reaches (`worldload::reach`:
any name in its world.json and chronicle.json, then in the biomes, plants and
structures those name) keep the id their file asks for, materials without an
`id` take free ones, room is kept for the grain copies of the world's woods,
and whatever is left goes to the rest. Saves name materials, not ids, so this
is safe; the renderer re-uploads its material map when a world installs new
ids.

`library/README.md` is how to work in the library and `library/CATALOGUE.md`
(generated by `python tools/libfmt.py --catalogue`) lists every entry and the
worlds that use it. `python tools/libfmt.py` keeps the JSON in one readable
form.

### Routes, kits and settlements

A line through a world (a road, a lane, a fence, a wall, a boardwalk, an
elevated bridge, a bored passage) is a polyline plus a kit. The kit
(`library/kits/*.json`) holds the cross-section: how wide the surface is and
what it is made of, what rails run along it, what posts repeat and how often,
whether it rides on a deck, what piers hold that deck up, and what it hands
over to where the ground falls away. Lines are drawn per voxel column, so
they take any angle, any length and any height; a bridge is not a structure
cut to a fixed length. `docs/ROUTES.md` is the full account.

Four events lay them down.

```json
{"year": -300, "event": "route", "cause": "the trunk road north to the ferry",
 "from": "ferry", "to": "crossroads", "kit": "highway_guard",
 "terms": {"grade": 0.09}}
```

`route` searches for its own line: climbing past the kit's grade, open water,
a cliff and ground an earlier event took all cost it distance, so the road
finds the pass and the footpath takes the dry side without the chronicle
naming either. `terms.wade` is the deepest water it will walk through rather
than bridge: under that a crossing is a ford, which is how Thornpan's trails
get across a delta in flood. Where the land falls more than 3 m below the grade it builds
the kit's `span` across the hole and puts piers under it.

`network` routes a set of links together. Each line laid makes its own ground
dearer, so the next link joins it at a junction rather than running alongside;
links are laid widest first, and where two meet both are trimmed back and the
crossing is aproned. Links that cross away from a node are aproned too, and a
kit's `max_span` now holds over open water as well as over gaps.

`ribbon` takes the polyline as given (`closed` for a ring, `meander` for a
line that wanders, `height` to hold it at a fixed height above sea level) and
is what a fence, a wall, a skywalk or an adit uses.

`watercourse` is the same search with water's idea of cost: only climbing is
paid for, so the channel runs where water would. `via` points hold it to the
geography a world is built round, and `branches` leave it for their own
mouths, which is how a delta is written: Thornpan's Spread and Lowmouth's
passes are both one event. What it cuts is registered as a river, so the
water settles into it and an `avulsion` can still break it out.

`canopy_town` is a network in the trees: it grows giant host trees where
it puts them, rings each trunk with a platform, strings walkways between
the platforms (the shortest set that joins them all, then a few loops) and
winds stairs up a few trunks from the ground. `docs/ROUTES.md`, "In the
trees", has the rules.

```json
{"year": 1880, "event": "canopy_town", "cause": "the treehouse town",
 "at": "centre", "radius": 90, "hosts": ["giant_redwood", "giant_douglas"],
 "count": 26, "spacing": 22, "ring": "tree_ring", "rail": "tree_rail",
 "walk": "cedar_walk", "stair": "tree_stair", "stairs": 3}
```

`village` lays out streets and builds on their frontages: `grid`, `radial`,
`organic` or `linear`, turned by `bearing` (a quarter turn suits a grid,
whose buildings then sit square to their street). Plots come off the street
edge at `setback`, every `spacing` m, alternating sides, on a square of ground
`plot` m either side; each building faces the street it is on and gets a path
from its door to the kerb. A `linear` plan wants its `bearing` along whatever
it is strung out on: Thornpan's Sedge runs down the spine of a sand island,
and a few degrees out puts the houses in the water. The `settlement` event
scatters houses in a disc instead, all facing the middle.

### Names and signs

A world can name its ground and write on it (`src/world/places.rs`).

An `area` event names a stretch of land, either a ragged circle (`at` and
`radius`) or an outline (`poly`). Walk far enough into one and its name fades
up in the bottom corner of the screen, holds a few seconds and then settles to
a dim line that stays while you are in it; it fades away as you leave. How far
in is `inside`, a tenth of the way across by default, and the name holds on
that far past the edge, so walking the boundary doesn't blink it. Areas may
overlap and nest, and the smallest one you are properly inside is the one you
are in, so a green inside a village inside a valley names the green. Not every
part of a map needs a name.

```json
{"year": 0, "event": "area", "cause": "the settlers' own name for the place",
 "name": "Skystone Village", "kind": "village", "at": "village", "radius": 95}
```

A `sign` event puts a board with a line or two on it in the world. The style
(`library/signs/`) decides the board's size and colours, the lettering and the
structure that holds it up: `timber_sign` and `painted_board` on posts,
`carved_stone` cut into a waist-high stone, `street_blade` on a galvanised
pole, `highway_board` on two steel legs, `iron_plate` bolted to a post. The
board and its letters are drawn as plates in world space
(`src/render/plates.rs`), not as voxels, so a 20 cm street blade reads as
clearly as the type in the menus; only the post is voxels. Lettering is set in
the game's own face, shrinks to fit the board, and `--validate` says so when a
sign's words come out too small to read.

```json
{"year": -150, "event": "sign", "cause": "the stage the settlers built",
 "at": [818, 646], "text": ["SKYSTONE", "LANDING"], "style": "timber_sign",
 "bearing": 90, "age": 40}
```

`bearing` is the way the front of the board looks, which is the direction a
reader stands in; `face` aims it at a place instead. Both the areas and the
signs are listed by `--gazetteer`. A world whose people don't write (Windbarrow)
has names and no signs.

### Glassfall

The default world: an old volcanic island, 1 km across, where something fell
from the sky. A crater with a glass floor in the eastern hills and fragments of
skystone strewn north-east, buried, with odd moss over each; a strangeness
around them (spiral pale pines, glass shrubs and spikes, pale hares that don't
flee, violet moths) that fades with distance. A settlers' village beside a
river that a flood moved 90 years ago, now ruins with a dry mill and a silted
floor, a landing stage and cart track on the east shore, the diggers' shed on
the crater rim. Three smaller pale glades, each about the size of the tarn,
where outlying fragments landed: a collectors' shed in the north one, a
timbered adit into the fault scarp in the south one, a plank hut in the
south-east one. A small tarn fills the summit crater. A prairie with snags where the western forest burned 40 years
ago. An old fault scarp crosses the southern woods, and a lava tube with two
collapsed skylights runs under the cone's eastern flank. Small finds lie about
to pick up: seashells on the beaches, mushrooms and pine cones in the woods,
pottery shards and the odd brass coin around the village, skystone chips in
the bloom. Two things nobody explains: a stone doorway standing alone on the
prairie, and the surveyor's last camp on the north slope, his instrument aimed
out to sea.

The woods carry their dead and young: standing snags and broken ones, stumps,
fallen trunks with snapped limbs, mossy logs, maple and fir saplings, and
patches of huckleberry and thimbleberry under the ferns.

A kelp forest grows on the reef shelf 3 to 16 m down within 70 m of the
coast, kelp stipes from bed to surface with a floating canopy, herring, the
odd manta ray and sea turtle. Redwood groves stand in the lowland woods and on
the floodplain. Boar live in the woods, capybara on the river banks, feral
horses on the prairie, tortoises round the village, and sea turtles haul out
on the beaches. Herons walk, wade and fly off on slow wings. Harbour seals and
sea lions haul out on the coast (they dive and swim off when startled, and
climb back out), puffins and cormorants nest on the basalt, geese and mallards
keep to the shallows and the shore, woodpeckers to the woods, bees to the
fireweed and heather, dragonflies to the floodplain. Walkers go to the river
to drink. The story is in `worlds/glassfall/story.md`.

The warm reef (`coral_reef` at year -6000) runs along the north coast between the
volcano and the strewn field: the old volcano breathes warm water into the sea there, so coral
grows where the cold coast only grows kelp, and toward the strewn field it turns to glass
(`glass_reef`). So that it is deep enough to dive, the bed falls away from the shore to 4 m,
the crest is 4.5 m down, the drop-off 11 m and the wall's foot 23 m, a metre off the floor of
the world. The surveyor's launch lies on the reef and a trading ship below the wall. The kelp
forests have rockfish, kelp perch, sea otters, and cormorants and puffins that dive after fish.

### Redrock

`--world redrock`. A red-rock high desert (Utah, northern Arizona, a little of
north-east California) on an island in a falling reservoir, 1.5 km across. It
climbs north in a staircase: lake flats with banded clay badlands and
petrified logs, the sandy desert bench, the Red Cliffs, a pinyon-juniper
bench, the White Cliffs with a hoodoo amphitheatre cut into them, and a
plateau of ponderosa pine with grass parks, a burned strip gone to prairie,
and a black cinder cone with an ice-cave lava tube. The rock is laid in
formations (`strata`) whose bands pinch and swell across the map
(`strata_swell`), so every cliff, butte and canyon wall shows the same bands
at slightly different heights; the old water marks stay level
(`strata_level`). Canyon benches wander in height and width (canyon
`irregular`), and cliff tops are weathered (`erode_rims` with `joints`): a
hard lip notched along joints. Two reefs (`reef`), the Comb in the south-east
and a smaller one west of the Gathering, are tilted ridges where the beds bend
up along a fold, each with a few cracks through the crest that a walker can
climb. A coral-pink dune field (`dunes`, biome `coral_dunes`) covers the
south-west corner.

Sorrel Canyon cuts the whole country north to south, slotting through the Red
Cliffs and drowning at its mouth, with cottonwoods and a river on its floor
and the canyon people's granaries in an alcove in its west wall. A 1929 steel
arch bridge carries the highway over it. The highway runs along the foot of
the Red Cliffs a third of the way down the map, from a barricade at the lip of the
washout in the west, where a 1983 flood cut the road away, to a ferry landing
in the east; Pruitt's
filling station stands beside it (the spawn), with a dirt-bike course behind
(banked turns, rollers, a double and two tabletops). East of the canyon stand
a row of fins with a great window worn through one, Sentinel Butte and a
pair of spires, and round them arch country: more fins, each with an opening
carved through it (`window_*` structures placed across a `mesa` fin): the
Lone Arch, Needle Window, the Spectacles, low spans and high windows. In the south-west corner is Harlow, an 1880s silver camp abandoned in
1893: false-fronted main street, stone assay office, stamp mill and tailings,
a timber headframe and the uranium men's steel one, each over an open shaft, an adit into Harlow Mesa,
a cemetery. Bitterwater's stone chapel stands at the lake's edge again now
the water has dropped. Two things nobody explains: a level pale band at the
same height on every cliff high enough to carry it, and a ring of eleven
hoodoos on the open desert far from any cliff.

Prickly pear, cholla, agave, ocotillo and the odd saguaro grow on the desert
bench; some agaves and yuccas carry flower stalks (sprites that don't move in
the wind), and the junipers are low, twisted and many-trunked. The ponderosa
forest floor is brown needle duff with tan bunchgrass. Ocotillo flowers only in spring (`flower_seasons`), so in Redrock's
default summer it shows bare canes. Desert tortoises and javelina live on the
bench, condors fly over the cliffs, feral burros roam round Harlow, and
mustangs and sandhill cranes graze the high meadows. Geese, mallards and a
swan are on the reservoir, acorn woodpeckers and flickers in the pines,
dragonflies and meadowhawks in the canyon. The story is in
`worlds/redrock/story.md`; its chronicle is edited as JSON.

### Jadewater

`--world jadewater`. Tower karst after Guilin and Ha Long Bay, 1 km across,
built for gliding and climbing. The towers are volumes (`tower` event), so
they taper, bulge, lean and overhang, and end in domes, knobs, slopes and
points; their tops are forested and vine curtains hang from every lip and
ledge. In the north-west is the Old Wall: forty-odd limestone towers packed
close, 45-130 m high, the gaps between them narrow misty gorges full of
golden bamboo you push through (the culms are passable), with pandas in them.
South and east of it a strip of banyan jungle, then a shallow turquoise
lagoon (a 3 m shelf of white lime sand, `shelf`) with about a hundred free
towers standing in it. Between the towers the lime mud banked up until it
stood out of the water and grew over (`deposit` domes with a `depth`): low
green hills with white beaches, so the east and south of the world reads as a
bay with headlands and winding inlets rather than open sea round a field of
islands. The sheltered shallows along those shores are eelgrass meadows. The stone is banded (`strata`), mostly pale blue-grey,
with about a fifth dark slate grey, a fifth near white, and one tower in
twelve wrapped entirely in vines. Banyans, oaks and small wind-twisted pines
grow on top and banyans out of the ledges. The lagoon towers stand on sand
beaches and bars with fallen blocks round their feet, and green turtles swim
over the sand. The lagoon is jade-turquoise and the cenotes a deeper green
(world and biome `water` colours). Giant bamboo and small broadleaf trees
fill the wetter ground east of the Wall, over long green grass.

The Lantern Well is an 80 m sinkhole in the biggest block of the Wall, flooded
at the bottom, with giant lily pads you can stand on and an undercut overhang
at the waterline whose ceiling glows cyan. A lit passage runs from it out to
the gorges (spawn-side entrance near 256,280). Four smaller cenotes sit in
limestone humps in the jungle, each with lilies and a lit overhang. More
passages go straight through Wall blocks and lagoon towers, half the big
lagoon towers have a sea cave cut through at the waterline, and the Old Wall
has caverns (big chambers, chimneys to the tops, passages between them, lit
near the Well).

The Order of the Lantern's temple (hall, pagoda, bell house, glowing stone
lanterns, rows of little statues) is in the jungle south of the Wall, reached
up a torii avenue from the spawn. Their coral-red rope bridges, the Red Way,
cross three gorges of the Wall at its top and run out over the lagoon on six
towers levelled to one height (start near 560,760). About two dozen more rope
bridges hang between lagoon towers at all heights, each landing in a tunnel
that crosses the tower to a doorway on its far side. The Order also left a
hall and lanterns on one levelled Wall top, a pagoda, bell house and shrine
on others, two great torii standing in the sea, and ruined shrines, stone
stupas, pavilions and lanterns through the jungle, beaches and tower tops.
Kite rays, jade-backed and spotted with cyan light, fly round the towers;
serow live on the ledges, boar and deer in the jungle, mantas and turtles in
the lagoon. Three things nobody explains: the cave light, which lays down
stone; the rays that swim in air; and the Choir, seven identical towers in a
ring round a blue hole east of the temple (835,790).

The story is in `worlds/jadewater/story.md`; its chronicle is edited as
JSON.

Two reefs sit at the end of `chronicle.json`: the atoll off the spawn beach in the south-west (144 m across, four motu, one pass) and the Deep Eye, a `cenote` in a karst hill in the
bamboo country at (560, 190) with its water at 41.8 m and 29 m of it, coral on its floor.

### Hemlow

`--world hemlow`. A Cotswold wool village on a midsummer evening, 1 km
across. It is the only world that sets `start_hour` (17.6), so it opens in low
gold light. The land is honey oolite (`strata`), ending in the east in Hemlow
Edge, a 26 m scarp with combes cut back into it and a beech hanger on its face.
The Hem rises in Hemmere, a shallow peat-bedded lake with two islets in the
west, ringed by the Wisp Moss (peat pools, reedmace, willow scrub, dead
alders, heather), and runs east down a soft grassy valley (`vale`) past
Hemlow Knap, over the Edge at Lantern Spout into a plunge pool, and across the
vale to the sea. You can walk in behind the fall (the curtain is passable)
into Lantern Hole, a branching cave of dark wet rock running back under the
wood: patches of glow-worms on the roof, two side branches each ending in a
chamber (the north one, the Lantern Room, toward the Hob Stones), and now
and then stalactites, stalagmites and a column.

The village is thirteen buildings of honey stone on the top of the Knap: a
wool church with a battlemented tower, the Hob and Lantern inn, a tithe barn
with two cart porches, a dovecote, a terrace of three cottages, houses and
cottages under stone slates or thatch round a green with a market cross, a
cobbled lane down to the mill and a three-arched packhorse bridge. Every
window has a lamp in it (`lamplit_glass` glows). The Knap and the valley are
hay meadow (`wildflower_grass`, which grows flowers thicker than prairie
grass) with dry-stone walls, elders and roses. Everything else is the
Hollins, an oak and beech wood that turns faerie toward the Hob Stones, a
ring of standing stones in a clearing to the north (470,210): gold maples,
wisteria trees, blossom vines, pale birch, glassbloom, mossback deer, faerie
horses (lilac here), moss bears, sky rays and a few giant firs. Trails lead
to a woodcutter's hut with its log pile, a wise woman's cottage with bee
skeps and herb beds, a smouldering charcoal clamp, and an empty cottage gone
to ruin. Blinking fireflies, lantern moths, barn and tawny owls (and, rarely,
a great horned owl, which also turns up in Glassfall's and Redrock's
forests), hedgehogs, kingfishers on the Hem, and marsh lights pulsing over
the bog after sunset. Hag stones (pebbles with a hole) turn up along the river and round
the stones.

The story is in `worlds/hemlow/story.md`; its library entries are
edited as JSON.

### Lowmouth

`--world lowmouth`. A bayou delta 1.4 km across where the river changed course
a century ago and the old lobe sank. About half the map is under water, most
of it a metre or so deep: you wade the sawgrass and paddle everything else.
The land is a delta island in a shallow sound (`landmass` with a 1.4 m
`shelf`), cut by the Marrow's old course (now a silted channel, by `avulsion`)
and its new one to the south-east, with four areas let down under the sea by
`subside` (the bay over the old parish, the cypress swamp, the sawgrass flats,
the mangrove mouth) and three bayous joining them (`bayou`).

Places: Theriot's landing on the new river (the spawn), the shell levee road
past the sugar house ruin and the pump house to the crossroads, the cane and
the burned cane field north of it; Lowmouth, a stilt town where the old
channel leaves the swamp (about forty houses and camps on posts along a grid
of boardwalks, the Big Bridge, the north bridge and a little bridge over the
channel, net sheds, houseboats), every boardwalk high enough to paddle under;
the back swamp of bald cypress and tupelo standing in tea-coloured water with
knees, Spanish moss and hyacinth mats; the ghost forest of dead cypress on the
bay's north shore; St. Anne's tower, fifteen sunken houses and tombs in the bay; the
trembling prairie (Hemlow's Wisp Moss, marsh lights and all) between the
rivers; the sawgrass flats with palm and oak hammocks and the grandmother
bottle tree; the red mangroves of the new mouth on their prop roots
(`mangrove` form) with the sunk shrimp boat; and the chenier on the south
point with live oaks, the parish tombs and the light.

More places: Mire, a cane town of two dozen board cottages, a store and a
filling station in the middle of the cane fields with dirt trails out through
the cane; dirt trails linking the sugar house, the landing, the mound grounds,
the woods east of the river, the east hammocks and the chenier; the sugar house's brick quarters, cistern and lone walls; the Bird Mound,
a 12 m effigy mound with spread wings on the central plain (`earthwork`), and
the low half rings east of it; Mother Ruth's camp deep in the swamp among
bottle trees; Fort St. Jean, a brick half-moon fort on the west shore; the
Bon Temps oil platform out in the Gulf; duck blinds in the sawgrass; a sunk
cane barge on the new river; fishing camps on the mangrove keys; and the
empty Broussard place on the chenier. Sawgrass, cattails, palmetto and sugar
cane are sprites (no collision, so the kayak slides through the reeds). The
sheltered shallows are eelgrass meadows, grass growing under the water the way
it grows over it (material `underwater`), and lily pads lie in patches on the
water over them and through the swamp, the drowned parish and the ghost forest
(material `floating`).

Animals: alligators in the fresh water and crocodiles in the brackish mouth,
basking on the banks (`haulout`) and lying awash in the water (`school` with
`awash` and `shallows`), and very rarely Old Pale, a white alligator half
again as long; great egrets, roseate spoonbills, anhingas, brown pelicans,
herons, cranes, nutria, deer and hogs, coyotes and, very rarely, a bear,
catfish and gar in the channels.

Its strangeness keeps to the crossroads and the drowned parish, and comes and
goes with `place` `when`: a juke joint stands on the empty corner lot from
midnight to three, chalk and candles appear at the crossing after dark, a
tall man in a hat stands there after dark but only while you are more than
70 m away, the drowned houses show lamplight at night, and the steamer lost in
the flood is out on the bay at night, seen only from more than 150 m.

The world sets `water` `still` (no shore foam, a dim reflection, murky), and
its materials leave out `id`. The story is in `worlds/lowmouth/story.md`;
its chronicle is edited as JSON, and `make_spurs.py` rebuilds the stilt
houses' walks to the boardwalks in place.

### Windbarrow

Neolithic chalk downland under a golden feathergrass, 1.5 km across and with
no coast: the landmass radius is far larger than the world and a world-wide
`deposit` dome lifts the whole block clear of the sea, so every cell is land
and the only water is one river and one salt lake. The Narrow rises in the
north, runs south-west down a wide shallow vale and ends in Saltmere, a lake
with no outlet ringed in salt crust. Down the east runs the Barrow Edge, a
20 m chalk scarp broken by nine dry combes so it reads as bays and noses
rather than one wall; two bands of flint make a level line along its face.
East of it is the Flint Down, thin grass over rubble, with the diggers' open
pits (short shallow `canyon`s) and a summer camp.

The people are there and the fires are lit: Durn, eleven round houses of
limewashed daub under deep conical straw roofs with a longhouse, granaries on
posts, hazel stock pens and drying racks, on the bank of the meandering Narrow, with
one enormous oak (`mother_oak`) on the rise above it; Crest,
seven houses in the lee of a ridge; barley plots behind hurdles. Trails a
foot and a half wide run between them along the ridges, with posts carrying a
horse skull where they part. Twenty-two kurgans (`deposit` domes) stand in three
loose knots along the barrow ridge with chalk kerbs and ochred oak posts, the
largest 34 m across;
the Long Barrow (an `earthwork` ridge) is older and open, its closing slab
pushed aside. On a hill west of them the Tepe: twelve carved T-pillars and two
taller ones in a chalk bench wall, raised by people the villages cannot
account for.

Trees are scarce: willow and alder along the river, small stands of birch and
alder in the damp valley bottoms, and single hawthorns and junipers elsewhere.
Exactly one great oak grows, planted by giving it a 9 m `region` of its own and
a `lone` spacing wider than that region.

Five more places hold the quarters the villages do not use: a frost hollow in
the northern downs where the grass is rimed white at any hour, the Bone Downs
east of the flint pits where the chalk gives mammoth bone instead of flint, the
Skate Pillar standing alone on the northern crest (the tallest of the Tepe's
stones, carved with a skate), a turf maze 84 m across cut to the white chalk on
the empty southern downs, and the bone house at the summer camp: eight skulls
for a footing, ribs for rafters, hides over them and five pairs of tusks arching
clear of the roof. A trail runs out to each, and each has a post to find it by.

Animals: Przewalski's horses, saiga, mammoth and wolf on the open grass, a
sabretooth now and then, eagles over the scarp, marmots in the rubble, cave
bears at the mouths of the caves, two herds of aurochs in the valley bottoms and
nowhere else, and wind skates, straw-pale rays that ride the wind over the awns
(`undulate`), with a sky manta ten times the size passing high over now and
then. After dark, corpse candles drift between the barrows on the `drift`
movement (a slower `flutter`).

Three things it does not account for: air comes steadily out of the Long
Barrow's mouth in all weathers and the grass round the forecourt is combed
flat; one kurgan is green when the field is gold and has nothing under it; the
cat and the bear carved on the Tepe's central pillars are animals nobody has
seen, and the same long-toothed cat is scratched, much more recently, on a
block at the mouth of the Gape.

A dry cave system in the chalk runs east from the head of the Gape with two
mouths and four daylight shafts; `cave_chalk` carries a low `glow` so the
passages are walkable without a light. The bone beds and the scratched block
stand at its mouths.

The story is in `worlds/windbarrow/story.md`; its chronicle is edited as
JSON.

### Thornpan

An inland delta in dry country, 1.5 km across: clear floodwater fanning out of
a rift trough into golden savanna, bleached salt pans and granite kopjes. The
engine has one water level, so here the sea **is** the flood: sea level 20 m,
`shelf` 2, the savanna rolling between 30 and 70 m, and the water pale and
moving rather than Lowmouth's still murk. Four fifths of the water in the world
is 35 cm deep, which is the whole look of the place -- a sheet you wade and see
the sand through.

The Thorn comes in at the north-east corner, runs south-west down a `vale` in
the trough between two short facing `escarpment`s (the Narrows), and ends in
the Spread: a `subside` that lets 585,000 m2 down to within half a metre of the
water with half of it standing as sand islands, cut by an `avulsion` and three
`bayou` channels and holding two pools deep enough for the hippo. Nothing
drains out of the world: the channels stop in the fan. South-west of it the
White Pans are the floor of a lake that is long gone -- a second `subside`,
then a `deposit` dome of salt that builds back up out of the water so you walk
on the pan with the sheet only at its rim.

The seven Stacks are granite `deposit` domes 12 to 26 m high with
`granite_stack` boulders scattered round and on them, and they are what the
long view stacks against. Lamp Kopje, the tallest, carries the Lamp: a ruined
round tower facing out over the pans.

The strangeness is one idea in three places, and the `strata` carry it: a 1.1 m
band of `barnacle_crust` at 45.4 m runs across every rock face in the country,
twenty-five metres over the highest the flood has ever reached. There are four
sea-going hulls (`wreck_ribs`, `careened_hull`) in the reeds and on the salt,
the Lamp was built to light a coast, and at the hour before sunset two more
hulls stand in open water on the pans -- `place` with
`when: {"far": 320, "hours": [16.4, 19]}`, so they are there until you walk at
them.

Three villages of daub and thatch: Sedge on the biggest island in the fan with
its mokoro drawn up and a flood post at the landing, Longwater upstream on the
open bank with the cattle, and Barnacle Camp beside the broken hull. The
baobabs are buildings as much as trees -- `hollow_baobab` (the council tree,
benches and a hearth inside), `baobab_cistern` (sealed with clay, kept full of
water, pegs up the outside) and `baobab_tomb` (closed with a slab) -- and the
Ringstone, a dry-stone enclosure on Lamp Kopje, is the work of people who were
gone before the present ones came. The carmine bee-eaters nest in a cut bank
(`bee_bank`) on a bend of the upper river.

The upper river is the world's one hard contrast: closed gallery forest of figs
and vines on the shaded north-west bank (region `green_bank`), open meadow on
the other, forty metres of water between them.

Its own library entries: materials `savanna_grass`, `granite`, `baobab_bark`,
`acacia_leaf`, `barnacle_crust`; plants `baobab`, `dead_baobab`, `acacia`,
`mopane`; creatures `hippo`, `giraffe`, `lechwe`, `wildebeest`, `warthog`,
`bee_eater`, `vulture`; structures `hollow_baobab`,
`baobab_cistern`, `baobab_tomb`, `granite_stack`, `termite_mound`, `mokoro`,
`flood_post`, `bee_bank`, `wreck_ribs`, `careened_hull`, `lamp_tower`,
`ring_wall`; biomes `golden_savanna`, `flood_meadow`, `palm_island`,
`kopje_rock`, `gallery_forest`, `pan_floor`, `delta_shallows`. It places
elephant, lion, zebra and water buffalo, and the herds are groups of up to 30.

The kopjes' bare rock is asked for by name, not by slope: the seven `region`
events all carry the name `stacks` (one mask, since `region_id` reuses the id
for a name), and `kopje_rock` has `"region": "stacks"` in its rules. Chosen by
slope alone, half the country would be granite.

Two limits of the shape ops: the `disc` op only draws in
the x and z axes (a `"y"` disc comes out as a one-voxel slab standing on edge),
and `log` is horizontal only, so a vertical trunk is a `cylinder` (a `yaw`
turns a log or a disc to any bearing in the ground plane). Anything
thin at an angle (a leaning mast, rigging, a brace, a spoke) is a `segment`:
the mesher draws it as a smooth tube along its line (`src/mesh/lean.rs`). A
flute cut into a hollow trunk goes through to the hollow and opens a window in
the back of the tree.

The story is in `worlds/thornpan/story.md`; its chronicle is edited as
JSON.

### Ribshore

A grey northern coast, 1.5 km across, with the skeleton of a giant lying along
its beach: a toothed animal like a whale, about 1.4 km from the tip of its jaw
to its tail. Sea on the west, a wide wet beach, then half a kilometre of
rhythmic dune ridges held by dunegrass, purple lupine and sea buckthorn, and
Sitka spruce at the east edge. A basalt headland with a lighthouse stands at
the north-west, with sea stacks off it and bull kelp on the reefs. The grade
is cool, grey-green and high in contrast; the climate leans on fog, and a new
game opens on a foggy autumn morning.

The skeleton is the `skeleton` event (`world/chronicle/events/skeleton.rs`),
which draws bones as volumes in `world/bones.rs` rather than as a structure:
a structure is a dense voxel grid that only turns in quarter turns, and one
rib alone would be about twenty million voxels. Each bone is a short list of
tapered capsules, flattened or cut where the bone is, with carves for the
inside of the skull, its eye sockets and blowhole. The chunk generator asks
per voxel whether a point is inside one (`Bones::draw`, after the routed
lines and before the trees), so a skeleton draws at every level of detail and
a buried bone is found by digging. The animal lies on its belly: the vertebrae
are worn flat on top and half sunk, with sand drifted level into the gaps so
the spine can be walked or ridden end to end; the ribs stand up from both
sides like tusks, each with its own height, reach, lean and wobble, a third of
them snapped off with the piece lying beside; the skull sits upright in the
surf with its jaws open and teeth in both. Broken ends show an ochre
honeycomb of marrow, bone open to the sky grows lichen, and bone near the
waterline is crusted. The same event makes a baleen whale under the headland
(plates standing in rows on an upturned snout) and a calf in the dunes.

The rest is ordinary coast built from `terrain`, `subside` (the shelf off the
coast, and a deeper hollow off the head where loose bones and teeth lie),
two `dunes` fields piled by the north-west wind, `drainage` and a
`watercourse` for the creek, `mesa`s for the headland and the stacks, and
blowout bowls let down into the dunes. Smaller sites: the whalers' try-works
at the creek mouth (1841-46), fallen teeth standing in the wet sand, drowned
spruce stumps among the seaward ribs, driftwood at the dune foot, a cabin and
a fallen giant spruce at the forest edge, and the empty lighthouse. The
driftwood keeps to the wrack's landward edge and the first metres of grass
(edge placement with `field`), lying along the shore at any bearing.

At night a faint amber light fills the titan's landward eye socket (`titan_eye`, placed with `when` `moon`, so it is there only while the moon is out; the skeleton event names each socket `<name>_eye_left` and `_eye_right` and prints its height). An old stand of giant fir and redwood stands inland in the spruce (region
`firstand`, biome `rib_oldgrowth`, round 1460,800).

Bald eagles (the library `eagle`, which hunts in every world it lives in) circle the headland, the
wrack and the spruce, sit on the tallest spruce, firs, redwoods and drowned snags, and stoop
on gulls, puffins and fish near the surface (CREATURES.md, `soar`).

New creatures: walrus, albatross (the `glide` flock verb: long low circles,
roosting together on high ground near the coast), black oystercatcher, the
bone crab (rig plan `crab`, which walks sideways), the giant octopus (verb
`creep`, rig plan `octopus`: its arms are serpent paths laid over rocks and
bones, and it pales toward sand as you close in), mola and sixgill shark. New
plants: sitka spruce, shore pine, sea buckthorn and lupine (a sprite layer of
its own). Anomalies: a harpoon twenty metres long stuck in a vertebra near
the hips, there before the whalers came, and a blowhole that spouts on a
rhythm whatever the sea is doing. Its chronicle is edited as JSON.

### Gannet Harbor

The coast north of Ribshore, 1.5 km across, built to meet Ribshore's north
edge: its world.json names `"neighbours": {"south": {"world": "ribshore"}}`,
and the build takes a strip off Ribshore's edge and blends its own land, coast
distances and trees into it (`world/seam.rs`; `notes/UBERWORLD_PLAN.md`). Its
south edge therefore runs on into Ribshore's beach, dunes and spruce, and the
two will walk as one once worlds can be joined at runtime. It shares
Ribshore's sea level, tide, surf, sky and weather; its grade is Ribshore's
made a little greener and brighter, to suit the pasture and redwoods. `--seam out.png`
prints how each edge meets and writes a map of the world beside its
neighbours.

The coast is let into a broad shallow bight, Mercy Bay, with clam flats, salt
marsh and an eelgrass floor. A New England whaling town stands on its east
shore, kept and empty: streets laid out by `village`, timber wharves
(the `wharf` kit), two whalers at the wharves, one at anchor and one
careened on the marine railway, and a granite breakwater (an `earthwork`
laid in granite with `mat`, capped by the `breakwater` kit) with the harbour
light at its end. Gannets circle over the bay and plunge into it after fish
(`plunge` on the `circle` mover, CREATURES.md). Behind it walled
fields (Voronoi cells of the farmland, each a `region` and a closed
`fieldstone_wall` ribbon) with farmsteads, sheds and two orchards; a lane
north over the fields to the redwoods and a branch to Shad Landing, a hamlet
at the mouth of the Cold River. The river is `drainage` with two inflows over
the north and east edges and a `watercourse` for each branch. Coast redwood
along the north edge, two old groves, and the edge of a faerie wood in the
north-east corner (`gh_faewood`, like Hemlow's Hollins), for the world that will
sit north of it. A baleen whale like Ribshore's lies half in the dunes and
half in the spruce south of the bay.

Offshore, Hake Island is a granite block with tiered cliffs on its seaward
side (`mesa` with `tiers` and `dip`), a puffin colony on the cliff tops and an
abandoned 1950s-70s marine station on the low east side. Hogback Island has a
sea cave in its south side, eight metres down in the kelp, that rises into a
chamber under the island (four `passage`s: the tunnel below sea level, a pool,
a sand slope and a dry back lit by two shafts) where the whaler *Thankful*
lies half buried. Anomalies: the town's people are gone and something keeps
it; the station's last hydrophone tapes keep time with Ribshore's blowhole.

New entries: fourteen `gh_` biomes, the `fieldstone_wall` kit, the `whaler` in three states
(moored, careened on a marine railway, buried), small craft, the New England houses and
waterfront buildings, the rope walk, a covered bridge, the station ruins, and the gannet,
salmon, lobster, banana slug, razorbill, apple, lowbush blueberry and cordgrass
(`notes/GANNET_HARBOR.md` lists them all).

### Saltmirror

A salt pan 1.5 km across, flat to the horizon, with the Saltmirror Array on
it: eleven radio dishes built larger than life, a hundred metres across, on
three dirt-road arms from a hub, the pads further apart the further out. The
array was built 1964-71 and abandoned ten years ago, three weeks after
something came down over the range at night. A grey quartzite range with
fynbos (heath, restio, protea, pincushion, silver trees in the ravines) fills
the north-east corner, white gypsum dunes are piled against its foot, and
gravel fans below it carry cardon, saguaro, ocotillo and cholla. The pan has
two waters: the Brine in the south, a few centimetres deep over pink mud,
which lies like a mirror of the sky, with flamingos on it; and the Turquoise
in the north-west, a clear blue-green lake ringed with brine eyes and
orange microbial mats. Three quartzite rock islands carry the cacti.
Elephant-sized salt tortoises cross the open salt.

Places: the control building at the hub, the transporter stopped halfway
out the east arm with a dish on its back, the staff camp on the fan
(A-frames, prefab bungalows, the mess hall with its planet mural, the water
tower, pickups in the motor pool), Varga Dome on the western summit, and the
last pickup at the end of the fire road. Behind the eastern ridge, in a
hollow you cannot see from the pan, is the wreck: a dark lifting body broken
in two in a ten-year-old burn, its pieces and hull shards (a collectible)
strewn down the slope. The anomaly is pad 12, at the end of the short arm:
from more than 420 m away between 10:30 and 15:30 a dish stands on it, and up
close there is only the pad (a `place` with `when`).

Its weather was written for it: after a shower the salt floods into one
sheet that mirrors the sky and the dishes (`floods`), dry storms come in with
silent lightning that shows overhead and again in the wet pan (`storm`), and
on hot days the far flat turns to a mirage (`mirage`). See "Sheet floods,
reflections and the mirage" under the weather.

The dishes use the `dish` shape op (a paraboloid shell at any tilt), added
for this world. Its chronicle is edited as JSON.

### Slagdeck

The shooter map, a world that is a map rather than a place to walk in. A
smelter on a granite shelf off a cold coast: one building on a five-metre
plinth, laid out as a classic arena read as a foundry. A hall open to
the sky round a slag pit four metres down; a roofed shop either side with a
gallery over it; a walkway ring at nine metres joining the two galleries; a
platform over the north walk, reached by two ramps, with the whole hall under
it; and corridors behind the hall so the deck is a loop with no dead end on
it. All ten weapons, nine spawn points and two bots.

The whole deck is under one roof, with five monitor slots down the hall
letting shafts of daylight in between the trusses, and 44 lamps light the
rest (see "Lamps" above). The two shops are lit from under their galleries
and through windows in the outer wall; the corridors are lit only by their
own bulkheads.

It allows `walk`, `shooter` and `screenshot` and nothing else -- no bike, no
glider, no camera, no animals except a few squirrels on the ballast outside.
Nothing decays (`weathering: 0`), because a map that changes is not a map.

The whole arena is one structure, `library/structures/slagdeck_arena.json`,
written by `worlds/slagdeck/make_arena.py` from a description in metres. The
same script writes the `lamp` events into `chronicle.json`, so the lights and
the fittings they hang from cannot drift apart. Edit the script, never either
output. Its ramps rise a quarter-metre per metre of run,
which is the grade the mesher draws as one clean slope ("Slopes" above); at
any other grade they come out as visible steps.

Its own `look.json` pulls most of the colour out and flattens the sun, so the
only warm thing in the view is the slag in the pit -- and a tracer.

## Testing without a window

Two things to know when writing tests:

- **A `Store` fixture has to insert the chunks it writes into.** `Store::set`
  outside an inserted chunk is silently dropped, and everything outside the
  store reads as *solid*, so a body placed out there is inside implicit rock
  and cannot move in any direction. The symptom is a body whose velocity is
  **exactly** zero while it presses forward on what looks like open ground; a
  collision does not look like that.
- **Tests share a process, and so share the material table.** `matlib`
  installs one global `TABLE` at world load, so once any test has loaded the
  real library, a fixture writing raw material ids reads whatever `hurt`,
  `glow` or drift that library happens to have at those ids. Test against a
  closure rather than the table where the shape of the search is what you are
  checking (`terrain::worst_under` does).

Every part of the engine can be checked offscreen:

```bash
cargo run --release -- --headless --screenshot shot.png --stats stats.json
```

- `--pos X,Y,Z --look YAW,PITCH` puts a flying camera anywhere; `--ground X,Z`
  stands on the ground instead. Yaw 0 looks north (-Z), -90 looks east.
- `--dig N`, `--place N` and `--showcase` exercise editing and the creature
  models before the screenshot.
- `--sound-bake FILE --secs N` renders what is heard where the camera
  stands into a WAV (see "Sound").
- `--anim-sheet NAME:moves` on a snake shows its crawl at four phases of the
  wave, a turn and a swim below, and stretched out, coiled, coiled with its
  head up, climbing, draped and limp above.
- `--anim-sheet NAME` floats two rows of a rigged creature above eye height,
  each column with one bone bent (lower row pitch, upper row yaw, rest first),
  and prints which bone each column bends. `--anim-sheet NAME:moves` shows the
  gaits at four phases and every posture instead (for birds, the wingbeat at
  eight phases and the glide, soar, bound, bank, flare and perched poses),
  `NAME:family` the creature beside its subspecies and morphs, `NAME:herd`
  twenty individuals of it. See [docs/ANIMATION.md](docs/ANIMATION.md) and
  [docs/CREATURES.md](docs/CREATURES.md).
- `VG_SPAWN=name,name` (an environment variable) puts one of each named
  creature (a group for flocks and schools) about `VG_SPAWN_DIST` m (default
  4) from the player when the world loads, or the nearest spot further out
  that suits it (water for fowl, a trunk for a woodpecker, flowers for a bee),
  to watch behaviour that needs the player close: a tortoise hiding in its
  shell, a heron taking off, geese flushing. It prints where each landed.
  Headless `--stats` counts them in `creature_pathing` (`hiding_steps`,
  `flights`, `flying_steps`, `dives`, `drinking_steps`). `--hour H` sets the
  clock, which drinking follows.
- `--bench` flies a fixed 30-second route with streaming live and reports
  frame time percentiles. It works in the window too.
- `--map map.png` writes a top-down map of the world; `--profile-gen` times
  generation and meshing on one thread.
- `--ride bike|wing|launch|rope|climb` rides for `--ride-secs` (default 4)
  before the screenshot and prints mode, speed and position every half second:
  the bike (holding W), gliding from `--pos`, a glide launched from the
  ground, the rope thrown at the view and climbed (holding W), or walking (W)
  into whatever is ahead and climbing it. `--chase` uses the chase camera,
  `--boost` holds Shift and `--tuck` holds Ctrl, so a bike ride down a hill
  stores its charge and the jump at the bottom is the floated one; the
  half-second lines show TUCK and the percentage as it fills. The Chimneys hoodoos in Glassfall are good for
  climbing and rope tests: `--ground 296.5,493 --look -90,5 --ride climb`, or
  `--ground 300,495 --look 0,12 --ride rope --chase`.
- `--screenshot-mode` enters screenshot mode before the frames and saves an
  image through the same capture as the window into `screenshots/`; with
  `--screenshot` you get the view with the overlay as well.
- `--camera` takes the camera out with the viewfinder up (`--camera-out`:
  down), `--lens portrait,12,2.8,85` sets the frame, focus, f-number and focal
  length, and `--shoot FILE` takes a photo after the frames, writes it to FILE
  and prints everything the photo measured and how it did against each task.
  It goes into the scrapbook only when `--save` names a folder other than
  `saves`; `--keep` keeps a photo that did no task too. `--book N` shows the
  logbook (0) or scrapbook page N. To put an animal in front of the lens use
  `VG_SPAWN=deer VG_SPAWN_DIST=10` and look toward it. The stats JSON lists
  the current tasks under `camera_tasks`.
- `--frames N` sets how many frames a headless run steps before the
  screenshot (default 30), and `--wait S` the most seconds it waits for the
  view to stream in before giving up and shooting anyway.
- `--tour S` walks a loop round the island for S seconds and reports every
  creature met, by species.
- `--hud` shots take `--show-help` (the key help over the view) and
  `--show-worlds` (the OPEN WORLD screen for the current world).
- The bot and weapon benches: `--duel N` (with `--duel-seconds S` and
  `--duel-weapon NAME` for one gun's row) is the weapon bench under
  "Shooters"; `--tourney N` (with `--tourney-seconds S`) the movement
  tournament and `--league N` (with `--league-weapon NAME|all`) the league,
  both in `docs/BOTS.md`. `--no-player` takes the player out of a
  shooter match, so a headless match measures the bots against each other.
- `tools/timing.sh` wraps a headless run and prints a one-line summary.
- `cargo test --release` runs the unit tests (noise, generator, mesher,
  physics, allocator, riding, gliding, climbing and the rope).

Environment switches that the sections above do not mention, all for
comparisons and debugging:

- `VOXEL_GRASS_FILL=0` drops the near grass fill, `VOXEL_SPRITE_RANGES=0`
  draws every sprite at every distance, `VOXEL_TUBES=0` turns off stem
  bevelling, and `VOXEL_UNDERSTORY=0` the grass understory in `scene.wgsl`.
- `VOXEL_TIMING=1` prints how long the horizon map took to bake.
- `VOXEL_TEX_SHEET=out.png` writes the material texture sheet as it is
  built.
- `VG_SPAWN_WET=1` lets `VG_SPAWN` set walkers down in rivers, to test
  swimming; `VG_CATCH_M=12` lets a hunter take prey across a clearing and
  prints each kill.
- `VG_CATALOG_DEBUG=1` prints how `--catalog` framed each subject.
- `VG_EDIT_POSE`, `VG_EDIT_POSE2`, `VG_EDIT_ORBIT`, `VG_EDIT_SECS`,
  `VG_EDIT_PAINT` and `VG_EDIT_STEP` drive the EDIT page in a headless
  `--edit` shot (`src/tools/headless.rs` reads them).
- `SPRITES_OUT=dir` and `TEXOPTS_OUT=dir` point the two ignored export tests
  (`cargo test -- --ignored export_sprites` and `export_texture_options`) at
  a folder.

### The catalogue

`--catalog DIR` renders everything one world reaches into a review page:

```bash
cargo run --release -- --world glassfall --catalog catalog/glassfall
```

Each biome gets a ground view along its longest run inside and an aerial;
each plant the roomiest specimen the world placed, close and with its
neighbours; each creature one in its habitat (under the water for fish),
one side-on against the sky, and its ages in a row; each find where it lies and against the sky.
Cameras test their line of sight against the voxels and turn until it is
clear. A run of Glassfall takes about ten minutes; `--match aspen,deer`
re-renders only those entries and keeps the rest, and `--only plants,creatures`
limits the kinds. `--hour` sets the light (default 11).

`catalog/` is gitignored. Open `DIR/index.html` straight from disk. Every
entry shows its pictures, what the engine made of it (how many were placed,
movement verb and mover, rig, size, biomes, which ages it comes in), its
library settings as chips
with colour swatches, the full entry folded underneath, and its spot on the
world map. Keys: `j`/`k` move, `1` good, `2` needs work, `3` bad, `n` writes
a note, `v` shows the pictures large. Ratings live in the browser, keyed by
world and entry, and survive a re-render (an entry rendered since its note is
marked). "Copy feedback" puts the rated entries on the clipboard as markdown,
with each entry's library file, ready to paste.

## How it works

- World (`src/world`): the chronicle, fields, biomes and flora described
  above, the `WorldSpec` file and noise for the random island, the chunk
  generator and the full-detail voxel store. Every generator function is pure,
  so any column can be generated on any thread, at any level of detail, in any
  order.
- Streaming (`src/stream.rs`): a quadtree picks a level of detail for each
  column of chunks. Full detail reaches the view distance (`--view`, VIEW
  DISTANCE in SETTINGS, default 128 m) but never past 128 m
  (`stream::FULL_DETAIL_M`, `--full-detail` to compare). The first coarse
  level reaches twice the view distance and each level after that twice
  again, so a 256 m view keeps its far forest and its far cover where they
  were and draws 128-256 m at half resolution. Worker threads (cores minus four, at most eight,
  `--threads`) generate and mesh columns nearest-first, below normal
  priority. A column that is no longer wanted keeps drawing until its
  replacement has loaded, so there are no holes and no overlap.
- Meshing (`src/mesh.rs`, `src/mesh/`): greedy meshing with ambient occlusion
  into 16-byte quads, grouped by face direction. With slopes on, each ground
  corner moves to the mean height of the ground surfaces in the 4x4 columns
  around it (within four levels), and side faces move their edges to match;
  the module comment explains the rules that keep the surface closed. The
  third word holds the corner offsets and the neighbouring ground material
  with its share at each corner. A test meshes generated chunks and checks
  that every edge is shared. Merged quads meet smaller ones at T-junctions,
  where rounding leaves pinholes; the vertex shader grows flat quads by a
  thousandth of a metre to seal them. Tree corners are placed by surface nets
  on a blurred tree density. Leaning quads are regrouped by the axis they face
  most and sorted by lean, so culling can keep only the ones that might show.
- Rendering (`src/render`, `src/shaders`): one large storage buffer of quads
  (up to 1 GiB, or what the device allows) that the vertex shader expands,
  with a shared index buffer and no vertex buffer. Each frame the CPU culls
  chunks against the view, drops face directions that point away from the
  camera, orders them front to back and issues one multi-draw where the device
  supports it. Shadows are the three cascades described above. Rendering is
  4x MSAA or none (the setting; `--msaa 2` as well) into an HDR target, then the post pass (`post.wgsl`: tone mapping,
  the distance blur and sun rays, among others) and the HUD.
- Game (`src/game.rs`, `player.rs`, `physics.rs`, `entities.rs`,
  `creatures/`, `items.rs`, `models.rs`): a fixed 60 Hz step with
  interpolated rendering, box-versus-voxel physics with a two-voxel step-up,
  creatures in a hecs world, voxel models at a finer scale than the terrain.
  Walking off a ledge of up to two voxels snaps down instead of falling, and
  the eye height follows the body through a critically damped spring (about
  0.1 s), so steps up and down read as slopes.
