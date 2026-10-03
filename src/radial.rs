//! Sunburst: lines radiating from a centre, like pizza slices or an
//! engraved sun. Lines start at staggered distances, so the spacing
//! between neighbours stays about `spacing` everywhere instead of piling
//! up into a black blot at the centre.

use crate::anchor::{centre, Anchor};
use crate::geo::{Region, Strokes};

/// Rays about `spacing` mm apart from the anchor: locally the middle of
/// the region's bounding box, globally a page point. `angle` turns the
/// first ray.
pub fn fill_sunburst(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let c = anchor.resolve(centre(region));
    let reach = [(minx, miny), (maxx, miny), (maxx, maxy), (minx, maxy)]
        .iter()
        .map(|p| (p.0 - c.0).hypot(p.1 - c.1))
        .fold(0.0, f64::max);
    let tau = std::f64::consts::TAU;
    let wanted = (tau * reach / spacing).ceil().max(4.0) as usize;
    let n = wanted.next_power_of_two();
    let bits = n.trailing_zeros();
    let mut out: Strokes = Vec::new();
    for k in 0..n {
        // Bit-reversed order refines evenly: the first 4 rays are 90° apart,
        // the first 8 are 45° apart, and so on. Ray of rank r only starts
        // where the circle is long enough for r rays `spacing` apart.
        let rank = k.reverse_bits() >> (usize::BITS - bits);
        let start = rank as f64 * spacing / tau;
        if start >= reach {
            continue;
        }
        let a = angle.to_radians() + tau * k as f64 / n as f64;
        let d = (a.cos(), a.sin());
        let p = (c.0 + d.0 * start, c.1 + d.1 * start);
        let q = (c.0 + d.0 * reach, c.1 + d.1 * reach);
        out.extend(region.clip_segment(p, q));
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
            assert!(gap > 0.5 && gap <= 1.0 + 1e-9, "at r={r}: gap {gap}");
        }
    }
}
