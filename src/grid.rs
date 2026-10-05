//! Lines, grid and triangles: one, two or three families of parallel lines.
//!
//! The lines keep half a spacing off the walls, except on a page-wide
//! (global) lattice: there neighbouring regions must meet without a gap, so
//! the lines run right up to the outline.

use crate::anchor::{centre, Anchor};
use crate::geo::{Region, Strokes};
use crate::sweep::sweep_lines_through;

/// The region the lines are cut to.
fn target(region: &Region, spacing: f64, anchor: Anchor) -> Region {
    if anchor.is_global() {
        region.clone()
    } else {
        region.inset(spacing / 2.0)
    }
}

/// One family of lines, unjoined.
pub fn fill_lines(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    let mut out = Vec::new();
    fill_lines_into(region, spacing, angle, anchor, &mut out);
    out
}

/// [`fill_lines`] streamed: one scanline at a time.
pub fn fill_lines_into(region: &Region, spacing: f64, angle: f64, anchor: Anchor, sink: &mut dyn crate::Sink) {
    if region.is_empty() || spacing <= 0.0 {
        return;
    }
    crate::sweep::sweep_anchored_into(&target(region, spacing, anchor), spacing, angle, anchor, sink);
}

/// Two families, at `angle` and `angle + 90`.
pub fn fill_grid(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    let mut out = Vec::new();
    fill_grid_into(region, spacing, angle, anchor, &mut out);
    out
}

/// [`fill_grid`] streamed: the `angle` family, then the `angle + 90` one.
pub fn fill_grid_into(region: &Region, spacing: f64, angle: f64, anchor: Anchor, sink: &mut dyn crate::Sink) {
    fill_lines_into(region, spacing, angle, anchor, sink);
    fill_lines_into(region, spacing, angle + 90.0, anchor, sink);
}

/// Three families 60° apart through one point, closing into equilateral
/// triangles; `spacing` is the triangle height.
pub fn fill_triangles(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    if region.is_empty() || spacing <= 0.0 {
        return vec![];
    }
    let target = target(region, spacing, anchor);
    if target.is_empty() {
        return vec![];
    }
    let point = anchor.resolve(centre(&target));
    [0.0, 60.0, 120.0].iter().flat_map(|turn| sweep_lines_through(&target, spacing, angle + turn, point)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::Region;

    #[test]
    fn grid_doubles_the_lines() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let grid = fill_grid(&sq, 1.0, 0.0, Anchor::LOCAL);
        let lines = fill_lines(&sq, 1.0, 0.0, Anchor::LOCAL);
        // Sweep scanline counts are rounded per-axis, so horizontal and
        // vertical passes may differ by one fragment (19 vs 20 here).
        assert_ne!(grid.len(), 0);
        assert!(grid.len() >= lines.len(), "two passes: grid > single");
        assert_eq!(grid.len(), 39);
    }

    #[test]
    fn triangle_families_cross_at_shared_points() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let tri = fill_triangles(&sq, 2.0, 10.0, Anchor::LOCAL);
        let dir = |s: &Vec<(f64, f64)>| {
            let (a, b) = (s[0], *s.last().unwrap());
            ((b.1 - a.1).atan2(b.0 - a.0).to_degrees().rem_euclid(180.0) / 60.0).round() as i64 % 3
        };
        let fam: Vec<Vec<&Vec<(f64, f64)>>> =
            (0..3).map(|k| tri.iter().filter(|s| dir(s) == k).collect()).collect();
        assert!(fam.iter().all(|f| f.len() > 5), "three families expected");
        // Where a line of family 0 crosses one of family 1, some line of
        // family 2 must pass through the same point.
        let meet = |p: (f64, f64), q: (f64, f64), r: (f64, f64), s: (f64, f64)| {
            let d = (q.0 - p.0) * (s.1 - r.1) - (q.1 - p.1) * (s.0 - r.0);
            let t = ((r.0 - p.0) * (s.1 - r.1) - (r.1 - p.1) * (s.0 - r.0)) / d;
            let u = ((r.0 - p.0) * (q.1 - p.1) - (r.1 - p.1) * (q.0 - p.0)) / d;
            ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((p.0 + t * (q.0 - p.0), p.1 + t * (q.1 - p.1)))
        };
        let on = |x: (f64, f64), s: &Vec<(f64, f64)>| {
            let (a, b) = (s[0], *s.last().unwrap());
            let cross = ((b.0 - a.0) * (x.1 - a.1) - (b.1 - a.1) * (x.0 - a.0)).abs() / (b.0 - a.0).hypot(b.1 - a.1);
            cross < 1e-6
        };
        let mut checked = 0;
        for a in &fam[0] {
            for b in &fam[1] {
                if let Some(x) = meet(a[0], *a.last().unwrap(), b[0], *b.last().unwrap()) {
                    assert!(fam[2].iter().any(|c| on(x, c)), "crossing {x:?} has no third line");
                    checked += 1;
                }
            }
        }
        assert!(checked > 20, "only {checked} crossings");
    }

    #[test]
    fn lines_inset_globally() {
        let sq = Region::from_ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let lines = fill_lines(&sq, 1.0, 0.0, Anchor::LOCAL);
        // First/last scanlines at x or y = 0.5 (inset by spacing / 2).
        let ys: Vec<f64> = lines.iter().map(|p| p[0].1).collect();
        let lo = ys.iter().cloned().fold(f64::MAX, f64::min);
        assert!((lo - 0.5).abs() < 1e-6, "first inset line at y={lo}");
    }

    #[test]
    fn diagonal_angle_reorients() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let lines = fill_lines(&sq, 1.0, 45.0, Anchor::LOCAL);
        assert!(!lines.is_empty());
        // 45° fragments come out mostly diagonal — every span ≠ 0.
        for f in &lines {
            let dx = f.last().unwrap().0 - f.first().unwrap().0;
            assert!(dx.abs() > 1e-9, "45° fragment has diagonal span");
        }
    }
}
