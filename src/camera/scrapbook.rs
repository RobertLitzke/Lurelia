//! The scrapbook: every photo the player kept, in the order taken, four to a
//! page. A photo that did a task carries a ribbon with the task and its
//! reward. Images live in `<save>/scrapbook/NNNN.png` and the list in
//! `<save>/scrapbook.json`; pages are drawn on the CPU into the HUD's image
//! (render/hud.rs IMAGE_PX square) with the text laid over by the HUD.

use crate::render::hud::IMAGE_PX;
use crate::util::FxHashMap as HashMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PER_PAGE: usize = 4;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    /// The image, relative to the scrapbook folder.
    pub file: String,
    /// The task it did, if any.
    pub task: Option<String>,
    pub needs: Vec<String>,
    pub reward: u32,
    pub hour: f32,
    /// What was sharp in the frame.
    pub subjects: Vec<String>,
}

/// A small copy of an image for the pages: RGBA, `w` x `h`.
#[derive(Clone)]
pub struct Thumb {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Thumb {
    /// Shrink RGBA pixels (box filter) to fit within `max_w` x `max_h`.
    pub fn fit(px: &[u8], w: usize, h: usize, max_w: usize, max_h: usize) -> Thumb {
        let k = (max_w as f32 / w as f32).min(max_h as f32 / h as f32).min(1.0);
        let (tw, th) = (((w as f32 * k) as usize).max(1), ((h as f32 * k) as usize).max(1));
        let mut out = vec![0u8; tw * th * 4];
        for ty in 0..th {
            let (y0, y1) = (ty * h / th, ((ty + 1) * h / th).max(ty * h / th + 1));
            for tx in 0..tw {
                let (x0, x1) = (tx * w / tw, ((tx + 1) * w / tw).max(tx * w / tw + 1));
                let mut acc = [0u32; 4];
                for y in y0..y1.min(h) {
                    for x in x0..x1.min(w) {
                        let i = (y * w + x) * 4;
                        for c in 0..4 {
                            acc[c] += px[i + c] as u32;
                        }
                    }
                }
                let n = ((y1.min(h) - y0) * (x1.min(w) - x0)).max(1) as u32;
                let o = (ty * tw + tx) * 4;
                for c in 0..4 {
                    out[o + c] = (acc[c] / n) as u8;
                }
            }
        }
        Thumb { w: tw, h: th, px: out }
    }
}

/// A label the HUD draws over a page, in page pixels (0..IMAGE_PX).
pub struct Label {
    pub x: f32,
    pub y: f32,
    /// Text size: 1 is the HUD's normal size, smaller below.
    pub scale: f32,
    /// Widest the text may run, page pixels (cut short past it).
    pub max_w: f32,
    pub color: [f32; 4],
    pub text: String,
}

#[derive(Default)]
pub struct Scrapbook {
    pub entries: Vec<Entry>,
    /// Where it is kept; None in runs that mustn't write (headless).
    pub dir: Option<PathBuf>,
    thumbs: HashMap<String, Thumb>,
}

/// Room for a photo in one quarter of the page.
const CELL_W: usize = IMAGE_PX / 2;
const CELL_H: usize = IMAGE_PX / 2;
const PHOTO_W: usize = 450;
const PHOTO_H: usize = 330;
const PAPER: [u8; 3] = [236, 226, 204];
const RIBBON: [u8; 3] = [178, 52, 44];

impl Scrapbook {
    /// Load the list from `dir` (the world's save folder).
    pub fn open(save_dir: &Path) -> Scrapbook {
        // The list is the only record of which photo met which task. One
        // that will not parse is set aside, because the next photo taken
        // writes the list again from whatever loaded here.
        let path = save_dir.join("scrapbook.json");
        let entries = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                crate::util::set_aside(&path, format_args!("{e}; the scrapbook starts empty (the photos are still in the folder)"));
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        Scrapbook {
            entries,
            dir: Some(save_dir.to_path_buf()),
            thumbs: HashMap::default(),
        }
    }

    pub fn pages(&self) -> usize {
        self.entries.len().div_ceil(PER_PAGE)
    }

    /// Add a photo: RGBA pixels. Starts writing the PNG and the list; returns
    /// the writer, or None when this run doesn't keep files.
    pub fn add(&mut self, mut e: Entry, px: Vec<u8>, w: u32, h: u32) -> Option<std::thread::JoinHandle<Result<PathBuf, String>>> {
        let n = self.entries.len() + 1;
        e.file = format!("{n:04}.png");
        self.thumbs.insert(e.file.clone(), Thumb::fit(&px, w as usize, h as usize, PHOTO_W, PHOTO_H));
        self.entries.push(e);
        let dir = self.dir.clone()?;
        let list = serde_json::to_string_pretty(&self.entries).unwrap_or_default();
        let _ = std::fs::create_dir_all(&dir);
        let tmp = dir.join("scrapbook.tmp");
        if std::fs::write(&tmp, list).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("scrapbook.json"));
        }
        let path = dir.join("scrapbook").join(&self.entries.last().unwrap().file);
        Some(crate::screenshot::write_png(px, w, h, false, path))
    }

    fn thumb(&mut self, file: &str) -> Option<&Thumb> {
        if !self.thumbs.contains_key(file) {
            let path = self.dir.as_ref()?.join("scrapbook").join(file);
            let t = read_png(&path).map(|(px, w, h)| Thumb::fit(&px, w, h, PHOTO_W, PHOTO_H))?;
            self.thumbs.insert(file.to_string(), t);
        }
        self.thumbs.get(file)
    }

    /// Page `page` (0-based) as sRGB RGBA, IMAGE_PX square, and its labels.
    pub fn draw_page(&mut self, page: usize) -> (Vec<u8>, Vec<Label>) {
        let n = IMAGE_PX;
        let mut img = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                // Paper with a faint grain.
                let g = (crate::world::noise::hash2(x as i32, y as i32, 0x5c4a) & 7) as i32 - 3;
                let i = (y * n + x) * 4;
                for c in 0..3 {
                    img[i + c] = (PAPER[c] as i32 + g).clamp(0, 255) as u8;
                }
                img[i + 3] = 255;
            }
        }
        let mut labels = Vec::new();
        let ink = [0.16, 0.12, 0.08, 1.0];
        let first = page * PER_PAGE;
        let entries: Vec<Entry> = self.entries.iter().skip(first).take(PER_PAGE).cloned().collect();
        for (k, e) in entries.iter().enumerate() {
            let (cx, cy) = ((k % 2) * CELL_W, (k / 2) * CELL_H);
            // The photo, centred in the top of the cell on a white border, with
            // a soft shadow.
            let Some(t) = self.thumb(&e.file).cloned() else {
                labels.push(Label {
                    max_w: (CELL_W - 60) as f32,
                    x: (cx + 40) as f32,
                    y: (cy + 150) as f32,
                    scale: 1.0,
                    color: ink,
                    text: format!("({} IS MISSING)", e.file),
                });
                continue;
            };
            let px0 = cx + (CELL_W - t.w) / 2;
            let py0 = cy + 24 + (PHOTO_H - t.h) / 2;
            let tilt = if k % 3 == 1 { 1 } else { 0 };
            fill(&mut img, px0 as i32 - 6, py0 as i32 - 2, t.w + 16, t.h + 16, [196, 186, 166], 0.6);
            fill(&mut img, px0 as i32 - 10 + tilt, py0 as i32 - 10, t.w + 20, t.h + 20, [252, 250, 244], 1.0);
            for y in 0..t.h {
                for x in 0..t.w {
                    let s = (y * t.w + x) * 4;
                    let d = ((py0 + y) * n + px0 + x) * 4;
                    img[d..d + 3].copy_from_slice(&t.px[s..s + 3]);
                }
            }
            // Corner tabs holding it to the page.
            for (tx, ty) in [
                (px0 as i32 - 14, py0 as i32 - 14),
                (px0 as i32 + t.w as i32 - 10, py0 as i32 - 14),
                (px0 as i32 - 14, py0 as i32 + t.h as i32 - 10),
                (px0 as i32 + t.w as i32 - 10, py0 as i32 + t.h as i32 - 10),
            ] {
                fill(&mut img, tx, ty, 24, 24, [120, 96, 70], 0.85);
            }
            let below = cy + 24 + PHOTO_H + 20;
            if let Some(task) = &e.task {
                // A ribbon across the photo's lower edge with notched ends.
                let (rx, ry, rw, rh) = (cx as i32 + 30, (py0 + t.h) as i32 - 22, CELL_W - 60, 34usize);
                fill(&mut img, rx, ry + 3, rw, rh, [90, 30, 26], 0.5);
                fill(&mut img, rx, ry, rw, rh, RIBBON, 1.0);
                for j in 0..rh as i32 {
                    let notch = 12 - (j - rh as i32 / 2).abs().min(12);
                    fill(&mut img, rx, ry + j, notch as usize, 1, PAPER, 1.0);
                    fill(&mut img, rx + rw as i32 - notch, ry + j, notch as usize, 1, PAPER, 1.0);
                }
                labels.push(Label {
                    max_w: (rw - 40) as f32,
                    x: (rx + 24) as f32,
                    y: (ry + 10) as f32,
                    scale: 1.0,
                    color: [1.0, 0.95, 0.82, 1.0],
                    text: format!("TASK DONE  +{}", e.reward),
                });
                labels.push(Label {
                    max_w: (CELL_W - 150) as f32,
                    x: (cx + 34) as f32,
                    y: (below + 6) as f32,
                    scale: 1.0,
                    color: ink,
                    text: task.clone(),
                });
                for (i, l) in e.needs.iter().enumerate().take(3) {
                    labels.push(Label {
                        max_w: (CELL_W - 60) as f32,
                        x: (cx + 34) as f32,
                        y: (below + 36 + i * 30) as f32,
                        scale: 1.0,
                        color: [0.32, 0.26, 0.2, 1.0],
                        text: format!("- {l}"),
                    });
                }
            } else {
                labels.push(Label {
                    max_w: (CELL_W - 150) as f32,
                    x: (cx + 34) as f32,
                    y: (below + 6) as f32,
                    scale: 1.0,
                    color: ink,
                    text: "SNAPSHOT".into(),
                });
                if !e.subjects.is_empty() {
                    labels.push(Label {
                        max_w: (CELL_W - 60) as f32,
                        x: (cx + 34) as f32,
                        y: (below + 36) as f32,
                        scale: 1.0,
                        color: [0.32, 0.26, 0.2, 1.0],
                        text: e.subjects.join(", "),
                    });
                }
            }
            let hh = e.hour.floor();
            labels.push(Label {
                max_w: 120.0,
                x: (cx + CELL_W - 110) as f32,
                y: (below + 6) as f32,
                scale: 1.0,
                color: [0.4, 0.33, 0.26, 1.0],
                text: format!("{:02}:{:02}", hh as u32, ((e.hour - hh) * 60.0) as u32),
            });
        }
        if entries.is_empty() {
            labels.push(Label {
                max_w: 600.0,
                x: 360.0,
                y: 480.0,
                scale: 2.0,
                color: ink,
                text: "NO PHOTOS YET".into(),
            });
        }
        (img, labels)
    }
}

/// Blend a rectangle of `rgb` over the image at `alpha`, clipped.
fn fill(img: &mut [u8], x: i32, y: i32, w: usize, h: usize, rgb: [u8; 3], alpha: f32) {
    let n = IMAGE_PX as i32;
    for yy in y.max(0)..(y + h as i32).min(n) {
        for xx in x.max(0)..(x + w as i32).min(n) {
            let i = ((yy * n + xx) * 4) as usize;
            for c in 0..3 {
                img[i + c] = (img[i + c] as f32 * (1.0 - alpha) + rgb[c] as f32 * alpha) as u8;
            }
        }
    }
}

/// Decode an 8-bit RGB or RGBA PNG to RGBA.
fn read_png(path: &Path) -> Option<(Vec<u8>, usize, usize)> {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let px = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3].chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        _ => return None,
    };
    Some((px, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Photos are kept, listed and shown again after reopening.
    #[test]
    fn photos_survive_reopening() {
        let dir = std::env::temp_dir().join(format!("voxel_scrapbook_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut book = Scrapbook::open(&dir);
        let px: Vec<u8> = (0..60 * 40).flat_map(|i| [(i % 60) as u8 * 4, 80, 120, 255]).collect();
        let e = Entry {
            task: Some("DEER EATING".into()),
            needs: vec!["ANIMAL: DEER".into()],
            reward: 25,
            hour: 17.5,
            ..Default::default()
        };
        book.add(e, px.clone(), 60, 40).unwrap().join().unwrap().unwrap();
        book.add(Entry::default(), px, 60, 40).unwrap().join().unwrap().unwrap();
        let mut again = Scrapbook::open(&dir);
        assert_eq!(again.entries.len(), 2);
        assert_eq!(again.entries[0].file, "0001.png");
        assert_eq!(again.pages(), 1);
        let (img, labels) = again.draw_page(0);
        assert_eq!(img.len(), IMAGE_PX * IMAGE_PX * 4);
        assert!(labels.iter().any(|l| l.text == "TASK DONE  +25"));
        assert!(labels.iter().any(|l| l.text == "SNAPSHOT"));
        assert!(!labels.iter().any(|l| l.text.contains("MISSING")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn thumbnails_fit_their_box() {
        let px = vec![200u8; 900 * 600 * 4];
        let t = Thumb::fit(&px, 900, 600, PHOTO_W, PHOTO_H);
        assert!(t.w <= PHOTO_W && t.h <= PHOTO_H && t.px.len() == t.w * t.h * 4);
        assert_eq!(t.px[0], 200);
    }
}
