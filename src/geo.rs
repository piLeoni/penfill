//! i_overlay-backed geometry kernel for penfill.
//!
//! Coordinates are in **page millimetres** throughout — the crate's unit of
//! account, mirroring the postcards fill contract (`docs/fills.md`).
//! i_overlay snaps each operation's input to an integer grid fitted to its
//! bounds, which removes the floating-point robustness problems (the
//! `buffer(0)` repair dance the Python version needed).
//!
//! The kernel covers exactly what fills need from shapely in the Python
//! original (`fills/_common.py`): region normalization/repair, mitred
//! insetting, and point-in-region testing. Scanline generation deliberately
//! does NOT use boolean clipping — see `sweep.rs` for why.

use std::sync::{Arc, OnceLock};

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::outline::offset::OutlineOffset;
use i_overlay::mesh::stroke::offset::StrokeOffset;
use i_overlay::mesh::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};

/// One coordinate pair in millimetres (the public unit).
pub type Pt = (f64, f64);

/// Open polyline in millimetres (`Strokes` = what fills return).
pub type Strokes = Vec<Vec<Pt>>;

/// i_overlay's point.
type P = [f64; 2];

/// Pieces, each an outer contour (counter-clockwise) followed by its holes
/// (clockwise): the shape of every i_overlay result.
type Shapes = Vec<Vec<Vec<P>>>;

/// Mitred corners are cut where the miter would reach past 5× the offset
/// (shapely's default limit), given to i_overlay as the
/// sharpest corner still mitred: 2·asin(1/5).
const MITER_ANGLE: f64 = 0.4027;

/// Arcs of round joins and caps are split into segments spanning this
/// angle (radians): 63 per full turn.
const ARC_STEP: f64 = 0.1;

/// Points this close to a contour count as on it (and so covered).
const ON_EDGE_MM: f64 = 1e-6;

fn to_p(points: &[Pt]) -> Vec<P> {
    points.iter().map(|&(x, y)| [x, y]).collect()
}

fn from_p(points: &[P]) -> Vec<Pt> {
    points.iter().map(|p| (p[0], p[1])).collect()
}

fn ring_area(ring: &[P]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i][0] * ring[(i + 1) % n][1] - ring[(i + 1) % n][0] * ring[i][1]).sum::<f64>() / 2.0
}

/// Counter-clockwise copies of the rings with at least 3 points, so that
/// non-zero filling reads every ring as covered whatever its input winding.
fn ccw(rings: impl IntoIterator<Item = Vec<P>>) -> Vec<Vec<P>> {
    rings
        .into_iter()
        .filter(|r| r.len() >= 3)
        .map(|mut r| {
            if ring_area(&r) < 0.0 {
                r.reverse();
            }
            r
        })
        .collect()
}

/// The contour without vertices lying on the line through their neighbours.
/// i_overlay snaps to an integer grid, so points that were collinear come
/// back a grid step off the line and survive its own collinear pass.
fn drop_collinear(mut ring: Vec<P>) -> Vec<P> {
    let mut i = 0;
    while ring.len() >= 3 && i < ring.len() {
        let n = ring.len();
        let (a, p, b) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        let off = ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs();
        let between = (p[0] - a[0]) * dx + (p[1] - a[1]) * dy >= 0.0 && (p[0] - b[0]) * dx + (p[1] - b[1]) * dy <= 0.0;
        if between && off <= ON_EDGE_MM * len {
            ring.remove(i);
            i = i.saturating_sub(1);
        } else {
            i += 1;
        }
    }
    ring
}

/// A polygonal region: one or more outer boundaries, each optionally with
/// holes. Mirrors the `as_region()` normalization contract in `_common.py`.
#[derive(Debug, Clone, Default)]
pub struct Region {
    shapes: Shapes,
    /// Built on the first clip and shared by clones: regions never change.
    cutter: OnceLock<Arc<Cutter>>,
}

impl Region {
    fn from_shapes(shapes: Shapes) -> Region {
        let shapes = shapes
            .into_iter()
            .map(|s| s.into_iter().map(drop_collinear).filter(|c| c.len() >= 3).collect::<Vec<_>>())
            .filter(|s| !s.is_empty())
            .collect();
        Region { shapes, cutter: OnceLock::new() }
    }

    fn cutter(&self) -> &Cutter {
        self.cutter.get_or_init(|| Arc::new(Cutter::new(self)))
    }

    /// Build from mm-space polygons-with-holes.
    ///
    /// `input[c][j][k]` = contour `j` of polygon `c`, point `k`. The first
    /// contour of each polygon is the outer boundary; the rest are holes.
    /// Outers are unioned, holes are subtracted — input winding does not
    /// matter.
    pub fn from_polygons(polygons: &[Vec<Vec<Pt>>]) -> Region {
        let outers = ccw(polygons.iter().filter_map(|p| p.first()).map(|c| to_p(c)));
        let holes = ccw(polygons.iter().flat_map(|p| p.iter().skip(1)).map(|c| to_p(c)));
        if outers.is_empty() {
            return Region::default();
        }
        let region = outers.simplify_shape(FillRule::NonZero);
        if holes.is_empty() {
            return Region::from_shapes(region);
        }
        Region::from_shapes(region.overlay(&holes, OverlayRule::Difference, FillRule::NonZero))
    }

    /// Build from rings filled even-odd: a ring inside another is a hole, a
    /// ring inside that an island again — what the contours of a field bound.
    pub fn from_rings_even_odd(rings: &[Vec<Pt>]) -> Region {
        let contours: Vec<Vec<P>> = rings.iter().filter(|r| r.len() >= 3).map(|r| to_p(r)).collect();
        if contours.is_empty() {
            return Region::default();
        }
        Region::from_shapes(contours.simplify_shape(FillRule::EvenOdd))
    }

    /// Build from a single outer ring (mm). Test/fixture convenience.
    pub fn from_ring(ring: &[Pt]) -> Region {
        Region::from_polygons(&[vec![ring.to_vec()]])
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// Axis-aligned bounding box of all rings, in mm.
    pub fn bounds_mm(&self) -> (f64, f64, f64, f64) {
        let mut minx = f64::MAX;
        let mut miny = f64::MAX;
        let mut maxx = f64::MIN;
        let mut maxy = f64::MIN;
        for p in self.shapes.iter().flatten().flatten() {
            minx = minx.min(p[0]);
            miny = miny.min(p[1]);
            maxx = maxx.max(p[0]);
            maxy = maxy.max(p[1]);
        }
        (minx, miny, maxx, maxy)
    }

    /// All contours (outers and holes) as mm rings. Fills that pair
    /// scanline crossings even-odd traverse both kinds together.
    pub fn all_rings_mm(&self) -> Vec<Vec<Pt>> {
        self.shapes.iter().flatten().map(|r| from_p(r)).collect()
    }

    /// The separate pieces, each with its own holes. An island inside a
    /// hole is a piece of its own.
    pub fn parts(&self) -> Vec<Region> {
        self.shapes.iter().map(|s| Region::from_shapes(vec![s.clone()])).collect()
    }

    /// Shrink by `amount` mm with mitred joins — the `inset()` helper.
    ///
    /// A region thinner than the inset collapses to nothing, matching the
    /// "corridor dies after a couple of rings" behaviour the fills expect.
    pub fn inset(&self, amount: f64) -> Region {
        if amount <= 0.0 || self.is_empty() {
            return self.clone();
        }
        let style = OutlineStyle::new(-amount).line_join(LineJoin::Miter(MITER_ANGLE));
        Region::from_shapes(self.shapes.outline(&style))
    }

    /// Grow by `amount` mm with round joins: every point within `amount` of
    /// the region. Mitred joins would regrow sharp tips far past that.
    pub fn outset(&self, amount: f64) -> Region {
        if amount <= 0.0 || self.is_empty() {
            return self.clone();
        }
        let style = OutlineStyle::new(amount).line_join(LineJoin::Round(ARC_STEP));
        Region::from_shapes(self.shapes.outline(&style))
    }

    /// Everything within `radius` of the strokes (round caps and joins).
    pub fn around_strokes(strokes: &Strokes, radius: f64) -> Region {
        let open: Vec<Vec<P>> = strokes.iter().filter(|s| s.len() >= 2).map(|s| to_p(s)).collect();
        if open.is_empty() || radius <= 0.0 {
            return Region::default();
        }
        let style = StrokeStyle::new(2.0 * radius)
            .line_join(LineJoin::Round(ARC_STEP))
            .start_cap(LineCap::Round(ARC_STEP))
            .end_cap(LineCap::Round(ARC_STEP));
        Region::from_shapes(open.stroke(style, false))
    }

    /// Everything in `self` or `other`.
    pub fn union(&self, other: &Region) -> Region {
        if other.is_empty() {
            return self.clone();
        }
        if self.is_empty() {
            return other.clone();
        }
        Region::from_shapes(self.shapes.overlay(&other.shapes, OverlayRule::Union, FillRule::NonZero))
    }

    /// `self` minus `other`.
    pub fn difference(&self, other: &Region) -> Region {
        if other.is_empty() || self.is_empty() {
            return self.clone();
        }
        Region::from_shapes(self.shapes.overlay(&other.shapes, OverlayRule::Difference, FillRule::NonZero))
    }

    /// The inside portions of segment `a`→`b`, one stroke per run, ends
    /// exactly on the boundary.
    pub fn clip_segment(&self, a: Pt, b: Pt) -> Strokes {
        self.cutter().polyline(&[a, b])
    }

    /// The inside portions of a polyline, kept joined where consecutive
    /// pieces meet.
    pub fn clip_polyline(&self, pts: &[Pt]) -> Strokes {
        self.cutter().polyline(pts)
    }

    /// Any drawing (mm polylines) cut to the region — for artwork that did
    /// not come from a fill, like an imported SVG.
    pub fn clip_strokes(&self, strokes: &Strokes) -> Strokes {
        let cutter = self.cutter();
        strokes.iter().flat_map(|s| cutter.polyline(s)).collect()
    }

    /// Common part of two regions.
    pub fn intersection(&self, other: &Region) -> Region {
        if other.is_empty() || self.is_empty() {
            return Region::default();
        }
        Region::from_shapes(self.shapes.overlay(&other.shapes, OverlayRule::Intersect, FillRule::NonZero))
    }

    /// Whether the (mm) point lies strictly inside the region, holes
    /// respected. Even-odd counting across all contours: entering an outer
    /// toggles on, entering a hole toggles off again. Points exactly on a
    /// boundary count as covered (the `-1e-6 slack` equivalent in tests).
    pub fn covers_point(&self, p: Pt) -> bool {
        let cutter = self.cutter();
        cutter.near_edge(p, ON_EDGE_MM) || cutter.inside(p)
    }

    /// Whether the straight segment `a`→`b` stays inside the region.
    ///
    /// Sampling the *midpoints* (never the endpoints) avoids the trap that
    /// broke the earlier `hop_ok`: `covers_point` on a point exactly on a
    /// boundary toggles even-odd and reads as *outside*, so every connector
    /// between two ring points — which is exactly what a serpentine hop is —
    /// was rejected on any shape that is not a rectangle.
    pub fn segment_inside(&self, a: Pt, b: Pt, samples: usize) -> bool {
        let n = samples.max(2);
        for k in 1..n {
            let t = k as f64 / n as f64;
            let p = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
            if !self.covers_point(p) {
                return false;
            }
        }
        true
    }

    /// Like [`Self::segment_inside`], but nudges sample points that land on the
    /// boundary toward the segment midpoint before rejecting. Serpentine hops
    /// between inset scanline ends sit on the contour; integer even-odd tests
    /// then read as outside even when the chord stays in the fill corridor.
    pub fn segment_inside_relaxed(&self, a: Pt, b: Pt, samples: usize, nudge_mm: f64) -> bool {
        let n = samples.max(2);
        let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        for k in 1..n {
            let t = k as f64 / n as f64;
            let mut p = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
            if self.covers_point(p) {
                continue;
            }
            let vx = mid.0 - p.0;
            let vy = mid.1 - p.1;
            let len = (vx * vx + vy * vy).sqrt();
            if len > 1e-12 {
                p = (
                    p.0 + nudge_mm * vx / len,
                    p.1 + nudge_mm * vy / len,
                );
            }
            if !self.covers_point(p) {
                return false;
            }
        }
        true
    }

    /// Distance from `p` to the nearest contour edge, in mm.
    pub fn dist_to_boundary(&self, p: Pt) -> f64 {
        self.cutter().distance(p)
    }

    /// Boundary route from `a` to `b` walking along the region contours,
    /// whichever way is shorter. `None` when either point is not near a
    /// contour. This is the connector that makes a serpentine survive on
    /// non-rectangular outlines: at an octagon or a blob, a straight hop
    /// between two scanline ends cuts the corner and leaves the shape, but
    /// the boundary is right there and hugs it legitimately.
    ///
    /// `snap` is how far a point may sit from a contour and still be routed.
    pub fn boundary_route(&self, a: Pt, b: Pt, snap: f64) -> Option<Vec<Pt>> {
        let mut best: Option<(f64, Vec<Pt>)> = None;

        for ring in self.all_rings_mm() {
            if ring.len() < 3 {
                continue;
            }
            // Cumulative arclength around the closed ring.
            let n = ring.len();
            let mut cum: Vec<f64> = Vec::with_capacity(n + 1);
            cum.push(0.0);
            let mut total = 0.0;
            for i in 0..n {
                let p = ring[i];
                let q = ring[(i + 1) % n];
                total += ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
                cum.push(total);
            }

            let (da, pa) = project_on_ring(&ring, &cum, a);
            let (db, pb) = project_on_ring(&ring, &cum, b);
            if dist(a, pa) > snap || dist(b, pb) > snap {
                continue;
            }

            // Arc from `a`'s projection to `b`'s projection. `walk_ring`
            // emits both projected endpoints plus every intermediate vertex
            // in between, so a short hop that lives inside one edge still
            // produces a usable (non-degenerate) route.
            let fwd = walk_ring(&ring, &cum, da, db);
            let bwd = walk_ring(&ring, &cum, db, da);
            let arc = if ring_len(&fwd) <= ring_len(&bwd) { fwd } else { bwd };

            // Stitch: a → (arc, whose endpoints are the projections) → b.
            let mut route: Vec<Pt> = Vec::new();
            if dist(a, pa) > 1e-9 {
                route.push(pa);
            }
            for p in arc {
                if route.last().is_none_or(|&q| dist(q, p) > 1e-9) {
                    route.push(p);
                }
            }
            route.push(b);

            let len = ring_len(&route);
            if best.as_ref().is_none_or(|(l, _)| len < *l) {
                best = Some((len, route));
            }
        }

        best.map(|(_, r)| r)
    }
}

fn dist(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn dist_point_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    let t = if len2 <= 1e-18 {
        0.0
    } else {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    let q = (a.0 + t * abx, a.1 + t * aby);
    dist(p, q)
}

fn ring_len(pts: &[Pt]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Nearest point on a closed ring and its arclength position.
fn project_on_ring(ring: &[Pt], cum: &[f64], p: Pt) -> (f64, Pt) {
    let n = ring.len();
    let mut best = (f64::MAX, 0.0, ring[0]);
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let (abx, aby) = (b.0 - a.0, b.1 - a.1);
        let len2 = abx * abx + aby * aby;
        let t = if len2 <= 1e-18 {
            0.0
        } else {
            (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
        };
        let q = (a.0 + t * abx, a.1 + t * aby);
        let d = dist(p, q);
        if d < best.0 {
            best = (d, cum[i] + t * (cum[i + 1] - cum[i]), q);
        }
    }
    (best.1, best.2)
}

/// Ring points along the contour from arclength `from` forward to `to`,
/// inclusive of both projected endpoints.
///
/// The old version walked only whole vertices, started counting from the
/// beginning of `from`'s edge (ignoring where inside that edge `from` lay)
/// and never emitted the endpoint `to`. A route that stayed within a single
/// edge — the common case for a 0.8 mm connector on a dense star — therefore
/// came back empty, and `boundary_route` collapsed to a bare straight line
/// between the two scanline ends: exactly the wall-crossing connector this
/// helper exists to prevent.
fn walk_ring(ring: &[Pt], cum: &[f64], from: f64, to: f64) -> Vec<Pt> {
    let n = ring.len();
    let total = *cum.last().unwrap();
    // Signed forward span; a full loop is `total`, so normalize into [0, total).
    let raw_span = to - from;
    let span = if raw_span >= 0.0 {
        raw_span
    } else {
        raw_span + total
    };
    if span <= 1e-12 {
        return vec![point_at(ring, cum, from)];
    }

    let mut out: Vec<Pt> = vec![point_at(ring, cum, from)];
    // First vertex strictly after `from`.
    let mut i = cum.partition_point(|&c| c < from).min(n);
    let mut travelled = cum[i % n] - from;
    if travelled < 0.0 {
        travelled += total;
    }
    while travelled < span - 1e-12 {
        let v = ring[i % n];
        if dist(*out.last().unwrap(), v) > 1e-9 {
            out.push(v);
        }
        // Advance to the next vertex.
        let next = (i + 1) % n;
        let seg = cum[next] - cum[i % n];
        travelled += if seg >= 0.0 { seg } else { seg + total };
        i = next;
        if out.len() > n + 2 {
            break; // full loop guard
        }
    }
    let end = point_at(ring, cum, to);
    if dist(*out.last().unwrap(), end) > 1e-9 {
        out.push(end);
    }
    out
}

/// Point on the ring at arclength position `s` (wraps around `cum.last()`).
fn point_at(ring: &[Pt], cum: &[f64], s: f64) -> Pt {
    let n = ring.len();
    let total = *cum.last().unwrap();
    let mut s = s % total;
    if s < 0.0 {
        s += total;
    }
    let i = cum.partition_point(|&c| c <= s).saturating_sub(1).min(n - 1);
    let seg = cum[i + 1] - cum[i];
    let t = if seg <= 1e-18 { 0.0 } else { (s - cum[i]) / seg };
    let a = ring[i];
    let b = ring[(i + 1) % n];
    (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
}

/// A region's edges bucketed on a grid, for cutting many polylines against
/// it: each segment only meets the edges near it, and inside/outside is
/// only asked again where the polyline actually crosses the boundary.
#[derive(Debug)]
pub struct Cutter {
    edges: Vec<(Pt, Pt)>,
    origin: Pt,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl Cutter {
    pub fn new(region: &Region) -> Cutter {
        let edges: Vec<(Pt, Pt)> = region
            .all_rings_mm()
            .iter()
            .filter(|r| r.len() >= 3)
            .flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
            .filter(|(a, b)| a != b)
            .collect();
        if edges.is_empty() {
            return Cutter { edges, origin: (0.0, 0.0), cell: 1.0, nx: 0, ny: 0, cells: vec![] };
        }
        let (minx, miny, maxx, maxy) = region.bounds_mm();
        let (w, h) = ((maxx - minx).max(1e-9), (maxy - miny).max(1e-9));
        let cell = (w * h / edges.len() as f64).sqrt().max(w.max(h) / 1024.0);
        let nx = ((w / cell).ceil() as usize).max(1);
        let ny = ((h / cell).ceil() as usize).max(1);
        let mut c = Cutter { edges, origin: (minx, miny), cell, nx, ny, cells: vec![Vec::new(); nx * ny] };
        for (k, &(a, b)) in c.edges.iter().enumerate() {
            let (i0, j0) = c.index((a.0.min(b.0), a.1.min(b.1)));
            let (i1, j1) = c.index((a.0.max(b.0), a.1.max(b.1)));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    c.cells[j * nx + i].push(k as u32);
                }
            }
        }
        c
    }

    fn index(&self, p: Pt) -> (usize, usize) {
        let i = ((p.0 - self.origin.0) / self.cell).floor().clamp(0.0, (self.nx - 1) as f64) as usize;
        let j = ((p.1 - self.origin.1) / self.cell).floor().clamp(0.0, (self.ny - 1) as f64) as usize;
        (i, j)
    }

    /// Even-odd ray cast to +x over the cells right of `p`. A crossing is
    /// counted in the one cell it falls in, so edges spanning several cells
    /// count once.
    pub fn inside(&self, p: Pt) -> bool {
        if self.cells.is_empty() {
            return false;
        }
        let (i0, j) = self.index(p);
        if p.1 < self.origin.1 || p.1 > self.origin.1 + self.ny as f64 * self.cell {
            return false;
        }
        let mut inside = false;
        for i in i0..self.nx {
            for &k in &self.cells[j * self.nx + i] {
                let (a, b) = self.edges[k as usize];
                if (a.1 > p.1) == (b.1 > p.1) {
                    continue;
                }
                let x = a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0);
                if x > p.0 && self.index((x, p.1)).0 == i {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Whether some edge passes within `eps` of `p`.
    pub fn near_edge(&self, p: Pt, eps: f64) -> bool {
        if self.cells.is_empty() {
            return false;
        }
        let (i0, j0) = self.index((p.0 - eps, p.1 - eps));
        let (i1, j1) = self.index((p.0 + eps, p.1 + eps));
        (j0..=j1).any(|j| {
            (i0..=i1).any(|i| {
                self.cells[j * self.nx + i].iter().any(|&k| {
                    let (a, b) = self.edges[k as usize];
                    dist_point_segment(p, a, b) <= eps
                })
            })
        })
    }

    /// Distance to the nearest edge, searching outward ring by ring of cells
    /// and stopping once no farther cell could hold anything closer.
    pub fn distance(&self, p: Pt) -> f64 {
        let mut best = f64::MAX;
        if self.cells.is_empty() {
            return best;
        }
        let (ci, cj) = self.index(p);
        let (ci, cj) = (ci as i64, cj as i64);
        let reach = self.nx.max(self.ny) as i64;
        for r in 0..=reach {
            for j in (cj - r)..=(cj + r) {
                if j < 0 || j >= self.ny as i64 {
                    continue;
                }
                let on_edge_row = j == cj - r || j == cj + r;
                let step = if on_edge_row || r == 0 { 1 } else { 2 * r as usize };
                let mut i = ci - r;
                while i <= ci + r {
                    if i >= 0 && i < self.nx as i64 {
                        for &k in &self.cells[j as usize * self.nx + i as usize] {
                            let (a, b) = self.edges[k as usize];
                            best = best.min(dist_point_segment(p, a, b));
                        }
                    }
                    i += step as i64;
                }
            }
            if best <= r as f64 * self.cell {
                break;
            }
        }
        best
    }

    /// Where `a`→`b` meets an edge, as fractions along it (unsorted,
    /// possibly repeated at vertices).
    fn crossings(&self, a: Pt, b: Pt, out: &mut Vec<f64>) {
        if self.cells.is_empty() {
            return;
        }
        let d = (b.0 - a.0, b.1 - a.1);
        let (i0, j0) = self.index((a.0.min(b.0), a.1.min(b.1)));
        let (i1, j1) = self.index((a.0.max(b.0), a.1.max(b.1)));
        for j in j0..=j1 {
            for i in i0..=i1 {
                for &k in &self.cells[j * self.nx + i] {
                    let (p, q) = self.edges[k as usize];
                    let e = (q.0 - p.0, q.1 - p.1);
                    let den = d.0 * e.1 - d.1 * e.0;
                    if den.abs() < 1e-18 {
                        continue;
                    }
                    let ap = (p.0 - a.0, p.1 - a.1);
                    let t = (ap.0 * e.1 - ap.1 * e.0) / den;
                    let u = (ap.0 * d.1 - ap.1 * d.0) / den;
                    if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    if self.index((a.0 + t * d.0, a.1 + t * d.1)) == (i, j) {
                        out.push(t);
                    }
                }
            }
        }
    }

    /// The inside runs of a polyline, joined where they continue.
    pub fn polyline(&self, pts: &[Pt]) -> Strokes {
        let mut out: Strokes = Vec::new();
        if pts.len() < 2 {
            return out;
        }
        let lerp = |a: Pt, b: Pt, t: f64| match t {
            0.0 => a,
            1.0 => b,
            _ => (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)),
        };
        let mut current: Vec<Pt> = Vec::new();
        let mut state: Option<bool> = None;
        let mut ts: Vec<f64> = Vec::new();
        let flush = |current: &mut Vec<Pt>, out: &mut Strokes| {
            if current.len() >= 2 && polyline_length(current) >= MIN_SEGMENT_MM {
                out.push(std::mem::take(current));
            }
            current.clear();
        };
        let mut touched_end = false;
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            ts.clear();
            self.crossings(a, b, &mut ts);
            let recheck = touched_end || ts.iter().any(|&t| t <= 1e-12);
            touched_end = ts.iter().any(|&t| t >= 1.0 - 1e-12);
            ts.retain(|&t| t > 1e-12 && t < 1.0 - 1e-12);
            ts.sort_by(|x, y| x.total_cmp(y));
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
            if state.is_none() || recheck || !ts.is_empty() {
                let end = ts.first().copied().unwrap_or(1.0);
                state = Some(self.inside(lerp(a, b, end * 0.5)));
            }
            let mut t0 = 0.0;
            for k in 0..=ts.len() {
                let t1 = ts.get(k).copied().unwrap_or(1.0);
                if k > 0 {
                    state = Some(self.inside(lerp(a, b, (t0 + t1) * 0.5)));
                }
                if state == Some(true) {
                    let p = lerp(a, b, t0);
                    if current.last().is_none_or(|&l| dist(l, p) > 1e-9) {
                        flush(&mut current, &mut out);
                        current.push(p);
                    }
                    current.push(lerp(a, b, t1));
                } else {
                    flush(&mut current, &mut out);
                }
                t0 = t1;
            }
        }
        flush(&mut current, &mut out);
        out
    }
}

/// Length of an open polyline in millimetres.
pub fn polyline_length(pts: &[Pt]) -> f64 {
    pts.windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum()
}

/// Strokes shorter than this are not ink: rounding artefacts the consumer
/// (vpype) would filter anyway. Keeps drawn-length numbers meaningful.
pub const MIN_SEGMENT_MM: f64 = 1e-6;

/// A run of points as a stroke, dropping degenerate runs — `segment()`.
pub fn segment(pts: Vec<Pt>) -> Option<Vec<Pt>> {
    if pts.len() < 2 {
        return None;
    }
    if polyline_length(&pts) < MIN_SEGMENT_MM {
        return None;
    }
    Some(pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn within(inner: (f64, f64, f64, f64), outer: (f64, f64, f64, f64)) -> bool {
        inner.0 >= outer.0 - 1e-6 && inner.1 >= outer.1 - 1e-6 && inner.2 <= outer.2 + 1e-6 && inner.3 <= outer.3 + 1e-6
    }

    fn area(region: &Region) -> f64 {
        region.all_rings_mm().iter().map(|r| crate::concentric::signed_area(r)).sum()
    }

    /// Slivers left by a polygon overlay (sub-micron wide tips) inset to
    /// nothing or to a piece of themselves (Clipper2 threw them hundreds of
    /// metres off the page).
    #[test]
    fn inset_of_slivers_stays_inside() {
        let slivers: [&[Pt]; 3] = [
            &[(146.58488280366294, 39.316112387203646), (146.56190950463645, 39.209675180935335), (146.58500463555686, 39.31608687641568)],
            &[(135.7255242545449, 18.31666647198148), (134.50059706757895, 18.8051334500546), (135.72358853410117, 18.311978209042024)],
            &[
                (76.0451510627114, 21.86720668079801),
                (74.35978419373862, 20.22001491787381),
                (74.64253933022849, 19.937734234356356),
                (76.32506830285422, 21.542765486263704),
                (76.58200461457602, 21.269442427181673),
                (76.32739026139609, 21.54029594662137),
                (76.32285101006858, 21.545339930080843),
                (76.05223471711508, 21.859519112133455),
            ],
        ];
        for ring in slivers {
            let region = Region::from_ring(ring);
            for amount in [0.01, 0.075, 0.4] {
                let inset = region.inset(amount);
                assert!(inset.is_empty() || within(inset.bounds_mm(), region.bounds_mm()), "{amount}: {:?}", inset.bounds_mm());
            }
        }
    }

    /// A long 2° wedge loses its thin end to the inset but keeps its wide
    /// part. A 2° crack into a square is kept open, its tip mitred at most
    /// 5× the inset further in, never carried across the square.
    #[test]
    fn inset_of_sharp_tips() {
        let (h, amount) = (100.0 * 1f64.to_radians().tan(), 0.5);
        let wedge = Region::from_ring(&[(0.0, 0.0), (100.0, -h), (100.0, h)]);
        let inset = wedge.inset(amount);
        assert!(within(inset.bounds_mm(), wedge.bounds_mm()), "{:?}", inset.bounds_mm());
        let apex = amount / 1f64.to_radians().sin();
        let side = 100.0 - amount - apex;
        let exact = side * side * 1f64.to_radians().tan();
        assert!((area(&inset) - exact).abs() < 0.02 * exact, "{} vs {exact}", area(&inset));
        assert!(!inset.covers_point((apex - 1.0, 0.0)) && inset.covers_point((apex + 5.0, 0.0)));

        let crack = Region::from_ring(&[(0.0, 0.0), (50.0, 0.0), (50.0, 50.0), (0.0, 50.0), (0.0, 25.0 + h / 4.0), (25.0, 25.0), (0.0, 25.0 - h / 4.0)]);
        let inset = crack.inset(amount);
        assert!(within(inset.bounds_mm(), crack.bounds_mm()), "{:?}", inset.bounds_mm());
        assert!(!inset.covers_point((24.8, 25.0)));
        assert!(inset.covers_point((25.0 + 5.0 * amount + 0.1, 25.0)) && inset.covers_point((45.0, 25.0)));
    }

    /// A route between two nearby points that both sit on the same straight
    /// contour edge must return a real arc, not a degenerate single point.
    /// (Regression: `walk_ring` used to count travel from the start of
    /// `from`'s edge and never emit the endpoint, so any sub-edge route
    /// collapsed to `[b]` — a bare straight connector, which is precisely
    /// the wall-crossing hop the route exists to avoid.)
    #[test]
    fn boundary_route_inside_one_edge_has_length() {
        let region = Region::from_ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let a = (4.0, 0.0);
        let b = (6.0, 0.0);
        let route = region.boundary_route(a, b, 1.0).expect("route");
        assert!(
            polyline_length(&route) > 0.5,
            "degenerate route {route:?} (len {})",
            polyline_length(&route)
        );
        assert!(polyline_length(&route) <= 2.5, "route detoured: {route:?}");
    }

    /// A short hop within one edge round a notched outline: the route must
    /// stay on the contour, never cut straight across.
    #[test]
    fn boundary_route_follows_contour_not_chord() {
        // L-shaped polygon; both points sit on the same long edge.
        let region = Region::from_ring(&[
            (0.0, 0.0),
            (20.0, 0.0),
            (20.0, 8.0),
            (8.0, 8.0),
            (8.0, 20.0),
            (0.0, 20.0),
        ]);
        let a = (2.0, 0.0);
        let b = (18.0, 0.0);
        let route = region.boundary_route(a, b, 1.0).expect("route");
        // Straight-line distance is 16; the contour route along the bottom
        // edge is the same 16, so it must not be longer than the chord by
        // more than a hair.
        let len = polyline_length(&route);
        assert!((15.9..=16.2).contains(&len), "route len {len}: {route:?}");
    }

    /// The grid cutter keeps exactly the inside length (measured by dense
    /// sampling), on a star with a hole and on polylines that run through
    /// its vertices.
    #[test]
    fn cutter_matches_the_sampling_clip() {
        let star: Vec<Pt> = (0..20)
            .map(|i| {
                let a = i as f64 * std::f64::consts::PI / 10.0;
                let r = if i % 2 == 0 { 40.0 } else { 15.0 };
                (50.0 + r * a.cos(), 50.0 + r * a.sin())
            })
            .collect();
        let hole = vec![(45.0, 45.0), (55.0, 45.0), (55.0, 55.0), (45.0, 55.0)];
        let region = Region::from_polygons(&[vec![star.clone(), hole]]);
        let mut rng = crate::random::Rng::new(3);
        let mut lines: Strokes = (0..300)
            .map(|_| (0..6).map(|_| (rng.range(0.0, 100.0), rng.range(0.0, 100.0))).collect())
            .collect();
        lines.extend(star.iter().map(|&(x, y)| vec![(50.0, 50.0), (x, y), (50.0 + 1.5 * (x - 50.0), 50.0 + 1.5 * (y - 50.0))]));
        for &v in &star {
            lines.push(vec![(rng.range(0.0, 100.0), rng.range(0.0, 100.0)), v, (rng.range(0.0, 100.0), rng.range(0.0, 100.0))]);
        }
        lines.push(vec![(0.0, 50.0), (100.0, 50.0)]);
        let cutter = Cutter::new(&region);
        let len = |s: &Strokes| s.iter().map(|p| polyline_length(p)).sum::<f64>();
        let sampled = |line: &[Pt]| -> f64 {
            line.windows(2)
                .map(|w| {
                    const N: usize = 4_000;
                    let hits = (0..N)
                        .filter(|&k| {
                            let t = (k as f64 + 0.5) / N as f64;
                            region.covers_point((w[0].0 + t * (w[1].0 - w[0].0), w[0].1 + t * (w[1].1 - w[0].1)))
                        })
                        .count();
                    dist(w[0], w[1]) * hits as f64 / N as f64
                })
                .sum()
        };
        for line in &lines {
            let new = cutter.polyline(line);
            let truth = sampled(line);
            assert!((len(&new) - truth).abs() < 0.1, "{} vs {truth} for {line:?}", len(&new));
            for s in &new {
                for w in s.windows(2) {
                    let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
                    assert!(region.covers_point(mid), "{mid:?}");
                }
            }
        }
        assert!(cutter.inside((80.0, 50.0)) && !cutter.inside((50.0, 20.0)) && !cutter.inside((50.0, 50.0)));
        assert!(!cutter.inside((0.0, 0.0)) && !cutter.inside((200.0, 50.0)));

        let brute = |p: Pt| {
            region
                .all_rings_mm()
                .iter()
                .flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()])))
                .map(|(a, b)| dist_point_segment(p, a, b))
                .fold(f64::MAX, f64::min)
        };
        for _ in 0..2000 {
            let p = (rng.range(-50.0, 150.0), rng.range(-50.0, 150.0));
            assert_eq!(cutter.distance(p), brute(p), "{p:?}");
        }
    }
}
