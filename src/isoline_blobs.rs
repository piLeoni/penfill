//! Nested, fusing contour texture — one continuous scalar field sliced into
//! many isolines, so neighbouring masses merge at saddle points instead of
//! sitting as separate stamped shapes (a topographic map's lowest contour
//! already does this for connected high ground — this is the same idea
//! turned into foliage/cloud ink).
//!
//! Visual target: `drawland/docs/architecture/img/isoline-blob-fills/`.

use crate::field::{fill_isolines, levels_every};
use crate::fray::fray_strokes;
use crate::geo::{Pt, Region, Strokes};
use crate::noise::Perlin;
use crate::pattern::{EdgeMode, FillOutput, FillParams};

/// Linework plus mask geometry for occlusion: the lowest contour's closed
/// loops (the overall silhouette), not every nested ring.
#[derive(Clone, Debug, Default)]
pub struct FillWithCoverage {
    pub strokes: Strokes,
    pub coverage: Region,
}

impl From<FillWithCoverage> for FillOutput {
    fn from(v: FillWithCoverage) -> Self {
        FillOutput {
            strokes: v.strokes,
            mask: v.coverage,
        }
    }
}

impl From<FillOutput> for FillWithCoverage {
    fn from(v: FillOutput) -> Self {
        FillWithCoverage {
            strokes: v.strokes,
            coverage: v.mask,
        }
    }
}

/// A known position that raises the field locally — a real tree, a cloud
/// seed, anything you already have coordinates for. `radius` is the mm
/// scale of its influence; `weight` is how high it pushes the field
/// (comparable to the ambient noise's own `[-1, 1]` range).
#[derive(Clone, Copy, Debug)]
pub struct FieldSite {
    pub at: Pt,
    pub weight: f64,
    pub radius: f64,
}

/// What shapes the field besides the boundary.
#[derive(Clone, Debug)]
pub enum FieldSource {
    /// Pure ambient noise — clusters appear wherever the noise happens to
    /// peak, roughly `feature_size` mm apart. No input positions needed.
    Noise,
    /// Only the given sites raise the field; flat (no ink) everywhere else.
    /// Use when you have real positions and want exact placement.
    Sites(Vec<FieldSite>),
    /// Sites plus ambient noise texture, so placed bumps still read as the
    /// same organic material as a procedural patch.
    NoiseAndSites(Vec<FieldSite>),
}

#[derive(Clone, Debug)]
pub struct IsolineFieldParams {
    pub seed: u64,
    pub source: FieldSource,
    /// Wavelength of the ambient noise in mm — roughly the spacing between
    /// procedurally-formed clusters. Ignored for `Sites`.
    pub feature_size: f64,
    /// Target distance between nested contour lines, in mm.
    pub contour_spacing: f64,
    /// 0 (smooth, calm) upward — domain-warp strength; higher reads jumpier
    /// and more hand-drawn.
    pub warp: f64,
    /// Field height where ink begins (paper below, nested rings above).
    /// Raising it shrinks coverage down to just the tallest peaks; lowering
    /// it grows and fuses clusters into fewer, bigger connected masses.
    pub threshold: f64,
    /// Boundary mode (`Clip`, `Inside`, or `Overflow(mm)`).
    pub edge: EdgeMode,
    /// Smooth perimeter disappearing band in mm (`0.0` = off).
    pub fray: f64,
}

impl Default for IsolineFieldParams {
    fn default() -> Self {
        IsolineFieldParams {
            seed: 0,
            source: FieldSource::Noise,
            feature_size: 12.0,
            contour_spacing: 0.5,
            warp: 0.45,
            threshold: 0.25,
            edge: EdgeMode::Clip,
            fray: 0.0,
        }
    }
}

/// Reasonable starting point: procedural, scattered, moderate density.
pub fn default_params(feature_size: f64, contour_spacing: f64, seed: u64) -> IsolineFieldParams {
    IsolineFieldParams {
        seed,
        feature_size: feature_size.max(0.1),
        contour_spacing: contour_spacing.max(0.02),
        ..IsolineFieldParams::default()
    }
}

/// Strokes only, default params.
pub fn fill_isoline_blobs(region: &Region, feature_size: f64, contour_spacing: f64, seed: u64) -> Strokes {
    fill_isoline_blobs_with_coverage(region, &default_params(feature_size, contour_spacing, seed)).strokes
}

/// Standardized entry point from [`FillParams`] (where `params.spacing` is the
/// contour spacing and `feature_size` scales proportionally).
pub fn fill_isoline_blobs_params(region: &Region, params: &FillParams) -> FillOutput {
    let target = params.resolve_region(region, 0.0);
    let contour_spacing = params.spacing.max(0.05);
    let feature_size = (contour_spacing * 16.0).max(4.0);
    let mut p = default_params(feature_size, contour_spacing, params.seed);
    p.edge = params.edge;
    p.fray = params.fray;
    fill_isoline_blobs_with_coverage(&target, &p).into()
}

pub fn fill_isoline_blobs_with_coverage(region: &Region, p: &IsolineFieldParams) -> FillWithCoverage {
    if region.is_empty() || p.contour_spacing <= 0.0 {
        return FillWithCoverage::default();
    }
    let feature = p.feature_size.max(1e-6);
    let limit = match p.edge {
        EdgeMode::Overflow(m) if m > 0.0 => region.outset(m),
        _ => region.clone(),
    };
    let fade_band = (feature * 0.35).max(p.contour_spacing * 3.0);

    let ambient = Perlin::new(p.seed);
    let warp_x = Perlin::new(p.seed.wrapping_add(101));
    let warp_y = Perlin::new(p.seed.wrapping_add(202));
    let warp_amt = p.warp.max(0.0) * feature * 0.35;

    let use_noise = !matches!(p.source, FieldSource::Sites(_));
    let sites: &[FieldSite] = match &p.source {
        FieldSource::Noise => &[],
        FieldSource::Sites(s) | FieldSource::NoiseAndSites(s) => s,
    };

    let field = |pt: Pt| -> f64 {
        let (wx, wy) = if warp_amt > 0.0 {
            let u = pt.0 / feature;
            let v = pt.1 / feature;
            let dx = warp_x.fbm(u, v, 2);
            let dy = warp_y.fbm(u + 5.2, v - 3.1, 2);
            (pt.0 + warp_amt * dx, pt.1 + warp_amt * dy)
        } else {
            (pt.0, pt.1)
        };
        let mut h = if use_noise {
            ambient.fbm(wx / feature, wy / feature, 4)
        } else {
            0.0
        };
        for s in sites {
            let r = s.radius.max(1e-6);
            let dx = (pt.0 - s.at.0) / r;
            let dy = (pt.1 - s.at.1) / r;
            h += s.weight * (-(dx * dx + dy * dy)).exp();
        }
        if matches!(p.edge, EdgeMode::Inside | EdgeMode::Overflow(_)) {
            if !limit.covers_point(pt) {
                return p.threshold - 1.0;
            }
            let d = limit.dist_to_boundary(pt);
            if d < fade_band {
                let t = (d / fade_band).clamp(0.0, 1.0);
                let s = t * t * (3.0 - 2.0 * t);
                h = (p.threshold - 0.5) + (h - (p.threshold - 0.5)) * s;
            }
        }
        h
    };

    let max_level = 1.05 + sites.iter().map(|s| s.weight.max(0.0)).fold(0.0, f64::max);
    if p.threshold >= max_level {
        return FillWithCoverage::default();
    }

    let level_step = (p.contour_spacing / feature).clamp(0.01, 0.5);
    let levels = levels_every(p.threshold, max_level, level_step);
    let cell = (p.contour_spacing * 0.3).min(feature * 0.12).clamp(0.04, 0.4);

    let raw_strokes = fill_isolines(&limit, cell, &levels, field);
    let strokes = if p.fray > 0.0 {
        fray_strokes(&limit, raw_strokes, p.contour_spacing, p.fray, p.seed)
    } else {
        raw_strokes
    };
    let coverage = coverage_from_threshold(&limit, cell, p.threshold, &field);

    FillWithCoverage { strokes, coverage }
}

/// The silhouette: closed loops of the lowest ("ink begins") contour,
/// unioned as one region.
fn coverage_from_threshold(region: &Region, cell: f64, threshold: f64, field: &impl Fn(Pt) -> f64) -> Region {
    let loops = fill_isolines(region, cell, &[threshold], field);
    let polys: Vec<Vec<Vec<Pt>>> =
        loops.into_iter().filter(|r| r.len() >= 4 && r.first() == r.last()).map(|r| vec![r]).collect();
    if polys.is_empty() {
        return Region::default();
    }
    region.intersection(&Region::from_polygons(&polys))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (80.0, 0.0), (80.0, 80.0), (0.0, 80.0)])
    }

    fn inside(s: &Strokes, region: &Region) {
        for p in s.iter().flatten() {
            assert!(region.covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn noise_field_produces_nested_lines() {
        let p = default_params(12.0, 0.4, 7);
        let out = fill_isoline_blobs_with_coverage(&square(), &p);
        assert!(out.strokes.len() >= 20, "only {} strokes", out.strokes.len());
        inside(&out.strokes, &square());
    }

    #[test]
    fn lower_threshold_grows_and_fuses_coverage() {
        let mut low = default_params(12.0, 0.4, 7);
        let mut high = low.clone();
        low.threshold = -0.6;
        high.threshold = 0.6;
        let low_out = fill_isoline_blobs_with_coverage(&square(), &low);
        let high_out = fill_isoline_blobs_with_coverage(&square(), &high);
        let area = |r: &Region| r.all_rings_mm().len();
        assert!(area(&low_out.coverage) >= area(&high_out.coverage));
    }

    #[test]
    fn two_close_sites_fuse_into_one_loop() {
        let p = IsolineFieldParams {
            seed: 1,
            source: FieldSource::Sites(vec![
                FieldSite { at: (30.0, 40.0), weight: 1.0, radius: 10.0 },
                FieldSite { at: (42.0, 40.0), weight: 1.0, radius: 10.0 },
            ]),
            feature_size: 12.0,
            contour_spacing: 0.5,
            warp: 0.1,
            threshold: 0.2,
            ..Default::default()
        };
        let out = fill_isoline_blobs_with_coverage(&square(), &p);
        assert!(!out.coverage.is_empty());
        assert_eq!(out.coverage.all_rings_mm().len(), 1, "sites should fuse at this spacing");
    }

    #[test]
    fn far_sites_stay_separate() {
        let p = IsolineFieldParams {
            seed: 1,
            source: FieldSource::Sites(vec![
                FieldSite { at: (10.0, 10.0), weight: 1.0, radius: 4.0 },
                FieldSite { at: (70.0, 70.0), weight: 1.0, radius: 4.0 },
            ]),
            feature_size: 12.0,
            contour_spacing: 0.5,
            warp: 0.1,
            threshold: 0.2,
            ..Default::default()
        };
        let out = fill_isoline_blobs_with_coverage(&square(), &p);
        assert_eq!(out.coverage.all_rings_mm().len(), 2);
    }
}
