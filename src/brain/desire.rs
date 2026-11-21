//! What a bot wants: the gun for this range, and which fighter to shoot at.
//!
//! Both are additive scores, and both are worked out from the weapon's own
//! file rather than from a table in here, which is what a library-as-data
//! engine wants: the bot does not know what a scrap gun is, it asks each gun
//! it is carrying how much that gun wants to be the one in hand at this
//! range. A new weapon file needs no bot code, and when a weapon file
//! changes -- the scrap gun stopped being a hitscan and became nine flying
//! shards -- the answer changes with it.
//!
//! Pickups rate themselves the same way, on the rules' side of the fence
//! (`shooter::pickup::Pad::good`, handed over as a `brain::Lure`).

use crate::combat::weapon::{Arsenal, Held, Trigger};
use crate::library::{FireDef, FireKind};

/// How wide a body is to shoot at, metres of radius. A cone narrower than
/// this lands everything it throws; a wider one wastes most of it.
const TARGET_R: f32 = 0.45;

/// What share of `pellets` lands on a body at `dist`, given a cone of
/// `spread_deg`. One pellet with no spread is all of it; nine shards at
/// fifty metres is nearly none.
fn landing(pellets: u32, spread_deg: f32, ring: bool, dist: f32) -> f32 {
    let n = pellets.max(1) as f32;
    let cone = dist * spread_deg.to_radians().tan();
    if cone <= TARGET_R {
        return n;
    }
    // A ring past the body's width lands only the arc that crosses it.
    if ring {
        return n * (2.0 * TARGET_R / (std::f32::consts::PI * cone)).min(1.0);
    }
    n * (TARGET_R / cone).powi(2)
}

/// Shots a second this mode really manages, which for a magazine weapon is
/// the reload and not the rate of fire: the rocket launcher goes at 240 rpm
/// and holds one round, so it is a shot every one and a bit seconds. A
/// charging mode pays its `charge_s` before every shot as well, which is
/// most of what a charge costs. A burst mode's pause between bursts is
/// shared out over the rounds in each burst.
fn cadence(g: &Held, f: &FireDef) -> f32 {
    let pause = if f.burst > 1 { f.burst_gap_s.max(0.0) / f.burst as f32 } else { 0.0 };
    let gap = 1.0 / (f.rpm / 60.0).max(0.01) + f.charge_s.max(0.0) + pause;
    if g.def.mag > 0 && g.def.reload_s > 0.0 {
        let shots = (g.def.mag / f.per_shot.max(1)).max(1) as f32;
        shots / (shots * gap + g.def.reload_s)
    } else {
        1.0 / gap
    }
}

/// How far a shot of this kind can reach at all.
fn reach(f: &FireDef) -> f32 {
    match f.kind {
        // A beam is a ray that stays on, so it reaches exactly as far as a
        // ray does.
        FireKind::Hitscan | FireKind::Beam => f.range,
        // A shell is limited by its fuse, not by a range field: nine shards
        // at 62 m/s on a 0.85 s fuse are gone by fifty metres.
        FireKind::Shell if f.fuse_s > 0.0 => f.speed * f.fuse_s,
        FireKind::Shell => f32::MAX,
    }
}

/// How far the gun in hand works at all: the longer of its two modes'
/// reaches. A bot stands where its weapon can touch somebody, and this is
/// the only thing that tells it where that is.
pub fn carry(g: &Held) -> f32 {
    let fire = reach(&g.def.fire);
    match g.def.alt.as_ref() {
        Some(a) => fire.max(reach(a)),
        _ => fire,
    }
}

/// How much this gun wants to be out at `dist` metres: whichever of its two
/// buttons is worth more there, since a bot may press either.
///
/// `accuracy` is how well the bot shoots, 0 to 1. It is here because it is
/// what separates a gun that has to connect from one that only has to land
/// nearby: a poor shot is right to prefer splash, and a good one is right to
/// prefer the gun that rewards aim. Nothing else in the scoring needs to
/// know anything about who is asking.
pub fn rating(g: &Held, dist: f32, accuracy: f32) -> f32 {
    let fire = mode_rating(g, &g.def.fire, dist, accuracy);
    match alt_of(g) {
        Some(a) => fire.max(mode_rating(g, a, dist, accuracy)),
        None => fire,
    }
}

/// The second fire mode a bot may actually press, which is not the same as
/// `Held::mode(Alt)`: a gun with no `alt` falls back to its primary there,
/// which is not a choice worth weighing, and a sight with no `alt` only
/// narrows the view. A sight that charges is an `alt` like any other.
fn alt_of(g: &Held) -> Option<&FireDef> {
    g.def.alt.as_ref()
}

/// Which button to hold on this gun at `dist`. `held` is the one it is
/// already on, because switching button mid-burst for a hair of score is the
/// same flapping that `best` guards weapon choice against; `margin` is how
/// much better the other one has to be.
///
/// Nothing in here knows which gun it is holding. A gun whose right button is
/// worth more at this range gets the right button. A sight with no `alt`
/// gets the left one always, because its right button fires nothing.
pub fn button(g: &Held, dist: f32, accuracy: f32, held: Trigger, margin: f32) -> Trigger {
    let Some(alt) = alt_of(g) else { return Trigger::Fire };
    let fire = mode_rating(g, &g.def.fire, dist, accuracy);
    let alt = mode_rating(g, alt, dist, accuracy);
    match held {
        Trigger::Alt => {
            if fire > alt * (1.0 + margin) {
                Trigger::Fire
            } else {
                Trigger::Alt
            }
        }
        _ => {
            if alt > fire * (1.0 + margin) {
                Trigger::Alt
            } else {
                Trigger::Fire
            }
        }
    }
}

/// How much one fire mode of a gun is worth at `dist` metres.
pub fn mode_rating(g: &Held, f: &FireDef, dist: f32, accuracy: f32) -> f32 {
    if g.empty() || g.reloading() {
        return 0.0;
    }
    if dist > reach(f) {
        return 0.0;
    }
    // A mode that costs more rounds than the gun has left is not an option,
    // whatever it would be worth.
    if !g.unlimited() && g.in_gun() < f.per_shot {
        return 0.0;
    }
    let land = landing(f.pellets, f.spread_deg, f.ring, dist);
    // What a bot of this skill actually gets on the body: a third of it for
    // free, the rest bought with aim.
    let aim = accuracy.clamp(0.0, 1.0);
    let hits = land * (0.35 + 0.65 * aim);
    // A gun that pays for a head shot is worth more to a bot that can find
    // one, and nothing extra to a bot that cannot: half the bonus at a
    // perfect aim, none at no aim.
    let hits = hits * (1.0 + (f.head_mult - 1.0) * aim * 0.5);
    // What a burst is worth before anybody has to aim. It is credited by
    // its radius rather than flat: a burst only pays at all if the shell
    // lands inside it, and a five metre rocket forgives a miss that a three
    // metre disc does not.
    let burst = |s: &crate::library::SplashDef| s.damage * 2.0 / (1.0 + s.curve) * (0.3 + 0.09 * s.radius).min(0.85);
    let splash = f.splash.as_ref().map_or(0.0, burst);
    // What it breaks into where it lands: a handful of little bursts strewn
    // about, worth the same way a burst is and damped for the same reason --
    // ten of them on one man is not ten men's worth of damage. They are not
    // charged for him moving, as a direct hit is: the whole of what a
    // cluster does is cover the ground he is moving over.
    let cluster = f.cluster.as_ref().map_or(0.0, |c| {
        let one = c.damage * 0.25 + c.splash.as_ref().map_or(0.0, burst);
        one * (c.count as f32).sqrt()
    });
    let per_shot = match f.kind {
        FireKind::Hitscan | FireKind::Beam => {
            // A ray loses damage down its range.
            let k = (dist / f.range.max(0.01)).clamp(0.0, 1.0);
            f.damage * (1.0 - k * (1.0 - f.falloff)) * hits + splash + cluster
        }
        FireKind::Shell => {
            let flight = dist / f.speed.max(1.0);
            // A shell that drops has to be lobbed, and the further it falls
            // on the way the finer the arc has to be: a grenade thrown fifty
            // metres falls twenty of them, which is a mortar shot at a man
            // who is walking. Without this a lobbing alt mode outrates every
            // gun in the arsenal across the hall.
            let drop = 0.5 * f.gravity * flight * flight;
            let arc = 1.0 + flight * 0.8 + drop * 0.8;
            // Direct damage has to be led onto a man who is moving, and a
            // sixth of a second is long enough for him to be somewhere
            // else. The bench measured the pulse gun's bolts landing on 7%
            // of the shots a ray out of the same gun landed on everything
            // it was pointed at, with the old flight term charging them
            // thirteen per cent for it. Splash does not need the shot to
            // connect at all, so it pays only the arc.
            let dodge = 1.0 + flight * 4.0 + drop * 0.8;
            // A burst still has to land near him, which is most of the way
            // to landing on him: `arc` alone said a shell nobody could
            // possibly be standing next to by the time it arrived was worth
            // its full burst.
            let near = arc + flight * 1.7;
            f.damage * hits / dodge + splash / near + cluster / arc
        }
    };
    // Damage a second, which is the one currency every kind can be compared
    // in. A beam's `damage` is already per second (`Combat::hold_beam`
    // scales the ray by `dt`), so charging it a cadence as well would count
    // the same seconds twice -- and comparing it raw against a weapon whose
    // damage is per round undervalues it by the rate of fire, which for a
    // rifle at 180 rpm is a factor of three.
    // A charged shot is worth its multiplier, and `cadence` has already
    // charged it the wait.
    let per_shot = per_shot * if f.charge_s > 0.0 { f.charge_mult.max(1.0) } else { 1.0 };
    let mut score = if f.kind == FireKind::Beam { per_shot } else { per_shot * cadence(g, f) };
    // Hurting yourself with your own splash: the closer the target, the
    // worse an idea it is. This is what stops a bot rocketing its own feet.
    if let Some(sp) = &f.splash {
        // A burst that cannot hurt the man who set it off is no reason to
        // keep your distance: that is the Digger, whose whole use is
        // at arm's length inside its own blast.
        if sp.self_damage > 0.0 && dist < sp.radius * 1.25 {
            score *= (dist / (sp.radius * 1.25)).max(0.05);
        }
    }
    // A gun that has to spin up is worth less in a snap fight.
    if f.spin_up > 0.0 && dist > 25.0 {
        score *= 0.9;
    }
    score
}

/// Which gun to have out at `dist`, if a better one than the one in hand is
/// carried. `margin` is how much better it has to be before the bot bothers
/// switching, which is hysteresis: without it a bot at the crossover range
/// swaps weapons every step and never fires.
pub fn best(a: &Arsenal, dist: f32, margin: f32, accuracy: f32) -> Option<usize> {
    let have = a.current().map_or(0.0, |g| rating(g, dist, accuracy));
    let mut pick = (a.held, have * (1.0 + margin));
    for (i, g) in a.guns.iter().enumerate() {
        let r = rating(g, dist, accuracy);
        if r > pick.1 {
            pick = (i, r);
        }
    }
    (pick.0 != a.held).then_some(pick.0)
}

/// Which fighter to shoot at: a small sum of terms. `current` is who it is already fighting, and switching away
/// costs something, so a bot between two targets does not twitch between
/// them and hit neither.
pub struct Candidate {
    pub who: usize,
    pub dist: f32,
    /// Share of their health still left, if the bot has any way of knowing;
    /// 1 when it does not.
    pub whole: f32,
    pub in_sight: bool,
    /// Seconds since last seen.
    pub stale: f32,
}

pub fn threat(c: &Candidate, current: Option<usize>) -> f32 {
    let mut v = 0.0;
    // Close is dangerous and also easy.
    v += (1.0 - (c.dist / 60.0).min(1.0)) * 0.6;
    // Nearly dead is worth finishing.
    v += (1.0 - c.whole) * 0.4;
    if c.in_sight {
        v += 0.5;
    } else {
        v -= (c.stale / super::senses::REMEMBER_S).min(1.0) * 0.4;
    }
    // Hysteresis: it takes this much to be worth turning away from the one
    // it is already fighting.
    if current == Some(c.who) {
        v += 0.25;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;
    use std::sync::Arc;

    fn held(lib: &Library, name: &str) -> Held {
        let i = lib.weapon(name).unwrap_or_else(|| panic!("no weapon {name}"));
        Held::new(Arc::new(lib.weapons[i].clone()), true)
    }

    /// The real library's three guns pick themselves for the range they are
    /// for: the scrap gun in a knife fight, the minigun down the hall, and the
    /// launcher never at point blank.
    #[test]
    fn each_gun_wants_its_own_range() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let scrap = held(&lib, "scrap_gun");
        let gun = held(&lib, "minigun");
        let rocket = held(&lib, "rocket_launcher");
        let a = 0.5;
        assert!(rating(&scrap, 4.0, a) > rating(&gun, 4.0, a), "the minigun won a knife fight");
        assert!(rating(&gun, 50.0, a) > rating(&scrap, 50.0, a), "the scrap gun won at fifty metres");
        assert!(rating(&rocket, 2.0, a) < rating(&rocket, 20.0, a) * 0.75, "it is happy to rocket its own feet");
        assert_eq!(rating(&gun, 400.0, a), 0.0, "a ray reached past its range");
        assert_eq!(rating(&scrap, 80.0, a), 0.0, "a shard outlived its fuse");
    }

    /// The arsenal's ten guns each want a range, and the two ends of it are
    /// the test: the hammer is worthless at anything but arm's length, and
    /// the guns that reach are the only ones still worth anything across the
    /// arena.
    #[test]
    fn the_whole_arsenal_sorts_itself_by_range() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let a = 0.6;
        let hammer = held(&lib, "digger");
        let sidearm = held(&lib, "sidearm");
        let sniper = held(&lib, "sniper_rifle");
        let scrap = held(&lib, "scrap_gun");
        let skipjack = held(&lib, "skipjack");
        let pulse = held(&lib, "pulse_gun");
        let goo = held(&lib, "goo_gun");
        assert!(rating(&hammer, 2.0, a) > rating(&sidearm, 2.0, a), "the sidearm won a brawl");
        assert_eq!(rating(&hammer, 6.0, a), 0.0, "the hammer reached six metres");
        assert!(rating(&scrap, 2.0, a) > rating(&hammer, 2.0, a), "the hammer beat a shell of scrap point blank");
        assert!(rating(&sniper, 90.0, a) > rating(&pulse, 90.0, a), "the pulse gun won across the hall");
        assert!(rating(&skipjack, 40.0, a) > rating(&scrap, 40.0, a), "scrap carried forty metres");
        assert!(rating(&goo, 5.0, a) > rating(&goo, 60.0, a), "a lobbed glob is as good far off as near");
        // Every gun in the library is worth something somewhere, which is
        // the check that a new weapon file has not been written unusable.
        for w in &lib.weapons {
            let g = held(&lib, &w.name);
            let best = [2.0f32, 5.0, 12.0, 30.0, 80.0].iter().fold(0.0f32, |m, d| m.max(rating(&g, *d, a)));
            assert!(best > 10.0, "{} is not worth carrying at any range", w.name);
        }
    }

    /// The right button is chosen where it is worth more, and the choice is
    /// made from the weapon files: anybody takes the scrap gun's lobbed
    /// shell over a cone of shards at fifteen metres, and nobody lobs the
    /// skipjack's cluster down a hall.
    #[test]
    fn the_right_button_is_pressed_where_it_pays() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let m = 0.25;
        let scrap = held(&lib, "scrap_gun");
        assert_eq!(button(&scrap, 3.0, 0.8, Trigger::Fire, m), Trigger::Fire, "it lobbed a shell point blank");
        assert_eq!(button(&scrap, 15.0, 0.8, Trigger::Fire, m), Trigger::Alt, "it threw shards fifteen metres");
        // The skipjack's right button is a shell lobbed into a room, which
        // breaks into ten little bouncing ones: it is worth something at
        // the far end of a room and nothing at the far end of the arena,
        // and down a hall the single skipping disc is the shot.
        let skipjack = held(&lib, "skipjack");
        let lob = skipjack.def.alt.as_ref().unwrap();
        let near = mode_rating(&skipjack, lob, 8.0, 0.1);
        let far = mode_rating(&skipjack, lob, 40.0, 0.1);
        assert!(near > far * 4.0, "the lob was as good across the arena ({far:.1}) as in a room ({near:.1})");
        assert_eq!(button(&skipjack, 40.0, 0.1, Trigger::Fire, m), Trigger::Fire, "it lobbed a cluster down a hall");
    }

    /// A sight with no `alt` is always fired on the left: the right button
    /// puts nothing in the air to be worth anything. The sniper rifle's
    /// sight charges, so its right button is weighed like any other: the
    /// charge costs more time than its multiplier pays back, so a bot on the
    /// plain shot stays on it at every range.
    #[test]
    fn a_sight_gets_the_right_button_only_if_it_charges() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let rifle = held(&lib, "sniper_rifle");
        assert!(rifle.def.zoom > 0.0, "the sniper rifle stopped having a sight");
        let charge = rifle.def.alt.as_ref().expect("the sniper rifle's sight stopped charging");
        assert!(charge.charge_s > 0.0, "the sniper rifle's alt does not charge");
        for d in [2.0f32, 20.0, 200.0] {
            for aim in [0.0f32, 1.0] {
                assert!(mode_rating(&rifle, charge, d, aim) > 0.0, "the charged shot is worth nothing at {d} m with aim {aim}");
                assert_eq!(button(&rifle, d, aim, Trigger::Fire, 0.25), Trigger::Fire, "at {d} m with aim {aim}");
            }
        }
        let mut def = (*rifle.def).clone();
        def.alt = None;
        let bare = Held::new(Arc::new(def), true);
        for d in [2.0f32, 20.0, 200.0] {
            assert_eq!(button(&bare, d, 1.0, Trigger::Alt, 0.25), Trigger::Fire, "a bare sight got the right button at {d} m");
        }
    }

    /// Switching buttons takes a margin, the same way switching weapons
    /// does: a finger that flutters between two modes loses a spin-up or a
    /// charge every time it moves.
    #[test]
    fn it_takes_a_margin_to_change_buttons() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let scrap = held(&lib, "scrap_gun");
        // A range where the two modes are close: the one already held wins.
        let near = (5..40)
            .map(|d| d as f32)
            .find(|d| {
                let f = mode_rating(&scrap, &scrap.def.fire, *d, 0.6);
                let a = mode_rating(&scrap, scrap.def.alt.as_ref().unwrap(), *d, 0.6);
                a > f && a < f * 1.25
            })
            .expect("no range where the scrap gun's two modes are within the margin");
        assert_eq!(button(&scrap, near, 0.6, Trigger::Fire, 0.25), Trigger::Fire, "at {near} m");
        assert_eq!(button(&scrap, near, 0.6, Trigger::Alt, 0.25), Trigger::Alt, "at {near} m");
    }

    /// A gun that pays for a head shot is worth more to a bot that can aim.
    #[test]
    fn a_head_shot_is_worth_more_to_a_good_shot() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let rifle = held(&lib, "sniper_rifle");
        assert!(rifle.def.fire.head_mult > 1.0, "the sniper rifle stopped rewarding a head shot");
        let good = rating(&rifle, 60.0, 1.0);
        let bad = rating(&rifle, 60.0, 0.0);
        assert!(good > bad * 1.5, "aim bought nothing with it: {good} against {bad}");
    }

    /// A poor shot prefers splash and a good one prefers the gun that
    /// rewards aim, which falls out of the scoring rather than being a rule.
    #[test]
    fn a_poor_shot_prefers_splash() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let gun = held(&lib, "minigun");
        let rocket = held(&lib, "rocket_launcher");
        let far = 22.0;
        let bad = rating(&rocket, far, 0.0) / rating(&gun, far, 0.0);
        let good = rating(&rocket, far, 1.0) / rating(&gun, far, 1.0);
        assert!(bad > good, "aim bought nothing: {bad} vs {good}");
    }

    /// A magazine and a reload are most of what a weapon really fires: the
    /// launcher's 240 rpm is a shot every second and a bit.
    #[test]
    fn a_reload_is_the_real_cadence() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let rocket = held(&lib, "rocket_launcher");
        let belt = held(&lib, "minigun");
        assert!(rocket.def.mag > 0 && rocket.def.reload_s > 0.0, "the launcher stopped having a magazine");
        let c = cadence(&rocket, &rocket.def.fire);
        assert!(c > 0.5 && c < 1.2, "the launcher thinks it fires {c} a second");
        assert!((cadence(&belt, &belt.def.fire) - belt.def.fire.rpm / 60.0).abs() < 1e-3, "the belt-fed gun was charged a reload");
    }

    /// A gun being reloaded is worth nothing, so a bot caught mid-reload
    /// reaches for something else.
    #[test]
    fn a_reloading_gun_is_worth_nothing() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let mut rocket = held(&lib, "rocket_launcher");
        let before = rating(&rocket, 20.0, 0.5);
        assert!(before > 0.0);
        rocket.loaded = 0;
        assert!(rocket.start_reload(), "it would not reload an empty magazine");
        assert_eq!(rating(&rocket, 20.0, 0.5), 0.0);
    }

    /// Switching needs a margin, so a bot at the crossover does not twitch.
    #[test]
    fn it_takes_a_margin_to_switch() {
        let (lib, _) = Library::load(std::path::Path::new("."));
        let mut a = Arsenal::new(&lib, &["minigun".into(), "scrap_gun".into()], true, |n| panic!("{n}"));
        a.held = 0;
        // Far off the minigun is right, so nothing to do; close up it is not.
        assert_eq!(best(&a, 60.0, 0.25, 0.5), None);
        assert!(best(&a, 3.0, 0.25, 0.5).is_some(), "it kept the minigun in a knife fight");
    }

    /// The one it is already fighting wins a close call, and a target it
    /// cannot see loses ground the longer it has been gone.
    #[test]
    fn the_target_it_has_wins_a_close_call() {
        let a = Candidate {
            who: 1,
            dist: 20.0,
            whole: 1.0,
            in_sight: true,
            stale: 0.0,
        };
        let b = Candidate { who: 2, ..a };
        assert!(threat(&a, Some(1)) > threat(&b, Some(1)));
        let gone = Candidate { in_sight: false, stale: 5.0, ..a };
        assert!(threat(&gone, None) < threat(&a, None) - 0.5);
    }
}
