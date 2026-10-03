//! Your own pattern, on the same terms as the built-in ones.
//!
//! The engine calls the pattern once with two things:
//!
//! - a [`Canvas`]: where it is drawing. The region (ask it anything:
//!   [`Region::dist_to_boundary`], [`Region::covers_point`],
//!   [`Region::parts`], [`Region::inset`], ...), its bounds and the seed.
//! - [`Tools`]: an optional toolkit, the pieces the built-in fills are made
//!   of, already set to the canvas region and seed. A pattern can ignore it
//!   and use anything else.
//!
//! The pattern returns lines anywhere on the page, and the engine keeps the
//! parts inside the shape with one clip over the whole region. Splitting
//! the region into pieces costs time, so it only happens when the pattern
//! asks for [`Region::parts`].

use crate::anchor::Anchor;
use crate::geo::{Pt, Region, Strokes};
use crate::noise::Perlin;

/// Where a pattern is drawing. All coordinates are page millimetres, the
/// same ones the region was built in.
pub struct Canvas<'a> {
    /// Everything to fill, holes included.
    pub region: &'a Region,
    /// Bounding box of the whole region: `(min_x, min_y, max_x, max_y)`.
    pub bounds: (f64, f64, f64, f64),
    /// For patterns with randomness: same seed, same drawing.
    pub seed: u64,
    /// Where the pattern's lattice should sit ([`Anchor`]).
    pub anchor: Anchor,
    /// The anchor resolved for this region: the point to build a lattice
    /// through. Locally the middle of `bounds`, globally a page point.
    pub origin: Pt,
}

/// The toolkit: what the built-in fills are made of, set to one region and
/// one seed.
#[derive(Clone)]
pub struct Tools<'a> {
    region: &'a Region,
    seed: u64,
    perlin: Perlin,
    anchor: Anchor,
}

impl<'a> Tools<'a> {
    pub fn new(region: &'a Region, seed: u64) -> Tools<'a> {
        Tools { region, seed, perlin: Perlin::new(seed), anchor: Anchor::LOCAL }
    }

    /// The same tools with lattices placed by `anchor`.
    pub fn anchored(self, anchor: Anchor) -> Tools<'a> {
        Tools { anchor, ..self }
    }

    /// The same tools for another region (a piece from `parts`, an inset).
    pub fn on<'b>(&self, region: &'b Region) -> Tools<'b> {
        Tools { region, seed: self.seed, perlin: self.perlin.clone(), anchor: self.anchor }
    }

    /// Smooth Perlin noise, roughly -1 to 1, one bump per unit; each extra
    /// octave adds detail twice as fine and half as strong.
    pub fn noise(&self, x: f64, y: f64, octaves: usize) -> f64 {
        self.perlin.fbm(x, y, octaves.max(1))
    }

    /// Random points in the region, no two closer than `spacing` mm.
    pub fn scatter(&self, spacing: f64) -> Vec<Pt> {
        crate::random::poisson_disk(self.region, spacing, self.seed)
    }

    /// Random points `dense` mm apart where `field` is 0 and `sparse` mm
    /// apart where it is 1.
    pub fn scatter_field(&self, dense: f64, sparse: f64, field: impl Fn(Pt) -> f64) -> Vec<Pt> {
        crate::stipple::scatter_field(self.region, dense, sparse, self.seed, field)
    }

    /// Parallel lines `spacing` mm apart, cut to the region right up to its
    /// walls, placed by the anchor.
    pub fn hatch(&self, spacing: f64, angle: f64) -> Strokes {
        crate::sweep::sweep_anchored(self.region, spacing, angle, self.anchor)
    }
}

/// Calls `draw` once with the canvas and the tools, and clips what it
/// returns to the region.
pub fn fill_pattern(
    region: &Region,
    seed: u64,
    anchor: Anchor,
    draw: impl FnOnce(&Canvas, &Tools) -> Strokes,
) -> Strokes {
    if region.is_empty() {
        return vec![];
    }
    let origin = anchor.resolve(crate::anchor::centre(region));
    let canvas = Canvas { region, bounds: region.bounds_mm(), seed, anchor, origin };
    region.clip_strokes(&draw(&canvas, &Tools::new(region, seed).anchored(anchor)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::polyline_length;
    use crate::sweep::sweep_lines;

    /// A doughnut with an island in its hole, and a square beside it.
    fn doughnut_and_square() -> Region {
        let square = |x: f64, y: f64, s: f64| vec![(x, y), (x + s, y), (x + s, y + s), (x, y + s)];
        let frame = Region::from_polygons(&[vec![square(10.0, 10.0, 20.0), square(15.0, 15.0, 10.0)]]);
        Region::from_polygons(&[vec![square(0.0, 0.0, 40.0)], vec![square(60.0, 0.0, 20.0)]]).difference(&frame)
    }

    fn length(s: &Strokes) -> f64 {
        s.iter().map(|p| polyline_length(p)).sum()
    }

    #[test]
    fn canvas_describes_the_layer() {
        let region = doughnut_and_square();
        fill_pattern(&region, 7, Anchor::LOCAL, |c, _| {
            assert_eq!(c.bounds, (0.0, 0.0, 80.0, 40.0));
            assert_eq!(c.seed, 7);
            vec![]
        });
    }

    #[test]
    fn parts_keep_their_holes_and_islands() {
        let mut parts: Vec<(usize, f64)> = doughnut_and_square()
            .parts()
            .iter()
            .map(|s| (s.all_rings_mm().len(), s.bounds_mm().2 - s.bounds_mm().0))
            .collect();
        parts.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert_eq!(parts, vec![(1, 10.0), (1, 20.0), (2, 40.0)]);
    }

    #[test]
    fn output_is_clipped_to_the_region() {
        let region = doughnut_and_square();
        let grid: Strokes = (0..=40).map(|i| vec![(-10.0, i as f64), (90.0, i as f64)]).collect();
        let out = fill_pattern(&region, 0, Anchor::LOCAL, |_, _| grid.clone());
        assert!(!out.is_empty());
        for s in &out {
            let mid = ((s[0].0 + s[1].0) / 2.0, (s[0].1 + s[1].1) / 2.0);
            assert!(region.covers_point(mid), "{mid:?}");
        }
    }

    #[test]
    fn built_in_fills_compose_whole_or_per_piece() {
        let region = doughnut_and_square();
        let direct = sweep_lines(&region, 1.5, 30.0);
        let whole = fill_pattern(&region, 0, Anchor::LOCAL, |_, t| t.hatch(1.5, 30.0));
        let per_piece = fill_pattern(&region, 0, Anchor::LOCAL, |c, t| c.region.parts().iter().flat_map(|p| t.on(p).hatch(1.5, 30.0)).collect());
        assert!((length(&whole) - length(&direct)).abs() < 1e-6 * length(&direct));
        assert!((length(&per_piece) - length(&direct)).abs() < 0.01 * length(&direct));
    }

    #[test]
    fn stipple_edge_rebuilt_from_the_tools() {
        let region = doughnut_and_square();
        let mine = fill_pattern(&region, 5, Anchor::LOCAL, |c, t| {
            crate::stipple::fill_stipple_field(c.region, 0.8, 4.0, 0.3, c.seed, |p| {
                (c.region.dist_to_boundary(p) / 8.0).clamp(0.0, 1.0) + 0.5 * t.noise(p.0 / 8.0, p.1 / 8.0, 2) * 0.5
            })
        });
        assert_eq!(mine, crate::stipple::fill_stipple_edge(&region, 0.8, 4.0, 0.3, 8.0, 0.5, 5));
    }
}
