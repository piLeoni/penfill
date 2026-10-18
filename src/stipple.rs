//! Stippling: scattered dots with no clumps and no grid.
//!
//! Dots sit on Poisson-disk points (`random::poisson_disk`): random, yet
//! never closer than `spacing`. Each dot is a short mark at a random angle
//! — a pen needs a stroke, and random angles keep the texture from reading
//! as a direction.

use crate::geo::{Pt, Region, Strokes};
use crate::noise::Perlin;
use crate::random::{poisson_disk, poisson_disk_varying, Rng};

/// Dots at least `spacing` mm apart, each `mark` mm long.
pub fn fill_stipple(region: &Region, spacing: f64, mark: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let target = region.inset(mark / 2.0);
    marks(poisson_disk(&target, spacing, seed), mark, seed)
}

/// Dots whose density follows `field`: where it is 0 they are `dense` mm
/// apart, where it is 1 they are `sparse` mm apart (either may be larger),
/// in between the ink per area blends evenly (so a linear field reads as a
/// linear gradient).
pub fn fill_stipple_field(
    region: &Region,
    dense: f64,
    sparse: f64,
    mark: f64,
    seed: u64,
    field: impl Fn(Pt) -> f64,
) -> Strokes {
    if dense <= 0.0 || sparse <= 0.0 || region.is_empty() {
        return vec![];
    }
    marks(scatter_field(&region.inset(mark / 2.0), dense, sparse, seed, field), mark, seed)
}

/// The points under [`fill_stipple_field`], before any mark is drawn:
/// random, no two closer than the local spacing, which runs from `dense`
/// where `field` is 0 to `sparse` where it is 1 (either may be larger;
/// NaN reads as 1).
pub fn scatter_field(region: &Region, dense: f64, sparse: f64, seed: u64, field: impl Fn(Pt) -> f64) -> Vec<Pt> {
    if dense <= 0.0 || sparse <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (r_min, r_max) = (dense.min(sparse), dense.max(sparse));
    let (a, b) = (1.0 / (dense * dense), 1.0 / (sparse * sparse));
    let radius = |p: Pt| {
        let t = field(p);
        let t = if t.is_nan() { 1.0 } else { t.clamp(0.0, 1.0) };
        1.0 / (a + t * (b - a)).max(1e-12).sqrt()
    };
    poisson_disk_varying(region, r_min, r_max, radius, seed)
}

/// Clouds: density drifts with Perlin noise whose blobs are about
/// `feature` mm across.
pub fn fill_stipple_noise(region: &Region, dense: f64, sparse: f64, mark: f64, feature: f64, seed: u64) -> Strokes {
    let noise = Perlin::new(seed);
    let f = feature.max(1e-6);
    fill_stipple_field(region, dense, sparse, mark, seed, |p| cloud(&noise, p, f))
}

/// Dense along the walls, thinning out over `falloff` mm toward the inside
/// — the cartographic shading for lakes and coastlines. `rough` (0–1) mixes
/// in Perlin noise so the fade doesn't read as rings parallel to the wall.
pub fn fill_stipple_edge(
    region: &Region,
    dense: f64,
    sparse: f64,
    mark: f64,
    falloff: f64,
    rough: f64,
    seed: u64,
) -> Strokes {
    let noise = Perlin::new(seed);
    let f = falloff.max(1e-6);
    fill_stipple_field(region, dense, sparse, mark, seed, |p| {
        let edge = (region.dist_to_boundary(p) / f).clamp(0.0, 1.0);
        let n = noise.fbm(p.0 / f, p.1 / f, 2) * 0.5;
        edge + rough * n
    })
}

/// Noise mapped to `[0, 1]` with the middle stretched, so the drawing has
/// real clearings and real clusters instead of an even grey.
fn cloud(noise: &Perlin, p: Pt, feature: f64) -> f64 {
    let n = noise.fbm(p.0 / feature, p.1 / feature, 3);
    let t = (0.5 + n * 1.2).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A short stroke at a random angle on each point — a pen needs a stroke.
fn marks(points: Vec<Pt>, mark: f64, seed: u64) -> Strokes {
    let mut rng = Rng::new(seed.wrapping_add(1));
    points
        .into_iter()
        .map(|p| {
            let a = rng.range(0.0, std::f64::consts::PI);
            let (dx, dy) = (a.cos() * mark / 2.0, a.sin() * mark / 2.0);
            vec![(p.0 - dx, p.1 - dy), (p.0 + dx, p.1 + dy)]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_stay_inside_and_apart() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let dots = fill_stipple(&sq, 1.5, 0.3, 9);
        assert!(dots.len() > 60, "only {} dots", dots.len());
        for d in &dots {
            for p in d {
                assert!(sq.covers_point(*p), "dot end {p:?} outside");
            }
        }
    }

    #[test]
    fn nan_field_reads_as_sparse() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let nan = fill_stipple_field(&sq, 0.5, 4.0, 0.2, 3, |_| f64::NAN);
        let sparse = fill_stipple_field(&sq, 0.5, 4.0, 0.2, 3, |_| 1.0);
        assert_eq!(nan.len(), sparse.len());
    }

    #[test]
    fn edge_shading_is_dense_at_the_wall() {
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let dots = fill_stipple_edge(&sq, 0.6, 2.5, 0.2, 8.0, 0.0, 4);
        let (mut rim, mut core) = (0, 0);
        for d in &dots {
            let m = ((d[0].0 + d[1].0) / 2.0, (d[0].1 + d[1].1) / 2.0);
            let wall = m.0.min(m.1).min(30.0 - m.0).min(30.0 - m.1);
            if wall < 3.0 {
                rim += 1;
            } else if (m.0 - 15.0).abs() < 5.0 && (m.1 - 15.0).abs() < 5.0 {
                core += 1;
            }
        }
        // Rim band is 324 mm², core 100 mm²; per area the rim must be far denser.
        assert!(rim as f64 / 324.0 > 4.0 * core as f64 / 100.0, "rim {rim}, core {core}");
    }
}
