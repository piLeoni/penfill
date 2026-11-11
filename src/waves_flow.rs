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

use crate::anchor::Frame;
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
    let flow = Flow {
        wavelength: s * c.custom.get_or("wavelength", 13.0).max(1.0),
        amplitude: s * c.custom.get_or("amplitude", 2.6).max(0.0),
        drift: c.custom.get_or("drift", 1.0).max(0.0),
        breaks: c.custom.get_or("breaks", 0.5).clamp(0.0, 1.0),
    };
    flow_lines(&c.limit, Frame::new(c.origin, c.angle), s, &flow, c.seed)
}

/// The shape of a flowing sea, every length in mm.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Flow {
    /// The length of one swell.
    pub wavelength: f64,
    /// The swell's height either side of a line's mean.
    pub amplitude: f64,
    /// How fast the swell's height and phase change across the lines.
    pub drift: f64,
    /// 0 – 1: how broken the lines are.
    pub breaks: f64,
}

/// Flowing wave lines `s` mm apart over `region`, in `frame` (lines along
/// its `u`). Everything — the field, the samples along a line, where a line
/// breaks — is a function of frame coordinates and the line's level, never
/// of the region, so with a global frame two areas share one sea.
pub(crate) fn flow_lines(region: &Region, frame: Frame, s: f64, p: &Flow, seed: u64) -> Strokes {
    if s <= 0.0 || region.is_empty() {
        return Strokes::new();
    }
    let lambda = p.wavelength.max(s);
    let amp = p.amplitude.max(0.0);
    let noise = Perlin::new(seed ^ 0x51DE_F00D);
    // The field: F(x, y) = y + A(x, y)·sin(2π·x/λ + P(x, y)). The lines are
    // its level sets F = n·spacing, so they can bunch and spread but never
    // cross. A and P change slowly, mostly across the rows (`drift`).
    let across = s * 22.0 / p.drift.max(0.05);
    let field = |x: f64, y: f64| -> f64 {
        let a = amp * (0.5 + 0.5 * noise.fbm(x / (lambda * 2.3), y / across + 11.0, 2));
        let ph = TAU * 1.2 * noise.fbm(x / (lambda * 4.0) + 5.0, y / across, 2);
        y + a * (TAU * x / lambda + ph).sin()
    };

    let (u0, u1, v0, v1) = frame.span(region, s * 2.0);
    // Samples on a grid fixed in the frame, so a line is sampled at the same
    // x whichever region draws it.
    let step = (lambda / 36.0).min(s * 0.5).max(0.05);
    let dy = s / 3.0;
    let (i0, i1) = ((u0 / step).floor() as i64, (u1 / step).ceil() as i64);
    let (k0, k1) = (((v0 - amp * 2.0) / dy).floor() as i64, ((v1 + amp * 2.0) / dy).ceil() as i64);
    let (lo_level, hi_level) = ((v0 / s).floor() as i64 - 1, (v1 / s).ceil() as i64 + 1);
    let mut lines: Vec<Vec<(i64, Pt)>> = vec![Vec::new(); (hi_level - lo_level + 1) as usize];
    let ny = (k1 - k0) as usize;
    let mut col = vec![0.0f64; ny + 1];
    for i in i0..=i1 {
        let x = i as f64 * step;
        let ys = |k: usize| (k0 + k as i64) as f64 * dy;
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
            lines[(lvl - lo_level) as usize].push((i, (x, ys(k) + t * dy)));
        }
    }

    let mut out: Strokes = Vec::new();
    for (n, line) in lines.into_iter().enumerate() {
        let lvl = lo_level + n as i64;
        let mut run: Vec<Pt> = Vec::new();
        let mut last = i64::MIN;
        let mut flush = |run: &mut Vec<Pt>| {
            if run.len() >= 2 {
                out.extend(region.clip_polyline(&run.iter().map(|&q| frame.world(q)).collect::<Vec<_>>()));
            }
            run.clear();
        };
        for (i, q) in line {
            // A line runs on while its samples are consecutive and not in a break.
            let gap = in_gap(q.0, lvl, s, p.breaks, seed);
            if gap || i != last + 1 {
                flush(&mut run);
            }
            if !gap {
                run.push(q);
            }
            last = i;
        }
        flush(&mut run);
    }
    out
}

/// Whether `x` along line `lvl` falls in one of its breaks. The line is cut
/// into stretches of a fixed length, each holding one gap at a place of its
/// own, and now and then a second one close by, which leaves a short dash:
/// mostly long strokes, some short, the way a hand prints waves.
fn in_gap(x: f64, lvl: i64, s: f64, breaks: f64, seed: u64) -> bool {
    if breaks <= 0.0 {
        return false;
    }
    let stretch = s * 12.0 / breaks.max(0.2);
    let cell = (x / stretch).floor() as i64;
    // A gap may run on past its stretch, so look at the one before too.
    for c in [cell - 1, cell] {
        let mut rng = Rng::new(crate::anchor::cell_seed(seed ^ 0x00B4_EAC5, c, lvl));
        let gap = s * rng.range(0.5, 1.3) * (0.5 + breaks);
        let at = (c as f64 + rng.unit()) * stretch;
        if x >= at && x < at + gap {
            return true;
        }
        if rng.unit() < 0.3 * breaks {
            let dash = s * rng.range(0.8, 3.0);
            let second = at + gap + dash;
            if x >= second && x < second + gap * 0.8 {
                return true;
            }
        }
    }
    false
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
