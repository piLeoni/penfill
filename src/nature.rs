//! Nature-plan vector textures: tree canopies and twigs — built on [`Canvas`] and [`Tools`],
//! supporting [`EdgeMode`] (`Clip`, `Inside`, `Overflow(mm)`), perimeter
//! `fray`, and polygon `mask` export ([`FillOutput`]).

use crate::geo::{Pt, Region, Strokes};
use crate::pattern::{fill_pattern_output, Canvas, EdgeMode, FillOutput, FillParams, Tools};
use crate::random::{poisson_disk, Rng};

fn push_closed(out: &mut Strokes, ring: &[Pt]) {
    if ring.len() < 3 {
        return;
    }
    let mut closed = ring.to_vec();
    if closed.first() != closed.last() {
        closed.push(closed[0]);
    }
    out.push(closed);
}

// ---------------------------------------------------------------------------
// Tree Canopy
// ---------------------------------------------------------------------------

/// Top-down tree canopies: clustered wobbly lobes with a centre dot per lobe.
pub fn fill_canopy(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fill_canopy_params(region, &FillParams::new(spacing).seed(seed)).strokes
}

pub fn fill_canopy_params(region: &Region, params: &FillParams) -> FillOutput {
    if params.spacing <= 0.0 || region.is_empty() {
        return FillOutput::default();
    }
    fill_pattern_output(region, params, draw_canopy)
}

pub(crate) fn draw_canopy(canvas: &Canvas, tools: &Tools) -> Strokes {
    let spacing = canvas.spacing;
    let mut rng = tools.rng_salted(23);
    let field = canvas.region.outset(spacing);
    let sites = poisson_disk(&field, spacing * 1.05, canvas.seed).into_iter().map(|p| (p, Rng::new(rng.next_u64()))).collect();
    canopy_at(canvas, tools, spacing, sites)
}

/// Canopies of trees about `size` mm across, one at each site with its own
/// randomness (sites outside the region are skipped).
pub(crate) fn canopy_at(canvas: &Canvas, tools: &Tools, size: f64, sites: Vec<(Pt, Rng)>) -> Strokes {
    let spacing = size;
    let mut out: Strokes = Vec::new();
    let dot_r = spacing * 0.04;
    for (anchor, mut rng) in sites {
        if !canvas.region.covers_point(anchor) {
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
            if !canvas.region.covers_point(c) {
                continue;
            }
            let rad = spacing * rng.range(0.30, 0.44);
            if !tools.element_fits(c, rad * (1.0 + rough)) {
                continue;
            }
            lobes.push((c, rad));
        }
        if lobes.is_empty() {
            continue;
        }
        let polys: Vec<Vec<Vec<Pt>>> = lobes
            .iter()
            .map(|(c, rad)| vec![tools.lobe(*c, *rad, phase + c.0 * 0.1, rough)])
            .collect();
        let merged = Region::from_polygons(&polys);
        tools.add_mask(&merged);
        for ring in merged.all_rings_mm() {
            if ring.len() < 3 {
                continue;
            }
            push_closed(&mut out, &ring);
        }
        for (c, _) in &lobes {
            push_closed(&mut out, &tools.circle(*c, dot_r));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Twigs
// ---------------------------------------------------------------------------

/// Scattered branched twigs: Poisson-sampled stick motifs with a 3–4 segment
/// slightly kinked spine and 2–4 straight side shoots at acute angles.
pub fn fill_twigs(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fill_twigs_params(region, &FillParams::new(spacing).seed(seed)).strokes
}

/// [`fill_twigs`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_twigs_frayed(region: &Region, spacing: f64, seed: u64, fray: f64) -> Strokes {
    fill_twigs_params(region, &FillParams::new(spacing).seed(seed).fray(fray)).strokes
}

pub fn fill_twigs_params(region: &Region, params: &FillParams) -> FillOutput {
    if params.spacing <= 0.0 || region.is_empty() {
        return FillOutput::default();
    }
    fill_pattern_output(region, params, draw_twigs)
}

pub(crate) fn draw_twigs(canvas: &Canvas, tools: &Tools) -> Strokes {
    let spacing = canvas.spacing;
    let field = canvas.region.outset(spacing * 0.6);
    let mut rng = tools.rng_salted(73);
    let sites = poisson_disk(&field, spacing * 1.18, canvas.seed).into_iter().map(|p| (p, Rng::new(rng.next_u64()))).collect();
    twigs_at(canvas, tools, spacing, sites)
}

/// A twig of about `size` mm at each site, with the site's own randomness
/// (sites outside the region are skipped). Frays twig by twig.
pub(crate) fn twigs_at(canvas: &Canvas, tools: &Tools, size: f64, sites: Vec<(Pt, Rng)>) -> Strokes {
    tools.claim_fray();
    let spacing = size;
    let mut raw: Strokes = Vec::new();

    for (c, mut rng) in sites {
        if !canvas.region.covers_point(c) {
            continue;
        }
        let w = if canvas.fray > 0.0 {
            let w = tools.whole_at(c);
            if w < rng.range(0.10, 0.38) {
                continue;
            }
            0.35 + 0.65 * w
        } else {
            1.0
        };

        let n_segs = 3 + (rng.next_u64() % 2) as usize;
        let seg_len = spacing * rng.range(0.38, 0.54) * w;
        let half_span = 0.5 * n_segs as f64 * seg_len;
        if matches!(canvas.edge, EdgeMode::Inside) && canvas.region.dist_to_boundary(c) < half_span * 1.25 {
            continue;
        }
        let mut dir = rng.range(0.0, std::f64::consts::TAU);

        let (ca0, sa0) = dir.cos_sin();
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
    raw
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
    fn canopy_lobes_and_overflow_mask() {
        let sq = square();
        let s = fill_canopy(&sq, 8.0, 4);
        assert!(s.len() >= 10, "only {} canopy strokes", s.len());
        inside(&s);

        let over = fill_canopy_params(&sq, &FillParams::new(8.0).seed(4).edge(EdgeMode::Overflow(4.0)));
        assert!(!over.mask.is_empty());
    }
}
