//! `tile`: a motif repeated on an anchored lattice — brickwork, paving,
//! parquet, tiles, gratings, steps; your own polylines; or a CAD `.pat`
//! hatch.
//!
//! A named motif is a set of **pieces**: the bricks, setts, boards or
//! tiles themselves, laid in frame coordinates (`u` along the courses, `v`
//! across) through the anchor. Every irregular choice — a flagstone's
//! length, a course's height, a sett's worn corner — is a function of the
//! piece's lattice indices, never of the region, so with a global anchor two
//! areas side by side are one pavement.
//!
//! Pieces are drawn one of two ways, as their joints are in real drawings:
//!
//! - `gap: 0` — single joint lines, each drawn once: the pieces' edges are
//!   gathered and every run of collinear, overlapping edges merged into one
//!   line, so a brick course is one long stroke and no joint is plotted
//!   twice.
//! - `gap > 0` — every piece its own outline, the joint between neighbours
//!   this wide (setts, tiles with grout).
//!
//! With `edge: inside` only the pieces wholly inside are laid. `jitter`
//! makes hand-laid pieces irregular (their corners wander), for pieces drawn
//! with a gap.

use super::gen_cells::{hatch_in, inset_convex, round_convex};
use super::schema::{Args, Default, Kind, Motif, Opt, JITTER};
use super::Generator;
use crate::anchor::{cell_seed, indices};
use crate::geo::{Pt, Region, Strokes};
use crate::pattern::{Canvas, EdgeMode, Tools};
use crate::random::Rng;

/// Every named motif.
pub const MOTIFS: &[&str] = &["brick", "herringbone", "basket", "parquet", "setts", "flagstones", "tiles", "hexagons", "stairs", "grate", "chess"];

const MOTIF: Opt = Opt {
    key: "motif",
    kind: Kind::Motif(MOTIFS),
    default: Default::Text("brick"),
    doc: "What is laid: `brick` (running bond), `herringbone` (pavers at right angles in a zigzag), `basket` (pairs of pavers, alternating direction), `parquet` (square blocks of boards, alternating), `setts` (small stones in staggered courses), `flagstones` (random rectangular paving: course heights and stone lengths vary, joints staggered), `tiles` (square tiles with a grout joint), `hexagons` (hexagonal pavers), `stairs` (stone steps seen from above), `grate` (bar grating), `chess` (squares, alternate ones hatched); or your own polylines in mm around the origin.",
};
const SIZE: Opt = Opt {
    key: "size",
    kind: Kind::Lengths,
    default: Default::None,
    doc: "One piece, `[length, width]` in mm. Unset, at spacing s: brick [3s, s], herringbone and basket [2s, s], parquet [4s, 0.8s] (a block of five boards), setts [1.15s, s], flagstones [3s, 2s], tiles and chess [3s, 3s], hexagons 2.5s across, stairs [6s, 1.5s] (slab length, tread depth), grate [3s, 0.5s] (cross bar pitch, bar pitch). For your own polylines: the tile box, centred on the origin (unset: the lines' extent).",
};
const SHIFT: Opt = Opt {
    key: "shift",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::Num(0.0),
    doc: "For your own motif: how far each row is shifted along, as a share of the tile width (0.5: brick bond).",
};
const GAP: Opt = Opt {
    key: "gap",
    kind: Kind::Length,
    default: Default::None,
    doc: "The joint between pieces, in mm: 0 draws single joint lines, each once; more draws every piece as its own outline, this far from its neighbours. Unset: 0.18 × width for setts, 0.1 × width for tiles, 0 for the rest.",
};
const PAT: Opt = Opt {
    key: "pat",
    kind: Kind::Text,
    default: Default::None,
    doc: "A CAD hatch instead of a motif: a built-in name (ANSI31, ANSI32, ANSI33, ANSI37, BRICK, HERRINGBONE, BASKET, PARQUET, EARTH, CROSS, DASH, STAIRS, GRATE, ANGLE) or the text of a `.pat` definition. One pattern unit is `spacing` mm.",
};
pub static TILE: Generator = Generator {
    name: "tile",
    about: "A motif laid on a lattice: brick, herringbone, basket weave, parquet, setts, flagstones, tiles, hexagons, stairs, grating, chess; your own polylines; or a CAD .pat hatch.",
    spacing: 2.0,
    angle: 0.0,
    options: &[&MOTIF, &SIZE, &SHIFT, &GAP, &JITTER, &PAT],
    own_mask: false,
    draw,
};

fn draw(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    let s = c.spacing;
    if s <= 0.0 || c.region.is_empty() {
        return Strokes::new();
    }
    let pat = a.word("pat").trim();
    if a.has("pat") && !pat.is_empty() {
        return cad(c, pat);
    }
    match a.motif("motif") {
        Some(Motif::Lines(lines)) => custom(c, lines, a.pair("size"), a.num("shift")),
        Some(Motif::Named(name)) => named(c, t, name, a),
        None => Strokes::new(),
    }
}

/// A `.pat` hatch: a built-in by name, or the first pattern in the text.
fn cad(c: &Canvas, text: &str) -> Strokes {
    let pattern = crate::pat::builtin(text).or_else(|| crate::pat::parse_pat(text).into_iter().find(|p| !p.lines.is_empty()));
    match pattern {
        Some(p) => crate::pat::fill_pat(&c.limit, &p, c.spacing, c.angle, c.anchor),
        None => Strokes::new(),
    }
}

/// Your own polylines, around the origin, tiled in a `size` box.
fn custom(c: &Canvas, lines: &Strokes, size: Option<(f64, f64)>, shift: f64) -> Strokes {
    let (x0, y0, x1, y1) = lines.iter().flatten().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(a, b, c, d), p| (a.min(p.0), b.min(p.1), c.max(p.0), d.max(p.1)));
    let (w, h) = size.unwrap_or((x1 - x0, y1 - y0));
    let floor = c.spacing * 0.25;
    let (w, h) = (if w > 1e-6 { w } else { floor }, if h > 1e-6 { h } else { floor });
    // fill_tile's box is [0, w] × [0, h]; ours is centred on the origin.
    let moved: Strokes = lines.iter().map(|l| l.iter().map(|p| (p.0 + w / 2.0, p.1 + h / 2.0)).collect()).collect();
    crate::tile::fill_tile(&c.limit, &moved, w, h, c.angle, shift, c.anchor)
}

// ---------------------------------------------------------------------------
// Named motifs
// ---------------------------------------------------------------------------

/// One piece: a convex polygon in frame coordinates.
struct Piece {
    ring: Vec<Pt>,
    /// The piece's own randomness, from its lattice indices.
    key: u64,
    /// Chess: hatched.
    dark: bool,
}

/// What a motif lays over a span of its frame.
#[derive(Default)]
struct Laid {
    pieces: Vec<Piece>,
    /// Lines that are not piece edges (stair nosings, grating bars).
    lines: Strokes,
}

#[derive(Clone, Copy, PartialEq)]
enum Name {
    Brick,
    Herringbone,
    Basket,
    Parquet,
    Setts,
    Flagstones,
    Tiles,
    Hexagons,
    Stairs,
    Grate,
    Chess,
}

impl Name {
    fn parse(word: &str) -> Name {
        match word {
            "herringbone" => Name::Herringbone,
            "basket" => Name::Basket,
            "parquet" => Name::Parquet,
            "setts" => Name::Setts,
            "flagstones" => Name::Flagstones,
            "tiles" => Name::Tiles,
            "hexagons" => Name::Hexagons,
            "stairs" => Name::Stairs,
            "grate" => Name::Grate,
            "chess" => Name::Chess,
            _ => Name::Brick,
        }
    }

    /// A piece's `[length, width]` at spacing `s`.
    fn size(self, s: f64) -> (f64, f64) {
        match self {
            Name::Brick => (3.0 * s, s),
            Name::Herringbone | Name::Basket => (2.0 * s, s),
            Name::Parquet => (4.0 * s, 0.8 * s),
            Name::Setts => (1.15 * s, s),
            Name::Flagstones => (3.0 * s, 2.0 * s),
            Name::Tiles | Name::Chess => (3.0 * s, 3.0 * s),
            Name::Hexagons => (2.5 * s, 2.5 * s),
            Name::Stairs => (6.0 * s, 1.5 * s),
            Name::Grate => (3.0 * s, 0.5 * s),
        }
    }

    /// The joint, as a share of the piece's width, when `gap` is unset.
    fn gap(self) -> f64 {
        match self {
            Name::Setts => 0.18,
            Name::Tiles => 0.1,
            _ => 0.0,
        }
    }

    /// How far the frame is turned from the pattern's angle: herringbone is
    /// laid on the diagonal of its bricks so its zigzag runs along `u`.
    fn turn(self) -> f64 {
        if self == Name::Herringbone {
            -45.0
        } else {
            0.0
        }
    }
}

fn named(c: &Canvas, t: &Tools, word: &str, a: &Args) -> Strokes {
    let name = Name::parse(word);
    let (w, h) = a.pair("size").filter(|(w, h)| *w > 0.0 && *h > 0.0).unwrap_or_else(|| name.size(c.spacing));
    let gap = a.opt_num("gap").unwrap_or(name.gap() * w.min(h)).max(0.0);
    let jitter = a.num("jitter");
    let frame = t.frame(c.angle + name.turn());
    let margin = 2.0 * w.max(h);
    let span = frame.span(&c.limit, margin);
    let seed = c.seed ^ 0x711e;
    let laid = match name {
        Name::Brick => courses(span, w, h, seed, Courses { stagger: Stagger::Half, vary_length: 0.0, vary_height: 0.0 }),
        Name::Setts => courses(span, w, h, seed, Courses { stagger: Stagger::Half, vary_length: 0.3, vary_height: 0.0 }),
        Name::Flagstones => courses(span, w, h, seed, Courses { stagger: Stagger::Random, vary_length: 0.6, vary_height: 0.5 }),
        Name::Herringbone => herringbone(span, w, h, seed),
        Name::Basket | Name::Parquet => blocks(span, w, h, seed),
        Name::Tiles | Name::Chess => grid(span, w, h, seed, name == Name::Chess),
        Name::Hexagons => hexagons(span, w, seed),
        Name::Stairs => stairs(span, w, h, seed),
        Name::Grate => grate(span, w, h),
    };

    // Which pieces are laid.
    let world = |p: Pt| frame.world(p);
    let keep = |piece: &Piece| -> bool {
        let ring: Vec<Pt> = piece.ring.iter().map(|p| world(*p)).collect();
        match c.edge {
            EdgeMode::Inside => wholly_inside(c.region, &ring),
            EdgeMode::Overflow(_) => c.region.covers_point(mean(&ring)),
            EdgeMode::Clip => true,
        }
    };
    let pieces: Vec<&Piece> = laid.pieces.iter().filter(|p| keep(p)).collect();

    let mut out = Strokes::new();
    let hatch = |ring: &[Pt], out: &mut Strokes| {
        let pitch = (w.min(h) / 7.0).max(0.35);
        for l in hatch_in(ring, pitch, std::f64::consts::FRAC_PI_4, 0.0) {
            out.push(l.into_iter().map(world).collect());
        }
    };
    if gap > 0.0 {
        let worn = if name == Name::Setts { 0.16 * w.min(h) } else { 0.0 };
        let wander = (jitter * 0.12 + if name == Name::Setts { 0.04 } else { 0.0 }) * w.min(h);
        for piece in pieces {
            let mut rng = Rng::new(piece.key);
            let ring: Vec<Pt> = piece.ring.iter().map(|p| (p.0 + rng.range(-wander, wander), p.1 + rng.range(-wander, wander))).collect();
            let core = inset_convex(&ring, gap / 2.0 + worn);
            if core.len() < 3 {
                continue;
            }
            let outline = round_convex(&core, worn);
            if piece.dark {
                hatch(&outline[..outline.len() - 1], &mut out);
            }
            out.push(outline.into_iter().map(world).collect());
        }
    } else {
        let mut edges = Vec::new();
        for piece in &pieces {
            let n = piece.ring.len();
            edges.extend((0..n).map(|k| (piece.ring[k], piece.ring[(k + 1) % n])));
            if piece.dark {
                hatch(&piece.ring, &mut out);
            }
        }
        out.extend(merge(edges).into_iter().map(|(a, b)| vec![world(a), world(b)]));
    }
    out.extend(laid.lines.into_iter().map(|l| l.into_iter().map(world).collect()));
    out
}

/// A frame span: `(u0, u1, v0, v1)`.
type Span = (f64, f64, f64, f64);

fn rect(u: f64, v: f64, w: f64, h: f64) -> Vec<Pt> {
    vec![(u, v), (u + w, v), (u + w, v + h), (u, v + h)]
}

/// A random number in `0..1` for lattice place `(i, j)`, stream `k`.
fn hash(seed: u64, i: i64, j: i64, k: u64) -> f64 {
    let z = cell_seed(seed ^ k.wrapping_mul(0x9E37_79B9_7F4A_7C15), i, j);
    (z >> 11) as f64 / (1u64 << 53) as f64
}

#[derive(Clone, Copy, PartialEq)]
enum Stagger {
    /// Every other course shifted half a piece (running bond).
    Half,
    /// Each course its own offset, joints kept off the courses beside it.
    Random,
}

/// How a coursed motif varies.
#[derive(Clone, Copy)]
struct Courses {
    stagger: Stagger,
    /// How far a joint may move along its course, as a share of the length.
    vary_length: f64,
    /// How far a course line may move, as a share of the course height.
    vary_height: f64,
}

/// Pieces laid in courses: brick, setts, flagstones. Course `j` runs from
/// `line(j)` to `line(j + 1)`; its joints are `joint(i, j)`. Both are pure
/// functions of the indices, so any region gets the same pavement.
fn courses(span: Span, w: f64, h: f64, seed: u64, how: Courses) -> Laid {
    let (u0, u1, v0, v1) = span;
    let line = |j: i64| j as f64 * h + how.vary_height * h * (hash(seed, 0, j, 1) - 0.5);
    let base = |i: i64, j: i64| {
        let offset = match how.stagger {
            Stagger::Half => 0.5 * j.rem_euclid(2) as f64,
            Stagger::Random => hash(seed, 0, j, 2),
        };
        (i as f64 + offset) * w + how.vary_length * w * (hash(seed, i, j, 3) - 0.5)
    };
    // Random courses: odd courses step their joints off the joints of the
    // (fixed) even courses above and below, so no joint runs through two.
    let joint = |i: i64, j: i64| -> f64 {
        let x = base(i, j);
        if how.stagger != Stagger::Random || j.rem_euclid(2) == 0 {
            return x;
        }
        let clear = |x: f64| {
            [j - 1, j + 1].iter().all(|&jj| {
                let k = ((x / w).floor() as i64 - 2)..=((x / w).ceil() as i64 + 2);
                k.into_iter().all(|ii| (base(ii, jj) - x).abs() > 0.2 * w)
            })
        };
        let (lo, hi) = (base(i - 1, j) + 0.35 * w, base(i + 1, j) - 0.35 * w);
        [0.0, 0.25, -0.25, 0.4, -0.4].iter().map(|d| x + d * w).find(|&x| x > lo && x < hi && clear(x)).unwrap_or(x)
    };
    let mut laid = Laid::default();
    let j0 = (v0 / h).floor() as i64 - 2;
    let j1 = (v1 / h).ceil() as i64 + 1;
    for j in j0..=j1 {
        let (top, bottom) = (line(j), line(j + 1));
        for i in indices(u0 - w, u1 + w, w) {
            let (a, b) = (joint(i, j), joint(i + 1, j));
            if b - a > 1e-6 {
                laid.pieces.push(Piece { ring: vec![(a, top), (b, top), (b, bottom), (a, bottom)], key: cell_seed(seed, i, j), dark: false });
            }
        }
    }
    laid
}

/// 90° herringbone of `l × wd` pavers: along each diagonal a staircase of
/// alternately flat and upright pavers, each one paver width along and up
/// from the last; the staircases repeat every `(l, -l)`. The frame is turned
/// 45° so the zigzag runs along the pattern's angle.
fn herringbone(span: Span, l: f64, wd: f64, seed: u64) -> Laid {
    let (u0, u1, v0, v1) = span;
    let corners = [(u0, v0), (u1, v0), (u0, v1), (u1, v1)];
    let sum = corners.iter().map(|p| p.0 + p.1);
    let dif = corners.iter().map(|p| p.0 - p.1);
    let (s0, s1) = sum.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    let (d0, d1) = dif.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
    let mut laid = Laid::default();
    // A paver at step n of staircase m starts at n·(wd, wd) + m·(l, −l).
    let (n0, n1) = ((s0 / (2.0 * wd)).floor() as i64 - 2, (s1 / (2.0 * wd)).ceil() as i64 + 2);
    let (m0, m1) = ((d0 / (2.0 * l)).floor() as i64 - 2, (d1 / (2.0 * l)).ceil() as i64 + 2);
    for m in m0..=m1 {
        for n in n0..=n1 {
            let (x, y) = (n as f64 * wd + m as f64 * l, n as f64 * wd - m as f64 * l);
            if x + l < u0 - wd || x - wd > u1 || y + l < v0 || y > v1 {
                continue;
            }
            let key = cell_seed(seed, n, m);
            laid.pieces.push(Piece { ring: rect(x, y, l, wd), key, dark: false });
            laid.pieces.push(Piece { ring: rect(x - wd, y, wd, l), key: key ^ 1, dark: false });
        }
    }
    laid
}

/// Square blocks of parallel pieces, alternate blocks turned a quarter:
/// basket weave (two pavers a block) and block parquet (boards). The block
/// is one piece long; it holds as many pieces as their width allows.
fn blocks(span: Span, l: f64, wd: f64, seed: u64) -> Laid {
    let (u0, u1, v0, v1) = span;
    let k = (l / wd).round().max(2.0) as usize;
    let strip = l / k as f64;
    let mut laid = Laid::default();
    for j in indices(v0, v1, l) {
        for i in indices(u0, u1, l) {
            let (x, y) = (i as f64 * l, j as f64 * l);
            for q in 0..k {
                let d = q as f64 * strip;
                let ring = if (i + j).rem_euclid(2) == 0 { rect(x, y + d, l, strip) } else { rect(x + d, y, strip, l) };
                laid.pieces.push(Piece { ring, key: cell_seed(seed, i, j) ^ q as u64, dark: false });
            }
        }
    }
    laid
}

/// A square grid of `w × h` pieces; with `chess`, alternate ones dark.
fn grid(span: Span, w: f64, h: f64, seed: u64, chess: bool) -> Laid {
    let (u0, u1, v0, v1) = span;
    let mut laid = Laid::default();
    for j in indices(v0, v1, h) {
        for i in indices(u0, u1, w) {
            let dark = chess && (i + j).rem_euclid(2) == 0;
            laid.pieces.push(Piece { ring: rect(i as f64 * w, j as f64 * h, w, h), key: cell_seed(seed, i, j), dark });
        }
    }
    laid
}

/// Hexagonal pavers `across` mm across the flats, points up, in rows.
fn hexagons(span: Span, across: f64, seed: u64) -> Laid {
    let (u0, u1, v0, v1) = span;
    let r = across / 3f64.sqrt();
    let dy = 1.5 * r;
    let corners: Vec<Pt> = (0..6).map(|k| (std::f64::consts::FRAC_PI_6 + std::f64::consts::FRAC_PI_3 * k as f64).sin_cos()).map(|(s, c)| (r * c, r * s)).collect();
    let mut laid = Laid::default();
    for j in indices(v0, v1, dy) {
        let shift = 0.5 * across * j.rem_euclid(2) as f64;
        for i in indices(u0, u1, across) {
            let (x, y) = (i as f64 * across + shift, j as f64 * dy);
            laid.pieces.push(Piece { ring: corners.iter().map(|p| (x + p.0, y + p.1)).collect(), key: cell_seed(seed, i, j), dark: false });
        }
    }
    laid
}

/// Stone steps from above: each tread a course of slabs `w` long, `h` deep,
/// joints staggered from step to step, and a second line just behind each
/// nosing where the riser drops away.
fn stairs(span: Span, w: f64, h: f64, seed: u64) -> Laid {
    let (u0, u1, v0, v1) = span;
    let mut laid = courses(span, w, h, seed, Courses { stagger: Stagger::Half, vary_length: 0.2, vary_height: 0.0 });
    for j in indices(v0, v1, h) {
        let v = j as f64 * h + 0.14 * h;
        laid.lines.push(vec![(u0, v), (u1, v)]);
    }
    laid
}

/// Bar grating: bearing bars every `h`, crossed by rods every `w`.
fn grate(span: Span, w: f64, h: f64) -> Laid {
    let (u0, u1, v0, v1) = span;
    let mut laid = Laid::default();
    for j in indices(v0, v1, h) {
        laid.lines.push(vec![(u0, j as f64 * h), (u1, j as f64 * h)]);
    }
    for i in indices(u0, u1, w) {
        laid.lines.push(vec![(i as f64 * w, v0), (i as f64 * w, v1)]);
    }
    laid
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

fn mean(ring: &[Pt]) -> Pt {
    let n = ring.len().max(1) as f64;
    let (x, y) = ring.iter().fold((0.0, 0.0), |(x, y), p| (x + p.0, y + p.1));
    (x / n, y / n)
}

/// Whether the convex piece `ring` (page mm) lies wholly in `region`.
fn wholly_inside(region: &Region, ring: &[Pt]) -> bool {
    if !ring.iter().all(|p| region.covers_point(*p)) {
        return false;
    }
    let mid = mean(ring);
    let reach = ring.iter().map(|p| (p.0 - mid.0).hypot(p.1 - mid.1)).fold(0.0, f64::max);
    if region.dist_to_boundary_within(mid, reach * 1.01) >= reach {
        return true;
    }
    // Near the wall: the region's own corners or holes may poke in.
    let outside = Region::from_ring(ring).difference(region);
    outside.all_rings_mm().iter().map(|r| super::gen_cells::area_signed(r)).sum::<f64>().abs() < 1e-6
}

/// Segments with every run of collinear, overlapping ones merged into one:
/// a joint shared by two pieces is drawn once, a course line is one stroke.
fn merge(segments: Vec<(Pt, Pt)>) -> Vec<(Pt, Pt)> {
    struct Seg {
        angle: f64,
        dir: Pt,
        offset: f64,
        t0: f64,
        t1: f64,
    }
    let mut segs: Vec<Seg> = segments
        .into_iter()
        .filter_map(|(a, b)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy);
            if len < 1e-9 {
                return None;
            }
            // One direction per line: pointing right, or up when vertical.
            let (dx, dy) = if dx < -1e-9 * len || (dx.abs() <= 1e-9 * len && dy < 0.0) { (-dx / len, -dy / len) } else { (dx / len, dy / len) };
            let offset = -a.0 * dy + a.1 * dx;
            let (ta, tb) = (a.0 * dx + a.1 * dy, b.0 * dx + b.1 * dy);
            Some(Seg { angle: dy.atan2(dx), dir: (dx, dy), offset, t0: ta.min(tb), t1: ta.max(tb) })
        })
        .collect();
    segs.sort_by(|a, b| a.angle.total_cmp(&b.angle));
    let mut out = Vec::new();
    let mut start = 0;
    while start < segs.len() {
        let mut end = start + 1;
        while end < segs.len() && segs[end].angle - segs[start].angle < 1e-7 {
            end += 1;
        }
        let family = &mut segs[start..end];
        family.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let mut k = 0;
        while k < family.len() {
            let mut m = k + 1;
            while m < family.len() && family[m].offset - family[k].offset < 1e-6 {
                m += 1;
            }
            let line = &mut family[k..m];
            let (dir, offset) = (line[0].dir, line[0].offset);
            let at = |t: f64| (t * dir.0 - offset * dir.1, t * dir.1 + offset * dir.0);
            line.sort_by(|a, b| a.t0.total_cmp(&b.t0));
            let (mut lo, mut hi) = (line[0].t0, line[0].t1);
            for s in line.iter().skip(1) {
                if s.t0 <= hi + 1e-6 {
                    hi = hi.max(s.t1);
                } else {
                    out.push((at(lo), at(hi)));
                    (lo, hi) = (s.t0, s.t1);
                }
            }
            out.push((at(lo), at(hi)));
            k = m;
        }
        start = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::MOTIFS;
    use crate::catalog::{fill, Spec, Value};
    use crate::geo::{polyline_length, Region, Strokes};
    use crate::pattern::EdgeMode;
    use crate::Anchor;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
        Region::from_ring(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
    }

    fn ink(s: &Strokes) -> f64 {
        s.iter().map(|p| polyline_length(p)).sum()
    }

    fn tile(region: &Region, spec: Spec) -> Strokes {
        fill("tile", region, &spec).unwrap().strokes
    }

    #[test]
    fn every_motif_draws_in_a_small_square() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        for m in MOTIFS {
            let s = tile(&sq, Spec::default().option("motif", *m));
            assert!(s.len() >= 8, "{m}: {} strokes", s.len());
            for p in s.iter().flatten() {
                assert!(sq.covers_point(*p), "{m}: {p:?} outside");
            }
        }
    }

    #[test]
    fn same_seed_same_drawing() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        for m in ["flagstones", "setts"] {
            let spec = Spec::default().seed(3).option("motif", m).option("jitter", 0.5);
            assert_eq!(tile(&sq, spec.clone()), tile(&sq, spec.clone()));
            assert_ne!(tile(&sq, spec.clone()), tile(&sq, spec.seed(4)), "{m}");
        }
    }

    #[test]
    fn a_global_anchor_leaves_no_seam() {
        for m in MOTIFS {
            for angle in [0.0, 30.0] {
                let spec = Spec::default().anchor(Anchor::GLOBAL).edge(EdgeMode::Clip).angle(angle).option("motif", *m);
                let whole = ink(&tile(&rect(0.0, 0.0, 40.0, 20.0), spec.clone()));
                let halves = ink(&tile(&rect(0.0, 0.0, 20.0, 20.0), spec.clone())) + ink(&tile(&rect(20.0, 0.0, 40.0, 20.0), spec));
                // The halves may add a joint along the cut, where a line
                // runs exactly on it; nothing more.
                assert!(((whole - halves) / whole).abs() < 0.02, "{m} at {angle}°: {whole} vs {halves}");
            }
        }
    }

    #[test]
    fn joints_are_drawn_once() {
        // Brick in a 12 × 4 box, bricks 6 × 2: two courses, one joint line
        // between them, the bed lines merged into single strokes.
        let r = rect(0.0, 0.0, 30.0, 10.0);
        let s = tile(&r, Spec::default().option("motif", "brick").option("size", Value::Nums(vec![6.0, 2.0])));
        let mut horizontal: Vec<f64> = s.iter().filter(|l| (l[0].1 - l[1].1).abs() < 1e-9).map(|l| l[0].1).collect();
        horizontal.sort_by(f64::total_cmp);
        let before = horizontal.len();
        horizontal.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        assert_eq!(before, horizontal.len(), "a bed line drawn twice");
    }

    #[test]
    fn inside_lays_only_whole_pieces() {
        let blob = Region::from_ring(&[(0.0, 0.0), (24.0, 3.0), (30.0, 18.0), (12.0, 26.0), (2.0, 15.0)]);
        for m in ["setts", "tiles", "hexagons", "herringbone"] {
            let s = tile(&blob, Spec::default().spacing(1.0).edge(EdgeMode::Inside).option("motif", m).option("gap", 0.3));
            assert!(s.len() > 10, "{m}");
            for l in &s {
                assert_eq!(l.first(), l.last(), "{m}: a piece cut open");
                for p in l {
                    assert!(blob.covers_point(*p), "{m}: {p:?} outside");
                }
            }
        }
    }

    #[test]
    fn a_pat_text_parses_and_draws() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let text = "*MINE, two families\n0, 0,0, 0,1\n90, 0,0, 0,2, 1,-1\n";
        let s = tile(&sq, Spec::default().spacing(1.0).option("pat", text));
        assert!(s.len() > 30, "{}", s.len());
        let ansi = tile(&sq, Spec::default().spacing(1.0).option("pat", "ansi31"));
        assert!(ansi.len() > 20);
        // `pat` wins over `motif`.
        assert_eq!(tile(&sq, Spec::default().spacing(1.0).option("pat", "ANSI31").option("motif", "hexagons")), ansi);
    }

    #[test]
    fn a_custom_motif_tiles() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let plus: Strokes = vec![vec![(-1.0, 0.0), (1.0, 0.0)], vec![(0.0, -1.0), (0.0, 1.0)]];
        let s = tile(&sq, Spec::default().option("motif", plus).option("size", 4.0).option("shift", 0.5));
        // 25 tiles, two strokes each, some cut at the wall.
        assert!(s.len() >= 40, "{}", s.len());
        for p in s.iter().flatten() {
            assert!(sq.covers_point(*p));
        }
    }
}
