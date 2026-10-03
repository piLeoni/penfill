//! Your own hatch: any drawing repeated like wallpaper and cut by the wall.
//!
//! The tile is a set of polylines in mm, laid out in a `width × height`
//! box whose lower-left corner is the origin. Copies are placed on a grid,
//! each row shifted by `row_shift × width` (0.5 = brick bond), the whole
//! grid turned by `angle`. Lines that cross the tile box are fine — they
//! simply overlap the neighbour, as in a CAD hatch.

use crate::anchor::{centre, indices, Anchor, Frame};
use crate::geo::{Pt, Region, Strokes};

pub fn fill_tile(
    region: &Region,
    tile: &Strokes,
    width: f64,
    height: f64,
    angle: f64,
    row_shift: f64,
    anchor: Anchor,
) -> Strokes {
    if width <= 0.0 || height <= 0.0 || tile.is_empty() || region.is_empty() {
        return vec![];
    }
    // How far tile lines reach outside their box.
    let spill = tile
        .iter()
        .flatten()
        .map(|p| (-p.0).max(p.0 - width).max(-p.1).max(p.1 - height).max(0.0))
        .fold(0.0, f64::max);
    let frame = Frame::anchored(anchor, centre(region), angle);
    let (u0, u1, v0, v1) = frame.span(region, width.hypot(height) + spill);

    let mut out: Strokes = Vec::new();
    for j in indices(v0, v1, height) {
        let shift = (j as f64 * row_shift).rem_euclid(1.0) * width;
        for i in indices(u0, u1, width) {
            let o = (i as f64 * width + shift - width / 2.0, j as f64 * height - height / 2.0);
            for line in tile {
                let pts: Vec<Pt> = line.iter().map(|p| frame.world((o.0 + p.0, o.1 + p.1))).collect();
                out.extend(region.clip_polyline(&pts));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cross_tile_repeats_and_is_cut() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let cross: Strokes = vec![vec![(1.0, 2.0), (3.0, 2.0)], vec![(2.0, 1.0), (2.0, 3.0)]];
        let out = fill_tile(&sq, &cross, 4.0, 4.0, 0.0, 0.0, Anchor::LOCAL);
        // 5 × 5 tiles fit, two strokes each (plus cut ones at the wall).
        assert!(out.len() >= 50, "only {} strokes", out.len());
        for p in out.iter().flatten() {
            assert!(sq.covers_point(*p), "{p:?} outside");
        }
    }
}
