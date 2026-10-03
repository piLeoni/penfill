//! Grass: the CAD / site-plan hatch for lawns and meadows — a tile of
//! tufts repeated over the plane and cut by the wall, like any other hatch.
//!
//! The tile is 4 × 4 tufts on offset rows `spacing` apart. Inside the tile
//! every tuft has its own nudge, blade count, lengths and bends, so the
//! texture looks hand-drawn up close, yet it repeats exactly every tile the
//! way a `.pat` hatch does. Tufts crossing the wall are clipped, not moved
//! or dropped. `angle` turns the whole hatch (0 = blades point up).

use crate::anchor::{centre, indices, Anchor, Frame};
use crate::geo::{polyline_length, Pt, Region, Strokes};
use crate::random::Rng;

/// Tufts per tile side.
const TILE: i64 = 4;
const STEPS: usize = 4;

/// One tuft of the tile: offset from its lattice point, and per blade
/// (direction, length, bend), all in the hatch's own frame.
struct Tuft {
    nudge: Pt,
    blades: Vec<(f64, f64, f64)>,
}

/// Tufts `spacing` mm apart; a blade is about 0.6 × `spacing` long.
pub fn fill_grass(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let tile = make_tile(spacing, seed);
    let row = spacing * 3f64.sqrt() / 2.0;
    let frame = Frame::anchored(anchor, centre(region), angle);
    let world = |p: Pt| frame.world(p);
    let (u0, u1, v0, v1) = frame.span(region, spacing);

    let mut out: Strokes = Vec::new();
    for j in indices(v0, v1, row) {
        let shift = if j.rem_euclid(2) == 1 { spacing / 2.0 } else { 0.0 };
        for i in indices(u0, u1, spacing) {
            let tuft = &tile[(j.rem_euclid(TILE) * TILE + i.rem_euclid(TILE)) as usize];
            let base = (i as f64 * spacing + shift + tuft.nudge.0, j as f64 * row + tuft.nudge.1);
            for &(a, len, bend) in &tuft.blades {
                let pts: Vec<Pt> = blade(base, a, len, bend).into_iter().map(world).collect();
                // A blade tip just inside the wall plots as a speck.
                out.extend(
                    region
                        .clip_polyline(&pts)
                        .into_iter()
                        .filter(|s| polyline_length(s) >= spacing * 0.08),
                );
            }
        }
    }
    out
}

fn make_tile(spacing: f64, seed: u64) -> Vec<Tuft> {
    let mut rng = Rng::new(seed.wrapping_add(2));
    let up = std::f64::consts::FRAC_PI_2;
    let fan = 0.9;
    (0..TILE * TILE)
        .map(|_| {
            let nudge = (rng.range(-0.3, 0.3) * spacing, rng.range(-0.25, 0.25) * spacing);
            let n = 3 + (rng.next_u64() % 3) as usize;
            let blades = (0..n)
                .map(|b| {
                    let t = b as f64 / (n - 1) as f64;
                    let a = up - fan / 2.0 + fan * t + rng.range(-0.12, 0.12);
                    let len = spacing * rng.range(0.45, 0.75);
                    // Bend toward the outside of the fan.
                    let bend = (t - 0.5) * 0.35 + rng.range(-0.08, 0.08);
                    (a, len, bend)
                })
                .collect();
            Tuft { nudge, blades }
        })
        .collect()
}

fn blade(base: Pt, angle: f64, len: f64, bend: f64) -> Vec<Pt> {
    let mut pts = vec![base];
    let mut p = base;
    for i in 1..=STEPS {
        let a = angle + bend * (i as f64 / STEPS as f64);
        let step = len / STEPS as f64;
        p = (p.0 + a.cos() * step, p.1 + a.sin() * step);
        pts.push(p);
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)])
    }

    #[test]
    fn blades_stay_inside() {
        let grass = fill_grass(&square(), 3.0, 20.0, 5, Anchor::LOCAL);
        assert!(grass.len() > 100, "only {} blades", grass.len());
        for p in grass.iter().flatten() {
            assert!(square().covers_point(*p), "blade point {p:?} outside");
        }
    }

    #[test]
    fn wall_cuts_tufts_instead_of_dropping_them() {
        let grass = fill_grass(&square(), 3.0, 0.0, 5, Anchor::LOCAL);
        let on_wall = |p: &Pt| p.0.min(p.1).min(30.0 - p.0).min(30.0 - p.1) < 1e-3;
        let cut = grass.iter().filter(|s| on_wall(s.last().unwrap()) || on_wall(&s[0])).count();
        assert!(cut > 10, "only {cut} blades end on the wall");
    }

    #[test]
    fn hatch_repeats_every_tile() {
        // Same tuft pattern one tile to the right: shift by TILE × spacing.
        let big = Region::from_ring(&[(0.0, 0.0), (60.0, 0.0), (60.0, 60.0), (0.0, 60.0)]);
        let grass = fill_grass(&big, 3.0, 0.0, 5, Anchor::LOCAL);
        let key = |p: Pt| ((p.0 * 1e3).round() as i64, (p.1 * 1e3).round() as i64);
        let starts: std::collections::HashSet<_> = grass.iter().map(|s| key(s[0])).collect();
        let dx = TILE as f64 * 3.0;
        let inner: Vec<Pt> = grass.iter().map(|s| s[0]).filter(|p| p.0 > 10.0 && p.0 < 40.0 && p.1 > 10.0 && p.1 < 50.0).collect();
        assert!(inner.len() > 50);
        for p in inner {
            assert!(starts.contains(&key((p.0 + dx, p.1))), "no repeat of {p:?}");
        }
    }
}
