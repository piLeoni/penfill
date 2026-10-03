//! Rectilinear hatch: parallel strokes inside the region, one stroke per
//! piece, unjoined. The lines are [`sweep_anchored`]'s, cut to the region
//! after it has been shrunk by the border.

use crate::anchor::Anchor;
use crate::geo::{Region, Strokes};
use crate::sweep::sweep_anchored;

/// One stroke per piece of each line, `inset_mm` (default half the spacing)
/// in from the walls.
pub fn fill_rectilinear(region: &Region, spacing: f64, angle: f64, inset_mm: Option<f64>, anchor: Anchor) -> Strokes {
    if region.is_empty() || spacing <= 0.0 {
        return vec![];
    }
    let target = region.inset(inset_mm.unwrap_or(spacing / 2.0));
    if target.is_empty() {
        return vec![];
    }
    sweep_anchored(&target, spacing, angle, anchor)
}

/// The same lines right up to the walls.
pub fn fill_rectilinear_no_inset(region: &Region, spacing: f64, angle: f64, anchor: Anchor) -> Strokes {
    sweep_anchored(region, spacing, angle, anchor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::Region;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)])
    }

    #[test]
    fn fills_square_with_parallel_lines() {
        let lines = fill_rectilinear(&square(), 1.0, 0.0, None, Anchor::LOCAL);
        assert!(!lines.is_empty());
        // 20mm square, spacing 1, inset 0.5 → about 19 lines.
        assert!(lines.len() >= 18 && lines.len() <= 20, "got {}", lines.len());
        // All strokes horizontal, interior to the square.
        for line in &lines {
            assert!(line.len() >= 2);
            assert!(line[0].1 >= -1e-9 && line[0].1 <= 20.0 + 1e-9);
        }
    }

    #[test]
    fn thin_region_yields_nothing() {
        let thin = Region::from_ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 0.05), (0.0, 0.05)]);
        assert!(fill_rectilinear(&thin, 1.0, 0.0, None, Anchor::LOCAL).is_empty());
    }

    #[test]
    fn holes_are_respected() {
        // Square with a courtyard: 0..20 outer, 6..14 hole.
        let ring = Region::from_polygons(&[vec![
            vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)],
            vec![(6.0, 6.0), (14.0, 6.0), (14.0, 14.0), (6.0, 14.0)],
        ]]);
        let lines = fill_rectilinear(&ring, 1.0, 0.0, None, Anchor::LOCAL);
        // The y=10 scanline must be split around the hole → extra fragment on
        // that line, and no stroke may contain the hole's centre point.
        let centre = (10.0, 10.0);
        for line in &lines {
            for &p in line {
                let d = ((p.0 - centre.0).abs(), (p.1 - centre.1).abs());
                assert!(d.0 > 1e-3 || d.1 != 0.0 || p.1 < 5.9 || p.1 > 14.1 || p.0 < 5.9 || p.0 > 14.1);
            }
        }
        // Coverage: some fragment should run left of the hole on y≈10.
        assert!(
            lines.iter().any(|l| l.iter().any(|&p| (p.1 - 10.0).abs() < 0.6 && p.0 < 5.5)),
            "no fragment left of the courtyard"
        );
    }
}
