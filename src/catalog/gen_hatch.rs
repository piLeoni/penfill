//! `hatch`: parallel lines, and everything that is parallel lines underneath
//! — a grid or a triad (`cross`), one serpentine stroke (`join`), rows of
//! dashes (`dash`, `dash_jitter`: coal, shale, clay), bedding that bends
//! together (`wobble`), uneven gaps (`vary`: layered soil), and tone
//! (`density`): lines break off where the area is lighter, and cross layers
//! drop out one by one, the way an engraver builds a shadow.
//!
//! Lines are built in the anchored frame (`u` along, `v` across, line `j`
//! at `v = j · spacing`), so every random choice — a line's offset, a dash's
//! length, a threshold — is a function of `j` and the dash's place along
//! `u`, never of the region: two areas side by side share one hatch.

use super::schema::{Args, Default, Kind, Opt, CONTRAST, DENSITY, FEATURE, TOWARD};
use super::tone::{rank, Tone};
use super::Generator;
use crate::anchor::{indices, Frame};
use crate::geo::{Pt, Strokes};
use crate::noise::Perlin;
use crate::pattern::{Canvas, Tools};
use crate::random::Rng;

const CROSS: Opt = Opt {
    key: "cross",
    kind: Kind::Count { min: 1, max: 3 },
    default: Default::Num(1.0),
    doc: "Families of lines: 1 parallel lines, 2 a grid (crossing at right angles), 3 a triad (at 60°).",
};
const JOIN: Opt = Opt {
    key: "join",
    kind: Kind::Flag,
    default: Default::Flag(false),
    doc: "Join the lines end to end into serpentines, so the pen lifts as little as possible (one family, no dashes).",
};
const DASH: Opt = Opt {
    key: "dash",
    kind: Kind::Lengths,
    default: Default::None,
    doc: "Break the lines into dashes: `[on, off]` in mm.",
};
const DASH_JITTER: Opt = Opt {
    key: "dash_jitter",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::Num(0.0),
    doc: "How much dash and gap lengths differ, and how far each row's dashes are offset, 0 to 1.",
};
const WOBBLE: Opt = Opt {
    key: "wobble",
    kind: Kind::Length,
    default: Default::Num(0.0),
    doc: "How far the lines bend away from straight, in mm. Neighbouring lines bend together, like bedding.",
};
const WOBBLE_LENGTH: Opt = Opt {
    key: "wobble_length",
    kind: Kind::Length,
    default: Default::Rel(14.0),
    doc: "The length of one bend along the lines, in mm.",
};

const VARY: Opt = Opt {
    key: "vary",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::Num(0.0),
    doc: "How uneven the gaps between lines are, 0 (even) to 1: layered soil, worn bedding.",
};

pub static HATCH: Generator = Generator {
    name: "hatch",
    about: "Parallel lines; crossed into a grid or a triad, joined into serpentines, dashed, bent, unevenly spaced, or thinned by tone.",
    spacing: 0.6,
    angle: 0.0,
    options: &[&CROSS, &JOIN, &DASH, &DASH_JITTER, &WOBBLE, &WOBBLE_LENGTH, &VARY, &DENSITY, &CONTRAST, &FEATURE, &TOWARD],
    own_mask: false,
    inset: 0.0,
    draw,
};

fn draw(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    let s = c.spacing;
    let cross = a.count("cross").clamp(1, 3) as usize;
    let dash = a.pair("dash").filter(|(on, off)| *on > 0.0 && *off > 0.0);
    let wobble = a.num("wobble").max(0.0);
    let vary = a.num("vary");
    let tone = Tone::new(a, c.region, c.seed);

    // The fast paths: plain lines and serpentines, through the sweep code
    // (exact, and no resampling).
    let plain = dash.is_none() && wobble == 0.0 && vary == 0.0 && tone.is_even();
    if plain && cross == 1 && a.flag("join") {
        return crate::zigzag::fill_zigzag_inset(&c.limit, s, c.angle, None, c.anchor);
    }
    if plain {
        return (0..cross)
            .flat_map(|k| crate::sweep::sweep_anchored(&c.limit, s, c.angle + k as f64 * 180.0 / cross as f64, c.anchor))
            .collect();
    }

    let mut out = Strokes::new();
    for k in 0..cross {
        let angle = c.angle + k as f64 * 180.0 / cross as f64;
        let frame = t.frame(angle);
        let family = Family { c, frame, s, k, cross, wobble, wobble_length: a.num("wobble_length").max(s), vary, noise: Perlin::new(c.seed ^ 0xbed5 ^ k as u64) };
        for (j, line) in family.lines() {
            for piece in c.limit.clip_polyline(&line) {
                let pieces = match dash {
                    Some((on, off)) => dashes(&piece, &frame, j, on, off, a.num("dash_jitter"), c.seed ^ k as u64),
                    None => vec![piece],
                };
                for p in pieces {
                    if tone.is_even() {
                        out.push(p);
                    } else {
                        // Line j of family k is kept where the tone reaches its
                        // threshold: the first family fades line by line, the
                        // later ones only come in where it is darker.
                        let threshold = (k as f64 + rank(j, k as i64, c.seed ^ 0x7a)) / cross as f64;
                        out.extend(keep_where(&p, s * 0.5, |q| tone.at(q) >= threshold));
                    }
                }
            }
        }
    }
    out
}

/// One family of lines, in its frame.
struct Family<'a> {
    c: &'a Canvas<'a>,
    frame: Frame,
    s: f64,
    k: usize,
    cross: usize,
    wobble: f64,
    wobble_length: f64,
    vary: f64,
    noise: Perlin,
}

impl Family<'_> {
    /// Each line `(j, polyline)` across the limit, before clipping.
    fn lines(&self) -> Vec<(i64, Vec<Pt>)> {
        let margin = self.s * 2.0 + self.wobble;
        let (u0, u1, v0, v1) = self.frame.span(&self.c.limit, margin);
        let step = if self.wobble > 0.0 { (self.wobble_length / 10.0).clamp(0.2, 2.0) } else { u1 - u0 };
        let n = ((u1 - u0) / step).ceil().max(1.0) as usize;
        let mut out = Vec::new();
        for j in indices(v0, v1, self.s) {
            let mut v = j as f64 * self.s;
            if self.vary > 0.0 {
                // A line moves within its own gap, so lines never swap order.
                let mut rng = Rng::new(crate::anchor::cell_seed(self.c.seed ^ 0x5a ^ self.k as u64, 0, j));
                v += rng.range(-0.45, 0.45) * self.vary * self.s;
            }
            let pts = (0..=n)
                .map(|i| {
                    let u = u0 + (u1 - u0) * i as f64 / n as f64;
                    let mut dv = 0.0;
                    if self.wobble > 0.0 {
                        // Shared bend (the bed) plus a little of each line's own.
                        let shared = self.noise.fbm(u / self.wobble_length, v / (self.wobble_length * 3.0), 2);
                        let own = self.noise.noise(u / (self.wobble_length * 0.6), j as f64 * 0.73 + 100.0);
                        dv = self.wobble * (0.8 * shared + 0.35 * own);
                    }
                    self.frame.world((u, v + dv))
                })
                .collect();
            out.push((j, pts));
        }
        let _ = self.cross;
        out
    }
}

/// `line` cut into dashes `on` long with `off` gaps, measured along the
/// frame's `u` so a dash falls at the same place whichever region draws it.
fn dashes(line: &[Pt], frame: &Frame, j: i64, on: f64, off: f64, jitter: f64, seed: u64) -> Strokes {
    let period = on + off;
    let row = Rng::new(crate::anchor::cell_seed(seed ^ 0xda5, 0, j)).unit();
    let phase = if jitter > 0.0 { row * period } else { 0.0 };
    // The dash in period `i` of row `j`: its start and end along u.
    let dash = |i: i64| -> (f64, f64) {
        let base = i as f64 * period - phase;
        if jitter == 0.0 {
            return (base, base + on);
        }
        let mut rng = Rng::new(crate::anchor::cell_seed(seed ^ 0xda6, i, j));
        let len = on * (1.0 + jitter * rng.range(-0.6, 0.6));
        let start = base + off * jitter * rng.range(-0.45, 0.45);
        (start, start + len.max(on * 0.15))
    };
    let us: Vec<f64> = line.iter().map(|p| frame.to_frame(*p).0).collect();
    let (lo, hi) = us.iter().fold((f64::MAX, f64::MIN), |(lo, hi), u| (lo.min(*u), hi.max(*u)));
    let mut out = Strokes::new();
    let first = ((lo + phase) / period).floor() as i64 - 1;
    let last = ((hi + phase) / period).ceil() as i64 + 1;
    for i in first..=last {
        let (a, b) = dash(i);
        let piece = between(line, &us, a, b);
        if piece.len() >= 2 {
            out.push(piece);
        }
    }
    out
}

/// The part of `line` whose `u` (given per point, increasing or decreasing)
/// lies in `a..b`.
fn between(line: &[Pt], us: &[f64], a: f64, b: f64) -> Vec<Pt> {
    let forward = us.last() >= us.first();
    let mut out = Vec::new();
    let inside = |u: f64| u >= a && u <= b;
    let lerp = |p: Pt, q: Pt, t: f64| (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t);
    for w in 0..line.len().saturating_sub(1) {
        let (p, q, up, uq) = (line[w], line[w + 1], us[w], us[w + 1]);
        if inside(up) && out.is_empty() {
            out.push(p);
        }
        // Enter or leave the window on this segment.
        for edge in if forward == (uq >= up) { [a, b] } else { [b, a] } {
            if (up - edge) * (uq - edge) < 0.0 {
                let t = (edge - up) / (uq - up);
                out.push(lerp(p, q, t));
            }
        }
        if inside(uq) {
            out.push(q);
        }
    }
    out.dedup();
    out
}

/// The runs of `line` where `keep` holds, sampled every `step` mm.
fn keep_where(line: &[Pt], step: f64, keep: impl Fn(Pt) -> bool) -> Strokes {
    let pts = crate::fray::resample(line, step);
    let mut out = Strokes::new();
    let mut run: Vec<Pt> = Vec::new();
    for p in pts {
        if keep(p) {
            run.push(p);
        } else if !run.is_empty() {
            if run.len() >= 2 {
                out.push(std::mem::take(&mut run));
            } else {
                run.clear();
            }
        }
    }
    if run.len() >= 2 {
        out.push(run);
    }
    out
}
