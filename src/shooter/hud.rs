//! The shooter's HUD: health, armour, the gun in hand, the crosshair, who
//! killed whom, and the score.
//!
//! It reads a `Shooter` and draws; it changes nothing. Every readout goes
//! through `render::ui`, which hands out one row of a corner at a time, so
//! no two of them can land on each other and none of the positions here are
//! arithmetic anybody has to redo when a line is added. Only the crosshair
//! and the full-screen flashes are drawn straight onto the `Hud`, because
//! neither of them is in a corner.

use super::round::TEAM_INK;
use super::Shooter;
use crate::brain::Score;
use crate::combat::viewmodel;
use crate::render::hud::Hud;
use crate::render::ui::{Corner, Side, Tint};

/// Health, and the share of it under which the bar turns red.
const HEALTH: Tint = Tint {
    fill: [0.85, 0.9, 0.9, 0.85],
    warn: [0.9, 0.25, 0.2, 0.9],
    low: 0.3,
    ink: [1.0, 0.97, 0.94, 0.9],
};
const ARMOUR: Tint = Tint {
    fill: [0.45, 0.65, 0.85, 0.85],
    warn: [0.45, 0.65, 0.85, 0.85],
    low: 0.0,
    ink: [0.92, 0.96, 1.0, 0.8],
};
/// The boost: the same amber as a gun winding up, because both of them are
/// a thing being spent.
const BOOST: Tint = Tint {
    fill: [1.0, 0.8, 0.3, 0.85],
    warn: [0.9, 0.4, 0.3, 0.9],
    low: 0.15,
    ink: [1.0, 0.95, 0.9, 0.9],
};
const AMMO: Tint = Tint {
    fill: [1.0, 0.8, 0.45, 0.7],
    warn: [0.9, 0.4, 0.3, 0.8],
    low: 0.2,
    ink: [1.0, 0.95, 0.9, 0.95],
};

pub fn draw(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let line = 10.0 * s;
    if sh.me.hurt > 0.0 {
        let a = (sh.me.hurt / crate::brain::HURT_S) * 0.32;
        hud.rect(0.0, 0.0, w, h, [0.7, 0.05, 0.04, a]);
    }
    carrying(sh, hud, w, h, s);
    crosshair(sh, hud, w, h, s);
    banner(sh, hud, w, h, s);
    standing(sh, hud, w, h, s);
    alert(sh, hud, w, h, s);
    score(sh, hud, w, h, s);
    if sh.zed.is_some() {
        zombie_round(sh, hud, w, h, s);
        if !sh.alive() {
            return;
        }
    }

    if !sh.alive() {
        let (cx, cy) = ((w * 0.5).floor(), (h * 0.5).floor());
        let msg = "YOU DIED";
        let wmsg = Hud::text_width(msg, s * 3.0);
        hud.label(cx - wmsg * 0.5, cy - line * 4.0, s * 3.0, [1.0, 0.86, 0.8, 0.95], msg);
        let back = format!("BACK IN {:.1}", sh.me.respawn);
        let wb = Hud::text_width(&back, s);
        hud.label(cx - wb * 0.5, cy - line * 2.0, s, [1.0, 0.9, 0.85, 0.8], &back);
        return;
    }

    vitals(sh, hud, w, h, s);
    weapon(sh, hud, w, h, s);
}

/// Four ticks round a gap that opens with the weapon's spread, and turns
/// when something was hit. Down a sight it is hairlines across the whole
/// frame instead, which is the one readout that says the view is narrowed.
fn crosshair(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let (cx, cy) = ((w * 0.5).floor(), (h * 0.5).floor());
    if sh.sighted {
        let t = (1.0 * s).max(1.0);
        let c = if sh.mark > 0.0 { [1.0, 0.45, 0.3, 0.9] } else { [0.86, 0.9, 0.88, 0.5] };
        let gap = 8.0 * s;
        // Two hairlines with a hole in the middle, so the thing being aimed
        // at is not under a line.
        hud.rect(0.0, cy - t * 0.5, cx - gap, t, c);
        hud.rect(cx + gap, cy - t * 0.5, w - cx - gap, t, c);
        hud.rect(cx - t * 0.5, 0.0, t, cy - gap, c);
        hud.rect(cx - t * 0.5, cy + gap, t, h - cy - gap, c);
        // Range ticks down the lower hairline, which is what a scope has.
        for k in 1..5 {
            let y = cy + gap + k as f32 * 16.0 * s;
            let wide = (10.0 - k as f32 * 1.5) * s;
            hud.rect(cx - wide * 0.5, y, wide, t, c);
        }
        // The frame darkens at the edges: a sight is a tube.
        let band = 14.0 * s;
        let dim = [0.0, 0.0, 0.0, 0.35];
        hud.rect(0.0, 0.0, w, band, dim);
        hud.rect(0.0, h - band, w, band, dim);
        hud.rect(0.0, 0.0, band, h, dim);
        hud.rect(w - band, 0.0, band, h, dim);
        return;
    }
    // The spread a shot would go out with now: the gun's own, what the burst
    // has added, and what moving adds where the game says so.
    let (vel, ground) = sh.me_motion;
    let spread = sh.me.arsenal.current().map_or(1.0, |g| {
        let f = &g.def.fire;
        1.0 + (f.spread_deg + g.bloom() + f.move_spread.map_or(0.0, |m| m.degrees(vel, ground))) * 0.9
    });
    let gap = (3.0 * s * spread).min(w * 0.08);
    let len = 5.0 * s;
    let t = (1.0 * s).max(1.0);
    let c = if sh.mark > 0.0 { [1.0, 0.45, 0.3, 0.95] } else { [0.92, 0.95, 0.96, 0.72] };
    hud.rect(cx - gap - len, cy - t * 0.5, len, t, c);
    hud.rect(cx + gap, cy - t * 0.5, len, t, c);
    hud.rect(cx - t * 0.5, cy - gap - len, t, len, c);
    hud.rect(cx - t * 0.5, cy + gap, t, len, c);
    marker(sh, hud, cx, cy, s);
}

/// The hit marker: four diagonal dashes round the crosshair, longer for a
/// bigger hit and red for a head shot, opening out as they fade. The
/// killing blow is a bigger, thicker X that holds a moment longer.
fn marker(sh: &Shooter, hud: &mut Hud, cx: f32, cy: f32, s: f32) {
    if sh.mark_kill > 0.0 {
        let k = sh.mark_kill / super::KILL_MARK_S;
        // Pops in large and settles.
        let pop = 1.0 + 0.5 * (k - 0.7).max(0.0) / 0.3;
        let c = [1.0, 0.3, 0.2, 0.25 + 0.75 * k];
        diagonals(hud, cx, cy, 7.0 * s * pop, 17.0 * s * pop, 2.2 * s, c);
        diagonals(hud, cx, cy, 8.0 * s * pop, 15.0 * s * pop, 1.0 * s, [1.0, 1.0, 1.0, k]);
        return;
    }
    if sh.mark <= 0.0 {
        return;
    }
    let k = (sh.mark / super::MARK_S).min(1.0);
    let size = sh.mark_size.clamp(0.25, 1.0);
    let open = (1.0 - k) * 3.0 * s;
    let inner = 6.0 * s + open;
    let outer = inner + (4.0 + 8.0 * size) * s;
    let c = if sh.mark_head { [1.0, 0.25, 0.2, k] } else { [1.0, 1.0, 1.0, 0.9 * k] };
    diagonals(hud, cx, cy, inner, outer, (1.0 + 0.8 * size) * s, c);
}

/// Four dashes on the diagonals from `inner` to `outer` pixels out from
/// (`cx`, `cy`), `t` thick. The HUD draws only upright rectangles, so each
/// dash is a run of small squares.
fn diagonals(hud: &mut Hud, cx: f32, cy: f32, inner: f32, outer: f32, t: f32, c: [f32; 4]) {
    let t = t.max(1.0);
    let step = (t * 0.6).max(1.0);
    let mut r = inner;
    while r <= outer {
        let d = r * std::f32::consts::FRAC_1_SQRT_2;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            hud.rect(cx + sx * d - t * 0.5, cy + sy * d - t * 0.5, t, t, c);
        }
        r += step;
    }
}

/// The kill banner, a third of the way down the screen so it stays clear of
/// the crosshair: who you killed, popping in large and settling, with the
/// streak under it from the second kill on. Gone in `BANNER_S`.
fn banner(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let Some((name, streak, age)) = &sh.banner else { return };
    let age = *age;
    let fade = ((super::BANNER_S - age) / 0.4).clamp(0.0, 1.0);
    let pop = 1.0 + 0.4 * (1.0 - age / 0.12).max(0.0);
    let cx = (w * 0.5).floor();
    let y = (h * 0.30).floor();
    let msg = format!("KILLED {name}");
    let k = s * 1.8 * pop;
    let wm = Hud::text_width(&msg, k);
    hud.label(cx - wm * 0.5 + s, y + s, k, [0.0, 0.0, 0.0, 0.5 * fade], &msg);
    hud.label(cx - wm * 0.5, y, k, [1.0, 0.86, 0.42, fade], &msg);
    let tag = match streak {
        0 | 1 => return,
        2 => "DOUBLE KILL",
        3 => "TRIPLE KILL",
        _ => "MULTI KILL",
    };
    let k = s * 1.2 * pop;
    let wt = Hud::text_width(tag, k);
    let ty = y + 26.0 * s;
    hud.label(cx - wt * 0.5 + s, ty + s, k, [0.0, 0.0, 0.0, 0.5 * fade], tag);
    hud.label(cx - wt * 0.5, ty, k, [1.0, 0.55, 0.3, fade], tag);
}

/// Health and armour, bottom left: the armour's bar lowest, then the health
/// bar with its count over it.
fn vitals(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let mut c = Corner::new(Side::BottomLeft, w, h, s);
    // The boost lowest of the three, because it is the one that refills by
    // itself and so the one you glance at least.
    if let Some((left, spending)) = sh.boost {
        let t = if spending { Tint::solid([1.0, 0.86, 0.45, 0.95]) } else { BOOST };
        c.meter(hud, left, 0.4, t);
    }
    if sh.me.full_armour > 0.0 {
        let af = sh.me.armour / sh.me.full_armour.max(1.0);
        c.meter(hud, af, 0.4, ARMOUR);
    }
    let frac = sh.me.health / sh.me.full_health.max(1.0);
    c.gauge(hud, &format!("{:.0}", sh.me.health.max(0.0)), "", frac, HEALTH);
}

/// The gun in hand, bottom right: the rounds in it, its name, and under
/// both whatever the gun is doing to itself -- winding up, or reloading.
fn weapon(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let Some(g) = sh.me.arsenal.current() else { return };
    let mut c = Corner::new(Side::BottomRight, w, h, s);
    // A reload is the gun's whole business while it lasts, so it takes the
    // bar; the minigun's barrels have it the rest of the time.
    if g.reloading() {
        c.meter(hud, g.reload_frac(), 0.5, Tint::solid([1.0, 0.72, 0.4, 0.85]));
        c.text(hud, 1.0, [1.0, 0.86, 0.7, 0.75], "RELOADING");
    } else if g.charge > 0.01 {
        // A charge building: its own colour, because what it means is the
        // opposite of a reload -- waiting is buying something.
        c.meter(hud, g.charge, 0.5, Tint::solid([0.62, 0.95, 0.35, 0.9]));
    } else if g.def.fire.spin_up > 0.0 && g.spin > 0.01 {
        c.meter(hud, g.spin, 0.4, Tint::solid([1.0, 0.8, 0.45, 0.7]));
    }
    let name = g.def.display.to_uppercase();
    // A magazine weapon counts what is in the gun and what is left behind
    // it; one fed from a belt has only the one number, and one with nothing
    // in it to run out of has none.
    let rounds = if g.unlimited() {
        "--".to_string()
    } else if g.def.mag > 0 {
        format!("{} / {}", g.in_gun(), g.ammo.saturating_sub(g.in_gun()))
    } else {
        format!("{}", g.ammo)
    };
    c.readout(hud, &rounds, &name, AMMO);
    // A weapon with no view-model of its own still has its flat silhouette.
    if viewmodel::shape_of(&g.def.view).is_none() {
        let pw = 150.0 * s;
        let ph = 72.0 * s;
        let r = c.row(pw, ph);
        let [red, green, blue] = g.def.tint;
        for bx in &g.def.grip {
            // The boxes are shares of the panel, x right from its left edge
            // and y up from its bottom, so the row's own height is what
            // turns one into pixels.
            hud.rect(r.x + bx[0] * r.w, r.y + (1.0 - bx[1] - bx[3]) * r.h, bx[2] * r.w, bx[3] * r.h, [red, green, blue, 0.92]);
        }
    }
}

/// Everyone in the match, best first, with the kill feed under them. Both
/// are rows of the same stack, so the feed cannot run into the score however
/// many fighters there are -- which is what it used to do the moment a third
/// bot was added.
fn score(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let mut c = Corner::new(Side::TopRight, w, h, s);
    let mut rows: Vec<(String, Score, Option<u8>)> = vec![("YOU".into(), sh.me.score, sh.me.team)];
    for (i, b) in sh.bots.iter().enumerate() {
        // The zombie round lists the party, not forty zombies.
        if sh.is_zombie(i + 1) {
            continue;
        }
        let name = if sh.zed.is_some() && !b.fighter.alive() { format!("{} (DOWN)", b.name.to_uppercase()) } else { b.name.to_uppercase() };
        rows.push((name, b.fighter.score, b.fighter.team));
    }
    // In a team match the player's side first, each side best first.
    let mine = sh.me.team;
    rows.sort_by_key(|(_, sc, t)| (mine.is_some() && *t != mine, std::cmp::Reverse(sc.caps), std::cmp::Reverse(sc.kills)));
    for (name, sc, team) in &rows {
        let ink = match team {
            Some(t) => {
                let k = TEAM_INK[(*t as usize).min(1)];
                [k[0], k[1], k[2], 0.75]
            }
            None => [1.0, 0.96, 0.92, 0.6],
        };
        c.text(hud, 1.0, ink, &format!("{name}  {}-{}", sc.kills, sc.deaths));
    }
    c.gap(0.6);
    for (msg, left) in &sh.feed {
        let a = (left / 0.8).min(1.0) * 0.85;
        c.text(hud, 1.0, [1.0, 0.9, 0.82, a], msg);
    }
}

/// How long an alert is up, seconds.
pub(crate) const ALERT_S: f32 = 2.6;

/// The newest flag alert, across the top of the screen in the colour of the
/// side it is about: YOUR FLAG HAS BEEN TAKEN and the rest (`Shooter::heard_flag`).
fn alert(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let Some((msg, ink, age)) = sh.alerts.last() else { return };
    if *age > ALERT_S {
        return;
    }
    let fade = ((ALERT_S - age) / 0.5).clamp(0.0, 1.0);
    let pop = 1.0 + 0.3 * (1.0 - age / 0.15).max(0.0);
    let k = s * 2.0 * pop;
    let wm = Hud::text_width(msg, k);
    let (cx, y) = ((w * 0.5).floor(), (h * 0.17).floor());
    hud.label(cx - wm * 0.5 + s, y + s, k, [0.0, 0.0, 0.0, 0.55 * fade], msg);
    hud.label(cx - wm * 0.5, y, k, [ink[0], ink[1], ink[2], fade], msg);
}

/// Top middle, always up in a team match: each side's score and, in capture
/// the flag, where each flag is, with the clock between them.
fn standing(sh: &Shooter, hud: &mut Hud, w: f32, _h: f32, s: f32) {
    let Some(_) = sh.me.team else { return };
    // The zombie round puts the party on a side without there being a
    // second one to count against.
    if sh.zed.is_some() {
        return;
    }
    let ctf = !sh.flags.is_empty();
    let totals = if ctf { sh.flags.caps } else { super::round::team_kills(&sh.scores(), &sh.sides()) };
    let y = (8.0 * s).floor();
    let cx = (w * 0.5).floor();
    let big = s * 1.8;
    let middle = match sh.round.left() {
        Some(t) => format!("{}:{:02}", (t / 60.0) as u32, (t % 60.0) as u32),
        None => "-".to_string(),
    };
    let wm = Hud::text_width(&middle, s * 1.2);
    hud.label(cx - wm * 0.5, y + 3.0 * s, s * 1.2, [1.0, 0.97, 0.94, 0.8], &middle);
    let gap = wm * 0.5 + 10.0 * s;
    for t in 0..2usize {
        let n = totals[t].to_string();
        let wn = Hud::text_width(&n, big);
        let x = if t == 0 { cx - gap - wn } else { cx + gap };
        let ink = TEAM_INK[t];
        hud.label(x + s, y + s, big, [0.0, 0.0, 0.0, 0.5], &n);
        hud.label(x, y, big, ink, &n);
        // Under each side's number, where its flag is.
        let Some(f) = sh.flags.of(t as u8) else { continue };
        let line = match f.state {
            super::flag::State::Home => "HOME".to_string(),
            super::flag::State::Carried(who) => format!("TAKEN, {}", sh.name_of(who)),
            super::flag::State::Dropped { left, .. } => format!("DOWN {:.0}", left.ceil()),
        };
        let wl = Hud::text_width(&line, s);
        let lx = if t == 0 { cx - gap - wl } else { cx + gap };
        let a = if f.state == super::flag::State::Home { 0.55 } else { 0.95 };
        hud.label(lx, y + 22.0 * s, s, [ink[0], ink[1], ink[2], a], &line);
    }
    let _ = ctf;
}

/// A thin border in the colour of the flag on your back, for as long as it
/// is there: you are the one everybody is after.
fn carrying(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let Some(team) = sh.flags.carried_by(0) else { return };
    let k = TEAM_INK[(team as usize).min(1)];
    let pulse = 0.35 + 0.15 * (sh.elapsed() * 4.0).sin();
    let c = [k[0], k[1], k[2], pulse];
    let t = (4.0 * s).max(2.0);
    hud.rect(0.0, 0.0, w, t, c);
    hud.rect(0.0, h - t, w, t, c);
    hud.rect(0.0, 0.0, t, h, c);
    hud.rect(w - t, 0.0, t, h, c);
}

/// The zombie round's readouts: the way to the refuge, the party, the time,
/// and across the middle how it ended.
fn zombie_round(sh: &Shooter, hud: &mut Hud, w: f32, h: f32, s: f32) {
    let Some(z) = sh.zed.as_ref() else { return };
    let (up, of) = z.standing(sh);
    let clock = format!("{}:{:02}", (z.clock / 60.0) as u32, (z.clock % 60.0) as u32);
    let (cx, cy) = ((w * 0.5).floor(), (h * 0.5).floor());
    let line = 10.0 * s;
    // Across the top, where a match keeps its score.
    let stage = match z.stage {
        super::zombies::Stage::Leg if z.last_leg() => format!("LEG {}/{}: TO THE EXTRACTION", z.leg + 1, z.legs.len()),
        super::zombies::Stage::Leg => format!("LEG {}/{}: TO THE REFUGE", z.leg + 1, z.legs.len()),
        super::zombies::Stage::Refuge => "SAFE FOR NOW: LEAVE WHEN READY".to_string(),
        super::zombies::Stage::Finale => {
            let left = (z.finale_len - z.stage_t).max(0.0) as u32;
            format!("HOLD OUT {}:{:02}", left / 60, left % 60)
        }
        super::zombies::Stage::Out => "THE WAY OUT IS OPEN".to_string(),
    };
    let top = [
        (format!("PARTY {up}/{of}    {clock}"), s * 1.4, [0.85, 1.0, 0.85, 0.9]),
        (format!("{stage}    {}", z.condition.title().to_uppercase()), s, [0.85, 1.0, 0.85, 0.8]),
        (if z.hold.is_some() { "X: THE PARTY HOLDS HERE".to_string() } else { "X: THE PARTY FOLLOWS".to_string() }, s, [1.0, 0.95, 0.9, 0.65]),
    ];
    let mut y = (8.0 * s).floor();
    for (text, k, ink) in &top {
        let wt = Hud::text_width(text, *k);
        hud.label(cx - wt * 0.5 + s, y + s, *k, [0.0, 0.0, 0.0, 0.5], text);
        hud.label(cx - wt * 0.5, y, *k, *ink, text);
        y += 10.0 * k + 4.0 * s;
    }
    let Some((how, _)) = z.over else { return };
    let (msg, tint) = match how {
        super::zombies::Outcome::Made => ("YOU MADE IT", [0.7, 1.0, 0.75, 0.95]),
        super::zombies::Outcome::Lost => ("THE DEAD GOT YOU", [1.0, 0.86, 0.8, 0.95]),
    };
    let wm = Hud::text_width(msg, s * 3.0);
    hud.label(cx - wm * 0.5, cy - line * 5.0, s * 3.0, tint, msg);
    // Who made it, by name.
    let mut made: Vec<String> = Vec::new();
    if sh.me.alive() {
        made.push("YOU".to_string());
    }
    made.extend(sh.bots.iter().take(z.party).filter(|b| b.fighter.alive()).map(|b| b.name.clone()));
    let first = match how {
        super::zombies::Outcome::Made => format!("{clock}   {}   MADE IT: {}", z.condition.title().to_uppercase(), made.join(", ")),
        super::zombies::Outcome::Lost => format!("{clock}   {}   LOST ON LEG {} OF {}", z.condition.title().to_uppercase(), z.leg + 1, z.legs.len()),
    };
    let tags = z.tags(sh);
    let mut lines = vec![
        first,
        format!("HEALTH SPENT {:.0}   {} ZOMBIES DOWN   {} HORDES   NOISE {:.0}", z.hurt, z.kills, z.hordes, z.noise),
    ];
    if !tags.is_empty() {
        lines.push(tags.join("   "));
    }
    lines.push("ENTER FOR ANOTHER GO".to_string());
    for (k, l) in lines.iter().enumerate() {
        let wl = Hud::text_width(l, s);
        hud.label(cx - wl * 0.5, cy - line * (2.5 - 1.4 * k as f32), s, [1.0, 0.95, 0.9, 0.85], l);
    }
}
