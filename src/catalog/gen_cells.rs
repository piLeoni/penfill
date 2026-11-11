//! `cells`: a tessellation of the area into irregular cells — gravel and
//! cobbles, pebble beds, mixed rock, terrazzo chips, mud cracks, crazing.
//!
//! The cells are a power diagram (a Voronoi diagram whose sites carry a
//! weight) over anchored blue-noise sites from [`super::points::sites`].
//! Every choice a cell makes — its site, weight, gap, rounding, inner mark —
//! comes from its own lattice cell's seed, and the sites are laid over the
//! area padded by several cells, so a cell near the boundary is the same
//! whichever region draws it: with a global anchor two areas side by side
//! share one gravel bed.
//!
//! Cells are built from each site's neighbours only (a bucket grid, and the
//! usual security radius: once no farther site can cut the cell, stop), so
//! the cost is linear in the number of cells. Cells are convex, and stones
//! are built from them exactly: the cell shrunk by half the gap, corners
//! rounded by an arc, then a little irregularity so they read as stone
//! rather than as a diagram.
//!
//! `vary` gives the sites weights: a few become boulders that absorb their
//! neighbours, some split into a clutch of small stones, so `vary: 0` is an
//! even gravel and `vary: 1` mixed rocks.

use super::points::{sites, Arrange};
use super::schema::{Args, Default, Kind, Opt, CONTRAST, DENSITY, FEATURE, JITTER, TOWARD, VARY};
use super::tone::Tone;
use super::Generator;
use crate::geo::{Cutter, Pt, Region, Strokes};
use crate::pattern::{Canvas, EdgeMode, Tools};
use crate::random::Rng;

const STYLE: Opt = Opt {
    key: "style",
    kind: Kind::Choice(&["stones", "net", "cracks"]),
    default: Default::Text("stones"),
    doc: "`stones`: every cell its own rounded outline, apart from its neighbours (gravel, cobbles, pebble beds). `net`: the shared walls, drawn once (cell nets, mud cracks seen from above, crazing). `cracks`: angular plates with wide cracks between them (dry cracked earth).",
};
const GAP: Opt = Opt {
    key: "gap",
    kind: Kind::Length,
    default: Default::None,
    doc: "The space between neighbouring cells, in mm. Unset: 0.14 × spacing for stones, 0.24 × spacing for cracks; for a net, the break left where walls meet (unset: none, the walls join).",
};
const ROUND: Opt = Opt {
    key: "round",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::None,
    doc: "How rounded the corners are, 0 (sharp) to 1 (as round as the cell allows). Unset: 0.7 for stones, 0.25 for cracks.",
};
const INNER: Opt = Opt {
    key: "inner",
    kind: Kind::Choice(&["none", "dots", "hatch", "ring", "mixed"]),
    default: Default::Text("none"),
    doc: "A mark inside each cell: `dots` (a few specks: chalky soil), `hatch` (lines at each stone's own angle), `ring` (an inner outline: a rimmed pebble), `mixed` (terrazzo: some chips hatched, a few dense, most left plain).",
};

pub static CELLS: Generator = Generator {
    name: "cells",
    about: "Irregular cells: rounded stones apart from each other, a net of shared walls, or plates with wide cracks; with marks inside, sizes from even to mixed, thinned by tone.",
    spacing: 2.0,
    angle: 0.0,
    options: &[&STYLE, &GAP, &ROUND, &VARY, &JITTER, &INNER, &DENSITY, &CONTRAST, &FEATURE, &TOWARD],
    own_mask: true,
    draw,
};

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Stones,
    Net,
    Cracks,
}

#[derive(Clone, Copy, PartialEq)]
enum Inner {
    None,
    Dots,
    Hatch,
    Ring,
    Mixed,
}

fn draw(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    let s = c.spacing;
    if s <= 0.0 || c.region.is_empty() {
        return Strokes::new();
    }
    let style = match a.word("style") {
        "net" => Style::Net,
        "cracks" => Style::Cracks,
        _ => Style::Stones,
    };
    let inner = match a.word("inner") {
        "dots" => Inner::Dots,
        "hatch" => Inner::Hatch,
        "ring" => Inner::Ring,
        "mixed" => Inner::Mixed,
        _ => Inner::None,
    };
    let (gap0, round0) = match style {
        Style::Stones => (0.14, 0.7),
        Style::Cracks => (0.24, 0.25),
        Style::Net => (0.0, 0.0),
    };
    let look = Look {
        style,
        inner,
        gap: a.opt_num("gap").unwrap_or(gap0 * s).max(0.0),
        round: a.opt_num("round").unwrap_or(round0).clamp(0.0, 1.0),
        pitch: s,
    };
    let diagram = Diagram::new(c, t, s, a.num("vary").clamp(0.0, 1.0), a.num("jitter").clamp(0.0, 1.0));
    let tone = Tone::new(a, c.region, c.seed);
    let cover = c.region.cutter();
    // Overflow keeps whole stones rooted in the area; the others cull by it.
    let reach: &Cutter = if matches!(c.edge, EdgeMode::Overflow(_)) { c.limit.cutter() } else { cover };

    let mut out = Strokes::new();
    for (i, site) in diagram.sites.iter().enumerate() {
        // Cull before building: a cell lies within `cull` of its site.
        if !reach.inside(site.at) && reach.distance_within(site.at, diagram.cull) >= diagram.cull {
            continue;
        }
        let Some(cell) = diagram.cell(i) else { continue };
        let inside = cover.inside(site.at);
        let clear = cover.distance_within(site.at, cell.radius * 1.01);
        if !inside && clear >= cell.radius {
            continue; // wholly outside
        }
        if matches!(c.edge, EdgeMode::Overflow(_)) && !inside {
            continue;
        }
        let whole = inside && clear >= cell.radius;
        match style {
            Style::Net => {
                net_walls(c, &diagram, i, &cell, &look, &tone, &mut out);
                let density = if tone.is_even() { 1.0 } else { tone.at(site.at) };
                if look.inner != Inner::None && (whole || c.edge != EdgeMode::Inside) && site.rank <= density {
                    let mut ring = inset_convex(&cell.poly, 0.12 * s);
                    if ring.len() >= 3 {
                        ring.push(ring[0]);
                        look.inner_marks(&ring, &mut Rng::new(site.seed ^ 0x570e), &mut out);
                    }
                }
            }
            Style::Stones | Style::Cracks => {
                let density = if tone.is_even() { 1.0 } else { tone.at(site.at) };
                if site.rank > density {
                    continue;
                }
                let mut rng = Rng::new(site.seed ^ 0x570e);
                let conform = (c.edge == EdgeMode::Inside && !whole).then_some(c.region);
                let Some(stone) = look.stone(&cell.poly, &mut rng, 1.0 - density, conform) else { continue };
                t.add_mask_ring(&stone);
                look.inner_marks(&stone, &mut rng, &mut out);
                out.push(stone);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The diagram
// ---------------------------------------------------------------------------

/// One site of the diagram.
struct Seed {
    at: Pt,
    /// Power weight in mm²: a heavier site claims more of the plane.
    w: f64,
    /// A global identity (from the site's lattice cell), so a wall shared by
    /// two cells is drawn once, by the same one of them, whoever asks.
    id: u64,
    rank: f64,
    seed: u64,
}

struct Cell {
    /// Convex, counter-clockwise, absolute mm.
    poly: Vec<Pt>,
    /// The neighbour across each edge `poly[k] → poly[k + 1]`.
    across: Vec<u32>,
    /// Farthest vertex from the site.
    radius: f64,
}

struct Diagram {
    sites: Vec<Seed>,
    buckets: Buckets,
    /// The heaviest weight.
    wmax: f64,
    /// Half-side of the box a cell starts as: no cell is bigger.
    bound: f64,
    /// How far neighbours are gathered.
    reach: f64,
    /// A cell lies within this of its site.
    cull: f64,
}

const OPEN: u32 = u32::MAX;

impl Diagram {
    fn new(c: &Canvas, t: &Tools, pitch: f64, vary: f64, jitter: f64) -> Diagram {
        let bound = pitch * (1.6 + 0.6 * jitter + 1.8 * vary);
        let wmax = pitch * pitch * (2.2 * vary + 0.1);
        let reach = 2.2 * bound + wmax.sqrt();
        let cull = bound * std::f64::consts::SQRT_2;
        let pad = cull + reach + pitch;
        let (x0, y0, x1, y1) = c.region.bounds_mm();
        let ring = [(x0 - pad, y0 - pad), (x1 + pad, y0 - pad), (x1 + pad, y1 + pad), (x0 - pad, y1 + pad)];
        let field = Region::from_ring(&ring);
        let placed = sites(&t.on(&field), c.seed, &field, pitch, Arrange::Blue, jitter, c.angle);

        let mut out = Vec::with_capacity(placed.len() + placed.len() / 8);
        for mut site in placed {
            let r = &mut site.rng;
            let id = r.next_u64();
            let seed = r.next_u64();
            let (big, split, own) = (r.unit(), r.unit(), r.unit());
            if vary > 0.0 && big < 0.13 * vary {
                // A boulder: its weight pushes its walls out past its neighbours.
                let g = vary * (0.7 + 1.5 * r.unit().powi(2));
                out.push(Seed { at: site.at, w: g * pitch * pitch, id, rank: site.rank, seed });
            } else if vary > 0.0 && split < 0.3 * vary {
                // A clutch of small stones where one would be.
                let n = 2 + usize::from(r.unit() < 0.4);
                let turn = r.range(0.0, std::f64::consts::TAU);
                for k in 0..n {
                    let a = turn + std::f64::consts::TAU * (k as f64 + r.range(-0.15, 0.15)) / n as f64;
                    let d = pitch * r.range(0.22, 0.32);
                    let at = (site.at.0 + d * a.cos(), site.at.1 + d * a.sin());
                    let rank = (site.rank + 0.381_966 * k as f64).fract();
                    out.push(Seed { at, w: -0.05 * pitch * pitch, id: id.rotate_left(17 * k as u32 + 1), rank, seed: seed ^ k as u64 });
                }
            } else {
                let w = vary * 0.2 * (own - 0.5) * pitch * pitch;
                out.push(Seed { at: site.at, w, id, rank: site.rank, seed });
            }
        }
        let buckets = Buckets::new(&out, (x0 - pad, y0 - pad), pitch);
        Diagram { sites: out, buckets, wmax, bound, reach, cull }
    }

    /// Cell `i`, or `None` when it is empty (absorbed by heavier neighbours)
    /// or open (at the far edge of the sites, never inside the area).
    fn cell(&self, i: usize) -> Option<Cell> {
        let me = &self.sites[i];
        let c = me.at;
        let mut near: Vec<(f64, u32)> = Vec::with_capacity(64);
        self.buckets.around(c, self.reach, |j| {
            if j as usize != i {
                let o = self.sites[j as usize].at;
                near.push(((o.0 - c.0).powi(2) + (o.1 - c.1).powi(2), j));
            }
        });
        near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let b = self.bound;
        let mut poly: Vec<(Pt, u32)> = vec![((-b, -b), OPEN), ((b, -b), OPEN), ((b, b), OPEN), ((-b, b), OPEN)];
        let mut far = 2.0 * b * b;
        let mut done = false;
        for &(d2, j) in &near {
            if d2 < 1e-18 {
                continue;
            }
            let d = d2.sqrt();
            // No site this far (or farther) can reach the cell any more.
            if (d2 + me.w - self.wmax) / (2.0 * d) > far.sqrt() {
                done = true;
                break;
            }
            let other = &self.sites[j as usize];
            let o = (other.at.0 - c.0, other.at.1 - c.1);
            poly = clip_power(&poly, o, d2 + me.w - other.w, j);
            if poly.len() < 3 {
                return None;
            }
            far = poly.iter().map(|(p, _)| p.0 * p.0 + p.1 * p.1).fold(0.0, f64::max);
        }
        let r = self.reach;
        if !done && (r * r + me.w - self.wmax) / (2.0 * r) <= far.sqrt() {
            return None;
        }
        if poly.iter().any(|(_, j)| *j == OPEN) {
            return None;
        }
        Some(Cell {
            poly: poly.iter().map(|(p, _)| (c.0 + p.0, c.1 + p.1)).collect(),
            across: poly.iter().map(|(_, j)| *j).collect(),
            radius: far.sqrt(),
        })
    }
}

/// The part of `poly` (relative to its site) on the site's side of the power
/// bisector with a neighbour at `o`: `2 p·o <= rhs`. Each vertex carries the
/// neighbour across the edge that starts at it.
fn clip_power(poly: &[(Pt, u32)], o: Pt, rhs: f64, tag: u32) -> Vec<(Pt, u32)> {
    let side = |p: Pt| 2.0 * (p.0 * o.0 + p.1 * o.1) - rhs;
    let mut out = Vec::with_capacity(poly.len() + 1);
    for k in 0..poly.len() {
        let (a, ta) = poly[k];
        let (b, _) = poly[(k + 1) % poly.len()];
        let (sa, sb) = (side(a), side(b));
        let cut = || {
            let u = sa / (sa - sb);
            (a.0 + u * (b.0 - a.0), a.1 + u * (b.1 - a.1))
        };
        if sa <= 0.0 {
            out.push((a, ta));
            if sb > 0.0 {
                out.push((cut(), tag));
            }
        } else if sb <= 0.0 {
            out.push((cut(), ta));
        }
    }
    out
}

/// Sites binned on a square grid, for neighbour queries.
struct Buckets {
    origin: Pt,
    size: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl Buckets {
    fn new(sites: &[Seed], origin: Pt, size: f64) -> Buckets {
        let (mut x1, mut y1) = (origin.0 + size, origin.1 + size);
        for s in sites {
            x1 = x1.max(s.at.0);
            y1 = y1.max(s.at.1);
        }
        let nx = ((x1 - origin.0) / size).ceil() as usize + 2;
        let ny = ((y1 - origin.1) / size).ceil() as usize + 2;
        let mut b = Buckets { origin, size, nx, ny, cells: vec![Vec::new(); nx * ny] };
        for (k, s) in sites.iter().enumerate() {
            let (i, j) = b.index(s.at);
            b.cells[j * nx + i].push(k as u32);
        }
        b
    }

    fn index(&self, p: Pt) -> (usize, usize) {
        let i = ((p.0 - self.origin.0) / self.size).floor().clamp(0.0, (self.nx - 1) as f64) as usize;
        let j = ((p.1 - self.origin.1) / self.size).floor().clamp(0.0, (self.ny - 1) as f64) as usize;
        (i, j)
    }

    fn around(&self, p: Pt, r: f64, mut f: impl FnMut(u32)) {
        let (i0, j0) = self.index((p.0 - r, p.1 - r));
        let (i1, j1) = self.index((p.0 + r, p.1 + r));
        for j in j0..=j1 {
            for i in i0..=i1 {
                for &k in &self.cells[j * self.nx + i] {
                    f(k);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing a cell
// ---------------------------------------------------------------------------

struct Look {
    style: Style,
    inner: Inner,
    gap: f64,
    round: f64,
    pitch: f64,
}

impl Look {
    /// The stone (or plate) for a cell: a closed ring, or `None` when nothing
    /// worth drawing is left. `light` (0..1) shrinks it where the tone is
    /// lighter. With `conform`, the cell is first cut by that region and the
    /// stone follows the wall (`edge: inside`).
    fn stone(&self, cell: &[Pt], rng: &mut Rng, light: f64, conform: Option<&Region>) -> Option<Vec<Pt>> {
        let cell_area = area(cell);
        let inr = 2.0 * cell_area / perimeter(cell);
        // Every stone a little different: its own gap and corner.
        let half = (self.gap * rng.range(0.75, 1.3) + light * 0.45 * inr) / 2.0;
        let wiggle = (rng.unit(), rng.unit(), rng.unit());
        // Corners round to a pebble's scale, not the stone's: a boulder keeps
        // its faces and is rounded at the corners, as real ones are.
        let corner = self.round * (inr - half).max(0.0).min(0.55 * self.pitch) * rng.range(0.82, 1.0);
        let min_area = (0.22 * self.pitch).powi(2);

        if let Some(region) = conform {
            let piece = Region::from_ring(cell).intersection(region);
            let biggest = largest(piece.all_rings_mm())?;
            if area(&biggest) < 0.3 * cell_area {
                return None;
            }
            let shaped = Region::from_ring(&biggest).inset(half + corner).outset(corner);
            let mut ring = largest(shaped.all_rings_mm())?;
            if ring.len() < 3 || area_signed(&ring).abs() < min_area || ring.iter().any(|p| !region.covers_point(*p)) {
                return None;
            }
            ring.push(ring[0]);
            return Some(ring);
        }

        let mut r = corner;
        let mut core = Vec::new();
        for _ in 0..4 {
            core = inset_convex(cell, half + r);
            if core.len() >= 3 && area_signed(&core).abs() > 1e-9 {
                break;
            }
            r *= 0.5;
        }
        if core.len() < 3 {
            return None;
        }
        let ring = round_convex(&core, r);
        let open = &ring[..ring.len() - 1];
        let kept = area(open);
        // A sliver squeezed between boulders is not a stone: keep shapes no
        // thinner than about 1 : 4 (a disc scores 1 here).
        if kept < min_area || 4.0 * std::f64::consts::PI * kept / perimeter(open).powi(2) < 0.45 {
            return None;
        }
        // Irregularity, drawn inward only so stones never touch: pebbles get
        // a lump or two, plates a slightly ragged edge.
        let (amp, f1, f2) = if self.style == Style::Stones { (0.09, 2.0, 3.0) } else { (0.035, 5.0, 9.0) };
        let ring = crate::fray::resample(&ring, (cell_area.sqrt() / 9.0).max(0.08));
        let mid = centroid(&ring[..ring.len() - 1]);
        let tau = std::f64::consts::TAU;
        let mut out: Vec<Pt> = ring
            .iter()
            .map(|&p| {
                let th = (p.1 - mid.1).atan2(p.0 - mid.0);
                let w = 0.55 * (f1 * th + wiggle.0 * tau).sin() + 0.3 * (f2 * th + wiggle.1 * tau).sin() + 0.15 * (7.0 * th + wiggle.2 * tau).sin();
                let k = 1.0 - amp * 0.5 * (1.0 + w);
                (mid.0 + (p.0 - mid.0) * k, mid.1 + (p.1 - mid.1) * k)
            })
            .collect();
        let n = out.len();
        out[n - 1] = out[0];
        Some(out)
    }

    /// The mark inside a stone, as `inner` asks.
    fn inner_marks(&self, stone: &[Pt], rng: &mut Rng, out: &mut Strokes) {
        let open = &stone[..stone.len() - 1];
        let size = area(open).sqrt();
        let mid = centroid(open);
        // The closest two lines can sit and still read apart at a 0.25 pen.
        let pen = 0.35_f64;
        let pick = match self.inner {
            Inner::Mixed => {
                // Terrazzo: about a third of the chips dark, a few near-solid.
                let u = rng.unit();
                if u < 0.12 {
                    Mark::Dense
                } else if u < 0.38 {
                    Mark::Hatch
                } else {
                    return;
                }
            }
            Inner::None => return,
            Inner::Dots => Mark::Dots,
            Inner::Hatch => Mark::Hatch,
            Inner::Ring => Mark::Ring,
        };
        match pick {
            Mark::Dots => {
                let n = (size / self.pitch * 2.2).round().clamp(1.0, 5.0) as usize;
                let r = (0.05 * self.pitch).max(0.1);
                let shrunk: Vec<Pt> = open.iter().map(|p| (mid.0 + (p.0 - mid.0) * 0.62, mid.1 + (p.1 - mid.1) * 0.62)).collect();
                let (x0, y0, x1, y1) = bbox(&shrunk);
                let mut placed: Vec<Pt> = Vec::new();
                for _ in 0..n * 8 {
                    if placed.len() == n {
                        break;
                    }
                    let p = (rng.range(x0, x1), rng.range(y0, y1));
                    if contains(&shrunk, p) && placed.iter().all(|q| (q.0 - p.0).hypot(q.1 - p.1) > 4.0 * r) {
                        placed.push(p);
                    }
                }
                out.extend(placed.into_iter().map(|p| dot(p, r)));
            }
            Mark::Hatch => {
                let pitch = (0.22 * size).max(pen + 0.1);
                let inset = shrink_ring(open, mid, pitch * 0.35);
                out.extend(hatch_in(&inset, pitch, rng.range(0.0, std::f64::consts::PI), rng.unit()));
            }
            Mark::Dense => {
                let pitch = (0.13 * size).max(pen);
                let inset = shrink_ring(open, mid, pitch * 0.3);
                let a = rng.range(0.0, std::f64::consts::PI);
                out.extend(hatch_in(&inset, pitch, a, rng.unit()));
                out.extend(hatch_in(&inset, pitch, a + std::f64::consts::FRAC_PI_2, rng.unit()));
            }
            Mark::Ring => {
                let d = (0.16 * size).max(pen);
                let mut ring = shrink_ring(open, mid, d);
                if ring.len() >= 3 && area(&ring) > (0.1 * size).powi(2) {
                    ring.push(ring[0]);
                    out.push(ring);
                }
            }
        }
    }
}

enum Mark {
    Dots,
    Hatch,
    Dense,
    Ring,
}

/// The walls of cell `i` it owns (a shared wall belongs to the cell with the
/// smaller id), each with a kink or two so the net reads as drawn by hand.
fn net_walls(c: &Canvas, d: &Diagram, i: usize, cell: &Cell, look: &Look, tone: &Tone, out: &mut Strokes) {
    let me = &d.sites[i];
    let n = cell.poly.len();
    for k in 0..n {
        let j = cell.across[k];
        let other = &d.sites[j as usize];
        if (other.id, j) < (me.id, i as u32) {
            continue;
        }
        let (mut a, mut b) = (cell.poly[k], cell.poly[(k + 1) % n]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len < 1e-6 {
            continue;
        }
        if c.edge == EdgeMode::Inside && !(c.region.covers_point(a) && c.region.covers_point(b) && c.region.segment_inside(a, b, 6)) {
            continue;
        }
        let mut rng = Rng::new(me.id ^ other.id.rotate_left(29) ^ 0x3a11);
        let threshold = rng.unit();
        if !tone.is_even() && threshold > tone.at(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)) {
            continue;
        }
        let (dx, dy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        if look.gap > 0.0 {
            let cut = (look.gap / 2.0).min(len * 0.3);
            a = (a.0 + dx * cut, a.1 + dy * cut);
            b = (b.0 - dx * cut, b.1 - dy * cut);
        }
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let normal = (-dy, dx);
        let at = |t: f64, off: f64| (a.0 + (b.0 - a.0) * t + normal.0 * off, a.1 + (b.1 - a.1) * t + normal.1 * off);
        let wall = if len > 0.45 * look.pitch && rng.unit() < 0.7 {
            let (t1, t2) = (0.33 + rng.range(-0.07, 0.07), 0.67 + rng.range(-0.07, 0.07));
            vec![a, at(t1, rng.range(-0.09, 0.09) * len), at(t2, rng.range(-0.09, 0.09) * len), b]
        } else if len > 0.2 * look.pitch {
            vec![a, at(0.5 + rng.range(-0.12, 0.12), rng.range(-0.1, 0.1) * len), b]
        } else {
            vec![a, b]
        };
        out.push(wall);
    }
}

// ---------------------------------------------------------------------------
// Polygon helpers (the convex ones shared with `tile`)
// ---------------------------------------------------------------------------

pub(super) fn area_signed(poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|k| {
            let (a, b) = (poly[k], poly[(k + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f64>()
        / 2.0
}

fn area(poly: &[Pt]) -> f64 {
    area_signed(poly).abs()
}

fn largest(rings: Vec<Vec<Pt>>) -> Option<Vec<Pt>> {
    rings.into_iter().max_by(|a, b| area(a).total_cmp(&area(b)))
}

fn perimeter(poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n).map(|k| (poly[(k + 1) % n].0 - poly[k].0).hypot(poly[(k + 1) % n].1 - poly[k].1)).sum::<f64>().max(1e-12)
}

fn centroid(poly: &[Pt]) -> Pt {
    let n = poly.len().max(1) as f64;
    let (sx, sy) = poly.iter().fold((0.0, 0.0), |(x, y), p| (x + p.0, y + p.1));
    (sx / n, sy / n)
}

fn bbox(poly: &[Pt]) -> (f64, f64, f64, f64) {
    poly.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(a, b, c, d), p| (a.min(p.0), b.min(p.1), c.max(p.0), d.max(p.1)))
}

/// Even-odd point in polygon.
fn contains(poly: &[Pt], p: Pt) -> bool {
    let mut inside = false;
    let n = poly.len();
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            inside = !inside;
        }
    }
    inside
}

/// A convex polygon (either winding, open) shrunk by `d`: every edge moved
/// inward and the polygon cut by each. Empty when it collapses.
pub(super) fn inset_convex(poly: &[Pt], d: f64) -> Vec<Pt> {
    if poly.len() < 3 {
        return Vec::new();
    }
    let ccw = area_signed(poly) > 0.0;
    let mut cur = poly.to_vec();
    let n = poly.len();
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if len < 1e-12 {
            continue;
        }
        // Inward normal: left of a→b for a counter-clockwise polygon.
        let (nx, ny) = if ccw { (-(b.1 - a.1) / len, (b.0 - a.0) / len) } else { ((b.1 - a.1) / len, -(b.0 - a.0) / len) };
        let side = |p: Pt| (p.0 - a.0) * nx + (p.1 - a.1) * ny - d;
        let mut next = Vec::with_capacity(cur.len() + 1);
        for m in 0..cur.len() {
            let (p, q) = (cur[m], cur[(m + 1) % cur.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                next.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let u = sp / (sp - sq);
                next.push((p.0 + u * (q.0 - p.0), p.1 + u * (q.1 - p.1)));
            }
        }
        cur = next;
        if cur.len() < 3 {
            return Vec::new();
        }
    }
    cur
}

/// A convex polygon grown by `r` with round corners (its Minkowski sum with
/// a disc), as a closed ring.
pub(super) fn round_convex(poly: &[Pt], r: f64) -> Vec<Pt> {
    let mut pts: Vec<Pt> = Vec::with_capacity(poly.len());
    for &p in poly {
        if pts.last().is_none_or(|q: &Pt| (q.0 - p.0).hypot(q.1 - p.1) > 1e-9) {
            pts.push(p);
        }
    }
    while pts.len() > 1 && (pts[0].0 - pts[pts.len() - 1].0).hypot(pts[0].1 - pts[pts.len() - 1].1) <= 1e-9 {
        pts.pop();
    }
    if area_signed(&pts) < 0.0 {
        pts.reverse();
    }
    let n = pts.len();
    let mut out = Vec::new();
    if r <= 1e-9 || n < 3 {
        out.extend_from_slice(&pts);
    } else {
        for k in 0..n {
            let (prev, p, next) = (pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]);
            // Outward normals of the edges into and out of `p`.
            let a0 = (p.1 - prev.1).atan2(p.0 - prev.0) - std::f64::consts::FRAC_PI_2;
            let mut a1 = (next.1 - p.1).atan2(next.0 - p.0) - std::f64::consts::FRAC_PI_2;
            while a1 < a0 {
                a1 += std::f64::consts::TAU;
            }
            let steps = ((a1 - a0) / (std::f64::consts::PI / 10.0)).ceil().max(1.0) as usize;
            for s in 0..=steps {
                let a = a0 + (a1 - a0) * s as f64 / steps as f64;
                out.push((p.0 + r * a.cos(), p.1 + r * a.sin()));
            }
        }
    }
    if let Some(&first) = out.first() {
        out.push(first);
    }
    out
}

/// `ring` (open) pulled `d` toward `mid`, along each point's ray.
fn shrink_ring(ring: &[Pt], mid: Pt, d: f64) -> Vec<Pt> {
    ring.iter()
        .filter_map(|p| {
            let (dx, dy) = (p.0 - mid.0, p.1 - mid.1);
            let len = dx.hypot(dy);
            (len > d).then(|| (mid.0 + dx * (len - d) / len, mid.1 + dy * (len - d) / len))
        })
        .collect()
}

/// Parallel lines `pitch` apart at `angle` (radians) inside the open
/// polygon `poly`, offset by `phase` (0..1 of a pitch) from the line through
/// the page origin.
pub(super) fn hatch_in(poly: &[Pt], pitch: f64, angle: f64, phase: f64) -> Strokes {
    let mut out = Strokes::new();
    if poly.len() < 3 || pitch <= 0.0 {
        return out;
    }
    let (s, c) = angle.sin_cos();
    let to = |p: Pt| (p.0 * c + p.1 * s, -p.0 * s + p.1 * c);
    let back = |q: Pt| (q.0 * c - q.1 * s, q.0 * s + q.1 * c);
    let local: Vec<Pt> = poly.iter().map(|p| to(*p)).collect();
    let (_, v0, _, v1) = bbox(&local);
    let first = ((v0 / pitch) - phase).ceil() as i64;
    let last = ((v1 / pitch) - phase).floor() as i64;
    let n = local.len();
    for k in first..=last {
        let v = (k as f64 + phase) * pitch;
        let mut xs: Vec<f64> = Vec::new();
        for m in 0..n {
            let (a, b) = (local[m], local[(m + 1) % n]);
            if (a.1 <= v) != (b.1 <= v) {
                xs.push(a.0 + (v - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
        xs.sort_by(f64::total_cmp);
        for w in xs.chunks(2) {
            if w.len() == 2 && w[1] - w[0] > pitch * 0.25 {
                out.push(vec![back((w[0], v)), back((w[1], v))]);
            }
        }
    }
    out
}

/// A dot a pen can draw: a tiny closed loop.
fn dot(p: Pt, r: f64) -> Vec<Pt> {
    (0..=6)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / 6.0;
            (p.0 + r * a.cos(), p.1 + r * a.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::catalog::{fill, fill_output, Spec};
    use crate::geo::{polyline_length, Region, Strokes};
    use crate::pattern::EdgeMode;
    use crate::Anchor;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
        Region::from_ring(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
    }

    fn ink(s: &Strokes) -> f64 {
        s.iter().map(|p| polyline_length(p)).sum()
    }

    fn cells(region: &Region, spec: Spec) -> Strokes {
        fill("cells", region, &spec).unwrap().strokes
    }

    #[test]
    fn every_style_and_mark_draws_in_a_small_square() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        for style in ["stones", "net", "cracks"] {
            for inner in ["none", "dots", "hatch", "ring", "mixed"] {
                for vary in [0.0, 1.0] {
                    let s = cells(&sq, Spec::default().option("style", style).option("inner", inner).option("vary", vary));
                    assert!(s.len() > 20, "{style}/{inner}/{vary}: {} strokes", s.len());
                }
            }
        }
    }

    #[test]
    fn same_seed_same_drawing() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let spec = Spec::default().seed(4).option("vary", 0.6).option("inner", "mixed").option("jitter", 0.5);
        assert_eq!(cells(&sq, spec.clone()), cells(&sq, spec.clone()));
        assert_ne!(cells(&sq, spec.clone()), cells(&sq, spec.seed(5)));
    }

    #[test]
    fn a_global_anchor_leaves_no_seam() {
        for style in ["stones", "net", "cracks"] {
            for vary in [0.0, 1.0] {
                let spec = Spec::default()
                    .anchor(Anchor::GLOBAL)
                    .edge(EdgeMode::Clip)
                    .option("style", style)
                    .option("vary", vary)
                    .option("inner", "hatch");
                let whole = ink(&cells(&rect(0.0, 0.0, 40.0, 20.0), spec.clone()));
                let halves = ink(&cells(&rect(0.0, 0.0, 20.0, 20.0), spec.clone())) + ink(&cells(&rect(20.0, 0.0, 40.0, 20.0), spec));
                assert!(((whole - halves) / whole).abs() < 0.01, "{style} vary {vary}: {whole} vs {halves}");
            }
        }
    }

    #[test]
    fn inside_keeps_stones_whole_and_inside() {
        let blob = Region::from_ring(&[(0.0, 0.0), (24.0, 3.0), (30.0, 18.0), (12.0, 26.0), (2.0, 15.0)]);
        for style in ["stones", "cracks"] {
            let s = cells(&blob, Spec::default().edge(EdgeMode::Inside).option("style", style).option("vary", 0.5));
            assert!(s.len() > 30);
            for stroke in &s {
                assert_eq!(stroke.first(), stroke.last(), "{style}: a stone cut open");
                for p in stroke {
                    assert!(blob.covers_point(*p), "{style}: {p:?} outside");
                }
            }
        }
    }

    #[test]
    fn stones_register_their_mask() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let o = fill_output("cells", &sq, &Spec::default()).unwrap();
        let (x0, y0, x1, y1) = o.mask.bounds_mm();
        assert!(x1 - x0 > 15.0 && y1 - y0 > 15.0);
        // The mask is the stones, not the square: the gaps are left out.
        let covered: f64 = o.mask.all_rings_mm().iter().map(|r| super::area_signed(r)).sum::<f64>().abs();
        assert!(covered < 0.9 * 400.0, "mask covers {covered} of 400 mm²");
    }

    #[test]
    fn vary_makes_sizes_differ() {
        let sq = rect(0.0, 0.0, 40.0, 40.0);
        let biggest = |vary: f64| {
            cells(&sq, Spec::default().option("vary", vary).edge(EdgeMode::Inside))
                .iter()
                .map(|s| super::area(&s[..s.len() - 1]))
                .fold(0.0, f64::max)
        };
        let (even, mixed) = (biggest(0.0), biggest(1.0));
        assert!(mixed > 2.5 * even, "{mixed} vs {even}");
    }
}
