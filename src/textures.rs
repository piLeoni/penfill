//! Natural textures drawn as contour lines of a noise-driven field
//! (`field::fill_isolines`): marble veins, wood grain, cracked ground.

use crate::field::{fill_isolines, levels_every};
use crate::anchor::Anchor;
use crate::geo::{Pt, Region, Strokes};
use crate::noise::Perlin;
use crate::random::{poisson_disk, Rng};

/// Marble: veins that drift across the slab at `angle`, about `spacing` mm
/// apart, warped by turbulence. Each vein is drawn as its two edges, so it
/// pinches and swells like a real one.
pub fn fill_marble(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let noise = Perlin::new(seed);
    let thick = Perlin::new(seed.wrapping_add(1));
    let (sin, cos) = angle.to_radians().sin_cos();
    let warp = spacing * 1.2;
    // The veins are a field over the page; the anchor moves the field.
    let at = anchor.resolve((0.0, 0.0));
    let f = |p: Pt| {
        let p = (p.0 - at.0, p.1 - at.1);
        let across = -p.0 * sin + p.1 * cos;
        let (u, v) = (p.0 / (spacing * 4.0), p.1 / (spacing * 4.0));
        // Domain warp twice: slow drift, then a folded ripple on top.
        let w1 = noise.fbm(u, v, 3);
        let w2 = noise.fbm(u * 2.0 + 5.2 * w1, v * 2.0 - 3.1 * w1, 2);
        let g = across + warp * (w1 + 0.35 * w2);
        // Veins swell and thin along their length, never vanishing.
        let t = 0.5 + 0.5 * thick.noise(u * 0.8 + 9.0, v * 0.8);
        (std::f64::consts::PI * g / spacing).sin().abs() - (0.1 + 0.14 * t)
    };
    // Sampled finer than the thinnest vein, or it breaks into dashes.
    fill_isolines(region, (spacing / 25.0).clamp(0.05, 0.3), &[0.0], f)
}

/// Wood grain: long lines `spacing` mm apart running along `angle`,
/// wandering slowly, with a few knots the grain flows around.
/// The grain is a field over the page, moved by the anchor; the knots are
/// placed inside each board.
pub fn fill_wood(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let noise = Perlin::new(seed);
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let area = (maxx - minx) * (maxy - miny);
    let mut rng = Rng::new(seed.wrapping_add(9));
    // About one knot per 900 mm², at least one.
    let n_knots = ((area / 900.0).round() as usize).max(1);
    // Knots sit inside the board: one just outside the wall only crowds
    // the grain into a dark wedge along the edge.
    let knots: Vec<(Pt, f64)> = (0..n_knots)
        .filter_map(|_| {
            // Knot size is about the board, not the line spacing: at fine
            // spacing a spacing-sized knot is a black blot.
            let r = spacing + rng.range(2.5, 5.0);
            let inner = region.inset(r);
            (0..50)
                .map(|_| (rng.range(minx, maxx), rng.range(miny, maxy)))
                .find(|&c| inner.covers_point(c))
                .map(|c| (c, r))
        })
        .collect();
    let (sin, cos) = angle.to_radians().sin_cos();
    let at = anchor.resolve((0.0, 0.0));
    let f = |p: Pt| {
        let (along, across) = {
            let q = (p.0 - at.0, p.1 - at.1);
            (q.0 * cos + q.1 * sin, -q.0 * sin + q.1 * cos)
        };
        let mut g = across + spacing * 2.0 * noise.fbm(along / (spacing * 25.0), across / (spacing * 6.0), 3);
        for &(k, r) in &knots {
            let d2 = ((p.0 - k.0).powi(2) + (p.1 - k.1).powi(2)) / (r * r);
            // Just past the height that closes rings around the core;
            // higher, the grain on the knot's flanks crowds into a blot.
            g += 1.3 * r * (-d2).exp();
        }
        g
    };
    let reach = (maxx - minx).hypot(maxy - miny) + spacing * 20.0;
    fill_isolines(region, (spacing / 8.0).clamp(0.08, 0.4), &levels_every(-reach, reach, spacing), f)
}

/// Parched ground / mud cracks: a wobbly net along Voronoi edges after domain
/// warp. `spacing` is roughly the cell size in mm (map symbol for dry,
/// cracked earth).
pub fn fill_cracks(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let sites = poisson_disk(&region.outset(spacing * 2.0), spacing * 0.92, seed);
    let grid = Bins::new(&sites, spacing * 0.85);
    let noise = Perlin::new(seed.wrapping_add(4));
    let ripple = Perlin::new(seed.wrapping_add(11));
    let wobble = spacing * 0.55;
    let f = |p: Pt| {
        let s = spacing * 1.35;
        let u = p.0 / s;
        let v = p.1 / s;
        let w1 = noise.fbm(u, v, 3);
        let w2 = noise.fbm(u * 2.4 + 1.7 * w1, v * 2.4 - 1.1 * w1, 2);
        let q = (
            p.0 + wobble * (w1 + 0.45 * w2),
            p.1 + wobble * (ripple.noise(u + 4.2, v - 2.8) + 0.4 * w2),
        );
        let (d1, d2) = grid.two_nearest(q);
        let edge = d2 - d1;
        // Thicken and pinch crack lines slightly.
        edge + spacing * 0.05 * noise.noise(u * 1.6 - 3.0, v * 1.6)
    };
    let band = (spacing * 0.07).clamp(0.15, 0.6);
    fill_isolines(
        region,
        (spacing / 18.0).clamp(0.04, 0.22),
        &[0.0, band],
        f,
    )
}

/// Points binned on a grid for nearest-neighbour lookups.
struct Bins<'a> {
    pts: &'a [Pt],
    cell: f64,
    origin: Pt,
    cols: usize,
    rows: usize,
    bins: Vec<Vec<usize>>,
}

impl<'a> Bins<'a> {
    fn new(pts: &'a [Pt], cell: f64) -> Bins<'a> {
        let minx = pts.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let miny = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let maxx = pts.iter().map(|p| p.0).fold(f64::MIN, f64::max);
        let maxy = pts.iter().map(|p| p.1).fold(f64::MIN, f64::max);
        let cols = ((maxx - minx) / cell) as usize + 1;
        let rows = ((maxy - miny) / cell) as usize + 1;
        let mut bins = vec![Vec::new(); cols * rows];
        for (i, p) in pts.iter().enumerate() {
            let c = ((p.0 - minx) / cell) as usize;
            let r = ((p.1 - miny) / cell) as usize;
            bins[r * cols + c].push(i);
        }
        Bins { pts, cell, origin: (minx, miny), cols, rows, bins }
    }

    /// Distances to the nearest and second-nearest point.
    fn two_nearest(&self, p: Pt) -> (f64, f64) {
        let c = ((p.0 - self.origin.0) / self.cell).floor() as i64;
        let r = ((p.1 - self.origin.1) / self.cell).floor() as i64;
        let (mut d1, mut d2) = (f64::MAX, f64::MAX);
        for rr in r - 2..=r + 2 {
            for cc in c - 2..=c + 2 {
                if rr < 0 || cc < 0 || rr as usize >= self.rows || cc as usize >= self.cols {
                    continue;
                }
                for &i in &self.bins[rr as usize * self.cols + cc as usize] {
                    let q = self.pts[i];
                    let d = (q.0 - p.0).hypot(q.1 - p.1);
                    if d < d1 {
                        d2 = d1;
                        d1 = d;
                    } else if d < d2 {
                        d2 = d;
                    }
                }
            }
        }
        (d1, d2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)])
    }

    fn inside(s: &Strokes) {
        for p in s.iter().flatten() {
            assert!(square().covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn marble_has_veins_inside() {
        let m = fill_marble(&square(), 4.0, 30.0, 1, Anchor::LOCAL);
        assert!(m.len() >= 6, "only {} vein edges", m.len());
        inside(&m);
    }

    #[test]
    fn wood_grain_runs_along_the_angle() {
        let w = fill_wood(&square(), 2.0, 0.0, 1, Anchor::LOCAL);
        assert!(w.len() >= 15, "only {} grain lines", w.len());
        inside(&w);
        // Mostly horizontal: total x travel beats y travel.
        let (mut dx, mut dy) = (0.0, 0.0);
        for s in &w {
            for p in s.windows(2) {
                dx += (p[1].0 - p[0].0).abs();
                dy += (p[1].1 - p[0].1).abs();
            }
        }
        assert!(dx > 2.0 * dy, "grain not along 0°: dx {dx}, dy {dy}");
    }

    #[test]
    fn cracks_draw_a_net() {
        let w = fill_cracks(&square(), 5.0, 1);
        assert!(w.len() >= 20, "only {} crack segments", w.len());
        inside(&w);
    }
}
