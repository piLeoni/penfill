//! Doodled trees: the crowns a hand draws on a sketch map. Each crown is one
//! continuous curlicue — small loops running round a lumpy circle, often
//! overshooting where it started — with a crescent of shade on the side away
//! from the light. Crowns nearer the bottom of the sheet are drawn
//! whole; the ones behind them are hidden where they overlap.
//!
//! `spacing` is the typical crown diameter. Custom keys:
//! - `coverage` (0–1, default 0.85): share of the scattered sites that get a tree.
//! - `loops` (default 1.0): how many curls round the rim (scales the count).
//! - `shadow` (0–1, default 0.8): share of crowns that get a shade crescent.
//! - `light_angle` (degrees, default 225 = from the top left): where light comes from.

use crate::geo::{Pt, Region, Strokes};
use crate::pattern::{fill_pattern_output, Canvas, FillOutput, FillParams, Tools};
use crate::random::{poisson_disk, Rng};
use std::f64::consts::{PI, TAU};

/// Doodled tree crowns about `spacing` mm across.
pub fn fill_doodle_trees(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fill_doodle_trees_params(region, &FillParams::new(spacing).seed(seed)).strokes
}

pub fn fill_doodle_trees_params(region: &Region, params: &FillParams) -> FillOutput {
    if params.spacing <= 0.0 || region.is_empty() {
        return FillOutput::default();
    }
    fill_pattern_output(region, params, draw_doodle_trees)
}

struct Crown {
    c: Pt,
    r: f64,
}

pub(crate) fn draw_doodle_trees(canvas: &Canvas, tools: &Tools) -> Strokes {
    let diameter = canvas.spacing;
    let coverage = canvas.custom.get_or("coverage", 0.85).clamp(0.0, 1.0);
    let loops = canvas.custom.get_or("loops", 1.0).max(0.2);
    let shadow = canvas.custom.get_or("shadow", 0.8).clamp(0.0, 1.0);
    let light = canvas.custom.get_or("light_angle", 225.0).to_radians();
    let mut rng = tools.rng_salted(71);

    // Sites a little closer than a crown, so neighbours overlap.
    let field = tools.placement_field(diameter * 0.5);
    let mut crowns: Vec<Crown> = Vec::new();
    for c in poisson_disk(&field, diameter * 0.72, canvas.seed) {
        let r = 0.5 * diameter * rng.range(0.7, 1.2);
        if rng.unit() > coverage || !tools.element_fits(c, r * 0.9) {
            continue;
        }
        crowns.push(Crown { c, r });
    }
    // Front (bottom of the sheet) first: each crown hides those behind it.
    crowns.sort_by(|a, b| b.c.1.total_cmp(&a.c.1));

    let mut out: Strokes = Vec::new();
    for (i, crown) in crowns.iter().enumerate() {
        let mut rng = Rng::new(canvas.seed ^ (i as u64 + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93));
        let mut strokes = vec![curl_ring(crown, loops, &mut rng)];
        if rng.unit() < shadow {
            strokes.push(shade_crescent(crown, light, &mut rng));
        }
        let front = &crowns[..i];
        for s in strokes {
            out.extend(hide_behind(&s, crown, front));
        }
        tools.add_mask_ring(&tools.circle(crown.c, crown.r));
    }
    out
}

/// One stroke of small loops running round a lumpy circle, ending past (or
/// short of) where it began. The loops speed up, slow down and change size as
/// the pen goes round, and a big crown often gets a second, tighter lap.
fn curl_ring(k: &Crown, loops: f64, rng: &mut Rng) -> Vec<Pt> {
    let n = ((TAU / 0.5) * loops * rng.range(0.7, 1.3)).round().clamp(5.0, 22.0);
    let small = k.r * rng.range(0.12, 0.2);
    let start = rng.range(0.0, TAU);
    let laps = if rng.unit() < 0.45 { rng.range(1.5, 1.9) } else { rng.range(0.95, 1.15) };
    let sweep = TAU * laps;
    // Lumpiness and loop irregularity: a few slow harmonics with random phases.
    let harm: Vec<(f64, f64, f64)> = (2..5).map(|h| (h as f64, rng.range(0.04, 0.12), rng.range(0.0, TAU))).collect();
    let size: Vec<(f64, f64, f64)> = (3..7).map(|h| (h as f64, rng.range(0.15, 0.35), rng.range(0.0, TAU))).collect();
    let pace: Vec<(f64, f64, f64)> = (2..5).map(|h| (h as f64, rng.range(0.15, 0.4), rng.range(0.0, TAU))).collect();
    let wave = |hs: &[(f64, f64, f64)], t: f64| -> f64 { hs.iter().map(|(h, a, p)| a * (h * t + p).sin()).sum() };
    let steps = (n * laps * 16.0) as usize;
    let dt = sweep / steps as f64;
    let mut phi = rng.range(0.0, TAU);
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = start + dt * i as f64;
        let lap = (t - start) / TAU;
        // Second lap: tighter, a little inside the first.
        let shrink = if lap > 1.0 { 1.0 - 0.28 * (lap - 1.0).min(1.0) } else { 1.0 };
        let big = (k.r - small) * (1.0 + wave(&harm, t)) * shrink;
        let s = small * (1.0 + wave(&size, t)).max(0.35) * shrink;
        // The pen turns n extra times per lap — at an uneven pace — so it runs
        // back on itself at the rim and curls outward in loops of mixed size.
        phi += dt * (n + 1.0) * (1.0 + wave(&pace, t)).max(0.3);
        pts.push((k.c.0 + big * t.cos() + s * phi.cos(), k.c.1 + big * t.sin() + s * phi.sin()));
    }
    pts
}

/// A loose crescent of shade inside the rim, on the side away from the light.
fn shade_crescent(k: &Crown, light: f64, rng: &mut Rng) -> Vec<Pt> {
    let away = light + PI + rng.range(-0.35, 0.35);
    let half = rng.range(0.7, 1.15);
    let (r0, r1) = (rng.range(0.55, 0.7), rng.range(0.6, 0.78));
    let wob = rng.range(0.0, TAU);
    let steps = 18;
    (0..=steps)
        .map(|j| {
            let u = j as f64 / steps as f64;
            let a = away - half + 2.0 * half * u;
            let rr = k.r * (r0 + (r1 - r0) * u + 0.04 * (u * 9.0 + wob).sin());
            (k.c.0 + rr * a.cos(), k.c.1 + rr * a.sin())
        })
        .collect()
}

/// Split `stroke` where it runs inside any crown already drawn in front.
fn hide_behind(stroke: &[Pt], me: &Crown, front: &[Crown]) -> Strokes {
    let near: Vec<&Crown> = front
        .iter()
        .filter(|f| (f.c.0 - me.c.0).hypot(f.c.1 - me.c.1) < (f.r + me.r) * 1.3)
        .collect();
    if near.is_empty() {
        return vec![stroke.to_vec()];
    }
    let hidden = |p: Pt| near.iter().any(|f| (p.0 - f.c.0).hypot(p.1 - f.c.1) < f.r * 0.92);
    let mut out = Vec::new();
    let mut cur: Vec<Pt> = Vec::new();
    for &p in stroke {
        if hidden(p) {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        } else {
            cur.push(p);
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_trees_inside_a_park() {
        let park = Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0), (0.0, 0.0)]);
        let s = fill_doodle_trees(&park, 4.0, 3);
        assert!(s.len() > 10);
        assert_eq!(s, fill_doodle_trees(&park, 4.0, 3));
        for l in &s {
            for p in l {
                assert!(p.0 > -0.5 && p.0 < 40.5 && p.1 > -0.5 && p.1 < 30.5, "{p:?}");
            }
        }
    }
}
