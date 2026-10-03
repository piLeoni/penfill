//! Seeded randomness for the scattered fills. The same seed always gives
//! the same drawing, on every platform — no external RNG crate.

use crate::geo::{Pt, Region};

/// SplitMix64: tiny, fast, and good enough for placing marks.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

/// Poisson-disk points inside `region`: random, but no two closer than
/// `min_dist` (Bridson 2007). Grown over the whole bounding box and then
/// filtered, so separate parts of the region all get points.
pub fn poisson_disk(region: &Region, min_dist: f64, seed: u64) -> Vec<Pt> {
    if min_dist <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let (w, h) = (maxx - minx, maxy - miny);
    let cell = min_dist / std::f64::consts::SQRT_2;
    let cols = (w / cell).ceil() as usize + 1;
    let rows = (h / cell).ceil() as usize + 1;
    let mut grid: Vec<Option<usize>> = vec![None; cols * rows];
    let mut pts: Vec<Pt> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut rng = Rng::new(seed);
    let slot = |p: Pt| {
        let c = (((p.0 - minx) / cell) as usize).min(cols - 1);
        let r = (((p.1 - miny) / cell) as usize).min(rows - 1);
        (c, r)
    };

    let first = (rng.range(minx, maxx), rng.range(miny, maxy));
    let (c, r) = slot(first);
    grid[r * cols + c] = Some(0);
    pts.push(first);
    active.push(0);

    const TRIES: usize = 30;
    while !active.is_empty() {
        let ai = (rng.next_u64() as usize) % active.len();
        let base = pts[active[ai]];
        let mut placed = false;
        for _ in 0..TRIES {
            let a = rng.range(0.0, std::f64::consts::TAU);
            let d = rng.range(min_dist, 2.0 * min_dist);
            let p = (base.0 + d * a.cos(), base.1 + d * a.sin());
            if p.0 < minx || p.0 > maxx || p.1 < miny || p.1 > maxy {
                continue;
            }
            let (c, r) = slot(p);
            let mut ok = true;
            'near: for rr in r.saturating_sub(2)..=(r + 2).min(rows - 1) {
                for cc in c.saturating_sub(2)..=(c + 2).min(cols - 1) {
                    if let Some(j) = grid[rr * cols + cc] {
                        let q = pts[j];
                        if (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2) < min_dist * min_dist {
                            ok = false;
                            break 'near;
                        }
                    }
                }
            }
            if ok {
                grid[r * cols + c] = Some(pts.len());
                active.push(pts.len());
                pts.push(p);
                placed = true;
                break;
            }
        }
        if !placed {
            active.swap_remove(ai);
        }
    }
    pts.retain(|&p| region.covers_point(p));
    pts
}

/// Poisson-disk points whose spacing changes over the region: two points
/// keep at least the average of `radius` at each of them apart. `radius`
/// must stay within `[r_min, r_max]`.
pub fn poisson_disk_varying(
    region: &Region,
    r_min: f64,
    r_max: f64,
    radius: impl Fn(Pt) -> f64,
    seed: u64,
) -> Vec<Pt> {
    if r_min <= 0.0 || r_max < r_min || region.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let cell = r_min / std::f64::consts::SQRT_2;
    let cols = ((maxx - minx) / cell).ceil() as usize + 1;
    let rows = ((maxy - miny) / cell).ceil() as usize + 1;
    let reach = (r_max / cell).ceil() as usize;
    let mut grid: Vec<Option<usize>> = vec![None; cols * rows];
    let mut pts: Vec<(Pt, f64)> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut rng = Rng::new(seed);
    let slot = |p: Pt| {
        let c = (((p.0 - minx) / cell) as usize).min(cols - 1);
        let r = (((p.1 - miny) / cell) as usize).min(rows - 1);
        (c, r)
    };
    let r_at = |p: Pt| {
        let r = radius(p);
        if r.is_nan() { r_max } else { r.clamp(r_min, r_max) }
    };

    let first = (rng.range(minx, maxx), rng.range(miny, maxy));
    let (c, r) = slot(first);
    grid[r * cols + c] = Some(0);
    pts.push((first, r_at(first)));
    active.push(0);

    const TRIES: usize = 30;
    while !active.is_empty() {
        let ai = (rng.next_u64() as usize) % active.len();
        let (base, rb) = pts[active[ai]];
        let mut placed = false;
        for _ in 0..TRIES {
            let a = rng.range(0.0, std::f64::consts::TAU);
            let d = rng.range(rb, 2.0 * rb);
            let p = (base.0 + d * a.cos(), base.1 + d * a.sin());
            if p.0 < minx || p.0 > maxx || p.1 < miny || p.1 > maxy {
                continue;
            }
            let rp = r_at(p);
            let (c, r) = slot(p);
            let mut ok = true;
            'near: for rr in r.saturating_sub(reach)..=(r + reach).min(rows - 1) {
                for cc in c.saturating_sub(reach)..=(c + reach).min(cols - 1) {
                    if let Some(j) = grid[rr * cols + cc] {
                        let (q, rq) = pts[j];
                        let need = (rp + rq) / 2.0;
                        if (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2) < need * need {
                            ok = false;
                            break 'near;
                        }
                    }
                }
            }
            if ok {
                grid[r * cols + c] = Some(pts.len());
                active.push(pts.len());
                pts.push((p, rp));
                placed = true;
                break;
            }
        }
        if !placed {
            active.swap_remove(ai);
        }
    }
    pts.into_iter().map(|(p, _)| p).filter(|&p| region.covers_point(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_points() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        assert_eq!(poisson_disk(&sq, 1.0, 3), poisson_disk(&sq, 1.0, 3));
        assert_ne!(poisson_disk(&sq, 1.0, 3), poisson_disk(&sq, 1.0, 4));
    }

    #[test]
    fn varying_radius_thins_out_where_asked() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        // Dense on the left, sparse on the right.
        let pts = poisson_disk_varying(&sq, 0.5, 2.0, |p| 0.5 + 1.5 * p.0 / 20.0, 3);
        let left = pts.iter().filter(|p| p.0 < 5.0).count();
        let right = pts.iter().filter(|p| p.0 > 15.0).count();
        assert!(left > 4 * right, "left {left}, right {right}");
        for i in 0..pts.len() {
            for j in i + 1..pts.len() {
                let d = ((pts[i].0 - pts[j].0).powi(2) + (pts[i].1 - pts[j].1).powi(2)).sqrt();
                assert!(d >= 0.5 - 1e-9, "points {i} and {j} only {d} apart");
            }
        }
    }

    #[test]
    fn points_keep_their_distance_and_fill_the_square() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let pts = poisson_disk(&sq, 1.0, 1);
        for i in 0..pts.len() {
            for j in i + 1..pts.len() {
                let d = ((pts[i].0 - pts[j].0).powi(2) + (pts[i].1 - pts[j].1).powi(2)).sqrt();
                assert!(d >= 1.0 - 1e-9, "points {i} and {j} only {d} apart");
            }
        }
        // Bridson packs roughly one point per 1.5–2 r² of area.
        assert!(pts.len() > 150, "only {} points", pts.len());
    }
}
