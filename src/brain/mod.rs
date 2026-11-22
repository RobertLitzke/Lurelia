//! Bots: a body that presses the keys.
//!
//! The whole module rests on one claim (`docs/BOTS.md`, "The claim"): a
//! bot is not a kind of creature, it is a thing that fills in a
//! `MoveInput`. `Player::update` already takes one and knows nothing about
//! where it came from, so a bot walks, falls, collides, swims and jumps
//! through exactly the code the human drives. It cannot take a corner the
//! player could not take or shoot through a wall, because neither is
//! reachable from the input struct, which is what forces every difficulty
//! knob to be a knob on perception and judgement.
//!
//! Four layers, each usable without the one above it:
//!
//! | Layer | Question | Where |
//! |---|---|---|
//! | Senses | what may this body know? | `senses.rs` |
//! | Steering | which keys get me toward that point? | `steer.rs` |
//! | Navigation | where are the points? | `nav.rs` |
//! | Decision | what do I want? | `duel.rs`, and whatever comes next |
//!
//! Only the decision layer is per-game: a deathmatch bot, a guard on patrol
//! and a racer are three of those over one set of the other three. Across
//! all of them sits `person.rs`, a few scalars that every layer reads, so a
//! difficulty is a row in a table rather than a branch anywhere.
//!
//! `Ctx` holds nothing mutable, so a brain cannot cheat by construction.

pub mod aim;
pub mod avatar;
pub mod desire;
pub mod duel;
pub mod nav;
pub mod person;
pub mod senses;
pub mod sheet;
pub mod steer;
pub mod team;
pub mod team_v0;

use crate::anim::figure::Figure;
use crate::combat::weapon::{Arsenal, Trigger};
use crate::player::{MoveInput, Player};
use glam::Vec3;

/// Seconds the red flash of taking damage lasts.
pub const HURT_S: f32 = 0.45;
/// The most health a fighter can have when the world does not say
/// (`ShooterDef::health_max`): twice the usual 100 at spawn.
pub const HEALTH_MAX: f32 = 200.0;

/// Everything a body can be told to do this step. A human fills this from the
/// keyboard and the mouse; a brain fills it from code. There is nothing else.
#[derive(Default, Clone, Copy)]
pub struct Pilot {
    pub mv: MoveInput,
    pub yaw: f32,
    pub pitch: f32,
    pub trigger: Trigger,
    /// Take out the weapon on this digit.
    pub want_slot: Option<u32>,
    /// Dodge that way, taken by whoever steps the body (`steer::Tapper`).
    /// A brain sets it and never clears it, so the want lasts until a body
    /// step has taken it.
    pub dodge: Option<steer::Dodge>,
}

/// One fighter as everyone else sees them.
#[derive(Clone, Copy, Debug)]
pub struct Foe {
    pub who: usize,
    pub foot: Vec3,
    pub vel: Vec3,
    pub height: f32,
    pub alive: bool,
    /// Its side in a team match. Plates are painted, so anybody can see it.
    pub team: Option<u8>,
}

impl Foe {
    /// On the same side as `team`: somebody not to shoot.
    pub fn friend_of(&self, team: Option<u8>) -> bool {
        team.is_some() && self.team == team
    }

    /// What a shot is aimed at: the middle of the body, not the feet.
    pub fn chest(&self) -> Vec3 {
        self.foot + Vec3::Y * self.height * 0.55
    }
}

/// What a brain may read. Everything in it is shared, not borrowed mutably,
/// so thinking cannot change the world.
pub struct Ctx<'a> {
    pub store: &'a crate::world::store::Store,
    pub nav: &'a nav::Graph,
    /// Seconds since the match began.
    pub now: f32,
    pub fighters: &'a [Foe],
    /// What is lying about that this bot would be glad of, already scored
    /// for it.
    pub lures: &'a [Lure],
    /// What has been heard lately, newest last.
    pub noises: &'a [Noise],
    /// Everything in the air that could hurt somebody. A brain may only act
    /// on what it could see of these (`duel::Duel::incoming` checks the view
    /// cone and the line of sight).
    pub threats: &'a [Threat],
    /// Things lying in wait that go off when a body comes near: goo blobs.
    /// The same rule applies; only those in view count.
    pub mines: &'a [Threat],
}

/// A shell in flight, as a body watching it would know it: where it is,
/// where it is going, how far its blast reaches, and who fired it.
#[derive(Clone, Copy, Debug)]
pub struct Threat {
    pub at: Vec3,
    pub vel: Vec3,
    pub gravity: f32,
    pub reach: f32,
    pub by: usize,
}

/// Something somebody did that could be heard: a shot, mostly.
///
/// Sight was the only sense in here, and on an arena of corners that meant
/// two bots could fight within earshot of a third that never knew. It also
/// meant roaming was blind: two bots sampling a fifty-six-node graph
/// independently rarely meet, which is why a duel was spending about a
/// tenth of itself fighting. A noise is the cheapest thing that makes a map
/// converge, and it is what a player does -- you walk toward the shooting.
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    pub at: Vec3,
    /// When it was made, in match seconds.
    pub when: f32,
    /// How far it carries, metres.
    pub reach: f32,
    /// Who made it (`Ctx::fighters` numbering), so a bot does not walk
    /// toward its own gun.
    pub by: usize,
}

/// Something on the floor worth crossing for, as a bot is told about it.
///
/// Deliberately thin: a point and a number. The brain is never told what a
/// health pack is or what a rocket launcher is for -- the rules work out how
/// much good a pad would do the fighter asking (`shooter::pickup::Pad::good`)
/// and hand over the answer. The item rates itself for whoever asks, which
/// keeps the knowledge on the right side of the fence: a new kind of pickup
/// needs no bot code, the same way a new weapon file needs none
/// (`brain::desire`).
#[derive(Clone, Copy, Debug)]
pub struct Lure {
    pub at: Vec3,
    pub good: f32,
    /// For a gun lying on the floor, which of the fighter's arsenal slots it
    /// is, so the brain can weigh the gun itself rather than its slot
    /// number. A player can see what gun it is; so may a bot.
    pub gun: Option<usize>,
    /// Whether it heals: a player can see a health pack for what it is.
    pub mends: bool,
}

/// What a bot made of what was lying about, as of its last look at the
/// floor. For the headless report only: `want` against `FETCH_IDLE` says
/// whether a pad was outbid, and `blocked` says whether it was wanted and
/// out of reach, which are two different faults with the same symptom.
#[derive(Default, Clone, Copy, Debug)]
pub struct Scouting {
    /// Pads it set off for, and fetches abandoned as unreachable.
    pub fetches: u32,
    pub gave_up: u32,
    /// Pads worth anything at all to it at its last look, and the best
    /// score any of them had after the walk was charged.
    pub lures: u32,
    pub want: f32,
    /// Pads it wanted enough to go for but could not route to.
    pub blocked: u32,
    /// Nodes it reckons it can reach, of the whole graph.
    pub reach: u32,
    pub nodes: u32,
    /// Routes it has been given that go through a portal, and lifts they
    /// ride. A map's teleporter can look broken when nothing ever routes
    /// over it, which is a different fault from one that does not work.
    pub portal_routes: u32,
    pub lift_routes: u32,
}

/// The bot's own state, as it sees itself.
pub struct Me<'a> {
    pub who: usize,
    pub foot: Vec3,
    pub eye: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    /// The body's own size, which steering needs to ask whether a gap is
    /// one it fits through.
    pub half_w: f32,
    pub height: f32,
    pub health: f32,
    pub full: f32,
    /// Who landed a blow on it since it last thought, if anybody. The one
    /// thing in here that is an event rather than a state: it is taken, so
    /// a brain sees each blow once.
    pub struck: Option<usize>,
    pub guns: &'a Arsenal,
    pub person: &'a person::Personality,
    /// Its side in a team match, and what its side has asked of it
    /// (`team.rs`). Both None in a deathmatch.
    pub team: Option<u8>,
    pub order: Option<Order>,
}

/// A job the team hands one bot (`team::TeamBrain`). The brain is told the
/// job and a place, never what a flag is, so another objective mode needs a
/// new team brain and no new decision code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Go and take the enemy's flag.
    Attack,
    /// It has the flag: take it home.
    Carry,
    /// Stay with whoever has the flag and keep them alive.
    Escort,
    /// Hold ground by its own flag.
    Defend,
    /// Go after whoever has its own flag.
    Retrieve,
    /// Walk onto its own flag where it was dropped.
    Return,
    /// Hold high ground over the middle with a long gun and shoot whoever
    /// crosses it, without going down to them.
    Overwatch,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Attack => "attack",
            Role::Carry => "carry",
            Role::Escort => "escort",
            Role::Defend => "defend",
            Role::Retrieve => "retrieve",
            Role::Return => "return",
            Role::Overwatch => "overwatch",
        }
    }
}

/// Where a job is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goal {
    /// Get to this point.
    Point(Vec3),
    /// Stay near this fighter.
    Follow(usize),
    /// Hold this post: get there, stay round it looking toward `watch`,
    /// fight what comes within `leash` metres of it and come back.
    Hold { post: Vec3, leash: f32, watch: Vec3 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Order {
    pub role: Role,
    pub goal: Goal,
    /// 0..1: how much the job outbids a fight or a pad. A carrier at 1 runs
    /// rather than trades; a defender at 0.3 still fights what it sees.
    pub urgency: f32,
    /// A place to go through first: the lane an attacker was given, so two
    /// attackers do not walk the same corridor.
    pub via: Option<Vec3>,
    /// The fighter that matters most to the job, when it can see them: the
    /// enemy carrier, for a bot sent to get its own flag back.
    pub mark: Option<usize>,
    /// The range to pick a gun for while nobody is in sight, metres: an
    /// overwatch wants the long one out before anybody shows.
    pub reach: Option<f32>,
    /// Changes to how a role is played that are under test, one bit each
    /// (`TWEAKS`). The team brain never sets them; the shooter stamps a
    /// side's set on every order it hands out (`Shooter::tweak`), so the
    /// CTF bench can play one side's roles against the same roles without
    /// the change, with the jobs dealt the same way.
    pub tweaks: u32,
}

impl Order {
    pub fn new(role: Role, goal: Goal, urgency: f32) -> Order {
        Order {
            role,
            goal,
            urgency,
            via: None,
            mark: None,
            reach: None,
            tweaks: 0,
        }
    }

    /// Whether the role change `name` (one of `TWEAKS`) is on for this job.
    pub fn has(&self, name: &str) -> bool {
        tweak_bit(name).is_some_and(|b| self.tweaks & b != 0)
    }
}

/// The role changes the CTF bench can switch on for one side
/// (docs/BOTS.md "What the bench found"): four tries at each role, and three ways a team
/// brain sets its side's stance (`team::TeamBrain::tweaks`).
pub const TWEAKS: [&str; 28] = [
    "stance",
    "reset",
    "group",
    "lane",
    "attack1",
    "attack2",
    "attack3",
    "attack4",
    "carry1",
    "carry2",
    "carry3",
    "carry4",
    "escort1",
    "escort2",
    "escort3",
    "escort4",
    "defend1",
    "defend2",
    "defend3",
    "defend4",
    "retrieve1",
    "retrieve2",
    "retrieve3",
    "retrieve4",
    "return1",
    "return2",
    "return3",
    "return4",
];

pub fn tweak_bit(name: &str) -> Option<u32> {
    TWEAKS.iter().position(|t| *t == name).map(|i| 1 << i)
}

/// The bits for a comma-separated list of `TWEAKS` names, and any names
/// that are not one.
pub fn tweak_mask(list: &str) -> (u32, Vec<String>) {
    let mut mask = 0;
    let mut bad = Vec::new();
    for n in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match tweak_bit(n) {
            Some(b) => mask |= b,
            None => bad.push(n.to_string()),
        }
    }
    (mask, bad)
}

/// One fighter a bot remembers, as it would tell a teammate: who, where,
/// and when it last saw them. Pooled by the team brain, which is what a
/// side on voice comms has; nothing in here is more than one of them saw.
#[derive(Clone, Copy, Debug)]
pub struct Sighting {
    pub who: usize,
    pub at: Vec3,
    pub when: f32,
}

impl Me<'_> {
    /// Health as a share of full, which is what nerve is measured against.
    pub fn whole(&self) -> f32 {
        (self.health / self.full.max(1.0)).clamp(0.0, 1.0)
    }
}

/// A decision layer. One per kind of game.
pub trait Brain: Send {
    /// Who it is fighting, how far off it thinks they are, and how many
    /// fighters it remembers and can see. For the headless report, which is
    /// the only way to tell a bot that has chosen badly from one that has
    /// not seen anybody.
    fn aiming_at(&self) -> (Option<usize>, f32, usize, usize) {
        (None, 0.0, 0, 0)
    }

    /// Routes asked for, routes the graph could not answer, and goals
    /// actually arrived at. The three numbers that say whether a bot is
    /// roaming or only intending to.
    fn wandering(&self) -> (u32, u32, u32) {
        (0, 0, 0)
    }

    /// Times it set off toward a noise rather than toward somewhere it had
    /// merely not been lately. Zero says hearing never fired, which is a
    /// different thing from hearing not helping.
    fn heards(&self) -> u32 {
        0
    }

    /// Times it has had to shove itself out of something it was wedged in,
    /// for the report: a number that should stay small and did not.
    fn jams(&self) -> u32 {
        0
    }

    /// Times it stepped or dodged off the line of a shell it saw coming.
    fn evades(&self) -> u32 {
        0
    }

    /// One line on what it is walking toward and why, for diagnostics.
    fn note(&self) -> String {
        String::new()
    }

    /// Dodges taken, for the report. A dodge is four body steps and is over
    /// before anything that samples the match could catch it, so whether a
    /// bot dodges at all is otherwise unanswerable from outside.
    fn dodges(&self) -> u32 {
        0
    }

    /// Decisions made and sight checks run, for the report.
    fn worked(&self) -> (u32, u32) {
        (0, 0)
    }

    /// Rounds it has fired on the left button and on the right. For the report,
    /// because "do bots use the second fire mode at all" is otherwise a
    /// question nothing can answer from outside.
    fn triggers(&self) -> (u32, u32) {
        (0, 0)
    }

    /// Rounds it has banked off a wall at somebody it could not see
    /// (`duel::find_bank`), for the report: whether a bot ever shoots round
    /// a corner is otherwise invisible from outside.
    fn banks(&self) -> u32 {
        0
    }

    /// What its appetite for the floor came to, for the report: the one way
    /// to tell a bot that does not want a pad from one that wants it and
    /// cannot get to it.
    fn scouting(&self) -> Scouting {
        Scouting::default()
    }

    /// Decisions it has spent on a job its side gave it, for the report.
    fn errands(&self) -> u32 {
        0
    }

    /// Everybody it remembers, for the team brain to pool.
    fn sightings(&self, _out: &mut Vec<Sighting>) {}

    /// Fill in `out`. It need not think every step; `dt` is how long since
    /// it last did.
    fn think(&mut self, me: &Me, cx: &Ctx, out: &mut Pilot, dt: f32);
    /// What it is doing, for the HUD and for tests.
    fn doing(&self) -> &'static str {
        ""
    }
}

/// How a round is going, for one fighter.
#[derive(Default, Clone, Copy, Debug)]
pub struct Score {
    pub kills: u32,
    pub deaths: u32,
    /// Rounds fired: trigger pulls, not pellets.
    pub shots: u32,
    /// Damage events landed on somebody else. A scrap shell throws nine
    /// bodies for one round, and splash lands on everyone it catches, so
    /// this is hits per round and not a percentage -- for a one-pellet gun
    /// like the minigun the two are the same thing.
    pub hits: u32,
    /// Damage dealt to other fighters, which is the honest measure when a
    /// round can land nine times.
    pub dealt: f32,
    /// Damage taken from the floor itself (`hurt` materials: slag, acid),
    /// and lives the floor ended. What says whether bots keep out of it.
    pub scalded: f32,
    pub scald_deaths: u32,
    /// Lives ended by the fighter's own round: a blast, or a disc coming
    /// back off a wall.
    pub own_deaths: u32,
    /// Capture the flag: flags taken from a stand or off the floor, carried
    /// home, and own flags walked back onto.
    pub grabs: u32,
    pub caps: u32,
    pub returns: u32,
}

/// The rules' half of a fighter: what the match knows about a body, whoever
/// is driving it. The player has one of these and so does every bot, which
/// is what lets a match run with no player in it at all.
pub struct Fighter {
    pub health: f32,
    pub armour: f32,
    /// Health at spawn, and what "whole" means to everything that asks how
    /// hurt a fighter is: the HUD bar, the bot's lamp, its wish for a pad.
    pub full_health: f32,
    /// The most health a fighter can have; pads heal past `full_health` up
    /// to here (`health_cap`).
    pub max_health: f32,
    pub full_armour: f32,
    /// What a respawn hands back. Armour a world issues at spawn is worn
    /// again on the next life; armour picked up off the floor is not, or a
    /// fighter would come back in the shield belt it just lost.
    pub spawn_armour: f32,
    /// Seconds until this body comes back; 0 while alive.
    pub respawn: f32,
    /// Seconds left of the flash of being hit.
    pub hurt: f32,
    /// Where the body was when it went down.
    pub fell: Vec3,
    /// What the killing blow does to the body, metres a second. Read once,
    /// by whoever is stepping the corpse (`shooter::corpse`).
    pub fling: Vec3,
    pub score: Score,
    pub arsenal: Arsenal,
    /// Pads walked over and actually used. Nothing in the rules reads it;
    /// it is in the headless report, because "the bots ignore the floor" is
    /// otherwise an impression rather than a number.
    pub picked: u32,
    /// Who last landed a blow, set by the rules and taken by the brain
    /// (`Me::struck`). `take` is handed damage and not an attacker, so it
    /// is set where the hits are handed out instead.
    pub struck: Option<usize>,
    /// Who last hurt this fighter, and seconds left in which a death from
    /// the ground is still theirs (`BLAME_S`). Knocked into the slag by a
    /// rocket is a kill for whoever fired it, not a fighter who managed it
    /// alone. Unlike `struck`, nothing takes this; it runs out.
    pub blame: Option<(usize, f32)>,
    /// Its side in a team match; None in a deathmatch.
    pub team: Option<u8>,
}

/// Seconds after a hit in which a death from the environment is credited to
/// whoever landed it.
pub const BLAME_S: f32 = 0.5;

impl Fighter {
    pub fn new(arsenal: Arsenal, health: f32, armour: f32) -> Fighter {
        Fighter {
            health,
            armour,
            full_health: health,
            max_health: health,
            full_armour: armour,
            spawn_armour: armour,
            respawn: 0.0,
            hurt: 0.0,
            fell: Vec3::ZERO,
            fling: Vec3::ZERO,
            score: Score::default(),
            arsenal,
            picked: 0,
            struck: None,
            blame: None,
            team: None,
        }
    }

    /// Who a death from the ground is credited to, for fighter `who`: the
    /// last fighter to hurt it while the blame clock runs, else itself.
    pub fn ground_killer(&self, who: usize) -> usize {
        self.blame.map_or(who, |(by, _)| by)
    }

    /// Run the blame clock down; past `BLAME_S` the ground's kills are the
    /// fighter's own again.
    pub fn forget_blame(&mut self, dt: f32) {
        if let Some((_, left)) = &mut self.blame {
            *left -= dt;
            if *left < 0.0 {
                self.blame = None;
            }
        }
    }

    /// How much armour this fighter can wear, which is not how much it
    /// starts in. An arena with armour pads on the floor issues none at
    /// spawn and a capacity to fill from them; one with no pads issues what
    /// it wants worn and leaves the capacity there.
    pub fn armour_cap(&mut self, cap: f32) {
        self.full_armour = cap.max(self.armour);
    }

    /// How much health this fighter can have, which is not how much it
    /// starts with: never less than that.
    pub fn health_cap(&mut self, cap: f32) {
        self.max_health = cap.max(self.full_health);
    }

    pub fn alive(&self) -> bool {
        self.respawn <= 0.0
    }

    /// Damage through the armour: armour soaks up two thirds of what lands
    /// until it is gone. True if that was the killing blow.
    pub fn take(&mut self, dmg: f32) -> bool {
        if dmg <= 0.0 || !self.alive() {
            return false;
        }
        let soaked = (dmg * 0.66).min(self.armour);
        self.armour -= soaked;
        self.health -= dmg - soaked;
        self.hurt = HURT_S;
        self.health <= 0.0
    }

    /// Down: the body is let go at `at`, with `fling` metres a second of
    /// whatever killed it still in it, and comes back in `respawn_s`.
    pub fn die(&mut self, at: Vec3, respawn_s: f32, fling: Vec3) {
        self.health = 0.0;
        self.respawn = respawn_s.max(0.2);
        self.score.deaths += 1;
        self.fell = at;
        self.fling = fling;
    }

    /// Whole again, with the best loaded weapon out.
    ///
    /// `give_all` is the world's choice of respawn. With it, a respawn hands
    /// over every gun, full: an arena with nothing to
    /// pick up has to, or a fighter's second life is worse than its first
    /// and the match decays. Without it, a respawn hands back the sidearm
    /// and nothing else, and the guns are where the map keeps them -- which
    /// is what makes the floor worth crossing (`shooter::pickup`). Keeping
    /// what you were carrying would be the worst of both: whoever was
    /// winning would stay armed through death.
    pub fn revive(&mut self, give_all: bool) {
        self.health = self.full_health;
        self.armour = self.spawn_armour;
        self.respawn = 0.0;
        self.hurt = 0.0;
        // A blow landed on the last life is not news to this one.
        self.struck = None;
        self.blame = None;
        if give_all {
            self.arsenal.refill();
        } else {
            self.arsenal.strip();
        }
        if let Some(i) = self.arsenal.best_loaded() {
            self.arsenal.held = i;
        }
    }

    pub fn fade(&mut self, dt: f32) {
        self.hurt = (self.hurt - dt).max(0.0);
    }
}

/// A bot: the same body the human drives, the rules' half of a fighter, and
/// a brain to fill in the input.
pub struct Bot {
    pub name: String,
    pub body: Player,
    pub fighter: Fighter,
    pub person: person::Personality,
    pub brain: Box<dyn Brain>,
    /// Whose side it is on, as the world or the launch screen set it: the
    /// colour its plates are washed with. Health is not this -- a tint can
    /// only darken, which says nothing once a body has colours of its own --
    /// it is how red the lamps in its chest have gone (`avatar::lamp_colour`).
    pub tint: [f32; 3],
    /// What the brain asked for last step, kept so the body can be stepped
    /// on the fixed tick while the brain thinks on the frame.
    pub pilot: Pilot,
    /// How the body is drawn: the pose its movement puts it in
    /// (`anim::figure`). Display only. Nothing in this module may read it to
    /// decide anything, and it may never write to `body`: a bot moves through
    /// `Player::update` on the same input struct a human fills in, and that
    /// is the whole guarantee that it cannot do what a player could not.
    pub figure: Figure,
    /// Which of the avatar's builds it is drawn as (`avatar::BUILDS`),
    /// picked from its seed and kept for the match.
    pub build: usize,
    /// Times it has found its own box inside solid voxels, and the last
    /// place it happened. A body in that state cannot move at all until
    /// `physics::free_body` shoves it clear, and the thing that puts it
    /// there without anybody noticing is a lift carrying it into a ceiling,
    /// so it is counted and reported rather than guessed at.
    pub buried: u32,
    pub buried_at: Vec3,
    /// Whether it was buried last step, so the count is of separate times
    /// rather than of steps.
    pub was_buried: bool,
    /// The hand that taps a direction key twice when the brain asks for a
    /// dodge (`steer::Tapper`). It lives here rather than in the brain
    /// because it has to be stepped once per body step.
    pub tap: steer::Tapper,
}

impl Bot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(name: &str, body: Player, fighter: Fighter, person: person::Personality, brain: Box<dyn Brain>, tint: [f32; 3], av: &avatar::Avatar, seed: u32) -> Bot {
        let pilot = Pilot { yaw: body.yaw, ..Default::default() };
        Bot {
            name: name.to_string(),
            body,
            fighter,
            person,
            brain,
            tint,
            pilot,
            figure: av.body(Self::build_for(seed, av)).figure(seed),
            build: Self::build_for(seed, av),
            buried: 0,
            buried_at: Vec3::ZERO,
            was_buried: false,
            tap: steer::Tapper::default(),
        }
    }

    /// The build a bot with this seed is drawn as: any of them, evenly.
    fn build_for(seed: u32, av: &avatar::Avatar) -> usize {
        (seed.wrapping_mul(0x9e37_79b9) >> 16) as usize % av.bodies.len()
    }

    /// The body's own numbers. `Bot::body` is a whole `Player`, so these
    /// save every caller writing `b.body.body`.
    pub fn pos(&self) -> Vec3 {
        self.body.pos
    }
    pub fn vel(&self) -> Vec3 {
        self.body.body.vel
    }
    pub fn height(&self) -> f32 {
        self.body.body.height
    }
    pub fn half_w(&self) -> f32 {
        self.body.body.half_w
    }
    pub fn on_ground(&self) -> bool {
        self.body.body.on_ground
    }

    /// Where its shot leaves from: the muzzle of the gun in its hand.
    ///
    /// Not an offset from the eye. The player's shot already leaves the gun
    /// that is drawn rather than the eye that is aiming it, and a bot whose
    /// tracers come from somewhere other than its visible weapon looks broken
    /// in exactly the way the player's did before that was fixed.
    pub fn muzzle(&self, av: &avatar::Avatar, shape: usize) -> Vec3 {
        let (at, yaw, rot) = self.grip(av, 1.0);
        let (sy, cy) = yaw.sin_cos();
        let v = rot * ((crate::combat::viewmodel::muzzle(shape) - Vec3::new(16.0, 0.0, 16.0)) * crate::combat::viewmodel::VOXEL * crate::shooter::BOT_GUN);
        at + Vec3::new(cy, 0.0, -sy) * v.x + Vec3::Y * v.y + Vec3::new(sy, 0.0, cy) * v.z
    }

    /// Where the gun it is holding sits this frame, taken from the hand that
    /// is holding it.
    pub fn grip(&self, av: &avatar::Avatar, alpha: f32) -> (Vec3, f32, glam::Quat) {
        let at = self.body.prev_pos.lerp(self.body.pos, alpha);
        let b = av.body(self.build);
        self.figure.grip(&b.rig, b.voxel, 1.0, b.pivot, at, self.body.yaw, self.body.pitch, alpha)
    }

    /// How much of it is left, for the lamp in its chest.
    pub fn whole(&self) -> f32 {
        (self.fighter.health / self.fighter.full_health.max(1.0)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A death from the ground within half a second of somebody's hit is
    /// theirs; later, or with nobody's hit before it, it is the fighter's
    /// own, and a respawn forgets who it was.
    #[test]
    fn the_ground_kills_for_whoever_hit_you_last() {
        let mut f = Fighter::new(Arsenal { guns: Vec::new(), held: 0 }, 100.0, 0.0);
        assert_eq!(f.ground_killer(3), 3, "nobody hit it and somebody got the kill");
        f.blame = Some((1, BLAME_S));
        for _ in 0..27 {
            f.forget_blame(1.0 / 60.0);
        }
        assert_eq!(f.ground_killer(3), 1, "0.45 s after the hit the kill was not credited");
        for _ in 0..6 {
            f.forget_blame(1.0 / 60.0);
        }
        assert_eq!(f.ground_killer(3), 3, "0.55 s after the hit it was still credited");
        f.blame = Some((1, BLAME_S));
        f.revive(false);
        assert_eq!(f.ground_killer(3), 3, "the blame outlived a respawn");
    }

    /// Armour takes two thirds of what lands, and the last point kills.
    #[test]
    fn armour_soaks_and_the_last_point_kills() {
        let mut f = Fighter::new(Arsenal { guns: Vec::new(), held: 0 }, 100.0, 50.0);
        assert!(!f.take(30.0));
        assert!((f.armour - 30.2).abs() < 0.1, "armour {}", f.armour);
        assert!((f.health - 89.8).abs() < 0.1, "health {}", f.health);
        assert!(f.take(1000.0));
        f.die(Vec3::ZERO, 3.0, Vec3::ZERO);
        assert!(!f.alive() && f.score.deaths == 1);
        f.revive(true);
        assert!(f.alive() && f.health == 100.0 && f.armour == 50.0);
    }
}

#[cfg(test)]
mod tweak_tests {
    use super::*;

    #[test]
    fn a_side_s_role_changes_reach_its_orders_by_name() {
        let (mask, bad) = tweak_mask("attack1, carry2,nonsense");
        assert_eq!(bad, vec!["nonsense".to_string()]);
        let o = Order {
            tweaks: mask,
            ..Order::new(Role::Attack, Goal::Point(glam::Vec3::ZERO), 0.7)
        };
        assert!(o.has("attack1") && o.has("carry2"));
        assert!(!o.has("attack2") && !o.has("nonsense"));
        assert!(!Order::new(Role::Carry, Goal::Point(glam::Vec3::ZERO), 1.0).has("carry2"));
    }
}
