//! Where a windowed frame's time went, for finding hitches in real play.
//!
//! The frame loop and the tick mark their parts with `start`/`end`; at the end
//! of a frame `frame_done` sums them. A frame whose work (everything but the
//! wait for the display) is over `SLOW_MS`, or whose wait is over
//! `SLOW_WAIT_MS`, is a hitch: with `VOXEL_TRACE=1` it
//! is printed with its parts, and the stats panel (F3) shows the worst one of
//! the last few seconds either way. Headless runs print their own line
//! (tools/headless.rs) and add these parts to it.

use std::cell::RefCell;
use std::time::Instant;

/// Work over this is a hitch: a frame that cannot make a 60 Hz refresh.
pub const SLOW_MS: f32 = 16.0;
/// A wait for the display longer than this is a hitch too: under vsync the
/// wait is up to two refreshes (34 ms at 60 Hz), and past that it is the GPU
/// still busy with an earlier frame, or an upload.
pub const SLOW_WAIT_MS: f32 = 50.0;
/// Seconds the panel keeps showing the worst hitch.
const HOLD_S: f32 = 5.0;

#[derive(Default)]
struct State {
    parts: Vec<(&'static str, f32)>,
    worst: Option<(Instant, f32, String)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub fn start() -> Instant {
    Instant::now()
}

/// Add the time since `t` to this frame's `name` part. A part marked more
/// than once a frame (one per fixed step) adds up.
pub fn end(name: &'static str, t: Instant) {
    add(name, t.elapsed().as_secs_f32() * 1000.0);
}

pub fn add(name: &'static str, ms: f32) {
    STATE.with(|s| {
        let parts = &mut s.borrow_mut().parts;
        match parts.iter_mut().find(|p| p.0 == name) {
            Some(p) => p.1 += ms,
            None => parts.push((name, ms)),
        }
    });
}

/// This frame's parts, emptied for the next one.
pub fn take() -> Vec<(&'static str, f32)> {
    STATE.with(|s| std::mem::take(&mut s.borrow_mut().parts))
}

/// "tick 14.5 creatures 12.1 player 0.4": parts over 0.05 ms, in the order
/// they were first marked.
pub fn line(parts: &[(&'static str, f32)]) -> String {
    parts.iter().filter(|p| p.1 >= 0.05).map(|p| format!("{} {:.1}", p.0, p.1)).collect::<Vec<_>>().join(" ")
}

/// End a windowed frame that took `work_ms` of work and `wait_ms` waiting for
/// the display. Returns the parts line if it was a hitch. The panel names the
/// biggest part, or GPU when the frame was mostly a long wait.
pub fn frame_done(work_ms: f32, wait_ms: f32) -> Option<String> {
    let parts = take();
    if work_ms <= SLOW_MS && wait_ms <= SLOW_WAIT_MS {
        return None;
    }
    let l = line(&parts);
    let total = work_ms + wait_ms;
    let part = if wait_ms > SLOW_WAIT_MS && wait_ms > work_ms { "gpu".to_string() } else { biggest(&parts) };
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let keep = s.worst.as_ref().is_some_and(|w| w.0.elapsed().as_secs_f32() < HOLD_S && w.1 >= total);
        if !keep {
            s.worst = Some((Instant::now(), total, part));
        }
    });
    Some(format!("slow frame {:.1} ms work, {:.1} ms waiting: {l}", work_ms, wait_ms))
}

/// The worst hitch of the last few seconds and its biggest part, for the
/// stats panel.
pub fn worst() -> Option<(f32, String)> {
    STATE.with(|s| {
        let s = s.borrow();
        s.worst.as_ref().filter(|w| w.0.elapsed().as_secs_f32() < HOLD_S).map(|w| (w.1, w.2.clone()))
    })
}

/// The parts the frame loop marks; every other part is inside the tick.
const OUTER: [&str; 7] = ["tick", "stream", "models", "hud", "sound", "render", "present"];

/// The part that took longest. The tick counts only for what its own parts
/// don't account for, so a slow creature step is named, not the tick round it.
fn biggest(parts: &[(&'static str, f32)]) -> String {
    let inner: f32 = parts.iter().filter(|p| !OUTER.contains(&p.0)).map(|p| p.1).sum();
    parts
        .iter()
        .map(|&(n, ms)| if n == "tick" { (n, ms - inner) } else { (n, ms) })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|p| p.0.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_add_up_and_hitches_are_kept() {
        add("creatures", 3.0);
        add("creatures", 9.0);
        add("tick", 13.0);
        add("render", 2.0);
        assert!(frame_done(10.0, 5.0).is_none(), "a quick frame is not a hitch");
        add("creatures", 12.0);
        add("tick", 13.0);
        let l = frame_done(20.0, 0.0).expect("a hitch");
        assert!(l.contains("creatures 12.0"), "{l}");
        assert_eq!(worst(), Some((20.0, "creatures".to_string())));
        assert!(take().is_empty());
        assert!(frame_done(2.0, 30.0).is_none(), "an ordinary vsync wait is not a hitch");
        assert!(frame_done(2.0, 200.0).is_some());
        assert_eq!(worst(), Some((202.0, "gpu".to_string())));
    }
}
