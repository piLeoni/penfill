//! Chessboard: squares `cell` mm wide, the dark ones outlined and hatched
//! diagonally, the light ones left as paper. `angle` turns the board.

use crate::anchor::{centre, indices, Anchor, Frame};
use crate::geo::{Pt, Region, Strokes};
use crate::sweep::{sweep_lines, sweep_lines_through};

/// Dark squares hatched `spacing` mm apart; the board turned by `angle`.
pub fn fill_chess(region: &Region, cell: f64, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    if cell <= 0.0 || spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let (u0, u1, v0, v1) = frame.span(region, cell);

    let mut dark: Vec<Vec<Pt>> = Vec::new();
    for j in indices(v0, v1, cell) {
        for i in indices(u0, u1, cell) {
            if (i + j).rem_euclid(2) == 0 {
                let (x, y) = (i as f64 * cell, j as f64 * cell);
                dark.push([(x, y), (x + cell, y), (x + cell, y + cell), (x, y + cell), (x, y)].map(|p| frame.world(p)).to_vec());
            }
        }
    }
    let mut out: Strokes = Vec::new();
    for sq in &dark {
        out.extend(region.clip_polyline(sq));
    }
    let polys: Vec<Vec<Vec<Pt>>> = dark.into_iter().map(|sq| vec![sq]).collect();
    let board = Region::from_polygons(&polys).intersection(region);
    // The hatch belongs to the board: off the local default it runs
    // through the board's lattice point, so neighbours line up.
    out.extend(if anchor == Anchor::LOCAL {
        sweep_lines(&board, spacing, angle + 45.0)
    } else {
        sweep_lines_through(&board, spacing, angle + 45.0, frame.origin)
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_the_squares_are_hatched() {
        let sq = Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)]);
        let board = fill_chess(&sq, 5.0, 1.0, 0.0, Anchor::LOCAL);
        let hatch: Vec<&Vec<Pt>> = board.iter().filter(|s| s.len() == 2 && (s[1].0 - s[0].0).abs() > 1e-6 && (s[1].1 - s[0].1).abs() > 1e-6).collect();
        assert!(!hatch.is_empty());
        // Hatch only ever runs through dark squares (a line may pass from
        // one to the next through their shared corner).
        let origin = 20.0;
        for s in hatch {
            for t in [0.13, 0.37, 0.61, 0.87] {
                let m = (s[0].0 + t * (s[1].0 - s[0].0), s[0].1 + t * (s[1].1 - s[0].1));
                let (i, j) = (((m.0 - origin) / 5.0).floor() as i64, ((m.1 - origin) / 5.0).floor() as i64);
                assert_eq!((i + j).rem_euclid(2), 0, "hatch at {m:?} in a light square");
            }
        }
        for p in board.iter().flatten() {
            assert!(sq.covers_point(*p), "{p:?} outside");
        }
    }
}
