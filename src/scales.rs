//! Scales: rows of half circles, each row shifted by half an arc so every
//! arc rests on the tips of the row below — fish scales, roof tiles, the
//! scallop hatch used for shingle roofs and water in drawings.
//!
//! A row is one continuous scalloped line, so the pen lifts once per row.

use crate::anchor::{centre, indices, Anchor, Frame};
use crate::geo::{Pt, Region, Strokes};

const ARC_STEPS: usize = 12;

/// Rows `spacing` mm apart; each arc is `2 × spacing` wide. `angle` turns
/// the rows (0 = rows run left to right, arcs hanging down).
pub fn fill_scales(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let r = spacing;
    let frame = Frame::anchored(anchor, centre(region), angle);
    let (u0, u1, v0, v1) = frame.span(region, 2.0 * r);

    let mut out: Strokes = Vec::new();
    for j in indices(v0, v1, r) {
        let y = j as f64 * r;
        let shift = if j.rem_euclid(2) == 1 { r } else { 0.0 };
        let mut row: Vec<Pt> = Vec::new();
        for i in indices(u0, u1, 2.0 * r) {
            let cx = 2.0 * r * i as f64 + shift;
            let skip = usize::from(!row.is_empty());
            for k in skip..=ARC_STEPS {
                let a = std::f64::consts::PI * (1.0 + k as f64 / ARC_STEPS as f64);
                row.push(frame.world((cx + r * a.cos(), y + r * a.sin())));
            }
        }
        out.extend(region.clip_polyline(&row));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_long_scalloped_lines_inside() {
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let s = fill_scales(&sq, 2.0, 0.0, Anchor::LOCAL);
        let arc = std::f64::consts::PI * 2.0;
        let long = s.iter().filter(|p| crate::geo::polyline_length(p) > 5.0 * arc).count();
        assert!(long >= 12, "only {long} full-width rows");
        for p in s.iter().flatten() {
            assert!(sq.covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn arcs_rest_on_the_tips_below() {
        // The bottom of an arc in one row is a tip of the row below.
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let s = fill_scales(&sq, 2.0, 0.0, Anchor::LOCAL);
        let pts: Vec<Pt> = s.iter().flatten().copied().collect();
        let bottoms: Vec<Pt> = pts
            .windows(3)
            .filter(|w| w[1].1 < w[0].1 && w[1].1 <= w[2].1 && (w[0].0 - w[2].0).abs() < 2.0)
            .map(|w| w[1])
            .filter(|p| p.0 > 4.0 && p.0 < 26.0 && p.1 > 4.0 && p.1 < 26.0)
            .collect();
        assert!(bottoms.len() > 20);
        for b in bottoms {
            // Once as the bottom itself, once as the tip in the row below.
            let hits = pts.iter().filter(|q| (q.0 - b.0).hypot(q.1 - b.1) < 1e-6).count();
            assert!(hits >= 2, "arc bottom {b:?} floats");
        }
    }
}
