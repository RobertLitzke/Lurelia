//! The tuning panel: every number in a vehicle's spec, stepped by hand while
//! driving with the stats on (F3), and written back into the vehicle's file.
//! A tool for finding the feel; the file stays the source.
//!
//! PageUp / PageDown pick a line, [ and ] step it, - and = step it ten
//! times as far, Backspace puts it back to the file's value, P writes the
//! changed lines into the file.

use super::spec::{Driven, Spec, Steering};
use std::path::Path;
use winit::keyboard::KeyCode;

/// One line of the panel: the spec key and the step for [ and ].
pub struct Knob {
    pub key: &'static str,
    pub step: f32,
}

macro_rules! knobs {
    ($($f:ident : $step:expr),* $(,)?) => {
        /// Every line, in the order the spec groups them.
        pub const KNOBS: &[Knob] = &[
            Knob { key: "steering", step: 1.0 },
            Knob { key: "driven", step: 1.0 },
            Knob { key: "inertia_pitch", step: 20.0 },
            Knob { key: "inertia_yaw", step: 20.0 },
            Knob { key: "inertia_roll", step: 10.0 },
            $(Knob { key: stringify!($f), step: $step },)*
        ];

        fn num<'a>(s: &'a mut Spec, key: &str) -> Option<&'a mut f32> {
            match key {
                "inertia_pitch" => Some(&mut s.inertia[0]),
                "inertia_yaw" => Some(&mut s.inertia[1]),
                "inertia_roll" => Some(&mut s.inertia[2]),
                $(stringify!($f) => Some(&mut s.$f),)*
                _ => None,
            }
        }
    };
}

knobs! {
    mass: 10.0, gravity: 0.5, com_height: 0.02,
    wheelbase: 0.05, track: 0.05, rear_weight: 0.01, wheel_radius: 0.01, tyre_width: 0.01,
    travel: 0.01, freq_front: 0.05, freq_rear: 0.05, damp_bump: 0.02, damp_rebound: 0.02, anti_roll: 0.05,
    damp_knee: 0.05, roll_lever: 0.05, settle: 0.2, upright: 1.0,
    drift_charge: 0.05, drift_boost: 0.2, drift_angle: 1.0, drift_grip: 0.02,
    grip: 0.02, slide: 0.01, handbrake_grip: 0.02,
    force: 100.0, power: 5.0, top_speed: 0.5, reverse_speed: 0.5, brake: 0.05, drag: 0.05,
    boost_force: 100.0, boost_speed: 0.5,
    lock: 1.0, lock_fast: 0.5, steer_rate: 5.0,
    yaw_damp: 0.1, downforce: 0.05, air_pitch: 0.2, air_roll: 0.2, air_damp: 0.1, air_level: 0.1,
    hop: 0.1, hop_loaded: 0.1, clean_impact: 0.1, clean_kick: 0.1,
    visual_roll: 0.005, visual_pitch: 0.005,
    cam_dist: 0.1, cam_height: 0.05, idle_hz: 1.0, top_hz: 2.0,
}

/// The panel's keys, which also repeat while held.
pub fn is_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::PageUp | KeyCode::PageDown | KeyCode::BracketLeft | KeyCode::BracketRight | KeyCode::Minus | KeyCode::Equal | KeyCode::Backspace | KeyCode::KeyP
    )
}

/// The line's value as the file would say it.
pub fn show(s: &Spec, key: &str) -> String {
    match key {
        "steering" => match s.steering {
            Steering::Look => "\"look\"".into(),
            Steering::Direct => "\"direct\"".into(),
        },
        "driven" => match s.driven {
            Driven::Rear => "\"rear\"".into(),
            Driven::All => "\"all\"".into(),
        },
        "inertia_pitch" | "inertia_yaw" | "inertia_roll" => format!("[{}, {}, {}]", fmt(s.inertia[0]), fmt(s.inertia[1]), fmt(s.inertia[2])),
        _ => {
            let mut s = s.clone();
            num(&mut s, key).map_or_else(String::new, |v| fmt(*v))
        }
    }
}

/// The line's value for the panel: one number, or the choice's name.
pub fn value(s: &Spec, key: &str) -> String {
    let mut s = s.clone();
    match num(&mut s, key) {
        Some(v) => fmt(*v),
        None => show(&s, key).trim_matches('"').to_string(),
    }
}

/// A number the way the files write them: no trailing zeros.
fn fmt(v: f32) -> String {
    let t = format!("{:.4}", v);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" {
        "0".into()
    } else {
        t.to_string()
    }
}

/// Step line `key` by `steps` of its step; the choices flip.
pub fn nudge(s: &mut Spec, k: &Knob, steps: f32) {
    match k.key {
        "steering" => {
            s.steering = if s.steering == Steering::Look { Steering::Direct } else { Steering::Look };
        }
        "driven" => s.driven = if s.driven == Driven::Rear { Driven::All } else { Driven::Rear },
        key => {
            if let Some(v) = num(s, key) {
                // Rounded so repeated nudges don't drift off round numbers.
                let n = ((*v + k.step * steps) * 1e4).round() / 1e4;
                *v = if key == "com_height" { n } else { n.max(0.0) };
                if key == "rear_weight" {
                    *v = v.clamp(0.2, 0.8);
                }
            }
        }
    }
}

/// Put line `key` back to `from`'s value.
pub fn reset(s: &mut Spec, from: &Spec, key: &str) {
    match key {
        "steering" => s.steering = from.steering,
        "driven" => s.driven = from.driven,
        key => {
            let mut f = from.clone();
            if let (Some(v), Some(o)) = (num(s, key), num(&mut f, key)) {
                *v = *o;
            }
        }
    }
}

/// The spec key a line writes: the three inertia lines share one array.
fn file_key(key: &'static str) -> &'static str {
    if key.starts_with("inertia_") {
        "inertia"
    } else {
        key
    }
}

/// The lines where `s` differs from `from`, as (file key, value), one per
/// file key.
pub fn changes(s: &Spec, from: &Spec) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for k in KNOBS {
        let (a, b) = (show(s, k.key), show(from, k.key));
        let fk = file_key(k.key);
        if a != b && !out.iter().any(|(f, _)| *f == fk) {
            out.push((fk, a));
        }
    }
    out
}

/// Write `changes` into the JSON text: a key the file has gets its value
/// replaced where it stands, a key it lacks goes on the end, so the rest of
/// the file, its order and spacing, is left as it was.
pub fn patch(text: &str, changes: &[(&str, String)]) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut t = text.to_string();
    for (key, val) in changes {
        let pat = format!("\"{key}\"");
        if let Some(at) = t.find(&pat) {
            let colon = at + pat.len() + t[at + pat.len()..].find(':').unwrap_or(0) + 1;
            let rest = &t[colon..];
            let lead = rest.len() - rest.trim_start().len();
            let start = colon + lead;
            let body = &t[start..];
            let len = if body.starts_with('[') {
                body.find(']').map_or(body.len(), |i| i + 1)
            } else if let Some(inner) = body.strip_prefix('"') {
                inner.find('"').map_or(body.len(), |i| i + 2)
            } else {
                body.find([',', '\n', '\r', '}']).unwrap_or(body.len())
            };
            let end = start + len;
            // Keep any spacing that sat between the value and its comma.
            let trimmed = t[start..end].trim_end().len();
            t.replace_range(start..start + trimmed, val);
        } else if let Some(close) = t.rfind('}') {
            let before = t[..close].trim_end().len();
            t.insert_str(before, &format!(",{nl}  \"{key}\": {val}"));
        }
    }
    t
}

/// Write what changed into the file at `path`; the number of keys written.
pub fn save(path: &Path, s: &Spec, from: &Spec) -> Result<usize, String> {
    let ch = changes(s, from);
    if ch.is_empty() {
        return Ok(0);
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let out = patch(&text, &ch);
    serde_json::from_str::<serde_json::Value>(&out).map_err(|e| format!("would not parse: {e}"))?;
    std::fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(ch.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_number_line_reaches_a_field() {
        let mut s = Spec::default();
        for k in KNOBS {
            if !matches!(k.key, "steering" | "driven") {
                assert!(num(&mut s, k.key).is_some(), "{}", k.key);
            }
        }
    }

    #[test]
    fn a_patch_replaces_in_place_and_adds_at_the_end() {
        let text = "{\r\n  \"name\": \"x\",\r\n  \"mass\": 680,\r\n  \"inertia\": [620, 800, 260],\r\n  \"driven\": \"rear\"\r\n}\r\n";
        let out = patch(
            text,
            &[("mass", "700".into()), ("inertia", "[600, 800, 260]".into()), ("driven", "\"all\"".into()), ("grip", "1.2".into())],
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mass"], 700);
        assert_eq!(v["inertia"][0], 600);
        assert_eq!(v["driven"], "all");
        assert_eq!(v["grip"], 1.2);
        assert!(out.starts_with("{\r\n  \"name\": \"x\",\r\n  \"mass\": 700,\r\n"), "{out}");
    }

    #[test]
    fn nudges_snap_and_changes_list_them() {
        let from = Spec::default();
        let mut s = from.clone();
        let grip = KNOBS.iter().find(|k| k.key == "grip").unwrap();
        nudge(&mut s, grip, 1.0);
        assert_eq!(value(&s, "grip"), "1.17");
        let pitch = KNOBS.iter().find(|k| k.key == "inertia_pitch").unwrap();
        nudge(&mut s, pitch, -1.0);
        let ch = changes(&s, &from);
        assert_eq!(ch, vec![("inertia", "[600, 800, 260]".to_string()), ("grip", "1.17".to_string())]);
        reset(&mut s, &from, "grip");
        assert_eq!(changes(&s, &from).len(), 1);
    }
}
