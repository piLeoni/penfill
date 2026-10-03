//! Terrazzo: stone chips of many sizes set in a plain binder — a few big
//! shards, lots of small ones. About a third of the chips are dark: an open
//! hatch or cross-hatch inside the outline, never a solid block of ink. The
//! slab is cut by the wall like a real floor.

use crate::geo::{Pt, Region, Strokes};
use crate::random::Rng;
use crate::sweep::sweep_lines;

/// Biggest chips about `size` mm across; dark chips hatched `hatch` mm
/// apart. Keep `hatch` well above the pen width (size / 6 reads well) or
/// the dark chips plot as blots.
pub fn fill_terrazzo(region: &Region, size: f64, hatch: f64, seed: u64) -> Strokes {
    if size <= 0.0 || hatch <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let (w, h) = (maxx - minx + size, maxy - miny + size);
    let (x0, y0) = (minx - size / 2.0, miny - size / 2.0);
    let mut rng = Rng::new(seed.wrapping_add(13));
    let gap = size * 0.06;

    // Radii biggest first, skewed small: a few shards, many crumbs.
    let count = (w * h / (size * size) * 18.0) as usize;
    let mut radii: Vec<f64> = (0..count).map(|_| size / 2.0 * (0.12 + 0.88 * rng.unit().powi(3))).collect();
    radii.sort_by(|a, b| b.partial_cmp(a).unwrap());

    // Accepted chips binned by centre so the overlap test stays local.
    let bin = size + gap;
    let cols = (w / bin) as usize + 1;
    let rows = (h / bin) as usize + 1;
    let mut bins: Vec<Vec<(Pt, f64)>> = vec![Vec::new(); cols * rows];
    let mut chips: Vec<(Pt, f64)> = Vec::new();
    for r in radii {
        for _ in 0..20 {
            let c = (x0 + rng.unit() * w, y0 + rng.unit() * h);
            let (bc, br) = (((c.0 - x0) / bin) as usize, ((c.1 - y0) / bin) as usize);
            let free = (br.saturating_sub(1)..=(br + 1).min(rows - 1)).all(|rr| {
                (bc.saturating_sub(1)..=(bc + 1).min(cols - 1)).all(|cc| {
                    bins[rr * cols + cc].iter().all(|&(q, rq)| (q.0 - c.0).hypot(q.1 - c.1) >= r + rq + gap)
                })
            });
            if free {
                bins[br * cols + bc].push((c, r));
                chips.push((c, r));
                break;
            }
        }
    }

    let mut out: Strokes = Vec::new();
    for (c, r) in chips {
        let ring = shard(c, r, &mut rng);
        let dark = rng.unit() < 0.35;
        let hatch_angle = rng.range(0.0, 180.0);
        out.extend(region.clip_polyline(&ring));
        if dark {
            let inside = Region::from_ring(&ring).intersection(region);
            out.extend(sweep_lines(&inside, hatch, hatch_angle));
            if rng.coin() {
                out.extend(sweep_lines(&inside, hatch, hatch_angle + 90.0));
            }
        }
    }
    out
}

/// A convex-ish shard: 5–8 corners at jittered angles and radii.
fn shard(c: Pt, r: f64, rng: &mut Rng) -> Vec<Pt> {
    let n = 5 + (rng.next_u64() % 4) as usize;
    let turn = rng.range(0.0, std::f64::consts::TAU);
    let mut ring: Vec<Pt> = (0..n)
        .map(|k| {
            let a = turn + std::f64::consts::TAU * (k as f64 + rng.range(-0.3, 0.3)) / n as f64;
            let d = r * rng.range(0.65, 1.0);
            (c.0 + d * a.cos(), c.1 + d * a.sin())
        })
        .collect();
    ring.push(ring[0]);
    ring
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_inside_and_some_dark() {
        let sq = Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)]);
        let t = fill_terrazzo(&sq, 6.0, 1.0, 3);
        let closed = t.iter().filter(|s| s.len() > 3 && s.first() == s.last()).count();
        let hatch = t.iter().filter(|s| s.len() == 2).count();
        assert!(closed > 60, "only {closed} whole chips");
        assert!(hatch > 30, "only {hatch} hatch lines");
        for p in t.iter().flatten() {
            assert!(sq.covers_point(*p), "{p:?} outside");
        }
    }
}
