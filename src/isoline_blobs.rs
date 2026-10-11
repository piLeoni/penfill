//! Nested contour “blob” texture — scattered organic isoline stacks per site.
//!
//! Visual target: `drawland/docs/architecture/img/isoline-blob-fills/`.

use crate::field::{fill_isolines, levels_every};
use crate::geo::{Pt, Region, Strokes};
use crate::noise::Perlin;
use crate::random::{poisson_disk, Rng};

/// Linework plus mask geometry for occlusion (blob silhouettes, not every ring).
#[derive(Clone, Debug)]
pub struct FillWithCoverage {
    pub strokes: Strokes,
    pub coverage: Region,
}

/// Where blob centres come from.
#[derive(Clone, Debug)]
pub enum SiteSource {
    /// Poisson scatter; `min_spacing` is centre-to-centre distance in mm.
    Generate { min_spacing: f64, dense: bool },
    /// Caller-supplied sites in mm; points outside `region` are skipped.
    Points(Vec<Pt>),
}

#[derive(Clone, Debug)]
pub struct IsolineBlobParams {
    pub seed: u64,
    pub sites: SiteSource,
    /// Typical distance between nested contour lines in mm.
    pub contour_spacing: f64,
    /// How many height levels to slice per blob (capped in practice by support).
    pub level_count: usize,
    /// Nominal blob radius in mm.
    pub blob_radius: f64,
    /// 0–1-ish strength of edge wobble from noise on the height field.
    pub warp: f64,
}

/// Convenience: strokes only, default scattered layout.
pub fn fill_isoline_blobs(region: &Region, site_spacing: f64, contour_spacing: f64, seed: u64) -> Strokes {
    fill_isoline_blobs_with_coverage(region, &default_params(site_spacing, contour_spacing, seed)).strokes
}

pub fn default_params(site_spacing: f64, contour_spacing: f64, seed: u64) -> IsolineBlobParams {
    IsolineBlobParams {
        seed,
        sites: SiteSource::Generate {
            min_spacing: site_spacing.max(0.1),
            dense: false,
        },
        contour_spacing: contour_spacing.max(0.05),
        level_count: 8,
        blob_radius: site_spacing * 0.85,
        warp: 0.35,
    }
}

pub fn fill_isoline_blobs_with_coverage(region: &Region, p: &IsolineBlobParams) -> FillWithCoverage {
    if region.is_empty() || p.contour_spacing <= 0.0 || p.blob_radius <= 0.0 || p.level_count == 0 {
        return FillWithCoverage { strokes: vec![], coverage: Region::default() };
    }

    let sites = collect_sites(region, &p.sites, p.seed);
    if sites.is_empty() {
        return FillWithCoverage { strokes: vec![], coverage: Region::default() };
    }

    let mut rng = Rng::new(p.seed.wrapping_add(41));
    let mut strokes: Strokes = Vec::new();
    let mut footprint_polys: Vec<Vec<Vec<Pt>>> = Vec::new();

    let cell = (p.contour_spacing * 0.22).clamp(0.06, 0.32);
    let level_step = (p.contour_spacing / p.blob_radius * 0.55).clamp(0.04, 0.22);

    for (i, c) in sites.iter().enumerate() {
        if !region.covers_point(*c) {
            continue;
        }
        let r = p.blob_radius * rng.range(0.82, 1.08);
        let phase = rng.range(0.0, std::f64::consts::TAU);
        let rough = rng.range(0.08, 0.16) * p.warp.max(0.0);
        footprint_polys.push(vec![wobbly_envelope(*c, r, phase, rough)]);

        let salt = p.seed.wrapping_add(i as u64 * 131);
        let local_noise = Perlin::new(salt);
        let warp = p.warp.max(0.0);
        let bbox = square_region(*c, r * 1.15);
        let local = bbox.intersection(region);
        if local.is_empty() {
            continue;
        }

        let peak = 0.92 + rng.range(-0.06, 0.04);
        let lo = peak - level_step * p.level_count as f64;
        let levels = levels_every(lo.max(level_step), peak, level_step);
        if levels.is_empty() {
            continue;
        }

        let centre = *c;
        let f = move |p: Pt| blob_height(p, centre, r, warp, &local_noise);
        strokes.extend(fill_isolines(&local, cell, &levels, f));
    }

    let coverage = if footprint_polys.is_empty() {
        Region::default()
    } else {
        region.intersection(&Region::from_polygons(&footprint_polys))
    };

    FillWithCoverage { strokes, coverage }
}

fn collect_sites(region: &Region, source: &SiteSource, seed: u64) -> Vec<Pt> {
    match source {
        SiteSource::Points(pts) => pts.iter().copied().filter(|c| region.covers_point(*c)).collect(),
        SiteSource::Generate { min_spacing, dense } => {
            let spacing = if *dense { min_spacing * 0.52 } else { min_spacing * 1.05 };
            let field = region.outset(spacing * 0.5);
            poisson_disk(&field, spacing, seed)
                .into_iter()
                .filter(|c| region.covers_point(*c))
                .collect()
        }
    }
}

fn square_region(c: Pt, half: f64) -> Region {
    Region::from_ring(&[
        (c.0 - half, c.1 - half),
        (c.0 + half, c.1 - half),
        (c.0 + half, c.1 + half),
        (c.0 - half, c.1 + half),
    ])
}

/// Smooth dome with domain-warped ripples — isolines read as nested foliage loops.
fn blob_height(p: Pt, c: Pt, radius: f64, warp: f64, noise: &Perlin) -> f64 {
    let dx = (p.0 - c.0) / radius;
    let dy = (p.1 - c.1) / radius;
    let d2 = dx * dx + dy * dy;
    if d2 > 1.12 {
        return -1.0;
    }
    let dome = (1.0 - d2).max(0.0).sqrt();
    let u = p.0 / (radius * 0.65);
    let v = p.1 / (radius * 0.65);
    let n = noise.fbm(u * 1.4, v * 1.4, 3);
    let ripple = (3.5 * u + 2.1 * n).sin() * 0.08 + (4.2 * v - 1.7 * n).sin() * 0.06;
    dome + warp * (0.14 * n + ripple)
}

fn wobbly_envelope(c: Pt, r: f64, phase: f64, rough: f64) -> Vec<Pt> {
    let steps = 32;
    let mut pts = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = std::f64::consts::TAU * k as f64 / steps as f64;
        let w = (7.0 * t + phase).sin() * 0.55
            + (11.0 * t - phase * 1.3).sin() * 0.35
            + (17.0 * t + phase * 0.7).sin() * 0.22;
        let rr = r * (1.0 + rough * w);
        pts.push((c.0 + rr * t.cos(), c.1 + rr * t.sin()));
    }
    pts
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
    fn scattered_blobs_produce_nested_lines() {
        let p = default_params(6.0, 0.35, 7);
        let out = fill_isoline_blobs_with_coverage(&square(), &p);
        assert!(out.strokes.len() >= 40, "only {} strokes", out.strokes.len());
        inside(&out.strokes, &square());
    }

    #[test]
    fn coverage_inside_host() {
        let p = default_params(7.0, 0.4, 3);
        let host = square();
        let out = fill_isoline_blobs_with_coverage(&host, &p);
        assert!(!out.coverage.is_empty());
        for ring in out.coverage.all_rings_mm() {
            for pt in ring {
                assert!(host.covers_point(pt), "{pt:?} coverage outside host");
            }
        }
    }

    #[test]
    fn explicit_sites() {
        let p = IsolineBlobParams {
            seed: 1,
            sites: SiteSource::Points(vec![(20.0, 20.0), (55.0, 50.0)]),
            contour_spacing: 0.35,
            level_count: 10,
            blob_radius: 5.0,
            warp: 0.4,
        };
        let out = fill_isoline_blobs_with_coverage(&square(), &p);
        assert!(out.strokes.len() >= 20);
    }
}
