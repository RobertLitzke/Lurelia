//! `Library::validate`: every problem with the library and the world it is
//! loaded for, for `--validate` and the load screen.

#[allow(unused_imports)]
use super::*;

impl Library {
    /// Cross-references that don't resolve, duplicate names and ids, bad verbs.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashMap::new();
        for m in &self.materials {
            if let Some(i) = m.file_id.filter(|i| !(17..255).contains(i)) {
                out.push(format!("material {}: id {i} outside 17..254", m.name));
            }
            if let Some(t) = &m.tint {
                if !crate::world::matlib::TINTS.contains(&t.as_str()) {
                    out.push(format!("material {}: tint must be one of {:?}", m.name, crate::world::matlib::TINTS));
                }
            }
            if let Some(d) = &m.drift {
                // The field is shipped as the multiply that takes the
                // material's own texture to each end, so an end the texture
                // cannot reach is worth saying before the world is built.
                let face = m.top.as_ref().unwrap_or(&m.texture);
                let (calm, classic) = d.pairs();
                for (style, pair, base) in [("calm", calm, face.calm.as_ref().map_or(face.base, |c| c.base)), ("classic", classic, face.base)] {
                    let Some([lo, hi]) = pair else { continue };
                    let gap = [0, 1, 2].map(|i| lo[i] - hi[i]);
                    if (gap[0] * gap[0] + gap[1] * gap[1] + gap[2] * gap[2]).sqrt() < MIN_APART {
                        out.push(format!("material {}: the two ends of its {style} drift are the same colour to look at", m.name));
                    }
                    for (end, c) in [("first", lo), ("second", hi)] {
                        if let Some(i) = (0..3).find(|&i| base[i] <= 1.0 || !(0.0..=2.0).contains(&(c[i] / base[i]))) {
                            out.push(format!(
                                "material {}: the {end} end of its {style} drift is out of reach in {}: the texture is {:.0} there and the drift asks for {:.0}",
                                m.name,
                                ["red", "green", "blue"][i],
                                base[i],
                                c[i]
                            ));
                            break;
                        }
                    }
                }
            }
            if let Some(other) = m.file_id.and_then(|i| seen.insert(i, m.name.clone())) {
                out.push(format!("material {}: id {} already used by {other}", m.name, m.file_id.unwrap_or(0)));
            }
            for r in [Some(&m.texture), m.top.as_ref(), m.bottom.as_ref()].into_iter().flatten() {
                if !crate::render::recipes::PATTERNS.contains(&r.pattern.as_str()) {
                    out.push(format!("material {}: unknown pattern {}", m.name, r.pattern));
                }
                if let Some(p) = r.calm.as_ref().and_then(|c| c.pattern.as_ref()) {
                    if !crate::render::recipes::PATTERNS.contains(&p.as_str()) {
                        out.push(format!("material {}: unknown calm pattern {p}", m.name));
                    }
                }
            }
        }
        // By name: a material this world has no id for still exists.
        let mat = |name: &str, what: &str, out: &mut Vec<String>| {
            if self.material(name).is_none() && !self.materials.iter().any(|m| m.name == name) {
                out.push(format!("{what}: unknown material {name}"));
            }
        };
        for (pi, p) in self.plants.iter().enumerate() {
            if p.hosts.iter().any(|h| !["tree", "cliff", "wall", "rock"].contains(&h.as_str())) {
                out.push(format!("plant {}: hosts must be \"tree\", \"cliff\", \"wall\" or \"rock\"", p.name));
            }
            if let Some(c) = &p.crust {
                if c != "bracket" && c != "patch" {
                    out.push(format!("plant {}: crust must be \"bracket\" or \"patch\"", p.name));
                }
            }
            for a in p.affords.iter().filter(|a| !crate::creatures::affordance::AFFORDANCES.contains(&a.as_str())) {
                out.push(format!("plant {}: unknown affordance {a}", p.name));
            }
            mat(&p.bark, &format!("plant {}", p.name), &mut out);
            if let Some(l) = &p.leaf {
                mat(l, &format!("plant {}", p.name), &mut out);
            }
            if let Some((l, _)) = &p.leaf_alt {
                mat(l, &format!("plant {}", p.name), &mut out);
            }
            if let Some(l) = &p.autumn {
                mat(l, &format!("plant {}", p.name), &mut out);
            }
            if let Some(l) = &p.flower {
                mat(l, &format!("plant {}", p.name), &mut out);
            }
            for x in &p.flower_seasons {
                if !["spring", "summer", "autumn", "winter"].contains(&x.as_str()) {
                    out.push(format!("plant {}: flower_seasons: unknown season {x}", p.name));
                }
            }
            if !p.morphs.is_empty() && p.leaf_alt.is_some() {
                out.push(format!("plant {}: leaf_alt and morphs say the same thing; keep the morphs", p.name));
            }
            let pwhat = format!("plant {}", p.name);
            check_morphs(&p.morphs, &PLANT_SLOTS.map(String::from), true, &pwhat, self, &mut out);
            check_coats(&self.plant_coats(pi), &pwhat, &mut out);
            if p.water_depth.is_some() && p.standing_water.is_some() {
                out.push(format!("plant {}: water_depth (kelp) and standing_water don't go together", p.name));
            }
            // Water plants: kelp, the coral forms, and crusts that grow on
            // rock under the water (plate coral on a reef wall).
            let water_form = p.form == Form::Kelp || p.form.is_coral();
            if water_form && p.water_depth.is_none() {
                out.push(format!("plant {}: a {} needs water_depth", p.name, p.form));
            }
            // Canes may grow under water too: sea whips.
            if p.water_depth.is_some() && !water_form && p.form != Form::Crust && p.form != Form::Canes {
                out.push(format!("plant {}: water_depth goes with kelp, coral, crust and canes forms (water plants)", p.name));
            }
            if !(0.0..=1.0).contains(&p.rarity) || (p.rarity > 0.0 && p.like.is_none()) {
                out.push(format!("plant {}: rarity must be 0..1 and needs a parent (like)", p.name));
            }
        }
        for s in &self.signs {
            if let Some(post) = &s.post {
                if self.structure(post).is_none() {
                    out.push(format!("sign {}: unknown structure {post}", s.name));
                }
            }
            if s.board[0] <= 0.0 || s.board[1] <= 0.0 {
                out.push(format!("sign {}: board must have a width and a height", s.name));
            }
        }
        for s in &self.structures {
            for v in s.slots.values() {
                mat(v, &format!("structure {}", s.name), &mut out);
            }
            fn walk(ops: &[Op], f: &mut dyn FnMut(&str)) {
                for op in ops {
                    match op {
                        Op::Box { mat, .. }
                        | Op::Walls { mat, .. }
                        | Op::Gable { mat, .. }
                        | Op::Ellipsoid { mat, .. }
                        | Op::Cylinder { mat, .. }
                        | Op::Log { mat, .. }
                        | Op::Disc { mat, .. }
                        | Op::Dish { mat, .. }
                        | Op::Segment { mat, .. }
                        | Op::Set { mat, .. } => f(mat),
                        Op::RecolourBelow { .. } | Op::Recolour { .. } => {}
                        Op::Repeat { ops, .. } => walk(ops, f),
                    }
                }
            }
            let mut bad = Vec::new();
            // By name, not by id: past 238 materials a world leaves the ones
            // it cannot reach without an id, and a structure it never places
            // is not a misspelling.
            walk(&s.ops, &mut |m| {
                if m != "carve" && !s.slots.contains_key(m) && self.material(m).is_none() && !self.materials.iter().any(|d| d.name == m) {
                    bad.push(m.to_string());
                }
            });
            for b in bad {
                out.push(format!("structure {}: {b} is neither a slot nor a material", s.name));
            }
        }
        for (ci, c) in self.creatures.iter().enumerate() {
            if let Some(h) = &c.behaviour.habit {
                if !crate::creatures::swim::HABITS.contains(&h.as_str()) {
                    out.push(format!("creature {}: unknown habit {h} (one of {:?})", c.name, crate::creatures::swim::HABITS));
                }
            }
            if !crate::creatures::locomotion::MOVEMENTS.contains(&c.behaviour.movement.as_str()) {
                out.push(format!("creature {}: unknown movement {}", c.name, c.behaviour.movement));
            }
            if let Some(a) = &c.behaviour.active {
                if crate::creatures::diel::Period::parse(a).is_none() {
                    out.push(format!("creature {}: unknown active {a}, not one of {:?}", c.name, crate::creatures::diel::PERIODS));
                }
            }
            for (name, v) in [
                ("rain", c.behaviour.rain),
                ("snow", c.behaviour.snow),
                ("heat", c.behaviour.heat),
                ("wind", c.behaviour.wind),
                ("winter", c.behaviour.winter),
            ] {
                if let Some(v) = v {
                    if !(0.0..=1.0).contains(&v) {
                        out.push(format!("creature {}: {name} must be 0 to 1, not {v}", c.name));
                    }
                }
            }
            if let Some(o) = c.behaviour.off_hours {
                if !(0.0..=1.0).contains(&o) {
                    out.push(format!("creature {}: off_hours must be 0 to 1, not {o}", c.name));
                }
            }
            for n in c.behaviour.flees_from.iter().chain(&c.behaviour.hunts) {
                if n != "player" && self.creature(n).is_none() {
                    out.push(format!("creature {}: unknown creature {n}", c.name));
                }
            }
            if let Some(w) = &c.in_water {
                if !crate::creatures::locomotion::MOVEMENTS.contains(&w.movement.as_str()) {
                    out.push(format!("creature {}: in_water: unknown movement {}", c.name, w.movement));
                }
                // The water form is handed to a mover that wants water under
                // it, and nothing else in the library would put it there.
                if !matches!(crate::creatures::locomotion::verb(&w.movement).mover, crate::creatures::locomotion::Mover::School) {
                    out.push(format!("creature {}: in_water movement must be school, not {}", c.name, w.movement));
                }
                if w.movement == c.behaviour.movement {
                    out.push(format!("creature {}: in_water repeats its land movement; leave it out", c.name));
                }
            }
            crate::anim::validate(c, &mut out);
            op_colours(&c.ops, &c.colors, &format!("creature {}", c.name), &mut out);
            if let Some(g) = &c.behaviour.group {
                if g.size[0] == 0 || g.size[0] > g.size[1] {
                    out.push(format!("creature {}: group size must be [low, high] with 1 <= low <= high", c.name));
                }
            }
            if !(0.0..=1.0).contains(&c.rarity) || (c.rarity > 0.0 && c.like.is_none()) {
                out.push(format!("creature {}: rarity must be 0..1 and needs a parent (like)", c.name));
            }
            let slots: Vec<String> = c.colors.keys().cloned().collect();
            let cwhat = format!("creature {}", c.name);
            check_morphs(&c.morphs, &slots, false, &cwhat, self, &mut out);
            let coats = self.creature_coats(ci);
            check_coats(&coats, &cwhat, &mut out);
            check_fit(&coats, &c.colors, &cwhat, &mut out);
        }
        for c in &self.collectibles {
            op_colours(&c.ops, &c.colors, &format!("collectible {}", c.name), &mut out);
        }
        for b in &self.biomes {
            let what = format!("biome {}", b.name);
            mat(&b.surface, &what, &mut out);
            mat(&b.subsurface, &what, &mut out);
            if let Some(r) = &b.rock {
                mat(r, &what, &mut out);
            }
            for p in &b.plants {
                match self.plant(&p.plant) {
                    None => out.push(format!("{what}: unknown plant {}", p.plant)),
                    Some(i) => check_pick(&p.morph, &self.plant_coats(i), &p.plant, &what, &mut out),
                }
            }
            for f in &b.features {
                if self.structure(&f.structure).is_none() {
                    out.push(format!("{what}: unknown structure {}", f.structure));
                }
            }
            for c in &b.creatures {
                match self.creature(&c.creature) {
                    None => out.push(format!("{what}: unknown creature {}", c.creature)),
                    Some(i) => check_pick(&c.morph, &self.creature_coats(i), &c.creature, &what, &mut out),
                }
            }
            for c in &b.collectibles {
                if self.collectible(&c.item).is_none() {
                    out.push(format!("{what}: unknown collectible {}", c.item));
                }
            }
        }
        // City kits: every material and plant they name. Checked by name
        // rather than by id, since only the materials the world being built
        // reaches have ids (`reach`).
        for k in &self.cities {
            for n in k.names() {
                let known = self.materials.iter().any(|m| &m.name == n) || self.plants.iter().any(|p| &p.name == n);
                if !known {
                    out.push(format!("city kit {}: no material or plant called {n}", k.name));
                }
            }
        }
        // Route kits.
        for k in &self.kits {
            if !["grade", "deck", "follow", "none", "bore"].contains(&k.ground.as_str()) {
                out.push(format!("kit {}: ground must be grade, deck, follow, none or bore, not {}", k.name, k.ground));
            }
            if let Some(m) = &k.mat {
                mat(m, &format!("kit {}", k.name), &mut out);
            }
            if let Some(m) = &k.shoulder {
                mat(m, &format!("kit {}", k.name), &mut out);
            }
            for r in &k.rails {
                mat(&r.mat, &format!("kit {} rail", k.name), &mut out);
                if r.to <= r.from {
                    out.push(format!("kit {}: a rail from {} to {} holds no voxels", k.name, r.from, r.to));
                }
            }
            for p in &k.posts {
                mat(&p.mat, &format!("kit {} post", k.name), &mut out);
                if p.size[0] >= p.every {
                    out.push(format!("kit {}: posts {} m long every {} m run together into a wall", k.name, p.size[0], p.every));
                }
            }
            if let Some(d) = &k.deck {
                mat(&d.mat, &format!("kit {} deck", k.name), &mut out);
                if let Some(u) = &d.under {
                    mat(u, &format!("kit {} deck", k.name), &mut out);
                }
            }
            if let Some(w) = &k.walk {
                mat(&w.mat, &format!("kit {} walk", k.name), &mut out);
                if let Some(c) = &w.kerb {
                    mat(c, &format!("kit {} kerb", k.name), &mut out);
                }
            }
            if let Some(p) = &k.piers {
                mat(&p.mat, &format!("kit {} pier", k.name), &mut out);
            }
            if let Some(s) = &k.span {
                if self.kit(s).is_none() {
                    out.push(format!("kit {}: unknown span kit {s}", k.name));
                } else if s == &k.name {
                    out.push(format!("kit {}: names itself as its own span", k.name));
                }
            }
            if k.ground == "deck" && k.deck.is_none() {
                out.push(format!("kit {}: a deck line needs a deck", k.name));
            }
            if k.width <= 0.0 && k.ground == "grade" {
                out.push(format!("kit {}: a graded line with no width paves nothing", k.name));
            }
        }
        // Weapons.
        for w in &self.weapons {
            let what = format!("weapon {}", w.name);
            if !(1..=10).contains(&w.slot) {
                out.push(format!("{what}: slot {} is not 1 to 10", w.slot));
            }
            if !w.view.is_empty() && !crate::combat::viewmodel::SHAPES.contains(&w.view.as_str()) {
                out.push(format!("{what}: view must be one of {:?}", crate::combat::viewmodel::SHAPES));
            }
            if w.ammo_start > w.ammo_max {
                out.push(format!("{what}: ammo_start {} is over ammo_max {}", w.ammo_start, w.ammo_max));
            }
            if let Some(d) = &w.dig {
                if !(0.1..=2.0).contains(&d.radius) {
                    out.push(format!("{what}: a bite of {} m is not 0.1 to 2 metres", d.radius));
                }
                if !(10.0..=1200.0).contains(&d.rpm) {
                    out.push(format!("{what}: {} bites a minute is not 10 to 1200", d.rpm));
                }
                if !(0.5..=8.0).contains(&d.range) {
                    out.push(format!("{what}: a spade that reaches {} m is not 0.5 to 8 metres", d.range));
                }
            }
            for b in &w.grip {
                if b[2] <= 0.0 || b[3] <= 0.0 || b[0] < 0.0 || b[1] < 0.0 || b[0] + b[2] > 1.0 || b[1] + b[3] > 1.0 {
                    out.push(format!("{what}: grip box {b:?} is not inside the 0..1 panel"));
                }
            }
            if w.mag > 0 && w.reload_s <= 0.0 {
                out.push(format!("{what}: a magazine of {} needs a reload_s", w.mag));
            }
            if w.zoom != 0.0 && !(0.05..=0.9).contains(&w.zoom) {
                out.push(format!("{what}: a zoom of {} is not between 0.05 and 0.9", w.zoom));
            }
            // A sight and a second mode share the right button only when the
            // mode charges: holding it scopes and winds up, letting go fires.
            // A mode that fires on the press would loose a round every time
            // somebody reached for the sight.
            if w.zoom > 0.0 && w.alt.as_ref().is_some_and(|a| a.charge_s <= 0.0) {
                out.push(format!("{what}: the right button cannot be both a sight and a mode that fires on the press (give it a charge_s)"));
            }
            if let Some(kind) = &w.ammo_type {
                for o in &self.weapons {
                    if o.name != w.name && o.ammo_type.as_ref() == Some(kind) && o.ammo_max != w.ammo_max {
                        out.push(format!("{what}: shares {kind} with {} but holds {} of it to its {}", o.name, w.ammo_max, o.ammo_max));
                    }
                }
            }
            if w.mag == 0 && w.reload_s > 0.0 {
                out.push(format!("{what}: reload_s means nothing without a mag"));
            }
            for (which, f) in [("fire", Some(&w.fire)), ("alt", w.alt.as_ref())] {
                let Some(f) = f else { continue };
                let what = format!("{what} {which}");
                if f.rpm <= 0.0 {
                    out.push(format!("{what}: rpm must be over 0"));
                }
                if f.pellets == 0 {
                    out.push(format!("{what}: 0 pellets fires nothing"));
                }
                if f.per_shot == 0 {
                    out.push(format!("{what}: per_shot 0 never runs out of ammo"));
                }
                if f.charge_s > 0.0 && f.charge_mult <= 1.0 {
                    out.push(format!("{what}: a charge worth {} is a wait for nothing", f.charge_mult));
                }
                if f.charge_s <= 0.0 && f.charge_mult != 1.0 {
                    out.push(format!("{what}: charge_mult means nothing without a charge_s"));
                }
                if f.head_mult < 1.0 {
                    out.push(format!("{what}: head_mult {} punishes a head shot", f.head_mult));
                }
                if f.bore_r > 0.0 {
                    if f.kind == FireKind::Shell {
                        out.push(format!("{what}: a shell cannot bore -- only a ray cuts as it goes"));
                    }
                    if !(0.1..=3.0).contains(&f.bore_r) {
                        out.push(format!("{what}: bore_r {} is not 0.1 to 3 metres", f.bore_r));
                    }
                }
                if f.twist > 0.0 {
                    if f.kind != FireKind::Beam {
                        out.push(format!("{what}: only a beam has a thread round it to turn"));
                    }
                    if !(0.1..=20.0).contains(&f.twist) {
                        out.push(format!("{what}: a twist of {} turns a second is not 0.1 to 20", f.twist));
                    }
                }
                if let Some(c) = &f.cluster {
                    if f.kind != FireKind::Shell {
                        out.push(format!("{what}: only a shell can break into little ones"));
                    }
                    if !(2..=24).contains(&c.count) {
                        out.push(format!("{what}: a cluster of {} is not 2 to 24", c.count));
                    }
                    if c.speed <= 0.0 {
                        out.push(format!("{what}: the little ones need a speed to be thrown at"));
                    }
                    if c.fuse_s <= 0.0 {
                        out.push(format!("{what}: the little ones need a fuse, or they never go off"));
                    }
                    if c.damage <= 0.0 && c.splash.is_none() {
                        out.push(format!("{what}: the little ones do nothing at all"));
                    }
                    if !c.look.is_empty() && crate::combat::viewmodel::shell_of(&c.look).is_none() {
                        out.push(format!("{what}: no shell called {} ({})", c.look, crate::combat::viewmodel::SHELLS.join(", ")));
                    }
                }
                if f.goo_r > 0.0 {
                    if f.kind != FireKind::Shell {
                        out.push(format!("{what}: only a shell can lay goo -- a ray has nothing to leave behind"));
                    }
                    if !(0.2..=2.0).contains(&f.goo_r) {
                        out.push(format!("{what}: goo_r {} is not 0.2 to 2 metres", f.goo_r));
                    }
                    if f.stick_s <= 0.0 {
                        out.push(format!("{what}: goo_r needs a stick_s: that is the glob's fuse"));
                    }
                    if f.splash.is_none() {
                        out.push(format!("{what}: goo_r needs a splash: that is what the glob does when it goes"));
                    }
                }
                if !(0.0..=1.0).contains(&f.falloff) {
                    out.push(format!("{what}: falloff {} is not 0 to 1", f.falloff));
                }
                if f.arc_jumps > 0 && f.kind != FireKind::Beam {
                    out.push(format!("{what}: only a beam can arc from body to body"));
                }
                if f.arc_jumps > 0 && f.arc_m <= 0.0 {
                    out.push(format!("{what}: an arc needs arc_m, how far it can jump"));
                }
                match f.kind {
                    FireKind::Hitscan => {
                        if f.range <= 0.0 {
                            out.push(format!("{what}: a ray needs a range"));
                        }
                        if f.speed != 0.0 || f.bounces != 0 {
                            out.push(format!("{what}: speed and bounces mean nothing to a ray"));
                        }
                    }
                    FireKind::Shell => {
                        if f.speed <= 0.0 {
                            out.push(format!("{what}: a shell needs a speed"));
                        }
                        if f.pellets > 1 && f.spread_deg <= 0.0 {
                            out.push(format!("{what}: {} shells with no spread all fly the same line", f.pellets));
                        }
                        if f.stick_s > 0.0 && f.splash.is_none() {
                            out.push(format!("{what}: a shell that sticks has to have a burst to go off with"));
                        }
                    }
                    FireKind::Beam => {
                        if f.range <= 0.0 {
                            out.push(format!("{what}: a beam needs a range"));
                        }
                        if f.pellets > 1 || f.spread_deg > 0.0 {
                            out.push(format!("{what}: a beam is one line, so pellets and spread mean nothing"));
                        }
                        if f.splash.is_some() {
                            out.push(format!("{what}: a beam cannot burst -- it would go off every step it was held"));
                        }
                    }
                }
                if f.stick_s > 0.0 && f.kind != FireKind::Shell {
                    out.push(format!("{what}: only a shell can stick to anything"));
                }
                if f.head_mult > 1.0 && f.kind == FireKind::Shell {
                    out.push(format!("{what}: a shell bursts where it lands, so it has no head shot"));
                }
                if !f.look.is_empty() && crate::combat::viewmodel::shell_of(&f.look).is_none() {
                    out.push(format!("{what}: no shell called {} ({})", f.look, crate::combat::viewmodel::SHELLS.join(", ")));
                }
                if let Some(sp) = &f.splash {
                    if sp.radius <= 0.0 {
                        out.push(format!("{what}: a burst needs a radius"));
                    }
                }
            }
        }
        {
            let mut slots: HashMap<u32, &str> = HashMap::new();
            for w in &self.weapons {
                if let Some(other) = slots.insert(w.slot, &w.name) {
                    out.push(format!("weapon {}: slot {} is already {other}'s", w.name, w.slot));
                }
            }
        }
        for (kind, names) in [
            ("kit", self.kits.iter().map(|p| &p.name).collect::<Vec<_>>()),
            ("city kit", self.cities.iter().map(|p| &p.name).collect::<Vec<_>>()),
            ("plant", self.plants.iter().map(|p| &p.name).collect()),
            ("structure", self.structures.iter().map(|p| &p.name).collect()),
            ("creature", self.creatures.iter().map(|p| &p.name).collect()),
            ("biome", self.biomes.iter().map(|p| &p.name).collect()),
            ("material", self.materials.iter().map(|p| &p.name).collect()),
            ("collectible", self.collectibles.iter().map(|p| &p.name).collect()),
            ("weapon", self.weapons.iter().map(|p| &p.name).collect()),
        ] {
            let mut seen = std::collections::HashSet::new();
            for n in names {
                if !seen.insert(n) {
                    out.push(format!("{kind} {n} defined twice"));
                }
            }
        }
        out
    }
}
