//! Flowing waves: a sea of long parallel wave lines that ride big swells,
//! bunching where the swell steepens and spreading where it flattens, each
//! line broken into strokes of uneven length — the hand-printed wave textile
//! look. Drawn precisely; pair it with [`crate::hand`] to loosen it.
//!
//! The lines are the level sets of one field,
//! `F(x, y) = y + A·sin(2π·x / λ + P)`, at `spacing` mm steps, with the swell
//! height `A` and phase `P` drifting slowly over the sea: lines bunch where
//! the swell steepens and spread where it eases, and never cross.
//!
//! Custom keys:
//! - `wavelength` (× spacing, default 13): length of one swell.
//! - `amplitude` (× spacing, default 2.6): height of the swell.
//! - `drift` (default 1): how fast the phase and height change across lines.
//! - `breaks` (0–1, default 0.5): how broken the lines are; 0 = unbroken.

use crate::geo::{Pt, Strokes};
use crate::noise::Perlin;
use crate::pattern::{fill_pattern_output, Canvas, FillOutput, FillParams, Tools};
use crate::random::Rng;
use crate::Region;
use std::f64::consts::TAU;

/// Flowing wave lines `spacing` mm apart at `angle` degrees.
pub fn fill_flow_waves(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    fill_flow_waves_params(region, &FillParams::new(spacing).angle(angle).seed(seed)).strokes
}

pub fn fill_flow_waves_params(region: &Region, params: &FillParams) -> FillOutput {
    if params.spacing <= 0.0 || region.is_empty() {
        return FillOutput::default();
    }
    fill_pattern_output(region, params, draw_flow_waves)
}

pub(crate) fn draw_flow_waves(c: &Canvas, _t: &Tools) -> Strokes {
    let s = c.spacing;
    let lambda = s * c.custom.get_or("wavelength", 13.0).max(1.0);
    let amp = s * c.custom.get_or("amplitude", 2.6).max(0.0);
    let drift = c.custom.get_or("drift", 1.0).max(0.0);
    let breaks = c.custom.get_or("breaks", 0.5).clamp(0.0, 1.0);
    let noise = Perlin::new(c.seed ^ 0x51DE_F00D);

    let (a_sin, a_cos) = c.angle.to_radians().sin_cos();
    let o = c.origin;
    let (x0, y0, x1, y1) = c.bounds;
    let reach = (x1 - x0).hypot(y1 - y0) / 2.0 + ((x0 + x1) / 2.0 - o.0).hypot((y0 + y1) / 2.0 - o.1) + amp + s;
    let to_page = |p: Pt| (o.0 + p.0 * a_cos - p.1 * a_sin, o.1 + p.0 * a_sin + p.1 * a_cos);

    // The field: F(x, y) = y + A(x, y)·sin(2π·x/λ + P(x, y)). The lines are
    // its level sets F = n·spacing, so they can bunch and spread but never
    // cross. A and P change slowly, mostly across the rows (`drift`).
    let across = s * 22.0 / drift.max(0.05);
    let field = |x: f64, y: f64| -> f64 {
        let a = amp * (0.5 + 0.5 * noise.fbm(x / (lambda * 2.3), y / across + 11.0, 2));
        let p = TAU * 1.2 * noise.fbm(x / (lambda * 4.0) + 5.0, y / across, 2);
        y + a * (TAU * x / lambda + p).sin()
    };

    let step = (lambda / 36.0).min(s * 0.5).max(0.05);
    let dy = s / 3.0;
    let cols = (2.0 * reach / step).ceil() as usize;
    let ny = ((2.0 * (reach + amp)) / dy).ceil() as usize;
    let lo_level = ((-reach) / s).floor() as i64;
    let hi_level = (reach / s).ceil() as i64;
    let mut lines: Vec<Vec<Pt>> = vec![Vec::with_capacity(cols + 1); (hi_level - lo_level + 1) as usize];
    let mut col = vec![0.0f64; ny + 1];
    for i in 0..=cols {
        let x = -reach + i as f64 * step;
        let ys = |k: usize| -reach - amp + k as f64 * dy;
        let mut run = f64::NEG_INFINITY;
        for (k, v) in col.iter_mut().enumerate() {
            // Monotone up the column (first crossing wins), so a level is met once.
            run = run.max(field(x, ys(k)));
            *v = run;
        }
        let mut k = 0usize;
        for lvl in lo_level..=hi_level {
            let target = lvl as f64 * s;
            while k < ny && col[k + 1] < target {
                k += 1;
            }
            if k >= ny || col[k] > target {
                continue;
            }
            let (f0, f1) = (col[k], col[k + 1]);
            let t = if f1 > f0 { (target - f0) / (f1 - f0) } else { 0.0 };
            lines[(lvl - lo_level) as usize].push((x, ys(k) + t * dy));
        }
    }

    let mut out: Strokes = Vec::new();
    for (j, line) in lines.into_iter().enumerate() {
        if line.len() < 2 {
            continue;
        }
        if breaks <= 0.0 {
            out.push(line.iter().map(|&p| to_page(p)).collect());
            continue;
        }
        // Break the line into strokes of uneven length: mostly long, some short.
        let mut rng = Rng::new(c.seed.wrapping_mul(0x9E37_79B9).wrapping_add(j as u64 + 7));
        let mut k = (rng.unit() * 10.0) as usize;
        while k < line.len() {
            let long = rng.unit() > 0.3 * breaks;
            let len_mm = if long { s * rng.range(4.0, 20.0) / breaks.max(0.2) } else { s * rng.range(0.8, 3.0) };
            let gap_mm = s * rng.range(0.5, 1.3) * (0.5 + breaks);
            let take = ((len_mm / step) as usize).max(2);
            let end = (k + take).min(line.len());
            if end - k >= 2 {
                out.push(line[k..end].iter().map(|&p| to_page(p)).collect());
            }
            k = end + ((gap_mm / step) as usize).max(1);
        }
    }
    out.retain(|l| l.iter().any(|p| p.0 >= x0 - s && p.0 <= x1 + s && p.1 >= y0 - s && p.1 <= y1 + s));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_a_box_with_broken_lines() {
        let sea = Region::from_ring(&[(0.0, 0.0), (80.0, 0.0), (80.0, 50.0), (0.0, 50.0), (0.0, 0.0)]);
        let s = fill_flow_waves(&sea, 1.5, 0.0, 3);
        assert!(s.len() > 60, "{}", s.len());
        assert_eq!(s, fill_flow_waves(&sea, 1.5, 0.0, 3));
    }
}
