//! Hand hatching: cross-hatch that looks drawn, not plotted.
//!
//! Two layers of lines, the second turned about 70–110° from the first.
//! Every line gets its own small tilt and spacing error, and is broken into
//! strokes of random length with short gaps — the way a hand lifts the pen.
//! The texture of sketch shading in maps and architectural drawings.

use crate::geo::{Pt, Region, Strokes};
use crate::random::Rng;

/// Lines about `spacing` mm apart at `angle`, plus a crossing layer.
pub fn fill_scribble(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(seed.wrapping_add(4));
    let cross = angle + 90.0 + rng.range(-20.0, 20.0);
    let mut out = layer(region, spacing, angle, &mut rng);
    out.extend(layer(region, spacing * 1.15, cross, &mut rng));
    out
}

fn layer(region: &Region, spacing: f64, angle: f64, rng: &mut Rng) -> Strokes {
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let c = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
    let half = ((maxx - minx).hypot(maxy - miny)) / 2.0 + spacing;
    let mut out: Strokes = Vec::new();
    let mut v = -half;
    while v <= half {
        let tilt = (angle + rng.range(-4.0, 4.0)).to_radians();
        let (u, n) = ((tilt.cos(), tilt.sin()), (-tilt.sin(), tilt.cos()));
        let base = (c.0 + n.0 * v, c.1 + n.1 * v);
        let mut s = -half;
        while s < half {
            let len = spacing * rng.range(3.0, 14.0);
            let a = at(base, u, s);
            let b = at(base, u, (s + len).min(half));
            out.extend(region.clip_segment(a, b));
            s += len + spacing * rng.range(0.4, 1.2);
        }
        v += spacing * rng.range(0.75, 1.25);
    }
    out
}

fn at(base: Pt, u: Pt, s: f64) -> Pt {
    (base.0 + u.0 * s, base.1 + u.1 * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_layers_inside() {
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let s = fill_scribble(&sq, 2.0, 30.0, 1);
        assert!(s.len() > 30, "only {} strokes", s.len());
        for st in &s {
            for p in st {
                assert!(sq.covers_point(*p), "{p:?} outside");
            }
        }
    }
}
