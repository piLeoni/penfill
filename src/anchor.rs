//! Where a pattern's lattice sits.
//!
//! A repeating pattern is a lattice laid over the region: rows, columns,
//! cells, tiles. [`Anchor`] fixes one point the lattice passes through.
//!
//! - [`Origin::Local`]: the pattern's own placement on the shape (most
//!   centre themselves on the region's bounds), moved by `shift`.
//! - [`Origin::Global`]: a lattice point at page `shift`. Regions filled
//!   with the same pattern, spacing and angle then share one lattice, so
//!   two areas side by side read as one: their shared border leaves no seam.
//!
//! Random scatters (stipple, gravel, terrazzo, cracks, scribble) and fills
//! that follow the shape (concentric, spiral, hilbert) have no lattice and
//! take no anchor.

use crate::geo::{Pt, Region};

/// What `shift` is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// The pattern's own reference point on the region.
    #[default]
    Local,
    /// The page origin.
    Global,
}

/// A lattice point: `shift` mm from the [`Origin`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Anchor {
    pub origin: Origin,
    pub shift: Pt,
}

impl Anchor {
    /// Each pattern placed on its own shape, as it always was.
    pub const LOCAL: Anchor = Anchor { origin: Origin::Local, shift: (0.0, 0.0) };
    /// One lattice for the page, through the page origin.
    pub const GLOBAL: Anchor = Anchor { origin: Origin::Global, shift: (0.0, 0.0) };

    pub fn local(x: f64, y: f64) -> Anchor {
        Anchor { origin: Origin::Local, shift: (x, y) }
    }

    pub fn global(x: f64, y: f64) -> Anchor {
        Anchor { origin: Origin::Global, shift: (x, y) }
    }

    pub fn is_global(&self) -> bool {
        self.origin == Origin::Global
    }

    /// The lattice point, given the pattern's own reference point.
    pub fn resolve(&self, local: Pt) -> Pt {
        match self.origin {
            Origin::Local => (local.0 + self.shift.0, local.1 + self.shift.1),
            Origin::Global => self.shift,
        }
    }
}

/// A lattice frame on the page: an origin and a turn. Pattern coordinates
/// `(u, v)` map to the page as `origin + R(angle) · (u, v)`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub origin: Pt,
    sin: f64,
    cos: f64,
}

impl Frame {
    pub fn new(origin: Pt, angle: f64) -> Frame {
        let (sin, cos) = angle.to_radians().sin_cos();
        Frame { origin, sin, cos }
    }

    /// The frame through the anchor, for a pattern whose own reference
    /// point is `local`.
    pub fn anchored(anchor: Anchor, local: Pt, angle: f64) -> Frame {
        Frame::new(anchor.resolve(local), angle)
    }

    pub fn world(self, p: Pt) -> Pt {
        (self.origin.0 + p.0 * self.cos - p.1 * self.sin, self.origin.1 + p.0 * self.sin + p.1 * self.cos)
    }

    pub fn to_frame(self, p: Pt) -> Pt {
        let (dx, dy) = (p.0 - self.origin.0, p.1 - self.origin.1);
        (dx * self.cos + dy * self.sin, -dx * self.sin + dy * self.cos)
    }

    /// The region's bounds in frame coordinates, grown by `margin`:
    /// `(u_min, u_max, v_min, v_max)`.
    pub fn span(&self, region: &Region, margin: f64) -> (f64, f64, f64, f64) {
        let (x0, y0, x1, y1) = region.bounds_mm();
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|p| self.to_frame(p));
        let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for (u, v) in corners {
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(v);
            v1 = v1.max(v);
        }
        (u0 - margin, u1 + margin, v0 - margin, v1 + margin)
    }
}

/// Lattice indices `k` with `k · pitch` covering `lo..=hi`, one spare each side.
pub(crate) fn indices(lo: f64, hi: f64, pitch: f64) -> std::ops::RangeInclusive<i64> {
    ((lo / pitch).floor() as i64 - 1)..=((hi / pitch).ceil() as i64 + 1)
}

/// The middle of the region's bounds: most patterns' own reference point.
pub(crate) fn centre(region: &Region) -> Pt {
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    ((minx + maxx) / 2.0, (miny + maxy) / 2.0)
}

/// A random choice per lattice cell: the same cell gets the same answer
/// whichever region asks, so a global lattice stays continuous.
pub(crate) fn cell_seed(seed: u64, i: i64, j: i64) -> u64 {
    let mut z = seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{polyline_length, Strokes};
    use crate::*;

    fn rect(x0: f64, x1: f64) -> Region {
        Region::from_ring(&[(x0, 0.0), (x1, 0.0), (x1, 12.0), (x0, 12.0)])
    }

    fn ink(s: &Strokes) -> f64 {
        s.iter().map(|p| polyline_length(p)).sum()
    }

    type Fill = Box<dyn Fn(&Region, Anchor) -> Strokes>;

    /// Every lattice pattern, as `(name, fill)`.
    fn lattices() -> Vec<(&'static str, Fill)> {
        let brick = pat::builtin("BRICK").unwrap();
        let cross: Strokes = vec![vec![(0.2, 0.5), (0.8, 0.5)], vec![(0.5, 0.2), (0.5, 0.8)]];
        vec![
            ("rectilinear", Box::new(|r, a| rectilinear::fill_rectilinear(r, 0.7, 30.0, Some(0.0), a))),
            ("lines", Box::new(|r, a| grid::fill_lines(r, 0.7, 30.0, a))),
            ("grid", Box::new(|r, a| grid::fill_grid(r, 0.9, 10.0, a))),
            ("triangles", Box::new(|r, a| grid::fill_triangles(r, 1.1, 5.0, a))),
            ("dots", Box::new(|r, a| dotswaves::fill_dots(r, 0.45, 45.0, 0.2, a))),
            ("dots hex", Box::new(|r, a| dotswaves::fill_dots_with_lattice(r, 0.5, 10.0, 0.2, dotswaves::Lattice::Hexagonal, a))),
            ("waves", Box::new(|r, a| dotswaves::fill_waves(r, 0.8, 20.0, a))),
            ("wave_grid", Box::new(|r, a| dotswaves::fill_wave_grid(r, 1.0, 15.0, a))),
            ("scales", Box::new(|r, a| scales::fill_scales(r, 0.9, 25.0, a))),
            ("honeycomb", Box::new(|r, a| honeycomb::fill_honeycomb(r, 1.5, 35.0, 0.0, a))),
            ("chess", Box::new(|r, a| chess::fill_chess(r, 2.0, 0.4, 20.0, a))),
            ("truchet", Box::new(|r, a| truchet::fill_truchet(r, 1.5, 15.0, 3, a))),
            ("tile", Box::new(move |r, a| tile::fill_tile(r, &cross, 1.0, 1.0, 30.0, 0.5, a))),
            ("pat", Box::new(move |r, a| pat::fill_pat(r, &brick, 0.5, 10.0, a))),
            ("sunburst", Box::new(|r, a| radial::fill_sunburst(r, 0.6, 0.0, a))),
        ]
    }

    #[test]
    fn a_global_lattice_has_no_seam_where_two_regions_meet() {
        let at = Anchor::global(0.3, -0.2);
        for (name, fill) in lattices() {
            let whole = ink(&fill(&rect(0.0, 20.0), at));
            let halves = ink(&fill(&rect(0.0, 8.3), at)) + ink(&fill(&rect(8.3, 20.0), at));
            assert!(whole > 0.0, "{name}: no ink");
            // A seam drops a row of marks: percents of the ink. What is left
            // is the 0.01 mm simplification of curved lines.
            assert!((whole - halves).abs() < 1e-4 * whole, "{name}: whole {whole} vs halves {halves}");
        }
    }

    #[test]
    fn a_local_shift_moves_the_lattice_with_the_shape() {
        // A shape and its copy moved by (d, 0) get the same pattern moved by d.
        let d = 3.7;
        for (name, fill) in lattices() {
            let a = fill(&rect(0.0, 10.0), Anchor::local(0.2, 0.1));
            let b = fill(&rect(d, 10.0 + d), Anchor::local(0.2, 0.1));
            if name == "pat" {
                continue; // a .pat hatch is drawn from the page origin
            }
            assert!((ink(&a) - ink(&b)).abs() < 1e-4 * ink(&a), "{name}: {} vs {}", ink(&a), ink(&b));
        }
    }
}
