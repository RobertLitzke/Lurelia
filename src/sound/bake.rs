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
    let levels = Levels { on: true, volume: 100, ..Levels::default() };
    super::listen();
    // The world runs at its own 60 Hz, one frame per sixtieth of audio.
    let step = 1.0 / 60.0;
    let frames = (args.secs * RATE as f32) as usize;
    let mut out = vec![0.0f32; frames * 2];
    let per = (step * RATE as f32) as usize * 2;
    let mut heard = Default::default();
    let mut posted: std::collections::BTreeMap<String, u32> = Default::default();
    let mut rng = super::dsp::Rng::new(3);
    let input = crate::player::MoveInput::default();
    for chunk in out.chunks_mut(per) {
        let mut actions = crate::game::Actions::default();
        game.tick(step, &input, &mut actions);
        game.stream(&mut sink, Duration::from_millis(2));
        let (eye, fwd) = game.listener();
        let facing = [fwd.x, fwd.z];
        watch.look(&game, step, eye);
        for n in super::take_noises() {
            let name = format!("{:?}", n.kind);
            let name = name.split(['(', ' ']).next().unwrap_or("").to_string();
            *posted.entry(name).or_default() += 1;
            let pitch = 1.0 + (rng.unit() - 0.5) * 0.06;
            if let Some(s) = super::fx::place(&n, eye, facing, pitch) {
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
    if let Err(e) = write_wav(&path, RATE, &out) {
        eprintln!("--sound-bake: {}: {e}", path.display());
        std::process::exit(1);
    }
    println!("wrote {} ({:.0} s)", path.display(), args.secs);
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
