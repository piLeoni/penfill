//! The symbol library: the marks `scatter` places, drawn the way a map
//! engraver draws them at 1–3 mm with a 0.2–0.3 mm pen.
//!
//! Every mark is centred on the origin, about `size` mm across (it always
//! fits the `size × size` box), and drawn **upright**: up is negative y, as
//! on the page, so a scatter that keeps marks aligned keeps trees standing.
//! A mark is a handful of pen strokes (open polylines; a closed outline
//! repeats its first point). Each instance takes a little of its own shape
//! from the `rng` it is given — a blade longer, a lobe fuller, an arm a
//! touch off square — so a field of them reads as drawn, not stamped.
//!
//! The marks, by family:
//!
//! - **points and strokes**: `dot`, `tick`, `dash`, `plus`, `cross`, `ring`,
//!   `ring-dot`;
//! - **stones**: `pebble` (rounded outline), `blob` (a solid blot), `shard`
//!   (an angular fragment), `rock` (a boulder with its shaded facet);
//! - **geology**: `tee`, `vee` (the volcanic v), `bracket`;
//! - **plants**: `tuft` (grass), `reed` (marsh), `tree` (deciduous), `conifer`,
//!   `shrub`, `sprout`, `leaf`, `vine` (vineyard stock), `flower`;
//! - **built**: `grave` (the cemetery cross).

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::geo::{Pt, Strokes};
use crate::random::Rng;

/// Every named mark.
pub const NAMES: &[&str] = &[
    "dot", "tick", "dash", "plus", "cross", "ring", "ring-dot", "pebble", "blob", "shard", "rock", "tee", "vee", "bracket", "tuft",
    "reed", "tree", "conifer", "shrub", "sprout", "leaf", "vine", "flower", "grave",
];

/// The pen the marks are designed for, in mm: solid marks (a large `dot`, a
/// `blob`) are filled with turns closer than this, and outlines keep their
/// detail coarser than it so they do not clog.
const PEN: f64 = 0.25;

/// Mark `name` (one of [`NAMES`]) about `size` mm across, centred on the
/// origin and upright. An unknown name draws a `dot`.
pub fn draw(name: &str, size: f64, rng: &mut Rng) -> Strokes {
    if !(size > 0.0 && size.is_finite()) {
        return Strokes::new();
    }
    let s = size;
    let mut out = match name {
        "tick" => tick(s, rng),
        "dash" => dash(s, rng),
        "plus" => plus(s, rng),
        "cross" => cross(s, rng),
        "ring" => vec![ring(0.46 * s, rng)],
        "ring-dot" => ring_dot(s, rng),
        "pebble" => pebble(s, rng),
        "blob" => blob(s, rng),
        "shard" => shard(s, rng),
        "rock" => rock(s, rng),
        "tee" => tee(s, rng),
        "vee" => vee(s, rng),
        "bracket" => bracket(s, rng),
        "tuft" => tuft(s, rng),
        "reed" => reed(s, rng),
        "tree" => tree(s, rng),
        "conifer" => conifer(s, rng),
        "shrub" => shrub(s, rng),
        "sprout" => sprout(s, rng),
        "leaf" => leaf(s, rng),
        "vine" => vine(s, rng),
        "flower" => flower(s, rng),
        "grave" => grave(s, rng),
        _ => dot(s, rng),
    };
    fit(&mut out, s / 2.0);
    out
}

/// The farthest any point of `strokes` lies from the origin.
pub fn radius(strokes: &Strokes) -> f64 {
    strokes.iter().flatten().map(|p| p.0.hypot(p.1)).fold(0.0, f64::max)
}

// ---------------------------------------------------------------------------
// Points and strokes
// ---------------------------------------------------------------------------

/// A pen dot. Up to two pen widths it is the pen's own dot: a stroke so
/// short it reads round at any pen width. Larger, a solid round dot — a
/// tight spiral the pen fills in.
fn dot(s: f64, rng: &mut Rng) -> Strokes {
    if s <= 2.0 * PEN {
        let (sin, cos) = rng.range(0.0, PI).sin_cos();
        let l = 0.02 + 0.04 * (s / (2.0 * PEN));
        return vec![vec![(-cos * l / 2.0, -sin * l / 2.0), (cos * l / 2.0, sin * l / 2.0)]];
    }
    let r = s / 2.0 - PEN / 2.0;
    vec![solid(|_| r, rng.range(0.0, TAU))]
}

/// A short upright stroke, a hair off vertical.
fn tick(s: f64, rng: &mut Rng) -> Strokes {
    let l = 0.82 * s * rng.range(0.9, 1.0);
    let a = rng.range(-0.07, 0.07);
    vec![along(a + FRAC_PI_2, l, (0.0, 0.0))]
}

/// A horizontal dash the full width.
fn dash(s: f64, rng: &mut Rng) -> Strokes {
    let l = s * rng.range(0.86, 1.0);
    let a = rng.range(-0.04, 0.04);
    vec![along(a, l, (0.0, rng.range(-0.03, 0.03) * s))]
}

/// `+`: two arms at right angles, each a touch its own length.
fn plus(s: f64, rng: &mut Rng) -> Strokes {
    let twist = rng.range(-0.05, 0.05);
    let v = 0.9 * s * rng.range(0.92, 1.0);
    let h = 0.9 * s * rng.range(0.92, 1.0);
    vec![along(FRAC_PI_2 + twist, v, (0.0, 0.0)), along(twist + rng.range(-0.03, 0.03), h, (0.0, 0.0))]
}

/// `×`: the plus on its corners.
fn cross(s: f64, rng: &mut Rng) -> Strokes {
    let twist = rng.range(-0.06, 0.06);
    let a = 1.22 * s * rng.range(0.92, 1.0);
    let b = 1.22 * s * rng.range(0.92, 1.0);
    vec![along(PI / 4.0 + twist, a, (0.0, 0.0)), along(-PI / 4.0 + twist + rng.range(-0.04, 0.04), b, (0.0, 0.0))]
}

/// A small circle of radius `r`, a little out of true, its pen join at a
/// random place so a field of rings shows no seam.
fn ring(r: f64, rng: &mut Rng) -> Vec<Pt> {
    let (p1, p2) = (rng.range(0.0, TAU), rng.range(0.0, TAU));
    let (w1, w2) = (rng.range(0.0, 0.05), rng.range(0.0, 0.03));
    polar(steps_for(r), rng.range(0.0, TAU), |t| r * (1.0 + w1 * (2.0 * t + p1).sin() + w2 * (3.0 * t + p2).sin()))
}

fn ring_dot(s: f64, rng: &mut Rng) -> Strokes {
    let mut out = vec![ring(0.46 * s, rng)];
    out.extend(dot(PEN, rng));
    out
}

// ---------------------------------------------------------------------------
// Stones
// ---------------------------------------------------------------------------

/// A water-worn pebble: a rounded outline, longer one way than the other,
/// never quite an ellipse.
fn pebble(s: f64, rng: &mut Rng) -> Strokes {
    let a = 0.47 * s;
    let b = a * rng.range(0.6, 0.9);
    let tilt = rng.range(-0.3, 0.3);
    let (p2, p3) = (rng.range(0.0, TAU), rng.range(0.0, TAU));
    let (w2, w3) = (rng.range(0.02, 0.07), rng.range(0.0, 0.05));
    let n = steps_for(a);
    let ring = polar(n, rng.range(0.0, TAU), |t| {
        let e = a * b / ((b * t.cos()).powi(2) + (a * t.sin()).powi(2)).sqrt();
        e * (1.0 + w2 * (2.0 * t + p2).sin() + w3 * (3.0 * t + p3).sin())
    });
    vec![turn(&ring, tilt)]
}

/// A solid blot of ink: an irregular rounded shape, filled.
fn blob(s: f64, rng: &mut Rng) -> Strokes {
    if s <= 2.0 * PEN {
        return dot(s, rng);
    }
    let r = s / 2.0 - PEN / 2.0;
    let (p2, p3) = (rng.range(0.0, TAU), rng.range(0.0, TAU));
    let (w2, w3) = (rng.range(0.06, 0.14), rng.range(0.03, 0.08));
    let shape = move |t: f64| r * (1.0 - w2 - w3 + w2 * (2.0 * t + p2).sin() + w3 * (3.0 * t + p3).sin());
    vec![solid(shape, rng.range(0.0, TAU))]
}

/// An angular fragment: four to six straight sides, sharp corners, never a
/// sliver.
fn shard(s: f64, rng: &mut Rng) -> Strokes {
    let n = 4 + (rng.next_u64() % 3) as usize;
    let r = 0.5 * s;
    let phase = rng.range(0.0, TAU);
    let stretch = rng.range(1.0, 1.35);
    let mut pts: Vec<Pt> = (0..n)
        .map(|k| {
            let t = phase + TAU * (k as f64 + rng.range(-0.28, 0.28)) / n as f64;
            let rr = r * rng.range(0.6, 1.0);
            (rr * t.cos() * stretch, rr * t.sin() / stretch)
        })
        .collect();
    pts.push(pts[0]);
    vec![pts]
}

/// A boulder in profile: an angular crown on a flat bed, with the facet
/// away from the light ruled off and shaded with a couple of short strokes.
fn rock(s: f64, rng: &mut Rng) -> Strokes {
    let h = s / 2.0;
    let bed = 0.36 * s;
    let w = h * rng.range(0.85, 1.0);
    // The crown: corners from the left end of the bed over to the right.
    let n = 3 + (rng.next_u64() % 2) as usize;
    let mut crown = vec![(-w, bed)];
    for k in 0..n {
        let t = PI - PI * (k as f64 + 0.5 + rng.range(-0.22, 0.22)) / n as f64;
        let rr = rng.range(0.78, 1.0);
        crown.push((w * rr * t.cos(), bed - (bed + h * 0.95) * rr * t.sin()));
    }
    crown.push((w * rng.range(0.85, 1.0), bed));
    // The summit: the highest corner. The facet line drops from it to the bed.
    let top = crown[1..crown.len() - 1].iter().copied().fold((0.0, f64::MAX), |m, p| if p.1 < m.1 { p } else { m });
    let foot = (top.0 + w * rng.range(0.15, 0.3), bed);
    let mut outline = crown.clone();
    outline.push(crown[0]);
    let mut out = vec![outline, vec![top, foot]];
    // Shading: short strokes parallel to the facet, in the shaded side.
    let right = *crown.last().unwrap();
    for k in 1..=2 {
        let f = k as f64 / 3.0;
        let a = lerp(top, right, f * 0.9);
        let b = lerp(foot, right, f * 0.9);
        out.push(vec![lerp(a, b, 0.2), lerp(a, b, 0.85)]);
    }
    out
}

// ---------------------------------------------------------------------------
// Geology
// ---------------------------------------------------------------------------

/// `T`: a cap over a stem.
fn tee(s: f64, rng: &mut Rng) -> Strokes {
    let h = 0.44 * s;
    let w = 0.42 * s * rng.range(0.88, 1.0);
    let lean = rng.range(-0.05, 0.05);
    let top = -h;
    let foot = (lean * 2.0 * h, h * rng.range(0.9, 1.0));
    vec![vec![(-w, top + rng.range(-0.02, 0.02) * s), (w, top + rng.range(-0.02, 0.02) * s)], vec![(0.0, top), foot]]
}

/// The volcanic `v`: two arms opening upward from a point.
fn vee(s: f64, rng: &mut Rng) -> Strokes {
    let h = 0.4 * s;
    let foot = (rng.range(-0.03, 0.03) * s, h);
    let l = (-0.36 * s * rng.range(0.9, 1.05), -h * rng.range(0.9, 1.0));
    let r = (0.36 * s * rng.range(0.9, 1.05), -h * rng.range(0.9, 1.0));
    vec![vec![l, foot, r]]
}

/// `└`: an upright and a foot, one stroke.
fn bracket(s: f64, rng: &mut Rng) -> Strokes {
    let a = 0.42 * s;
    let up = a * rng.range(0.85, 1.0);
    let along = a * rng.range(0.85, 1.0);
    vec![vec![(-a, -up), (-a, a), (along, a)]]
}

// ---------------------------------------------------------------------------
// Plants
// ---------------------------------------------------------------------------

/// A grass tuft: three to five blades (as many as the size allows) springing from a short root and
/// fanning up and out, the middle ones tallest, every blade bowing away
/// from the centre as it rises. The blades are long and the root has some
/// width, so a field of tufts reads as grass, not as arrowheads.
fn tuft(s: f64, rng: &mut Rng) -> Strokes {
    // As many blades as stay a pen width apart at mid height.
    let most = ((s / (5.0 * PEN)).floor() as usize).clamp(3, 5);
    let n = if most > 3 && rng.unit() < 0.4 { most - 1 } else { most };
    let root = (rng.range(-0.03, 0.03) * s, 0.47 * s);
    let width = rng.range(0.14, 0.2) * s;
    let spread = rng.range(0.6, 0.8);
    let lean = rng.range(-0.1, 0.1);
    (0..n)
        .map(|k| {
            let f = 2.0 * k as f64 / (n - 1) as f64 - 1.0 + rng.range(-0.25, 0.25) / n as f64;
            let len = 0.95 * s * (1.0 - 0.24 * f * f) * rng.range(0.72, 1.0);
            let a0 = lean + f * spread * 0.35;
            let a1 = lean + f * spread * (1.0 + rng.range(0.0, 0.3)) + rng.range(-0.06, 0.06);
            let base = (root.0 + f * width, root.1 + rng.range(-0.02, 0.02) * s);
            blade(base, a0, a1, len)
        })
        .collect()
}

/// The marsh symbol: a short line of water with a tuft of upright reeds
/// standing on it, and sometimes a second, shorter line of water below.
fn reed(s: f64, rng: &mut Rng) -> Strokes {
    let water = 0.3 * s;
    let half = 0.48 * s * rng.range(0.85, 1.0);
    let shift = rng.range(-0.04, 0.04) * s;
    let mut out = vec![vec![(-half + shift, water), (half + shift, water)]];
    let n = if s >= 8.0 * PEN && rng.unit() < 0.35 { 4 } else { 3 };
    for k in 0..n {
        let f = 2.0 * k as f64 / (n - 1) as f64 - 1.0;
        let len = (water + 0.46 * s) * (1.0 - 0.36 * f.abs()) * rng.range(0.85, 1.0);
        let a = f * 0.26 + rng.range(-0.05, 0.05);
        let base = (shift + f * 0.2 * s, water);
        out.push(blade(base, a * 0.6, a * 1.3, len));
    }
    if rng.unit() < 0.5 {
        let y = water + 0.17 * s;
        let x = if rng.coin() { 0.12 * s } else { -0.12 * s } + shift;
        let l = 0.24 * s * rng.range(0.8, 1.0);
        out.push(vec![(x - l, y), (x + l, y)]);
    }
    out
}

/// A deciduous tree, the topographic woodland symbol: a round scalloped
/// crown on a short trunk. The lobes are kept wider than the pen.
fn tree(s: f64, rng: &mut Rng) -> Strokes {
    let r = 0.33 * s;
    let centre = (0.0, -0.13 * s);
    let n = ((TAU * r / (3.4 * PEN)).round() as usize).clamp(5, 9);
    let crown = scalloped(r, n, 0.22, rng);
    let crown: Vec<Pt> = crown.iter().map(|p| (p.0 + centre.0, p.1 + centre.1)).collect();
    // The trunk leaves the crown at its lowest point and stands a little
    // off true.
    let bottom = crown.iter().copied().filter(|p| p.0.abs() < 0.2 * r).fold(centre, |m, p| if p.1 > m.1 { p } else { m });
    let foot = (rng.range(-0.02, 0.02) * s, 0.5 * s * rng.range(0.9, 1.0));
    vec![crown, vec![(bottom.0 * 0.3, bottom.1), foot]]
}

/// A conifer: tiers of drooping branches down a straight stem, narrow at
/// the top and widest at the foot, the stem showing below.
fn conifer(s: f64, rng: &mut Rng) -> Strokes {
    let h = s / 2.0;
    let tiers = if s < 1.4 { 2 } else { 3 };
    let top = -h * rng.range(0.94, 1.0);
    let foot = h;
    let reach = 0.82 * s; // apex of the top tier to the tips of the lowest
    let mut out = vec![vec![(0.0, top), (rng.range(-0.015, 0.015) * s, foot)]];
    for k in 0..tiers {
        let f = (k as f64 + 1.0) / tiers as f64;
        let apex = top + reach * (k as f64 / (tiers as f64 + 0.6));
        let drop = 0.3 * s * rng.range(0.9, 1.05);
        let w = 0.4 * s * (0.35 + 0.65 * f);
        let wl = w * rng.range(0.9, 1.0);
        let wr = w * rng.range(0.9, 1.0);
        // Each branch bows a little, convex upward.
        let left = bow((0.0, apex), (-wl, apex + drop), 0.06 * s);
        let right = bow((0.0, apex), (wr, apex + drop * rng.range(0.92, 1.05)), -0.06 * s);
        let mut tier: Vec<Pt> = left.into_iter().rev().collect();
        tier.extend(right.into_iter().skip(1));
        out.push(tier);
    }
    out
}

/// A shrub: a low irregular cloud of a few round lobes, flat underneath.
fn shrub(s: f64, rng: &mut Rng) -> Strokes {
    let r = 0.46 * s;
    let n = ((TAU * r / (4.5 * PEN)).round() as usize).clamp(4, 7);
    let cloud = scalloped(r, n, 0.3, rng);
    let flat = 0.24 * s;
    let squash = rng.range(0.66, 0.78);
    let ring: Vec<Pt> = cloud.iter().map(|p| (p.0, (p.1 * squash + 0.04 * s).min(flat))).collect();
    vec![ring]
}

/// A sprout: a short stem opening into two leaves.
fn sprout(s: f64, rng: &mut Rng) -> Strokes {
    let fork = (rng.range(-0.03, 0.03) * s, 0.02 * s);
    let foot = (0.0, 0.5 * s);
    let mut out = vec![bow(foot, fork, rng.range(-0.04, 0.04) * s)];
    for side in [-1.0, 1.0] {
        let a = side * rng.range(0.75, 1.0);
        let len = 0.5 * s * rng.range(0.85, 1.0);
        let tip = (fork.0 + len * a.sin(), fork.1 - len * a.cos());
        out.push(almond(fork, tip, 0.34));
    }
    out
}

/// A leaf: an almond blade on a short stalk, the midrib drawn.
fn leaf(s: f64, rng: &mut Rng) -> Strokes {
    let tilt = rng.range(-0.25, 0.25);
    let base = (0.0, 0.3 * s);
    let tip = (0.0, -0.5 * s);
    let stalk = (0.0, 0.5 * s);
    let outline = almond(base, tip, rng.range(0.3, 0.4));
    let rib = vec![stalk, base, lerp(base, tip, 0.78)];
    vec![turn(&outline, tilt), turn(&rib, tilt)]
}

/// A vine stock, the vineyard symbol: a short trunk trained into two arms,
/// each ending in a curl.
fn vine(s: f64, rng: &mut Rng) -> Strokes {
    let head = (rng.range(-0.02, 0.02) * s, -0.08 * s);
    let mut out = vec![bow((0.0, 0.5 * s), head, rng.range(-0.03, 0.03) * s)];
    for side in [-1.0, 1.0] {
        let reach = 0.3 * s * rng.range(0.9, 1.05);
        let rise = 0.2 * s * rng.range(0.85, 1.1);
        let end = (head.0 + side * reach, head.1 - rise);
        let mut arm = bow(head, end, side * -0.05 * s);
        // The curl: over the end and down, winding in.
        let r0 = 0.11 * s;
        let c = (end.0, end.1 + r0);
        let turns = rng.range(0.8, 1.05);
        for k in 1..=12 {
            let t = k as f64 / 12.0;
            let a = -std::f64::consts::FRAC_PI_2 + side * TAU * turns * t;
            let r = r0 * (1.0 - 0.45 * t);
            arm.push((c.0 + r * a.cos(), c.1 + r * a.sin()));
        }
        out.push(arm);
    }
    out
}

/// A five-petalled flower in one stroke (a rose curve).
fn flower(s: f64, rng: &mut Rng) -> Strokes {
    let r = 0.48 * s;
    let phase = rng.range(0.0, TAU / 5.0);
    let n = 80;
    let mut petals: Vec<Pt> = (0..=n)
        .map(|k| {
            let t = PI * k as f64 / n as f64;
            let c = (5.0 * t).cos();
            let rr = r * c.signum() * c.abs().powf(0.8);
            let a = t + phase;
            (rr * a.cos(), rr * a.sin())
        })
        .collect();
    petals.dedup();
    vec![petals]
}

// ---------------------------------------------------------------------------
// Built
// ---------------------------------------------------------------------------

/// The cemetery cross: a small latin cross, the bar a third of the way down.
fn grave(s: f64, rng: &mut Rng) -> Strokes {
    let h = 0.5 * s;
    let bar = -h + 0.32 * s;
    let w = 0.3 * s * rng.range(0.95, 1.0);
    vec![vec![(0.0, -h), (0.0, h)], vec![(-w, bar), (w, bar)]]
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// A straight stroke `len` long through `c` at `angle` radians.
fn along(angle: f64, len: f64, c: Pt) -> Vec<Pt> {
    let (sin, cos) = angle.sin_cos();
    let h = len / 2.0;
    vec![(c.0 - cos * h, c.1 - sin * h), (c.0 + cos * h, c.1 + sin * h)]
}

/// A closed curve of `n` sides: radius `r(t)` at angle `t`, starting at `start`.
fn polar(n: usize, start: f64, r: impl Fn(f64) -> f64) -> Vec<Pt> {
    let mut pts: Vec<Pt> = (0..n)
        .map(|k| {
            let t = start + TAU * k as f64 / n as f64;
            let rr = r(t);
            (rr * t.cos(), rr * t.sin())
        })
        .collect();
    pts.push(pts[0]);
    pts
}

/// Sides for a smooth outline of radius `r`: about one per fifth of a
/// millimetre, never fewer than ten.
fn steps_for(r: f64) -> usize {
    ((TAU * r / 0.2).ceil() as usize).clamp(10, 48)
}

/// A solid shape: its outline `r(t)` once round, then a spiral winding in
/// a little under a pen width a turn, so the pen fills it. One stroke.
fn solid(r: impl Fn(f64) -> f64, start: f64) -> Vec<Pt> {
    let outer = (0..48).map(|k| r(start + TAU * k as f64 / 48.0)).fold(0.0, f64::max);
    let turns = (outer / (0.8 * PEN)).max(0.5);
    let per_turn = steps_for(outer).max(12);
    let n = ((1.0 + turns) * per_turn as f64).ceil() as usize;
    let total = TAU * (1.0 + turns);
    (0..=n)
        .map(|k| {
            let t = total * k as f64 / n as f64;
            let shrink = ((t - TAU) / (TAU * turns)).clamp(0.0, 1.0);
            let a = start + t;
            let rr = r(a) * (1.0 - shrink);
            (rr * a.cos(), rr * a.sin())
        })
        .collect()
}

/// A scalloped closed outline round the origin: `n` round lobes, the cusps
/// between them pointing in by `depth` of the radius, each lobe a little
/// its own size.
fn scalloped(r: f64, n: usize, depth: f64, rng: &mut Rng) -> Vec<Pt> {
    let phase = rng.range(0.0, TAU);
    let sizes: Vec<f64> = (0..n).map(|_| rng.range(0.9, 1.08)).collect();
    let per = 8;
    let mut pts = Vec::with_capacity(n * per + 1);
    for k in 0..n * per {
        let x = k as f64 / per as f64;
        let i = x.floor() as usize % n;
        // Each lobe is a round arch: sqrt(sin) is near a semicircle. Lobes
        // of different sizes meet at the cusp, where the arch is zero.
        let arch = (PI * x.fract()).sin().sqrt();
        let t = phase + TAU * x / n as f64;
        let rr = r * (1.0 - depth + depth * arch) * (1.0 + (sizes[i] - 1.0) * arch);
        pts.push((rr * t.cos(), rr * t.sin()));
    }
    pts.push(pts[0]);
    pts
}

/// A grass blade from `base`: it leaves at `a0` and ends heading `a1`
/// (radians from straight up, positive leaning right), `len` long.
fn blade(base: Pt, a0: f64, a1: f64, len: f64) -> Vec<Pt> {
    let up = |a: f64| (a.sin(), -a.cos());
    let (d0, d1) = (up(a0), up(a1));
    let c = (base.0 + d0.0 * len * 0.55, base.1 + d0.1 * len * 0.55);
    let tip = (c.0 + d1.0 * len * 0.5, c.1 + d1.1 * len * 0.5);
    quad(base, c, tip, 6)
}

/// A gentle arc from `a` to `b`, its middle pushed `by` mm to the left of
/// the direction of travel (negative: to the right).
fn bow(a: Pt, b: Pt, by: f64) -> Vec<Pt> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l = dx.hypot(dy).max(1e-9);
    let m = ((a.0 + b.0) / 2.0 + dy / l * by * 2.0, (a.1 + b.1) / 2.0 - dx / l * by * 2.0);
    quad(a, m, b, 5)
}

/// An almond (a leaf blade) from `a` to `b`, its width `w` of its length.
fn almond(a: Pt, b: Pt, w: f64) -> Vec<Pt> {
    let l = (b.0 - a.0).hypot(b.1 - a.1);
    let mut one = bow(a, b, w * l * 0.5);
    let other = bow(b, a, w * l * 0.5);
    one.extend(other.into_iter().skip(1));
    one
}

/// The quadratic Bézier `a`, `c`, `b` in `n` segments.
fn quad(a: Pt, c: Pt, b: Pt, n: usize) -> Vec<Pt> {
    (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64;
            let u = 1.0 - t;
            (u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1)
        })
        .collect()
}

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// `pts` turned by `a` radians about the origin.
fn turn(pts: &[Pt], a: f64) -> Vec<Pt> {
    let (sin, cos) = a.sin_cos();
    pts.iter().map(|p| (p.0 * cos - p.1 * sin, p.0 * sin + p.1 * cos)).collect()
}

/// Shrink `strokes` about the origin, if need be, to lie within `±half`.
fn fit(strokes: &mut Strokes, half: f64) {
    let reach = strokes.iter().flatten().map(|p| p.0.abs().max(p.1.abs())).fold(0.0, f64::max);
    if reach > half && reach > 0.0 {
        let k = half / reach;
        for p in strokes.iter_mut().flatten() {
            *p = (p.0 * k, p.1 * k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mark_is_drawn_within_its_box() {
        for &name in NAMES {
            for size in [0.3, 0.8, 1.5, 3.0] {
                for seed in 0..40 {
                    let m = draw(name, size, &mut Rng::new(seed));
                    assert!(!m.is_empty(), "{name} at {size} is empty");
                    assert!(m.iter().all(|s| s.len() >= 2), "{name}: a stroke with one point");
                    for p in m.iter().flatten() {
                        assert!(p.0.is_finite() && p.1.is_finite(), "{name}: not finite");
                        assert!(
                            p.0.abs() <= size / 2.0 + 1e-9 && p.1.abs() <= size / 2.0 + 1e-9,
                            "{name} at {size} leaves its box: {p:?}"
                        );
                    }
                    let len: f64 = m.iter().map(|s| crate::geo::polyline_length(s)).sum();
                    assert!(len > 0.0, "{name}: no ink");
                }
            }
        }
    }

    #[test]
    fn marks_vary_but_are_deterministic() {
        for &name in NAMES {
            let a = draw(name, 2.0, &mut Rng::new(1));
            let b = draw(name, 2.0, &mut Rng::new(1));
            let c = draw(name, 2.0, &mut Rng::new(2));
            assert_eq!(a, b, "{name} is not deterministic");
            assert_ne!(a, c, "{name} never varies");
        }
    }

    #[test]
    fn closed_outlines_close() {
        for name in ["ring", "pebble", "shard", "tree", "shrub"] {
            let m = draw(name, 2.0, &mut Rng::new(3));
            let s = &m[0];
            assert_eq!(s.first(), s.last(), "{name}'s outline is open");
        }
    }

    #[test]
    fn unknown_name_and_bad_size() {
        assert_eq!(draw("nope", 0.3, &mut Rng::new(0)).len(), 1);
        assert!(draw("tree", 0.0, &mut Rng::new(0)).is_empty());
        assert!(draw("tree", f64::NAN, &mut Rng::new(0)).is_empty());
    }
}
