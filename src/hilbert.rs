//! Space-filling Hilbert curve: one long pen-down path that visits the
//! whole region cell by cell.

use crate::geo::{Pt, Region, Strokes};

/// Hilbert path clipped to `region`. `spacing` is the largest step between
/// turns: the curve runs on a power-of-two square grid, so the cell size
/// lands between `spacing / 2` and `spacing`. `inset_mm` defaults to half
/// the spacing.
pub fn fill_hilbert(region: &Region, spacing: f64, inset_mm: Option<f64>) -> Strokes {
    let amount = inset_mm.unwrap_or_else(|| spacing / 2.0);
    let target = region.inset(amount);
    if spacing <= 0.0 || target.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = target.bounds_mm();
    let (w, h) = (maxx - minx, maxy - miny);
    if w <= 0.0 || h <= 0.0 {
        return vec![];
    }
    let side = w.max(h);
    let order = ((side / spacing).log2().ceil() as u32).clamp(1, 12);
    let n = 1u32 << order;
    let cell = side / n as f64;
    let (ox, oy) = ((minx + minx + w - side) / 2.0, (miny + miny + h - side) / 2.0);
    let count = (n as u64) * (n as u64);
    let pts: Vec<Pt> = (0..count)
        .map(|d| {
            let (i, j) = d2xy(n, d as u32);
            (ox + (i as f64 + 0.5) * cell, oy + (j as f64 + 0.5) * cell)
        })
        .collect();
    target.clip_polyline(&pts)
}

/// Grid coordinate for Hilbert index `d` on an `n`×`n` grid (`n` a power of two).
fn d2xy(n: u32, mut d: u32) -> (u32, u32) {
    let (mut x, mut y) = (0u32, 0u32);
    let mut s = 1u32;
    while s < n {
        let rx = 1 & (d / 2);
        let ry = 1 & (d ^ rx);
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += s * rx;
        y += s * ry;
        d /= 4;
        s *= 2;
    }
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::polyline_length;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)])
    }

    #[test]
    fn visits_the_interior_in_one_clip() {
        let out = fill_hilbert(&square(), 4.0, None);
        assert!(!out.is_empty());
        let ink: f64 = out.iter().map(|s| polyline_length(s)).sum();
        let area = 36.0 * 36.0;
        assert!(ink > 0.9 * area / 4.0 && ink < 2.0 * area / 4.0, "ink {ink}");
        for s in &out {
            for w in s.windows(2) {
                let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
                assert!(square().covers_point(mid), "{mid:?}");
            }
        }
    }

    #[test]
    fn empty_when_too_tight() {
        assert!(fill_hilbert(&square(), 1.0, Some(25.0)).is_empty());
    }
}
