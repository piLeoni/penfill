//! Perimeter-aware edge fraying: lines and marks that sense the region
//! boundary, break into shorter and shorter dashes, and disappear smoothly
//! before the wall instead of stopping at a hard cut.

use crate::geo::{polyline_length, Pt, Region, Strokes, MIN_SEGMENT_MM};
use crate::noise::Perlin;
use crate::random::Rng;

/// Smooth `[0, 1]` survival weight at `p` for a fray band of `fray` mm inside
/// `region`. Returns `1.0` deep inside, `0.0` at or near the boundary, and a
/// noise-modulated ramp in between so the edge wanders organically.
pub fn whole_at(region: &Region, p: Pt, spacing: f64, fray: f64, noise: &Perlin) -> f64 {
    if fray <= 0.0 {
        return if region.covers_point(p) { 1.0 } else { 0.0 };
    }
    if !region.covers_point(p) {
        return 0.0;
    }
    let d = region.dist_to_boundary(p);
    let start = (fray * 0.12).min(spacing * 0.35);
    let band = (fray - start).max(1e-3);
    let scale = fray.max(spacing * 2.5).max(1.5);
    let wander = noise.fbm(p.0 / scale, p.1 / scale, 2) * 0.65;
    ((d - start) / band + wander).clamp(0.0, 1.0)
}

/// Fray `strokes` inside `region` over a `fray` mm band from the perimeter:
/// deep inside, strokes stay whole; approaching the boundary, they break into
/// progressively shorter dashes and dots before vanishing smoothly.
pub fn fray_strokes(
    region: &Region,
    strokes: Strokes,
    spacing: f64,
    fray: f64,
    seed: u64,
) -> Strokes {
    let clipped = region.clip_strokes(&strokes);
    if fray <= 0.0 || spacing <= 0.0 || clipped.is_empty() {
        return clipped;
    }
    let noise = Perlin::new(seed ^ 0xF4A7_5EA1);
    let period = (spacing * 1.45).max(0.6);
    let step = (spacing * 0.08).clamp(0.04, 0.20);
    let dot_len = (spacing * 0.12).clamp(0.06, 0.22);

    let mut out = Vec::new();
    for (i, line) in clipped.into_iter().enumerate() {
        if line.len() < 2 {
            continue;
        }
        // Fast path: if every vertex is well inside the solid core, keep as-is.
        if line
            .iter()
            .all(|&p| region.dist_to_boundary(p) >= fray * 1.45)
        {
            out.push(line);
            continue;
        }
        let mut rng = Rng::new(seed ^ 0x5EA1_F4A7 ^ (i as u64).wrapping_mul(0x9E37_79B9));
        let pts = resample(&line, step);
        if pts.len() < 2 {
            continue;
        }
        let phase = rng.range(0.0, period);
        let mut piece: Vec<Pt> = Vec::new();
        let mut s = phase;
        let mut cell = usize::MAX;
        let mut ink = (0.0f64, f64::MAX);

        for k in 0..pts.len() {
            if k > 0 {
                s += dist(pts[k - 1], pts[k]);
            }
            let c = (s / period) as usize;
            if c != cell {
                cell = c;
                let w = whole_at(region, pts[k], spacing, fray, &noise);
                ink = if w >= 0.96 {
                    (0.0, f64::MAX)
                } else if w <= 0.04 {
                    (f64::MAX, 0.0)
                } else {
                    let mut len = period * w.powf(0.85) * rng.range(0.65, 1.35);
                    if len < dot_len {
                        len = if rng.unit() < w * 1.35 { dot_len } else { 0.0 };
                    }
                    let len = len.min(period * 0.95);
                    let cell_start = c as f64 * period;
                    let at = cell_start + rng.range(0.0, (period - len).max(0.0));
                    (at, at + len)
                };
            }
            if s >= ink.0 && s <= ink.1 {
                piece.push(pts[k]);
            } else if !piece.is_empty() {
                if piece.len() >= 2 && polyline_length(&piece) >= MIN_SEGMENT_MM {
                    out.push(std::mem::take(&mut piece));
                }
                piece.clear();
            }
        }
        if piece.len() >= 2 && polyline_length(&piece) >= MIN_SEGMENT_MM {
            out.push(piece);
        }
    }
    out
}

fn dist(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Resample a polyline so consecutive points are at most `step` mm apart.
pub(crate) fn resample(pts: &[Pt], step: f64) -> Vec<Pt> {
    if pts.len() < 2 || step <= 0.0 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    let mut left = step;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = dist(a, b);
        if len <= 1e-12 {
            continue;
        }
        let mut at = 0.0;
        while len - at >= left {
            at += left;
            let t = at / len;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            left = step;
        }
        left -= len - at;
    }
    if out.last().is_none_or(|&p| dist(p, *pts.last().unwrap()) > 1e-9) {
        out.push(*pts.last().unwrap());
    }
    out
}
