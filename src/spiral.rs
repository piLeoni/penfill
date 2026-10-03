//! Spiral: one path per lobe winding inward, lap after lap, a spacing
//! apart.
//!
//! The path follows a stack of inset contours eight to a spacing, the first
//! half a spacing in from the wall (holes filled in), leaving out contours
//! too thin for a loop. A lap runs an eighth of the way round one contour
//! while easing over to the next, and so on through eight of them: after one
//! turn it has come in by one spacing. Because it always runs between two
//! neighbouring inset contours, its distance from the wall grows the way the
//! insets do, tips pulling in faster than sides, and it never cuts straight
//! across a spike. Contours of different depth never meet, so laps a
//! spacing apart never cross.
//!
//! The contours form a tree: where an inset splits into several pieces,
//! each lobe gets its own spiral, starting a full spacing inside the end of
//! its parent's, so the two never run side by side. When the first lap
//! would drag the outer edge far inward (a sharp tip), the first contour is
//! traced once as a ring and the spiral starts a spacing inside it.
//!
//! The paths are then cut to the region (holes are only respected here),
//! knots near sharp tips are cut out where the path crosses itself, thin
//! leftovers get centrelines, and paths whose ends are close are joined.

use crate::concentric::{gap_fill, signed_area, solid_outers};
use crate::geo::{polyline_length, Pt, Region, Strokes, MIN_SEGMENT_MM};

/// Inset contours per spacing.
const STEPS: usize = 8;

/// A knot whose loop is at most this many spacings long is cut out; a
/// longer one is kept as a path of its own.
const KNOT_TIMES_SPACING: f64 = 3.0;

/// Spiral paths, then centrelines for thin leftovers, one piece of the
/// region after another.
pub fn fill_spiral(region: &Region, spacing: f64) -> Strokes {
    if region.is_empty() || spacing <= 0.0 {
        return vec![];
    }
    region.parts().iter().flat_map(|piece| piece_spiral(piece, spacing)).collect()
}

/// Projection of `p` on segment `a`–`b`: the parameter along it (0 to 1,
/// clamped) and the distance from `p` to that point.
fn project(p: Pt, a: Pt, b: Pt) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-18 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    (t, dist(p, (a.0 + t * dx, a.1 + t * dy)))
}

fn dist(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
}

/// Appends `points`, skipping any that would not move the pen.
fn extend(path: &mut Vec<Pt>, points: impl IntoIterator<Item = Pt>) {
    for p in points {
        if path.last().is_none_or(|&q| dist(p, q) >= MIN_SEGMENT_MM) {
            path.push(p);
        }
    }
}

fn piece_spiral(piece: &Region, spacing: f64) -> Strokes {
    let tree = Tree::build(piece, spacing);
    let paths = tree.wind(spacing);
    let mut paths = contain(paths, piece, spacing);
    untangle(&mut paths, spacing);
    let gaps = gap_fill(piece, &paths, spacing, spacing / 2.0);
    paths.extend(gaps);
    untangle(&mut paths, spacing);
    join(paths, piece, spacing)
}

/// A closed inset contour, counter-clockwise, walked by arclength.
struct Contour {
    pts: Vec<Pt>,
    /// Arclength at each point, and the whole length at the end.
    cum: Vec<f64>,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl Contour {
    fn new(pts: Vec<Pt>) -> Contour {
        let n = pts.len();
        let mut cum = Vec::with_capacity(n + 1);
        cum.push(0.0);
        for i in 0..n {
            cum.push(cum[i] + dist(pts[i], pts[(i + 1) % n]));
        }
        Contour { pts, cum, parent: None, children: Vec::new() }
    }

    fn len(&self) -> f64 {
        *self.cum.last().unwrap()
    }

    fn wrap(&self, s: f64) -> f64 {
        s.rem_euclid(self.len())
    }

    /// The edge holding arclength `s` (already wrapped).
    fn edge(&self, s: f64) -> usize {
        self.cum.partition_point(|&c| c <= s).saturating_sub(1).min(self.pts.len() - 1)
    }

    fn at(&self, s: f64) -> Pt {
        let s = self.wrap(s);
        let i = self.edge(s);
        let n = self.pts.len();
        let seg = self.cum[i + 1] - self.cum[i];
        let t = if seg <= 1e-18 { 0.0 } else { (s - self.cum[i]) / seg };
        lerp(self.pts[i], self.pts[(i + 1) % n], t)
    }

    /// Arclength of the nearest point to `p`, and its distance.
    fn nearest(&self, p: Pt) -> (f64, f64) {
        let n = self.pts.len();
        let mut best = (0.0, f64::MAX);
        for i in 0..n {
            let (t, d) = project(p, self.pts[i], self.pts[(i + 1) % n]);
            if d < best.1 {
                best = (self.cum[i] + t * (self.cum[i + 1] - self.cum[i]), d);
            }
        }
        best
    }

    /// The points met walking `length` forward from arclength `from`: the
    /// corners passed, then the end point. The start point is not included.
    fn walk(&self, from: f64, length: f64) -> Vec<Pt> {
        let n = self.pts.len();
        let from = self.wrap(from);
        let mut out = Vec::new();
        let mut k = self.cum.partition_point(|&c| c <= from);
        let mut travelled = self.cum[k.min(n)] - from;
        while travelled < length - 1e-9 && out.len() <= 2 * n {
            out.push(self.pts[k % n]);
            let e = k % n;
            travelled += self.cum[e + 1] - self.cum[e];
            k = e + 1;
        }
        out.push(self.at(from + length));
        out
    }

    /// How far along the corners lie, walking `length` forward from
    /// arclength `from`, as fractions of `length` strictly between 0 and 1.
    fn corners(&self, from: f64, length: f64) -> Vec<f64> {
        let n = self.pts.len();
        let mut out = Vec::new();
        if length <= 1e-12 {
            return out;
        }
        let from = self.wrap(from);
        let mut k = self.cum.partition_point(|&c| c <= from);
        let mut travelled = self.cum[k.min(n)] - from;
        while travelled < length - 1e-9 && out.len() <= n {
            out.push(travelled / length);
            let e = k % n;
            travelled += self.cum[e + 1] - self.cum[e];
            k = e + 1;
        }
        out
    }

    fn contains(&self, p: Pt) -> bool {
        let n = self.pts.len();
        let mut inside = false;
        for i in 0..n {
            let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
            if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                inside = !inside;
            }
        }
        inside
    }

    /// Distance from `p` to the contour.
    fn distance(&self, p: Pt) -> f64 {
        self.nearest(p).1
    }

    /// The whole contour as a closed stroke from arclength `start`.
    fn ring(&self, start: f64) -> Vec<Pt> {
        let mut ring = vec![self.at(start)];
        extend(&mut ring, self.walk(start, self.len()));
        ring
    }
}

/// The inset contours of one piece, level by level, linked parent to child.
struct Tree {
    levels: Vec<Vec<Contour>>,
}

impl Tree {
    /// Contours at `spacing / 2 + i · spacing / 8` for level `i`. A contour
    /// that would vanish when inset by a further quarter spacing (it has no
    /// contour two levels in) is thinner than half a spacing and left out.
    fn build(piece: &Region, spacing: f64) -> Tree {
        let solid = solid_outers(piece);
        let step = spacing / STEPS as f64;
        let mut raw: Vec<Vec<Contour>> = Vec::new();
        loop {
            let offset = solid.inset(spacing / 2.0 + raw.len() as f64 * step);
            let contours: Vec<Contour> = offset
                .all_rings_mm()
                .into_iter()
                .filter(|r| r.len() >= 3 && signed_area(r) > 0.0)
                .map(Contour::new)
                .collect();
            if contours.is_empty() {
                break;
            }
            raw.push(contours);
        }
        for i in 1..raw.len() {
            let (outer, inner) = raw.split_at_mut(i);
            let outer = &mut outer[i - 1];
            for (k, c) in inner[0].iter_mut().enumerate() {
                if let Some(p) = outer.iter().position(|o| o.contains(c.pts[0])) {
                    c.parent = Some(p);
                    outer[p].children.push(k);
                }
            }
        }

        // Keep the contours with grandchildren, renumbering as we go.
        let keep: Vec<Vec<bool>> = (0..raw.len())
            .map(|i| {
                raw[i]
                    .iter()
                    .map(|c| c.children.iter().any(|&k| i + 1 < raw.len() && !raw[i + 1][k].children.is_empty()))
                    .collect()
            })
            .collect();
        let index: Vec<Vec<Option<usize>>> = keep
            .iter()
            .map(|flags| {
                let mut next = 0..;
                flags.iter().map(|&k| if k { next.next() } else { None }).collect()
            })
            .collect();
        let mut levels: Vec<Vec<Contour>> = Vec::new();
        for (i, contours) in raw.into_iter().enumerate() {
            let mut kept = Vec::new();
            for (k, mut c) in contours.into_iter().enumerate() {
                if !keep[i][k] {
                    continue;
                }
                c.parent = c.parent.and_then(|p| if i > 0 { index[i - 1][p] } else { None });
                c.children = c.children.iter().filter_map(|&ch| index.get(i + 1).and_then(|l| l[ch])).collect();
                kept.push(c);
            }
            if kept.is_empty() {
                break;
            }
            levels.push(kept);
        }
        Tree { levels }
    }

    /// The contours `depth` levels below contour `k` of level `level`.
    fn descendants(&self, level: usize, k: usize, depth: usize) -> Vec<usize> {
        if level + depth >= self.levels.len() {
            return vec![];
        }
        let mut found = vec![k];
        for d in 0..depth {
            found = found.iter().flat_map(|&c| self.levels[level + d][c].children.iter().copied()).collect();
        }
        found
    }

    /// Where on contour `k` of `level` to start: the point closest to the
    /// contours a spacing further in, so the step inward is short.
    fn start(&self, level: usize, k: usize) -> f64 {
        let c = &self.levels[level][k];
        let inner = self.descendants(level, k, STEPS);
        if inner.is_empty() {
            return 0.0;
        }
        let n = c.pts.len();
        let mut best = (0.0, f64::MAX);
        for i in 0..n {
            for s in [c.cum[i], (c.cum[i] + c.cum[i + 1]) / 2.0] {
                let p = c.at(s);
                let d = inner.iter().map(|&j| self.levels[level + STEPS][j].distance(p)).fold(f64::MAX, f64::min);
                if d < best.1 - 1e-9 {
                    best = (s, d);
                }
            }
        }
        best.0
    }

    /// Every spiral and ring of the piece, outside in.
    fn wind(&self, spacing: f64) -> Strokes {
        let mut paths: Strokes = Vec::new();
        // (level, contour, a point to start near)
        let mut jobs: Vec<(usize, usize, Option<Pt>)> = Vec::new();
        for (level, contours) in self.levels.iter().enumerate() {
            for (k, c) in contours.iter().enumerate() {
                if c.parent.is_some() {
                    continue;
                }
                let inner = self.descendants(level, k, STEPS);
                let sharp = !inner.is_empty()
                    && c.pts.iter().any(|&p| {
                        inner.iter().map(|&j| self.levels[level + STEPS][j].distance(p)).fold(f64::MAX, f64::min)
                            > 1.5 * spacing
                    });
                if sharp {
                    let ring = c.ring(self.start(level, k));
                    jobs.extend(inner.iter().map(|&j| (level + STEPS, j, Some(ring[0]))));
                    paths.push(ring);
                } else {
                    jobs.push((level, k, None));
                }
            }
        }
        let mut next = 0;
        while next < jobs.len() {
            let (level, k, near) = jobs[next];
            next += 1;
            let mut chain = vec![k];
            while let [only] = self.levels[level + chain.len() - 1][*chain.last().unwrap()].children[..] {
                chain.push(only);
            }
            let first = &self.levels[level][k];
            let start = match near {
                Some(p) => first.nearest(p).0,
                None => self.start(level, k),
            };
            let path = if chain.len() < STEPS {
                first.ring(start)
            } else {
                self.spiral(level, &chain, start)
            };
            let last = level + chain.len() - 1;
            let end = *path.last().unwrap();
            let split = &self.levels[last][*chain.last().unwrap()];
            if split.children.len() > 1 {
                jobs.extend(self.descendants(last, *chain.last().unwrap(), STEPS).into_iter().map(|j| (last + STEPS, j, Some(end))));
            }
            paths.push(path);
        }
        paths
    }

    /// One spiral down the chain of contours starting at `level`.
    ///
    /// Each contour is followed for an eighth of its length while the path
    /// eases over to the next contour: the stretch of the next contour
    /// between the nearest points to the two ends is matched by arclength,
    /// and the path blends from the one to the other along the way.
    fn spiral(&self, level: usize, chain: &[usize], start: f64) -> Vec<Pt> {
        let contour = |j: usize| &self.levels[level + j][chain[j]];
        let mut pos = start;
        let mut path = vec![contour(0).at(pos)];
        for j in 0..chain.len() {
            let c = contour(j);
            let step = c.len() / STEPS as f64;
            if j + 1 == chain.len() {
                extend(&mut path, c.walk(pos, step));
                break;
            }
            let next = contour(j + 1);
            let from = next.nearest(c.at(pos)).0;
            let to = next.nearest(c.at(pos + step)).0;
            let mut span = (to - from).rem_euclid(next.len());
            if span > 0.5 * next.len() {
                span = 0.0;
            }
            let mut ts: Vec<f64> = c.corners(pos, step);
            ts.extend(next.corners(from, span));
            ts.extend([0.25, 0.5, 0.75, 1.0]);
            ts.sort_by(|a, b| a.total_cmp(b));
            ts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
            extend(&mut path, ts.into_iter().map(|t| lerp(c.at(pos + t * step), next.at(from + t * span), t)));
            pos = from + span;
        }
        path
    }
}

/// The paths cut to the piece: holes kept clear by half a spacing, and a
/// path split wherever a step would leave the piece.
fn contain(paths: Strokes, piece: &Region, spacing: f64) -> Strokes {
    let holes: Vec<Vec<Vec<Pt>>> =
        piece.all_rings_mm().into_iter().filter(|r| r.len() >= 3 && signed_area(r) < 0.0).map(|r| vec![r]).collect();
    let keep = (!holes.is_empty()).then(|| piece.difference(&Region::from_polygons(&holes).outset(spacing / 2.0)));
    let mut out = Vec::new();
    for path in paths {
        let pieces = match &keep {
            Some(k) => k.clip_polyline(&path),
            None => vec![path],
        };
        for p in pieces {
            let mut run: Vec<Pt> = vec![p[0]];
            for w in p.windows(2) {
                if piece.segment_inside(w[0], w[1], 8) {
                    run.push(w[1]);
                    continue;
                }
                if run.len() >= 2 && polyline_length(&run) >= 0.3 * spacing {
                    out.push(std::mem::take(&mut run));
                }
                run = vec![w[1]];
            }
            if run.len() >= 2 && polyline_length(&run) >= 0.3 * spacing {
                out.push(run);
            }
        }
    }
    out
}

/// Segments bucketed on a grid, for finding the ones near a segment.
struct Grid {
    origin: Pt,
    cell: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<usize>>,
}

impl Grid {
    fn new(points: impl Iterator<Item = Pt>, cell: f64) -> Grid {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in points {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        if x0 > x1 {
            return Grid { origin: (0.0, 0.0), cell: 1.0, nx: 1, ny: 1, cells: vec![Vec::new()] };
        }
        let cell = cell.max((x1 - x0).max(y1 - y0) / 512.0).max(1e-6);
        let nx = ((x1 - x0) / cell).floor() as usize + 1;
        let ny = ((y1 - y0) / cell).floor() as usize + 1;
        Grid { origin: (x0, y0), cell, nx, ny, cells: vec![Vec::new(); nx * ny] }
    }

    fn index(&self, p: Pt) -> (usize, usize) {
        let i = ((p.0 - self.origin.0) / self.cell).floor().clamp(0.0, (self.nx - 1) as f64) as usize;
        let j = ((p.1 - self.origin.1) / self.cell).floor().clamp(0.0, (self.ny - 1) as f64) as usize;
        (i, j)
    }

    fn cells_of(&self, a: Pt, b: Pt) -> impl Iterator<Item = usize> + '_ {
        let (i0, j0) = self.index((a.0.min(b.0), a.1.min(b.1)));
        let (i1, j1) = self.index((a.0.max(b.0), a.1.max(b.1)));
        (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| j * self.nx + i))
    }

    fn insert(&mut self, id: usize, a: Pt, b: Pt) {
        let cells: Vec<usize> = self.cells_of(a, b).collect();
        for c in cells {
            self.cells[c].push(id);
        }
    }

    /// Ids stored in the cells `a`–`b` passes over, each once.
    fn near(&self, a: Pt, b: Pt) -> Vec<usize> {
        let mut ids: Vec<usize> = self.cells_of(a, b).flat_map(|c| self.cells[c].iter().copied()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

/// Where `a`–`b` and `c`–`d` cut through each other, as the parameter along
/// `a`–`b`; `None` if they only touch, meet at an end or do not meet.
fn crossing(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<f64> {
    let side = |p: Pt, q: Pt, r: Pt| {
        let len = dist(p, q);
        if len <= 1e-12 {
            0.0
        } else {
            ((q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)) / len
        }
    };
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    let clear = |x: f64| x.abs() > 1e-9;
    if !(clear(d1) && clear(d2) && clear(d3) && clear(d4)) || d1.signum() == d2.signum() || d3.signum() == d4.signum() {
        return None;
    }
    Some(d1 / (d1 - d2))
}

/// A segment of a path: (path, index of its first point).
type SegmentAt = (usize, usize);

/// The first place where two segments of the paths cross: each segment, the
/// earlier first, and the parameter along the earlier one.
fn first_crossing(paths: &Strokes, spacing: f64) -> Option<(SegmentAt, SegmentAt, f64)> {
    let segs: Vec<(usize, usize)> =
        paths.iter().enumerate().flat_map(|(p, path)| (0..path.len().saturating_sub(1)).map(move |s| (p, s))).collect();
    let seg = |id: usize| {
        let (p, s) = segs[id];
        (paths[p][s], paths[p][s + 1])
    };
    let mut grid = Grid::new(paths.iter().flatten().copied(), 2.0 * spacing);
    for id in 0..segs.len() {
        let (a, b) = seg(id);
        grid.insert(id, a, b);
    }
    for id in 0..segs.len() {
        let (a, b) = seg(id);
        for other in grid.near(a, b) {
            if other <= id || (segs[other].0 == segs[id].0 && segs[other].1 == segs[id].1 + 1) {
                continue;
            }
            let (c, d) = seg(other);
            if let Some(t) = crossing(a, b, c, d) {
                return Some((segs[id], segs[other], t));
            }
        }
    }
    None
}

/// Removes every crossing. A small knot in one path is cut out at the
/// crossing; a large loop becomes a path of its own; where two paths cross,
/// the shorter one is broken there with a small gap.
fn untangle(paths: &mut Strokes, spacing: f64) {
    for _ in 0..10_000 {
        let Some(((p, s), (q, r), t)) = first_crossing(paths, spacing) else {
            return;
        };
        let x = lerp(paths[p][s], paths[p][s + 1], t);
        if p == q {
            let path = std::mem::take(&mut paths[p]);
            let mut lap = vec![x];
            extend(&mut lap, path[s + 1..=r].iter().copied().chain([x]));
            let mut main = path[..=s].to_vec();
            extend(&mut main, std::iter::once(x).chain(path[r + 1..].iter().copied()));
            paths[p] = main;
            if polyline_length(&lap) > KNOT_TIMES_SPACING * spacing && lap.len() >= 3 {
                paths.push(lap);
            }
        } else {
            let (k, e, y) = if polyline_length(&paths[p]) <= polyline_length(&paths[q]) {
                (p, s, x)
            } else {
                (q, r, x)
            };
            let path = std::mem::take(&mut paths[k]);
            let (a, b) = (path[e], path[e + 1]);
            let gap = (0.25 * spacing).min(0.45 * dist(a, y)).min(0.45 * dist(y, b));
            let len = dist(a, b).max(1e-12);
            let dir = ((b.0 - a.0) / len, (b.1 - a.1) / len);
            let mut head = path[..=e].to_vec();
            extend(&mut head, [(y.0 - gap * dir.0, y.1 - gap * dir.1)]);
            let mut tail = vec![(y.0 + gap * dir.0, y.1 + gap * dir.1)];
            extend(&mut tail, path[e + 1..].iter().copied());
            paths[k] = head;
            paths.push(tail);
        }
        paths.retain(|path| path.len() >= 2 && polyline_length(path) >= MIN_SEGMENT_MM);
    }
}

/// Joins paths whose ends lie within one and a half spacings, closest pairs
/// first, where the joining segment stays inside the piece and crosses no
/// drawn segment.
fn join(paths: Strokes, piece: &Region, spacing: f64) -> Strokes {
    let n = paths.len();
    if n < 2 {
        return paths;
    }
    let reach = 1.5 * spacing;
    let end = |e: usize| if e.is_multiple_of(2) { paths[e / 2][0] } else { *paths[e / 2].last().unwrap() };

    let mut ends = Grid::new((0..2 * n).map(end), reach);
    for e in 0..2 * n {
        ends.insert(e, end(e), end(e));
    }
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for e in 0..2 * n {
        let p = end(e);
        for f in ends.near((p.0 - reach, p.1 - reach), (p.0 + reach, p.1 + reach)) {
            if f / 2 > e / 2 {
                let d = dist(p, end(f));
                if d <= reach {
                    pairs.push((d, e, f));
                }
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    let mut drawn: Vec<(Pt, Pt)> =
        paths.iter().flat_map(|p| p.windows(2).map(|w| (w[0], w[1]))).collect();
    let mut grid = Grid::new(paths.iter().flatten().copied(), 2.0 * spacing);
    for (id, &(a, b)) in drawn.iter().enumerate() {
        grid.insert(id, a, b);
    }
    let mut group: Vec<usize> = (0..n).collect();
    fn find(group: &mut [usize], mut k: usize) -> usize {
        while group[k] != k {
            group[k] = group[group[k]];
            k = group[k];
        }
        k
    }
    let mut partner: Vec<Option<usize>> = vec![None; 2 * n];
    for (d, e, f) in pairs {
        if partner[e].is_some() || partner[f].is_some() {
            continue;
        }
        let (ge, gf) = (find(&mut group, e / 2), find(&mut group, f / 2));
        if ge == gf {
            continue;
        }
        let (a, b) = (end(e), end(f));
        if d >= MIN_SEGMENT_MM {
            if !piece.segment_inside(a, b, 8) {
                continue;
            }
            if grid.near(a, b).into_iter().any(|id| crossing(a, b, drawn[id].0, drawn[id].1).is_some()) {
                continue;
            }
            grid.insert(drawn.len(), a, b);
            drawn.push((a, b));
        }
        partner[e] = Some(f);
        partner[f] = Some(e);
        group[ge] = gf;
    }

    let mut used = vec![false; n];
    let mut out = Vec::new();
    for k in 0..n {
        if used[k] {
            continue;
        }
        let Some(mut entry) = [2 * k, 2 * k + 1].into_iter().find(|&e| partner[e].is_none()) else {
            continue;
        };
        let mut joined: Vec<Pt> = Vec::new();
        loop {
            let path = entry / 2;
            used[path] = true;
            if entry % 2 == 0 {
                extend(&mut joined, paths[path].iter().copied());
            } else {
                extend(&mut joined, paths[path].iter().rev().copied());
            }
            match partner[entry ^ 1] {
                Some(next) => entry = next,
                None => break,
            }
        }
        out.push(joined);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{polyline_length, Region};

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)])
    }

    fn star() -> Region {
        let mut pts = Vec::new();
        for i in 0..22 {
            let a = std::f64::consts::PI * (i as f64) / 11.0;
            let r = if i % 2 == 0 { 28.0 } else { 12.0 };
            pts.push((30.0 + r * a.cos(), 30.0 + r * a.sin()));
        }
        Region::from_ring(&pts)
    }

    fn jagged() -> Region {
        Region::from_ring(&[
            (2.0, 8.0),
            (18.0, 2.0),
            (28.0, 14.0),
            (22.0, 16.0),
            (34.0, 22.0),
            (16.0, 28.0),
            (24.0, 42.0),
            (8.0, 36.0),
            (4.0, 44.0),
            (0.0, 26.0),
        ])
    }

    fn longest(sp: &Strokes) -> &Vec<Pt> {
        sp.iter()
            .max_by(|a, b| polyline_length(a).partial_cmp(&polyline_length(b)).unwrap())
            .unwrap()
    }

    /// A real crossing: the segments cut through each other with every end
    /// at least 0.1 mm clear of the other segment. Grazes where a lap peels
    /// off the ring it started on are thinner than any pen.
    fn cross(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
        let o = |p: Pt, q: Pt, r: Pt| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
        let (d1, d2) = (o(c, d, a), o(c, d, b));
        let (d3, d4) = (o(a, b, c), o(a, b, d));
        if !(d1 * d2 < -1e-12 && d3 * d4 < -1e-12) {
            return false;
        }
        let clear = |p: Pt, s: Pt, e: Pt| project(p, s, e).1 > 0.1;
        clear(a, c, d) && clear(b, c, d) && clear(c, a, b) && clear(d, a, b)
    }

    /// No two non-adjacent segments of any stroke cross each other.
    fn assert_no_crossings(sp: &Strokes) {
        let segs: Vec<(usize, usize, Pt, Pt)> = sp
            .iter()
            .enumerate()
            .flat_map(|(si, s)| s.windows(2).enumerate().map(move |(i, w)| (si, i, w[0], w[1])))
            .collect();
        for i in 0..segs.len() {
            for j in i + 1..segs.len() {
                let (si, ii, a, b) = segs[i];
                let (sj, ij, c, d) = segs[j];
                if si == sj && ij <= ii + 1 {
                    continue;
                }
                assert!(
                    !cross(a, b, c, d),
                    "strokes {si}#{ii} (len {:.1}) and {sj}#{ij} (len {:.1}) of {} cross: {a:?}-{b:?} and {c:?}-{d:?}",
                    polyline_length(&sp[si]),
                    polyline_length(&sp[sj]),
                    sp.len()
                );
            }
        }
    }

    #[test]
    fn square_is_one_spiral() {
        let sp = fill_spiral(&square(), 1.0);
        let total: f64 = sp.iter().map(|s| polyline_length(s)).sum();
        let main = polyline_length(longest(&sp));
        assert!(main / total > 0.95, "main path {main} of {total}");
    }

    #[test]
    fn distance_to_wall_grows_steadily() {
        let sp = fill_spiral(&square(), 1.0);
        let mut path = longest(&sp).clone();
        let wall = |p: Pt| p.0.min(p.1).min(20.0 - p.0).min(20.0 - p.1);
        if wall(path[0]) > wall(*path.last().unwrap()) {
            path.reverse();
        }
        // The first lap and the gap line joined onto its end both run along
        // the wall; from the second lap on the path only moves inward.
        let outer_band = 1.5;
        for w in path.windows(2) {
            let (last, d) = (wall(w[0]), wall(w[1]));
            if last < outer_band && d < outer_band {
                continue;
            }
            assert!(d + 0.05 >= last, "went back toward the wall at {:?}", w[1]);
            assert!(d - last < 0.35, "leap of {} at {:?}", d - last, w[1]);
        }
    }

    #[test]
    fn star_winds_as_one_spiral_without_crossing() {
        for spacing in [0.8, 2.8] {
            let sp = fill_spiral(&star(), spacing);
            let long: Vec<_> = sp.iter().filter(|s| polyline_length(s) > 40.0).collect();
            assert_eq!(long.len(), 1, "spacing {spacing}: {} long paths", long.len());
            assert_no_crossings(&sp);
        }
    }

    #[test]
    fn jagged_shape_never_cuts_across() {
        for spacing in [0.8, 2.8] {
            let region = jagged();
            let sp = fill_spiral(&region, spacing);
            assert!(!sp.is_empty());
            for s in &sp {
                for w in s.windows(2) {
                    assert!(
                        region.segment_inside(w[0], w[1], 8),
                        "spacing {spacing}: {:?}-{:?} leaves the shape",
                        w[0],
                        w[1]
                    );
                }
            }
            assert_no_crossings(&sp);
        }
    }

    #[test]
    fn spiral_never_leaves_the_region() {
        let outer = vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)];
        let hole = vec![(12.0, 12.0), (18.0, 12.0), (18.0, 18.0), (12.0, 18.0)];
        let ring = Region::from_polygons(&[vec![outer, hole]]);
        let sp = fill_spiral(&ring, 2.0);
        for path in &sp {
            for w in path.windows(2) {
                let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
                assert!(
                    !(mid.0 > 12.2 && mid.0 < 17.8 && mid.1 > 12.2 && mid.1 < 17.8),
                    "path crossed the courtyard at {:?}",
                    mid
                );
            }
        }
    }

    #[test]
    fn empty_region_no_paths() {
        assert!(fill_spiral(&Region::default(), 1.0).is_empty());
    }

    #[test]
    fn projection_is_clamped_to_the_segment() {
        assert_eq!(project((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)), (0.5, 3.0));
        assert_eq!(project((-4.0, 3.0), (0.0, 0.0), (10.0, 0.0)), (0.0, 5.0));
        assert_eq!(project((13.0, 4.0), (0.0, 0.0), (10.0, 0.0)), (1.0, 5.0));
    }

    /// Two rooms joined by a narrow door: the offsets split, each room gets
    /// its own spiral, and the whole floor is covered without crossings.
    #[test]
    fn each_lobe_is_wound_and_covered() {
        let region = Region::from_ring(&[
            (0.0, 0.0),
            (20.0, 0.0),
            (20.0, 8.0),
            (24.0, 8.0),
            (24.0, 0.0),
            (44.0, 0.0),
            (44.0, 20.0),
            (24.0, 20.0),
            (24.0, 12.0),
            (20.0, 12.0),
            (20.0, 20.0),
            (0.0, 20.0),
        ]);
        let spacing = 1.0;
        let sp = fill_spiral(&region, spacing);
        assert_no_crossings(&sp);
        for s in &sp {
            for w in s.windows(2) {
                assert!(region.segment_inside(w[0], w[1], 8), "{:?}-{:?} leaves the shape", w[0], w[1]);
            }
        }
        let near = |p: Pt| {
            sp.iter().flat_map(|s| s.windows(2)).map(|w| project(p, w[0], w[1]).1).fold(f64::MAX, f64::min)
        };
        let inner = region.inset(spacing);
        for i in 0..88 {
            for j in 0..40 {
                let p = (0.25 + i as f64 * 0.5, 0.25 + j as f64 * 0.5);
                if inner.covers_point(p) {
                    assert!(near(p) < 1.1 * spacing, "{p:?} is {} from the ink", near(p));
                }
            }
        }
        let rooms = |left: bool| {
            sp.iter().flatten().filter(|p| if left { p.0 < 18.0 } else { p.0 > 26.0 }).count()
        };
        assert!(rooms(true) > 20 && rooms(false) > 20);
    }
}
