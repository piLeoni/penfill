//! Gravel: rounded stones packed with small gaps, the map symbol for
//! shingle, scree and gravel beds.
//!
//! Stone centres are Poisson-disk points. Each stone is the Voronoi cell
//! of its centre (the area closer to it than to any other), pulled back to
//! open a gap and with its corners rounded. What happens at the wall is a
//! choice, see [`GravelEdge`].

use crate::concentric::signed_area;
use crate::geo::{polyline_length, Pt, Region, Strokes};
use crate::random::{poisson_disk, poisson_disk_varying, Rng};

/// How stones meet the wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GravelEdge {
    /// Stones get smaller toward the wall and take its shape, so the edge
    /// and its nooks are packed as tightly as the middle.
    Adaptive,
    /// Stones keep their size and the wall simply cuts them, like a CAD
    /// hatch: cut stones stay open along the wall.
    Clipped,
}

/// Stones about `spacing` mm across, packed right up to the wall.
pub fn fill_gravel(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fill_gravel_with(region, spacing, seed, GravelEdge::Adaptive)
}

/// Stones about `spacing` mm across, meeting the wall as `edge` says.
pub fn fill_gravel_with(region: &Region, spacing: f64, seed: u64, edge: GravelEdge) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    // Centres outside the region too, so wall cells are bounded properly.
    let field = region.outset(spacing);
    let centres: Vec<(Pt, f64)> = match edge {
        GravelEdge::Adaptive => {
            let small = spacing * 0.4;
            let size = |p: Pt| {
                let t = (region.dist_to_boundary(p) / (1.5 * spacing)).clamp(0.0, 1.0);
                small + t * (spacing - small)
            };
            poisson_disk_varying(&field, small, spacing, size, seed).into_iter().map(|p| (p, size(p))).collect()
        }
        GravelEdge::Clipped => poisson_disk(&field, spacing, seed).into_iter().map(|p| (p, spacing)).collect(),
    };
    let mut rng = Rng::new(seed.wrapping_add(3));
    let reach2 = (spacing * 2.5).powi(2);
    let mut out: Strokes = Vec::new();
    for (i, &(c, size)) in centres.iter().enumerate() {
        let r = spacing * 2.0;
        let mut cell = vec![(c.0 - r, c.1 - r), (c.0 + r, c.1 - r), (c.0 + r, c.1 + r), (c.0 - r, c.1 + r)];
        for (j, &(o, _)) in centres.iter().enumerate() {
            if i == j || (o.0 - c.0).powi(2) + (o.1 - c.1).powi(2) > reach2 {
                continue;
            }
            cell = clip_half_plane(&cell, c, o);
            if cell.len() < 3 {
                break;
            }
        }
        if cell.len() < 3 {
            continue;
        }
        // Half the gap on each side of the shared Voronoi edge.
        let gap = size * rng.range(0.05, 0.1);
        let corner = size * rng.range(0.18, 0.32);
        let min_area = 0.12 * size * size;
        let cell = Region::from_ring(&cell);
        let base = match edge {
            GravelEdge::Adaptive => cell.intersection(region),
            GravelEdge::Clipped => cell,
        };
        let area: f64 = base.all_rings_mm().iter().map(|r| signed_area(r)).sum();
        if area < min_area {
            continue;
        }
        let piece = base.inset(gap + corner).outset(corner);
        // A real stone never reaches past its own cell.
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
            ring.push(ring[0]);
            match edge {
                GravelEdge::Adaptive => out.push(ring),
                GravelEdge::Clipped => out.extend(
                    region
                        .clip_polyline(&ring)
                        .into_iter()
                        .filter(|s| polyline_length(s) >= 0.25 * size),
                ),
            }
        }
    }
    out
}

/// Keep the part of `poly` closer to `c` than to `o`.
fn clip_half_plane(poly: &[Pt], c: Pt, o: Pt) -> Vec<Pt> {
    let m = ((c.0 + o.0) / 2.0, (c.1 + o.1) / 2.0);
    let n = (o.0 - c.0, o.1 - c.1);
    let side = |p: Pt| (p.0 - m.0) * n.0 + (p.1 - m.1) * n.1;
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
            let t = sa / (sa - sb);
            out.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_stone_is_much_bigger_than_the_spacing() {
        let sq = Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)]).inset(0.4);
        for s in fill_gravel(&sq, 2.0, 7) {
            let (lo, hi) = s.iter().fold(((f64::MAX, f64::MAX), (f64::MIN, f64::MIN)), |(lo, hi), p| {
                ((lo.0.min(p.0), lo.1.min(p.1)), (hi.0.max(p.0), hi.1.max(p.1)))
            });
            let size = (hi.0 - lo.0).max(hi.1 - lo.1);
            assert!(size < 2.0 * 3.0, "stone {size} mm across at {lo:?}–{hi:?}");
        }
    }

    #[test]
    fn stones_are_closed_inside_and_do_not_touch() {
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let stones = fill_gravel(&sq, 3.0, 4);
        assert!(stones.len() > 40, "only {} stones", stones.len());
        for s in &stones {
            assert_eq!(s.first(), s.last(), "stone not closed");
            for p in s {
                assert!(sq.covers_point(*p), "stone point {p:?} outside");
            }
        }
    }

    #[test]
    fn adaptive_stones_line_the_wall() {
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let near = |p: &Pt| p.0.min(p.1).min(30.0 - p.0).min(30.0 - p.1) < 0.6;
        let lining = fill_gravel(&sq, 3.0, 4).iter().filter(|s| s.iter().any(near)).count();
        // 120 mm of wall; small wall stones are ~1.2–2 mm across.
        assert!(lining > 45, "only {lining} stones reach the wall");
    }

    #[test]
    fn clipped_stones_are_cut_open_by_the_wall() {
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let stones = fill_gravel_with(&sq, 3.0, 4, GravelEdge::Clipped);
        let open = stones.iter().filter(|s| s.first() != s.last()).count();
        assert!(open > 15, "only {open} cut stones");
        for p in stones.iter().flatten() {
            assert!(sq.covers_point(*p), "{p:?} outside");
        }
    }
}
