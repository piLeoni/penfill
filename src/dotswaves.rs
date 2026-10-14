//! Dot and wave pattern fills, ported from `postcards.utils.patterns`
//! (`dots.py`, `waves.py`).

use crate::anchor::{centre, indices, Anchor, Frame};
use crate::geo::{Region, Strokes};

/// March step for the wave curve, mm (postcards `_STEP = 0.1`).
const STEP: f64 = 0.1;

/// Marks on a square grid of `interval` mm, each `mark` mm long — clipped
/// to the region. Port of `dots_pattern`.
pub fn fill_dots(region: &Region, interval: f64, angle: f64, mark: f64, anchor: Anchor) -> Strokes {
    fill_dots_with_lattice(region, interval, angle, mark, Lattice::Square, anchor)
}

/// Which grid the dots sit on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lattice {
    /// Rows and columns (postcards `dots_pattern`).
    Square,
    /// Offset rows — each row shifted by half the interval and the row pitch
    /// reduced to `interval · √3/2`, so every mark has six equidistant
    /// neighbours. The classic hexagonal dot screen.
    Hexagonal,
}

/// Marks on a grid of `interval` mm, each `mark` mm long. `mark` runs
/// horizontally for the square lattice; for the hex lattice the marks are
/// horizontal too (a dense punch screen reads better than tilted ticks).
///
/// The local lattice is centred on the region, so the leftover margin is
/// the same on both sides, and only whole marks are kept. On a global
/// lattice marks are cut at the edge instead: a mark on a border shared
/// with another region is split between the two, so the border leaves no
/// seam.
pub fn fill_dots_with_lattice(
    region: &Region,
    interval: f64,
    angle: f64,
    mark: f64,
    lattice: Lattice,
    anchor: Anchor,
) -> Strokes {
    let mut out = Vec::new();
    fill_dots_with_lattice_into(region, interval, angle, mark, lattice, anchor, &mut out);
    out
}

/// [`fill_dots_with_lattice`] streamed: rows are emitted to `sink` as they
/// are made, so the peak is one row of marks, not the whole screen.
///
/// With a local anchor the whole mark is kept or dropped and rows arrive as
/// batches; with a global anchor each row is clipped before it is handed on.
pub fn fill_dots_with_lattice_into(
    region: &Region,
    interval: f64,
    angle: f64,
    mark: f64,
    lattice: Lattice,
    anchor: Anchor,
    sink: &mut dyn crate::Sink,
) {
    if interval <= 0.0 || region.is_empty() {
        return;
    }
    let row_pitch = match lattice {
        Lattice::Square => interval,
        Lattice::Hexagonal => interval * (3.0f64).sqrt() / 2.0,
    };
    let frame = Frame::anchored(anchor, centre(region), angle);
    let (u0, u1, v0, v1) = frame.span(region, interval);
    let mut row: Strokes = Vec::new();
    let mut last = i64::MIN;
    for j in indices(v0, v1, row_pitch) {
        if j != last && !row.is_empty() {
            emit_row(region, anchor, std::mem::take(&mut row), sink);
        }
        last = j;
        let offset = match lattice {
            Lattice::Hexagonal if j.rem_euclid(2) == 1 => interval / 2.0,
            _ => 0.0,
        };
        let v = j as f64 * row_pitch;
        for i in indices(u0, u1, interval) {
            let u = i as f64 * interval + offset;
            row.push(vec![frame.world((u - mark / 2.0, v)), frame.world((u + mark / 2.0, v))]);
        }
    }
    if !row.is_empty() {
        emit_row(region, anchor, row, sink);
    }
}

/// A local lattice keeps whole marks and filters by point; a global one is
/// cut at the boundary so neighbouring regions share the mark.
fn emit_row(region: &Region, anchor: Anchor, row: Strokes, sink: &mut dyn crate::Sink) {
    if anchor.is_global() {
        for s in row {
            for cut in region.clip_polyline(&s) {
                sink.stroke(cut);
            }
        }
    } else {
        sink.batch(filter_inside(row, region));
    }
}

/// Wavy lines `interval` mm apart; amplitude scales with the interval
/// (`wavelength = interval / 4`, `frequency = π / wavelength / 8`).
/// Port of `waves_pattern`; the local lattice is centred on the region.
pub fn fill_waves(region: &Region, interval: f64, angle: f64, anchor: Anchor) -> Strokes {
    let mut out = Vec::new();
    fill_waves_into(region, interval, angle, anchor, &mut out);
    out
}

/// [`fill_waves`] streamed: one wavy row at a time.
pub fn fill_waves_into(region: &Region, interval: f64, angle: f64, anchor: Anchor, sink: &mut dyn crate::Sink) {
    if interval <= 0.0 || region.is_empty() {
        return;
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let wavelength = interval / 4.0;
    let frequency_scale = std::f64::consts::PI / wavelength / 8.0;

    let (u0, u1, v0, v1) = frame.span(region, interval);
    let samples = indices(u0, u1, STEP);
    for k in indices(v0, v1, interval) {
        let v = k as f64 * interval;
        let coords: Vec<(f64, f64)> = samples
            .clone()
            .map(|m| {
                let u = m as f64 * STEP;
                frame.world((u, v + (u * frequency_scale).sin() * wavelength))
            })
            .collect();
        for cut in region.clip_polyline(&coords) {
            sink.stroke(cut);
        }
    }
}

/// [`fill_waves`] with optional perimeter-aware edge fraying (`fray` in mm):
/// wave amplitude eases down near the boundary and lines break into progressively
/// shorter dashes and dots before disappearing.
pub fn fill_waves_frayed(
    region: &Region,
    interval: f64,
    angle: f64,
    anchor: Anchor,
    fray: f64,
    seed: u64,
) -> Strokes {
    if fray <= 0.0 {
        return fill_waves(region, interval, angle, anchor);
    }
    if interval <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let wavelength = interval / 4.0;
    let frequency_scale = std::f64::consts::PI / wavelength / 8.0;
    let fray_noise = crate::noise::Perlin::new(seed.wrapping_add(811));

    let (u0, u1, v0, v1) = frame.span(region, interval);
    let samples = indices(u0, u1, STEP);
    let mut raw = Vec::new();
    for k in indices(v0, v1, interval) {
        let v = k as f64 * interval;
        let coords: Vec<(f64, f64)> = samples
            .clone()
            .map(|m| {
                let u = m as f64 * STEP;
                let p_base = frame.world((u, v));
                let w = crate::fray::whole_at(region, p_base, interval, fray, &fray_noise);
                let amp_scale = 0.25 + 0.75 * w;
                frame.world((u, v + (u * frequency_scale).sin() * wavelength * amp_scale))
            })
            .collect();
        raw.push(coords);
    }
    crate::fray::fray_strokes(region, raw, interval, fray, seed)
}

/// Ogee, the curved-diamond wallpaper lattice: two sets of waves running
/// the same way at `angle`, one the mirror of the other. Rows are
/// `spacing` mm apart and each wave swings half a row, so the crest of one
/// set meets the trough of the other and the cells close into onion-dome
/// diamonds.
pub fn fill_wave_grid(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    let amp = spacing * 0.5;
    // Cells 1.8 rows long: shorter reads as a wavy grid, not domes.
    let k = std::f64::consts::PI / (spacing * 1.8);
    let mut out = crate::field::fill_profile(region, spacing, angle, anchor, |x| amp * (k * x).sin());
    out.extend(crate::field::fill_profile(region, spacing, angle, anchor, |x| -amp * (k * x).sin()));
    out
}

/// [`fill_wave_grid`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_wave_grid_frayed(
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: Anchor,
    fray: f64,
    seed: u64,
) -> Strokes {
    let strokes = fill_wave_grid(region, spacing, angle, anchor);
    if fray > 0.0 {
        crate::fray::fray_strokes(region, strokes, spacing, fray, seed)
    } else {
        strokes
    }
}

/// Keep only sub-strokes whose points are (mostly) inside the region; a
/// sampled straight-line containment test splits at holes and boundaries.
fn filter_inside(strokes: Strokes, region: &Region) -> Strokes {
    let mut out: Strokes = Vec::new();
    for line in strokes {
        let mut current: Vec<(f64, f64)> = Vec::new();
        for &p in &line {
            if region.covers_point(p) {
                current.push(p);
            } else if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out.retain(|p| p.len() >= 2);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::Region;
    use crate::geo::polyline_length;

    fn blob() -> Region {
        Region::from_polygons(&[vec![vec![
            (10.0, 10.0),
            (30.0, 8.0),
            (38.0, 22.0),
            (28.0, 34.0),
            (12.0, 32.0),
            (6.0, 20.0),
        ]]])
    }

    #[test]
    fn dots_grid_covers_the_blob() {
        let strokes = fill_dots(&blob(), 2.0, 0.0, 0.3, Anchor::LOCAL);
        assert!(!strokes.is_empty());
        // Every kept point is within the region (filter_inside guarantees).
        for p in strokes.iter().flatten() {
            assert!(blob().covers_point(*p) || borderline(*p));
        }
    }

    fn borderline(p: (f64, f64)) -> bool {
        // Points on the region edge count as borderline, not violations —
        // sampled filtering may retain boundary-adjacent marks.
        let r = blob();
        let eps = 0.15;
        (0..8)
            .map(|k| {
                let a = k as f64 * std::f64::consts::PI / 4.0;
                (p.0 + eps * a.cos(), p.1 + eps * a.sin())
            })
            .any(|q| r.covers_point(q))
    }

    #[test]
    fn waves_span_the_region() {
        let strokes = fill_waves(&blob(), 2.0, 0.0, Anchor::LOCAL);
        assert!(!strokes.is_empty());
        let total: f64 = strokes.iter().map(|p| polyline_length(p)).sum();
        assert!(total > 200.0, "wave length {total}");
        // Wavy: interior points deviate from a straight baseline.
        let first = &strokes[0];
        let dy: f64 = (0..first.len())
            .map(|i| {
                let base = first[0].1;
                (first[i].1 - base).abs()
            })
            .sum();
        assert!(dy > 0.5, "wave should undulate, deviation {dy}");
    }

    #[test]
    fn anchored_dots_tile_across_a_shared_border() {
        let square = |x0: f64, x1: f64| {
            Region::from_polygons(&[vec![vec![(x0, 0.0), (x1, 0.0), (x1, 10.0), (x0, 10.0)]]])
        };
        let ink = |r: &Region| -> f64 {
            fill_dots_with_lattice(r, 0.45, 45.0, 0.2, Lattice::Square, Anchor::GLOBAL).iter().map(|s| polyline_length(s)).sum()
        };
        let whole = ink(&square(0.0, 10.0));
        let halves = ink(&square(0.0, 4.3)) + ink(&square(4.3, 10.0));
        assert!((whole - halves).abs() < 1e-6, "whole {whole} vs halves {halves}");
    }

    #[test]
    fn angle_rotates_dots() {
        let strokes = fill_dots(&blob(), 2.0, 30.0, 0.3, Anchor::LOCAL);
        assert!(!strokes.is_empty());
    }

    #[test]
    fn streamed_dots_equal_the_one_shot_fill() {
        for lattice in [Lattice::Square, Lattice::Hexagonal] {
            for anchor in [Anchor::LOCAL, Anchor::GLOBAL] {
                for angle in [0.0, 45.0] {
                    let region = blob();
                    let direct = fill_dots_with_lattice(&region, 0.7, angle, 0.25, lattice, anchor);
                    let mut streamed: Strokes = Vec::new();
                    fill_dots_with_lattice_into(&region, 0.7, angle, 0.25, lattice, anchor, &mut streamed);
                    assert_eq!(direct, streamed, "lattice {lattice:?} anchor {anchor:?} angle {angle}");
                }
            }
        }
    }
}
