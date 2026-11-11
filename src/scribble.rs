//! Scribble: shading the way an illustrator does it quickly with a fine
//! pen — the pen going back and forth without lifting, in loose zigzags
//! with rounded turns, patch after patch, the patches overlapping where
//! they meet, and a second (and third) layer laid over the first at
//! another angle where the shade is deeper.
//!
//! One patch is one stroke: a zigzag of slightly bowed lines about
//! `length` long, each pass advancing about one `spacing`, its turns
//! rounded into small hairpins that overshoot a little. The ends of the
//! passes wander, so a patch's edge is ragged; the passes are shorter near
//! its top and bottom, so a patch is a rough lozenge rather than a box; all
//! the lines of a patch bow the same way, the way a wrist swings.
//!
//! Patches sit on a brick lattice in each layer's frame, a little closer
//! than their size so they overlap, and every random choice comes from the
//! lattice cell's own seed: with a global origin, two areas share one
//! scribble.

use crate::anchor::{cell_seed, centre, indices, Frame};
use crate::geo::{Pt, Region, Strokes};
use crate::random::Rng;

/// How each layer is turned from the first, in degrees.
const LAYER_TURN: [f64; 3] = [0.0, 55.0, -50.0];

/// A scribble's settings, lengths in mm.
#[derive(Clone, Copy, Debug)]
pub struct Scribble {
    /// Distance between neighbouring passes.
    pub spacing: f64,
    /// The first layer's stroke direction, degrees.
    pub angle: f64,
    /// 1 – 3 layers, each turned from the last.
    pub layers: usize,
    /// 0 – 1: how messy — wandering ends, uneven passes, tilted patches.
    pub loose: f64,
    /// The length of one pass, end to end.
    pub length: f64,
}

impl Scribble {
    /// The usual scribble at `spacing`: two layers, half loose.
    pub fn new(spacing: f64, angle: f64) -> Scribble {
        Scribble { spacing, angle, layers: 2, loose: 0.5, length: spacing * 9.0 }
    }
}

/// Two layers of scribble about `spacing` mm apart at `angle`, placed on
/// the region.
pub fn fill_scribble(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    scribble(region, centre(region), &Scribble::new(spacing, angle), seed, |_, _| true)
}

/// Scribble over `region`, the lattice through `origin`. `keep(at, layer)`
/// says whether the patch centred at `at` in `layer` is drawn (tone: thin
/// the patches and drop the upper layers where the area is lighter).
pub fn scribble(region: &Region, origin: Pt, p: &Scribble, seed: u64, keep: impl Fn(Pt, usize) -> bool) -> Strokes {
    let s = p.spacing;
    if s.is_nan() || s <= 0.0 || region.is_empty() {
        return Strokes::new();
    }
    let loose = p.loose.clamp(0.0, 1.0);
    let length = p.length.max(s * 2.0);
    let mut out = Strokes::new();
    for (layer, turn) in LAYER_TURN.iter().enumerate().take(p.layers.clamp(1, 3)) {
        let salt = seed ^ (0x5C21_BB1E + layer as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut rng = Rng::new(salt);
        let angle = p.angle + turn + if layer > 0 { rng.range(-6.0, 6.0) } else { 0.0 };
        let frame = Frame::new(origin, angle);
        // A patch is a band: the hand goes back and forth `length` wide
        // while it moves across the area, then lifts and starts the next
        // band beside it. Bands sit in columns a little narrower than they
        // are, every other column half a band down, so neighbours overlap
        // at their ragged edges.
        let (cw, ch) = (length * 0.92, length * 3.2);
        let (u0, u1, v0, v1) = frame.span(region, length * 2.0);
        for i in indices(u0, u1, cw) {
            let shift = if i.rem_euclid(2) == 1 { ch * 0.5 } else { 0.0 };
            for j in indices(v0 - shift, v1 - shift, ch) {
                let mut rng = Rng::new(cell_seed(salt, i, j));
                let c = (
                    i as f64 * cw + cw * 0.12 * loose * rng.range(-1.0, 1.0),
                    j as f64 * ch + shift + ch * 0.12 * loose * rng.range(-1.0, 1.0),
                );
                if !keep(frame.world(c), layer) {
                    continue;
                }
                let path = patch(&mut rng, s, length, ch, loose);
                let tilt = ((3.0 + 8.0 * loose) * rng.range(-1.0, 1.0)).to_radians();
                let (st, ct) = tilt.sin_cos();
                let world: Vec<Pt> =
                    path.iter().map(|&(x, y)| frame.world((c.0 + x * ct - y * st, c.1 + x * st + y * ct))).collect();
                out.extend(region.clip_polyline(&world));
            }
        }
    }
    out
}

/// One patch, centred on the origin, passes along x and advancing down y:
/// one continuous stroke.
///
/// A fast hand moves the pen along a line with the fingers and steps across
/// at the turn, so a pass is nearly parallel to the last: only a share of
/// each step (`zig`, more the looser the hand) is taken along the pass, the
/// rest in the rounded U of the turn, which swings a little past the pass's
/// end.
fn patch(rng: &mut Rng, s: f64, length: f64, cell_h: f64, loose: f64) -> Vec<Pt> {
    let len = length * rng.range(0.9, 1.15);
    let height = cell_h * rng.range(0.95, 1.12);
    // The whole patch bows one way, as a wrist swings.
    let bow = len * rng.range(0.012, 0.04) * if rng.coin() { 1.0 } else { -1.0 };
    let wander = len * (0.04 + 0.14 * loose);
    let zig = 0.45 + 0.3 * loose;
    // The hand drifts sideways a little as it moves down the band.
    let mut sway = 0.0;
    let mut sway_v = 0.0;
    // A slow drift of the ends, so the edge is ragged in runs, not noise.
    let mut drift = 0.0;
    let mut side = if rng.coin() { 1.0 } else { -1.0 };
    let mut y = -height / 2.0;
    let mut pts: Vec<Pt> = Vec::new();
    let mut prev_dir = 0.0;
    while y <= height / 2.0 {
        let rel = (2.0 * y / height).clamp(-1.0, 1.0);
        // Passes are shorter at the top and bottom: a rough lozenge.
        let half = len / 2.0 * (1.0 - 0.3 * rel.powi(4));
        sway_v = 0.85 * sway_v + s * 0.05 * (0.5 + loose) * rng.range(-1.0, 1.0);
        sway = 0.96 * sway + sway_v;
        let mut end = || {
            drift = 0.6 * drift + 0.4 * rng.range(-1.0, 1.0);
            half + wander * (0.55 * drift + 0.45 * rng.range(-1.0, 1.0))
        };
        let (from, to) = (sway - side * end(), sway + side * end());
        let step = s * (1.0 + 0.35 * loose * rng.range(-1.0, 1.0)).max(0.5);
        let rise = step * zig;
        let own = loose * s * 0.15 * rng.range(-1.0, 1.0);
        let a = (from, y);
        let b = (to, y + rise);
        // The turn from the last pass into this one.
        if let Some(&e) = pts.last() {
            // A convex U: both handles reach the same line just past the
            // outer of the two ends, so the turn never loops.
            let r = s * rng.range(0.15, 0.3) * (1.0 + 0.5 * loose);
            let outer = prev_dir * (prev_dir * e.0).max(prev_dir * a.0) + prev_dir * r;
            let (c1, c2) = ((outer, e.1), (outer, a.1));
            for q in 1..6 {
                let t = q as f64 / 6.0;
                let u = 1.0 - t;
                pts.push((
                    u * u * u * e.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * a.0,
                    u * u * u * e.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * a.1,
                ));
            }
        }
        // The pass: a straight run bowed to the patch's side.
        let n = ((to - from).abs() / (s * 0.6)).ceil().clamp(2.0, 24.0) as usize;
        for q in 0..=n {
            let t = q as f64 / n as f64;
            let sag = 4.0 * t * (1.0 - t) * (bow + own);
            pts.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t + sag));
        }
        prev_dir = side;
        y += step;
        side = -side;
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::polyline_length;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)])
    }

    #[test]
    fn patches_are_long_strokes_inside() {
        let sq = square();
        let s = fill_scribble(&sq, 1.0, 30.0, 1);
        assert!(s.len() > 20, "only {} strokes", s.len());
        for st in &s {
            for p in st {
                assert!(sq.covers_point(*p) || sq.dist_to_boundary(*p) < 1e-6, "{p:?} outside");
            }
        }
        // A patch is one stroke going back and forth, not a dash.
        let longest = s.iter().map(|p| polyline_length(p)).fold(0.0, f64::max);
        assert!(longest > 40.0, "longest stroke {longest}");
    }

    #[test]
    fn layers_add_ink() {
        let sq = square();
        let ink = |layers| -> f64 {
            let p = Scribble { layers, ..Scribble::new(0.8, 20.0) };
            scribble(&sq, (0.0, 0.0), &p, 3, |_, _| true).iter().map(|s| polyline_length(s)).sum()
        };
        let (one, two, three) = (ink(1), ink(2), ink(3));
        assert!(two > one * 1.7 && three > two * 1.3, "{one} {two} {three}");
    }
}
