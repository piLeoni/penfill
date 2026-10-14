//! Nature-plan vector textures: pebbles, sparse sprouts, lily pads, tree
//! canopies — the kind of fills used on site plans and illustration libraries.

use crate::anchor::{centre, Anchor, Frame};
use crate::concentric::signed_area;
use crate::fray::{fray_strokes, whole_at};
use crate::geo::{polyline_length, Pt, Region, Strokes};
use crate::noise::Perlin;
use crate::random::{poisson_disk, Rng};

const CIRCLE_STEPS: usize = 24;

/// Smooth circle outline (one closed polyline).
fn circle(c: Pt, r: f64) -> Vec<Pt> {
    let mut pts = Vec::with_capacity(CIRCLE_STEPS + 1);
    for k in 0..=CIRCLE_STEPS {
        let a = std::f64::consts::TAU * k as f64 / CIRCLE_STEPS as f64;
        pts.push((c.0 + r * a.cos(), c.1 + r * a.sin()));
    }
    pts
}

/// Tiny filled marker at `c` (closed ring).
fn dot(c: Pt, r: f64) -> Vec<Pt> {
    circle(c, r)
}

/// Irregular lobe outline — squiggly top-down foliage disk.
fn wobbly_lobe(c: Pt, r: f64, phase: f64, rough: f64) -> Vec<Pt> {
    let steps = 28;
    let mut pts = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = std::f64::consts::TAU * k as f64 / steps as f64;
        let w = (7.0 * t + phase).sin() * 0.55
            + (11.0 * t - phase * 1.3).sin() * 0.35
            + (17.0 * t + phase * 0.7).sin() * 0.22;
        let rr = r * (1.0 + rough * w);
        pts.push((c.0 + rr * t.cos(), c.1 + rr * t.sin()));
    }
    pts
}

fn uf_find(parent: &mut [usize], i: usize) -> usize {
    let mut i = i;
    while parent[i] != i {
        let p = parent[i];
        parent[i] = parent[p];
        i = p;
    }
    i
}

fn clip_ring(region: &Region, ring: &[Pt]) -> Strokes {
    region.clip_polyline(ring)
}

fn push_closed(region: &Region, out: &mut Strokes, ring: &[Pt]) {
    if ring.len() < 3 {
        return;
    }
    let mut closed = ring.to_vec();
    if closed.first() != closed.last() {
        closed.push(closed[0]);
    }
    out.extend(clip_ring(region, &closed));
}

/// Small rounded stones: circle outlines on a Poisson grid (`spacing` ≈ centre
/// distance). Radius varies per stone — lighter than [`gravel`](crate::gravel).
pub fn fill_pebbles(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(seed);
    let field = region.outset(spacing);
    let mut out: Strokes = Vec::new();
    for c in poisson_disk(&field, spacing, seed) {
        let r = spacing * rng.range(0.22, 0.42);
        out.extend(clip_ring(region, &circle(c, r)));
    }
    out
}

/// Seamless tile of short blades and V-shaped sprouts (`spacing` = tile pitch).
/// Same role as a light site-plan grass hatch — not the dense [`grass`](crate::grass) tufts.
pub fn fill_leaves(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fill_sprouts(region, spacing, seed)
}

/// Sparse sprout hatch: offset rows of single strokes and V marks, repeating tile.
pub fn fill_sprouts(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(seed.wrapping_add(5));
    let pitch = spacing;
    let row = pitch * 0.78;
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let pad = pitch;
    let mut out: Strokes = Vec::new();
    let j0 = ((miny - pad) / row).floor() as i64;
    let j1 = ((maxy + pad) / row).ceil() as i64;
    let i0 = ((minx - pad) / pitch).floor() as i64;
    let i1 = ((maxx + pad) / pitch).ceil() as i64;
    for j in j0..=j1 {
        let shift = if j.rem_euclid(2) == 1 { pitch * 0.5 } else { 0.0 };
        for i in i0..=i1 {
            let base = (
                i as f64 * pitch + shift + rng.range(-0.06, 0.06) * pitch,
                j as f64 * row + rng.range(-0.06, 0.06) * row,
            );
            if !region.covers_point(base) {
                continue;
            }
            let h = pitch * rng.range(0.42, 0.62);
            let tilt = rng.range(-0.12, 0.12);
            let tag = (seed ^ (i as u64).wrapping_mul(0x9E37) ^ (j as u64).wrapping_mul(0x85EB)) % 5;
            match tag {
                0 | 2 | 4 => {
                    let mid = (base.0 + tilt * h * 0.35, base.1 + h * 0.52);
                    let tip = (base.0 + tilt * h * 0.55, base.1 + h);
                    out.extend(region.clip_polyline(&[base, mid, tip]));
                }
                _ => {
                    let spread = pitch * rng.range(0.16, 0.24);
                    let l = (base.0 - spread, base.1 + h);
                    let r = (base.0 + spread, base.1 + h);
                    let ml = (base.0 - spread * 0.55 + tilt * h * 0.2, base.1 + h * 0.55);
                    let mr = (base.0 + spread * 0.55 + tilt * h * 0.2, base.1 + h * 0.55);
                    out.extend(region.clip_polyline(&[base, ml, l]));
                    out.extend(region.clip_polyline(&[base, mr, r]));
                }
            }
        }
    }
    out
}

fn flow_stroke(region: &Region, out: &mut Strokes, c: Pt, angle: f64, half: f64, min_len: f64) {
    let (ca, sa) = angle.sin_cos();
    let pts = [(c.0 - ca * half, c.1 - sa * half), (c.0 + ca * half, c.1 + sa * half)];
    out.extend(
        region
            .clip_polyline(&pts)
            .into_iter()
            .filter(|s| polyline_length(s) >= min_len),
    );
}

/// Grass as short strokes aligned to a smooth vector field (flow lines).
pub fn fill_fluid_grass(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let noise = Perlin::new(seed);
    let mut rng = Rng::new(seed.wrapping_add(41));
    let field = region.outset(spacing * 0.4);
    let scale = spacing * 3.2;
    let min_len = spacing * 0.1;
    let mut out: Strokes = Vec::new();
    for c in poisson_disk(&field, spacing * 0.72, seed) {
        if !region.covers_point(c) {
            continue;
        }
        let u = c.0 / scale;
        let v = c.1 / scale;
        let angle = noise.fbm(u, v, 4) * std::f64::consts::TAU;
        let half = spacing * rng.range(0.22, 0.38);
        flow_stroke(region, &mut out, c, angle, half, min_len);
        if rng.unit() < 0.55 {
            let jitter = (rng.range(-0.18, 0.18) * spacing, rng.range(-0.12, 0.12) * spacing);
            let c2 = (c.0 + jitter.0, c.1 + jitter.1);
            let u2 = c2.0 / scale;
            let v2 = c2.1 / scale;
            let a2 = noise.fbm(u2 + 1.7, v2 - 0.9, 3) * std::f64::consts::TAU;
            flow_stroke(region, &mut out, c2, a2, half * rng.range(0.75, 0.95), min_len);
        }
    }
    out
}

/// Lily pads / bubbles: equal circles with centre dots; overlapping pads are
/// merged into one outline (no internal seams).
pub fn fill_lily_pads(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let field = region.outset(spacing);
    let centres: Vec<Pt> = poisson_disk(&field, spacing * 0.92, seed)
        .into_iter()
        .filter(|c| region.covers_point(*c))
        .collect();
    if centres.is_empty() {
        return vec![];
    }
    let r = spacing * 0.46;
    let merge = 2.0 * r * 0.88;
    let merge2 = merge * merge;
    let n = centres.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in i + 1..n {
            let dx = centres[i].0 - centres[j].0;
            let dy = centres[i].1 - centres[j].1;
            if dx * dx + dy * dy <= merge2 {
                let (a, b) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
                if a != b {
                    parent[b] = a;
                }
            }
        }
    }
    let mut groups: std::collections::HashMap<usize, Vec<Pt>> = std::collections::HashMap::new();
    for (i, c) in centres.iter().enumerate() {
        groups.entry(uf_find(&mut parent, i)).or_default().push(*c);
    }
    let dot_r = spacing * 0.045;
    let mut out: Strokes = Vec::new();
    for group in groups.values() {
        let polys: Vec<Vec<Vec<Pt>>> = group.iter().map(|c| vec![circle(*c, r)]).collect();
        let merged = Region::from_polygons(&polys);
        for ring in merged.all_rings_mm() {
            if ring.len() < 3 || signed_area(&ring).abs() < 1e-6 {
                continue;
            }
            push_closed(region, &mut out, &ring);
        }
        for c in group {
            push_closed(region, &mut out, &dot(*c, dot_r));
        }
    }
    out
}

/// Top-down tree canopies: clustered wobbly lobes with a centre dot per lobe.
pub fn fill_canopy(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(seed.wrapping_add(23));
    let field = region.outset(spacing);
    let mut out: Strokes = Vec::new();
    let dot_r = spacing * 0.04;
    for anchor in poisson_disk(&field, spacing * 1.05, seed) {
        if !region.covers_point(anchor) {
            continue;
        }
        let n_lobes = 1 + (rng.next_u64() % 4) as usize;
        let phase = rng.range(0.0, std::f64::consts::TAU);
        let rough = rng.range(0.10, 0.18);
        let mut lobes: Vec<(Pt, f64)> = Vec::new();
        for k in 0..n_lobes {
            let a = phase + std::f64::consts::TAU * k as f64 / n_lobes as f64 + rng.range(-0.25, 0.25);
            let dist = if n_lobes == 1 { 0.0 } else { spacing * rng.range(0.22, 0.34) };
            let c = (anchor.0 + dist * a.cos(), anchor.1 + dist * a.sin());
            if !region.covers_point(c) {
                continue;
            }
            let rad = spacing * rng.range(0.30, 0.44);
            lobes.push((c, rad));
        }
        if lobes.is_empty() {
            continue;
        }
        let polys: Vec<Vec<Vec<Pt>>> = lobes
            .iter()
            .map(|(c, rad)| vec![wobbly_lobe(*c, *rad, phase + c.0 * 0.1, rough)])
            .collect();
        let merged = Region::from_polygons(&polys);
        for ring in merged.all_rings_mm() {
            if ring.len() < 3 {
                continue;
            }
            push_closed(region, &mut out, &ring);
        }
        for (c, _) in &lobes {
            push_closed(region, &mut out, &dot(*c, dot_r));
        }
    }
    out
}

/// Clustered columns of short curved grass/sedge blades and 2-blade V-tufts
/// whose lean varies smoothly across patches via Perlin noise.
pub fn fill_tufts(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    fill_tufts_frayed(region, spacing, angle, seed, 0.0)
}

/// [`fill_tufts`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_tufts_frayed(region: &Region, spacing: f64, angle: f64, seed: u64, fray: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(Anchor::LOCAL, centre(region), angle);
    let lean_noise = Perlin::new(seed);
    let lane_noise = Perlin::new(seed.wrapping_add(13));
    let fray_noise = Perlin::new(seed.wrapping_add(503));
    let mut rng = Rng::new(seed.wrapping_add(37));

    let col_w = spacing * 0.68;
    let row_h = spacing * 0.72;
    let pad = spacing * 1.5;
    let (u_min, u_max, v_min, v_max) = frame.span(region, pad);
    let i0 = (u_min / col_w).floor() as i64;
    let i1 = (u_max / col_w).ceil() as i64;
    let j0 = (v_min / row_h).floor() as i64;
    let j1 = (v_max / row_h).ceil() as i64;

    let mut out: Strokes = Vec::new();
    let blade = |base: Pt, len: f64, dir: f64, bend: f64, frame: &Frame| -> Vec<Pt> {
        let (s1, c1) = dir.sin_cos();
        let (s2, c2) = (dir + bend).sin_cos();
        let p0 = frame.world(base);
        let p1 = frame.world((base.0 + 0.52 * len * s1, base.1 + 0.52 * len * c1));
        let p2 = frame.world((
            base.0 + 0.52 * len * s1 + 0.48 * len * s2,
            base.1 + 0.52 * len * c1 + 0.48 * len * c2,
        ));
        vec![p0, p1, p2]
    };

    for i in i0..=i1 {
        let col_u = i as f64 * col_w;
        for j in j0..=j1 {
            let stagger = if i.rem_euclid(2) == 1 { row_h * 0.48 } else { 0.0 };
            let v0 = j as f64 * row_h + stagger;
            let col_wave = lane_noise.noise(col_u / (spacing * 2.8), v0 / (spacing * 6.5));
            let u = col_u + rng.range(-0.28, 0.28) * col_w + col_wave * spacing * 0.16;
            let v = v0 + rng.range(-0.24, 0.24) * row_h;

            // Vertical band modulation so columns group into 3–4 stroke wide ribbons
            // separated by subtle open vertical lanes, like Image 2.
            let band_phase = std::f64::consts::TAU * u / (spacing * 3.8)
                + 0.7 * lane_noise.noise(u / (spacing * 4.0), v / (spacing * 5.0));
            if band_phase.cos() < -0.42 {
                continue;
            }
            let p_world = frame.world((u, v));
            if !region.covers_point(p_world) {
                continue;
            }
            let scale = if fray > 0.0 {
                let w = whole_at(region, p_world, spacing, fray, &fray_noise);
                if w < rng.range(0.08, 0.42) {
                    continue;
                }
                0.28 + 0.72 * w
            } else {
                1.0
            };

            let lean = lean_noise.fbm(u / (spacing * 3.4), v / (spacing * 3.4), 2) * 0.72
                + rng.range(-0.14, 0.14);
            let h = spacing * rng.range(0.46, 0.74) * scale;
            let bend = rng.range(0.14, 0.34) * if rng.unit() < 0.5 { 1.0 } else { -1.0 };

            if rng.unit() < 0.32 {
                // Two-blade V-tuft
                let splay = rng.range(0.24, 0.42);
                let b1 = blade((u, v), h * rng.range(0.85, 1.05), lean - splay, -bend.abs() * 0.7, &frame);
                let b2 = blade(
                    (u + rng.range(0.0, 0.06) * spacing, v),
                    h * rng.range(0.80, 1.05),
                    lean + splay,
                    bend.abs() * 0.85,
                    &frame,
                );
                out.extend(region.clip_polyline(&b1));
                out.extend(region.clip_polyline(&b2));
            } else {
                let b = blade((u, v), h, lean, bend, &frame);
                out.extend(region.clip_polyline(&b));
            }
        }
    }
    out
}

/// Scattered branched twigs: Poisson-sampled stick motifs with a 3–4 segment
/// slightly kinked spine and 2–4 straight side shoots at acute angles.
pub fn fill_twigs(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fill_twigs_frayed(region, spacing, seed, 0.0)
}

/// [`fill_twigs`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_twigs_frayed(region: &Region, spacing: f64, seed: u64, fray: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let field = region.outset(spacing * 0.6);
    let fray_noise = Perlin::new(seed.wrapping_add(607));
    let mut rng = Rng::new(seed.wrapping_add(73));
    let mut raw: Strokes = Vec::new();

    for c in poisson_disk(&field, spacing * 1.18, seed) {
        if !region.covers_point(c) {
            continue;
        }
        let w = if fray > 0.0 {
            let w = whole_at(region, c, spacing, fray, &fray_noise);
            if w < rng.range(0.10, 0.38) {
                continue;
            }
            0.35 + 0.65 * w
        } else {
            1.0
        };

        let n_segs = 3 + (rng.next_u64() % 2) as usize;
        let seg_len = spacing * rng.range(0.38, 0.54) * w;
        let mut dir = rng.range(0.0, std::f64::consts::TAU);

        // Build kinked spine centred near `c`.
        let (ca0, sa0) = dir.cos_sin();
        let half_span = 0.5 * n_segs as f64 * seg_len;
        let mut spine = Vec::with_capacity(n_segs + 1);
        let mut cur = (c.0 - ca0 * half_span, c.1 - sa0 * half_span);
        spine.push(cur);

        let mut seg_dirs = Vec::with_capacity(n_segs);
        for s in 0..n_segs {
            if s > 0 {
                dir += rng.range(-0.34, 0.34);
            }
            seg_dirs.push(dir);
            let sl = seg_len * rng.range(0.82, 1.18);
            let (ca, sa) = dir.cos_sin();
            cur = (cur.0 + ca * sl, cur.1 + sa * sl);
            spine.push(cur);
        }
        raw.push(spine.clone());

        // Sprout straight or 2-segment side shoots from internal joints.
        let mut side_sign = if rng.unit() < 0.5 { 1.0 } else { -1.0 };
        for j in 1..n_segs {
            let joint = spine[j];
            let base_dir = 0.5 * (seg_dirs[j - 1] + seg_dirs[j]);
            let both_sides = rng.unit() < 0.38;
            let count = if both_sides { 2 } else { 1 };
            for b in 0..count {
                let sign = if b == 0 { side_sign } else { -side_sign };
                let branch_ang = base_dir + sign * rng.range(0.52, 0.92);
                let blen = spacing * rng.range(0.30, 0.58) * w;
                let (bca, bsa) = branch_ang.cos_sin();
                let p1 = (joint.0 + bca * blen, joint.1 + bsa * blen);
                if rng.unit() < 0.28 {
                    let ang2 = branch_ang + sign * rng.range(0.20, 0.42);
                    let blen2 = blen * rng.range(0.42, 0.65);
                    let (ca2, sa2) = ang2.cos_sin();
                    let p2 = (p1.0 + ca2 * blen2, p1.1 + sa2 * blen2);
                    raw.push(vec![joint, p1, p2]);
                } else {
                    raw.push(vec![joint, p1]);
                }
            }
            side_sign = -side_sign;
        }
    }

    if fray > 0.0 {
        fray_strokes(region, raw, spacing, fray, seed)
    } else {
        region.clip_strokes(&raw)
    }
}

trait CosSin {
    fn cos_sin(self) -> (f64, f64);
}
impl CosSin for f64 {
    #[inline]
    fn cos_sin(self) -> (f64, f64) {
        let (s, c) = self.sin_cos();
        (c, s)
    }
}

/// Parallel meandering stems running along `angle` (`spacing` mm apart) with
/// short straight side-shoots sprouting at acute angles on alternating sides.
pub fn fill_vines(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    fill_vines_frayed(region, spacing, angle, seed, anchor, 0.0)
}

/// [`fill_vines`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_vines_frayed(
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: Anchor,
    fray: f64,
) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let noise = Perlin::new(seed);
    let fray_noise = Perlin::new(seed.wrapping_add(709));

    let pad = spacing * 2.0;
    let (u_min, u_max, v_min, v_max) = frame.span(region, pad);
    let k0 = (v_min / spacing).floor() as i64;
    let k1 = (v_max / spacing).ceil() as i64;
    let step = (spacing * 0.18).clamp(0.10, 0.4);

    let mut stem_strokes: Strokes = Vec::new();
    let mut barb_strokes: Strokes = Vec::new();

    for k in k0..=k1 {
        let v0 = k as f64 * spacing;
        let mut vrng = Rng::new(seed ^ (k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let phase = vrng.range(0.0, std::f64::consts::TAU);
        let wave_len = spacing * 2.55;
        let amp = spacing * 0.16;

        let stem_v = |u: f64| -> f64 {
            let w1 = (std::f64::consts::TAU * u / wave_len + phase).sin();
            let w2 = noise.fbm(u / (spacing * 2.4), k as f64 * 0.67 + 1.9, 2);
            v0 + amp * (0.65 * w1 + 0.85 * w2)
        };

        let mut stem = Vec::new();
        let mut u = u_min;
        while u <= u_max {
            stem.push(frame.world((u, stem_v(u))));
            u += step;
        }
        stem_strokes.push(stem);

        // Sprout alternating side barbs along the stem.
        let barb_pitch = spacing * 0.54;
        let mut bu = u_min + vrng.range(0.1, 0.9) * barb_pitch;
        let mut side = if k.rem_euclid(2) == 0 { 1.0 } else { -1.0 };
        while bu <= u_max - step {
            let jitter_u = bu + vrng.range(-0.10, 0.10) * barb_pitch;
            let v_here = stem_v(jitter_u);
            let p_base = frame.world((jitter_u, v_here));

            let barb_scale = if fray > 0.0 {
                let w = whole_at(region, p_base, spacing, fray, &fray_noise);
                if w < vrng.range(0.22, 0.48) {
                    side = -side;
                    bu += barb_pitch * vrng.range(0.85, 1.20);
                    continue;
                }
                0.35 + 0.65 * w
            } else {
                1.0
            };

            // Local tangent of stem in (u, v)
            let dv = stem_v(jitter_u + step * 0.5) - stem_v(jitter_u - step * 0.5);
            let stem_ang = dv.atan2(step);
            let barb_ang = stem_ang + side * vrng.range(0.58, 0.78);
            let barb_len = spacing * vrng.range(0.38, 0.50) * barb_scale;
            let p_tip = frame.world((
                jitter_u + barb_len * barb_ang.cos(),
                v_here + barb_len * barb_ang.sin(),
            ));
            barb_strokes.push(vec![p_base, p_tip]);

            side = -side;
            bu += barb_pitch * vrng.range(0.86, 1.16);
        }
    }

    let mut out = if fray > 0.0 {
        fray_strokes(region, stem_strokes, spacing, fray, seed)
    } else {
        region.clip_strokes(&stem_strokes)
    };
    out.extend(region.clip_strokes(&barb_strokes));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)])
    }

    fn inside(s: &Strokes) {
        for p in s.iter().flatten() {
            assert!(square().covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn pebbles_scatter_circles() {
        let s = fill_pebbles(&square(), 3.0, 1);
        assert!(s.len() >= 25, "only {} pebbles", s.len());
        inside(&s);
    }

    #[test]
    fn leaves_sparse_sprouts() {
        let s = fill_leaves(&square(), 4.0, 2);
        assert!(s.len() >= 12, "only {} sprout strokes", s.len());
        inside(&s);
    }

    #[test]
    fn fluid_grass_curved_blades() {
        let s = fill_fluid_grass(&square(), 3.5, 9);
        assert!(s.len() >= 25, "only {} grass strokes", s.len());
        inside(&s);
    }

    #[test]
    fn lily_pads_merge_and_dots() {
        let s = fill_lily_pads(&square(), 5.0, 3);
        assert!(s.len() >= 15, "only {} pad strokes", s.len());
        inside(&s);
    }

    #[test]
    fn canopy_lobes() {
        let s = fill_canopy(&square(), 8.0, 4);
        assert!(s.len() >= 10, "only {} canopy strokes", s.len());
        inside(&s);
    }
}
