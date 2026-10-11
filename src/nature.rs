//! Nature-plan vector textures: pebbles, sparse sprouts, lily pads, tree
//! canopies — the kind of fills used on site plans and illustration libraries.

use crate::concentric::signed_area;
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
