//! Parallel scanlines clipped to a region: the first step of every
//! direction-based fill.
//!
//! This is the textbook scanline polygon fill. Each line of the family is
//! cut against every edge of every ring at once; the cuts, sorted along
//! the line, pair up first with second, third with fourth, and each pair
//! bounds a piece inside the region (the even-odd rule, so holes need no
//! special case). Edges are half-open in the across direction: an edge
//! counts when the line lies at or above its lower end and strictly below
//! its upper end, so a line through a vertex is cut once and an edge lying
//! along the line not at all.
//!
//! Lines are found without any boolean clipping: the edges are sorted by
//! their lower end and kept in an active list as the line moves up, so a
//! sweep costs about one pass over the edges plus the cuts it finds.

use crate::anchor::Anchor;
use crate::geo::{Pt, Region, Strokes, MIN_SEGMENT_MM};

/// Where a line ends on the outline: edge `edge` of ring `ring`, numbered as
/// in [`Region::all_rings_mm`], the edge running from point `edge` to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnEdge {
    pub ring: usize,
    pub edge: usize,
}

/// One scanline piece and the outline edges its two ends lie on.
#[derive(Clone, Debug)]
pub struct Fragment {
    pub line: Vec<Pt>,
    pub ends: [OnEdge; 2],
}

/// The line family's frame: `u` along the lines, `v` across them.
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

    fn page(self, u: f64, v: f64) -> Pt {
        (u * self.cos - v * self.sin, u * self.sin + v * self.cos)
    }
}

/// Lowest and highest across-coordinate of the region's points.
fn extent(rings: &[Vec<Pt>], frame: Frame) -> (f64, f64) {
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for &p in rings.iter().flatten() {
        let v = frame.v(p);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    (lo, hi)
}

/// Number of lines and the first one, laid symmetrically over the extent.
fn centred(lo: f64, hi: f64, spacing: f64) -> (usize, f64) {
    let count = ((hi - lo) / spacing).floor() as usize + 1;
    (count, (lo + hi) / 2.0 - (count as f64 - 1.0) * spacing / 2.0)
}

/// Number of lines and the first one, on the lattice through across-coordinate `a`.
fn through(lo: f64, hi: f64, spacing: f64, a: f64) -> (usize, f64) {
    let first = a + ((lo - a) / spacing).floor() * spacing;
    (((hi - first) / spacing).floor() as usize + 1, first)
}

/// Whether the inputs can produce any line at all.
fn usable(region: &Region, spacing: f64) -> bool {
    !region.is_empty() && spacing > 0.0 && spacing.is_finite()
}

/// The pieces of `count` lines from `first`, `spacing` apart, each tagged with
/// the index of its line.
fn cut_rows(rings: &[Vec<Pt>], frame: Frame, first: f64, count: usize, spacing: f64) -> Vec<(usize, Fragment)> {
    let mut out = Vec::new();
    cut_rows_into(rings, frame, first, count, spacing, |i, pieces| {
        for f in pieces {
            out.push((i, f));
        }
    });
    out
}

/// [`cut_rows`] streamed: `row` is called once per line that has pieces,
/// with the index of the line and the pieces of that line, as the sweep
/// advances. Only one line's active-edge bookkeeping is alive at a time.
fn cut_rows_into(
    rings: &[Vec<Pt>],
    frame: Frame,
    first: f64,
    count: usize,
    spacing: f64,
    mut row: impl FnMut(usize, Vec<Fragment>),
) {
    struct Edge {
        lo: f64,
        hi: f64,
        p: (f64, f64),
        q: (f64, f64),
        at: OnEdge,
    }
    let mut edges: Vec<Edge> = Vec::new();
    for (r, ring) in rings.iter().enumerate() {
        let n = ring.len();
        if n < 2 {
            continue;
        }
        for k in 0..n {
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            let p = (frame.u(a), frame.v(a));
            let q = (frame.u(b), frame.v(b));
            if p.1 == q.1 {
                continue;
            }
            edges.push(Edge { lo: p.1.min(q.1), hi: p.1.max(q.1), p, q, at: OnEdge { ring: r, edge: k } });
        }
    }
    edges.sort_by(|a, b| a.lo.total_cmp(&b.lo));

    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut cuts: Vec<(f64, OnEdge)> = Vec::new();
    for i in 0..count {
        let v = first + i as f64 * spacing;
        while next < edges.len() && edges[next].lo <= v {
            active.push(next);
            next += 1;
        }
        active.retain(|&e| edges[e].hi > v);
        cuts.clear();
        for &e in &active {
            let Edge { p, q, at, .. } = edges[e];
            let t = (v - p.1) / (q.1 - p.1);
            cuts.push((p.0 + t * (q.0 - p.0), at));
        }
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut pieces: Vec<Fragment> = Vec::new();
        for &[(ua, ea), (ub, eb)] in cuts.as_chunks::<2>().0 {
            if ub - ua < MIN_SEGMENT_MM {
                continue;
            }
            pieces.push(Fragment { line: vec![frame.page(ua, v), frame.page(ub, v)], ends: [ea, eb] });
        }
        if !pieces.is_empty() {
            row(i, pieces);
        }
    }
}

fn strokes_of(fragments: Vec<Fragment>) -> Strokes {
    fragments.into_iter().map(|f| f.line).collect()
}

fn fragments_local(region: &Region, spacing: f64, angle: f64) -> Vec<Fragment> {
    let rings = region.all_rings_mm();
    let frame = Frame::new(angle);
    let (lo, hi) = extent(&rings, frame);
    let (count, first) = centred(lo, hi, spacing);
    cut_rows(&rings, frame, first, count, spacing).into_iter().map(|(_, f)| f).collect()
}

fn fragments_on_lattice(region: &Region, spacing: f64, angle: f64, point: Pt) -> Vec<Fragment> {
    let rings = region.all_rings_mm();
    let frame = Frame::new(angle);
    let (lo, hi) = extent(&rings, frame);
    let (count, first) = through(lo, hi, spacing, frame.v(point));
    cut_rows(&rings, frame, first, count, spacing).into_iter().map(|(_, f)| f).collect()
}

/// Parallel lines `spacing` apart at `angle` degrees inside `region`, laid
/// symmetrically about the middle of the region's extent.
pub fn sweep_lines(region: &Region, spacing: f64, angle: f64) -> Strokes {
    if !usable(region, spacing) {
        return vec![];
    }
    strokes_of(fragments_local(region, spacing, angle))
}

/// [`sweep_lines`] with its lines placed by `anchor`.
pub fn sweep_anchored(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    strokes_of(sweep_anchored_fragments(region, spacing, angle, anchor))
}

/// [`sweep_anchored`], each line with the outline edges its ends lie on.
pub fn sweep_anchored_fragments(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Vec<Fragment> {
    let mut out = Vec::new();
    sweep_anchored_fragments_into(region, spacing, angle, anchor, |f| out.push(f));
    out
}

/// [`sweep_anchored_fragments`] streamed: one line of the family at a time,
/// in order across the family. The memory peak is a single row's pieces.
pub fn sweep_anchored_fragments_into(
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: Anchor,
    mut sink: impl FnMut(Fragment),
) {
    if !usable(region, spacing) {
        return;
    }
    let rings = region.all_rings_mm();
    let frame = Frame::new(angle);
    let (lo, hi) = extent(&rings, frame);
    let (count, first) = if anchor == Anchor::LOCAL {
        centred(lo, hi, spacing)
    } else {
        let (_, own_first) = centred(lo, hi, spacing);
        let own = (-own_first * frame.sin, own_first * frame.cos);
        through(lo, hi, spacing, frame.v(anchor.resolve(own)))
    };
    cut_rows_into(&rings, frame, first, count, spacing, |_, pieces| {
        for f in pieces {
            sink(f);
        }
    });
}

/// [`sweep_anchored`] streamed: pieces are emitted line by line, clipped as
/// they come (the sweep already produces them inside the region).
pub fn sweep_anchored_into(region: &Region, spacing: f64, angle: f64, anchor: Anchor, sink: &mut dyn crate::Sink) {
    if !usable(region, spacing) {
        return;
    }
    sweep_anchored_fragments_into(region, spacing, angle, anchor, |f| sink.stroke(f.line));
}

/// Like [`sweep_lines`], but one of the lines passes through `through`.
pub fn sweep_lines_through(region: &Region, spacing: f64, angle: f64, through: Pt) -> Strokes {
    if !usable(region, spacing) {
        return vec![];
    }
    strokes_of(fragments_on_lattice(region, spacing, angle, through))
}

/// [`sweep_lines`] grouped by line: one entry per line that has pieces, in
/// order across the family, each listing its pieces along the line.
pub(crate) fn sweep_rows(region: &Region, spacing: f64, angle: f64) -> Vec<Strokes> {
    if !usable(region, spacing) {
        return vec![];
    }
    let rings = region.all_rings_mm();
    let frame = Frame::new(angle);
    let (lo, hi) = extent(&rings, frame);
    let (count, first) = centred(lo, hi, spacing);
    let mut rows: Vec<Strokes> = Vec::new();
    let mut last = usize::MAX;
    for (i, f) in cut_rows(&rings, frame, first, count, spacing) {
        if i != last {
            rows.push(Vec::new());
            last = i;
        }
        rows.last_mut().unwrap().push(f.line);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)])
    }

    #[test]
    fn lines_are_symmetric_and_half_open() {
        // Extent 0..20, spacing 1: 21 lines at 0, 1, … 20. The one at 0
        // crosses the side edges at their lower ends and counts; the one at
        // 20 meets their upper ends and does not.
        let lines = sweep_lines(&square(), 1.0, 0.0);
        assert_eq!(lines.len(), 20);
        assert_eq!(lines[0], vec![(0.0, 0.0), (20.0, 0.0)]);
        assert_eq!(lines[19], vec![(0.0, 19.0), (20.0, 19.0)]);
        // Centred: extent 0..10.5 at spacing 1 gives 11 lines, 0.25 … 10.25.
        let r = Region::from_ring(&[(0.0, 0.0), (5.0, 0.0), (5.0, 10.5), (0.0, 10.5)]);
        let ys: Vec<f64> = sweep_lines(&r, 1.0, 0.0).iter().map(|l| l[0].1).collect();
        assert!((ys[0] - 0.25).abs() < 1e-12 && (ys[10] - 10.25).abs() < 1e-12, "{ys:?}");
    }

    #[test]
    fn holes_split_lines_and_ends_name_their_edges() {
        let region = Region::from_polygons(&[vec![
            vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)],
            vec![(6.0, 6.0), (14.0, 6.0), (14.0, 14.0), (6.0, 14.0)],
        ]]);
        let rings = region.all_rings_mm();
        let frags = sweep_anchored_fragments(&region, 1.0, 30.0, Anchor::LOCAL);
        let on_line = |p: Pt, e: OnEdge| {
            let ring = &rings[e.ring];
            let (a, b) = (ring[e.edge], ring[(e.edge + 1) % ring.len()]);
            let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
            cross.abs() / (b.0 - a.0).hypot(b.1 - a.1) < 1e-9
        };
        assert!(frags.iter().any(|f| f.ends[0].ring != f.ends[1].ring), "no line stops at the hole");
        for f in &frags {
            assert_eq!(f.line.len(), 2);
            assert!(on_line(f.line[0], f.ends[0]) && on_line(f.line[1], f.ends[1]), "{f:?}");
            let mid = ((f.line[0].0 + f.line[1].0) / 2.0, (f.line[0].1 + f.line[1].1) / 2.0);
            assert!(region.covers_point(mid), "{f:?} crosses the hole");
        }
        let plain = sweep_anchored(&region, 1.0, 30.0, Anchor::LOCAL);
        assert_eq!(plain, frags.iter().map(|f| f.line.clone()).collect::<Strokes>());
    }

    #[test]
    fn a_global_anchor_puts_a_line_through_its_point() {
        let lines = sweep_anchored(&square(), 1.5, 20.0, Anchor::global(3.3, 4.4));
        let (sin, cos) = 20f64.to_radians().sin_cos();
        let v = |p: Pt| -p.0 * sin + p.1 * cos;
        let a = v((3.3, 4.4));
        for l in &lines {
            let k = (v(l[0]) - a) / 1.5;
            assert!((k - k.round()).abs() < 1e-9, "line off the lattice: {l:?}");
        }
        let through = sweep_lines_through(&square(), 1.5, 20.0, (3.3, 4.4));
        assert_eq!(lines, through);
    }

    #[test]
    fn nothing_for_nothing() {
        assert!(sweep_lines(&Region::default(), 1.0, 0.0).is_empty());
        assert!(sweep_lines(&square(), 0.0, 0.0).is_empty());
    }

    #[test]
    fn a_local_shift_moves_the_symmetric_layout() {
        let plain = sweep_anchored(&square(), 1.0, 0.0, Anchor::LOCAL);
        let shifted = sweep_anchored(&square(), 1.0, 0.0, Anchor::local(0.0, 0.25));
        assert_eq!(plain.len(), shifted.len());
        for (a, b) in plain.iter().zip(&shifted) {
            assert!((b[0].1 - a[0].1 - 0.25).abs() < 1e-9, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn streamed_sweep_equals_the_one_shot() {
        let region = Region::from_polygons(&[vec![
            vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)],
            vec![(6.0, 6.0), (14.0, 6.0), (14.0, 14.0), (6.0, 14.0)],
        ]]);
        for anchor in [Anchor::LOCAL, Anchor::GLOBAL, Anchor::global(3.3, 4.4)] {
            for angle in [0.0, 30.0, 90.0] {
                let direct = sweep_anchored(&region, 0.9, angle, anchor);
                let mut streamed: Strokes = Vec::new();
                sweep_anchored_into(&region, 0.9, angle, anchor, &mut streamed);
                assert_eq!(direct, streamed, "anchor {anchor:?} angle {angle}");
            }
        }
    }
}
