//! The Xbox controller, through XInput. xinput1_4.dll (or an older one) is
//! loaded at run time, so nothing is linked and a machine without it, or any
//! other OS, simply has no pad. One controller is read: the first one found
//! connected. Empty slots are slow to ask about, so they are searched only
//! every couple of seconds.
//!
//! `Pad::poll` reads it once a frame. Sticks come out with their dead zones
//! taken off and rescaled to 0..1 beyond them; buttons as held bits, with the
//! frame's presses and releases. main.rs turns those into the game's keys
//! and fills the analogue part of `MoveInput`.

use glam::Vec2;

pub const UP: u16 = 0x0001;
pub const DOWN: u16 = 0x0002;
pub const LEFT: u16 = 0x0004;
pub const RIGHT: u16 = 0x0008;
pub const START: u16 = 0x0010;
pub const BACK: u16 = 0x0020;
pub const L_STICK: u16 = 0x0040;
pub const R_STICK: u16 = 0x0080;
pub const LB: u16 = 0x0100;
pub const RB: u16 = 0x0200;
pub const A: u16 = 0x1000;
pub const B: u16 = 0x2000;
pub const X: u16 = 0x4000;
pub const Y: u16 = 0x8000;
/// Not XInput bits: the triggers past half way, as buttons, for menus.
pub const LT: u32 = 0x1_0000;
pub const RT: u32 = 0x2_0000;

/// XInput's own recommended dead zones, as a share of full travel.
const LEFT_DEAD: f32 = 7849.0 / 32767.0;
const RIGHT_DEAD: f32 = 8689.0 / 32767.0;
const TRIGGER_DEAD: f32 = 30.0 / 255.0;
/// Seconds between searches for a controller when none is connected.
const SEARCH_EVERY: f32 = 2.0;

/// The controller as read this frame.
#[derive(Clone, Copy, Default, Debug)]
pub struct State {
    /// Left stick, x right and y up (forward), 0..1 past the dead zone.
    pub left: Vec2,
    pub right: Vec2,
    /// Triggers, 0..1 past the dead zone.
    pub lt: f32,
    pub rt: f32,
    /// Held buttons, the XInput bits and LT/RT.
    pub held: u32,
    /// Pressed and released since the last poll.
    pub pressed: u32,
    pub released: u32,
}

impl State {
    pub fn down(&self, b: u32) -> bool {
        self.held & b != 0
    }
    pub fn hit(&self, b: u32) -> bool {
        self.pressed & b != 0
    }
}

pub struct Pad {
    api: Option<sys::Api>,
    slot: Option<u32>,
    search_in: f32,
    last: u32,
    /// The rumble last sent, both motors 0..1, so it is only sent on change.
    sent: (f32, f32),
    /// Rumble level, eased down each frame (`rumble`).
    buzz: f32,
}

impl Pad {
    pub fn new() -> Pad {
        Pad {
            api: sys::Api::load(),
            slot: None,
            search_in: 0.0,
            last: 0,
            sent: (0.0, 0.0),
            buzz: 0.0,
        }
    }

    /// Read the controller. None when there isn't one.
    pub fn poll(&mut self, dt: f32) -> Option<State> {
        let api = self.api.as_ref()?;
        if self.slot.is_none() {
            self.search_in -= dt;
            if self.search_in > 0.0 {
                return None;
            }
            self.search_in = SEARCH_EVERY;
            self.slot = (0..4).find(|&i| api.get(i).is_some());
            if self.slot.is_some() {
                self.last = 0;
                self.sent = (-1.0, -1.0);
            }
        }
        let Some(raw) = api.get(self.slot?) else {
            self.slot = None;
            self.search_in = SEARCH_EVERY;
            return None;
        };
        let lt = trigger(raw.lt);
        let rt = trigger(raw.rt);
        let mut held = raw.buttons as u32;
        if lt > 0.5 {
            held |= LT;
        }
        if rt > 0.5 {
            held |= RT;
        }
        let s = State {
            left: stick(raw.lx, raw.ly, LEFT_DEAD),
            right: stick(raw.rx, raw.ry, RIGHT_DEAD),
            lt,
            rt,
            held,
            pressed: held & !self.last,
            released: self.last & !held,
        };
        self.last = held;
        Some(s)
    }

    /// Shake the controller: `kick` 0..1 is raised to, not added; it dies
    /// away over a few tenths of a second. Call every frame (with 0 when
    /// nothing hit), and with `on` false behind a menu to stop it.
    pub fn rumble(&mut self, kick: f32, on: bool, dt: f32) {
        self.buzz = if on { self.buzz.max(kick.clamp(0.0, 1.0)) } else { 0.0 };
        // The heavy motor carries the thump; the light one the rattle at its start.
        let want = (self.buzz, (self.buzz * 1.5 - 0.5).max(0.0));
        self.buzz *= (-dt / 0.12).exp();
        if self.buzz < 0.02 {
            self.buzz = 0.0;
        }
        let (Some(api), Some(slot)) = (&self.api, self.slot) else { return };
        if (want.0 - self.sent.0).abs() > 0.02 || (want.1 - self.sent.1).abs() > 0.02 || (want.0 == 0.0 && self.sent.0 != 0.0) {
            api.set(slot, want.0, want.1);
            self.sent = want;
        }
    }
}

impl Drop for Pad {
    fn drop(&mut self) {
        if let (Some(api), Some(slot)) = (&self.api, self.slot) {
            api.set(slot, 0.0, 0.0);
        }
    }
}

/// A stick with a round dead zone, rescaled so it starts from 0 at its edge.
fn stick(x: i16, y: i16, dead: f32) -> Vec2 {
    let v = Vec2::new(x as f32, y as f32) / 32767.0;
    let len = v.length();
    if len <= dead {
        return Vec2::ZERO;
    }
    v / len * ((len - dead) / (1.0 - dead)).min(1.0)
}

fn trigger(t: u8) -> f32 {
    let t = t as f32 / 255.0;
    if t <= TRIGGER_DEAD {
        0.0
    } else {
        (t - TRIGGER_DEAD) / (1.0 - TRIGGER_DEAD)
    }
}

/// One read of a controller, as XInput gives it.
#[cfg_attr(not(windows), allow(dead_code))]
struct Raw {
    buttons: u16,
    lt: u8,
    rt: u8,
    lx: i16,
    ly: i16,
    rx: i16,
    ry: i16,
}

#[cfg(windows)]
mod sys {
    use super::Raw;
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct Gamepad {
        buttons: u16,
        lt: u8,
        rt: u8,
        lx: i16,
        ly: i16,
        rx: i16,
        ry: i16,
    }
    #[repr(C)]
    #[derive(Default)]
    struct XState {
        packet: u32,
        pad: Gamepad,
    }
    #[repr(C)]
    struct Vibration {
        left: u16,
        right: u16,
    }

    type GetState = unsafe extern "system" fn(u32, *mut XState) -> u32;
    type SetState = unsafe extern "system" fn(u32, *mut Vibration) -> u32;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryA(name: *const u8) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }

    pub struct Api {
        get: GetState,
        set: Option<SetState>,
    }

    impl Api {
        pub fn load() -> Option<Api> {
            // 1_4 ships with Windows 8 and later; 1_3 with the DirectX
            // runtime; 9_1_0 with Vista and later, without rumble on some.
            for dll in [&b"xinput1_4.dll\0"[..], b"xinput1_3.dll\0", b"xinput9_1_0.dll\0"] {
                // SAFETY: null-terminated names; the library stays loaded
                // for the life of the process, so the pointers stay good.
                unsafe {
                    let m = LoadLibraryA(dll.as_ptr());
                    if m.is_null() {
                        continue;
                    }
                    let g = GetProcAddress(m, b"XInputGetState\0".as_ptr());
                    if g.is_null() {
                        continue;
                    }
                    let s = GetProcAddress(m, b"XInputSetState\0".as_ptr());
                    return Some(Api {
                        get: std::mem::transmute::<*mut c_void, GetState>(g),
                        set: (!s.is_null()).then(|| std::mem::transmute::<*mut c_void, SetState>(s)),
                    });
                }
            }
            None
        }

        pub fn get(&self, slot: u32) -> Option<Raw> {
            let mut s = XState::default();
            // SAFETY: a valid out pointer to a struct of XInput's layout.
            if unsafe { (self.get)(slot, &mut s) } != 0 {
                return None;
            }
            let p = s.pad;
            Some(Raw {
                buttons: p.buttons,
                lt: p.lt,
                rt: p.rt,
                lx: p.lx,
                ly: p.ly,
                rx: p.rx,
                ry: p.ry,
            })
        }

        pub fn set(&self, slot: u32, heavy: f32, light: f32) {
            let Some(set) = self.set else { return };
            let mut v = Vibration {
                left: (heavy.clamp(0.0, 1.0) * 65535.0) as u16,
                right: (light.clamp(0.0, 1.0) * 65535.0) as u16,
            };
            // SAFETY: as above.
            unsafe {
                set(slot, &mut v);
            }
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Raw;
    pub struct Api;
    impl Api {
        pub fn load() -> Option<Api> {
            None
        }
        pub fn get(&self, _slot: u32) -> Option<Raw> {
            None
        }
        pub fn set(&self, _slot: u32, _heavy: f32, _light: f32) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_zones_rescale_from_their_edge() {
        assert_eq!(stick(5000, -3000, LEFT_DEAD), Vec2::ZERO);
        let full = stick(32767, 0, LEFT_DEAD);
        assert!((full.x - 1.0).abs() < 1e-4 && full.y == 0.0);
        let half = stick(0, 16384 + 3925, LEFT_DEAD);
        assert!(half.y > 0.3 && half.y < 0.7, "{half:?}");
        assert_eq!(trigger(20), 0.0);
        assert!((trigger(255) - 1.0).abs() < 1e-4);
    }

    /// With a controller plugged in, prints what it reads for five seconds:
    /// cargo test --release read_the_pad -- --ignored --nocapture
    #[test]
    #[ignore]
    fn read_the_pad() {
        let mut pad = Pad::new();
        println!("xinput loaded: {}", pad.api.is_some());
        for _ in 0..50 {
            match pad.poll(0.1) {
                Some(s) => println!("slot {:?}  {s:?}", pad.slot),
                None => println!("no controller"),
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
