//! The kart on a real circuit: a world's race loaded without the GPU, four
//! karts racing it under the bots, and everything that went wrong counted.
//! The flat test circuit in mod.rs has no walls, kerbs, banking or rises, and
//! those are where the kart came to grief.
//!
//! `WORLD=peregrine TUNE=kart cargo test --release kart_lab -- --ignored --nocapture`
//! (WORLD=vermilion for the other circuit; LAPS= to change the three laps).
//! One kart is driven with keyboard steering (full lock or nothing), as a
//! player at the keys drives it; the others steer as the bots do.

use super::bot::{Driver, Rival, Style, Track};
use super::{ground, meet, yaw_along, Catalogue, Controls, Vehicle};
use crate::world::store::Store;
use glam::{Vec2, Vec3};
use std::time::Duration;

/// The world's circuit streamed in at full detail, and its track.
pub(crate) fn open(name: &str) -> (Store, Track) {
    struct NoSink;
    impl crate::stream::ChunkSink for NoSink {
        fn upload(&mut self, _origin: Vec3, _scale: f32, _mesh: &crate::mesh::Mesh) -> Option<crate::stream::GpuChunk> {
            None
        }
        fn free(&mut self, _c: &crate::stream::GpuChunk) {}
    }
    crate::worldload::select(name);
    let loaded = crate::worldload::load(name).expect("world loads");
    let world = loaded.world.clone();
    let race = world.def.race.clone().expect("the world has a race");
    let circuit = world.circuits.iter().find(|c| c.name == race.circuit).expect("its circuit").clone();
    let track = Track::of(&circuit);
    let gen = crate::world::gen::Generator::with_world(world.clone());
    let mut store = Store {
        height_vox: gen.height_vox,
        gravity: world.def.gravity,
        ..Store::default()
    };
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in &circuit.points {
        b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
    }
    let pad = 24.0;
    let centre = Vec3::new((b[0] + b[2]) * 0.5, circuit.heights.first().copied().unwrap_or(0.0), (b[1] + b[3]) * 0.5);
    let mut streamer = crate::stream::Streamer::new(gen, Default::default(), 128.0, 4);
    streamer.set_smooth(true, Vec3::ZERO);
    streamer.full_box = Some([b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad]);
    let mut sink = NoSink;
    let began = std::time::Instant::now();
    loop {
        streamer.update(centre, Vec3::NEG_Z, &mut sink, &mut store, Duration::from_millis(20));
        let (full, _) = store.census();
        if streamer.idle() && full > 0 {
            break;
        }
        assert!(began.elapsed().as_secs_f32() < 180.0, "the circuit did not stream in");
        std::thread::sleep(Duration::from_millis(4));
    }
    (store, track)
}

/// What went wrong for one kart.
#[derive(Default)]
struct Tally {
    laps: Vec<f32>,
    /// Times the body went more than 0.3 m above where it rides on the
    /// road (the hop reaches about 0.1), and the fastest it went up.
    launches: u32,
    max_vy: f32,
    /// Highest the body got above where it rides on the road, metres.
    max_rise: f32,
    /// Longest with no wheel down, seconds.
    max_air: f32,
    /// Chassis strikes on the world over 1 m/s, and the hardest.
    strikes: u32,
    max_strike: f32,
    /// Off the road's edge, seconds, and furthest.
    off_t: f32,
    max_off: f32,
    spins: u32,
    resets: u32,
    max_roll: f32,
}

#[test]
#[ignore]
fn kart_lab() {
    let name = std::env::var("WORLD").unwrap_or_else(|_| "peregrine".into());
    let tune = std::env::var("TUNE").unwrap_or_else(|_| "kart".into());
    let laps: f32 = std::env::var("LAPS").ok().and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let spec = Catalogue::load(&root).get(&tune);
    let t0 = std::time::Instant::now();
    let (store, track) = open(&name);
    eprintln!("{name}: {:.0} m round, {:.0} m wide, streamed in {:.0} s", track.length, track.width, t0.elapsed().as_secs_f32());

    let n = 4;
    let base = Style::kart();
    let mut cars = Vec::new();
    let mut bots = Vec::new();
    let mut progress = Vec::new();
    for i in 0..n {
        let s = -6.0 * (i as f32 + 1.0);
        let (p, d) = track.at(s);
        let side = if i % 2 == 0 { 2.5 } else { -2.5 };
        let at = p + Vec2::new(-d.y, d.x) * side;
        let y = ground(&store, at, track.heights.first().copied().unwrap_or(0.0)) + 0.1;
        cars.push(Vehicle::new(spec.clone(), Vec3::new(at.x, y, at.y), (-d.x).atan2(-d.y)));
        let k = i as f32;
        let mut b = Driver::new(Style { corner_g: base.corner_g - 0.6 * k, brake_g: base.brake_g - 0.4 * k, ..base });
        b.lane = side;
        bots.push(b);
        progress.push(s);
    }
    let dt = 1.0 / 60.0;
    for c in cars.iter_mut() {
        for _ in 0..30 {
            c.step(&Controls::default(), &store, -100.0, true, dt);
        }
    }
    // Where each kart rides above the road at rest.
    let ride: Vec<f32> = cars.iter().map(|c| c.pos().y - ground(&store, Vec2::new(c.pos().x, c.pos().z), c.pos().y)).collect();
    let mut tally: Vec<Tally> = (0..n).map(|_| Tally::default()).collect();
    let mut last_s: Vec<f32> = progress.iter().map(|s| s.rem_euclid(track.length)).collect();
    let (mut air, mut slow, mut upset, mut spinning, mut lap_start) = (vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n], vec![false; n], vec![0.0f32; n]);
    let mut done = vec![false; n];
    let mut high = vec![false; n];
    let mut contacts = 0;
    let mut t = 0.0f32;
    // The karts' own physics, timed, for `Vehicle::step`'s cost.
    let (mut phys, mut steps) = (std::time::Duration::ZERO, 0u32);
    while t < 240.0 && done.iter().any(|d| !d) {
        let places: Vec<_> = (0..n).map(|i| track.place(cars[i].pos(), bots[i].place)).collect();
        for i in 0..n {
            if done[i] {
                cars[i].step(&Controls { brake: 0.4, ..Default::default() }, &store, -100.0, true, dt);
                continue;
            }
            let rivals: Vec<Rival> = (0..n).filter(|&j| j != i).map(|j| Rival { place: places[j], speed: cars[j].v_fwd() }).collect();
            let mut c = bots[i].race(&cars[i], &track, &rivals);
            if i == 0 {
                // At the keys: full lock or none.
                c.steer = if c.steer.abs() > 0.25 { c.steer.signum() } else { 0.0 };
            }
            let t0 = std::time::Instant::now();
            cars[i].step(&c, &store, -100.0, true, dt);
            phys += t0.elapsed();
            steps += 1;
        }
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = cars.split_at_mut(j);
                if meet(&mut a[i], &mut b[0], Some(&store)) > 1.0 {
                    contacts += 1;
                }
            }
        }
        t += dt;
        for i in 0..n {
            if done[i] {
                continue;
            }
            let car = &cars[i];
            let tl = &mut tally[i];
            let pl = bots[i].place.unwrap_or(places[i]);
            let mut ds = pl.s - last_s[i];
            if ds < -track.length * 0.5 {
                ds += track.length;
            } else if ds > track.length * 0.5 {
                ds -= track.length;
            }
            let before = (progress[i] / track.length).floor();
            progress[i] += ds;
            last_s[i] = pl.s;
            if (progress[i] / track.length).floor() > before && before >= 0.0 {
                tl.laps.push(t - lap_start[i]);
                lap_start[i] = t;
            }
            if progress[i] >= laps * track.length {
                done[i] = true;
            }
            tl.max_vy = tl.max_vy.max(car.b.vel.y);
            let road = ground(&store, Vec2::new(car.pos().x, car.pos().z), car.pos().y);
            let rise = car.pos().y - road - ride[i];
            if rise > 0.3 && !high[i] {
                tl.launches += 1;
                if tl.launches <= 3 && std::env::var("WHERE").is_ok() {
                    eprintln!("    kart {i} up {rise:.2} m at s {:.0} off {:.1}, {:.0} km/h, strike {:.1}", pl.s, pl.off, car.v_fwd() * 3.6, car.events.crash);
                }
            }
            high[i] = rise > 0.15;
            tl.max_rise = tl.max_rise.max(rise);
            air[i] = if car.grounded == 0 { air[i] + dt } else { 0.0 };
            tl.max_air = tl.max_air.max(air[i]);
            if car.events.crash > 1.0 {
                tl.strikes += 1;
                tl.max_strike = tl.max_strike.max(car.events.crash);
            }
            let off = pl.off.abs() - track.width_at(pl.s) * 0.5;
            if off > 0.0 {
                tl.off_t += dt;
            }
            tl.max_off = tl.max_off.max(off);
            let f = car.forward();
            let vel = Vec3::new(car.b.vel.x, 0.0, car.b.vel.z);
            let slip = if vel.length() > 3.0 { vel.normalize().dot(Vec3::new(f.x, 0.0, f.z).normalize()).clamp(-1.0, 1.0).acos().to_degrees() } else { 0.0 };
            if slip > 90.0 && !spinning[i] {
                tl.spins += 1;
            }
            spinning[i] = slip > 60.0;
            tl.max_roll = tl.max_roll.max((car.b.rot * Vec3::X).y.asin().to_degrees().abs());
            slow[i] = if car.b.vel.length() < 1.0 { slow[i] + dt } else { 0.0 };
            upset[i] = if car.up().y < 0.5 { upset[i] + dt } else { 0.0 };
            if slow[i] > 3.0 || upset[i] > 2.0 || off > 8.0 {
                tl.resets += 1;
                let (p, d) = track.at(pl.s);
                let y = ground(&store, p, car.pos().y) + 0.3;
                cars[i] = Vehicle::new(spec.clone(), Vec3::new(p.x, y, p.y), yaw_along(d));
                slow[i] = 0.0;
                upset[i] = 0.0;
            }
        }
    }
    eprintln!("{tune} on {name}: {t:.0} s, kart-to-kart contacts {contacts}");
    // Where the karts ended up, to the bit: a change that should not alter
    // the driving (a faster read of the same voxels) leaves this as it was.
    let print = cars.iter().fold(0u64, |h, c| {
        [c.b.pos.x, c.b.pos.y, c.b.pos.z, c.b.vel.x, c.b.vel.z].iter().fold(h, |h, v| (h ^ v.to_bits() as u64).wrapping_mul(0x100_0000_01b3))
    });
    eprintln!("  physics {:.3} ms a kart-step over {steps} steps; final state {print:016x}", phys.as_secs_f64() * 1000.0 / steps.max(1) as f64);
    for (i, tl) in tally.iter().enumerate() {
        let laps: Vec<String> = tl.laps.iter().map(|x| format!("{x:.1}")).collect();
        eprintln!(
            "  {} laps [{}]  launches {} (max vy {:.1} m/s, rise {:.2} m, air {:.2} s)  strikes {} (hardest {:.1})  off {:.1} s ({:.1} m)  spins {}  resets {}  roll {:.0}",
            if i == 0 { "keys" } else { "bot " },
            laps.join(" "),
            tl.launches,
            tl.max_vy,
            tl.max_rise,
            tl.max_air,
            tl.strikes,
            tl.max_strike,
            tl.off_t,
            tl.max_off.max(0.0),
            tl.spins,
            tl.resets,
            tl.max_roll
        );
    }
}
