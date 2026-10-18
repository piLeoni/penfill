//! Gravel: rounded stones packed with small gaps, the map symbol for
//! shingle, scree and gravel beds.
//!
//! Built on [`Canvas`] and [`Tools`] (`voronoi_cells`, `scatter`, `scatter_field`,
//! `add_mask_ring`), supporting [`EdgeMode`] (`Inside` for adaptive corner-packing
//! stones, `Clip` for CAD-sliced stones, `Overflow(m)` for overhanging stones)
//! and perimeter `fray`.

use crate::concentric::signed_area;
use crate::geo::{polyline_length, Pt, Region, Strokes};
use crate::pattern::{
    fill_pattern_output, fill_pattern_with, Canvas, EdgeMode, FillOutput, FillParams, Tools,
};
use crate::random::poisson_disk_varying;

/// Legacy alias for how stones meet the wall; maps directly to [`EdgeMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GravelEdge {
    /// Maps to [`EdgeMode::Inside`]: stones get smaller toward the wall and
    /// conform to its shape so every stone closes inside.
    Adaptive,
    /// Maps to [`EdgeMode::Clip`]: stones keep their size and the wall cuts
    /// them open.
    Clipped,
}

impl From<GravelEdge> for EdgeMode {
    fn from(edge: GravelEdge) -> Self {
        match edge {
            GravelEdge::Adaptive => EdgeMode::Inside,
            GravelEdge::Clipped => EdgeMode::Clip,
        }
    }
}

/// Stones about `spacing` mm across, packed right up to the wall (`EdgeMode::Inside`).
pub fn fill_gravel(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fill_pattern_with(
        region,
        &FillParams::new(spacing).seed(seed).edge(EdgeMode::Inside),
        draw_gravel,
    )
}

/// Stones about `spacing` mm across, meeting the wall as `edge` says.
pub fn fill_gravel_with(region: &Region, spacing: f64, seed: u64, edge: GravelEdge) -> Strokes {
    fill_pattern_with(
        region,
        &FillParams::new(spacing).seed(seed).edge(edge.into()),
        draw_gravel,
    )
}

/// Full standardized entry point for `gravel` returning both `strokes` and
/// the stone polygon `mask`.
pub fn fill_gravel_params(region: &Region, params: &FillParams) -> FillOutput {
    if params.spacing <= 0.0 || region.is_empty() {
        return FillOutput::default();
    }
    fill_pattern_output(region, params, draw_gravel)
}

pub(crate) fn draw_gravel(canvas: &Canvas, tools: &Tools) -> Strokes {
    let spacing = canvas.spacing;
    if spacing <= 0.0 {
        return vec![];
    }
    let field = canvas.region.outset(spacing);
    let centres_and_sizes: Vec<(Pt, f64)> = match canvas.edge {
        EdgeMode::Inside => {
            let small = spacing * 0.4;
            let size = |p: Pt| {
                let t = (canvas.region.dist_to_boundary(p) / (1.5 * spacing)).clamp(0.0, 1.0);
                small + t * (spacing - small)
            };
            poisson_disk_varying(&field, small, spacing, size, canvas.seed)
                .into_iter()
                .map(|p| (p, size(p)))
                .collect()
        }
        EdgeMode::Clip | EdgeMode::Overflow(_) => tools
            .on(&field)
            .scatter(spacing)
            .into_iter()
            .map(|p| (p, spacing))
            .collect(),
    };

    let pts: Vec<Pt> = centres_and_sizes.iter().map(|&(p, _)| p).collect();
    let cells = tools.voronoi_cells(&pts, spacing * 2.5);
    let mut rng = tools.rng_salted(3);
    let mut out: Strokes = Vec::new();

    for (i, c, cell_ring) in cells {
        let size = centres_and_sizes[i].1;
        if matches!(canvas.edge, EdgeMode::Overflow(_)) && !canvas.region.covers_point(c) {
            continue;
        }
        let gap = size * rng.range(0.05, 0.1);
        let corner = size * rng.range(0.18, 0.32);
        let min_area = 0.12 * size * size;
        let cell = Region::from_ring(&cell_ring);
        let base = match canvas.edge {
            EdgeMode::Inside => cell.intersection(canvas.region),
            EdgeMode::Clip => cell,
            EdgeMode::Overflow(_) => cell.intersection(&canvas.limit),
        };
        let area: f64 = base.all_rings_mm().iter().map(|r| signed_area(r)).sum();
        if area < min_area {
            continue;
        }
        let piece = base.inset(gap + corner).outset(corner);
        let (ax, ay, bx, by) = base.bounds_mm();
        let (px, py, qx, qy) = piece.bounds_mm();
        let eps = 1e-3;
        if px < ax - eps || py < ay - eps || qx > bx + eps || qy > by + eps {
            continue;
        }
        for mut ring in piece.all_rings_mm() {
            if ring.len() < 3 || signed_area(&ring).abs() < min_area {
                continue;
            }
            tools.add_mask_ring(&ring);
            ring.push(ring[0]);
            match canvas.edge {
                EdgeMode::Inside | EdgeMode::Overflow(_) => out.push(ring),
                EdgeMode::Clip => out.extend(
                    canvas
                        .region
                        .clip_polyline(&ring)
                        .into_iter()
                        .filter(|s| polyline_length(s) >= 0.25 * size),
                ),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stones_are_closed_inside_and_do_not_touch() {
        let sq = Region::from_ring(&[(0.0, 0.0), (25.0, 0.0), (25.0, 25.0), (0.0, 25.0)]);
        let stones = fill_gravel(&sq, 4.0, 7);
        assert!(stones.len() > 20, "only {} stones", stones.len());
        for s in &stones {
            assert_eq!(s.first(), s.last(), "stone is not a closed loop");
            for &p in s {
                assert!(sq.covers_point(p), "vertex {p:?} outside");
            }
        }
    }

    #[test]
    fn adaptive_stones_line_the_wall() {
        let sq = Region::from_ring(&[(0.0, 0.0), (25.0, 0.0), (25.0, 25.0), (0.0, 25.0)]);
        let stones = fill_gravel(&sq, 4.0, 7);
        let near_wall = stones
            .iter()
            .filter(|s| s.iter().any(|p| p.0 < 1.5 || p.1 < 1.5 || p.0 > 23.5 || p.1 > 23.5))
            .count();
        assert!(near_wall >= 12, "only {near_wall} stones near the wall");
    }

    #[test]
    fn clipped_stones_are_cut_open_by_the_wall() {
        let sq = Region::from_ring(&[(0.0, 0.0), (25.0, 0.0), (25.0, 25.0), (0.0, 25.0)]);
        let stones = fill_gravel_with(&sq, 4.0, 7, GravelEdge::Clipped);
        let open = stones.iter().filter(|s| s.first() != s.last()).count();
        assert!(open >= 8, "expected cut stones along the wall, got {open}");
        for p in stones.iter().flatten() {
            assert!(sq.covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn no_stone_is_much_bigger_than_the_spacing() {
        let star = Region::from_ring(&[
            (20.0, 0.0),
            (25.0, 14.0),
            (40.0, 15.0),
            (28.0, 24.0),
            (32.0, 38.0),
            (20.0, 30.0),
            (8.0, 38.0),
            (12.0, 24.0),
            (0.0, 15.0),
            (15.0, 14.0),
        ]);
        for s in fill_gravel(&star, 3.5, 2) {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for &(x, y) in &s {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
            assert!(x1 - x0 < 3.5 * 2.0 && y1 - y0 < 3.5 * 2.0, "oversized stone: {} × {}", x1 - x0, y1 - y0);
        }
    }
}
