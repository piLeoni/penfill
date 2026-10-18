//! Perimeter-aware edge fraying: lines, marks, and cells that sense the region
//! boundary, break into shorter dashes or scatter-fade, and disappear smoothly
//! before the wall instead of stopping at a hard cut.

use crate::geo::{polyline_length, Pt, Region, Strokes, MIN_SEGMENT_MM};
use crate::noise::Perlin;
use crate::random::Rng;
use crate::stream::Sink;

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

/// Smooth `[0, 1]` perimeter ramp at `p` over `falloff` mm from the boundary
/// (`0.0` at the wall, `1.0` at `falloff` mm or deeper inside), mixed with
/// `rough` (`0..1`) Perlin noise so density transitions don't form rigid rings.
pub fn border_ramp(region: &Region, p: Pt, falloff: f64, rough: f64, noise: &Perlin) -> f64 {
    if falloff <= 0.0 {
        return 1.0;
    }
    let f = falloff.max(1e-6);
    let edge = (region.dist_to_boundary(p) / f).clamp(0.0, 1.0);
    let n = noise.fbm(p.0 / f, p.1 / f, 2) * 0.5;
    (edge + rough.clamp(0.0, 1.0) * n).clamp(0.0, 1.0)
}

/// Fray `strokes` inside `region` over a `fray` mm band from the perimeter:
/// - Deep inside, strokes stay whole.
/// - Approaching the boundary:
///   - Long polylines break into progressively shorter dashes and dots.
///   - Short discrete marks/dots stochastically thin out and shrink.
///   - Small closed cell rings shrink toward their centroid and break into arcs.
pub fn fray_strokes(
    region: &Region,
    strokes: Strokes,
    spacing: f64,
    fray: f64,
    seed: u64,
) -> Strokes {
    let mut out = Vec::new();
    fray_strokes_into(region, strokes, spacing, fray, seed, &mut out);
    out
}

/// Streaming form of [`fray_strokes`]: clips and frays `strokes` into `sink`.
pub fn fray_strokes_into(
    region: &Region,
    strokes: Strokes,
    spacing: f64,
    fray: f64,
    seed: u64,
    sink: &mut dyn Sink,
) {
    let clipped = region.clip_strokes(&strokes);
    if fray <= 0.0 || spacing <= 0.0 || clipped.is_empty() {
        sink.batch(clipped);
        return;
    }
    let noise = Perlin::new(seed ^ 0xF4A7_5EA1);
    for (i, line) in clipped.into_iter().enumerate() {
        fray_clipped_stroke(region, line, i, spacing, fray, seed, &noise, sink);
    }
}

/// Fray a single already-clipped stroke `line` with deterministic index `i`.
pub(crate) fn fray_clipped_stroke(
    region: &Region,
    line: Vec<Pt>,
    i: usize,
    spacing: f64,
    fray: f64,
    seed: u64,
    noise: &Perlin,
    sink: &mut dyn Sink,
) {
    if line.len() < 2 {
        return;
    }
    // Fast path: if every vertex is well inside the solid core, keep as-is.
    if line
        .iter()
        .all(|&p| region.dist_to_boundary(p) >= fray * 1.45)
    {
        sink.stroke(line);
        return;
    }

    let mut rng = Rng::new(seed ^ 0x5EA1_F4A7 ^ (i as u64).wrapping_mul(0x9E37_79B9));
    let len = polyline_length(&line);
    if len < MIN_SEGMENT_MM {
        return;
    }

    let is_closed = line.len() >= 4 && dist(line[0], *line.last().unwrap()) <= 1e-4;
    let diam = bbox_diameter(&line);

    // Case 1: Short discrete mark / tick (e.g. dots, stipple, litho ticks, sprouts).
    if !is_closed && len <= spacing * 0.75 {
        let mid = centroid(&line);
        let w = whole_at(region, mid, spacing, fray, noise);
        if w <= 0.04 || rng.unit() > w.powf(1.15) {
            return;
        }
        let scale = 0.35 + 0.65 * w;
        let scaled: Vec<Pt> = line
            .into_iter()
            .map(|p| (mid.0 + (p.0 - mid.0) * scale, mid.1 + (p.1 - mid.1) * scale))
            .collect();
        if polyline_length(&scaled) >= MIN_SEGMENT_MM {
            sink.stroke(scaled);
        }
        return;
    }

    // Case 2: Small closed ring / cell (e.g. pebbles, gravel stones, small pads).
    let line = if is_closed && diam <= spacing * 1.85 {
        let mid = centroid(&line[..line.len() - 1]);
        let w = whole_at(region, mid, spacing, fray, noise);
        if w <= 0.05 || rng.unit() > w.powf(0.95) {
            return;
        }
        let scale = 0.40 + 0.60 * w;
        let scaled: Vec<Pt> = line
            .into_iter()
            .map(|p| (mid.0 + (p.0 - mid.0) * scale, mid.1 + (p.1 - mid.1) * scale))
            .collect();
        if w >= 0.65 {
            if polyline_length(&scaled) >= MIN_SEGMENT_MM {
                sink.stroke(scaled);
            }
            return;
        }
        scaled
    } else {
        line
    };

    // Case 3: Continuous polyline (or low-weight closed ring) -> dash & dot erosion.
    let period = (spacing * 1.45).max(0.6);
    let step = (spacing * 0.08).clamp(0.04, 0.20);
    let dot_len = (spacing * 0.12).clamp(0.06, 0.22);

    let pts = resample(&line, step);
    if pts.len() < 2 {
        return;
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
            let w = whole_at(region, pts[k], spacing, fray, noise);
            ink = if w >= 0.96 {
                (0.0, f64::MAX)
            } else if w <= 0.04 {
                (f64::MAX, 0.0)
            } else {
                let mut dlen = period * w.powf(0.85) * rng.range(0.65, 1.35);
                if dlen < dot_len {
                    dlen = if rng.unit() < w * 1.35 { dot_len } else { 0.0 };
                }
                let dlen = dlen.min(period * 0.95);
                let cell_start = c as f64 * period;
                let at = cell_start + rng.range(0.0, (period - dlen).max(0.0));
                (at, at + dlen)
            };
        }
        if s >= ink.0 && s <= ink.1 {
            piece.push(pts[k]);
        } else if !piece.is_empty() {
            if piece.len() >= 2 && polyline_length(&piece) >= MIN_SEGMENT_MM {
                sink.stroke(std::mem::take(&mut piece));
            }
            piece.clear();
        }
    }
    if piece.len() >= 2 && polyline_length(&piece) >= MIN_SEGMENT_MM {
        sink.stroke(piece);
    }
}

fn dist(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn centroid(pts: &[Pt]) -> Pt {
    if pts.is_empty() {
        return (0.0, 0.0);
    }
    let (sx, sy) = pts.iter().fold((0.0, 0.0), |(ax, ay), &(x, y)| (ax + x, ay + y));
    let n = pts.len() as f64;
    (sx / n, sy / n)
}

fn bbox_diameter(pts: &[Pt]) -> f64 {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x1 - x0).hypot(y1 - y0)
}

/// Resample a polyline so consecutive points are at most `step` mm apart.
pub fn resample(pts: &[Pt], step: f64) -> Vec<Pt> {
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

/// Chaikin corner-cutting smoothing over `rounds` passes. Closed rings stay
/// closed; open polylines keep their endpoints fixed.
pub fn smooth(pts: &[Pt], rounds: usize) -> Vec<Pt> {
    if pts.len() < 3 || rounds == 0 {
        return pts.to_vec();
    }
    let closed = pts.len() >= 4 && dist(pts[0], *pts.last().unwrap()) <= 1e-6;
    let mut cur = pts.to_vec();
    for _ in 0..rounds {
        if closed {
            let ring = &cur[..cur.len() - 1];
            let n = ring.len();
            let mut next = Vec::with_capacity(n * 2 + 1);
            for i in 0..n {
                let a = ring[i];
                let b = ring[(i + 1) % n];
                next.push((0.75 * a.0 + 0.25 * b.0, 0.75 * a.1 + 0.25 * b.1));
                next.push((0.25 * a.0 + 0.75 * b.0, 0.25 * a.1 + 0.75 * b.1));
            }
            if let Some(&first) = next.first() {
                next.push(first);
            }
            cur = next;
        } else {
            let mut next = Vec::with_capacity(cur.len() * 2);
            next.push(cur[0]);
            for w in cur.windows(2) {
                let (a, b) = (w[0], w[1]);
                next.push((0.75 * a.0 + 0.25 * b.0, 0.75 * a.1 + 0.25 * b.1));
                next.push((0.25 * a.0 + 0.75 * b.0, 0.25 * a.1 + 0.75 * b.1));
            }
            next.push(*cur.last().unwrap());
            cur = next;
        }
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::Anchor;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)])
    }

    #[test]
    fn fray_fragments_straight_lines_near_border() {
        let r = square();
        let solid = crate::grid::fill_lines(&r, 1.5, 0.0, Anchor::LOCAL);
        let frayed = fray_strokes(&r, solid.clone(), 1.5, 8.0, 42);
        let solid_ink: f64 = solid.iter().map(|s| polyline_length(s)).sum();
        let frayed_ink: f64 = frayed.iter().map(|s| polyline_length(s)).sum();
        // Fraying removes ink near the border and breaks lines into more segments.
        assert!(frayed_ink < solid_ink * 0.9, "frayed {frayed_ink} vs solid {solid_ink}");
        assert!(frayed.len() > solid.len(), "frayed segments {} vs solid {}", frayed.len(), solid.len());
    }

    #[test]
    fn fray_thins_and_shrinks_short_dots_near_border() {
        let r = square();
        let dots = crate::dotswaves::fill_dots(&r, 1.5, 0.0, 0.3, Anchor::LOCAL);
        let frayed = fray_strokes(&r, dots.clone(), 1.5, 10.0, 7);
        assert!(!frayed.is_empty());
        assert!(frayed.len() < dots.len(), "frayed dots {} should be fewer than {}", frayed.len(), dots.len());
    }

    #[test]
    fn fray_thins_and_erodes_small_closed_cells() {
        let r = square();
        let pebbles = crate::nature::fill_pebbles(&r, 3.0, 9);
        let frayed = fray_strokes(&r, pebbles.clone(), 3.0, 10.0, 9);
        let orig_ink: f64 = pebbles.iter().map(|s| polyline_length(s)).sum();
        let frayed_ink: f64 = frayed.iter().map(|s| polyline_length(s)).sum();
        assert!(frayed_ink < orig_ink * 0.85, "frayed {frayed_ink} vs orig {orig_ink}");
    }
}
