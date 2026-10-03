//! Honeycomb infill: a grid of hexagons clipped to the region.
//!
//! `spacing` is the distance between parallel walls (flat to flat). Each wall
//! is drawn once. `angle` turns the grid inside the shape; the outline stays
//! put. `line_width` opens a gap between neighbouring walls (0 for a pen, so
//! the hexagons touch).

use crate::anchor::{centre, indices, Anchor, Frame};
use crate::geo::{Pt, Region, Strokes};
use std::collections::HashSet;

/// Hexagon walls over `region`.
pub fn fill_honeycomb(region: &Region, spacing: f64, angle: f64, line_width: f64, anchor: Anchor) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (bx0, by0, bx1, by1) = region.bounds_mm();
    if bx1 - bx0 < spacing * 0.5 && by1 - by0 < spacing * 0.5 {
        return vec![];
    }
    let gap = line_width.max(0.0);
    if gap >= spacing {
        return vec![];
    }
    // Flat-to-flat of the cell is `spacing`. The drawn hex is smaller by
    // `gap`, so a pen (`gap == 0`) shares corners with its neighbours.
    let pitch = spacing / 3.0_f64.sqrt();
    let radius = (spacing - gap) / 3.0_f64.sqrt();
    let frame = Frame::anchored(anchor, centre(region), angle);
    let (minx, maxx, miny, maxy) = frame.span(region, spacing);

    let col_step = 1.5 * pitch;
    let row_step = spacing;
    let mut seen: HashSet<((i64, i64), (i64, i64))> = HashSet::new();
    let mut edges: Vec<(Pt, Pt)> = Vec::new();
    for col in indices(minx, maxx, col_step) {
        for row in indices(miny, maxy, row_step) {
            let cx = col as f64 * col_step;
            let cy = row as f64 * row_step + if col % 2 != 0 { row_step * 0.5 } else { 0.0 };
            let verts = hex_vertices((cx, cy), radius);
            for i in 0..6 {
                let a = verts[i];
                let b = verts[(i + 1) % 6];
                let ka = quantize(a);
                let kb = quantize(b);
                let key = if ka <= kb { (ka, kb) } else { (kb, ka) };
                if seen.insert(key) {
                    edges.push((a, b));
                }
            }
        }
    }

    let mut out: Strokes = Vec::new();
    for (a, b) in edges {
        out.extend(region.clip_segment(frame.world(a), frame.world(b)));
    }
    out
}

fn hex_vertices(c: Pt, radius: f64) -> [Pt; 6] {
    std::array::from_fn(|i| {
        let a = std::f64::consts::FRAC_PI_3 * i as f64;
        (c.0 + radius * a.cos(), c.1 + radius * a.sin())
    })
}

fn quantize(p: Pt) -> (i64, i64) {
    ((p.0 * 1_000.0).round() as i64, (p.1 * 1_000.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::Region;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)])
    }

    #[test]
    fn honeycomb_covers_square() {
        let hc = fill_honeycomb(&square(), 2.0, 0.0, 0.0, Anchor::LOCAL);
        assert!(hc.len() > 4, "expected several hex edges, got {}", hc.len());
        let total: f64 = hc.iter().map(|p| crate::geo::polyline_length(p)).sum();
        assert!(total > 50.0, "total length {total}");
    }

    #[test]
    fn walls_meet_as_hexagons() {
        let hc = fill_honeycomb(&square(), 8.0, 0.0, 0.0, Anchor::LOCAL);
        let side = 8.0 / 3.0_f64.sqrt();
        let mut full = 0;
        for stroke in &hc {
            let len = crate::geo::polyline_length(stroke);
            if (len - side).abs() < 0.05 {
                full += 1;
                let dy = (stroke[0].1 - stroke[1].1).abs();
                let dx = (stroke[0].0 - stroke[1].0).abs();
                // Flat-top: horizontal edges, or 60° edges.
                let horizontal = dy < 0.05 && dx > 1.0;
                let angled = dx > 1.0 && dy > 1.0;
                assert!(horizontal || angled, "edge {dx},{dy} is not a hex side");
            }
        }
        assert!(full > 6, "expected full hex sides, got {full}");
    }

    #[test]
    fn angle_turns_the_hexes_inside_the_square() {
        let hc = fill_honeycomb(&square(), 6.0, 45.0, 0.0, Anchor::LOCAL);
        assert!(!hc.is_empty());
        let mut diagonal = false;
        for stroke in &hc {
            for p in stroke {
                assert!(
                    (-1e-3..=30.001).contains(&p.0) && (-1e-3..=30.001).contains(&p.1),
                    "rotated fill left the square at {p:?}"
                );
            }
            if stroke.len() >= 2 {
                let dx = (stroke[1].0 - stroke[0].0).abs();
                let dy = (stroke[1].1 - stroke[0].1).abs();
                if dx > 0.3 && dy > 0.3 {
                    diagonal = true;
                }
            }
        }
        assert!(diagonal, "45° should tilt the hex sides");
    }

    #[test]
    fn honeycomb_respects_holes() {
        let outer = vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)];
        let hole = vec![(12.0, 12.0), (18.0, 12.0), (18.0, 18.0), (12.0, 18.0)];
        let ring = Region::from_polygons(&[vec![outer, hole]]);
        let hc = fill_honeycomb(&ring, 2.0, 0.0, 0.0, Anchor::LOCAL);
        for path in &hc {
            for p in path {
                assert!(
                    !(p.0 > 12.3 && p.0 < 17.7 && p.1 > 12.3 && p.1 < 17.7),
                    "point {:?} in hole",
                    p
                );
            }
        }
    }

    #[test]
    fn tiny_region_empty() {
        let dot = Region::from_ring(&[(0.0, 0.0), (0.05, 0.0), (0.05, 0.05), (0.0, 0.05)]);
        assert!(fill_honeycomb(&dot, 2.0, 0.0, 0.0, Anchor::LOCAL).is_empty());
    }
}
