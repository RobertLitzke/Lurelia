//! A deathmatch brain: the decision layer for a bot that is trying to win a
//! fight.
//!
//! Nine named states, visited a few times a second, with a short `if` for a
//! hub. There is no planner and no behaviour tree, and the result still
//! reads as intent, because the states are the things a player would name.
//!
//! It decides on its own clock (`think_every`), not every step, which is
//! both cheaper and more human: a bot that reconsiders sixty times a second
//! dithers.

use super::nav::{Able, Path, Reach};
use super::person::Personality;
use super::senses::Memory;
use super::{aim, desire, senses, steer, Brain, Ctx, Me, Pilot};
use crate::combat::shot;
use crate::combat::weapon::Trigger;
use crate::library::FireDef;
use glam::Vec3;

/// What it is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// Nobody about: walk the map.
    Roam,
    /// Knows where somebody was, cannot see them: go and look.
    Hunt,
    /// Can see them: hold the range and shoot.
    Fight,
    /// Can see them and wants to be closer.
    Close,
    /// Hurt enough to want out of the open.
    Break,
    /// Falling, so not deciding anything much.
    Falling,
}

impl Act {
    fn name(self) -> &'static str {
        match self {
            Act::Roam => "roam",
            Act::Hunt => "hunt",
            Act::Fight => "fight",
            Act::Close => "close",
            Act::Break => "break",
            Act::Falling => "fall",
        }
    }
}

/// Metres: inside this it wants to back off, beyond it it closes.
const WANT_NEAR: f32 = 5.0;
/// Measured, not guessed. At 34 m a bot held the band at whatever range it
/// first saw you from, and on an arena whose sightlines are about ten metres
/// that meant every fight was fought at the far end of the aim cone: two
/// easy bots managed 0.023 accuracy and nobody died in a minute, which is
/// not a deathmatch. The cone is a fixed angle, so range is the single
/// biggest thing deciding whether a bot can hit anything, and it belongs
/// under the arena's own sightlines rather than above them.
const WANT_FAR: f32 = 16.0;
/// What range a bot expects its next fight at when it has nobody in mind.
/// Without this, `range` is the infinity that means "no target" and the
/// weapon scorer answers for two hundred metres, so a bot roams an arena
/// whose sightlines are ten metres holding the gun for a valley. It is then
/// holding the wrong one the moment something comes round a corner.
const ROAM_RANGE: f32 = 14.0;
/// How stale a contact may be and still be worth walking to where it was.
///
/// `senses::REMEMBER_S` is six seconds, and for a long time the hunt ran to
/// the whole of it: a duel on Slagdeck came out roam 31 / hunt 43 / fight 13
/// / close 10, so a bot spent more than three times as long going to look
/// for somebody as it did fighting them. That is "the bots are aimless" with
/// a number on it. Six seconds is also longer than the guess is worth --
/// `Contact::guess` carries a body on for at most 0.6 s, so a five-second
/// contact is a walk to where somebody stood five seconds ago, on a map it
/// takes eight seconds to cross. Past this, roaming is the better bet: it
/// keeps looking, it picks things up on the way, and it does not commit the
/// body to a destination that is already wrong. The contact is still
/// remembered -- this is how long it is worth *chasing*, not how long it is
/// known.
const HUNT_S: f32 = 3.0;
/// How long a bot sticks with somewhere it heard before another gun can
/// move it.
///
/// One reaction per noise was not enough. Three bots firing put a fresh
/// noise in the list several times a second, each with a new timestamp, so
/// a bot that reacts to every new one re-routes forty times a minute and
/// arrives nowhere -- which is the same aimlessness this session set out to
/// remove, arrived at from the opposite direction. A player who hears a gun
/// commits to going and looking; they do not pivot on every shot.
const HEARD_HOLD_S: f32 = 2.0;
/// Beyond this, closing is a journey and goes through the graph; inside it,
/// closing is a few steps and goes straight at the body.
///
/// Fighting and closing cleared the path and steered at the foe directly,
/// which is right for a scrap at eight metres and wrong for a decision to
/// cross an arena. A bot pressing toward somebody twenty metres away
/// through a hall of pillars is walking into geometry and sliding along it,
/// and the measurement said so: bots fight at 20-23 m while `band` says
/// they want to be inside sixteen, on a map whose sightlines are about ten.
/// Six metres too far out, with a fixed aim cone, is most of why a duel
/// looks like two bots missing each other.
const CLOSE_BY_GRAPH_M: f32 = 12.0;
/// How much better a weapon has to be before it swaps (desire.rs).
const SWAP_MARGIN: f32 = 0.3;
/// How much better the other button has to be before a bot moves its finger.
const BUTTON_MARGIN: f32 = 0.25;
/// Seconds between searches for a shot round a corner. The search walks a
/// shell through the world on half a dozen headings, which is not something
/// to do sixty times a second, and a corner does not move.
const BANK_EVERY_S: f32 = 0.3;
/// How near a predicted flight has to pass before the shot is worth taking.
/// Tight on purpose: a degree of aim error at the wall is metres after the
/// bounce, so a solution that only just works is a round thrown away, and
/// the skipjack carries a hundred of them.
const BANK_NEAR_M: f32 = 0.7;
/// What a bank shot keeps of the bot's usual aim error. A wall is not a man:
/// it does not strafe, and the error model is built out of how hard the
/// target is to track (`aim::error_rad`).
const BANK_STEADY: f32 = 0.5;
/// The fan of headings tried, degrees either side of the line to the target.
/// Nothing nearer than sixteen degrees, because a heading that close is not
/// a bank shot -- it is the shot that is already blocked.
const BANK_FAN: [f32; 12] = [12.0, -12.0, 22.0, -22.0, 33.0, -33.0, 45.0, -45.0, 58.0, -58.0, 72.0, -72.0];
/// The share of a weapon's store a bot keeps back from speculative shots.
const BANK_SPARE: f32 = 0.35;
/// How old a contact may be and still be worth banking a round at. A corner
/// shot is thrown at where somebody was; two seconds on, they are not there.
const BANK_STALE_S: f32 = 2.0;

/// Seconds a leg of the weave lasts.
const WEAVE_S: f32 = 1.1;
/// How long a bot sits on one aim error before picking another: the worst
/// bot's correction cycle, and the best one's.
const WOBBLE_S: f32 = 0.55;
const WOBBLE_MIN_S: f32 = 0.18;
/// How far ahead it checks for floor before stepping, metres, and what
/// counts as a drop.
const LOOK_AHEAD: f32 = 1.4;
const LEDGE_DROP: f32 = 2.6;
/// And how far ahead it looks for ground that burns. Further than the ledge
/// check, because a bot walks at about five metres a second and the step it
/// is refusing is the one it is already half committed to: a metre and a
/// half of warning is a quarter of a second, which is how bots kept ending
/// up in the slag they had just turned away from.
const FIRE_AHEAD: f32 = 2.8;
/// The soonest a bot considers another dodge, seconds. It rolls against
/// `DODGE_ODDS` times its own `tactics` each time this runs out, so an
/// average bot dodges every four seconds of a fight, a godlike one every
/// other second, and a novice never at all.
const DODGE_GAP_S: f32 = 1.0;
const DODGE_ODDS: f32 = 0.6;
/// The soonest a bot dodges again after being hit. Much shorter than the
/// habit's gap, because this one is a reaction, but not nothing: a minigun
/// lands a round every tenth of a second and a bot that tapped at every one
/// of them would never do anything else.
const HIT_DODGE_GAP_S: f32 = 0.5;
/// How far ahead a dodge is checked for floor, fire and wall, metres. A
/// dodge covers far more ground than a step, so the ledge check that keeps
/// a walking bot on the gantry is nowhere near enough for one.
const DODGE_AHEAD: f32 = 4.0;
/// How long it watches itself get nowhere before calling it wedged, and how
/// far counts as nowhere. A bot holding a band on purpose moves more than
/// this, because it circles.
const STUCK_S: f32 = 0.8;
const STUCK_M: f32 = 0.6;
/// How long it shoves sideways once it has decided it is wedged.
const SHOVE_S: f32 = 0.9;
/// How long it takes for somewhere a bot has been to feel worth revisiting,
/// and how much a named place is worth on top of that. The bonus is a thumb
/// on the scale rather than a magnet: fights do happen at the named places,
/// but a bot that only ever walks between them has not seen the map.
const STALE_S: f32 = 45.0;
const POI_WORTH: f32 = 7.0;
/// How long a bot will keep trying to reach one pickup before deciding that
/// whatever is in the way is not going to move, and how long it then leaves
/// that spot alone. The clock is this much plus the walk: a fixed seven
/// seconds is less than it takes to cross a large imported map, so every fetch
/// across the map was abandoned on the way and the pad blacklisted for
/// twenty seconds afterwards. Measured: 33 of 52 fetches given up in a
/// two-minute match.
///
/// Something has to say this. A goal you cannot reach but can still see the
/// worth of is a loop with no exit: the bot asks for a route to where it is
/// already standing, gets a path one step long, finishes it on the spot, and
/// asks again -- measured at 474 times in ninety seconds, with the bot
/// motionless for 97% of the match. `unjam` cannot catch it, because a bot
/// in that state is not asking to move and `unjam` only watches bots that
/// are.
const FETCH_GIVE_UP_S: f32 = 5.0;
/// Metres a second a fetch is allowed to take, which is well under the speed
/// a bot really walks: the allowance is a patience, not a schedule.
const FETCH_PACE: f32 = 2.5;
const AVOID_S: f32 = 20.0;
/// How often a bot works out afresh what it can reach. A lift arriving
/// changes the answer, so this cannot be done once.
const REACH_EVERY_S: f32 = 4.0;
/// And the soonest it will work it out again after a route it believed in
/// was refused: often enough to correct a stale answer, not so often that a
/// body standing somewhere with no routes out of it floods the graph on
/// every decision. Measured on an imported map without this: a bot that spawned in
/// a part of the graph nothing can be reached from asked for 250 routes in
/// two minutes and was refused 249 of them, re-flooding each time.
const REACH_AGAIN_S: f32 = 0.5;
/// How few nodes it has to be able to reach before it stops trusting the
/// graph and simply walks at what it wants. A body in a part of an imported
/// map that nothing links out of would otherwise stand still for the rest
/// of the match.
const STRANDED: usize = 4;
/// How long a bot will stand on a plate before deciding the lift is not
/// coming, and how long it then walks everywhere instead.
///
/// Something has to say this, for the same reason the fetch has a clock: a
/// wait is the one thing in the brain that looks exactly like being stuck,
/// so nothing else can tell the difference. A lift that is parked at the far
/// end, or one whose plate a body is standing on the wrong corner of, would
/// otherwise hold a bot for the rest of the match.
const RIDE_PATIENCE_S: f32 = 12.0;
const NO_LIFTS_S: f32 = 25.0;

/// How many candidates it looks at before choosing. A handful is enough to
/// steer toward the unvisited, and sampling rather than scanning keeps two
/// bots on the same map from walking the same circuit.
const TRIES: usize = 8;
/// How close to a plate's own node counts as standing on the plate.
const PLATE_M: f32 = 2.2;
/// Somewhere it is already standing is not somewhere to go.
const TOO_CLOSE_M: f32 = 5.0;

/// How badly a pad has to be wanted before a bot goes for it: the first
/// while it has nothing better to do, the second while it is in a fight.
/// `Lure::good` runs about 0 to 2.2, and the walk costs an eighth of a point
/// for every ten metres.
const FETCH_IDLE: f32 = 0.18;
const FETCH_BUSY: f32 = 0.75;

/// `dir` turned `rad` about the upright, which is how the fan of bank-shot
/// headings is swept across the walls either side of a target.
fn turned(dir: Vec3, rad: f32) -> Vec3 {
    let (s, c) = rad.sin_cos();
    Vec3::new(dir.x * c + dir.z * s, dir.y, -dir.x * s + dir.z * c).normalize_or(dir)
}

/// Which parts of moving are switched on.
///
/// Everything is, unless a world says otherwise. This exists for the
/// tournament (`tools/tourney.rs`), which turns one off at a time to find
/// out what each is actually worth: "the wall slide helps" is a claim, and
/// a claim about movement can only be settled by running the same bot on
/// the same map with it and without it. Keeping the switches here rather
/// than in the tool means the thing being measured is the shipping code
/// path, not a copy of it.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Slide along a wall rather than press into it (`steer::around`).
    pub wall_slide: bool,
    /// Shove out of whatever it is wedged in (`Duel::unjam`).
    pub unjam: bool,
    /// Circle a foe rather than only closing and backing off.
    pub circle: bool,
    /// Cross the map for what is lying on the floor.
    pub fetch: bool,
    /// Walk toward a gun it heard (`Duel::toward_noise`).
    pub hearing: bool,
    /// Break the circle with a dodge while fighting (`Duel::may_dodge`).
    pub dodge: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            wall_slide: true,
            unjam: true,
            circle: true,
            fetch: true,
            hearing: true,
            dodge: true,
        }
    }
}

impl Style {
    pub const NAMES: [&'static str; 6] = ["wall_slide", "unjam", "circle", "fetch", "hearing", "dodge"];

    /// Everything on except the named parts. A name nobody knows is
    /// reported rather than ignored, because a misspelt switch in a
    /// tournament entry would silently measure the same bot twice.
    pub fn without(names: &[String], mut warn: impl FnMut(String)) -> Style {
        let mut s = Style::default();
        for n in names {
            match n.as_str() {
                "wall_slide" => s.wall_slide = false,
                "unjam" => s.unjam = false,
                "circle" => s.circle = false,
                "fetch" => s.fetch = false,
                "hearing" => s.hearing = false,
                "dodge" => s.dodge = false,
                _ => warn(format!("no movement switch called {n} (one of {:?})", Style::NAMES)),
            }
        }
        s
    }
}

pub struct Duel {
    act: Act,
    style: Style,
    /// Seconds until it reconsiders.
    next: f32,
    mem: Memory,
    path: Path,
    /// Who it is fighting.
    foe: Option<usize>,
    /// Seconds it has been in this act, for the weave.
    since: f32,
    /// What this bot's own rolls are drawn from, kept so the weave can be
    /// its own shape without keeping state for it.
    seed: u32,
    /// A fixed offset into the weave, per bot. Without it two bots that
    /// meet are on the same clock, circle the same way at the same moment
    /// and hold a fixed bearing on each other for the whole fight, which is
    /// a face-off rather than a fight.
    lean: f32,
    rng: u32,
    /// Where it is aiming now, turned toward where it wants to aim.
    want_dir: Vec3,
    /// The aim error it is currently carrying, as the two unit rolls that
    /// `shot::spread` turns into an offset, and the seconds left before it
    /// picks a new one. Held rather than re-rolled every step: a cone
    /// re-sampled sixty times a second is white noise, and a head that
    /// turns at a rate averages white noise away and ends up pointing
    /// exactly at the target. That is a low-pass filter dressed as a miss
    /// chance, and it was most of why every difficulty was an aimbot with
    /// the minigun. Holding the offset makes the bot point somewhere wrong
    /// and stay there long enough to actually miss.
    wobble: (f32, f32),
    wobble_s: f32,
    /// Metres to the foe at the last decision, for the weapon choice.
    range: f32,
    /// Which button it means to hold. Kept because the choice between the
    /// two has the same hysteresis the weapon choice has: a bot at the range
    /// where a gun's two modes are worth the same must not flutter between
    /// them, because a mode that is charging or spinning up loses whatever
    /// it had built the moment the other button goes down.
    button: Trigger,
    /// The heading that puts a bouncing round on a target it cannot see,
    /// and the clock until the next search for one.
    bank: Option<Vec3>,
    bank_s: f32,
    /// Rounds sent round a corner, for the report, and whether last step's
    /// shot was one of them: a round is counted when the ammunition drops,
    /// not when the trigger goes down.
    banks: u32,
    banking: bool,
    /// Rounds in the gun it is holding, as of the last step, so that a round
    /// actually leaving can be told from a trigger merely being held: the
    /// brain has no other way of knowing a round went.
    had: Option<(u32, u32)>,
    /// The button it actually held last step.
    pressed: Trigger,
    /// Rounds fired on each button, for the report.
    left: u32,
    right: u32,
    /// How many decisions and sight checks it has made, for the report.
    decides: u32,
    looks: u32,
    /// Getting unwedged. `was` is where it stood when the check last looked
    /// and `watch` how long ago that was; `shove` counts down a deliberate
    /// sidestep out of whatever it is caught on, and `shove_side` is which
    /// way. `jams` is how many times it has had to.
    was: Vec3,
    watch: f32,
    shove: f32,
    shove_side: f32,
    jams: u32,
    /// Seconds until it may dodge again, and how many it has taken. The
    /// count is for the tournament, which cannot see a dodge in the frames
    /// it measures.
    dodge_in: f32,
    /// The same for the reactive one, which runs on its own clock.
    hit_dodge_in: f32,
    dodges: u32,
    /// Times it set off toward something it heard rather than somewhere it
    /// merely had not been lately, and the timestamp of the noise it last
    /// acted on, so one gun going off is reacted to once rather than on
    /// every tick it is still in the list.
    heards: u32,
    heard_at: f32,
    /// Routes asked for, routes refused, goals reached.
    asked: u32,
    failed: u32,
    reached: u32,
    /// The goal the current path is for, so arriving can be counted once.
    bound_for: Option<u16>,
    /// What it is currently crossing the map for, when it set off and how
    /// long it is giving itself. A fetch that never completes is the other
    /// way a bot locks up.
    fetch_at: Option<(u16, f32, f32)>,
    /// Nodes to leave alone until this time, because heading for them did
    /// not work out.
    avoid: Vec<f32>,
    /// Which nodes it can actually walk to from where it is, and when that
    /// was last worked out. Kept rather than discovered by failing, and
    /// refreshed on a clock because a lift moving changes the answer.
    can_reach: Vec<bool>,
    reach_due: f32,
    /// How long it has been standing on a plate waiting, and how long it
    /// has sworn off lifts after one did not come.
    riding: f32,
    lifts_at: f32,
    /// What its last look at the floor came to, for the report (`Scouting`).
    scout: super::Scouting,
    /// When it was last at each node, seconds on the match clock, sized to
    /// the graph the first time it is wanted.
    ///
    /// This is the whole of a bot's memory of the map and it exists because
    /// of one measurement: picking the next stop at random sent 55% of every
    /// bot's trips to Slagdeck's four named places, so a bot travelled three
    /// hundred metres a minute and saw a tenth of the arena. Knowing where
    /// you have just been is most of what makes a patrol look like a patrol
    /// rather than a pendulum.
    been: Vec<f32>,
}

impl Duel {
    pub fn new(seed: u32) -> Duel {
        Duel::playing(seed, Style::default())
    }

    /// The same, with parts of its movement switched off.
    pub fn playing(seed: u32, style: Style) -> Duel {
        Duel {
            act: Act::Roam,
            style,
            next: 0.0,
            // Stagger the sight checks, so a row of bots does not raycast on
            // the same step.
            mem: Memory::new(senses::LOOK_EVERY * (seed % 7) as f32 / 7.0),
            path: Path::default(),
            foe: None,
            since: 0.0,
            seed: seed | 1,
            lean: (seed % 97) as f32 / 97.0 * WEAVE_S * 2.0,
            rng: seed | 1,
            want_dir: Vec3::NEG_Z,
            wobble: (0.5, 0.5),
            wobble_s: 0.0,
            range: 0.0,
            button: Trigger::Fire,
            bank: None,
            bank_s: 0.0,
            banks: 0,
            banking: false,
            had: None,
            pressed: Trigger::Off,
            left: 0,
            right: 0,
            decides: 0,
            looks: 0,
            was: Vec3::ZERO,
            watch: 0.0,
            shove: 0.0,
            shove_side: 1.0,
            jams: 0,
            dodge_in: DODGE_GAP_S,
            hit_dodge_in: 0.0,
            dodges: 0,
            heards: 0,
            heard_at: -1.0,
            asked: 0,
            failed: 0,
            reached: 0,
            bound_for: None,
            fetch_at: None,
            avoid: Vec::new(),
            can_reach: Vec::new(),
            reach_due: 0.0,
            riding: 0.0,
            lifts_at: 0.0,
            scout: super::Scouting::default(),
            been: Vec::new(),
        }
    }

    /// Forgotten everything: what a respawn does.
    pub fn reset(&mut self) {
        self.act = Act::Roam;
        self.next = 0.0;
        self.mem.clear();
        self.path.clear();
        self.foe = None;
        self.since = 0.0;
        self.watch = 0.0;
        self.shove = 0.0;
    }

    /// Who it is fighting and how far off it thinks they are, for the
    /// headless report.
    pub fn foe(&self) -> Option<usize> {
        self.foe
    }

    pub fn range(&self) -> f32 {
        self.range
    }

    /// How many fighters it has any memory of, and how many it can see now.
    pub fn counts(&self) -> (u32, u32) {
        (self.decides, self.looks)
    }

    pub fn contacts(&self) -> (usize, usize) {
        (self.mem.of.len(), self.mem.of.iter().filter(|c| c.in_sight).count())
    }

    pub fn act(&self) -> Act {
        self.act
    }

    fn roll(&mut self) -> f32 {
        self.rng = crate::world::noise::hash32(self.rng);
        crate::world::noise::unit(self.rng)
    }

    /// How long until it reconsiders. A quicker bot decides more often.
    fn think_every(&mut self, p: &Personality) -> f32 {
        // An `average` bot used to land near 0.35 s and could reach half a
        // second: a third of a second in which a changed situation goes
        // unnoticed, several times over in any engagement. That reads as
        // dithering, and it falls hardest on the weaker bots a player is
        // most likely to be fighting.
        //
        // The cap is ordered by skill rather than flat. A flat 0.25 s cap
        // only ever binds the slow thinkers -- a high-`tactics` bot is
        // already inside it -- so it hands a novice the same cadence as a
        // godlike and flattens the ladder with it. Measured in the
        // tournament: with the flat cap `godlike` read +70 +-44 against the
        // shipping bot, and with no cap at all +93 +-46 and two errors
        // clear. So: nobody blacks out for half a second, and better bots
        // still think oftener.
        let cap = 0.20 + 0.18 * (1.0 - p.tactics);
        (0.1 + 0.35 * (1.0 - p.tactics) + self.roll() * 0.15).min(cap)
    }

    /// Who to fight, out of what it remembers.
    fn pick_foe(&mut self, me: &Me, cx: &Ctx) -> Option<usize> {
        let was = self.foe;
        let mut best: Option<(usize, f32)> = None;
        for c in &self.mem.of {
            let v = desire::threat(
                &desire::Candidate {
                    who: c.who,
                    dist: (c.guess(cx.now) - me.eye).length(),
                    // It has no way of knowing how hurt anyone else is, and
                    // giving it one would be a cheat.
                    whole: 1.0,
                    in_sight: c.in_sight,
                    stale: c.stale(cx.now),
                },
                was,
            );
            if best.is_none_or(|(_, bv)| v > bv) {
                best = Some((c.who, v));
            }
        }
        best.map(|(w, _)| w)
    }

    /// Set off somewhere. `to` is a node; the path is dropped if there is no
    /// way there.
    /// What this body is willing to take. Worked out in one place so the
    /// survey and the route cannot disagree about it -- a bot that surveys
    /// with one set of links and routes with another would rule places in
    /// that it then fails to reach.
    fn able(&self, me: &Me) -> Able {
        Able {
            // A timid bot will not take a drop it does not have to.
            jump: true,
            drop: me.person.tactics > 0.15 || me.whole() > 0.5,
            // Stops being true for a while once a plate has failed to
            // arrive, so a dead lift is not tried again every four seconds.
            lift: self.lifts_at <= 0.0,
            portal: true,
        }
    }

    fn head_for(&mut self, me: &Me, cx: &Ctx, to: u16) {
        self.head_for_at(me, cx, to, None);
    }

    /// The same, finishing with a short walk to a point off the graph: what
    /// a pad needs, because a pad is not on a node (`Path::tail`).
    fn head_for_at(&mut self, me: &Me, cx: &Ctx, to: u16, tail: Option<Vec3>) {
        let able = self.able(me);
        let from = cx.nav.nearest_seen(cx.store, me.eye, me.foot).unwrap_or(to);
        self.asked += 1;
        match cx.nav.route(from, to, able) {
            Some(steps) => {
                for w in steps.windows(2) {
                    match cx.nav.reach_between(w[0], w[1]) {
                        Reach::Portal => self.scout.portal_routes += 1,
                        Reach::Lift => self.scout.lift_routes += 1,
                        _ => {}
                    }
                }
                self.path = match tail {
                    Some(at) => Path::to_point(steps, at),
                    None => Path::new(steps),
                }
            }
            None => {
                // Somewhere it cannot get to from here: leave it alone for a
                // while rather than asking again every time it reconsiders.
                //
                // On a hand-drawn arena a route never fails. On an imported
                // map it fails constantly, because the graph has parts only a
                // lift reaches and, on one of them, two components -- and a bot
                // that keeps choosing from the whole graph spends its
                // decisions on places it cannot walk to and stands still
                // through every one of them. Measured on another: an easy bot
                // asked for 107 routes in two minutes and 64 of them failed.
                //
                // The cool-off matters as much as the marking. A lift moves,
                // so a node that is out of reach now may not be in twenty
                // seconds, and a permanent blacklist would teach a bot that
                // half the map does not exist.
                self.failed += 1;
                // Nothing the graph can do for it. If it can hardly reach
                // anything at all it is stranded -- a spawn inside a part of
                // an imported map with no links out of it -- and walking at
                // the thing is better than standing still: steering has its
                // own wall slide and ledge check, which is what a player
                // would be using.
                let few = self.can_reach.iter().filter(|&&r| r).count();
                if !self.can_reach.is_empty() && few <= STRANDED {
                    if let Some(n) = cx.nav.at(to) {
                        self.path = Path::to_point(Vec::new(), tail.unwrap_or(n.at));
                        return;
                    }
                }
                // What it believed it could reach was wrong, so it asks
                // again rather than spending the next four seconds making
                // the same mistake: a bot that has just dropped into a
                // pocket has a stale answer, and the stale answer is what
                // chose this goal.
                self.reach_due = self.reach_due.min(REACH_AGAIN_S);
                if self.avoid.len() != cx.nav.len() {
                    self.avoid = vec![0.0; cx.nav.len()];
                }
                if let Some(a) = self.avoid.get_mut(to as usize) {
                    *a = cx.now + AVOID_S;
                }
                if let Some(b) = self.been.get_mut(to as usize) {
                    *b = cx.now;
                }
                self.path.clear();
            }
        }
    }

    /// The pad worth going for, if one is worth more than the walk.
    ///
    /// Cost as well as value, which is the whole of what makes this read as
    /// a decision rather than as a magnet: a health pack across the map is
    /// worth less than one round the corner, and a bot in a fight has to
    /// want the thing badly before it turns its back. `good` comes from the
    /// rules, so nothing here knows what it is going for.
    fn worth_fetching(&mut self, me: &Me, cx: &Ctx, busy: bool) -> Option<(u16, Vec3)> {
        self.scout.lures = cx.lures.len() as u32;
        if !self.style.fetch || cx.lures.is_empty() || cx.nav.is_empty() {
            return None;
        }
        if self.avoid.len() != cx.nav.len() {
            self.avoid = vec![0.0; cx.nav.len()];
        }
        // Still trying for the same thing after long enough that it is not
        // going to happen: leave it alone for a while and find something
        // else to do.
        if let Some((node, since, allow)) = self.fetch_at {
            if cx.now - since > allow {
                if let Some(a) = self.avoid.get_mut(node as usize) {
                    *a = cx.now + AVOID_S;
                }
                self.scout.gave_up += 1;
                self.fetch_at = None;
                self.path.clear();
            }
        }
        let need = if busy { FETCH_BUSY } else { FETCH_IDLE };
        let mut best: Option<(u16, Vec3, f32)> = None;
        let mut want = f32::MIN;
        let mut blocked = 0;
        for l in cx.lures {
            // The walk, priced at an eighth of a point per ten metres. It
            // was a third, which on an arena forty metres across meant
            // almost nothing was ever worth fetching: a bot stood empty
            // rather than walk twenty metres for rounds. Cost has to order
            // the pads without vetoing them.
            let away = me.foot.distance(l.at);
            let v = l.good - away / 80.0;
            want = want.max(v);
            if v < need {
                continue;
            }
            let Some(node) = cx.nav.nearest(l.at) else {
                blocked += 1;
                continue;
            };
            if self.avoid.get(node as usize).is_some_and(|&t| cx.now < t) || !self.reaches(node) {
                blocked += 1;
                continue;
            }
            if best.is_none_or(|(_, _, bv)| v > bv) {
                best = Some((node, l.at, v));
            }
        }
        self.scout.want = if want == f32::MIN { 0.0 } else { want };
        self.scout.blocked = blocked;
        if let Some((n, at, _)) = best {
            if self.fetch_at.map(|(x, _, _)| x) != Some(n) {
                let allow = FETCH_GIVE_UP_S + me.foot.distance(at) / FETCH_PACE;
                self.fetch_at = Some((n, cx.now, allow));
                self.scout.fetches += 1;
            }
        } else {
            self.fetch_at = None;
        }
        best.map(|(n, at, _)| (n, at))
    }

    /// Work out afresh what it can walk to, now and then.
    fn survey(&mut self, me: &Me, cx: &Ctx, dt: f32) {
        self.reach_due -= dt;
        if self.reach_due > 0.0 && self.can_reach.len() == cx.nav.len() {
            return;
        }
        self.reach_due = REACH_EVERY_S;
        let able = self.able(me);
        match cx.nav.nearest_seen(cx.store, me.eye, me.foot) {
            Some(here) => cx.nav.reachable_from(here, able, &mut self.can_reach),
            // Nowhere to stand on the graph: do not rule anything out.
            None => self.can_reach.clear(),
        }
        self.scout.reach = self.can_reach.iter().filter(|&&r| r).count() as u32;
        self.scout.nodes = cx.nav.len() as u32;
    }

    /// Can it get to this node at all? True when it has not worked out
    /// otherwise, so a bot never refuses the whole map on a stale answer.
    fn reaches(&self, i: u16) -> bool {
        self.can_reach.get(i as usize).copied().unwrap_or(true)
    }

    /// Mark where it is standing as somewhere it has been.
    fn remember(&mut self, me: &Me, cx: &Ctx) {
        if self.been.len() != cx.nav.len() {
            // Never been anywhere: a node it has not visited is as stale as
            // a node can be.
            self.been = vec![-STALE_S * 4.0; cx.nav.len()];
        }
        for i in 0..cx.nav.len() as u16 {
            if cx.nav.at(i).is_some_and(|n| n.at.distance(me.foot) < TOO_CLOSE_M) {
                self.been[i as usize] = cx.now;
            }
        }
    }

    /// Somewhere else on the map, for roaming: somewhere it has not been
    /// lately, with a thumb on the scale for the named places.
    ///
    /// Sampled rather than scanned, and scored rather than drawn, so a bot
    /// does not walk a fixed circuit and two bots do not walk the same one.
    fn somewhere(&mut self, me: &Me, cx: &Ctx) -> Option<u16> {
        if cx.nav.is_empty() {
            return None;
        }
        self.remember(me, cx);
        if self.avoid.len() != cx.nav.len() {
            self.avoid = vec![0.0; cx.nav.len()];
        }
        let n = cx.nav.len();
        let mut best: Option<(u16, f32)> = None;
        for _ in 0..TRIES {
            let i = ((self.roll() * n as f32) as usize % n) as u16;
            // Somewhere it can actually be sent. Asking for a route that
            // cannot exist burns a decision and leaves the bot standing.
            if !cx.nav.safe(i) {
                continue;
            }
            let Some(node) = cx.nav.at(i) else { continue };
            if self.avoid.get(i as usize).is_some_and(|&t| cx.now < t) || !self.reaches(i) {
                continue;
            }
            let away = node.at.distance(me.foot);
            if away < TOO_CLOSE_M {
                continue;
            }
            // Staleness first, then distance -- and distance is a reason to
            // go, not a reason not to. Charging for the walk made a bot pick
            // the nearest of its candidates, and since most of a map is
            // unvisited early on, most candidates tie on staleness and the
            // distance term decides: a bot that always takes the nearest
            // unvisited node never leaves the room it started in. Measured:
            // charging for it held coverage at 13% of the graph a minute,
            // paying for it does better.
            let stale = (cx.now - self.been[i as usize]).min(STALE_S);
            let v = stale + away / 25.0 + if node.poi.is_some() { POI_WORTH } else { 0.0 };
            if best.is_none_or(|(_, bv)| v > bv) {
                best = Some((i, v));
            }
        }
        // A bot with a graph is never out of places to go. Sampling can come
        // up empty -- every candidate too close, or on ground that burns --
        // and a bot that takes that for an answer stops asking and stands
        // there for the rest of the match. Measured before this was here:
        // the quickest bot in the table, which reconsiders seven times a
        // second and so draws a blank seven times a second, spent 72% of a
        // round motionless. That is what "sometimes the bots just stand
        // still" was.
        let far_enough = |i: u16| cx.nav.at(i).is_some_and(|x| x.at.distance(me.foot) >= TOO_CLOSE_M);
        let n = cx.nav.len() as u16;
        best.map(|(i, _)| i)
            // Somewhere it can get to, before somewhere it merely likes the
            // look of: an imported map has parts a body can be in and not
            // leave, and a fallback that ignores reach sends a bot to one of
            // them over and over.
            .or_else(|| (0..n).find(|&i| cx.nav.safe(i) && self.reaches(i) && far_enough(i)))
            .or_else(|| (0..n).find(|&i| cx.nav.safe(i) && far_enough(i)))
            // And if nothing on the whole graph is safe, somewhere unsafe
            // beats standing still: a bot in a pool has to be able to walk
            // out of it, and a bot on a map whose every node is marked has
            // to be able to play at all.
            .or_else(|| (0..n).find(|&i| far_enough(i)))
    }

    /// The node nearest the freshest thing it heard, if anything it heard
    /// was close enough to hear and somebody else made it.
    ///
    /// Deliberately not a contact: hearing a gun tells a bot where to go,
    /// not who is there or where they are facing. So this feeds the roam
    /// rather than the hunt -- the bot walks over and finds out by looking,
    /// which is both the honest model and the one that cannot hand a bot a
    /// target through a wall.
    fn toward_noise(&mut self, me: &Me, cx: &Ctx) -> Option<u16> {
        if !self.style.hearing {
            return None;
        }
        let fresh = cx
            .noises
            .iter()
            .filter(|n| n.by != me.who)
            .filter(|n| n.at.distance(me.foot) < n.reach)
            .max_by(|a, b| a.when.total_cmp(&b.when))?;
        // One reaction per gun going off, and then a hold: see
        // `HEARD_HOLD_S`. Without the hold, three bots shooting is a body
        // that re-routes several times a second and never arrives.
        if fresh.when <= self.heard_at || cx.now - self.heard_at < HEARD_HOLD_S {
            return None;
        }
        self.heard_at = fresh.when;
        self.heards += 1;
        cx.nav
            .nearest(fresh.at)
            .filter(|&i| cx.nav.safe(i) && self.reaches(i))
            .filter(|&i| cx.nav.at(i).is_some_and(|n| n.at.distance(me.foot) > TOO_CLOSE_M))
    }

    /// The decision: one of six acts, chosen a few times a second.
    fn decide(&mut self, me: &Me, cx: &Ctx) {
        self.decides += 1;
        let was = self.act;
        self.foe = self.pick_foe(me, cx);
        let contact = self.foe.and_then(|f| self.mem.get(f)).copied();
        self.range = contact.map_or(f32::MAX, |c| (c.guess(cx.now) - me.eye).length());
        self.act = match contact {
            None => Act::Roam,
            Some(c) if !c.in_sight && c.stale(cx.now) < HUNT_S => Act::Hunt,
            // Lost them too long ago to go and look: walk the map instead.
            Some(c) if !c.in_sight => Act::Roam,
            // Hurt and it has a nerve: break off rather than trade.
            Some(_) if me.whole() < me.person.nerve => Act::Break,
            Some(_) if self.range > self.band(me).1 => Act::Close,
            Some(_) => Act::Fight,
        };
        // Properly falling, not merely off the ground: a body resting on a
        // ramp or stepping down a kerb reads as airborne for a step at a
        // time, and a bot that calls that falling stops steering and never
        // starts again.
        if !me.on_ground && me.vel.y < -9.0 {
            self.act = Act::Falling;
        }
        if self.act != was {
            self.since = 0.0;
        }
        // Where it is going, now it has decided why.
        match (self.act, contact) {
            (Act::Roam, _) => {
                self.remember(me, cx);
                if self.path.done() {
                    if let Some(to) = self.bound_for.take() {
                        if cx.nav.at(to).is_some_and(|n| n.at.distance(me.foot) < TOO_CLOSE_M) {
                            self.reached += 1;
                        }
                    }
                }
                // Nothing to fight, so the floor is the only thing with
                // anything on it: a roaming bot that walks past the rocket
                // launcher is a bot that has not understood the map.
                // A gun going off is an interrupt, not a suggestion to be
                // taken up whenever the current errand happens to finish.
                // Consulted only on a path running out, hearing fired 0 to
                // 0.2 times in a sixty-second duel -- built, tested and
                // inert, because a bot is nearly always part way along a
                // route or fetching something. What a player does with a
                // gunshot is turn round, so this turns round.
                if let Some(to) = self.toward_noise(me, cx) {
                    if self.path.goal() != Some(to) {
                        self.head_for(me, cx, to);
                        self.bound_for = Some(to);
                    }
                } else if let Some((to, at)) = self.worth_fetching(me, cx, false) {
                    if self.path.goal() != Some(to) || self.path.done() {
                        self.head_for_at(me, cx, to, Some(at));
                    }
                } else if self.path.done() {
                    if let Some(to) = self.somewhere(me, cx) {
                        self.head_for(me, cx, to);
                        self.bound_for = Some(to);
                    }
                }
            }
            (Act::Hunt, Some(c)) => {
                // A gun going off now is better information than where
                // somebody was standing when last seen, and a hunt runs on
                // a contact up to `HUNT_S` old. Hunting was the largest act
                // in a three-bot match for every competent bot -- 41% for
                // `godlike` -- so a hunt that re-aims onto live information
                // is worth more here than anywhere else.
                if let Some(to) = self.toward_noise(me, cx) {
                    if self.path.goal() != Some(to) {
                        self.head_for(me, cx, to);
                    }
                } else if let Some(to) = cx.nav.nearest(c.guess(cx.now)).filter(|&i| self.reaches(i)) {
                    if self.path.goal() != Some(to) {
                        self.head_for(me, cx, to);
                    }
                } else if self.path.done() {
                    // It wants to go and look, and the place it would look
                    // cannot be routed to -- behind a lift, across a
                    // marooned pocket, or simply not in the graph. Hunting
                    // was then a bot standing still facing a memory, which
                    // is the worst thing in here to watch and reads as
                    // staring at a wall. Walk the map instead: it keeps
                    // looking, and a map is where people are.
                    if let Some(to) = self.somewhere(me, cx) {
                        self.head_for(me, cx, to);
                        self.bound_for = Some(to);
                    }
                }
            }
            (Act::Break, Some(c)) => {
                // Breaking off toward something that mends it, if there is
                // one worth the trip: a bot that runs for a health pack is
                // doing what a player does, and a bot that runs for a corner
                // and bleeds there is not.
                if let Some((to, at)) = self.worth_fetching(me, cx, true) {
                    if self.path.goal() != Some(to) || self.path.done() {
                        self.head_for_at(me, cx, to, Some(at));
                    }
                    self.next = self.think_every(me.person);
                    return;
                }
                // Away from them: the node furthest from where they are that
                // it can still get to.
                let from = c.guess(cx.now);
                // Somewhere it can actually get to and has not just failed
                // to. Without the reach test, the furthest node from the
                // foe on one imported map is inside a pocket nothing can walk out of,
                // so a hurt bot asked for the same impossible route every
                // time it reconsidered: measured at 80 of 189 route
                // requests in a two-minute match, all to one node.
                let open = |i: u16| cx.nav.safe(i) && self.reaches(i) && !self.avoid.get(i as usize).is_some_and(|&t| cx.now < t);
                let away = (0..cx.nav.len() as u16).filter(|&i| open(i)).max_by(|&a, &b| {
                    let da = cx.nav.at(a).map_or(0.0, |n| n.at.distance(from) + n.cover * 8.0);
                    let db = cx.nav.at(b).map_or(0.0, |n| n.at.distance(from) + n.cover * 8.0);
                    da.total_cmp(&db)
                });
                if let Some(to) = away {
                    if self.path.goal() != Some(to) {
                        self.head_for(me, cx, to);
                    }
                }
            }
            // A long close is a journey: route it, like a hunt. Steering
            // straight at a body across an arena is walking into whatever
            // is between.
            (Act::Close, Some(c)) if self.range > CLOSE_BY_GRAPH_M => {
                if let Some(to) = cx.nav.nearest(c.guess(cx.now)).filter(|&i| self.reaches(i)) {
                    if self.path.goal() != Some(to) {
                        self.head_for(me, cx, to);
                    }
                } else {
                    self.path.clear();
                }
            }
            // Fighting, and closing the last few metres, steer by the foe
            // rather than by a path.
            _ => self.path.clear(),
        }
        // The gun for the range it thinks it is at.
        self.next = self.think_every(me.person);
    }

    /// Where to point the gun, and whether to pull the trigger.
    fn shoot(&mut self, me: &Me, cx: &Ctx, out: &mut Pilot, dt: f32) {
        let Some(c) = self.foe.and_then(|f| self.mem.get(f)).copied() else {
            // Nothing to shoot at: look where it is going.
            if let Some(t) = self.path.target(cx.nav) {
                let flat = Vec3::new(t.x - me.foot.x, 0.0, t.z - me.foot.z);
                if flat.length() > 0.5 {
                    self.want_dir = flat.normalize();
                }
            }
            out.trigger = Trigger::Off;
            return;
        };
        let at = if c.in_sight { c.pos } else { c.guess(cx.now) };
        let to_target = (at - me.eye).normalize_or(Vec3::NEG_Z);
        let gun = me.guns.current();
        // Which button, before anything else: the lead, the aim error and
        // the check for a burst at its own feet all belong to the mode it is
        // about to fire, not to the left button as a matter of course.
        let want = self.hand(me, (at - me.eye).length());
        let mode = gun.and_then(|g| g.mode(want)).unwrap_or_else(|| &gun.unwrap().def.fire);
        // A gun whose rounds come off walls may be shot at a wall. This is
        // the whole of what the skipjack is for -- aim beside a man who thinks
        // a corner is cover and let the disc do the rest -- and until now no
        // bot had ever done it: they threw discs straight down the line like
        // any other projectile and landed 7% of them. Only when the target
        // cannot be seen, because that is what a corner is, and only from a
        // bot good enough to lead a projectile.
        // And only while it can afford to speculate. A round thrown at a
        // wall is a round that may land on nobody, and the skipjack carries a
        // hundred of them: a bot that banks its last twenty has disarmed
        // itself for whoever walks round the corner.
        let flush = gun.is_some_and(|g| g.unlimited() || g.in_gun() as f32 > g.def.ammo_max as f32 * BANK_SPARE);
        let round_a_corner = !c.in_sight && mode.bounces > 0 && mode.speed > 0.0 && me.person.leads && flush && c.stale(cx.now) < BANK_STALE_S;
        let bank = if round_a_corner { self.find_bank(me, cx, mode, at, c.vel, dt) } else { self.forget_bank() };
        // A projectile is led; a ray is not, and neither is anything from a
        // bot that is not good enough to lead.
        let to_lead = match bank {
            // The predictor was already asked where the man is standing, so
            // the heading it found needs no lead on top.
            Some(off_the_wall) => off_the_wall,
            None if me.person.leads && mode.speed > 0.0 => aim::lead(me.eye, at, c.vel, mode.speed, mode.gravity),
            None => to_target,
        };
        let hitscan = mode.speed <= 0.0;
        let roll = self.roll();
        // How hard this one is to track, which is not the same question as
        // where to send the shot: `aim::ahead` is measured off both bodies'
        // motion, `to_lead` off the shell's flight time.
        let ahead = aim::ahead(me.eye, me.vel, at, c.vel);
        let err = aim::error_rad(
            me.person,
            hitscan,
            to_target,
            ahead,
            c.held(cx.now),
            me.health < me.full * 0.999 && c.in_sight && self.since < 0.4,
            !me.on_ground,
            roll,
        );
        // A new wrong answer every so often, not sixty a second. The
        // interval is the bot's correction cycle: a novice sits on a bad
        // aim for half a second, a machine notices in a fifth.
        self.wobble_s -= dt;
        if self.wobble_s <= 0.0 {
            self.wobble = (self.roll(), self.roll());
            self.wobble_s = WOBBLE_S - (WOBBLE_S - WOBBLE_MIN_S) * me.person.aim;
        }
        let err = if bank.is_some() { err * BANK_STEADY } else { err };
        self.want_dir = crate::combat::shot::spread(to_lead, err, self.wobble.0, self.wobble.1);
        aim::turn_head(&mut out.yaw, &mut out.pitch, aim::look_at(self.want_dir), aim::turn_rate(me.person), dt);

        // Only fire while it can see them and the gun is where it meant to
        // put it -- which is not the same as where they are. Gating this on
        // the true direction instead would throw the error away a second
        // time: a bot aiming six degrees wide would simply decline to fire
        // until its head drifted onto the target, so it would never miss,
        // it would only pause. An unseen target is hunted, not sprayed at.
        let facing = Vec3::new(-out.yaw.sin() * out.pitch.cos(), out.pitch.sin(), -out.yaw.cos() * out.pitch.cos());
        let on_target = facing.dot(self.want_dir) > 0.985;
        // A burst that cannot hurt the man who set it off is no reason to
        // hold fire: that is the Digger, which is all burst and is
        // swung at arm's length.
        let splash_at_my_feet = mode.splash.as_ref().is_some_and(|s| s.self_damage > 0.0 && self.range < s.radius * 0.9);
        // A mode worth nothing at this range cannot land, whatever the aim
        // is: out of reach, or out of rounds. The bench caught bots swinging
        // the Digger at a man twelve metres off, which is noise in
        // every number it produces and would be a dead giveaway to watch.
        let worth = me.guns.current().is_some_and(|g| desire::mode_rating(g, mode, self.range, me.person.aim) > 0.0);
        out.trigger = if (c.in_sight || bank.is_some()) && on_target && worth && !splash_at_my_feet && self.roll() < me.person.cadence {
            want
        } else {
            Trigger::Off
        };
        // `hand` counts it on the step the round actually leaves.
        self.banking = out.trigger != Trigger::Off && bank.is_some();
    }

    /// Which button to hold this step: the mode the gun in hand is worth
    /// more with at this range.
    fn hand(&mut self, me: &Me, dist: f32) -> Trigger {
        let Some(g) = me.guns.current() else { return Trigger::Fire };
        // Did a round actually leave? The trigger being down does not say
        // so: a gun has a cadence, a magazine and a reload between the
        // finger and the muzzle.
        let now = (g.def.slot, g.ammo);
        let fired = self.had.is_some_and(|was| was.0 == now.0 && was.1 > now.1);
        self.had = Some(now);
        // A full charge waits for the finger to come up (`Held::release`),
        // so a bot holding the button lets go for a step to throw it.
        if g.charge >= 1.0 {
            return Trigger::Off;
        }
        if fired {
            match self.pressed {
                Trigger::Alt => self.right += 1,
                _ => self.left += 1,
            }
            // `banking` is last step's, which is the step that fired it:
            // `hand` runs before this step's aim is worked out.
            if self.banking {
                self.banks += 1;
            }
        }
        self.button = desire::button(g, dist, me.person.aim, self.button, BUTTON_MARGIN);
        self.pressed = self.button;
        self.button
    }

    /// A heading that puts a bouncing round on somebody behind a wall, if
    /// there is one. A fan of headings either side of the line to them, each
    /// flown through the world by `shot::bank`; the nearest miss under
    /// `BANK_NEAR_M` wins.
    ///
    /// Nothing in here knows what a skipjack is. The mode says it has
    /// `bounces`; the predictor reads `speed`, `gravity` and `fuse_s` out of
    /// the same file and walks. A gun that gained a bouncing mode tomorrow
    /// would be shot round corners with no bot code at all.
    fn find_bank(&mut self, me: &Me, cx: &Ctx, f: &FireDef, at: Vec3, going: Vec3, dt: f32) -> Option<Vec3> {
        self.bank_s -= dt;
        if self.bank_s > 0.0 {
            return self.bank;
        }
        self.bank_s = BANK_EVERY_S;
        let to = (at - me.eye).normalize_or(Vec3::NEG_Z);
        let mut best = BANK_NEAR_M;
        self.bank = None;
        for off in BANK_FAN {
            let dir = turned(to, off.to_radians());
            let near = shot::bank(cx.store, me.eye, dir, f, at, going);
            if near < best {
                best = near;
                self.bank = Some(dir);
            }
        }
        self.bank
    }

    /// No corner to shoot round this step: drop whatever was found before,
    /// so a stale heading cannot be fired down once the man walks back into
    /// the open.
    fn forget_bank(&mut self) -> Option<Vec3> {
        self.bank = None;
        self.bank_s = 0.0;
        None
    }

    /// The range to fight at with the gun in hand: the standing band,
    /// pulled in when the weapon cannot reach that far.
    ///
    /// Without this the band is a pair of constants, and a bot holding the
    /// Digger -- which works at three metres -- stands at sixteen
    /// swinging at the air. The bench measured exactly that: a hammer-only
    /// match in an empty room produced no damage at all in six rounds.
    /// The gun's own file says how far it reaches; the bot stands there.
    fn band(&self, me: &Me) -> (f32, f32) {
        let far = me.guns.current().map_or(WANT_FAR, |g| desire::carry(g).min(WANT_FAR));
        (WANT_NEAR.min(far * 0.55), far)
    }

    /// Shove itself out of whatever it is wedged in.
    ///
    /// A bot pressed into a wall, shuffling, for the rest of the match is
    /// the worst thing a bot can do, because it is the one failure a player
    /// reads as the game being broken rather than as an opponent playing
    /// badly. Every cause has the same symptom -- it is asking to move and
    /// not moving -- so this watches for the symptom rather than trying to
    /// enumerate the causes. Imported maps are where it bites: a hand-drawn
    /// arena has no awkward corners because somebody walked it.
    ///
    /// The way out is sideways and up, which is what a person does, and the
    /// path is thrown away so it routes again from where it actually is
    /// rather than from where it thought it would be by now.
    ///
    /// The watch runs whether or not the shove does. Counting only the
    /// shoves would make `jams` a measure of the cure rather than of the
    /// disease, and the tournament would then hand a bot with the cure
    /// switched off a perfect score on that column -- which is exactly what
    /// it did on its first run.
    fn unjam(&mut self, me: &Me, cx: &Ctx, want: &mut steer::Want, dt: f32, shove_out: bool) -> bool {
        if self.shove > 0.0 {
            self.shove -= dt;
            let along = Vec3::new(want.go.x, 0.0, want.go.z).normalize_or_zero();
            let side = Vec3::new(-along.z, 0.0, along.x);
            want.go = side * self.shove_side + along * 0.25;
            want.strafe = 0.0;
            // The jump is early in the shove: a lip it is caught on is
            // cleared by going up and out at once, not by going up later.
            want.jump = self.shove > SHOVE_S * 0.55;
            return true;
        }
        let trying = want.go.length() > 0.3 || want.strafe.abs() > 0.25;
        self.watch += dt;
        if self.watch < STUCK_S {
            return false;
        }
        let moved = me.foot.distance(self.was);
        self.was = me.foot;
        self.watch = 0.0;
        if trying && moved < STUCK_M && me.on_ground {
            self.jams += 1;
            if shove_out {
                self.shove = SHOVE_S;
                // Toward a side that has floor on it. Picking blind sent a
                // bot wedged against the edge of a drop sideways off it,
                // where the ledge check turned it straight back into what it
                // was caught on -- seventeen shoves in thirty seconds and
                // not a metre of ground gained.
                let along = Vec3::new(want.go.x, 0.0, want.go.z).normalize_or(Vec3::NEG_Z);
                let side = Vec3::new(-along.z, 0.0, along.x);
                let ok = |s: f32| steer::footing(cx.store, me.foot, side * s, LOOK_AHEAD, LEDGE_DROP);
                self.shove_side = match (ok(1.0), ok(-1.0)) {
                    (true, false) => 1.0,
                    (false, true) => -1.0,
                    _ => {
                        if self.roll() < 0.5 {
                            -1.0
                        } else {
                            1.0
                        }
                    }
                };
                self.path.clear();
            }
        }
        false
    }

    /// Where to put its feet.
    fn move_feet(&mut self, me: &Me, cx: &Ctx, out: &mut Pilot, dt: f32) {
        let mut want = steer::Want::default();
        let foe = self.foe.and_then(|f| self.mem.get(f)).copied();
        match self.act {
            Act::Roam | Act::Hunt | Act::Break => {
                self.path.advance(cx.nav, me.foot);
                // Nothing to walk along. A bot only picks a destination in
                // `decide`, and `decide` runs every `think_every` -- up to
                // half a second for a slow one -- so a path that finishes
                // just after a decision left the body pressing nothing
                // until the next one. At three or four decisions a second
                // that is a visible hitch every time it arrives anywhere,
                // and it is most of what reads as dithering. Standing still
                // is never the answer while roaming, so ask again at once.
                if self.path.target(cx.nav).is_none() {
                    self.next = 0.0;
                }
                if let Some(t) = self.path.target(cx.nav) {
                    want.go = Vec3::new(t.x - me.foot.x, 0.0, t.z - me.foot.z);
                    // Including a hunt. It used to walk, on the reading
                    // that a hunt is a search and a search is careful --
                    // but a bot that has just lost sight of somebody is
                    // chasing, not searching, and strolling to where they
                    // went is both slower than they are and the single
                    // most aimless-looking thing in here.
                    want.sprint = true;
                    // Breaking off: dodge along the way out. It is the
                    // moment a dodge is worth most -- the distance comes
                    // free and whoever is shooting has to lead it -- and
                    // the one a player watching reads straight away.
                    if self.act == Act::Break {
                        if let Some(way) = steer::Dodge::toward(out.yaw, want.go) {
                            want.dodge = self.may_dodge(me, cx, out.yaw, way, dt);
                        }
                    }
                    match self.path.reach(cx.nav) {
                        // A step that has to be jumped is jumped on the
                        // approach.
                        Reach::Jump if want.go.length() < 3.0 => want.jump = true,
                        // Riding: stand still on the plate and let the floor
                        // do the travelling. Pressing toward a landing ten
                        // metres overhead would only walk the bot off the
                        // edge of the thing carrying it.
                        Reach::Lift if want.go.length() < PLATE_M => {
                            want.go = Vec3::ZERO;
                            want.sprint = false;
                            self.riding += dt;
                            if self.riding > RIDE_PATIENCE_S {
                                // It is not coming. Walk, and leave lifts
                                // alone long enough to get somewhere else.
                                self.riding = 0.0;
                                self.lifts_at = NO_LIFTS_S;
                                self.reach_due = 0.0;
                                self.path.clear();
                            }
                        }
                        _ => self.riding = 0.0,
                    }
                }
            }
            // The long half of a close: walk the route, and keep the gun
            // on the target -- the aim is turned by `shoot`, not by this,
            // so a routed close still fires on the way in.
            Act::Close if self.range > CLOSE_BY_GRAPH_M && !self.path.done() => {
                self.path.advance(cx.nav, me.foot);
                if let Some(t) = self.path.target(cx.nav) {
                    want.go = Vec3::new(t.x - me.foot.x, 0.0, t.z - me.foot.z);
                    want.sprint = true;
                }
            }
            Act::Fight | Act::Close => {
                if let Some(c) = foe {
                    let to = c.guess(cx.now) - me.foot;
                    let flat = Vec3::new(to.x, 0.0, to.z);
                    let d = flat.length().max(0.01);
                    // Hold the band: close if far, back off if nearly nose
                    // to nose, otherwise hold and circle.
                    let (near, far) = self.band(me);
                    let push = if self.range > far {
                        1.0
                    } else if self.range < near {
                        -0.8
                    } else {
                        0.0
                    };
                    // Round the target as well as in and out. A want that
                    // only pushed along the line to the foe left two bots
                    // holding the band at nine metres, weaving on the spot
                    // and travelling nowhere for the whole match -- which is
                    // the one thing a fight must not look like, and is also
                    // free hits for both of them, since the crossing term in
                    // aim.rs only charges for a target that is going across
                    // your view.
                    //
                    // The lap is the want, not the sidestep: the body is
                    // already facing the foe because the aim turns it, so
                    // `steer::drive` resolves a tangential want into the
                    // same two keys a player circling you would hold. A
                    // novice circles slowly and a good one hard, but nobody
                    // stands still.
                    let round = Vec3::new(-flat.z, 0.0, flat.x) / d;
                    // Uneven legs, not a metronome: equal ones put the bot
                    // back where it started every two beats, which over a
                    // whole fight is pacing across the same few metres and
                    // is the most machine-like thing a bot does.
                    let lap = steer::lurch(self.since + self.lean, WEAVE_S, 0.8, self.seed);
                    let circle = if self.style.circle { 1.0 } else { 0.0 };
                    want.go = flat / d * push + round * lap * (0.4 + 0.6 * me.person.tactics) * circle;
                    want.sprint = push > 0.5;
                    want.strafe = lap * me.person.tactics;
                    // And now and then a dodge instead of another step of
                    // the lap: away from somebody in its face, otherwise
                    // the way it was already going round. It is the one
                    // thing a bot does here that a person watching reads
                    // as a decision rather than as pathing, and it is the
                    // move the arena's own movement was built for.
                    let way = if push < 0.0 {
                        steer::Dodge::Back
                    } else if lap > 0.0 {
                        steer::Dodge::Right
                    } else {
                        steer::Dodge::Left
                    };
                    want.dodge = self.may_dodge(me, cx, out.yaw, way, dt);
                }
            }
            // Still steering on the way down. There is air control in this
            // engine and a bot that freezes in the air is a bot that has
            // given up on the rest of the match.
            Act::Falling => {
                if let Some(t) = self.path.target(cx.nav) {
                    want.go = Vec3::new(t.x - me.foot.x, 0.0, t.z - me.foot.z);
                } else if let Some(c) = foe {
                    let to = c.guess(cx.now) - me.foot;
                    want.go = Vec3::new(to.x, 0.0, to.z);
                }
            }
        }
        // Round the wall rather than into it. `Act::Fight` and `Act::Close`
        // steer straight at the foe, and in any map that was not drawn by
        // hand that line goes through geometry.
        if self.style.wall_slide {
            want.go = steer::around(cx.store, me.foot, want.go, LOOK_AHEAD, me.half_w, me.height);
        }
        // While it is shoving, the shove is the plan: the ledge check below
        // must not turn it back into whatever it is escaping from.
        let shoving = self.unjam(me, cx, &mut want, dt, self.style.unjam);
        // Nor must it turn the bot back from a fall the graph itself routed
        // it over. A `drop` link is "walk to the edge and go off it", and
        // the ledge check is the rule that a bot never steps off anything
        // it did not choose -- so on the one link where it did choose, the
        // rule was stopping it. On Slagdeck that was most of the jams: a
        // body on a four-metre gantry, pressed at the lip above the floor
        // node it was routed to, shuffling until the shove threw it
        // somewhere else.
        let dropping = self.path.target(cx.nav).is_some() && self.path.reach(cx.nav) == Reach::Drop;
        // Never walk off a ledge it did not choose: this is the commonest
        // way a bot looks stupid, so steering checks for itself.
        // Into the slag counts as off a ledge, and further off than a ledge:
        // a step down costs nothing and a step into a pool is health a
        // second for as long as it takes to get out again. A bot that is
        // already in it is walking out whatever else it wanted, so this is
        // asked before the want is, and a bot standing still in a pool is
        // the case that matters most.
        let standing_in_it = crate::creatures::terrain::hurt_at(cx.store, me.foot, me.half_w) > 0.0;
        if !shoving && (standing_in_it || want.go.length_squared() > 1e-4 || want.strafe.abs() > 0.1) {
            let (sy, cy) = out.yaw.sin_cos();
            let heading = Vec3::new(want.go.x, 0.0, want.go.z).normalize_or_zero() + Vec3::new(cy, 0.0, -sy) * want.strafe;
            let into_the_fire = !standing_in_it && steer::scalding(cx.store, me.foot, heading, FIRE_AHEAD, me.half_w);
            let no_floor = !dropping && !steer::footing(cx.store, me.foot, heading, LOOK_AHEAD, LEDGE_DROP);
            if standing_in_it || into_the_fire || no_floor {
                let look = if standing_in_it || into_the_fire { FIRE_AHEAD } else { LOOK_AHEAD };
                match steer::aside(cx.store, me.foot, heading, look, me.half_w, LEDGE_DROP, standing_in_it) {
                    // The nearest heading that is neither a drop nor a fire.
                    Some(d) => {
                        want.go = d * want.go.length().max(1.0);
                        want.strafe = 0.0;
                    }
                    // Nowhere better: turn along the edge rather than
                    // stopping dead on it, as this always did.
                    None => {
                        want.strafe = -want.strafe;
                        want.go = Vec3::new(-want.go.z, 0.0, want.go.x) * 0.6;
                    }
                }
            }
        }
        out.mv = steer::drive(out.yaw, &want);
        // Being shot at beats the habit: a dodge on the timer is a bot
        // keeping itself moving, and this one is a bot answering something
        // that just happened to it.
        self.hit_dodge_in = (self.hit_dodge_in - dt).max(0.0);
        if let Some(d) = self.struck_dodge(me, cx, out.yaw) {
            want.dodge = Some(d);
        }
        // Set and never cleared here: whoever steps the body takes it
        // (`Shooter::step_bodies`), so the want survives until a body step
        // has actually seen it.
        if want.dodge.is_some() {
            out.dodge = want.dodge;
            self.dodges += 1;
        }
    }

    /// Whether to dodge `way` now, and whether there is anywhere to dodge
    /// to.
    ///
    /// The rate is the bot's own `tactics`, which is the knob that already
    /// says how willing it is to strafe, dodge and jump: a novice is 0 and
    /// never dodges, a godlike one rolls every gap. Off the ground it
    /// cannot, because the body only dodges off the ground and a want it
    /// refuses is a wasted gap.
    ///
    /// The ground check is the part that matters. A dodge is 12 m/s and a
    /// walking step is 8, so the ledge check that keeps a bot on the
    /// gantry while it circles would let it dodge straight off the side,
    /// which is a bot killing itself with the move that was meant to keep
    /// it alive.
    fn may_dodge(&mut self, me: &Me, cx: &Ctx, yaw: f32, way: steer::Dodge, dt: f32) -> Option<steer::Dodge> {
        self.dodge_in -= dt;
        if !self.style.dodge || !me.on_ground || self.dodge_in > 0.0 {
            return None;
        }
        self.dodge_in = DODGE_GAP_S;
        if self.roll() > me.person.tactics * DODGE_ODDS {
            return None;
        }
        if !self.dodge_ok(me, cx, yaw, way) {
            return None;
        }
        Some(way)
    }

    /// Shot at: get off the line it was hit along.
    ///
    /// A hit causes sideways movement, which is most of what makes a bot
    /// look alive in a firefight, and this is the one dodge that is a
    /// reaction rather than a habit. A bot that keeps walking the same line while somebody
    /// lands shots on it has not noticed, and not noticing is what a player
    /// reads as the opposition being asleep -- it is also free hits, because
    /// `aim.rs` only charges for a target that is crossing the view.
    ///
    /// Across the line from whoever landed it, when it remembers where they
    /// are; either side will do, so it takes the one with floor under it.
    /// Keener than the habit, at the full `tactics` rather than a fraction
    /// of it, because this is the moment a dodge is worth most.
    fn struck_dodge(&mut self, me: &Me, cx: &Ctx, yaw: f32) -> Option<steer::Dodge> {
        let by = me.struck?;
        if !self.style.dodge || !me.on_ground || self.hit_dodge_in > 0.0 {
            return None;
        }
        self.hit_dodge_in = HIT_DODGE_GAP_S;
        if self.roll() > me.person.tactics {
            return None;
        }
        // Across the shot, or across its own facing if it has no idea who
        // that was: a bot shot from behind still wants to stop being where
        // it was.
        let across = match self.mem.get(by) {
            Some(c) => {
                let to = c.guess(cx.now) - me.foot;
                Vec3::new(-to.z, 0.0, to.x).normalize_or_zero()
            }
            None => {
                let (sy, cy) = yaw.sin_cos();
                Vec3::new(cy, 0.0, -sy)
            }
        };
        [across, -across].into_iter().filter_map(|d| steer::Dodge::toward(yaw, d)).find(|&way| self.dodge_ok(me, cx, yaw, way))
    }

    /// Somewhere to dodge to: floor under it and nothing burning on it.
    ///
    /// Walls are deliberately not asked about. A dodge into a wall is a
    /// dodge that stops early, which is what a player's does and costs
    /// nothing; a dodge off a gantry is the move killing the bot it was
    /// meant to save, which is why the floor is checked 4 m out rather than
    /// the 1.4 m a walking step is checked at.
    fn dodge_ok(&self, me: &Me, cx: &Ctx, yaw: f32, way: steer::Dodge) -> bool {
        let h = way.heading(yaw);
        steer::footing(cx.store, me.foot, h, DODGE_AHEAD, LEDGE_DROP) && !steer::scalding(cx.store, me.foot, h, DODGE_AHEAD, me.half_w)
    }
}

impl Brain for Duel {
    fn think(&mut self, me: &Me, cx: &Ctx, out: &mut Pilot, dt: f32) {
        self.since += dt;
        let facing = Vec3::new(-out.yaw.sin(), 0.0, -out.yaw.cos());
        if self.mem.look(cx.store, me.eye, facing, me.person, me.who, cx.fighters, cx.now, dt) {
            self.looks += 1;
        }
        self.lifts_at = (self.lifts_at - dt).max(0.0);
        self.survey(me, cx, dt);
        self.next -= dt;
        if self.next <= 0.0 {
            self.decide(me, cx);
            // The gun for the range, asked of the guns themselves.
            // The gun for the fight it is in, or the fight an arena is
            // likely to give it next.
            let for_range = if self.foe.is_some() { self.range.min(200.0) } else { ROAM_RANGE };
            out.want_slot = desire::best(me.guns, for_range, SWAP_MARGIN, me.person.aim).and_then(|i| me.guns.guns.get(i)).map(|g| g.def.slot);
        } else {
            out.want_slot = None;
        }
        self.shoot(me, cx, out, dt);
        self.move_feet(me, cx, out, dt);
    }

    fn doing(&self) -> &'static str {
        self.act.name()
    }

    fn aiming_at(&self) -> (Option<usize>, f32, usize, usize) {
        let (known, seen) = self.contacts();
        (self.foe, self.range, known, seen)
    }

    fn jams(&self) -> u32 {
        self.jams
    }

    fn dodges(&self) -> u32 {
        self.dodges
    }

    fn triggers(&self) -> (u32, u32) {
        (self.left, self.right)
    }

    fn banks(&self) -> u32 {
        self.banks
    }

    fn wandering(&self) -> (u32, u32, u32) {
        (self.asked, self.failed, self.reached)
    }

    fn heards(&self) -> u32 {
        self.heards
    }

    fn scouting(&self) -> super::Scouting {
        self.scout
    }

    fn worked(&self) -> (u32, u32) {
        self.counts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::nav::Graph;
    use crate::brain::person;
    use crate::brain::Foe;
    use crate::combat::weapon::Arsenal;
    use crate::library::Library;
    use crate::world::fields::{NavDef, NavLink, NavNode};
    use crate::world::material::STONE;
    use crate::world::store::{Store, Voxels};

    fn room() -> Store {
        let mut s = Store::default();
        for cz in -1..=1 {
            for cy in 0..=1 {
                for cx in -1..=1 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        for x in -60..60 {
            for z in -60..60 {
                for y in 0..8 {
                    s.set(x, y, z, STONE);
                }
            }
        }
        s
    }

    fn graph() -> Graph {
        let def = NavDef {
            nodes: (0..4)
                .map(|i| NavNode {
                    at: [i as f32 * 6.0, 0.0],
                    up: 0.0,
                    cover: 0.0,
                    poi: None,
                })
                .collect(),
            links: (0..3)
                .map(|i| NavLink {
                    a: i,
                    b: i + 1,
                    reach: "walk".into(),
                    cost: 0.0,
                    platform: None,
                    portal: None,
                })
                .collect(),
        };
        Graph::build(&def, 2.0, |_, _, _| Some(2.0), |_| false, |w| panic!("{w}"))
    }

    struct Rig {
        store: Store,
        nav: Graph,
        guns: Arsenal,
    }

    fn rig() -> Rig {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        Rig {
            store: room(),
            nav: graph(),
            guns: Arsenal::new(&lib, &["minigun".into(), "rocket_launcher".into(), "scrap_gun".into()], true, |n| panic!("{n}")),
        }
    }

    fn me<'a>(r: &'a Rig, p: &'a Personality, foot: Vec3, yaw: f32) -> Me<'a> {
        Me {
            who: 0,
            foot,
            eye: foot + Vec3::Y * 1.62,
            vel: Vec3::ZERO,
            yaw,
            pitch: 0.0,
            on_ground: true,
            half_w: 0.3,
            height: 1.75,
            health: 100.0,
            full: 100.0,
            struck: None,
            guns: &r.guns,
            person: p,
        }
    }

    /// Alone on the map it roams: it picks a node, sets off, and the keys it
    /// presses actually point that way.
    #[test]
    fn alone_it_sets_off_somewhere() {
        let r = rig();
        let p = person::level("easy").unwrap();
        let mut b = Duel::new(3);
        let mut out = Pilot::default();
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let cx = Ctx {
            store: &r.store,
            nav: &r.nav,
            now: 1.0,
            lures: &[],
            noises: &[],
            fighters: &[Foe {
                who: 0,
                foot,
                vel: Vec3::ZERO,
                height: 1.75,
                alive: true,
            }],
        };
        for _ in 0..20 {
            b.think(&me(&r, &p, foot, out.yaw), &cx, &mut out, 1.0 / 60.0);
        }
        assert_eq!(b.act(), Act::Roam);
        assert_eq!(out.trigger, Trigger::Off, "it is shooting at nobody");
        assert!(out.mv.forward || out.mv.back || out.mv.left || out.mv.right, "it never moved off");
    }

    /// Shot at, it gets off the line: a blow landed on a keen bot makes it
    /// dodge, and across the shot rather than along it. With the switch off
    /// or with a bot that has no appetite for it, nothing happens.
    #[test]
    fn a_blow_landed_makes_a_keen_bot_dodge() {
        let r = rig();
        let foot = Vec3::new(0.0, 2.0, 0.0);
        // The shooter is due north of it, and it is facing north too, so
        // across the shot is left or right and never forward or back.
        let shooter = Vec3::new(0.0, 2.0, -12.0);
        let cx = Ctx {
            store: &r.store,
            nav: &r.nav,
            now: 1.0,
            lures: &[],
            noises: &[],
            fighters: &[Foe {
                who: 0,
                foot: shooter,
                vel: Vec3::ZERO,
                height: 1.75,
                alive: true,
            }],
        };
        let hit = |skill: &str, without: &[&str]| {
            let p = person::level(skill).unwrap();
            let mut b = Duel::playing(5, Style::without(&without.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(), |w| panic!("{w}")));
            let mut out = Pilot::default();
            let mut ways = Vec::new();
            for i in 0..600 {
                let mut m = me(&r, &p, foot, out.yaw);
                // A blow every ten steps, which is a gun being held on it.
                m.struck = (i % 10 == 0).then_some(0);
                b.think(&m, &cx, &mut out, 1.0 / 60.0);
                if let Some(d) = out.dodge.take() {
                    ways.push(d);
                }
            }
            ways
        };
        let keen = hit("godlike", &[]);
        assert!(!keen.is_empty(), "it was shot sixty times and never moved off the line");
        assert!(
            keen.iter().all(|d| matches!(d, steer::Dodge::Left | steer::Dodge::Right)),
            "it dodged along the shot rather than across it: {keen:?}"
        );
        assert!(hit("godlike", &["dodge"]).is_empty(), "it dodged with the switch off");
        // A novice has no appetite for it, which is what `tactics` says.
        assert!(hit("novice", &[]).is_empty(), "a novice dodged");
    }

    /// A pad does not stand on a nav node, and the walk has to finish on the
    /// pad. The graph here has nodes every six metres and the pad is three
    /// metres past the last one: a bot that stops at the node is a bot that
    /// never picks anything up, which is what every imported arena did.
    #[test]
    fn it_walks_the_last_metres_onto_the_pad() {
        let r = rig();
        let p = person::level("easy").unwrap();
        let mut b = Duel::new(3);
        let mut out = Pilot::default();
        let foot = Vec3::new(0.0, 2.0, 0.0);
        // Worth having, and off the graph: node 3 is at x 18, this is at 21.
        let pad = Vec3::new(21.0, 2.0, 0.0);
        let cx = Ctx {
            store: &r.store,
            nav: &r.nav,
            now: 1.0,
            lures: &[crate::brain::Lure { at: pad, good: 1.5 }],
            noises: &[],
            fighters: &[Foe {
                who: 0,
                foot,
                vel: Vec3::ZERO,
                height: 1.75,
                alive: true,
            }],
        };
        b.think(&me(&r, &p, foot, out.yaw), &cx, &mut out, 1.0 / 60.0);
        assert_eq!(b.fetch_at.map(|(n, _, _)| n), Some(3), "it did not set off for the pad");
        // Walked the nodes and standing on the last one: still going, and
        // going at the pad rather than at the node under its feet.
        let at_node = Vec3::new(18.0, 2.0, 0.0);
        b.path.advance(&r.nav, Vec3::new(0.0, 2.0, 0.0));
        b.path.advance(&r.nav, Vec3::new(6.0, 2.0, 0.0));
        b.path.advance(&r.nav, Vec3::new(12.0, 2.0, 0.0));
        b.path.advance(&r.nav, at_node);
        assert!(!b.path.done(), "it called the pad reached from three metres away");
        assert_eq!(b.path.target(&r.nav), Some(pad));
        // And the keys it presses point at the pad, not back down the route.
        b.think(&me(&r, &p, at_node, out.yaw), &cx, &mut out, 1.0 / 60.0);
        assert!(out.mv.forward || out.mv.back || out.mv.left || out.mv.right, "it stopped walking at the node");
        // Standing on it finishes the walk.
        b.path.advance(&r.nav, Vec3::new(20.6, 2.0, 0.0));
        assert!(b.path.done());
    }

    /// A gun going off within earshot is somewhere to go. Nobody is in
    /// sight, so the bot is roaming, and roaming used to mean sampling the
    /// graph for somewhere it had not been: two bots on one map could spend
    /// a minute not finding each other that way. Hearing is the cheapest
    /// thing that makes a map converge.
    #[test]
    fn it_walks_toward_a_gun_it_heard() {
        let r = rig();
        let p = person::level("easy").unwrap();
        let mut out = Pilot::default();
        let foot = Vec3::new(0.0, 2.0, 0.0);
        // The far end of the graph, which is where the shooting is.
        let shot_at = Vec3::new(18.0, 2.0, 0.0);
        fn ctx<'a>(r: &'a Rig, noises: &'a [crate::brain::Noise]) -> Ctx<'a> {
            Ctx {
                store: &r.store,
                nav: &r.nav,
                now: 1.0,
                lures: &[],
                noises,
                fighters: &[],
            }
        }
        let heard = [crate::brain::Noise {
            at: Vec3::new(18.0, 2.0, 0.0),
            when: 1.0,
            reach: 40.0,
            by: 7,
        }];
        let mut b = Duel::new(3);
        b.think(&me(&r, &p, foot, out.yaw), &ctx(&r, &heard), &mut out, 1.0 / 60.0);
        assert_eq!(b.act(), Act::Roam, "nobody is in sight, so it is roaming");
        let goal = b.path.goal().and_then(|i| r.nav.at(i)).map(|n| n.at);
        assert_eq!(goal, Some(shot_at), "it did not set off toward the shooting");

        // Its own gun is not news. A bot that walks toward itself firing
        // would stand on the spot pressing nothing in particular.
        let mine = [crate::brain::Noise {
            at: Vec3::new(18.0, 2.0, 0.0),
            when: 1.0,
            reach: 40.0,
            by: 0,
        }];
        let mut b = Duel::new(3);
        b.think(&me(&r, &p, foot, out.yaw), &ctx(&r, &mine), &mut out, 1.0 / 60.0);
        // Not where it went -- `somewhere` may pick that node anyway, since
        // the rig's graph is a line and that is the far end of it -- but
        // whether it treated the noise as news at all.
        assert_eq!(b.heards, 0, "it set off toward its own gun");

        // And a shot too far away to hear is not heard.
        let far = [crate::brain::Noise {
            at: Vec3::new(18.0, 2.0, 0.0),
            when: 1.0,
            reach: 2.0,
            by: 7,
        }];
        let mut b = Duel::new(3);
        b.think(&me(&r, &p, foot, out.yaw), &ctx(&r, &far), &mut out, 1.0 / 60.0);
        assert_eq!(b.heards, 0, "it heard a shot from outside its reach");
    }

    /// Shown a target in the open it turns onto it, fires, and chooses a gun
    /// for the range. At thirty metres that is not the scrap gun.
    #[test]
    fn shown_a_target_it_turns_and_fires() {
        let r = rig();
        let p = person::level("masterful").unwrap();
        let mut b = Duel::new(5);
        let mut out = Pilot::default();
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let him = Foe {
            who: 1,
            foot: Vec3::new(0.0, 2.0, -30.0),
            vel: Vec3::ZERO,
            height: 1.75,
            alive: true,
        };
        let fighters = [
            Foe {
                who: 0,
                foot,
                vel: Vec3::ZERO,
                height: 1.75,
                alive: true,
            },
            him,
        ];
        let mut fired = false;
        let mut slot = None;
        for i in 0..240 {
            let cx = Ctx {
                store: &r.store,
                nav: &r.nav,
                now: 1.0 + i as f32 / 60.0,
                lures: &[],
                noises: &[],
                fighters: &fighters,
            };
            b.think(&me(&r, &p, foot, out.yaw), &cx, &mut out, 1.0 / 60.0);
            fired |= out.trigger == Trigger::Fire;
            slot = out.want_slot.or(slot);
        }
        assert!(matches!(b.act(), Act::Fight | Act::Close), "it is {:?}", b.act());
        // Pointed at him: yaw 0 faces -Z, which is where he is.
        assert!(out.yaw.abs() < 0.2, "it never turned onto him: yaw {}", out.yaw);
        assert!(fired, "it never pulled the trigger");
        assert_ne!(slot, Some(8), "it picked the scrap gun at thirty metres");
    }

    /// A bot holding a weapon that works at arm's length walks in. The
    /// standing band is sixteen metres, and with nothing to pull it in a
    /// hammer-only match in an empty room produced no damage at all.
    #[test]
    fn a_short_weapon_pulls_the_bot_in() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let hammer = Arsenal::new(&lib, &["digger".into()], true, |n| panic!("{n}"));
        let rifle = Arsenal::new(&lib, &["sniper_rifle".into()], true, |n| panic!("{n}"));
        let reach = desire::carry(hammer.current().unwrap());
        assert!(reach < 5.0, "the Digger now reaches {reach} m");
        let r = Rig {
            store: room(),
            nav: graph(),
            guns: Arsenal::new(&lib, &["digger".into()], true, |n| panic!("{n}")),
        };
        let p = person::level("average").unwrap();
        let b = Duel::new(5);
        let me_h = {
            let mut m = me(&r, &p, Vec3::new(0.0, 2.0, 0.0), 0.0);
            m.guns = &hammer;
            m
        };
        let me_r = {
            let mut m = me(&r, &p, Vec3::new(0.0, 2.0, 0.0), 0.0);
            m.guns = &rifle;
            m
        };
        let (near, far) = b.band(&me_h);
        assert!(far <= reach + 0.01, "it stands off {far} m with a gun that reaches {reach}");
        assert!(near < far, "the band closed on itself: {near} to {far}");
        assert_eq!(b.band(&me_r).1, WANT_FAR, "a gun that reaches the whole room moved the band");
    }

    /// It will not stand on a rocket: with only the launcher, nose to nose,
    /// it holds its fire rather than killing itself.
    #[test]
    fn it_does_not_rocket_its_own_feet() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let r = Rig {
            store: room(),
            nav: graph(),
            guns: Arsenal::new(&lib, &["rocket_launcher".into()], true, |n| panic!("{n}")),
        };
        let p = person::level("godlike").unwrap();
        let mut b = Duel::new(9);
        let mut out = Pilot::default();
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [
            Foe {
                who: 0,
                foot,
                vel: Vec3::ZERO,
                height: 1.75,
                alive: true,
            },
            Foe {
                who: 1,
                foot: Vec3::new(0.0, 2.0, -2.0),
                vel: Vec3::ZERO,
                height: 1.75,
                alive: true,
            },
        ];
        let mut fired = false;
        for i in 0..180 {
            let cx = Ctx {
                store: &r.store,
                nav: &r.nav,
                now: 1.0 + i as f32 / 60.0,
                lures: &[],
                noises: &[],
                fighters: &fighters,
            };
            b.think(&me(&r, &p, foot, out.yaw), &cx, &mut out, 1.0 / 60.0);
            fired |= out.trigger == Trigger::Fire;
        }
        assert!(!fired, "it fired a rocket two metres from its own face");
    }

    /// Hurt past its nerve, it breaks off instead of trading.
    #[test]
    fn hurt_enough_it_breaks_off() {
        let r = rig();
        let p = person::level("skilled").unwrap();
        let mut b = Duel::new(11);
        let mut out = Pilot::default();
        let foot = Vec3::new(0.0, 2.0, 0.0);
        let fighters = [
            Foe {
                who: 0,
                foot,
                vel: Vec3::ZERO,
                height: 1.75,
                alive: true,
            },
            Foe {
                who: 1,
                foot: Vec3::new(0.0, 2.0, -12.0),
                vel: Vec3::ZERO,
                height: 1.75,
                alive: true,
            },
        ];
        for i in 0..120 {
            let cx = Ctx {
                store: &r.store,
                nav: &r.nav,
                now: 1.0 + i as f32 / 60.0,
                lures: &[],
                noises: &[],
                fighters: &fighters,
            };
            let mut m = me(&r, &p, foot, out.yaw);
            m.health = 10.0;
            b.think(&m, &cx, &mut out, 1.0 / 60.0);
        }
        assert_eq!(b.act(), Act::Break, "it stood and traded on ten health");
    }

    /// A switch nobody knows is reported rather than ignored. A misspelt
    /// one in a tournament entry would quietly measure the shipping bot
    /// twice and call the pair a comparison.
    #[test]
    fn a_misspelt_switch_is_not_silent() {
        let mut said = Vec::new();
        let s = Style::without(&["wall_slide".into(), "wallslide".into()], |w| said.push(w));
        assert!(!s.wall_slide, "the one that was spelt right did not take");
        assert!(s.unjam && s.circle && s.fetch, "it turned off more than it was asked to");
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].contains("wallslide"), "{said:?}");
        // The default is every part on, which is what a world gets.
        let d = Style::default();
        assert!(d.wall_slide && d.unjam && d.circle && d.fetch);
    }
}
