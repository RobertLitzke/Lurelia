//! Rendering the engine into a file instead of a device (`--sound-bake`):
//! open a world with no window and no GPU, stand the camera where the
//! command line says, and run the world as the game would -- the ambience
//! surveyed, the feet and the match watched, every posted sound placed and
//! played -- into a file rather than a device. Thirty seconds of a wood in
//! the rain, or of a match on Slagdeck, on demand and with the numbers it
//! was made from printed beside it.

use super::bed::Ear;
use super::watch::Watch;
use super::{Cue, Engine, Levels};
use crate::cli::Args;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

/// The rate bakes are made at.
pub const RATE: u32 = 48_000;

/// Write interleaved stereo f32 samples as a 16-bit WAV.
pub fn write_wav(path: &Path, rate: u32, samples: &[f32]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::File::create(path)?.write_all(&out)
}

/// Loudness of interleaved stereo: RMS and peak, dBFS.
pub fn measure(samples: &[f32]) -> (f32, f32) {
    let n = samples.len().max(1) as f32;
    let rms = (samples.iter().map(|x| x * x).sum::<f32>() / n).sqrt();
    let peak = samples.iter().fold(0.0f32, |a, x| a.max(x.abs()));
    let db = |x: f32| 20.0 * x.max(1e-9).log10();
    (db(rms), db(peak))
}

pub fn run(args: &Args) {
    let Some(path) = args.sound_bake.clone() else { return };
    struct NoSink;
    impl crate::stream::ChunkSink for NoSink {
        fn upload(&mut self, _o: glam::Vec3, _s: f32, _m: &crate::mesh::Mesh) -> Option<crate::stream::GpuChunk> {
            None
        }
        fn free(&mut self, _c: &crate::stream::GpuChunk) {}
    }
    let mut a = args.clone();
    a.headless = true;
    let mut game = match crate::new_game(&a) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("--sound-bake: {e}");
            std::process::exit(1);
        }
    };
    if let Some(h) = args.hour {
        game.hour = crate::world::clock::wrap(h);
        game.clock_running = false;
    }
    if let Some(w) = args.weather.as_deref() {
        match crate::world::weather::Kind::parse(w) {
            Some(k) => game.weather = crate::world::weather::Weather::new(k, Default::default(), 0, false),
            None => {
                eprintln!("--weather: no such weather as {w}");
                std::process::exit(1);
            }
        }
    }
    // The voxels round the listener have to be in before the survey means
    // anything: the leaves, the roof, the walls are all read from them.
    let began = Instant::now();
    let mut sink = NoSink;
    loop {
        game.stream(&mut sink, Duration::from_millis(50));
        if game.streamer.idle() || began.elapsed().as_secs_f32() > 90.0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    let mut ear = Ear::default();
    let mut watch = Watch::default();
    let mut engine = Engine::new(RATE as f32);
    let levels = Levels {
        on: true,
        volume: 100,
        ..Levels::default()
    };
    super::listen();
    // The world runs at its own 60 Hz, one frame per sixtieth of audio.
    let step = 1.0 / 60.0;
    let frames = (args.secs * RATE as f32) as usize;
    let mut out = vec![0.0f32; frames * 2];
    let per = (step * RATE as f32) as usize * 2;
    let mut heard = Default::default();
    let mut posted: std::collections::BTreeMap<String, u32> = Default::default();
    let mut rng = super::dsp::Rng::new(3);
    // Each animal call once, however often it is posted again to follow
    // its caller, by species and cry.
    let mut calls: std::collections::HashSet<(u32, u32)> = Default::default();
    let mut called: std::collections::BTreeMap<String, u32> = Default::default();
    let input = crate::player::MoveInput::default();
    for chunk in out.chunks_mut(per) {
        let mut actions = crate::game::Actions::default();
        game.tick(step, &input, &mut actions);
        game.stream(&mut sink, Duration::from_millis(2));
        let (eye, fwd) = game.listener();
        let facing = [fwd.x, fwd.z];
        watch.look(&game, step, eye);
        for n in super::take_noises() {
            if let super::Kind::Call { cry, seed, who, .. } = n.kind {
                if calls.insert((n.key, seed)) {
                    let sp = if (who as usize) < game.ents.species.len() { crate::creatures::display_name(&game.ents.species, who as usize) } else { "?" };
                    *called.entry(format!("{sp} {}", cry.name())).or_default() += 1;
                }
            }
            let name = format!("{:?}", n.kind);
            let name = name.split(['(', ' ']).next().unwrap_or("").to_string();
            *posted.entry(name).or_default() += 1;
            let pitch = 1.0 + (rng.unit() - 0.5) * 0.06;
            if let Some(s) = super::fx::place(&n, eye, facing, pitch) {
                super::note_audible(&n, &s, eye, &levels);
                engine.start(s);
            }
        }
        heard = ear.hear(&game, step);
        engine.set(Cue { heard, facing, levels, duck: 1.0 });
        engine.fill(chunk, 2);
    }
    let (eye, _) = game.listener();
    println!("at {:.1}, {:.1}, {:.1}  hour {:.1}  weather {}", eye.x, eye.y, eye.z, game.hour, game.weather.name());
    for line in ear.lines(&heard) {
        println!("  {line}");
    }
    // The first two seconds are the glides arriving from silence.
    let settle = (2 * RATE as usize * 2).min(out.len() / 2);
    let (rms, peak) = measure(&out[settle..]);
    println!("level  rms {rms:.1} dBFS  peak {peak:.1} dBFS  (volume 100; the default volume of 70 is 6 dB under this)");
    if !posted.is_empty() {
        let list: Vec<String> = posted.iter().map(|(k, n)| format!("{k} {n}")).collect();
        println!("sounds {}", list.join(", "));
    }
    for a in super::audible().iter().take(8) {
        let sp = if (a.who as usize) < game.ents.species.len() { crate::creatures::display_name(&game.ents.species, a.who as usize) } else { "?" };
        println!("heard  {sp} {} {:.0} m, bearing {:.0}, {:.0} dB", a.cry.name(), a.dist, a.bearing.to_degrees(), a.db);
    }
    if !called.is_empty() {
        let list: Vec<String> = called.iter().map(|(k, n)| format!("{k} {n}")).collect();
        println!("calls  {}", list.join(", "));
    }
    if let Err(e) = write_wav(&path, RATE, &out) {
        eprintln!("--sound-bake: {}: {e}", path.display());
        std::process::exit(1);
    }
    println!("wrote {} ({:.0} s)", path.display(), args.secs);
}

/// Render animal calls alone (`--call-bake DIR`): each through the engine
/// from `--call-m` metres ahead, with no ambience, to DIR/NAME_SEED.wav,
/// and a spectrogram of it dry to DIR/NAME_SEED.png, 0 to 12 kHz up the
/// side. Printed beside each: its length and how loud it came out.
pub fn calls(args: &Args) {
    use super::call::{render_dry, Cry, CRIES};
    let Some(dir) = args.call_bake.clone() else { return };
    let which: Vec<Cry> = match args.calls.as_deref() {
        None | Some("all") => CRIES.to_vec(),
        Some(list) => list
            .split(',')
            .map(|n| {
                Cry::parse(n.trim()).unwrap_or_else(|| {
                    let names: Vec<&str> = CRIES.iter().map(|c| c.name()).collect();
                    eprintln!("--calls: no call named {n}; there are {}", names.join(", "));
                    std::process::exit(1);
                })
            })
            .collect(),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("--call-bake: {}: {e}", dir.display());
        std::process::exit(1);
    }
    let levels = Levels {
        on: true,
        volume: 100,
        wildlife: 100,
        ..Levels::default()
    };
    let d = args.call_m;
    for cry in which {
        for seed in [1u32, 2] {
            let dry = render_dry(cry, 1.0, seed, RATE as f32);
            let n = super::Noise::call(glam::Vec3::new(0.0, 1.6, -d), cry, 1.0, seed, u16::MAX, 1, 0.0);
            let mut engine = Engine::new(RATE as f32);
            engine.set(Cue {
                levels,
                duck: 1.0,
                ..Cue::default()
            });
            let secs = dry.len() as f32 / RATE as f32 + d / 343.0 + 1.5;
            let mut out = vec![0.0f32; (secs * RATE as f32) as usize * 2];
            if let Some(s) = super::fx::place(&n, glam::Vec3::new(0.0, 1.6, 0.0), [0.0, -1.0], 1.0) {
                engine.start(s);
            }
            engine.fill(&mut out, 2);
            let stem = format!("{}_{seed}", cry.name());
            let (rms, peak) = measure(&out);
            let (_, dry_peak) = measure(&dry);
            let wav = dir.join(format!("{stem}.wav"));
            let png = dir.join(format!("{stem}.png"));
            if let Err(e) = write_wav(&wav, RATE, &out).and_then(|_| spectrogram(&png, &dry, RATE as f32)) {
                eprintln!("--call-bake: {e}");
                std::process::exit(1);
            }
            println!(
                "{stem:12} {:5.1} s  at {d:.0} m: rms {rms:6.1} peak {peak:6.1} dBFS  (dry peak {dry_peak:5.1})",
                dry.len() as f32 / RATE as f32
            );
        }
    }
    println!("wrote {}", dir.display());
}

/// A spectrogram of mono `x` as a PNG: time along, 0 to 12 kHz up, 80 dB
/// of range from black through amber to white.
fn spectrogram(path: &Path, x: &[f32], sr: f32) -> std::io::Result<()> {
    const N: usize = 1024;
    const HOP: usize = 256;
    let top = ((12_000.0 / sr) * N as f32) as usize;
    let cols = (x.len().saturating_sub(N) / HOP + 1).max(1);
    let win: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
    let mut mags = vec![0.0f32; cols * top];
    let mut re = vec![0.0f32; N];
    let mut im = vec![0.0f32; N];
    for c in 0..cols {
        for i in 0..N {
            re[i] = x.get(c * HOP + i).copied().unwrap_or(0.0) * win[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for k in 0..top {
            mags[k * cols + c] = 20.0 * ((re[k] * re[k] + im[k] * im[k]).sqrt() / (N as f32 / 4.0)).max(1e-9).log10();
        }
    }
    let max = mags.iter().copied().fold(-200.0f32, f32::max);
    let mut px = vec![0u8; cols * top * 3];
    for k in 0..top {
        let row = top - 1 - k;
        for c in 0..cols {
            let v = ((mags[k * cols + c] - max + 80.0) / 80.0).clamp(0.0, 1.0);
            let o = (row * cols + c) * 3;
            px[o] = (255.0 * (v * 1.6).min(1.0)) as u8;
            px[o + 1] = (255.0 * (v * 1.2 - 0.2).clamp(0.0, 1.0)) as u8;
            px[o + 2] = (255.0 * (v * 2.0 - 1.0).clamp(0.0, 1.0)) as u8;
        }
    }
    // A faint line every 2 kHz.
    for khz in (2..12).step_by(2) {
        let k = (khz as f32 * 1000.0 / sr * N as f32) as usize;
        let row = top - 1 - k;
        for c in (0..cols).step_by(4) {
            let o = (row * cols + c) * 3;
            px[o..o + 3].copy_from_slice(&[70, 70, 90]);
        }
    }
    crate::util::save_png(path, cols as u32, top as u32, &px)
}

/// An in-place radix-2 FFT; `re.len()` a power of two.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (ang * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * c - im[b] * s;
                let ti = re[b] * s + im[b] * c;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_has_the_right_header() {
        let dir = std::env::temp_dir().join("lurelia_wav_test");
        let p = dir.join("t.wav");
        write_wav(&p, 48_000, &[0.0, 0.5, -0.5, 1.0]).unwrap();
        let b = std::fs::read(&p).unwrap();
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(b[40..44].try_into().unwrap()), 8);
        assert_eq!(b.len(), 52);
        let _ = std::fs::remove_dir_all(dir);
    }
}
