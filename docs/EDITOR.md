# The EDIT page

F8 while playing, or `--edit NAME` on the command line. One thing out of the
library stands in a studio in front of the camera with its entry beside it;
every field you step rebuilds what is on screen, and WRITE THE FILE puts the
entry back in `library/`. Five kinds: creatures, plants, materials,
the deathmatch guns and the things the camera goes looking for.

It is the same shape as the LOOK page (`src/lookpage.rs`): a list down one
side with the world live behind it, a colour picker, and an export that writes
a library file. The list is on the right here, because what it describes is
standing on the left.

## What is on screen

The page is a studio you stand in with a panel of fields down the right. The
camera is yours: drag to turn around the subject, the wheel to zoom, F to
frame it again, T to set it turning. A, D turn; W, S zoom; Q, E raise and
lower the eye. The arrows and the mouse work the panel. The player's body is
frozen where it was standing and put back, with their view and their ride,
when the page closes.

The subject is framed left of centre, because the panel takes the right 44% of
the screen and an animal drawn under it cannot be read. It is framed on how
far the model actually reaches, not on its collision height: a crow stands a
hand high and spans a metre of wing.

The panel is one scrolling form rather than a stack of menus. A pinned head
says what is being edited (KIND and ENTRY), then headings that open and shut
with the whole entry under them, then the help for the row you are on and
WRITE / REREAD / CLOSE pinned at the foot. Closing drops the entry as it
stands, so CLOSE (and Esc) on an entry with edits that were never written says
so and waits for a second press rather than taking an afternoon's tuning with
it; any further edit brings the warning back. A field whose value is not written
in this entry's own file -- inherited through `like`, or the engine's own
default -- is drawn dimmed, so what the file actually says can be read off the
panel.

The first heading, SHOWING, is the page's own state rather than anything in
the file:

- AGE (creatures): juvenile, adult, aged or deceased. A stage the entry has
  turned off still shows what it would look like, which is the quickest way to
  see why its `ages` block says what it says.
- POSE (creatures): what the subject is doing. REST holds the rig's own rest
  pose; STAND, WALK, RUN, GRAZE, LOOK ABOUT, SIT and LIE DOWN drive a walker's
  rig on the spot; FLY and PERCH do the same for a bird. DEAD lets any rigged
  animal go limp where it stands and settle on the floor (anim/limp.rs), as a
  fresh kill lies, and ROLL drops it another way. None of them has a
  mover or a mind, so the subject cannot walk out of the view. LIVE turns it
  loose in the world to graze and bolt as it would, tethered to the studio.
  Only the poses the entry's movement verb and rig can actually take are
  offered.
- GROUND: the grey grid platform, or the world's own ground where the page was
  opened. That second setting is the biome view.
- REGENERATE: another roll. For an animal that is another size, coat, speed
  and set of habits (`src/variety.rs`); for a plant it is another whole tree,
  since a plant's shape is drawn from its seed.

Then the entry's own headings -- BODY, COLOURS, BEHAVIOUR, GROUP and VARIETY
for a creature; SIZE, CROWN and MATERIALS for a plant; TEXTURE, CALM STYLE and
SURFACE for a material. A number steps with the arrows in its box or with a
click on one of them; a name drops a list open under its box, scrolled to the
one in force; a flag is a tick box, which says "unset" where the entry has
never set it; a colour opens the LOOK page's picker on it, and the model is
rebuilt as the colour moves. Morphs (named coats) show through what an
individual wears but have no rows, and nor do ops, rigs or `in_water`.

A **weapon** is all numbers: its shape is one of a handful built in Rust
(`combat::viewmodel::SHAPES`) that the file picks by name, so the page stands
that shape on the platform broadside -- a gun seen down its own barrel is a
dot -- and the minigun gets the barrel assembly that turns on an axis of its
own. The numbers are the point: the two fire modes each have their own
heading, with the cadence, damage, spread, falloff, shell flight, burst and
the charge and stick rules the shooter reads. A weapon's TRACER and
TINT are the linear 0..1 the renderer wants rather than the 0..255 a creature
is painted in; the page knows which and shows both as hex.

A **find** (`library/collectibles/`) is a model built from ops like a
creature's, with nothing driving it. It lies on the platform in its own
colours.

A material stands on the platform as a block two metres across and head high,
so its pattern can be seen on three faces at once and judged against something
person-sized. Its pattern, scale, contrast and both pairs of colours are live:
the texture tiles are rebuilt and re-uploaded as you step a field, with no
reload. So are the surface flags, which take a regenerate of the ground around
the studio because smooth, crisp and passable change how the stuff is meshed
rather than how it is painted. Editing a material changes it for the whole
world, so the ground underfoot can change colour while you work.

A creature that wants water or a tree (an alligator, a woodpecker) has nowhere
to stand on the platform, and the page says so; turn the platform off in a
biome that suits it. A plant whose bark or leaf materials the loaded world does
not reach is drawn in the default bark with no leaves, and the page names a
world to open instead.

## How it is put together

`src/editor/cam.rs` is the turntable: a target, a yaw, a pitch and a distance.
It writes the eye straight into the player every frame (`Game::look_from`)
while `Game::editing` freezes the body, so nothing of the player's own state
is disturbed and closing the page puts their view, their position and their
ride back. The whole rig slides sideways rather than turning, so the subject
lands left of centre without the view rolling about it.

`src/editor/pose.rs` is what the subject does. A posed subject is spawned with
a `Look` and a `Creature` and nothing else -- no body, no mover, no mind --
and this module drives its rig directly: `Anim::alive` with the activity
pinned for a walker, `Anim::flying` for a bird. Walking is walked on the spot,
which is what a studio is for. Only a LIVE subject has a body and a mind, so
only it needs the tether.

`src/editor/mod.rs` decides what an edit costs before doing anything
(`Impact`): a colour is PAINT, which writes the new colours into the palette
slots the model already points at; behaviour, group, variety and rarity are
ROW, which builds the species row again and puts it in place and then dresses
and poses the individual already standing there; everything that reaches the
model -- the shape, the size, the ages, the rig -- is WHOLE. Edits are
collected on `Editor::pending` and settled once a frame from `tick`, so
holding an arrow down does not build sixty models a second. A WHOLE edit drops
the subject and spawns a fresh one, so stepping VOXEL blinks; the other two
are smooth.

Two cases fall back to WHOLE on their own. A colour whose slots have moved
does: identical colours share a palette slot, so one of them changing can
shift every slot after it, and the model's voxels name the slots. And a LIVE
subject whose movement verb changes is spawned again, because its mover gave
it components a new verb would not.

`src/editor/form.rs` builds the panel's rows and `src/form.rs` draws them,
the same form the LOOK page is drawn with (on the left). A row carries the
`menu::Item` the rest of the page works in, so clicking, stepping and the
dropdown anchors work as they do on any menu; the form adds which kind of field it is, how deep it sits, and whether the value
is the entry's own. The panel is solid rather than a fade, because a form has
to be read against a bright studio.

`src/editor/mod.rs` keeps the entry as the JSON it was read from rather than
as a parsed `CreatureDef`: a field no knob covers is carried through an
edit untouched, and a bad edit fails at the parse with the page unchanged
instead of halfway through a rebuild. Every change reparses the whole entry
and rebuilds the subject, so what stands in the studio is built by exactly the
code the game uses.

An entry that names a parent with `like` is only half an entry on its own — an
alligator's file is its colours and nothing else — so the page reads every
value through the parent chain (`inherit::over`, the loader's own merge) and
writes only to the file in front of it. A knob on such an entry therefore
shows the inherited value and, when stepped, writes an override into the
child.

`src/editor/subject.rs` builds one creature's models, species row and palette
block, mirroring the per-creature part of `worldload::creatures`. The
subject is **appended** after everything the world loaded: models, species and
its palette block all go on the end, so no index the world already handed out
moves, and dropping the subject is a truncate. The two are kept apart on
purpose — the loader builds a whole world's creatures at once, the editor
builds one over and over and must leave the world's own alone.

`src/editor/splice.rs` is what makes WRITE THE FILE safe on a hand-written
library. It puts the edited value back into the text the file was read from
rather than serialising the entry again, which would reorder every nested
object (serde_json sorts an object's keys) and lose a creature's colours in
the order they are painted. A one-knob edit is a one-line diff; a test checks
that every entry in the library writes back byte for byte untouched.

Materials are a global table the engine installs from a library
(`matlib::install`), which is why the page keeps a library of its own: it
loads one for this world the first time a material is edited, puts the edited
material into it and installs that. The renderer notices the new table by its
generation and re-uploads the tiles by itself. A material the world never
reaches has no voxel id and so could not be drawn at all; the page gives it
one of the ids the world left free, which no chunk can be using.

The platform is centred on the player, not on the subject, and is wide enough
to hold both with room to walk round. That matters in a built-up world: a
platform laid only around the subject leaves the camera inside a wall looking
at the back of it. The page also turns the player to face what it is showing when it
opens, puts a match aside while it is up (`Game::set_editing`), and, opened
from the main menu, loads the last world as a world to walk rather than in
whatever mode was last played.

`src/editor/studio.rs` stands the subject a sensible distance off — a sapling
a few metres away, a beech most of its own height — and lays the grey grid
platform under it. The platform is part of the generated ground
(`Generator::studio_floor`), not a set of voxel edits: nothing of it can end
up in a save, and the plant being previewed is drawn after it, so the floor
cannot cut the tree in half. It is lit, shadowed and meshed like any other
ground, and taking it away is a flag and a regenerate, which is what makes the
two views one key apart.

A plant preview is not an entity at all — plants are terrain. The editor adds
one tree of its own through `Generator::studio`, a shared list `trees_near`
appends from (and which drops the world's own trees standing on the platform),
then asks the streamer to regenerate the ground around it. The plant is drawn
by the same code as every other tree in the world, sprites, grain and all.

## Verifying it

`--edit NAME` works headless, so a screenshot shows the page over the studio
exactly as the window draws it:

```
cargo run -- --headless --world hemlow --edit deer --screenshot shots/edit.png --size 1600x900
```

`--edit NAME:3` opens that heading on its own and shuts the rest,
`--edit NAME:3:world` drops the platform for the biome view, and
`--edit NAME:0:drop` leaves the entry list open under its row. The run prints
what it loaded and anything the page is complaining about;
`VG_EDIT_STEP="KNOB,DIR,TIMES"` steps a field before the shot, so a screenshot
can show an edit rather than only the entry as it is on disk,
`VG_EDIT_POSE=walk` sets the pose, `VG_EDIT_ORBIT="110,18,1.4"` turns the
camera that many degrees (yaw, then pitch) and multiplies its distance, and
`VG_EDIT_SECS` sets how long the page runs first (four seconds by default).
`VG_EDIT_POSE2=name` switches pose once the subject is standing and
`VG_EDIT_PAINT="SECTION,INDEX,R,G,B"` paints one of its colours; both, and
`VG_EDIT_STEP`, print what the edit cost and whether the individual standing
in the studio survived it, which is how the rebuild budget is checked:

```
EDIT: knob 8 is Some(Row); the individual stayed
EDIT: knob 0 is Some(Whole); the individual was spawned again
```
