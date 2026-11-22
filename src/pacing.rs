//! The step the world moves by each windowed frame.
//!
//! With vsync on, the display shows one frame every refresh, but the loop does
//! not reach the swapchain at even intervals: it runs a few frames ahead in
//! a few milliseconds, then waits 30-50 ms for a buffer to come free. Each
//! frame is still on screen for one refresh, so moving the world by the
//! loop's own interval moves it 7, 7, then 45 ms between pictures that are
//! shown 16.7 ms apart, and the motion judders even at a steady 60 fps.
//!
//! Under vsync the step is the average of the last few intervals, snapped to
//! the refresh interval when it is close to a whole number of them. What the
//! steps fall behind or run ahead of the clock is kept as `debt` and paid back
//! slowly once it passes two refreshes, so the world keeps time with the
//! clock without the bunching showing.

/// Intervals averaged: twelve, so a pattern that repeats every 2, 3 or 4
/// frames averages out exactly.
const WINDOW: usize = 12;
/// The longest step: one long stall (a world loading, the window coming back)
/// must not throw the simulation forward in a single step.
pub const MAX_STEP: f32 = 0.1;
/// How near a whole number of refreshes the average has to be to snap to it.
const SNAP: f32 = 0.25;
/// Debt within this many refreshes is left alone: the bunching itself swings
/// it by about that much and evens out over a few frames.
const DEADBAND: f32 = 2.0;
/// Seconds to pay back debt past the deadband.
const PAYBACK: f32 = 0.5;

#[derive(Default)]
pub struct Pacer {
    recent: [f32; WINDOW],
    n: usize,
    next: usize,
    /// Seconds the clock is ahead of the steps handed out (negative: behind).
    debt: f32,
    /// The display's refresh interval in seconds, if the monitor reports one.
    pub refresh: Option<f32>,
    /// `VOXEL_PACING=0`: hand out the raw interval, for comparisons.
    pub off: bool,
}

impl Pacer {
    pub fn new(refresh: Option<f32>, off: bool) -> Self {
        Self { refresh, off, ..Default::default() }
    }

    /// Forget the recent intervals: after a stall or a change of present mode.
    pub fn reset(&mut self) {
        self.n = 0;
        self.next = 0;
        self.debt = 0.0;
    }

    /// The step for a frame that came `raw` seconds after the last one.
    /// `vsync` is whether the swapchain waits for the display (FIFO); without
    /// it frames are shown as they finish and the raw interval is the step.
    pub fn step(&mut self, raw: f32, vsync: bool) -> f32 {
        if self.off || !vsync || raw >= MAX_STEP {
            self.reset();
            return raw.min(MAX_STEP);
        }
        self.recent[self.next] = raw;
        self.next = (self.next + 1) % WINDOW;
        self.n = (self.n + 1).min(WINDOW);
        let avg = self.recent[..self.n].iter().sum::<f32>() / self.n as f32;
        let mut dt = avg;
        if let Some(r) = self.refresh.filter(|&r| r > 0.0) {
            let k = (avg / r).round().max(1.0);
            if (avg / r - k).abs() < SNAP {
                dt = k * r;
            }
            self.debt += raw - dt;
            let excess = self.debt.abs() - DEADBAND * r;
            if excess > 0.0 {
                let pay = self.debt.signum() * excess * (dt / PAYBACK).min(1.0);
                dt += pay;
                self.debt -= pay;
            }
        } else {
            self.debt += raw - dt;
            let pay = self.debt * (dt / PAYBACK).min(1.0);
            dt += pay;
            self.debt -= pay;
        }
        dt.clamp(0.0, MAX_STEP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f32 = 1.0 / 60.0;

    /// The loop's pattern under vsync on the machine where this was found:
    /// two quick frames then a long wait, averaging one refresh.
    fn bunched(i: usize) -> f32 {
        [0.007, 0.007, 3.0 * R - 0.014][i % 3]
    }

    #[test]
    fn bunched_frames_get_even_steps() {
        let mut p = Pacer { refresh: Some(R), ..Default::default() };
        let steps: Vec<f32> = (0..600).map(|i| p.step(bunched(i), true)).collect();
        for &s in &steps[WINDOW..] {
            assert!((s - R).abs() < 0.0005, "step {s} is not one refresh");
        }
    }

    /// Lag behind the clock after each frame, summed in f64 so the sums
    /// themselves don't drift.
    fn lags(p: &mut Pacer, raws: impl Iterator<Item = f32>) -> Vec<f64> {
        let (mut clock, mut sim) = (0.0f64, 0.0f64);
        raws.map(|raw| {
            clock += raw as f64;
            sim += p.step(raw, true) as f64;
            clock - sim
        })
        .collect()
    }

    #[test]
    fn keeps_time_with_the_clock() {
        // A monitor that says 60 Hz and runs at 59.94, plus the bunching.
        let mut p = Pacer::new(Some(R), false);
        let real = 1.0 / 59.94;
        let lag = lags(&mut p, (0..36_000).map(|i| bunched(i) * real / R));
        let worst = lag.iter().fold(0.0f64, |a, &l| a.max(l.abs()));
        assert!(worst < 0.1, "ten minutes drifted up to {worst} s");
    }

    #[test]
    fn a_dropped_frame_is_caught_up() {
        // Every 20th frame misses a refresh. The steps settle a few refreshes
        // behind the clock and stay there rather than falling further back.
        let mut p = Pacer::new(Some(R), false);
        let lag = lags(&mut p, (0..1200).map(|i| if i % 20 == 19 { 2.0 * R } else { R }));
        assert!(lag[1199] < 0.1, "fell behind by {} s", lag[1199]);
        assert!((lag[1199] - lag[599]).abs() < R as f64, "still falling behind: {} then {}", lag[599], lag[1199]);
    }

    #[test]
    fn without_vsync_the_step_is_the_interval() {
        let mut p = Pacer { refresh: Some(R), ..Default::default() };
        for raw in [0.003, 0.009, 0.004] {
            assert_eq!(p.step(raw, false), raw);
        }
        assert_eq!(p.step(0.4, true), MAX_STEP);
    }

    #[test]
    fn unknown_refresh_still_smooths() {
        let mut p = Pacer::default();
        let steps: Vec<f32> = (0..300).map(|i| p.step(bunched(i), true)).collect();
        let worst = steps[60..].iter().map(|s| (s - R).abs()).fold(0.0, f32::max);
        assert!(worst < 0.004, "unsnapped steps swing by {worst}");
    }
}
