//! Scalloped waves: rows of shallow arcs that dip between sharp upward
//! points, each row one continuous stroke — the simplest drawn sea. Drawn
//! precisely; pair it with [`crate::hand`] to loosen it.
//!
//! `spacing` is the distance between rows; `angle` turns the rows. Custom keys:
//! - `wavelength` (× spacing, default 1.35): width of one scallop.
//! - `depth` (× spacing, default 0.35): how far each scallop dips.
//! - `round` (0.5–1.5, default 0.8): below 1 rounder, fuller arcs; above 1 more pointed.
//! - `stagger` (0–1, default 0): shift every other row by this share of a scallop.

use crate::geo::{Pt, Strokes};
use crate::pattern::{fill_pattern_output, Canvas, FillOutput, FillParams, Tools};
use crate::Region;
use std::f64::consts::PI;

/// Rows of scallops `spacing` mm apart at `angle` degrees.
pub fn fill_scallop_waves(region: &Region, spacing: f64, angle: f64) -> Strokes {
    fill_scallop_waves_params(region, &FillParams::new(spacing).angle(angle)).strokes
}

pub fn fill_scallop_waves_params(region: &Region, params: &FillParams) -> FillOutput {
    if params.spacing <= 0.0 || region.is_empty() {
        return FillOutput::default();
    }
    fill_pattern_output(region, params, draw_scallop_waves)
}

pub(crate) fn draw_scallop_waves(c: &Canvas, _t: &Tools) -> Strokes {
    let s = c.spacing;
    let w = s * c.custom.get_or("wavelength", 1.35).max(0.2);
    let depth = s * c.custom.get_or("depth", 0.35).clamp(0.0, 2.0);
    let round = c.custom.get_or("round", 0.8).clamp(0.3, 2.0);
    let stagger = c.custom.get_or("stagger", 0.0);

    let (a_sin, a_cos) = c.angle.to_radians().sin_cos();
    let o = c.origin;
    let (x0, y0, x1, y1) = c.bounds;
    let reach = (x1 - x0).hypot(y1 - y0) / 2.0 + ((x0 + x1) / 2.0 - o.0).hypot((y0 + y1) / 2.0 - o.1) + w;
    let to_page = |p: Pt| (o.0 + p.0 * a_cos - p.1 * a_sin, o.1 + p.0 * a_sin + p.1 * a_cos);
    let rows = (reach / s).ceil() as i64;
    let n = ((2.0 * reach) / w).ceil() as i64;
    let per = 16;

    let mut out: Strokes = Vec::new();
    // Scallops start at whole multiples of `w` from the anchor, so two
    // regions on one global anchor share their cusps.
    let first = (-reach / w).floor() as i64;
    for j in -rows..=rows {
        let y = j as f64 * s;
        let shift = if j.rem_euclid(2) == 1 { stagger * w } else { 0.0 };
        let mut line: Vec<Pt> = Vec::with_capacity((n as usize) * per + 1);
        for i in 0..=n {
            let xa = (first + i) as f64 * w + shift;
            for k in 0..per {
                let u = k as f64 / per as f64;
                // y grows down the page, so the arc dips below the row line
                // and meets the next one at a sharp point.
                line.push((xa + u * w, y + scallop(u, depth, round)));
            }
        }
        line.push(((first + n + 1) as f64 * w + shift, y));
        out.push(line.into_iter().map(to_page).collect());
    }
    out
}

/// The dip of a scallop `t` (0 – 1) of the way across it: 0 at both
/// cusps, `depth` in the middle; `round` below 1 fills the arc out, above 1
/// draws it to a point.
pub(crate) fn scallop(t: f64, depth: f64, round: f64) -> f64 {
    depth * (PI * t.clamp(0.0, 1.0)).sin().powf(round)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_stroke_per_row() {
        let sea = Region::from_ring(&[(0.0, 0.0), (60.0, 0.0), (60.0, 30.0), (0.0, 30.0), (0.0, 0.0)]);
        let s = fill_scallop_waves(&sea, 5.0, 0.0);
        assert!((5..=8).contains(&s.len()), "{}", s.len());
    }
}
