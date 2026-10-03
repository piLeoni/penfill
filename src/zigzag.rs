//! Zigzag: the lines of a hatch joined into serpentine paths, each turn
//! running along the inset outline.
//!
//! This is boustrophedon coverage: the pen runs along one line, turns, and
//! runs back along the next. The lines come from the sweep of the inset
//! region and are first grouped into columns, stacks of lines on
//! consecutive rows that overlap along the line direction, so that a turn
//! only ever joins neighbours. Each line joins at most one column and each
//! column takes at most one line per row, so a star or a comb splits into
//! one column per arm rather than collapsing into a single tangle.
//!
//! Walking a column, the turn between two lines follows the outline the
//! lines were cut from, round its corners, when both ends lie on the same
//! ring and the way round stays between the two rows; otherwise it is a
//! short straight hop that stays inside the shape and close to its wall.
//! Turns longer than a few spacings lift the pen instead: following the
//! outline any further would draw extra hatch along the walls, and so do
//! turns that would run back along nearly all of one of their lines.

use crate::anchor::Anchor;
use crate::geo::{polyline_length, Pt, Region, Strokes, MIN_SEGMENT_MM};
use crate::sweep::{sweep_anchored_fragments, OnEdge};
#[cfg(test)]
use std::f64::consts::PI;

/// The longest turn, in spacings; past it the pen lifts.
const MAX_HOP_TIMES_SPACING: f64 = 6.0;

/// Rows further apart than this, in spacings, are not neighbours.
const ADJACENT_ROWS: f64 = 1.65;

/// How far, in spacings, a turn along the outline may stray past the two
/// rows it joins. Further out it runs beside or across other lines.
const STRIP_SLACK: f64 = 0.05;

/// Ends further apart than this along the hatch, in spacings, with one
/// line reaching less than this past the other's end, make the turn run
/// back along that line; the pen lifts instead.
const DOUBLE_BACK: f64 = 1.5;

fn dist(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Along- and across-coordinates for lines at `angle` degrees.
#[derive(Clone, Copy)]
struct Frame {
    sin: f64,
    cos: f64,
}

impl Frame {
    fn new(angle: f64) -> Frame {
        let (sin, cos) = angle.to_radians().sin_cos();
        Frame { sin, cos }
    }

    fn u(self, p: Pt) -> f64 {
        p.0 * self.cos + p.1 * self.sin
    }

    fn v(self, p: Pt) -> f64 {
        -p.0 * self.sin + p.1 * self.cos
    }
}

/// The lines grouped into columns, as indices into `fragments`, in drawing
/// order.
fn bands(fragments: &[Vec<Pt>], spacing: f64, angle: f64) -> Vec<Vec<usize>> {
    let frame = Frame::new(angle);
    // (row, lo, hi) of each line.
    let spans: Vec<(f64, f64, f64)> = fragments
        .iter()
        .map(|f| {
            let (a, b) = (f[0], *f.last().unwrap());
            let (ua, ub) = (frame.u(a), frame.u(b));
            ((frame.v(a) + frame.v(b)) / 2.0, ua.min(ub), ua.max(ub))
        })
        .collect();
    let mut order: Vec<usize> = (0..fragments.len()).collect();
    order.sort_by(|&a, &b| spans[a].0.total_cmp(&spans[b].0).then(spans[a].1.total_cmp(&spans[b].1)));

    let mut columns: Vec<Vec<usize>> = Vec::new();
    for i in order {
        let (v, lo, hi) = spans[i];
        let mut best: Option<(usize, f64)> = None;
        for (c, column) in columns.iter().enumerate() {
            let (pv, plo, phi) = spans[*column.last().unwrap()];
            if v - pv <= 1e-9 || v - pv > 1.5 * spacing {
                continue;
            }
            let overlap = hi.min(phi) - lo.max(plo);
            let shorter = (hi - lo).min(phi - plo);
            let links = if overlap > 0.0 {
                overlap >= (0.5 * spacing).min(0.5 * shorter)
            } else {
                -overlap <= 0.15 * spacing
            };
            if links && best.is_none_or(|(_, o)| overlap > o) {
                best = Some((c, overlap));
            }
        }
        match best {
            Some((c, _)) => columns[c].push(i),
            None => columns.push(vec![i]),
        }
    }
    columns
}

/// Serpentine hatch over `region`, half a spacing in from its walls.
pub fn fill_zigzag(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    fill_zigzag_inset(region, spacing, angle, None, anchor)
}

/// Serpentine hatch `inset_mm` (default half the spacing) in from the walls.
pub fn fill_zigzag_inset(region: &Region, spacing: f64, angle: f64, inset_mm: Option<f64>, anchor: Anchor) -> Strokes {
    if region.is_empty() || spacing <= 0.0 {
        return vec![];
    }
    let inset = inset_mm.unwrap_or(spacing / 2.0);
    let target = region.inset(inset);
    if target.is_empty() {
        return vec![];
    }
    let fragments = sweep_anchored_fragments(&target, spacing, angle, anchor);
    let (lines, ends): (Vec<Vec<Pt>>, Vec<[OnEdge; 2]>) = fragments.into_iter().map(|f| (f.line, f.ends)).unzip();
    let walker = Walker { region, rings: target.all_rings_mm(), frame: Frame::new(angle), spacing, inset };

    let mut out = Vec::new();
    for column in bands(&lines, spacing, angle) {
        let mut path: Vec<Pt> = lines[column[0]].clone();
        // Where the line being drawn starts.
        let mut from = path[0];
        let mut at = ends[column[0]][1];
        for &i in &column[1..] {
            let last = *path.last().unwrap();
            let (line, [start, end]) = if dist(last, lines[i][1]) < dist(last, lines[i][0]) {
                (vec![lines[i][1], lines[i][0]], [ends[i][1], ends[i][0]])
            } else {
                (lines[i].clone(), ends[i])
            };
            let turn = if walker.doubles_back(from, last, &line) {
                None
            } else {
                walker.turn(last, at, line[0], start)
            };
            match turn {
                Some(turn) => extend(&mut path, &turn),
                None => {
                    if path.len() >= 2 {
                        out.push(std::mem::take(&mut path));
                    }
                    path.clear();
                }
            }
            from = line[0];
            extend(&mut path, &line);
            at = end;
        }
        if path.len() >= 2 {
            out.push(path);
        }
    }
    out
}

/// Appends `points`, skipping any that would not move the pen.
fn extend(path: &mut Vec<Pt>, points: &[Pt]) {
    for &p in points {
        if path.last().is_none_or(|&q| dist(p, q) >= MIN_SEGMENT_MM) {
            path.push(p);
        }
    }
}

/// What a turn between two lines needs to know.
struct Walker<'a> {
    region: &'a Region,
    rings: Vec<Vec<Pt>>,
    frame: Frame,
    spacing: f64,
    inset: f64,
}

impl Walker<'_> {
    /// The way from `a` (on outline edge `ea`) to `b` (on `eb`), or `None`
    /// when the pen must lift.
    fn turn(&self, a: Pt, ea: OnEdge, b: Pt, eb: OnEdge) -> Option<Vec<Pt>> {
        let rows = (self.frame.v(a) - self.frame.v(b)).abs();
        if rows <= 1e-9 || rows > ADJACENT_ROWS * self.spacing {
            return None;
        }
        let cap = MAX_HOP_TIMES_SPACING * self.spacing;
        if ea.ring == eb.ring {
            let route = ring_route(&self.rings[ea.ring], a, ea.edge, b, eb.edge);
            let slack = STRIP_SLACK * self.spacing;
            let (lo, hi) = (self.frame.v(a).min(self.frame.v(b)) - slack, self.frame.v(a).max(self.frame.v(b)) + slack);
            if polyline_length(&route) <= cap && route.iter().all(|&p| (lo..=hi).contains(&self.frame.v(p))) {
                return Some(route);
            }
        }
        let straight = dist(a, b) <= cap
            && self.region.segment_inside_relaxed(a, b, 32, 0.08)
            && (1..8).all(|k| {
                let t = k as f64 / 8.0;
                let p = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
                self.region.dist_to_boundary(p) <= self.inset + 2.0 * self.spacing
            });
        straight.then(|| vec![a, b])
    }

    /// Whether the turn from the line `from`→`a` into `next` would run
    /// back along nearly all of one of them, a thin Z: their ends lie
    /// more than [`DOUBLE_BACK`] spacings apart along the hatch and one
    /// line reaches less than that past the other's end.
    fn doubles_back(&self, from: Pt, a: Pt, next: &[Pt]) -> bool {
        let (f, s) = (self.frame, self.spacing);
        let (b, end) = (next[0], next[1]);
        let rows = (f.v(a) - f.v(b)).abs();
        if rows <= 1e-9 || rows > ADJACENT_ROWS * s {
            return false;
        }
        let (ua, ub) = (f.u(a), f.u(b));
        let dir = (ua - f.u(from)).signum();
        let past = (ub - ua) * dir;
        let limit = DOUBLE_BACK * s;
        if past > limit {
            (ua - f.u(end)) * dir < limit
        } else if past < -limit {
            (ub - f.u(from)) * dir < limit
        } else {
            false
        }
    }
}

/// The shorter way round `ring` from `a` on edge `i` to `b` on edge `j`,
/// through every corner in between, both ends included.
fn ring_route(ring: &[Pt], a: Pt, i: usize, b: Pt, j: usize) -> Vec<Pt> {
    let n = ring.len();
    let along = |p: Pt, k: usize| dist(ring[k], p);
    let forward = if i == j && along(b, i) >= along(a, i) {
        vec![a, b]
    } else {
        let mut route = vec![a];
        let mut k = (i + 1) % n;
        loop {
            route.push(ring[k]);
            if k == j {
                break;
            }
            k = (k + 1) % n;
        }
        route.push(b);
        route
    };
    let backward = if i == j && along(b, i) <= along(a, i) {
        vec![a, b]
    } else {
        let mut route = vec![a];
        let mut k = i;
        loop {
            route.push(ring[k]);
            if k == (j + 1) % n {
                break;
            }
            k = (k + n - 1) % n;
        }
        route.push(b);
        route
    };
    if polyline_length(&backward) < polyline_length(&forward) {
        backward
    } else {
        forward
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::Region;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)])
    }

    #[test]
    fn square_becomes_one_serpentine() {
        let zz = fill_zigzag(&square(), 1.0, 0.0, Anchor::LOCAL);
        assert_eq!(zz.len(), 1, "square hatch must be one connected path");
        assert_eq!(zz[0].len(), 38, "19 scanlines × 2 pts");
        // Serpentine alternation:
        assert_eq!(zz[0][1], (19.5, 0.5));
        assert_eq!(zz[0][3], (0.5, 1.5));
        assert_eq!(zz[0][5], (19.5, 2.5));
    }

    /// The turns follow the inset outline round its corners instead of
    /// cutting across: every turn lies on it all the way, and passes the
    /// corners between its two lines.
    #[test]
    fn turns_follow_the_inset_outline() {
        let ring: Vec<Pt> = (0..12)
            .map(|i| {
                let a = PI / 6.0 * i as f64 + 0.1;
                (10.0 + 10.0 * a.cos(), 10.0 + 10.0 * a.sin())
            })
            .collect();
        let region = Region::from_ring(&ring);
        let inset = region.inset(0.5);
        let zz = fill_zigzag(&region, 1.0, 0.0, Anchor::LOCAL);
        assert_eq!(zz.len(), 1, "a convex shape is one serpentine");
        let turns: Vec<_> = zz[0].windows(2).filter(|w| (w[0].1 - w[1].1).abs() > 1e-9).collect();
        for w in &turns {
            let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
            assert!(inset.dist_to_boundary(mid) < 1e-6, "turn {w:?} cuts inside the outline");
        }
        let corners = inset.all_rings_mm().remove(0);
        assert!(corners.iter().any(|c| zz[0].iter().any(|p| dist(*p, *c) < 1e-9)), "no corner on the path");
    }

    #[test]
    fn an_angle_renders_a_diamond_head() {
        let zz = fill_zigzag(&square(), 1.0, 45.0, Anchor::LOCAL);
        assert!(!zz.is_empty());
        assert!(zz.len() <= 4, "got {} paths", zz.len());
    }

    #[test]
    fn courtyard_splits_the_serpentine() {
        let outer = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)];
        let hole = vec![(6.0, 6.0), (14.0, 6.0), (14.0, 14.0), (6.0, 14.0)];
        let ring = Region::from_polygons(&[vec![outer, hole]]);
        let zz = fill_zigzag(&ring, 1.0, 0.0, Anchor::LOCAL);
        // No segment may cross the courtyard interior.
        for path in &zz {
            for w in path.windows(2) {
                let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
                assert!(
                    !(mid.0 > 6.0 && mid.0 < 14.0 && mid.1 > 6.0 && mid.1 < 14.0),
                    "connector crossed the courtyard at {:?}",
                    mid
                );
            }
        }
        // Banding joined the rows around the hole and both side walls: far
        // fewer paths than the 5 raw fragments (left wall, right wall × 5
        // scan rows through the hole band + full rows above/below; the
        // connector lift at the hole boundaries splits verticals too).
        assert!(
            zz.len() <= 20,
            "banding regressed: {} paths",
            zz.len()
        );
    }

    #[test]
    fn two_buildings_never_bridge() {
        let two = Region::from_polygons(&[
            vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]],
            vec![vec![(20.0, 0.0), (30.0, 0.0), (30.0, 10.0), (20.0, 10.0)]],
        ]);
        let zz = fill_zigzag(&two, 1.0, 0.0, Anchor::LOCAL);
        for path in &zz {
            for w in path.windows(2) {
                assert!(
                    !(w[0].0 <= 10.5 && w[1].0 >= 19.5 || w[0].0 >= 19.5 && w[1].0 <= 10.5),
                    "bridge segment {:?}",
                    w
                );
            }
        }
        assert!(zz.len() >= 2, "each building gets its own serpentine");
    }

    fn star(spikes: usize, outer: f64, inner: f64) -> Region {
        let mut pts: Vec<Pt> = Vec::new();
        for i in 0..2 * spikes {
            let a = std::f64::consts::PI * (i as f64) / (spikes as f64);
            let r = if i % 2 == 0 { outer } else { inner };
            pts.push((30.0 + r * a.cos(), 30.0 + r * a.sin()));
        }
        Region::from_ring(&pts)
    }

    /// Connectors longer than a couple of line-spacings bridge separate
    /// parts of the shape. Those are pen lifts, not ink.
    #[test]
    fn star_connectors_stay_within_a_few_spacings() {
        let star = star(11, 28.0, 12.0);
        let spacing = 0.8;
        let zz = fill_zigzag(&star, spacing, 0.0, Anchor::LOCAL);
        let limit = spacing * MAX_HOP_TIMES_SPACING + 1e-6;
        for path in &zz {
            for w in path.windows(2) {
                let dy = (w[0].1 - w[1].1).abs();
                if dy < 1e-5 {
                    continue;
                }
                let len = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
                assert!(
                    len <= limit,
                    "connector {len:.2} mm is more than {MAX_HOP_TIMES_SPACING}× spacing ({spacing})"
                );
            }
        }
    }

    #[test]
    fn star_splits_into_many_columns_not_one_serpentine() {
        let star = star(11, 28.0, 12.0);
        let target = star.inset(0.4);
        let frags = crate::sweep::sweep_lines(&target, 0.8, 0.0);
        let cols = bands(&frags, 0.8, 0.0);
        let longest = cols.iter().map(|c| c.len()).max().unwrap();
        // Before the fix, union-find merged ~137/139 fragments into one band.
        assert!(
            cols.len() >= 10,
            "star banding collapsed: {} columns (longest {} frags) for {} fragments",
            cols.len(),
            longest,
            frags.len()
        );
        assert!(
            longest <= 45,
            "one column still chains {} fragments (mega-band)",
            longest
        );
    }

    /// Every line of the hatch is drawn exactly once, whole.
    #[test]
    fn every_line_is_drawn_once() {
        let star = star(7, 25.0, 10.0);
        let target = star.inset(0.5);
        let lines = crate::sweep::sweep_lines(&target, 1.0, 20.0);
        let zz = fill_zigzag(&star, 1.0, 20.0, Anchor::LOCAL);
        let hatch: f64 = lines.iter().map(|l| polyline_length(l)).sum();
        let drawn: f64 = zz.iter().map(|p| polyline_length(p)).sum();
        assert!(drawn >= hatch - 1e-6, "drawn {drawn} < hatch {hatch}");
        for l in &lines {
            let found = zz.iter().any(|p| {
                p.windows(2).any(|w| (w[0] == l[0] && w[1] == l[1]) || (w[0] == l[1] && w[1] == l[0]))
            });
            assert!(found, "line {l:?} missing");
        }
    }

    /// A rectangle with rectangular notches and teeth along its sides,
    /// turned at random: the steps real building outlines have.
    fn stepped_building(seed: u64) -> Region {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut rand = move || {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let (w, h) = (2.0 + 4.0 * rand(), 2.0 + 4.0 * rand());
        let corners = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
        let mut ring: Vec<Pt> = Vec::new();
        for k in 0..4 {
            let (p, q) = (corners[k], corners[(k + 1) % 4]);
            let len = dist(p, q);
            let d = ((q.0 - p.0) / len, (q.1 - p.1) / len);
            let out = (d.1, -d.0);
            let at = |t: f64, depth: f64| (p.0 + t * d.0 + depth * out.0, p.1 + t * d.1 + depth * out.1);
            ring.push(p);
            for half in 0..2 {
                if rand() < 0.3 {
                    continue;
                }
                let width = (0.3 + 0.9 * rand()).min(len / 2.0 - 0.2);
                let t = half as f64 * len / 2.0 + 0.1 + rand() * (len / 2.0 - 0.2 - width);
                let depth = if rand() < 0.5 { 0.2 + 0.8 * rand() } else { -(0.2 + 0.4 * rand()) };
                ring.extend([at(t, 0.0), at(t, depth), at(t + width, depth), at(t + width, 0.0)]);
            }
        }
        let (sin, cos) = (360.0 * rand()).to_radians().sin_cos();
        let ring: Vec<Pt> = ring.iter().map(|p| (p.0 * cos - p.1 * sin, p.0 * sin + p.1 * cos)).collect();
        Region::from_ring(&ring)
    }

    fn crosses(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
        let side = |p: Pt, q: Pt, r: Pt| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
        side(c, d, a) * side(c, d, b) < -1e-12 && side(a, b, c) * side(a, b, d) < -1e-12
    }

    /// On stepped outlines no turn strays past its two rows and the path
    /// never crosses itself.
    #[test]
    fn turns_stay_in_their_strip_on_stepped_outlines() {
        let (spacing, angle) = (0.4, 45.0);
        let f = Frame::new(angle);
        for seed in 0..80 {
            let zz = fill_zigzag(&stepped_building(seed), spacing, angle, Anchor::LOCAL);
            for path in &zz {
                let across = |k: usize| (f.v(path[k]) - f.v(path[k + 1])).abs() > 1e-9;
                let mut k = 0;
                while k + 1 < path.len() {
                    if !across(k) {
                        k += 1;
                        continue;
                    }
                    let start = k;
                    while k + 1 < path.len() && across(k) {
                        k += 1;
                    }
                    let turn = &path[start..=k];
                    let (a, b) = (turn[0], turn[turn.len() - 1]);
                    let slack = STRIP_SLACK * spacing + 1e-9;
                    let (lo, hi) = (f.v(a).min(f.v(b)) - slack, f.v(a).max(f.v(b)) + slack);
                    assert!(
                        turn.iter().all(|&p| (lo..=hi).contains(&f.v(p))),
                        "seed {seed}: turn {turn:?} leaves its strip"
                    );
                }
            }
            let segments: Vec<(usize, usize, Pt, Pt)> = zz
                .iter()
                .enumerate()
                .flat_map(|(i, p)| p.windows(2).enumerate().map(move |(k, w)| (i, k, w[0], w[1])))
                .collect();
            for (n, &(i, k, a, b)) in segments.iter().enumerate() {
                for &(j, l, c, d) in &segments[n + 1..] {
                    if i == j && l <= k + 1 {
                        continue;
                    }
                    assert!(!crosses(a, b, c, d), "seed {seed}: {a:?}-{b:?} crosses {c:?}-{d:?}");
                }
            }
        }
    }
}
