//! Flat colour fills: no pen strokes. Plotters and line-only exporters skip
//! them; hosts that paint regions (SVG `fill`, a screen preview) use the
//! [`Region`] boundary themselves.

use crate::{Region, Strokes};

/// No ink paths — the region is filled by the host.
pub fn fill_solid(_region: &Region) -> Strokes {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Region;

    #[test]
    fn no_strokes() {
        let r = Region::from_polygons(&[vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 0.0)]]]);
        assert!(fill_solid(&r).is_empty());
    }
}
