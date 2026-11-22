//! Screenshot mode: F2 pauses the game and frees a camera; Enter or a click
//! saves the view as a PNG in `screenshots/`, without the overlay. F12 saves
//! the current view without the HUD at any time. (The in-game camera, with
//! film and tasks, is src/camera.)
//!
//! The camera flies (WASD, Space and Ctrl up and down, Shift faster), looks
//! with the mouse, zooms with the wheel and tilts with Q and E. It stays
//! within `RANGE` of the player, because the world streams in around them.
//! Time of day (N), style (T) and shadows (G) still work, so a screenshot can
//! wait for the light. A screenshot is the frame rendered again into an offscreen
//! texture with an empty HUD, read back, and written by a background thread.

use crate::game::Actions;
use crate::modes::{Activity, ActivityCtx, ModeId};
use crate::physics::hits;
use crate::player::MoveInput;
use crate::player::Player;
use crate::render::hud::Hud;
use crate::rig::Camera;
use crate::world::store::Store;
use glam::Vec3;
use std::path::{Path, PathBuf};

/// The camera stays within this many metres of the player.
pub const RANGE: f32 = 120.0;
/// Zoom limits, as a multiple of the normal field of view.
const ZOOM_MIN: f32 = 0.15;
const ZOOM_MAX: f32 = 1.6;

pub struct Screenshot {
    pub pos: Vec3,
    vel: Vec3,
    /// Tilt of the horizon, radians.
    pub roll: f32,
    /// Field of view as a multiple of the normal one (smaller is closer).
    pub zoom: f32,
    /// The player's view when screenshot mode began, restored on leaving.
    saved_yaw: f32,
    saved_pitch: f32,
    /// Seconds left of the white flash after a screenshot.
    pub flash: f32,
    /// The left button was down last frame. A held button comes back in
    /// `actions.dig` every frame (main.rs), so only the press takes a shot.
    clicking: bool,
}

pub fn forward(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(-sy * cp, sp, -cy * cp)
}

impl Screenshot {
    /// Start from where the camera is now; `yaw` and `pitch` are the player's.
    pub fn new(eye: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            pos: eye,
            vel: Vec3::ZERO,
            roll: 0.0,
            zoom: 1.0,
            saved_yaw: yaw,
            saved_pitch: pitch,
            flash: 0.0,
            clicking: false,
        }
    }

    /// The player's view to restore on leaving.
    pub fn saved_view(&self) -> (f32, f32) {
        (self.saved_yaw, self.saved_pitch)
    }

    /// Wheel steps: positive zooms in.
    pub fn zoom_by(&mut self, steps: i32) {
        self.zoom = (self.zoom * 0.85f32.powi(steps)).clamp(ZOOM_MIN, ZOOM_MAX);
    }

    /// Tilt the horizon by `deg` degrees, snapping back to level near zero.
    pub fn tilt(&mut self, deg: f32) {
        self.roll = (self.roll + deg.to_radians()).clamp(-0.8, 0.8);
        if self.roll.abs() < 0.01 {
            self.roll = 0.0;
        }
    }

    /// Fly the camera for one frame. `yaw` and `pitch` come from the mouse
    /// (the player's view); `anchor` is the player, whom the camera stays near.
    pub fn fly(&mut self, input: &MoveInput, yaw: f32, pitch: f32, anchor: Vec3, store: &Store, dt: f32) {
        let fwd = forward(yaw, pitch);
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let f = input.forward as i32 as f32 - input.back as i32 as f32;
        let r = input.right as i32 as f32 - input.left as i32 as f32;
        let u = input.jump as i32 as f32 - input.down as i32 as f32;
        let wish = (fwd * f + right * r + Vec3::Y * u).normalize_or_zero();
        // Slower when zoomed in, so framing stays fine.
        let speed = if input.sprint { 18.0 } else { 4.0 } * self.zoom.sqrt();
        self.vel = self.vel.lerp(wish * speed, (dt * 8.0).min(1.0));
        let mut next = self.pos + self.vel * dt;
        let d = next - anchor;
        if d.length() > RANGE {
            next = anchor + d * (RANGE / d.length());
        }
        // No going into the rock: a small box round the lens.
        if !hits(store, next - Vec3::Y * 0.1, 0.1, 0.2) {
            self.pos = next;
        } else {
            self.vel = Vec3::ZERO;
        }
        self.flash = (self.flash - dt).max(0.0);
    }

    pub fn camera(&self, yaw: f32, pitch: f32) -> Camera {
        Camera::looking(self.pos, yaw, pitch).rolled(self.roll)
    }
}

impl Activity for Screenshot {
    fn id(&self) -> ModeId {
        ModeId::Screenshot
    }

    /// The camera flies; the wheel zooms, Q and E tilt, a click takes a
    /// screenshot. All of it once a frame: the world is paused and the
    /// camera is the view, which is smoother moved every frame than stepped.
    fn frame(&mut self, cx: &mut ActivityCtx, input: &MoveInput, actions: &mut Actions, dt: f32) {
        if actions.brush_delta != 0 {
            self.zoom_by(actions.brush_delta);
            actions.brush_delta = 0;
        }
        if std::mem::take(&mut actions.tilt_left) {
            self.tilt(-5.0);
        }
        if std::mem::take(&mut actions.tilt_right) {
            self.tilt(5.0);
        }
        let down = std::mem::take(&mut actions.dig);
        if down && !self.clicking {
            *cx.screenshot_request = true;
        }
        self.clicking = down;
        self.fly(input, cx.player.yaw, cx.player.pitch, cx.player.pos, cx.store, dt);
    }

    fn pauses_world(&self) -> bool {
        true
    }

    fn camera(&self, player: &Player) -> Option<Camera> {
        Some(Screenshot::camera(self, player.yaw, player.pitch))
    }

    fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Screenshot mode: a thirds grid, the keys and the lens, none of it in the image.
    fn hud(&self, hud: &mut Hud, w: f32, h: f32, s: f32, fov: f32) {
        let ph = self;
        if ph.flash > 0.0 {
            hud.rect(0.0, 0.0, w, h, [1.0, 1.0, 1.0, (ph.flash * 2.4).min(0.6)]);
        }
        for k in [1.0f32, 2.0] {
            hud.rect((w * k / 3.0).floor(), 0.0, 1.0, h, [1.0, 1.0, 1.0, 0.14]);
            hud.rect(0.0, (h * k / 3.0).floor(), w, 1.0, [1.0, 1.0, 1.0, 0.14]);
        }
        // The lens as a full-frame focal length (24 mm tall sensor).
        let mm = 12.0 / (fov * 0.5).tan();
        let tilt = ph.roll.to_degrees();
        let lens = if tilt.abs() > 0.5 { format!("{mm:.0} MM   TILT {tilt:+.0}") } else { format!("{mm:.0} MM") };
        let lines = [
            "SCREENSHOT MODE",
            "ENTER OR CLICK  TAKE A SCREENSHOT",
            "WASD  SPACE  CTRL  MOVE    SHIFT  FASTER",
            "WHEEL  ZOOM    Q E  TILT",
            "N  TIME OF DAY    T  STYLE    G  SHADOWS",
            "F2 OR ESC  BACK TO THE GAME",
        ];
        let line = 10.0 * s;
        let wmax = lines.iter().map(|l| Hud::text_width(l, s)).fold(0.0, f32::max);
        let y0 = h - line * (lines.len() as f32 + 1.0) - 16.0;
        hud.rect(12.0, y0 - 8.0, wmax + 16.0, line * (lines.len() as f32 + 1.0) + 12.0, [0.0, 0.0, 0.0, 0.35]);
        for (i, l) in lines.iter().enumerate() {
            let c = if i == 0 { [1.0, 0.95, 0.9, 0.95] } else { [1.0, 0.95, 0.9, 0.75] };
            hud.label(20.0, y0 + i as f32 * line, s, c, l);
        }
        hud.label(20.0, y0 + lines.len() as f32 * line, s, [1.0, 0.95, 0.9, 0.9], &lens);
    }

    fn shutter(&mut self) {
        self.flash = 0.25;
    }

    fn exit(&mut self, player: &mut Player) {
        (player.yaw, player.pitch) = self.saved_view();
    }
}

/// The first free `<dir>/<world>-NNNN.png`.
pub fn next_path(dir: &Path, world: &str) -> PathBuf {
    let world: String = world.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    (1..100_000)
        .map(|n| dir.join(format!("{world}-{n:04}.png")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{world}.png")))
}

/// Write RGBA (or BGRA, when `bgra`) pixels as a PNG on a background thread.
pub fn write_png(mut pixels: Vec<u8>, w: u32, h: u32, bgra: bool, path: PathBuf) -> std::thread::JoinHandle<Result<PathBuf, String>> {
    std::thread::spawn(move || {
        if bgra {
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        // The frame's alpha isn't meaningful: make it opaque.
        for px in pixels.chunks_exact_mut(4) {
            px[3] = 255;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        crate::util::save_png(&path, w, h, &pixels).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    })
}

/// Render the current frame again into an offscreen texture without any HUD,
/// read it back and start writing it to `path`. Call after `prepare_frame`.
pub fn capture(
    gpu: &crate::render::Gpu,
    renderer: &mut crate::render::Renderer,
    game: &crate::game::Game,
    format: wgpu::TextureFormat,
    w: u32,
    h: u32,
    path: PathBuf,
) -> std::thread::JoinHandle<Result<PathBuf, String>> {
    let pixels = render_rgba(gpu, renderer, game, format, w, h, None);
    write_png(pixels, w, h, false, path)
}

/// Render the current frame again offscreen, without any HUD, through `lens`
/// (none: no depth of field), and read it back as RGBA.
pub fn render_rgba(
    gpu: &crate::render::Gpu,
    renderer: &mut crate::render::Renderer,
    game: &crate::game::Game,
    format: wgpu::TextureFormat,
    w: u32,
    h: u32,
    lens: Option<crate::render::LensFx>,
) -> Vec<u8> {
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let mut params = game.frame_params(w as f32 / h.max(1) as f32);
    params.lens = lens;
    renderer.render(&game.streamer, &params, &crate::render::hud::Hud::default(), &view);
    let mut pixels = crate::render::read_texture(gpu, &tex, w, h);
    let bgra = matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    for px in pixels.chunks_exact_mut(4) {
        if bgra {
            px.swap(0, 2);
        }
        px[3] = 255;
    }
    pixels
}

/// A camera photo: the frame rendered through its lens and cut out of the
/// screen (`crop` = x, y, w, h). RGBA and its size.
pub fn develop(
    gpu: &crate::render::Gpu,
    renderer: &mut crate::render::Renderer,
    game: &crate::game::Game,
    format: wgpu::TextureFormat,
    w: u32,
    h: u32,
    plan: &crate::game::ShotPlan,
) -> (Vec<u8>, u32, u32) {
    let px = render_rgba(gpu, renderer, game, format, w, h, Some(plan.lens));
    let [x0, y0, cw, ch] = plan.crop;
    (crop(&px, w, x0, y0, cw, ch), cw, ch)
}

/// Cut a `cw` x `ch` rectangle at (x0, y0) out of RGBA pixels `w` wide.
pub fn crop(px: &[u8], w: u32, x0: u32, y0: u32, cw: u32, ch: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for y in y0..y0 + ch {
        let row = ((y * w + x0) * 4) as usize;
        out.extend_from_slice(&px[row..row + (cw * 4) as usize]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::store::Voxels;

    fn open_air() -> Store {
        let mut s = Store::default();
        for cz in -2..2 {
            for cx in -2..2 {
                for cy in 0..4 {
                    s.chunks.insert((cx, cy, cz), Voxels::Uniform(0));
                }
            }
        }
        s
    }

    /// The camera flies where it looks and never strays beyond `RANGE`.
    #[test]
    fn flies_and_stays_near_the_player() {
        let s = open_air();
        let mut ph = Screenshot::new(Vec3::new(0.0, 10.0, 0.0), 0.0, 0.0);
        let go = MoveInput {
            forward: true,
            sprint: true,
            ..Default::default()
        };
        for _ in 0..60 {
            ph.fly(&go, 0.0, 0.0, Vec3::new(0.0, 10.0, 0.0), &s, 1.0 / 60.0);
        }
        assert!(ph.pos.z < -8.0, "should fly north (-Z): {:?}", ph.pos);
        for _ in 0..1200 {
            ph.fly(&go, 0.0, 0.0, Vec3::new(0.0, 10.0, 0.0), &s, 1.0 / 60.0);
        }
        assert!((ph.pos - Vec3::new(0.0, 10.0, 0.0)).length() <= RANGE + 0.01);
    }

    /// Zoom and tilt stay in range, and tilt snaps level near zero.
    #[test]
    fn zoom_and_tilt_are_bounded() {
        let mut ph = Screenshot::new(Vec3::ZERO, 0.0, 0.0);
        ph.zoom_by(100);
        assert!((ph.zoom - ZOOM_MIN).abs() < 1e-6);
        ph.zoom_by(-100);
        assert!((ph.zoom - ZOOM_MAX).abs() < 1e-6);
        ph.tilt(5.0);
        ph.tilt(-5.0);
        assert_eq!(ph.roll, 0.0);
        let cam = ph.camera(0.0, 0.0);
        assert!((cam.up - Vec3::Y).length() < 1e-5, "level camera up {:?}", cam.up);
        ph.tilt(10.0);
        let cam = ph.camera(0.0, 0.0);
        assert!(cam.up.x < -0.1, "tilting right lowers the right side: up {:?}", cam.up);
    }

    /// Screenshots are numbered from the first free name and written as PNG.
    #[test]
    fn numbers_and_writes_screenshots() {
        let dir = std::env::temp_dir().join(format!("voxel_screenshots_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = next_path(&dir, "glass fall");
        assert!(a.ends_with("glass_fall-0001.png"), "{}", a.display());
        let px = vec![10u8, 20, 30, 0, 40, 50, 60, 0];
        let out = write_png(px, 2, 1, true, a.clone()).join().unwrap().unwrap();
        assert_eq!(out, a);
        assert!(next_path(&dir, "glass fall").ends_with("glass_fall-0002.png"));
        let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&a).unwrap()));
        let mut r = dec.read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        r.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[..8], &[30, 20, 10, 255, 60, 50, 40, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
