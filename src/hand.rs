//! Hand-drawn modifier: takes the exact polylines any fill (or any other
//! source) produces and redraws them the way a hand would — a slow tremble
//! shared by the whole sheet, straight runs that bow a little, a fine tremor,
//! strokes that overshoot or stop short of their ends, rings that don't close
//! cleanly, and (optionally) long lines drawn in several pen strokes and
//! corners drawn as crossing lines.
//!
//! It is a pure `Strokes -> Strokes` post-process: it knows nothing about
//! regions, so it can run after clipping, masking or merging. Output is
//! deterministic for a given seed and independent of stroke order, because
//! each stroke's randomness is keyed on its own geometry.

use crate::geo::{polyline_length, Pt, Strokes};
use crate::noise::Perlin;
use crate::random::Rng;

/// How hand drawn the strokes look. Every length is in millimetres; any
/// effect set to `0` is off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandParams {
    /// Amplitude of a slow tremble shared by every stroke: points that
    /// coincide move together, so joins and junctions stay joined.
    pub wobble: f64,
    /// Wavelength of that tremble.
    pub wavelength: f64,
    /// Amplitude of a fine, per-stroke tremor along the line.
    pub tremor: f64,
    /// Sideways sag of each straight run, as a fraction of its length
    /// (capped at [`BOW_CAP_MM`]).
    pub bow: f64,
    /// How far open strokes overshoot (or, less often, stop short of) their ends.
    pub overshoot: f64,
    /// How far closed rings miss closing: either a gap or an overlap that
    /// drifts off the start.
    pub close: f64,
    /// Longest single pen stroke: longer lines are redrawn as several strokes
    /// that slightly overlap or leave a hairline gap. `0` = one stroke.
    pub reach: f64,
    /// Share (0–1) of strokes the hand goes over a second time, with their
    /// own bows and ends. `0` = every stroke drawn once.
    pub retrace: f64,
    /// How far apart the second pass is set down, at most.
    pub retrace_offset: f64,
    /// Sharp corners are drawn as two lines crossing, each running past the
    /// corner by up to this much (architect's sketch). `0` = corners kept.
    pub corners: f64,
    pub seed: u64,
}

impl Default for HandParams {
    fn default() -> Self {
        HandParams {
            wobble: 0.25,
            wavelength: 15.0,
            tremor: 0.04,
            bow: 0.012,
            overshoot: 0.3,
            close: 0.7,
            reach: 0.0,
            corners: 0.0,
            retrace: 0.0,
            retrace_offset: 0.25,
            seed: 1,
        }
    }
}

impl HandParams {
    /// Looser preset: crossing corners and long lines broken into strokes.
    pub fn sketch() -> Self {
        HandParams {
            wobble: 0.35,
            tremor: 0.05,
            bow: 0.018,
            overshoot: 0.45,
            close: 1.0,
            reach: 40.0,
            corners: 0.8,
            retrace: 0.3,
            ..HandParams::default()
        }
    }
}

/// Largest sideways sag of a straight run, however long it is.
pub const BOW_CAP_MM: f64 = 0.9;
/// Spacing of the points the modifier inserts, so the bends have somewhere to live.
const STEP_MM: f64 = 0.4;
/// Turns sharper than this split a stroke into separately bowed runs.
const RUN_TURN_DEG: f64 = 35.0;
/// Turns sharper than this are drawn as crossing lines when `corners > 0`.
const CROSS_TURN_DEG: f64 = 55.0;
/// Strokes shorter than this (dashes, dots, ticks) are never retraced.
const RETRACE_MIN_MM: f64 = 3.0;
/// A corner is crossed only if both its edges are at least this many times `corners` long.
const CROSS_MIN_EDGE: f64 = 4.0;
/// Wavelength of the fine tremor along a stroke.
const TREMOR_PERIOD_MM: f64 = 2.2;

/// Redraw `strokes` by hand. See [`HandParams`].
pub fn hand(strokes: &Strokes, params: &HandParams) -> Strokes {
    let field = Perlin::new(params.seed ^ 0x4A2D_11C3_9B07_5E21);
    let tremor = Perlin::new(params.seed ^ 0x7E11_0D5A_C3F2_8B64);
    let mut out = Vec::with_capacity(strokes.len());
    for s in strokes {
        hand_stroke(s, params, &field, &tremor, &mut out);
    }
    out
}

fn hand_stroke(raw: &[Pt], p: &HandParams, field: &Perlin, tremor: &Perlin, out: &mut Strokes) {
    let pts = dedup(raw);
    if pts.len() < 2 {
        if let Some(&q) = pts.first() {
            out.push(vec![shift(q, p, field)]);
        }
        return;
    }
    let key = stroke_key(&pts, p.seed);
    let mut rng = Rng::new(key);
    let again = p.retrace > 0.0 && polyline_length(&pts) >= RETRACE_MIN_MM && rng.unit() < p.retrace;
    draw_once(&pts, p, field, tremor, &mut rng, (0.0, 0.0), out);
    if again {
        // Second pass: its own bows, ends and tremor, set down a hair apart.
        let mut rng = Rng::new(key ^ 0x5851_F42D_4C95_7F2D);
        let a = rng.range(0.0, std::f64::consts::TAU);
        let d = rng.range(0.4, 1.0) * p.retrace_offset;
        draw_once(&pts, p, field, tremor, &mut rng, (d * a.cos(), d * a.sin()), out);
    }
}

fn draw_once(pts: &[Pt], p: &HandParams, field: &Perlin, tremor: &Perlin, rng: &mut Rng, offset: Pt, out: &mut Strokes) {
    let pts = pts.to_vec();
    let len = polyline_length(&pts);
    let closed = pts.len() >= 4 && dist(pts[0], pts[pts.len() - 1]) <= 1e-6;
    let rng = rng;

    // 1. Pieces: corners drawn as crossing lines, then long lines broken
    //    into several pen strokes.
    let mut pieces: Vec<(Vec<Pt>, bool)> = Vec::new();
    // Only corners between edges long enough to carry a crossing: tiny
    // notches (row houses, jagged contours) would turn into hair.
    let min_edge = (p.corners * CROSS_MIN_EDGE).max(1.5);
    let runs = if p.corners > 0.0 { split_at_turns(&pts, closed, CROSS_TURN_DEG, min_edge) } else { Vec::new() };
    if runs.len() > 1 || (closed && runs.len() == 1) {
        for run in runs {
            let rl = polyline_length(&run);
            let reach = p.corners.min(0.25 * rl);
            let a = rng.range(0.3, 1.0) * reach;
            let b = rng.range(0.3, 1.0) * reach;
            pieces.push((extend(&run, a, b), false));
        }
    } else {
        pieces.push((pts, closed));
    }
    if p.reach > 0.0 {
        pieces = pieces
            .into_iter()
            .flat_map(|(piece, closed)| break_long(piece, closed, p.reach, rng))
            .collect();
    }

    // 2. Each piece: bow its runs, fix its ends, tremble.
    for (piece, closed) in pieces {
        let pl = polyline_length(&piece);
        if pl < 1e-6 {
            continue;
        }
        let mut q = bend(&piece, closed, p.bow, rng);
        if closed {
            q = misclose(q, p.close.min(0.15 * len), rng);
        } else {
            q = loose_ends(q, p.overshoot.min(0.12 * pl), rng);
        }
        if p.tremor > 0.0 {
            add_tremor(&mut q, p.tremor, tremor, rng.range(0.0, 250.0));
        }
        for v in q.iter_mut() {
            let w = shift(*v, p, field);
            *v = (w.0 + offset.0, w.1 + offset.1);
        }
        out.push(q);
    }
}

/// The shared tremble: a smooth displacement field over the whole sheet.
fn shift(v: Pt, p: &HandParams, field: &Perlin) -> Pt {
    if p.wobble <= 0.0 || p.wavelength <= 0.0 {
        return v;
    }
    let (x, y) = (v.0 / p.wavelength, v.1 / p.wavelength);
    let dx = field.fbm(x, y, 2);
    let dy = field.fbm(x + 31.7, y - 47.3, 2);
    (v.0 + p.wobble * dx, v.1 + p.wobble * dy)
}

/// Densify each straight run and push it sideways by a lopsided arch, so
/// edges bow a little while the corners between them stay put.
fn bend(pts: &[Pt], closed: bool, bow: f64, rng: &mut Rng) -> Vec<Pt> {
    let runs = split_at_turns(pts, closed, RUN_TURN_DEG, 0.0);
    let runs = if runs.is_empty() { vec![pts.to_vec()] } else { runs };
    let mut out: Vec<Pt> = Vec::new();
    for run in runs {
        let mut d = densify(&run, STEP_MM);
        let rl = polyline_length(&d);
        let (a, b) = (d[0], d[d.len() - 1]);
        let chord = dist(a, b);
        // Curvy runs (circles, river bends) have no straight chord to bow from.
        if bow > 0.0 && chord > 0.6 * rl && chord > 1e-6 {
            let amp = rng.range(-1.0, 1.0) * (bow * rl).min(BOW_CAP_MM);
            let skew = rng.range(0.75, 1.33);
            let n = ((a.1 - b.1) / chord, (b.0 - a.0) / chord);
            let flat = d.clone();
            let mut s = 0.0;
            for i in 1..d.len() - 1 {
                s += dist(flat[i - 1], flat[i]);
                let t = (s / rl).clamp(0.0, 1.0).powf(skew);
                let k = amp * (std::f64::consts::PI * t).sin();
                d[i] = (d[i].0 + n.0 * k, d[i].1 + n.1 * k);
            }
        }
        if !out.is_empty() {
            d.remove(0);
        }
        out.extend(d);
    }
    out
}

/// Open strokes: each end overshoots (mostly) or stops short (sometimes).
fn loose_ends(q: Vec<Pt>, reach: f64, rng: &mut Rng) -> Vec<Pt> {
    if reach <= 0.0 {
        return q;
    }
    let a = reach * rng.range(-0.45, 1.0);
    let b = reach * rng.range(-0.45, 1.0);
    extend(&q, a, b)
}

/// Closed rings: stop short of the start, or run past it and drift off.
fn misclose(q: Vec<Pt>, reach: f64, rng: &mut Rng) -> Vec<Pt> {
    if reach <= 0.0 || q.len() < 3 {
        return q;
    }
    let amount = rng.range(0.35, 1.0) * reach;
    if rng.unit() < 0.4 {
        let l = polyline_length(&q);
        return slice(&q, 0.0, l - amount);
    }
    // Overlap: retrace the first `amount` mm, drifting sideways as it goes.
    let head = slice(&q, 0.0, amount);
    let drift = rng.range(-1.0, 1.0) * 0.35 * amount;
    let mut out = q;
    let mut s = 0.0;
    for i in 1..head.len() {
        s += dist(head[i - 1], head[i]);
        let (tx, ty) = unit(head[i - 1], head[i]);
        let k = drift * s / amount;
        out.push((head[i].0 - ty * k, head[i].1 + tx * k));
    }
    out
}

/// A fine tremor across the line, from 1-D noise along its length.
fn add_tremor(q: &mut [Pt], amp: f64, noise: &Perlin, row: f64) {
    if q.len() < 2 {
        return;
    }
    let orig = q.to_vec();
    let mut s = 0.0;
    for i in 0..orig.len() {
        if i > 0 {
            s += dist(orig[i - 1], orig[i]);
        }
        let a = orig[i.saturating_sub(1)];
        let b = orig[(i + 1).min(orig.len() - 1)];
        let (tx, ty) = unit(a, b);
        let k = amp * noise.noise(s / TREMOR_PERIOD_MM, row);
        q[i] = (orig[i].0 - ty * k, orig[i].1 + tx * k);
    }
}

/// Break a line longer than `reach` into several strokes that overlap a
/// little or leave a hairline gap where the pen was lifted.
fn break_long(pts: Vec<Pt>, closed: bool, reach: f64, rng: &mut Rng) -> Vec<(Vec<Pt>, bool)> {
    let l = polyline_length(&pts);
    if l < reach * 1.4 {
        return vec![(pts, closed)];
    }
    let n = (l / reach).round().max(2.0) as usize;
    let mut cuts = vec![0.0];
    for i in 1..n {
        let jitter = rng.range(-0.2, 0.2) * l / n as f64;
        cuts.push(l * i as f64 / n as f64 + jitter);
    }
    cuts.push(l);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        // Each new stroke restarts a little before (overlap) or after (gap)
        // where the last one ended.
        let start = if i == 0 { 0.0 } else { (cuts[i] - rng.range(-0.25, 0.6)).max(0.0) };
        let piece = slice(&pts, start, cuts[i + 1]);
        if piece.len() >= 2 {
            out.push((piece, false));
        }
    }
    out
}

/// Split at vertices that turn more than `deg` between edges at least
/// `min_edge` long. A closed ring is rotated to start at a corner first; with
/// no corners at all it comes back empty.
fn split_at_turns(pts: &[Pt], closed: bool, deg: f64, min_edge: f64) -> Vec<Vec<Pt>> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let ring: Vec<Pt> = if closed { pts[..n - 1].to_vec() } else { pts.to_vec() };
    let m = ring.len();
    let sharp = |i: usize| -> bool {
        let (a, b, c) = if closed {
            (ring[(i + m - 1) % m], ring[i], ring[(i + 1) % m])
        } else {
            (ring[i - 1], ring[i], ring[i + 1])
        };
        turn_deg(a, b, c) > deg && dist(a, b) >= min_edge && dist(b, c) >= min_edge
    };
    let idx: Vec<usize> = if closed { (0..m).filter(|&i| sharp(i)).collect() } else { (1..m - 1).filter(|&i| sharp(i)).collect() };
    if idx.is_empty() {
        return if closed { Vec::new() } else { vec![pts.to_vec()] };
    }
    let mut runs = Vec::new();
    if closed {
        for k in 0..idx.len() {
            let (from, to) = (idx[k], idx[(k + 1) % idx.len()]);
            let mut run = vec![ring[from]];
            let mut i = from;
            loop {
                i = (i + 1) % m;
                run.push(ring[i]);
                if i == to {
                    break;
                }
            }
            runs.push(run);
        }
    } else {
        let mut from = 0;
        for &i in idx.iter().chain(std::iter::once(&(m - 1))) {
            runs.push(ring[from..=i].to_vec());
            from = i;
        }
    }
    runs
}

/// Turning angle at `b`, in degrees (0 = straight on).
fn turn_deg(a: Pt, b: Pt, c: Pt) -> f64 {
    let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
    let (lu, lv) = (u.0.hypot(u.1), v.0.hypot(v.1));
    if lu < 1e-9 || lv < 1e-9 {
        return 0.0;
    }
    let cos = ((u.0 * v.0 + u.1 * v.1) / (lu * lv)).clamp(-1.0, 1.0);
    cos.acos().to_degrees()
}

/// Lengthen (positive) or shorten (negative) both ends along their tangents.
fn extend(pts: &[Pt], start: f64, end: f64) -> Vec<Pt> {
    let l = polyline_length(pts);
    let s0 = if start < 0.0 { (-start).min(0.4 * l) } else { 0.0 };
    let s1 = if end < 0.0 { l - (-end).min(0.4 * l) } else { l };
    let mut q = slice(pts, s0, s1);
    if q.len() < 2 {
        return q;
    }
    let back = (0.3 * polyline_length(&q)).min(1.0);
    if start > 0.0 {
        let (tx, ty) = unit(along(&q, back), q[0]);
        q.insert(0, (q[0].0 + tx * start, q[0].1 + ty * start));
    }
    if end > 0.0 {
        let k = q.len();
        let rev: Vec<Pt> = q.iter().rev().copied().collect();
        let (tx, ty) = unit(along(&rev, back), q[k - 1]);
        q.push((q[k - 1].0 + tx * end, q[k - 1].1 + ty * end));
    }
    q
}

/// The point `s` mm along `pts` from its start (or its last point).
fn along(pts: &[Pt], s: f64) -> Pt {
    let mut left = s;
    for w in pts.windows(2) {
        let l = dist(w[0], w[1]);
        if l >= left && l > 0.0 {
            let t = left / l;
            return (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
        }
        left -= l;
    }
    pts[pts.len() - 1]
}

/// The part of `pts` between arc lengths `s0` and `s1`.
fn slice(pts: &[Pt], s0: f64, s1: f64) -> Vec<Pt> {
    let mut out = Vec::new();
    if s1 <= s0 {
        return out;
    }
    let mut s = 0.0;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let l = dist(a, b);
        let (e0, e1) = (s, s + l);
        if e1 >= s0 && e0 <= s1 && l > 0.0 {
            let at = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if out.is_empty() {
                out.push(at(((s0 - e0) / l).clamp(0.0, 1.0)));
            }
            if e1 <= s1 {
                out.push(b);
            } else {
                out.push(at(((s1 - e0) / l).clamp(0.0, 1.0)));
                break;
            }
        }
        s = e1;
    }
    dedup_tol(&out, 1e-4)
}

/// Insert points so no segment is longer than `step`, keeping every vertex.
fn densify(pts: &[Pt], step: f64) -> Vec<Pt> {
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let n = (dist(a, b) / step).ceil().max(1.0) as usize;
        for i in 1..=n {
            let t = i as f64 / n as f64;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

fn dedup(pts: &[Pt]) -> Vec<Pt> {
    dedup_tol(pts, 1e-9)
}

fn dedup_tol(pts: &[Pt], tol: f64) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(pts.len());
    for &v in pts {
        if out.last().is_none_or(|&l| dist(l, v) > tol) {
            out.push(v);
        } else if out.len() > 1 {
            // Keep the exact final point of a run of near-duplicates.
            *out.last_mut().unwrap() = v;
        }
    }
    out
}

/// Randomness for one stroke, from its own geometry: the same line gets the
/// same hand whichever order (or direction) it comes in.
fn stroke_key(pts: &[Pt], seed: u64) -> u64 {
    let q = |v: Pt| {
        let x = (v.0 * 1000.0).round() as i64 as u64;
        let y = (v.1 * 1000.0).round() as i64 as u64;
        x.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ y.rotate_left(29).wrapping_mul(0xBF58_476D_1CE4_E5B9)
    };
    let (a, b) = (q(pts[0]), q(pts[pts.len() - 1]));
    let (lo, hi) = (a.min(b), a.max(b));
    seed.wrapping_mul(0x94D0_49BB_1331_11EB) ^ lo ^ hi.rotate_left(17) ^ ((pts.len() as u64) << 48)
}

fn dist(a: Pt, b: Pt) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

fn unit(a: Pt, b: Pt) -> Pt {
    let l = dist(a, b);
    if l < 1e-12 {
        (1.0, 0.0)
    } else {
        ((b.0 - a.0) / l, (b.1 - a.1) / l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Vec<Pt> {
        vec![(0.0, 0.0), (s, 0.0), (s, s), (0.0, s), (0.0, 0.0)]
    }

    fn max_dev(a: &[Pt], b: &[Pt]) -> f64 {
        // Furthest any output point sits from the nearest input vertex-densified point.
        let d = densify(b, 0.05);
        a.iter()
            .map(|p| d.iter().map(|q| dist(*p, *q)).fold(f64::MAX, f64::min))
            .fold(0.0, f64::max)
    }

    #[test]
    fn stays_close_to_the_original() {
        let p = HandParams::default();
        let input = vec![square(30.0), vec![(0.0, 40.0), (60.0, 40.0)]];
        let out = hand(&input, &p);
        assert_eq!(out.len(), 2);
        for (o, i) in out.iter().zip(&input) {
            let bound = p.wobble * 1.5 + BOW_CAP_MM + p.tremor + p.overshoot + p.close;
            assert!(max_dev(o, i) < bound, "deviation {} > {}", max_dev(o, i), bound);
            assert_ne!(o, i);
        }
    }

    #[test]
    fn deterministic_and_order_independent() {
        let p = HandParams::default();
        let a = vec![square(10.0), vec![(0.0, 20.0), (40.0, 22.0)]];
        let b = vec![a[1].clone(), a[0].clone()];
        let (oa, ob) = (hand(&a, &p), hand(&b, &p));
        assert_eq!(oa, hand(&a, &p));
        assert_eq!(oa[0], ob[1]);
        assert_eq!(oa[1], ob[0]);
    }

    #[test]
    fn shared_tremble_keeps_joins_joined() {
        let p = HandParams { tremor: 0.0, overshoot: 0.0, bow: 0.0, ..HandParams::default() };
        let out = hand(&vec![vec![(0.0, 0.0), (20.0, 5.0)], vec![(20.0, 5.0), (35.0, -3.0)]], &p);
        assert!(dist(*out[0].last().unwrap(), out[1][0]) < 1e-9);
    }

    #[test]
    fn crossing_corners_split_a_ring_into_edges() {
        let p = HandParams { corners: 1.0, ..HandParams::default() };
        let out = hand(&vec![square(20.0)], &p);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn retrace_draws_strokes_twice() {
        let p = HandParams { retrace: 1.0, ..HandParams::default() };
        let out = hand(&vec![vec![(0.0, 0.0), (30.0, 0.0)], vec![(0.0, 5.0), (1.0, 5.0)]], &p);
        assert_eq!(out.len(), 3);
        assert_ne!(out[0], out[1]);
    }

    #[test]
    fn small_notches_are_not_crossed() {
        let p = HandParams { corners: 1.0, ..HandParams::default() };
        let out = hand(&vec![square(2.0)], &p);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn long_lines_break_into_strokes() {
        let p = HandParams { reach: 20.0, ..HandParams::default() };
        let out = hand(&vec![vec![(0.0, 0.0), (100.0, 0.0)]], &p);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn all_off_only_densifies() {
        let p = HandParams { wobble: 0.0, tremor: 0.0, bow: 0.0, overshoot: 0.0, close: 0.0, ..HandParams::default() };
        let input = vec![square(10.0)];
        let out = hand(&input, &p);
        assert!(max_dev(&out[0], &input[0]) < 1e-9);
    }
}
