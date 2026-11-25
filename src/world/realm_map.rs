//! Which realms each 4 m site cell's life comes from (`library/realms/`).
//!
//! A world has a mix (`WorldDef::realms`, all `real` when unset). On top of
//! it each cell carries up to two overlays, one byte each: four bits of
//! realm (an index into this world's realm table, 0 for none) and four bits
//! of strength (sixteen levels). They are painted when the site fields are
//! built:
//!
//! - the chronicle's strangeness (`Fields::strange`, which the `impact` and
//!   `bloom` events raise), the mean over the cell, as `bloom` or whatever
//!   realm the world says it stands for (`WorldDef::strange_realm`);
//! - each biome's own overlay (`BiomeDef::realm`: a fae glade, a relict
//!   Mesozoic wood) over its cells;
//! - `realm` chronicle events (`Fields::realm_paint`), fading from a point.
//!
//! Painting a third realm onto a cell replaces the weaker of its two. A
//! cell's share of a realm is its overlays at their strength (scaled down if
//! they sum past 1), and the rest of the cell is the world mix. About 300 KB
//! on a 1536 m world. Plan: `notes/LIBRARY_HABITATS_PLAN.md` (H9).

use super::fields::World;
use super::survey::{self, CELL_M};
use glam::Vec2;

/// A `realm` chronicle event's paint, kept on the fields until the site
/// fields are built.
#[derive(Clone, Debug)]
pub struct Paint {
    pub realm: String,
    pub at: Vec2,
    /// Full strength out to here, metres.
    pub radius: f32,
    /// Then fading to nothing over this, metres.
    pub edge: f32,
    pub strength: f32,
}

/// Most realms one world can name: four bits, 0 meaning none.
pub const MAX_REALMS: usize = 15;

#[derive(Clone, Debug, Default)]
pub struct RealmMap {
    pub n: usize,
    /// This world's realm table; index 0 is unused (no overlay).
    pub names: Vec<String>,
    /// The world mix, by table index, shares summing to 1.
    pub mix: Vec<(u8, f32)>,
    overlay: Vec<[u8; 2]>,
}

fn pack(realm: u8, strength: f32) -> u8 {
    (realm << 4) | (strength.clamp(0.0, 1.0) * 15.0).round() as u8
}

fn unpack(b: u8) -> (u8, f32) {
    (b >> 4, (b & 15) as f32 / 15.0)
}

impl RealmMap {
    /// Build the map for `w`, whose site cells are counted in `biome`
    /// (library index per 4 m cell, `Site::biome`).
    pub fn build(w: &World, biome: &[u8], n: usize) -> RealmMap {
        let f = &w.f;
        let mut m = RealmMap { n, names: vec![String::new()], mix: Vec::new(), overlay: vec![[0, 0]; n * n] };
        // The world mix.
        let mix: Vec<(String, f32)> = match w.def.realms.as_ref().filter(|r| !r.is_empty()) {
            Some(r) => r.iter().filter(|(_, &s)| s > 0.0).map(|(k, &s)| (k.clone(), s)).collect(),
            None => vec![("real".to_string(), 1.0)],
        };
        let total: f32 = mix.iter().map(|x| x.1).sum::<f32>().max(1e-6);
        for (name, s) in &mix {
            if let Some(i) = m.index(name) {
                m.mix.push((i, s / total));
            }
        }
        // The strangeness, the mean over each cell's 1 m cells, as the realm
        // this world says it stands for (`bloom` unless it says otherwise).
        let n1 = f.n;
        let sub = (CELL_M / (f.size / n1 as f32)).round().max(1.0) as usize;
        if f.strange.iter().any(|&s| s > 0.0) {
            if let Some(bloom) = m.index(w.def.strange_realm.as_deref().unwrap_or("bloom")) {
                for cz in 0..n {
                    for cx in 0..n {
                        let (mut sum, mut cnt) = (0.0f32, 0.0f32);
                        for sz in 0..sub {
                            for sx in 0..sub {
                                let (x, z) = (cx * sub + sx, cz * sub + sz);
                                if x < n1 && z < n1 {
                                    sum += f.strange[z * n1 + x];
                                    cnt += 1.0;
                                }
                            }
                        }
                        let s = sum / cnt.max(1.0);
                        if s > 0.5 / 15.0 {
                            m.put(cz * n + cx, bloom, s);
                        }
                    }
                }
            }
        }
        // Each biome's own overlay over its cells.
        for (bi, b) in w.lib.biomes.iter().enumerate() {
            let Some(over) = b.realm.as_ref() else { continue };
            for (name, &s) in over {
                let Some(r) = m.index(name) else { continue };
                for (k, _) in biome.iter().enumerate().filter(|(_, &c)| c as usize == bi) {
                    m.put(k, r, s);
                }
            }
        }
        // Chronicle `realm` events.
        for p in &f.realm_paint {
            let Some(r) = m.index(&p.realm) else { continue };
            let reach = p.radius + p.edge;
            let (lo, hi) = (p.at - Vec2::splat(reach), p.at + Vec2::splat(reach));
            let c = |v: f32| ((v / CELL_M).floor() as i64).clamp(0, n as i64 - 1) as usize;
            for cz in c(lo.y)..=c(hi.y) {
                for cx in c(lo.x)..=c(hi.x) {
                    let q = Vec2::new((cx as f32 + 0.5) * CELL_M, (cz as f32 + 0.5) * CELL_M);
                    let d = q.distance(p.at);
                    let k = if d <= p.radius { 1.0 } else { 1.0 - ((d - p.radius) / p.edge.max(1e-3)).min(1.0) };
                    if k > 0.0 {
                        m.put(cz * n + cx, r, p.strength * k);
                    }
                }
            }
        }
        m
    }

    /// The table index of a realm, adding it if there is room.
    fn index(&mut self, name: &str) -> Option<u8> {
        if let Some(i) = self.names.iter().position(|x| x == name) {
            return Some(i as u8);
        }
        if self.names.len() > MAX_REALMS {
            return None;
        }
        self.names.push(name.to_string());
        Some((self.names.len() - 1) as u8)
    }

    /// Paint realm `r` onto cell `k` at strength `s`: raises its overlay if
    /// it has one, takes an empty slot, or replaces the weaker of two others.
    fn put(&mut self, k: usize, r: u8, s: f32) {
        let o = &mut self.overlay[k];
        let have = [unpack(o[0]), unpack(o[1])];
        let slot = if let Some(j) = have.iter().position(|h| h.0 == r && h.1 > 0.0) {
            if have[j].1 >= s {
                return;
            }
            j
        } else if let Some(j) = have.iter().position(|h| h.1 <= 0.0) {
            j
        } else {
            let j = if have[0].1 <= have[1].1 { 0 } else { 1 };
            if have[j].1 >= s {
                return;
            }
            j
        };
        o[slot] = pack(r, s);
    }

    /// Each realm's share of cell `k`, by table index: the overlays at their
    /// strength (scaled down if they sum past 1), the rest the world mix.
    pub fn shares(&self, k: usize) -> Vec<(u8, f32)> {
        let o = self.overlay[k];
        let ov: Vec<(u8, f32)> = o.iter().map(|&b| unpack(b)).filter(|x| x.1 > 0.0 && x.0 != 0).collect();
        let sum: f32 = ov.iter().map(|x| x.1).sum();
        let scale = if sum > 1.0 { 1.0 / sum } else { 1.0 };
        let mut out: Vec<(u8, f32)> = ov.iter().map(|&(r, s)| (r, s * scale)).collect();
        let rest = 1.0 - sum.min(1.0);
        for &(r, s) in &self.mix {
            match out.iter_mut().find(|x| x.0 == r) {
                Some(x) => x.1 += rest * s,
                None => out.push((r, rest * s)),
            }
        }
        out
    }

    pub fn idx(&self, x: f32, z: f32) -> usize {
        survey::cell_index(self.n, x, z)
    }

    /// The share of world point (x, z) that belongs to any of `realms`.
    pub fn share(&self, x: f32, z: f32, realms: &[String]) -> f32 {
        if self.n == 0 {
            return 1.0;
        }
        self.shares(self.idx(x, z)).iter().filter(|(r, _)| realms.iter().any(|n| self.names.get(*r as usize) == Some(n))).map(|x| x.1).sum()
    }

    /// The realms present anywhere among cells `ks` (by name, sorted).
    pub fn present_in(&self, ks: impl Iterator<Item = usize>) -> Vec<String> {
        let mut seen = vec![false; self.names.len()];
        for (r, _) in &self.mix {
            seen[*r as usize] = true;
        }
        for k in ks {
            for b in self.overlay[k] {
                let (r, s) = unpack(b);
                if r != 0 && s > 0.0 {
                    seen[r as usize] = true;
                }
            }
        }
        let mut v: Vec<String> = seen.iter().enumerate().filter(|(_, &s)| s).map(|(i, _)| self.names[i].clone()).collect();
        v.sort();
        v
    }

    /// The strongest overlay on cell `k`: (table index, strength), or none.
    pub fn strongest(&self, k: usize) -> Option<(u8, f32)> {
        self.overlay[k].iter().map(|&b| unpack(b)).filter(|x| x.0 != 0 && x.1 > 0.0).max_by(|a, b| a.1.total_cmp(&b.1))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> RealmMap {
        let mut m = RealmMap { n: 1, names: vec![String::new()], mix: Vec::new(), overlay: vec![[0, 0]] };
        let real = m.index("real").unwrap();
        m.mix.push((real, 1.0));
        m
    }

    #[test]
    fn overlays_blend_with_the_mix() {
        let mut m = map();
        let fae = m.index("fae").unwrap();
        m.put(0, fae, 0.6);
        let s = m.shares(0);
        let get = |r| s.iter().find(|x| x.0 == r).map_or(0.0, |x| x.1);
        assert!((get(fae) - 0.6).abs() < 0.04);
        assert!((get(1) - 0.4).abs() < 0.04);
    }

    #[test]
    fn a_third_realm_replaces_the_weaker() {
        let mut m = map();
        let (a, b, c) = (m.index("fae").unwrap(), m.index("bloom").unwrap(), m.index("dinosaur").unwrap());
        m.put(0, a, 0.8);
        m.put(0, b, 0.3);
        m.put(0, c, 0.5);
        let s = m.shares(0);
        assert!(s.iter().any(|x| x.0 == a) && s.iter().any(|x| x.0 == c) && !s.iter().any(|x| x.0 == b));
        // Strong overlays crowd the mix out.
        assert!(s.iter().find(|x| x.0 == 1).map_or(0.0, |x| x.1) < 1e-6);
    }
}
