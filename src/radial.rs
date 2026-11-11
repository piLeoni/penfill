//! Sunburst: lines radiating from a centre, like an engraved sun. Rays
//! come in generations so the gap between neighbours stays near `spacing`
//! at every radius: four rays run to the centre, and each later generation
//! doubles the count, its rays starting together on one circle — where the
//! gap between the rays already there has opened to √2 spacings — so they
//! split it into two of 1/√2. The gap therefore swings between 0.71 and
//! 1.41 spacings instead of crowding into a blot at the centre, and every
//! ray is one clean line from its start circle out.

use crate::anchor::{centre, Anchor};
use crate::geo::{Pt, Region, Strokes};

/// Rays about `spacing` mm apart from the anchor: locally the middle of
/// the region's bounding box, globally a page point. `angle` turns the
/// first ray.
pub fn fill_sunburst(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    fill_sunburst_from(region, spacing, angle, anchor.resolve(centre(region)))
}

/// Rays about `spacing` mm apart radiating from `c` (page mm), cut to the
/// region.
pub fn fill_sunburst_from(region: &Region, spacing: f64, angle: f64, c: Pt) -> Strokes {
    if spacing.is_nan() || spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let reach = [(minx, miny), (maxx, miny), (maxx, maxy), (minx, maxy)]
        .iter()
        .map(|p| (p.0 - c.0).hypot(p.1 - c.1))
        .fold(0.0, f64::max);
    let tau = std::f64::consts::TAU;
    let mut out: Strokes = Vec::new();
    // Generation g holds 2^g rays in all; its new ones (the odd multiples
    // of 2π / 2^g) start where 2^(g-1) rays stand √2·spacing apart.
    let mut count = 4usize;
    loop {
        let start = if count == 4 { 0.0 } else { std::f64::consts::SQRT_2 * spacing * (count / 2) as f64 / tau };
        if start >= reach || count > 1 << 22 {
            break;
        }
        let step = if count == 4 { 1 } else { 2 };
        let first = if count == 4 { 0 } else { 1 };
        for k in (first..count).step_by(step) {
            let a = angle.to_radians() + tau * k as f64 / count as f64;
            let d = (a.cos(), a.sin());
            let p = (c.0 + d.0 * start, c.1 + d.1 * start);
            let q = (c.0 + d.0 * reach, c.1 + d.1 * reach);
            out.extend(region.clip_segment(p, q));
        }
        count *= 2;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_keep_their_spacing_at_every_radius() {
        let sq = Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)]);
        let rays = fill_sunburst(&sq, 1.0, 0.0, Anchor::LOCAL);
        for r in [3.0, 8.0, 15.0] {
            // Count rays crossing the circle of radius r around the centre.
            let crossing = rays
                .iter()
                .filter(|s| {
                    let d0 = (s[0].0 - 20.0).hypot(s[0].1 - 20.0);
                    let d1 = (s[1].0 - 20.0).hypot(s[1].1 - 20.0);
                    d0.min(d1) <= r && d0.max(d1) >= r
                })
                .count() as f64;
            let gap = std::f64::consts::TAU * r / crossing;
            assert!(gap > 0.7 && gap <= 1.42, "at r={r}: gap {gap}");
        }
    }
}
