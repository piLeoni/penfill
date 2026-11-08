//! Curling waves: rows of the stylised breaking wave — a back that rises
//! from the trough, a crest that rolls over and curls down into a spiral
//! hook. Each wave is one stroke that starts at the trough tucked under the
//! previous wave's curl, so the row reads as one continuous sea. Drawn
//! precisely; pair it with [`crate::hand`] for the hand-drawn version.
//!
//! `spacing` is the distance between rows; `angle` turns the rows. Custom keys:
//! - `wavelength` (× spacing, default 1.35): how far apart waves sit along a row.
//! - `curl` (× period, default 0.2): radius of each curl.
//! - `turns` (default 0.95): how far each curl winds in (1 = one full turn).
//! - `flip` (0/1, default 0): mirror the waves so they break the other way.
//! - `stagger` (0–1, default 0): shift every other row by this share of a period.

use crate::geo::{Pt, Strokes};
use crate::pattern::{fill_pattern_output, Canvas, FillOutput, FillParams, Tools};
use crate::Region;
use std::f64::consts::{FRAC_PI_2, TAU};

/// Rows of curling waves `spacing` mm apart at `angle` degrees.
pub fn fill_curl_waves(region: &Region, spacing: f64, angle: f64) -> Strokes {
    fill_curl_waves_params(region, &FillParams::new(spacing).angle(angle)).strokes
}

pub fn fill_curl_waves_params(region: &Region, params: &FillParams) -> FillOutput {
    if params.spacing <= 0.0 || region.is_empty() {
        return FillOutput::default();
    }
    fill_pattern_output(region, params, draw_curl_waves)
}

pub(crate) fn draw_curl_waves(c: &Canvas, _t: &Tools) -> Strokes {
    let row = c.spacing;
    let period = row * c.custom.get_or("wavelength", 1.35).max(0.3);
    let r = period * c.custom.get_or("curl", 0.2).clamp(0.03, 0.4);
    let turns = c.custom.get_or("turns", 0.95).clamp(0.3, 3.0);
    let flip = c.custom.get_or("flip", 0.0) >= 0.5;
    let stagger = c.custom.get_or("stagger", 0.0);
    let mut unit = wave_unit(period, r, turns);
    if flip {
        for p in unit.iter_mut() {
            p.0 = period - p.0;
        }
    }

    // Lay rows out in the pattern's own frame (x along the rows), centred on
    // the anchor, wide enough to cover the bounds at any angle.
    let (a_sin, a_cos) = c.angle.to_radians().sin_cos();
    let o = c.origin;
    let (x0, y0, x1, y1) = c.bounds;
    let reach = (x1 - x0).hypot(y1 - y0) / 2.0 + ((x0 + x1) / 2.0 - o.0).hypot((y0 + y1) / 2.0 - o.1) + period * 2.0;
    let rows = (reach / row).ceil() as i64;
    let cols = (reach / period).ceil() as i64 + 1;
    let to_page = |p: Pt| (o.0 + p.0 * a_cos - p.1 * a_sin, o.1 + p.0 * a_sin + p.1 * a_cos);

    let mut out: Strokes = Vec::new();
    for j in -rows..=rows {
        let y = j as f64 * row;
        let shift = if j.rem_euclid(2) == 1 { stagger * period } else { 0.0 };
        for i in -cols..=cols {
            let x = i as f64 * period + shift;
            // Skip waves whose box can't touch the bounds.
            let mid = to_page((x + period * 0.5, y));
            if mid.0 < x0 - period * 1.5 || mid.0 > x1 + period * 1.5 || mid.1 < y0 - period * 1.5 || mid.1 > y1 + period * 1.5 {
                continue;
            }
            out.push(unit.iter().map(|p| to_page((p.0 + x, p.1 + y))).collect());
        }
    }
    out
}

/// One wave, in a frame where the row runs along +x and +y is down (the
/// page's way): from the trough at `(0, r)` up the back to the crest at
/// `(period, -r)`, then over and down into a clockwise spiral whose outer
/// bottom lands on the next wave's trough at `(period, r)`.
fn wave_unit(period: f64, r: f64, turns: f64) -> Vec<Pt> {
    let mut pts = Vec::new();
    // The back: a cubic from the trough to the crest, flat at both ends.
    let (s, k) = ((0.0, r), (period, -r));
    let (c1, c2) = ((period * 0.4, r), (period * 0.5, -r * 1.25));
    let n = 28;
    for i in 0..n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;
        pts.push((
            u * u * u * s.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * k.0,
            u * u * u * s.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * k.1,
        ));
    }
    // The curl: around (period, 0) from the top, clockwise on the page,
    // winding in from `r` to a third of it.
    let c = (period, 0.0);
    let sweep = TAU * turns;
    let m = (48.0 * turns).ceil() as usize;
    for i in 0..=m {
        let t = i as f64 / m as f64;
        let th = -FRAC_PI_2 + sweep * t;
        let rr = r * (1.0 - 0.55 * t.powf(1.3));
        pts.push((c.0 + rr * th.cos(), c.1 + rr * th.sin()));
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_of_waves_fill_a_box() {
        let sea = Region::from_ring(&[(0.0, 0.0), (60.0, 0.0), (60.0, 40.0), (0.0, 40.0), (0.0, 0.0)]);
        let s = fill_curl_waves(&sea, 5.0, 0.0);
        assert!(s.len() > 30, "{}", s.len());
        for l in &s {
            for p in l {
                assert!(p.0 >= -1e-6 && p.0 <= 60.0 + 1e-6 && p.1 >= -1e-6 && p.1 <= 40.0 + 1e-6);
            }
        }
    }
}
