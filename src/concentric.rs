//! Concentric rings: the outer wall, inset again and again, with a
//! centreline where a part is too thin for a ring.
//!
//! Each piece of the region is handled on its own. Its outer contour, holes
//! filled in, is inset by the border and then by one spacing per round;
//! every contour of every round is drawn as a closed ring, unless it is too
//! thin for a loop (then its centreline is drawn instead) or it has run into
//! a hole (then it is skipped). Holes therefore stop the rings rather than
//! growing rings of their own.
//!
//! Insetting a sharp corner pulls it in faster than the sides, so the next
//! ring can leave a spike tip far behind. Each round looks for the parts of
//! its band that the next ring will not reach and draws their centrelines.
//!
//! A centreline is found by cutting the thin piece across its long axis at
//! close intervals and chaining the midpoints of the cuts.

use crate::geo::{Pt, Region, Strokes};
use crate::sweep::sweep_rows;

/// More rounds than any real drawing needs.
const MAX_ROUNDS: usize = 10_000;

/// Closed rings (outside in), then centreline strokes, one piece of the
/// region after another.
pub fn fill_concentric(region: &Region, spacing: f64, inset_mm: Option<f64>) -> Strokes {
    if region.is_empty() || spacing <= 0.0 {
        return vec![];
    }
    let inset = inset_mm.unwrap_or(spacing / 2.0);
    let mut rings = Vec::new();
    let mut lines = Vec::new();
    for piece in region.parts() {
        let (r, l) = piece_rings(&piece, spacing, inset);
        rings.extend(r);
        lines.extend(l);
    }
    rings.extend(lines);
    rings
}

/// The rings and the centrelines of one piece.
fn piece_rings(piece: &Region, spacing: f64, inset: f64) -> (Strokes, Strokes) {
    let mut rings = Vec::new();
    let mut lines = Vec::new();
    let mut current = solid_outers(piece).inset(inset);
    for _ in 0..MAX_ROUNDS {
        if current.is_empty() {
            break;
        }
        let mut drawn: Vec<Vec<Pt>> = Vec::new();
        let mut drew = false;
        for contour in outer_contours(&current) {
            let shape = Region::from_ring(&contour);
            if shape.inset(0.25 * spacing).is_empty() {
                let centre = centreline(&shape, piece, spacing);
                drew |= !centre.is_empty();
                lines.extend(centre);
            } else if contour_inside(piece, &contour) {
                let mut ring = contour.clone();
                ring.push(contour[0]);
                rings.push(ring);
                drawn.push(contour);
                drew = true;
            }
        }
        if !drew {
            break;
        }
        if !drawn.is_empty() {
            lines.extend(missed(&drawn, piece, spacing));
        }
        current = current.inset(spacing);
    }
    (rings, lines)
}

/// Centrelines for what the ring after `drawn` will not reach: the band
/// inside these rings that lies well beyond the next ring.
fn missed(drawn: &[Vec<Pt>], piece: &Region, spacing: f64) -> Strokes {
    let polygons: Vec<Vec<Vec<Pt>>> = drawn.iter().map(|r| vec![r.clone()]).collect();
    let band = Region::from_polygons(&polygons).inset(spacing / 2.0);
    if band.is_empty() {
        return vec![];
    }
    let next = band.inset(spacing / 2.0);
    let leftover = band.difference(&next.outset(0.75 * spacing));
    outer_contours(&leftover)
        .iter()
        .flat_map(|c| centreline(&Region::from_ring(c), piece, spacing))
        .collect()
}

/// Every outer contour of the region (positive area, at least 3 points).
fn outer_contours(region: &Region) -> Vec<Vec<Pt>> {
    region.all_rings_mm().into_iter().filter(|r| r.len() >= 3 && signed_area(r) > 0.0).collect()
}

/// The middle line of a thin `shape`, kept inside `original`.
///
/// The shape is cut across its long axis (the principal axis of its outline
/// points) every quarter spacing. Cuts that fall in two or more pieces are
/// forks and are dropped; the midpoints of the others are chained in order
/// along the axis, each to the nearest chain end within 0.6 spacings.
fn centreline(shape: &Region, original: &Region, spacing: f64) -> Strokes {
    let points: Vec<Pt> = shape.all_rings_mm().concat();
    if points.len() < 3 {
        return vec![];
    }
    let n = points.len() as f64;
    let (mx, my) = points.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in &points {
        let (dx, dy) = (p.0 - mx, p.1 - my);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    let axis = 0.5 * (2.0 * sxy).atan2(sxx - syy);

    let mut chains: Vec<(Vec<Pt>, f64)> = Vec::new();
    for row in sweep_rows(shape, spacing / 4.0, axis.to_degrees() + 90.0) {
        if row.len() != 1 {
            continue;
        }
        let (a, b) = (row[0][0], row[0][1]);
        let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let width = dist(a, b);
        let nearest = chains
            .iter()
            .enumerate()
            .map(|(i, (c, _))| (i, dist(*c.last().unwrap(), mid)))
            .filter(|&(_, d)| d <= 0.6 * spacing)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match nearest {
            Some((i, _)) => {
                chains[i].0.push(mid);
                chains[i].1 = chains[i].1.max(width);
            }
            None => chains.push((vec![mid], width)),
        }
    }

    let mut out = Vec::new();
    for (chain, widest) in chains {
        if chain.len() < 2 || widest < 0.15 * spacing {
            continue;
        }
        let mut run: Vec<Pt> = Vec::new();
        for p in chain.into_iter().chain(std::iter::once((f64::NAN, f64::NAN))) {
            if !p.0.is_nan() && original.covers_point(p) {
                run.push(p);
                continue;
            }
            if run.len() >= 2 && crate::geo::polyline_length(&run) >= 0.3 * spacing {
                out.push(std::mem::take(&mut run));
            }
            run.clear();
        }
    }
    out
}

fn dist(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Centrelines for what `strokes` leave empty and thin inside
/// `region.inset(inset)`.
pub(crate) fn gap_fill(region: &Region, strokes: &Strokes, spacing: f64, inset: f64) -> Strokes {
    if region.is_empty() || spacing <= 0.0 {
        return vec![];
    }
    let target = region.inset(inset);
    if target.is_empty() {
        return vec![];
    }
    let leftover = target.difference(&Region::around_strokes(strokes, 0.75 * spacing));
    let mut out = Vec::new();
    for contour in outer_contours(&leftover) {
        let shape = Region::from_ring(&contour);
        if shape.inset(0.6 * spacing).is_empty() {
            out.extend(centreline(&shape, region, spacing));
        }
    }
    out
}

/// Outer contours only, holes filled in; each outer its own polygon.
pub(crate) fn solid_outers(region: &Region) -> Region {
    let polygons: Vec<Vec<Vec<Pt>>> = outer_contours(region).into_iter().map(|r| vec![r]).collect();
    if polygons.is_empty() {
        return Region::default();
    }
    Region::from_polygons(&polygons)
}

/// Whether every edge of the closed `ring` stays inside `region`.
pub(crate) fn contour_inside(region: &Region, ring: &[Pt]) -> bool {
    let mut pts: Vec<Pt> = Vec::with_capacity(ring.len());
    for &p in ring {
        if pts.last() != Some(&p) {
            pts.push(p);
        }
    }
    while pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    if pts.len() < 3 {
        return false;
    }
    (0..pts.len()).all(|i| region.segment_inside(pts[i], pts[(i + 1) % pts.len()], 16))
}

/// Shoelace area: positive for an outer contour, negative for a hole.
pub(crate) fn signed_area(ring: &[Pt]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    (0..n).map(|i| ring[i].0 * ring[(i + 1) % n].1 - ring[(i + 1) % n].0 * ring[i].1).sum::<f64>() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::Region;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)])
    }

    #[test]
    fn square_gets_nested_rings() {
        let rings = fill_concentric(&square(), 1.0, None);
        // 20mm square, spacing 1, first ring at 0.5 inset: rings at
        // 0.5, 1.5, … 9.5 → 10 rings, then the region is gone.
        assert_eq!(rings.len(), 10, "got {}", rings.len());
        // First ring at inset spacing/2: a 19x19 square with a corner at
        // (0.5, 0.5). The kernel may start the ring at any vertex, so check
        // membership rather than ordering.
        assert!(
            rings[0].iter().any(|&(x, y)| x == 0.5 && y == 0.5),
            "first ring {:?}",
            rings[0][0]
        );
    }

    #[test]
    fn corridor_dies_quickly() {
        let thin = Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 2.0), (0.0, 2.0)]);
        let rings = fill_concentric(&thin, 1.0, None);
        assert!(rings.len() <= 4, "got {}", rings.len());
    }

    #[test]
    fn outer_shrinks_up_to_the_hole() {
        let region = Region::from_polygons(&[vec![
            vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)],
            vec![(6.0, 6.0), (14.0, 6.0), (14.0, 14.0), (6.0, 14.0)],
        ]]);
        let rings = fill_concentric(&region, 1.0, None);
        // Wall is 6 mm. First ring at 0.5 mm, then every 1 mm, last one
        // still outside the hole. No second family grows out of the courtyard.
        assert!(
            (5..=6).contains(&rings.len()),
            "got {} rings",
            rings.len()
        );
        for r in &rings {
            let minx = r.iter().map(|p| p.0).fold(f64::MAX, f64::min);
            assert!(
                minx < 6.0,
                "a ring grew from the hole (min x {minx})"
            );
            for &p in r {
                assert!(
                    region.covers_point(p),
                    "ring point {p:?} left the wall"
                );
            }
        }
    }

    /// An island inside the hole of a doughnut gets its own rings, from
    /// both the concentric fill and the spiral.
    #[test]
    fn island_in_a_hole_is_filled() {
        let square = |x: f64, y: f64, s: f64| vec![(x, y), (x + s, y), (x + s, y + s), (x, y + s)];
        let frame = Region::from_polygons(&[vec![square(10.0, 10.0, 20.0), square(15.0, 15.0, 10.0)]]);
        let region = Region::from_polygons(&[vec![square(0.0, 0.0, 40.0)]]).difference(&frame);
        let on_island = |s: &Strokes| s.iter().filter(|l| l.iter().all(|p| p.0 > 15.0 && p.0 < 25.0 && p.1 > 15.0 && p.1 < 25.0)).count();
        assert!(on_island(&fill_concentric(&region, 2.0, None)) >= 2);
        assert!(on_island(&crate::spiral::fill_spiral(&region, 2.0)) >= 1);
    }

    /// A star's spike tips, which the next ring leaves far behind, get
    /// centrelines; the rings themselves stay closed and nested.
    #[test]
    fn spike_tips_get_centrelines() {
        let pts: Vec<Pt> = (0..10)
            .map(|i| {
                let a = std::f64::consts::PI * i as f64 / 5.0;
                let r = if i % 2 == 0 { 20.0 } else { 7.0 };
                (r * a.cos(), r * a.sin())
            })
            .collect();
        let star = Region::from_ring(&pts);
        let out = fill_concentric(&star, 1.0, None);
        let open = out.iter().filter(|s| s.first() != s.last()).count();
        assert!(open >= 5, "only {open} centrelines");
        for s in &out {
            for &p in s {
                assert!(star.covers_point(p), "{p:?} outside");
            }
        }
    }

    #[test]
    fn thin_leftovers_get_a_centreline() {
        let bar = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 1.6), (0.0, 1.6)]);
        let lines = gap_fill(&bar, &vec![], 1.0, 0.3);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].iter().all(|p| (p.1 - 0.8).abs() < 1e-6));
        assert!(crate::geo::polyline_length(&lines[0]) > 25.0);
    }
}
