//! Truchet tiles: a grid of cells, each holding two quarter-circle arcs in
//! one of two orientations picked at random. The arcs meet at the middle
//! of every cell edge, so they chain into long winding paths — few pen
//! lifts, which suits a plotter.

use crate::anchor::{cell_seed, centre, indices, Anchor, Frame};
use crate::geo::{Pt, Region, Strokes};
use crate::random::Rng;
use std::collections::HashMap;

const ARC_STEPS: usize = 8;

/// Cells `cell` mm wide, grid turned by `angle`.
pub fn fill_truchet(region: &Region, cell: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    if cell <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let (u0, u1, v0, v1) = frame.span(region, cell);
    let seed = seed.wrapping_add(5);

    let mut arcs: Vec<Vec<Pt>> = Vec::new();
    for i in indices(u0, u1, cell) {
        for j in indices(v0, v1, cell) {
            let (x0, y0) = (i as f64 * cell, j as f64 * cell);
            // Each cell's orientation depends on the cell alone, so the
            // same lattice cell turns the same way in every region.
            let corners = if Rng::new(cell_seed(seed, i, j)).coin() {
                [((x0, y0), 0.0), ((x0 + cell, y0 + cell), 180.0)]
            } else {
                [((x0 + cell, y0), 90.0), ((x0, y0 + cell), 270.0)]
            };
            for (centre, start) in corners {
                let pts: Vec<Pt> = (0..=ARC_STEPS)
                    .map(|k| {
                        let a = (start + 90.0 * k as f64 / ARC_STEPS as f64).to_radians();
                        frame.world((centre.0 + cell / 2.0 * a.cos(), centre.1 + cell / 2.0 * a.sin()))
                    })
                    .collect();
                arcs.push(pts);
            }
        }
    }

    chain(arcs)
        .iter()
        .flat_map(|path| region.clip_polyline(path))
        .collect()
}

/// Join arcs end to end into the longest paths their shared ends allow.
fn chain(arcs: Vec<Vec<Pt>>) -> Strokes {
    let key = |p: Pt| ((p.0 * 1e4).round() as i64, (p.1 * 1e4).round() as i64);
    let mut ends: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, a) in arcs.iter().enumerate() {
        ends.entry(key(a[0])).or_default().push(i);
        ends.entry(key(*a.last().unwrap())).or_default().push(i);
    }
    let mut used = vec![false; arcs.len()];
    let mut out: Strokes = Vec::new();
    for start in 0..arcs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut path = arcs[start].clone();
        // Grow forward, then backward.
        for _ in 0..2 {
            loop {
                let tail = *path.last().unwrap();
                let next = ends
                    .get(&key(tail))
                    .and_then(|v| v.iter().copied().find(|&k| !used[k]));
                let Some(k) = next else { break };
                used[k] = true;
                let mut arc = arcs[k].clone();
                if key(arc[0]) != key(tail) {
                    arc.reverse();
                }
                path.extend_from_slice(&arc[1..]);
            }
            path.reverse();
        }
        out.push(path);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcs_chain_into_long_paths_inside() {
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let t = fill_truchet(&sq, 3.0, 0.0, 2, Anchor::LOCAL);
        assert!(!t.is_empty());
        let arcs_len = std::f64::consts::PI / 4.0 * 3.0;
        let avg: f64 = t.iter().map(|s| crate::geo::polyline_length(s)).sum::<f64>() / t.len() as f64;
        assert!(avg > arcs_len * 2.5, "paths average {avg} mm, barely longer than one arc");
        for s in &t {
            for p in s {
                assert!(sq.covers_point(*p), "{p:?} outside");
            }
        }
    }
}
