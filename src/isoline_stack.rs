//! Layered forest & vegetation canopy contours seen from above.
//!
//! Models each tree crown as a 3D cluster of overlapping, jagged foliage masses
//! (sub-boughs and canopy tiers) rather than smooth concentric target rings or
//! simple parallel curve offsets:
//! 1. **Jagged, concave-convex internal tiers** — every contour (from the outer
//!    canopy skirt to the innermost upper bough tufts) maintains leaf-scale
//!    jaggedness (`jitter` intensity and `jitter_period` wavelength), deep inward
//!    concavities (bough clefts), and outward convexities with **independent
//!    angular phases, centers, and clefts** at each height level.
//! 2. **Overlapping 3D sub-boughs & broken crescent arcs** — inside the outer
//!    skirt, offset sub-bough ridges step upward toward the crown peak; upper
//!    tiers cut across lower tiers and inner contours open into jagged C-arcs
//!    and asymmetric foliage creases.
//! 3. **Optional perpendicular slope ticks (`ticks` / `Light.hatch`)** — short
//!    cross-contour hachure strokes running perpendicular to the contour rings
//!    along bough clefts and shadowed slopes, breaking pure concentricity.
//! 4. **Gradual stochastic edge scattering** — between dense forest groves and
//!    open clearings, tree probability and crown size taper smoothly through a
//!    scattering transition zone of medium trees, shrubs, and isolated bushes.
//! 5. **Fast analytic LUT occlusion + spatial de-overlap** — instead of running
//!    dozens of expensive polygon boolean unions (`i_overlay`) across every
//!    slice, each 3D foliage mass precomputes a 144-bin polar radius lookup table
//!    in a 2D spatial grid for $O(1)$ hidden-line removal, followed by a fast
//!    spatial-hash de-overlap pass.

use crate::fray::fray_strokes;
use crate::geo::{Pt, Region, Strokes};
use crate::isoline_blobs::FillWithCoverage;
use crate::noise::Perlin;
pub use crate::pattern::EdgeMode;
use crate::pattern::{FillOutput, FillParams};
use crate::random::{poisson_disk, Rng};
use std::f64::consts::TAU;

const LUT_BINS: usize = 144;

/// A crown at a known position (a mapped tree): `radius` across in mm,
/// `height` how tall it stands among its neighbours, in mm.
#[derive(Clone, Copy, Debug)]
pub struct Crown {
    pub at: Pt,
    pub radius: f64,
    pub height: f64,
}

/// A sun raking across every crown from one direction: foliage tiers pack
/// tighter on the shadowed flank and open into airy crescent arcs on the sunlit
/// flank while preserving the outer leafy silhouette.
#[derive(Clone, Copy, Debug)]
pub struct Light {
    /// Degrees, 0 = +x, 90 = +y: the direction the light comes *from*.
    pub from_deg: f64,
    /// 0 – 1: how strong the shading is.
    pub strength: f64,
    /// Degrees above the horizon; lower is longer shadows.
    pub elevation_deg: f64,
    /// mm between shading hatch lines; 0 draws no hatching.
    pub hatch: f64,
}

impl Default for Light {
    fn default() -> Self {
        Light { from_deg: -45.0, strength: 0.82, elevation_deg: 35.0, hatch: 0.0 }
    }
}

#[derive(Clone, Debug)]
pub enum CrownSource {
    /// Scattered over the region, patchy by `coverage` with gradual clearing scatter.
    Generate,
    /// Exactly these crowns; `coverage` is ignored.
    Crowns(Vec<Crown>),
}

#[derive(Clone, Debug)]
pub struct IsolineStackParams {
    pub seed: u64,
    pub crowns: CrownSource,
    /// Typical crown radius in mm.
    pub crown: f64,
    /// Typical leaf-cluster / bough-lobe detail scale in mm.
    pub bubble: f64,
    /// 0 – 1: share of the region the generated crowns cover.
    pub coverage: f64,
    /// Typical number of contour layers through a dominant crown.
    pub slices: usize,
    /// 0 – 2+: jitter intensity — how much jittered control points are displaced
    /// (`0.0` = smooth bough contours, `1.1` = crisp deciduous displacement,
    /// `1.5+` = deep, ragged serrated displacement).
    pub jitter: f64,
    /// 0.2 – 4.0: jitter period — spatial wavelength / spacing between jittered points
    /// along each contour (controls *how many points* we jitter around a crown:
    /// `0.4` = many closely-spaced points / high-frequency serration,
    /// `1.0` = balanced botanical point density,
    /// `2.2` = few widely-spaced points / broad angular reaches).
    pub jitter_period: f64,
    /// 0 – 1: crown aspect-ratio elongation / wind-swept ellipticity (`0.18` default).
    pub elongation: f64,
    /// 0 – 2+: prominence of deep bough clefts and compound sub-boughs (`1.0` default).
    pub lobes: f64,
    /// 0 – 1+: density of short perpendicular slope-tick / bark-crease strokes
    /// that bridge between consecutive contour tiers (`0.0` = contours only;
    /// `0.5` = moderate cleft & shadow hachures; `1.0` = rich cross-contour detail).
    /// Also activated automatically when `light` has `hatch > 0.0`.
    pub ticks: f64,
    pub edge: EdgeMode,
    /// Smooth perimeter disappearing band in mm (`0.0` = off).
    pub fray: f64,
    /// Directional sunlight across the canopy (`Some(Light::default())` by default).
    pub light: Option<Light>,
}

impl Default for IsolineStackParams {
    fn default() -> Self {
        IsolineStackParams {
            seed: 0,
            crowns: CrownSource::Generate,
            crown: 6.0,
            bubble: 1.8,
            coverage: 0.6,
            slices: 9,
            jitter: 1.10,
            jitter_period: 0.68,
            elongation: 0.18,
            lobes: 1.0,
            ticks: 0.42,
            edge: EdgeMode::Clip,
            fray: 0.0,
            light: Some(Light::default()),
        }
    }
}

impl IsolineStackParams {
    /// Build from standardized [`FillParams`] and its `custom` key-value map.
    ///
    /// Supported custom keys:
    /// - `"jitter"`, `"jitter_intensity"`, `"intensity"`, or `"jaggedness"`:
    ///   displacement intensity (`0.0..=2.5`, default `1.10`)
    /// - `"jitter_period"` or `"period"`:
    ///   jitter wavelength / point spacing (`0.2..=4.0`, default `0.68`; smaller = more points jittered)
    /// - `"coverage"`: forest coverage (`0.0..=1.0`, default `0.6`)
    /// - `"slices"`: vertical contour layers (`2..=18`, default `9`)
    /// - `"bubble"`: leaf-tuft scale in mm
    /// - `"elongation"`: crown elliptical elongation (`0.0..=1.0`, default `0.18`)
    /// - `"lobes"`: bough cleft depth & compound sub-crown lobing (`0.0..=2.5`, default `1.0`)
    /// - `"ticks"`, `"perpendicular"`, or `"hachures"`: perpendicular cross-contour slope ticks (`0.0..=1.5`, default `0.42`)
    /// - `"light"`: directional light strength (`0.0..=1.0`, default `0.82`; `0.0` = off)
    /// - `"light_angle"`: directional light angle in degrees (default `-45.0`)
    pub fn from_fill_params(params: &FillParams, default_crown: f64) -> Self {
        let crown = params.custom.get_or("crown", default_crown).max(0.6);
        let bubble = params.custom.get_or("bubble", (crown * 0.30).max(0.30));
        let coverage = params.custom.get_or("coverage", 0.60).clamp(0.0, 1.0);
        let slices = params.custom.get_or("slices", 9.0).round().clamp(2.0, 18.0) as usize;
        let jitter = params
            .custom
            .get("jitter")
            .or_else(|| params.custom.get("jitter_intensity"))
            .or_else(|| params.custom.get("intensity"))
            .or_else(|| params.custom.get("jaggedness"))
            .unwrap_or(1.10);
        let jitter_period = params
            .custom
            .get("jitter_period")
            .or_else(|| params.custom.get("period"))
            .unwrap_or(0.68)
            .clamp(0.20, 4.0);
        let elongation = params.custom.get_or("elongation", 0.18).clamp(0.0, 1.0);
        let lobes = params.custom.get_or("lobes", 1.0).clamp(0.0, 2.5);
        let hatch_mm = params.custom.get_or("hatch", 0.0).max(0.0);
        let ticks = params
            .custom
            .get("ticks")
            .or_else(|| params.custom.get("perpendicular"))
            .or_else(|| params.custom.get("hachures"))
            .unwrap_or(if hatch_mm > 0.01 { 0.65 } else { 0.42 })
            .clamp(0.0, 1.5);
        let light_strength = params.custom.get_or("light", 0.82).clamp(0.0, 1.0);
        let light = (light_strength > 0.01).then(|| Light {
            from_deg: params.custom.get_or("light_angle", -45.0),
            strength: light_strength,
            hatch: hatch_mm,
            ..Light::default()
        });

        IsolineStackParams {
            seed: params.seed,
            crowns: CrownSource::Generate,
            crown,
            bubble,
            coverage,
            slices,
            jitter,
            jitter_period,
            elongation,
            lobes,
            ticks,
            edge: params.edge,
            fray: params.fray,
            light,
        }
    }
}

pub fn fill_isoline_stack(region: &Region, p: &IsolineStackParams) -> Strokes {
    fill_isoline_stack_with_coverage(region, p).strokes
}

/// Standardized entry point from [`FillParams`] (where `params.spacing` is the
/// crown radius, and `params.custom` can override `jaggedness`, `coverage`,
/// `slices`, `light`, etc.).
pub fn fill_isoline_stack_params(region: &Region, params: &FillParams) -> FillOutput {
    let target = params.resolve_region(region, 0.0);
    let crown = params.spacing.max(0.6);
    let p = IsolineStackParams::from_fill_params(params, crown);
    fill_isoline_stack_with_coverage(&target, &p).into()
}

/// Primary bough lobe of a tree crown: defines one of the 3–5 asymmetric structural
/// branches that fuse to form the crown's non-circular base silhouette.
#[derive(Clone, Copy, Debug)]
struct BoughLobe {
    angle: f64,
    reach: f64,
    half_width: f64,
    skew: f64,
    max_u: f64,
    cleft_angle: f64,
    cleft_depth: f64,
    cleft_width: f64,
}

/// One 3D jagged contour layer belonging either to a tree's main elevation slice or to
/// one of its internal overlapping bough-crease arcs.
#[derive(Clone, Debug)]
struct JaggedLayer {
    /// Tree id this layer belongs to.
    tree_id: usize,
    /// Center of this specific contour layer.
    center: Pt,
    /// 3D elevation of this layer (higher occludes lower; equal fuses across trees).
    z_occlude: f64,
    /// Whether this layer is the base footprint of a tree/bough (contributes to `coverage` mask).
    is_base_footprint: bool,
    /// Whether this layer is drawn as an open C-arc rather than a full 360° ring.
    is_open_arc: bool,
    /// Whether this layer is an internal off-center sub-bough crease (only occludes within `arc_window`).
    is_sub_bough: bool,
    /// Angular window `(center_angle, half_span)` when `is_open_arc` is true.
    arc_window: (f64, f64),
    /// Normalized height `u in [0, 1]` within its tree.
    u_norm: f64,
    /// Precomputed polar radius at `LUT_BINS` evenly spaced angles in `[0, TAU)`
    /// for $O(1)$ analytical occlusion testing without polygon booleans.
    lut: [f64; LUT_BINS],
    min_r: f64,
    max_r: f64,
    /// Contour vertices in mm.
    ring: Vec<Pt>,
}

impl JaggedLayer {
    /// Fast $O(1)$ analytical test: does the 3D volume of this layer cover point `p`
    /// (shrunk by `margin`)?
    /// Main crown slices (`!is_sub_bough`) are solid 3D horizontal cross-sections that
    /// occlude across all 360° even when their sunlit rim is left un-inked as a highlight.
    #[inline]
    fn covers_volume(&self, p: Pt, margin: f64) -> bool {
        let dx = p.0 - self.center.0;
        let dy = p.1 - self.center.1;
        let d2 = dx * dx + dy * dy;
        let max_eff = (self.max_r - margin).max(0.0);
        if d2 >= max_eff * max_eff {
            return false;
        }
        if !self.is_sub_bough {
            let min_eff = (self.min_r - margin).max(0.0);
            if d2 <= min_eff * min_eff {
                return true;
            }
        }
        let dist = d2.sqrt();
        let mut theta = dy.atan2(dx);
        if theta < 0.0 {
            theta += TAU;
        }
        if self.is_sub_bough {
            let (win_center, half_span) = self.arc_window;
            if angle_diff(theta, win_center).abs() > half_span {
                return false;
            }
        }
        let pos = (theta / TAU) * (LUT_BINS as f64);
        let idx0 = (pos.floor() as usize) % LUT_BINS;
        let idx1 = (idx0 + 1) % LUT_BINS;
        let frac = pos - pos.floor();
        let r_bound = self.lut[idx0] * (1.0 - frac) + self.lut[idx1] * frac;
        dist < (r_bound - margin)
    }
}

/// 2D uniform spatial grid indexing all `JaggedLayer`s for fast local occlusion queries.
struct SpatialOccluder {
    min_x: f64,
    min_y: f64,
    cell_size: f64,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<usize>>,
}

impl SpatialOccluder {
    fn new(layers: &[JaggedLayer], cell_size: f64) -> Self {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        for l in layers {
            min_x = min_x.min(l.center.0 - l.max_r);
            min_y = min_y.min(l.center.1 - l.max_r);
            max_x = max_x.max(l.center.0 + l.max_r);
            max_y = max_y.max(l.center.1 + l.max_r);
        }
        let cs = cell_size.max(1.0);
        let cols = (((max_x - min_x) / cs).ceil() as usize + 1).clamp(1, 160);
        let rows = (((max_y - min_y) / cs).ceil() as usize + 1).clamp(1, 160);
        let cs_x = ((max_x - min_x) / cols as f64).max(cs);
        let cs_y = ((max_y - min_y) / rows as f64).max(cs);
        let cs_final = cs_x.max(cs_y);

        let mut cells = vec![Vec::new(); cols * rows];
        for (idx, l) in layers.iter().enumerate() {
            let c0 = (((l.center.0 - l.max_r - min_x) / cs_final).floor().max(0.0) as usize).min(cols - 1);
            let c1 = (((l.center.0 + l.max_r - min_x) / cs_final).floor().max(0.0) as usize).min(cols - 1);
            let r0 = (((l.center.1 - l.max_r - min_y) / cs_final).floor().max(0.0) as usize).min(rows - 1);
            let r1 = (((l.center.1 + l.max_r - min_y) / cs_final).floor().max(0.0) as usize).min(rows - 1);
            for r in r0..=r1 {
                for c in c0..=c1 {
                    cells[r * cols + c].push(idx);
                }
            }
        }

        SpatialOccluder {
            min_x,
            min_y,
            cell_size: cs_final,
            cols,
            rows,
            cells,
        }
    }

    #[inline]
    fn is_occluded(&self, p: Pt, self_idx: usize, layers: &[JaggedLayer], overlap_slack: f64) -> bool {
        let c = (((p.0 - self.min_x) / self.cell_size).floor() as isize).clamp(0, self.cols as isize - 1) as usize;
        let r = (((p.1 - self.min_y) / self.cell_size).floor() as isize).clamp(0, self.rows as isize - 1) as usize;
        let self_layer = &layers[self_idx];

        for &other_idx in &self.cells[r * self.cols + c] {
            if other_idx == self_idx {
                continue;
            }
            let other = &layers[other_idx];
            if other.tree_id == self_layer.tree_id {
                if other.z_occlude <= self_layer.z_occlude + 1e-4 {
                    continue;
                }
                if other.is_sub_bough {
                    if !self_layer.is_base_footprint && other.covers_volume(p, overlap_slack * 0.5) {
                        return true;
                    }
                } else if other.covers_volume(p, 0.03 * self_layer.max_r) {
                    return true;
                }
                continue;
            }

            // Different tree:
            if other.is_sub_bough {
                continue;
            }
            if other.z_occlude > self_layer.z_occlude + 1e-4 {
                // Higher neighboring crown slice overhangs and hides lower slice
                if other.covers_volume(p, overlap_slack) {
                    return true;
                }
            } else if !self_layer.is_sub_bough && (other.z_occlude - self_layer.z_occlude).abs() <= 1e-4 {
                // Same-elevation slice fusion: touching crowns at the same elevation merge
                // into a single connected multi-lobed contour without internal crossing rings!
                let fuse_margin = (overlap_slack * 1.5).max(0.03);
                if other.covers_volume(p, fuse_margin) {
                    return true;
                }
            }
        }
        false
    }
}

pub fn fill_isoline_stack_with_coverage(region: &Region, p: &IsolineStackParams) -> FillWithCoverage {
    if region.is_empty() || p.crown <= 0.0 || p.slices == 0 || p.coverage <= 0.0 {
        return FillWithCoverage::default();
    }

    let mut rng = Rng::new(p.seed.wrapping_add(137));
    let perlin = Perlin::new(p.seed.wrapping_add(59));

    let light_vec = p.light.map(|l| {
        let r = l.from_deg.to_radians();
        ((r.cos(), r.sin()), l.strength.clamp(0.0, 1.0))
    });

    let (clip_region, clamp_region) = match p.edge {
        EdgeMode::Clip => (region.clone(), None),
        EdgeMode::Inside => {
            let inset_margin = (p.crown * 0.025).clamp(0.06, 0.18);
            let inner = region.inset(inset_margin);
            if inner.is_empty() {
                return FillWithCoverage::default();
            }
            (region.clone(), Some(inner))
        }
        EdgeMode::Overflow(m) => {
            let m = m.max(0.0);
            let out = region.outset(m);
            (out.clone(), Some(out))
        }
    };

    let layers = match &p.crowns {
        CrownSource::Crowns(crowns) => {
            build_explicit_crowns(crowns, clamp_region.as_ref(), p, light_vec, &perlin, &mut rng)
        }
        CrownSource::Generate => {
            generate_forest_layers(region, clamp_region.as_ref(), p, light_vec, &perlin, &mut rng)
        }
    };

    if layers.is_empty() {
        return FillWithCoverage::default();
    }

    // Build 1 polygon coverage mask from the base footprint rings (single i_overlay call).
    let base_polys: Vec<Vec<Vec<Pt>>> = layers
        .iter()
        .filter(|l| l.is_base_footprint && l.ring.len() >= 3)
        .map(|l| vec![l.ring.clone()])
        .collect();
    let coverage = Region::from_polygons(&base_polys).intersection(&clip_region);

    // Render all contours using fast $O(1)$ analytical LUT occlusion + spatial de-overlap.
    let mut strokes = render_layers_fast(&layers, p, light_vec);

    strokes = match p.edge {
        EdgeMode::Clip => clip_region.clip_strokes(&strokes),
        EdgeMode::Inside => strokes,
        EdgeMode::Overflow(_) => clip_region.clip_strokes(&strokes),
    };

    // Fast spatial-hash de-overlap so adjacent/overlapping layers of different shapes
    // trim cleanly against one another without double-inking.
    let min_clearance = (p.crown * 0.024).clamp(0.05, 0.15);
    strokes = deoverlap_strokes(strokes, min_clearance);

    let min_len = (p.crown * 0.06).clamp(0.07, 0.36);
    strokes.retain(|s| polyline_len(s) >= min_len);
    strokes = order_strokes_for_plotter(strokes);

    if p.fray > 0.0 {
        strokes = fray_strokes(region, strokes, p.crown, p.fray, p.seed.wrapping_add(911));
    }

    FillWithCoverage { strokes, coverage }
}

fn build_explicit_crowns(
    crowns: &[Crown],
    clamp_region: Option<&Region>,
    p: &IsolineStackParams,
    light: Option<(Pt, f64)>,
    perlin: &Perlin,
    rng: &mut Rng,
) -> Vec<JaggedLayer> {
    let mut layers = Vec::new();
    for (idx, c) in crowns.iter().copied().enumerate() {
        if c.radius <= 0.0 || c.height <= 0.0 {
            continue;
        }
        let rings = p.slices.clamp(3, 16);
        append_tree_layers(
            &mut layers,
            idx,
            c.at,
            c.radius,
            c.height * 0.15,
            c.height,
            rings,
            p.jitter,
            p.jitter_period,
            p.bubble,
            p.elongation,
            p.lobes,
            light,
            clamp_region,
            perlin,
            rng,
            false,
            false,
        );
    }
    layers
}

/// Generate a forest with gradual stochastic scattering between dense groves and clearings,
/// fusing neighbouring crowns at shared elevation slices into multi-lobed canopy masses.
fn generate_forest_layers(
    region: &Region,
    clamp_region: Option<&Region>,
    p: &IsolineStackParams,
    light: Option<(Pt, f64)>,
    perlin: &Perlin,
    rng: &mut Rng,
) -> Vec<JaggedLayer> {
    let cov = p.coverage.clamp(0.0, 1.0);
    let r0 = p.crown;

    let seed_region = match p.edge {
        EdgeMode::Clip => region.outset(r0 * 0.70),
        EdgeMode::Inside => {
            let min_r = (r0 * 0.34).max(0.24);
            let inset = region.inset(min_r);
            if inset.is_empty() {
                region.inset(r0 * 0.14)
            } else {
                inset
            }
        }
        EdgeMode::Overflow(_) => region.clone(),
    };
    if seed_region.is_empty() {
        return Vec::new();
    }

    let (x0, y0, x1, y1) = seed_region.bounds_mm();
    // Candidate spacing allows natural crown fusion in grove cores without overcrowding.
    let step = (r0 * (1.44 - 0.32 * cov)).max(0.38);
    let mut candidates: Vec<Pt> = poisson_disk(&seed_region, step, rng.next_u64())
        .into_iter()
        .filter(|&pt| seed_region.covers_point(pt))
        .collect();

    if candidates.is_empty() {
        let cx = 0.5 * (x0 + x1);
        let cy = 0.5 * (y0 + y1);
        if seed_region.covers_point((cx, cy)) {
            candidates.push((cx, cy));
        } else {
            return Vec::new();
        }
    }

    // Evaluate a multi-scale grove field + medium clump noise + stochastic scatter so the
    // boundary between full forest and empty clearing is a gradual, organic scatter zone!
    let grove_scale = (r0 * 3.2).max(1.5);
    let mut scored: Vec<(f64, Pt)> = candidates
        .into_iter()
        .map(|pt| {
            let g_macro = perlin.fbm(pt.0 / grove_scale, pt.1 / grove_scale, 2);
            let g_meso = perlin.noise(
                pt.0 / (grove_scale * 0.40) + 17.3,
                pt.1 / (grove_scale * 0.40) - 13.1,
            );
            let scatter_noise = rng.range(-0.38, 0.38);
            let score = 0.42 * g_macro + 0.30 * g_meso + 0.28 * scatter_noise;
            (score, pt)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let total = scored.len().max(1);
    let min_ring_step = (r0 * 0.125).clamp(0.25, 0.52);
    let base_slices = p.slices.clamp(3, 14);

    // Scale the transition band with `cov` so `coverage = 0.10` (10% trees) accurately produces
    // sparse groves + scattered pioneer trees, while `coverage = 1.00` fills 100% of the canopy.
    let scatter_band = (0.34 * cov.sqrt()).clamp(0.06, 0.36);

    let mut layers: Vec<JaggedLayer> = Vec::new();
    let mut tree_id = 0usize;

    for (rank, &(_score, center)) in scored.iter().enumerate() {
        let q = rank as f64 / total as f64;

        let dist_from_edge = (cov - q) / scatter_band;
        let accept_prob = if cov >= 0.97 {
            1.0
        } else {
            let s = 1.0 / (1.0 + (-2.4 * dist_from_edge).exp());
            (s * 0.92 + 0.14 * cov * (-2.2 * (q - cov).max(0.0)).exp()).clamp(0.0, 1.0)
        };

        if rng.unit() > accept_prob {
            continue;
        }

        let local_density = (1.0 / (1.0 + (-2.0 * dist_from_edge).exp())).clamp(0.15, 1.0);
        // Avoid tiny pebble/bubble sizes; keep crowns in a cohesive, legible botanical range.
        let size_factor = if local_density > 0.68 {
            rng.range(0.82, 1.30)
        } else if local_density > 0.35 {
            if rng.unit() < 0.55 {
                rng.range(0.68, 1.02)
            } else {
                rng.range(0.48, 0.72)
            }
        } else {
            rng.range(0.45, 0.80)
        };

        let mut r_tree = r0 * size_factor;
        if let Some(clamp) = clamp_region {
            if matches!(p.edge, EdgeMode::Inside) {
                r_tree = r_tree.min(clamp.dist_to_boundary(center) * 0.92);
            }
        }
        if r_tree < (r0 * 0.24).max(0.22) {
            continue;
        }

        let max_rings = ((r_tree * 0.90) / min_ring_step).floor() as usize;
        let target_rings = ((base_slices as f64) * (r_tree / r0).powf(0.72)).round() as usize;
        let rings = target_rings.clamp(3, max_rings.max(3)).min(12);

        // Quantized elevation stratum (`0.0` for most trees so neighbours fuse into compound
        // multi-lobed canopy masses at every shared contour level; `1.0` for ~24% dominant
        // grove trees so dominant crowns can also overhang lower neighbours in 3D).
        let z_stratum = if local_density > 0.62 && size_factor > 1.02 && rng.unit() < 0.26 {
            1.0
        } else {
            0.0
        };
        let z_top = z_stratum + (rings as f64);

        let is_multi_bough = r_tree >= r0 * 0.58 && p.jitter > 0.05 && p.lobes > 0.35;
        append_tree_layers(
            &mut layers,
            tree_id,
            center,
            r_tree,
            z_stratum,
            z_top,
            rings,
            p.jitter,
            p.jitter_period,
            p.bubble,
            p.elongation,
            p.lobes,
            light,
            clamp_region,
            perlin,
            rng,
            is_multi_bough,
            true,
        );
        tree_id += 1;

        // Spawn 1 overlapping companion crown at close range (`0.52..0.86 * r_tree`) sharing the
        // exact same `z_stratum` so pairs/trios fuse into elongated, kidney- or L-shaped groves
        // instead of isolated round polka-dot bubbles!
        let want_companion = if local_density < 0.78 && local_density > 0.20 {
            rng.unit() < 0.68
        } else {
            rng.unit() < 0.42
        };

        if want_companion {
            let ang = rng.range(0.0, TAU);
            let dist_mult = rng.range(0.52, 0.86);
            let comp_c = (
                center.0 + r_tree * dist_mult * ang.cos(),
                center.1 + r_tree * dist_mult * ang.sin(),
            );
            if seed_region.covers_point(comp_c) {
                let mut r_comp = r_tree * rng.range(0.58, 0.86);
                if let Some(clamp) = clamp_region {
                    if matches!(p.edge, EdgeMode::Inside) {
                        r_comp = r_comp.min(clamp.dist_to_boundary(comp_c) * 0.92);
                    }
                }
                if r_comp >= (r0 * 0.24).max(0.22) {
                    let comp_max_rings = ((r_comp * 0.90) / min_ring_step).floor() as usize;
                    let comp_rings = (rings.saturating_sub(1)).clamp(3, comp_max_rings.max(3));
                    append_tree_layers(
                        &mut layers,
                        tree_id,
                        comp_c,
                        r_comp,
                        z_stratum,
                        z_stratum + (comp_rings as f64),
                        comp_rings,
                        p.jitter,
                        p.jitter_period,
                        p.bubble,
                        p.elongation,
                        p.lobes,
                        light,
                        clamp_region,
                        perlin,
                        rng,
                        false,
                        true,
                    );
                    tree_id += 1;
                }
            }
        }
    }

    layers
}

/// Build the 3D jagged layers of a single tree crown:
/// - Generates 3–5 asymmetric primary `BoughLobe`s that fuse into a compound, non-circular
///   crown envelope and recede at different heights `max_u` so upper slices migrate into the
///   dominant bough ridge.
/// - Applies categorized jittering separated into `jitter_period` (how many points around the
///   contour are jittered / spatial wavelength) and `jitter` (displacement intensity).
#[allow(clippy::too_many_arguments)]
fn append_tree_layers(
    out: &mut Vec<JaggedLayer>,
    tree_id: usize,
    center: Pt,
    radius: f64,
    z_base: f64,
    z_top: f64,
    rings: usize,
    jitter: f64,
    jitter_period: f64,
    bubble: f64,
    elongation: f64,
    lobes: f64,
    light: Option<(Pt, f64)>,
    clamp_region: Option<&Region>,
    perlin: &Perlin,
    rng: &mut Rng,
    multi_bough: bool,
    quantized_z: bool,
) {
    let mut local_rng = Rng::new(rng.next_u64().wrapping_add((tree_id as u64).wrapping_mul(0x9E3779B97F4A7C15)));
    let j = jitter.clamp(0.0, 2.5);
    let period = jitter_period.clamp(0.20, 4.0);
    let elong = elongation.clamp(0.0, 1.0);
    let lobe_scale = lobes.clamp(0.0, 2.5);

    // Overall crown drift (eccentricity + light-driven shadow shift).
    let nat_ang = local_rng.range(0.0, TAU);
    let nat_mag = if j > 0.0 {
        radius * local_rng.range(0.15, 0.26) * j.min(1.0)
    } else {
        0.0
    };
    let mut drift_dx = nat_mag * nat_ang.cos();
    let mut drift_dy = nat_mag * nat_ang.sin();
    let mut skirt_dx = 0.55 * drift_dx;
    let mut skirt_dy = 0.55 * drift_dy;
    let mut shadow_dir = nat_ang;

    if let Some(((lx, ly), strength)) = light {
        let eff_s = if j > 0.05 { strength.max(0.22) } else { strength };
        let blend = (eff_s * 0.85).clamp(0.0, 1.0);
        skirt_dx = skirt_dx * (1.0 - blend) - lx * radius * 0.28 * eff_s;
        skirt_dy = skirt_dy * (1.0 - blend) - ly * radius * 0.28 * eff_s;
        drift_dx = drift_dx * (1.0 - blend) - lx * radius * 0.25 * eff_s;
        drift_dy = drift_dy * (1.0 - blend) - ly * radius * 0.25 * eff_s;
        shadow_dir = (-ly).atan2(-lx) + local_rng.range(-0.22, 0.22) * (1.0 - 0.65 * strength);
    }

    let tooth_mm = (bubble * 0.52).clamp(radius * 0.11, radius * 0.36).max(0.24);
    let base_oval_phase = local_rng.range(0.0, TAU);
    let base_oval_amp = if j > 0.0 || elong > 0.01 {
        (elong * 0.36 + local_rng.range(0.06, 0.14) * j.min(1.2)).clamp(0.0, 0.46)
    } else {
        0.0
    };
    let noise_base = (local_rng.range(-80.0, 80.0), local_rng.range(-80.0, 80.0));

    // Generate 3..=5 asymmetric structural bough lobes for this crown.
    // Boughs have uneven angles, different reaches, and different cutoff heights `max_u`
    // so the crown is a compound, sprawling multi-bough form rather than a perturbed circle.
    let n_bough_lobes = if lobe_scale > 1.25 { 5 } else if lobe_scale > 0.75 { 4 } else { 3 };
    let mut boughs = Vec::with_capacity(n_bough_lobes);
    let base_b_ang = local_rng.range(0.0, TAU);
    let sector = TAU / (n_bough_lobes as f64);
    for b in 0..n_bough_lobes {
        let angle = (base_b_ang + (b as f64) * sector + local_rng.range(-0.34, 0.34) * sector).rem_euclid(TAU);
        // Bough 0 is the dominant apex bough (`max_u = 1.0`); secondary boughs recede earlier.
        let is_primary = b == 0 || (b == 1 && local_rng.unit() < 0.45);
        let reach = if is_primary {
            local_rng.range(0.14, 0.32) * lobe_scale.clamp(0.4, 1.7) * j.clamp(0.25, 1.3)
        } else {
            local_rng.range(-0.06, 0.22) * lobe_scale.clamp(0.4, 1.7) * j.clamp(0.25, 1.3)
        };
        let half_width = sector * local_rng.range(0.48, 0.82);
        let skew = local_rng.range(-0.26, 0.26);
        let max_u = if is_primary {
            1.0
        } else {
            local_rng.range(0.45, 0.85)
        };
        let cleft_angle = (angle + 0.5 * sector + local_rng.range(-0.18, 0.18) * sector).rem_euclid(TAU);
        let cleft_depth = local_rng.range(0.12, 0.26) * lobe_scale.clamp(0.35, 1.65) * j.min(1.4);
        let cleft_width = local_rng.range(0.16, 0.32);
        boughs.push(BoughLobe {
            angle,
            reach,
            half_width,
            skew,
            max_u,
            cleft_angle,
            cleft_depth,
            cleft_width,
        });
    }

    // Step 1: Main elevation contour stack from outer skirt (`k = 0`, `u = 0.0`) up to the
    // crown ridge (`k = rings - 1`, `u = 1.0`, stopping at `rad_scale = 0.22` so we never
    // emit tiny pinhole O-bubbles in the center!).
    let denom = (rings.saturating_sub(1).max(1)) as f64;
    for k in 0..rings {
        let u = if rings <= 1 { 0.0 } else { (k as f64) / denom };
        let rad_scale = (1.0 - 0.78 * u.powf(0.92)).max(0.22);
        let r_mean = radius * rad_scale;

        // Lateral wander at level `k` so successive layers shift across internal boughs
        let wander_ang = nat_ang + (k as f64) * 2.1 + local_rng.range(-0.5, 0.5);
        let wander_mag = if j > 0.0 && k > 0 {
            radius * 0.065 * (u.sqrt() * (1.0 - 0.45 * u)) * j.min(1.2)
        } else {
            0.0
        };
        let skirt_w = (1.0 - u).powi(2);
        let cx = center.0 + skirt_dx * skirt_w + drift_dx * u.powf(0.85) + wander_mag * wander_ang.cos();
        let cy = center.1 + skirt_dy * skirt_w + drift_dy * u.powf(0.85) + wander_mag * wander_ang.sin();

        let z_layer = if quantized_z {
            z_base + (k + 1) as f64
        } else {
            z_base + (z_top - z_base) * (0.04 + 0.94 * u)
        };

        // Inner tiers open on their sunlit/anti-aspect flank into cascading C-arcs, narrowing
        // near the peak (`u > 0.65`) into crisp ridge strokes instead of closed small loops.
        let light_s = light.map(|(_, s)| s).unwrap_or(0.0);
        let (is_open_arc, arc_window) = if j > 0.05 && k >= 1 {
            let open_dir = shadow_dir + local_rng.range(-0.36, 0.36);
            let span_base = 2.38 - 0.56 * light_s - 0.52 * u.powf(1.2);
            let half_span = (span_base + local_rng.range(-0.20, 0.20)).clamp(0.95, 2.62);
            (true, (open_dir, half_span))
        } else {
            (false, (0.0, TAU))
        };

        let layer = build_jagged_ring_layer(
            tree_id,
            (cx, cy),
            r_mean,
            radius,
            z_layer,
            u,
            k == 0,
            is_open_arc,
            false,
            arc_window,
            j,
            period,
            lobe_scale,
            tooth_mm,
            base_oval_amp,
            base_oval_phase + (k as f64) * 0.38 * (1.0 - 0.65 * elong),
            &boughs,
            noise_base,
            k as u64,
            clamp_region,
            perlin,
            &mut local_rng,
        );
        out.push(layer);
    }

    // Step 2: Internal overlapping bough-crease arcs.
    // Instead of small 360°/220° round sub-circles (which read as little bubbles), these are
    // broad, shallow crease arcs (`b_radius = 0.42..0.60 * radius`, `half_span = 0.72..1.15 rad`)
    // that tuck into the saddle between major boughs.
    if multi_bough && rings >= 5 {
        let n_creases = if lobe_scale > 1.35 { 2 } else { 1 };
        let bough_base_ang = local_rng.range(0.0, TAU);

        for b_idx in 0..n_creases {
            let b_ang = bough_base_ang + (b_idx as f64) * (TAU / n_creases as f64) + local_rng.range(-0.45, 0.45);
            let b_dist = radius * local_rng.range(0.18, 0.34) * lobe_scale.clamp(0.6, 1.35);
            let b_center = (
                center.0 + b_dist * b_ang.cos() + 0.35 * drift_dx,
                center.1 + b_dist * b_ang.sin() + 0.35 * drift_dy,
            );
            let b_radius = radius * local_rng.range(0.42, 0.58);
            let b_u = (0.32 + 0.25 * (b_idx as f64)).clamp(0.25, 0.75);
            let z_b = if quantized_z {
                z_base + (1.0 + b_u * (rings as f64 - 1.0)).round() + 0.35
            } else {
                z_base + (z_top - z_base) * (0.04 + 0.94 * b_u)
            };

            let arc_dir = shadow_dir + local_rng.range(-0.55, 0.55);
            let arc_win = (arc_dir, local_rng.range(0.72, 1.12));

            let layer = build_jagged_ring_layer(
                tree_id,
                b_center,
                b_radius,
                radius,
                z_b,
                b_u,
                false,
                true,
                true,
                arc_win,
                j * 1.02,
                period,
                lobe_scale,
                tooth_mm,
                base_oval_amp * 1.15,
                base_oval_phase + local_rng.range(-0.4, 0.4),
                &boughs,
                (noise_base.0 + (b_idx as f64 + 1.0) * 23.0, noise_base.1 - (b_idx as f64 + 1.0) * 17.0),
                (100 + b_idx * 17) as u64,
                clamp_region,
                perlin,
                &mut local_rng,
            );
            out.push(layer);
        }
    }
}

/// Construct a single `JaggedLayer` using categorized jittering:
/// 1. **Macro Bough Envelope**: evaluates the crown's 3–5 asymmetric `BoughLobe`s and V-clefts
///    at height `u_norm` so the underlying form is an irregular multi-bough mass, not a circle.
/// 2. **Jitter Period (`jitter_period`)**: determines *how many control points* `n_ctrl` around
///    the contour are sampled and jittered (`step_mm ∝ crown_radius * jitter_period`), as well
///    as the spatial wavelength of the 2D noise field.
/// 3. **Jitter Intensity (`jaggedness` / `jitter`)**: determines *how much* each control point is
///    displaced (both radially with chiseled outward facets + sharp inward notches, and
///    tangentially to break radial symmetry), with zero sinusoidal bubble waves.
#[allow(clippy::too_many_arguments)]
fn build_jagged_ring_layer(
    tree_id: usize,
    center: Pt,
    r_mean: f64,
    crown_radius: f64,
    z_occlude: f64,
    u_norm: f64,
    is_base_footprint: bool,
    is_open_arc: bool,
    is_sub_bough: bool,
    arc_window: (f64, f64),
    jaggedness: f64,
    jitter_period: f64,
    lobe_scale: f64,
    tooth_mm: f64,
    oval_amp: f64,
    oval_phase: f64,
    boughs: &[BoughLobe],
    noise_base: Pt,
    layer_salt: u64,
    clamp_region: Option<&Region>,
    perlin: &Perlin,
    rng: &mut Rng,
) -> JaggedLayer {
    let j = jaggedness.clamp(0.0, 2.5);
    let lobes = lobe_scale.clamp(0.0, 2.5);
    let s = layer_salt as f64;

    // Effective period combines explicit `jitter_period` with any custom `bubble` ratio.
    let bubble_ratio = (tooth_mm / (crown_radius * 0.156).max(0.15)).clamp(0.60, 1.55);
    let eff_period = (jitter_period.clamp(0.20, 4.0) * bubble_ratio.powf(0.35)).clamp(0.20, 4.0);

    // 1. Step the noise-space origin from layer `k` to `k + 1` so consecutive contour rings
    //    are decoupled and never look like parallel curve offsets.
    let macro_orig = (noise_base.0 + s * 0.58, noise_base.1 - s * 0.51);
    let period_orig = (noise_base.0 + 31.7 + s * 1.18, noise_base.1 - 27.3 + s * 1.04);
    let detail_orig = (noise_base.0 - 47.1 + s * 1.85, noise_base.1 + 53.9 - s * 1.65);

    // Spatial frequency in 2D Perlin space is inversely proportional to `eff_period`.
    let rho_macro = 1.15;
    let rho_period = (2.25 / eff_period.powf(0.78)).clamp(0.68, 5.80);
    let rho_detail = (rho_period * 1.65).clamp(1.20, 7.50);

    let inner_boost = 1.0 + 0.22 * u_norm;
    let macro_amp = (0.18 * j.min(1.4) * lobes.clamp(0.35, 1.6) * inner_boost).min(0.38);
    // Intensity directly controls how far the jittered points are displaced!
    let intensity_amp = (0.36 * j.powf(0.92) * inner_boost).min(0.72);
    let tri_phase = oval_phase * 1.37 + s * 0.45;
    let tri_amp = if j > 0.02 {
        (0.08 * j.min(1.3) * lobes.clamp(0.5, 1.5)).min(0.16)
    } else {
        0.0
    };

    // Helper: evaluate the structural multi-bough + cleft profile at any angle `theta`.
    let eval_bough_profile = |theta: f64| -> f64 {
        let (ct, st) = (theta.cos(), theta.sin());
        let oval = oval_amp * (2.0 * (theta - oval_phase)).cos()
            + tri_amp * (3.0 * (theta - tri_phase)).cos();
        let n_mac = perlin.fbm(macro_orig.0 + rho_macro * ct, macro_orig.1 + rho_macro * st, 2);

        let mut bough_sum = 0.0;
        let mut cleft_sum = 0.0;
        for b in boughs {
            // Bough prominence tapers off as elevation `u_norm` exceeds `b.max_u`
            let active = ((b.max_u - u_norm + 0.28) / 0.35).clamp(0.15, 1.0);
            let d_b = angle_diff(theta, b.angle + b.skew * u_norm);
            if d_b.abs() < b.half_width {
                let u_ang = d_b / b.half_width;
                // Flat-crested bough profile `(1 - u^4)` rather than a round parabola `(1 - u^2)`!
                let plateau = (1.0 - u_ang * u_ang * u_ang * u_ang).max(0.0);
                bough_sum += b.reach * active * plateau;
            }
            // Narrow V-cleft between boughs (drifts slightly with height `s` so clefts don't line up like spokes)
            let c_ang = b.cleft_angle + 0.14 * s * b.skew;
            let d_c = angle_diff(theta, c_ang).abs();
            if d_c < b.cleft_width {
                let t = 1.0 - d_c / b.cleft_width;
                cleft_sum -= b.cleft_depth * (0.75 + 0.25 * active) * t * t;
            }
        }
        1.0 + oval + macro_amp * n_mac + bough_sum + cleft_sum
    };

    // 2. Determine HOW MANY POINTS (`n_ctrl`) we jitter around the contour from `eff_period`.
    let circum = TAU * r_mean;
    let step_mm = (crown_radius * 0.19 * eff_period).max(0.18);
    let n_ctrl = if j <= 0.02 {
        ((circum / 0.28).round() as usize).clamp(28, 96)
    } else {
        ((circum / step_mm).round() as usize).clamp(7, 88)
    };

    let d_theta = TAU / (n_ctrl as f64);
    let mut ctrl_pts: Vec<Pt> = Vec::with_capacity(n_ctrl);
    let mut ctrl_r: Vec<f64> = Vec::with_capacity(n_ctrl);
    let mut prev_kick = rng.range(-1.0, 1.0);

    // 3. Displace each of the `n_ctrl` control points by `intensity_amp` (`jitter` intensity).
    for m in 0..n_ctrl {
        // Irregular angular spacing so jittered vertices are never evenly spaced like gear teeth
        let ang_jitter = if j > 0.02 {
            rng.range(-0.32, 0.32) * j.min(1.2) * d_theta
        } else {
            0.0
        };
        let theta = ((m as f64) * d_theta + ang_jitter).rem_euclid(TAU);
        let (ct, st) = (theta.cos(), theta.sin());

        let base_factor = eval_bough_profile(theta);

        let disp = if j > 0.02 {
            // Category A: Period-scaled 2D Perlin FBM with flat-crested outward / sharp inward shaping
            let n_p = perlin.fbm(period_orig.0 + rho_period * ct, period_orig.1 + rho_period * st, 2);
            let n_d = perlin.noise(detail_orig.0 + rho_detail * ct, detail_orig.1 + rho_detail * st);
            let combined_noise = 0.72 * n_p + 0.28 * n_d;
            let shaped_noise = if combined_noise >= 0.0 {
                // Chisel/flatten positive peaks so they never puff into round soap bubbles
                combined_noise / (1.0 + 0.65 * combined_noise)
            } else {
                // Deepen inward concavities into crisp foliage notches
                1.42 * combined_noise
            };

            // Category B: Discrete per-point ridge displacement (consecutive vertex contrast)
            let raw_kick = rng.range(-1.0, 1.0);
            let ridge = 0.68 * raw_kick - 0.32 * prev_kick;
            prev_kick = raw_kick;
            let shaped_kick = if ridge >= 0.0 {
                ridge * 0.75
            } else {
                ridge * 1.28
            };

            intensity_amp * (0.56 * shaped_noise + 0.44 * shaped_kick)
        } else {
            0.0
        };

        let factor = (base_factor + disp).clamp(0.28, 1.72);
        let r_m = (r_mean * factor).max(0.08);

        // Tangential vertex displacement breaks radial wheel symmetry so leaf tufts lean & hook
        let tan_shift = if j > 0.02 {
            let tan_amp = (0.15 * j.min(1.5) * eff_period.clamp(0.45, 1.35)).min(0.28);
            r_mean * tan_amp * rng.range(-1.0, 1.0) * d_theta.min(0.45)
        } else {
            0.0
        };

        let px = center.0 + r_m * ct - tan_shift * st;
        let py = center.1 + r_m * st + tan_shift * ct;
        ctrl_pts.push((px, py));
        ctrl_r.push(r_m);
    }

    // 4. Connect the `n_ctrl` displaced control points with a single tight asymmetric bevel pass:
    //    - Inward notches stay sharp (`0.08 / 0.92`)
    //    - Outward vertices get a crisp 2-segment botanical facet (`0.22 / 0.78`), NEVER a round bubble!
    let mut ring: Vec<Pt> = if j <= 0.02 {
        ctrl_pts
    } else {
        let mut beveled = Vec::with_capacity(n_ctrl * 2);
        for m in 0..n_ctrl {
            let prev_p = ctrl_pts[(m + n_ctrl - 1) % n_ctrl];
            let curr_p = ctrl_pts[m];
            let next_p = ctrl_pts[(m + 1) % n_ctrl];

            let prev_r = ctrl_r[(m + n_ctrl - 1) % n_ctrl];
            let curr_r = ctrl_r[m];
            let next_r = ctrl_r[(m + 1) % n_ctrl];

            let is_inward_notch = curr_r < 0.5 * (prev_r + next_r);
            let cut = if is_inward_notch { 0.08 } else { 0.22 };

            let q1 = (
                curr_p.0 * (1.0 - cut) + prev_p.0 * cut,
                curr_p.1 * (1.0 - cut) + prev_p.1 * cut,
            );
            let q2 = (
                curr_p.0 * (1.0 - cut) + next_p.0 * cut,
                curr_p.1 * (1.0 - cut) + next_p.1 * cut,
            );
            beveled.push(q1);
            beveled.push(q2);
        }
        beveled
    };

    // 5. Clamp every vertex to `clamp_region` (when `EdgeMode::Inside` or `Overflow` is active)
    //    and populate the 144-bin polar `lut` from the final polygon so `SpatialOccluder` matches.
    if let Some(clamp) = clamp_region {
        for pt in &mut ring {
            let dx = pt.0 - center.0;
            let dy = pt.1 - center.1;
            let dist = dx.hypot(dy);
            if dist > 1e-6 {
                let dir = (dx / dist, dy / dist);
                let clamped_d = clamp_ray_to_region(clamp, center, dir, dist);
                if clamped_d < dist {
                    *pt = (center.0 + clamped_d * dir.0, center.1 + clamped_d * dir.1);
                }
            }
        }
    }

    // Populate polar `lut[0..LUT_BINS]` from the polygon vertices
    let mut lut = [r_mean; LUT_BINS];
    let mut bin_sum = [0.0f64; LUT_BINS];
    let mut bin_cnt = [0u16; LUT_BINS];
    let n_ring = ring.len();
    for idx in 0..n_ring {
        let p0 = ring[idx];
        let p1 = ring[(idx + 1) % n_ring];
        for sub in 0..4 {
            let t = (sub as f64) * 0.25;
            let px = p0.0 + t * (p1.0 - p0.0) - center.0;
            let py = p0.1 + t * (p1.1 - p0.1) - center.1;
            let dist = px.hypot(py);
            let theta = py.atan2(px).rem_euclid(TAU);
            let bin = ((theta / TAU) * (LUT_BINS as f64)).floor() as usize % LUT_BINS;
            bin_sum[bin] += dist;
            bin_cnt[bin] += 1;
        }
    }

    // Fill any empty bins by interpolating between occupied neighbors
    for i in 0..LUT_BINS {
        if bin_cnt[i] > 0 {
            lut[i] = bin_sum[i] / (bin_cnt[i] as f64);
        }
    }
    for i in 0..LUT_BINS {
        if bin_cnt[i] == 0 {
            let mut prev_d = r_mean;
            let mut next_d = r_mean;
            let mut d_prev = 1usize;
            let mut d_next = 1usize;
            for step in 1..LUT_BINS {
                let bi = (i + LUT_BINS - step) % LUT_BINS;
                if bin_cnt[bi] > 0 {
                    prev_d = lut[bi];
                    d_prev = step;
                    break;
                }
            }
            for step in 1..LUT_BINS {
                let bi = (i + step) % LUT_BINS;
                if bin_cnt[bi] > 0 {
                    next_d = lut[bi];
                    d_next = step;
                    break;
                }
            }
            let w = (d_prev as f64) / ((d_prev + d_next) as f64);
            lut[i] = prev_d * (1.0 - w) + next_d * w;
        }
    }

    let mut min_r = f64::MAX;
    let mut max_r = 0.0f64;
    for (i, r_val) in lut.iter_mut().enumerate().take(LUT_BINS) {
        if let Some(clamp) = clamp_region {
            let theta = TAU * (i as f64) / (LUT_BINS as f64);
            *r_val = clamp_ray_to_region(clamp, center, (theta.cos(), theta.sin()), *r_val);
        }
        min_r = min_r.min(*r_val);
        max_r = max_r.max(*r_val);
    }

    JaggedLayer {
        tree_id,
        center,
        z_occlude,
        is_base_footprint,
        is_open_arc,
        is_sub_bough,
        arc_window,
        u_norm,
        lut,
        min_r,
        max_r,
        ring,
    }
}

/// Render all `JaggedLayer`s using the $O(1)$ `SpatialOccluder` lookup grid.
fn render_layers_fast(
    layers: &[JaggedLayer],
    p: &IsolineStackParams,
    light: Option<(Pt, f64)>,
) -> Strokes {
    let occluder = SpatialOccluder::new(layers, (p.crown * 1.8).max(1.5));
    // Slight overlap slack (`0.015 * crown`): allows lines to meet cleanly
    // at upper layer boundaries without boolean polygon clipping overhead.
    let overlap_slack = (p.crown * 0.015).clamp(0.0, 0.12);

    let mut strokes: Strokes = Vec::with_capacity(layers.len() * 2);
    let mut vis: Vec<bool> = Vec::with_capacity(256);

    for (idx, layer) in layers.iter().enumerate() {
        let n = layer.ring.len();
        if n < 3 {
            continue;
        }

        // Determine which vertices around the ring are visible (not hidden by a higher or
        // fused same-level layer, not outside an open-arc window, and not in a sunlit gap).
        let is_vertex_visible = |i: usize, pt: Pt| -> bool {
            let dx = pt.0 - layer.center.0;
            let dy = pt.1 - layer.center.1;
            let theta = dy.atan2(dx).rem_euclid(TAU);

            // 1. Open crescent arc window on inner bough layers
            if layer.is_open_arc {
                let (win_center, half_span) = layer.arc_window;
                if angle_diff(theta, win_center).abs() > half_span {
                    return false;
                }
            }

            // 2. Directional sunlight highlight gap on inner layers (`!is_base_footprint`)
            //    and optional subtle sunlit rim break on the sun-facing tip of outer skirts
            //    when `strength > 0.50` (note: `out.coverage` is built from `layer.ring`
            //    BEFORE `render_layers_fast`, so the exported mask polygon is always 100% closed!).
            if let Some(((lx, ly), strength)) = light {
                if strength > 0.12 {
                    let len = dx.hypot(dy);
                    if len > 1e-6 {
                        // Blend radial direction with local contour outward normal
                        let prev_pt = layer.ring[(i + n - 1) % n];
                        let next_pt = layer.ring[(i + 1) % n];
                        let tx = next_pt.0 - prev_pt.0;
                        let ty = next_pt.1 - prev_pt.1;
                        let t_len = tx.hypot(ty).max(1e-6);
                        let nx = 0.65 * (dx / len) + 0.35 * (ty / t_len);
                        let ny = 0.65 * (dy / len) + 0.35 * (-tx / t_len);
                        let facing_sun = nx * lx + ny * ly;

                        if !layer.is_base_footprint {
                            let thresh = (0.66 - 0.62 * strength * (0.35 + 0.65 * layer.u_norm)).clamp(0.02, 0.80);
                            if facing_sun > thresh {
                                return false;
                            }
                        } else if p.jitter > 0.05 && strength > 0.50 && (layer.tree_id % 3 != 0) {
                            let rim_thresh = (1.14 - 0.34 * strength).clamp(0.76, 0.98);
                            if facing_sun > rim_thresh {
                                return false;
                            }
                        }
                    }
                }
            }

            // 3. Fast $O(1)$ analytical 3D occlusion & same-level fusion check
            !occluder.is_occluded(pt, idx, layers, overlap_slack)
        };

        vis.clear();
        vis.resize(n, false);
        let mut all_vis = true;
        let mut any_vis = false;
        for (i, vis_slot) in vis.iter_mut().enumerate().take(n) {
            *vis_slot = is_vertex_visible(i, layer.ring[i]);
            all_vis &= *vis_slot;
            any_vis |= *vis_slot;
        }

        if !any_vis {
            continue;
        }

        if all_vis {
            let mut closed = layer.ring.clone();
            closed.push(layer.ring[0]);
            strokes.push(closed);
            continue;
        }

        // Extract continuous visible arcs, bisecting the exact boundary crossing point
        // where a segment enters/exits occlusion so stroke ends sit crisply on the upper contour!
        let start_invisible = (0..n).find(|&i| !vis[i]).unwrap_or(0);
        let mut run: Vec<Pt> = Vec::new();

        for step in 0..=n {
            let i = (start_invisible + step) % n;
            let prev_i = (i + n - 1) % n;
            let cur_p = layer.ring[i];
            let prev_p = layer.ring[prev_i];

            if vis[i] {
                if step > 0 && !vis[prev_i] {
                    // Transition from occluded -> visible: bisect crossing point
                    let cross = bisect_visibility(prev_p, cur_p, |q| {
                        !occluder.is_occluded(q, idx, layers, overlap_slack)
                    });
                    run.push(cross);
                }
                run.push(cur_p);
            } else if !run.is_empty() {
                // Transition from visible -> occluded: bisect crossing point and flush run
                let cross = bisect_visibility(prev_p, cur_p, |q| {
                    occluder.is_occluded(q, idx, layers, overlap_slack)
                });
                run.push(cross);
                if run.len() >= 2 {
                    strokes.push(std::mem::take(&mut run));
                } else {
                    run.clear();
                }
            }
        }
        if run.len() >= 2 {
            strokes.push(run);
        }
    }

    // Optional perpendicular slope-tick / cleft-crease hachure strokes (`p.ticks` or `Light.hatch`).
    let hatch_density = p
        .light
        .map(|l| if l.hatch > 0.01 { (0.45 / l.hatch).clamp(0.25, 1.2) } else { 0.0 })
        .unwrap_or(0.0);
    let eff_ticks = p.ticks.max(hatch_density).clamp(0.0, 1.5);
    if eff_ticks > 0.01 {
        append_perpendicular_ticks(&mut strokes, layers, &occluder, eff_ticks, light, overlap_slack);
    }

    strokes
}

/// Emit short perpendicular slope-tick strokes (`hachures`) that bridge across the gap between
/// consecutive elevation contours or step inward from bough clefts and shadowed
/// flanks. Because they terminate before touching the adjacent contour, `deoverlap_strokes`
/// preserves them cleanly without creating messy T-junction blobs on a pen plotter.
fn append_perpendicular_ticks(
    strokes: &mut Strokes,
    layers: &[JaggedLayer],
    occluder: &SpatialOccluder,
    tick_density: f64,
    light: Option<(Pt, f64)>,
    overlap_slack: f64,
) {
    for (idx, layer) in layers.iter().enumerate() {
        if layer.is_sub_bough || layer.u_norm > 0.82 {
            continue;
        }
        let n = layer.ring.len();
        if n < 6 {
            continue;
        }

        // Find the next higher main contour layer belonging to the same tree (if any)
        let next_layer = layers
            .iter()
            .skip(idx + 1)
            .find(|other| other.tree_id == layer.tree_id && !other.is_sub_bough && other.z_occlude > layer.z_occlude);

        for i in 0..n {
            let prev_p = layer.ring[(i + n - 1) % n];
            let curr_p = layer.ring[i];
            let next_p = layer.ring[(i + 1) % n];

            let dx = curr_p.0 - layer.center.0;
            let dy = curr_p.1 - layer.center.1;
            let r_curr = dx.hypot(dy);
            if r_curr < 0.25 {
                continue;
            }
            let r_prev = (prev_p.0 - layer.center.0).hypot(prev_p.1 - layer.center.1);
            let r_next = (next_p.0 - layer.center.0).hypot(next_p.1 - layer.center.1);

            let is_cleft_cusp = r_curr < 0.5 * (r_prev + r_next) - 0.015 * layer.max_r;
            let theta = dy.atan2(dx).rem_euclid(TAU);

            if layer.is_open_arc {
                let (win_center, half_span) = layer.arc_window;
                if angle_diff(theta, win_center).abs() > half_span * 0.92 {
                    continue;
                }
            }

            // Favor shadowed flanks and inward bough clefts; suppress on bright sunlit tips
            let shadow_weight = if let Some(((lx, ly), strength)) = light {
                let facing_sun = (dx * lx + dy * ly) / r_curr;
                if facing_sun > 0.35 {
                    continue;
                }
                (0.55 - 0.65 * facing_sun * strength).clamp(0.15, 1.25)
            } else {
                0.70
            };

            // Deterministic per-vertex hash for reproducible placement
            let h = ((layer.tree_id as u64).wrapping_mul(0x9E3779B97F4A7C15))
                ^ ((idx as u64).wrapping_mul(0xBF58476D1CE4E5B9))
                ^ ((i as u64).wrapping_mul(0x94D049BB133111EB));
            let u_rand = ((h >> 11) as f64) / ((1u64 << 53) as f64);

            let prob = if is_cleft_cusp {
                0.52 * tick_density * shadow_weight
            } else {
                0.14 * tick_density * shadow_weight
            };
            if u_rand > prob {
                continue;
            }

            // Inward normal perpendicular to the local contour tangent (blended with radial inward)
            let tx = next_p.0 - prev_p.0;
            let ty = next_p.1 - prev_p.1;
            let t_len = tx.hypot(ty).max(1e-6);
            let in_nx = -0.60 * (dx / r_curr) - 0.40 * (ty / t_len);
            let in_ny = -0.60 * (dy / r_curr) + 0.40 * (tx / t_len);
            let in_len = in_nx.hypot(in_ny).max(1e-6);
            let (ux, uy) = (in_nx / in_len, in_ny / in_len);

            // Target tick span: bridge ~68% of the radial step to the next inner contour
            let gap = if let Some(upper) = next_layer {
                let bin = ((theta / TAU) * (LUT_BINS as f64)).floor() as usize % LUT_BINS;
                let r_up = upper.lut[bin];
                let lo = 0.06;
                let hi = (layer.max_r * 0.34).max(lo);
                (r_curr - r_up).clamp(lo, hi)
            } else {
                let lo = 0.06;
                let hi = 0.90;
                (layer.max_r * 0.18).clamp(lo, hi)
            };

            let start_pad = (gap * 0.16).clamp(0.06, 0.16);
            let end_dist = (gap * if is_cleft_cusp { 0.82 } else { 0.68 }).max(start_pad + 0.12);

            let p0 = (curr_p.0 + ux * start_pad, curr_p.1 + uy * start_pad);
            let pm = (
                curr_p.0 + ux * (0.5 * (start_pad + end_dist)) - uy * 0.04 * gap,
                curr_p.1 + uy * (0.5 * (start_pad + end_dist)) + ux * 0.04 * gap,
            );
            let p1 = (curr_p.0 + ux * end_dist, curr_p.1 + uy * end_dist);

            if occluder.is_occluded(p0, idx, layers, overlap_slack)
                || occluder.is_occluded(p1, idx, layers, overlap_slack)
            {
                continue;
            }

            strokes.push(vec![p0, pm, p1]);
        }
    }
}

#[inline]
fn bisect_visibility(a: Pt, b: Pt, is_target_state: impl Fn(Pt) -> bool) -> Pt {
    let mut lo = a;
    let mut hi = b;
    for _ in 0..4 {
        let mid = (0.5 * (lo.0 + hi.0), 0.5 * (lo.1 + hi.1));
        if is_target_state(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    (0.5 * (lo.0 + hi.0), 0.5 * (lo.1 + hi.1))
}

/// Fast spatial-hash de-overlap pass: drops vertex runs that lie closer than
/// `min_clearance` to an already-accepted stroke so overlapping layers never
/// produce heavy double-inked lines on a pen plotter.
fn deoverlap_strokes(strokes: Strokes, min_clearance: f64) -> Strokes {
    if min_clearance <= 0.0 || strokes.is_empty() {
        return strokes;
    }

    let cell = (min_clearance * 3.0).max(0.25);
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for p in strokes.iter().flatten() {
        min_x = min_x.min(p.0);
        min_y = min_y.min(p.1);
        max_x = max_x.max(p.0);
        max_y = max_y.max(p.1);
    }
    if min_x >= max_x || min_y >= max_y {
        return strokes;
    }

    let cols = (((max_x - min_x) / cell).ceil() as usize + 2).clamp(1, 240);
    let rows = (((max_y - min_y) / cell).ceil() as usize + 2).clamp(1, 240);
    let cs_x = ((max_x - min_x) / (cols.saturating_sub(1).max(1) as f64)).max(cell);
    let cs_y = ((max_y - min_y) / (rows.saturating_sub(1).max(1) as f64)).max(cell);
    let cs = cs_x.max(cs_y);

    let mut grid: Vec<Vec<Pt>> = vec![Vec::new(); cols * rows];
    let slot = |p: Pt| -> (usize, usize) {
        let c = (((p.0 - min_x) / cs).floor() as isize).clamp(0, cols as isize - 1) as usize;
        let r = (((p.1 - min_y) / cs).floor() as isize).clamp(0, rows as isize - 1) as usize;
        (c, r)
    };

    let min_d2 = min_clearance * min_clearance;
    let is_too_close = |p: Pt, grid: &[Vec<Pt>]| -> bool {
        let (c, r) = slot(p);
        for rr in r.saturating_sub(1)..=(r + 1).min(rows - 1) {
            for cc in c.saturating_sub(1)..=(c + 1).min(cols - 1) {
                for &q in &grid[rr * cols + cc] {
                    if (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2) < min_d2 {
                        return true;
                    }
                }
            }
        }
        false
    };

    let mut out = Vec::with_capacity(strokes.len());
    let mut accepted_pts: Vec<Pt> = Vec::with_capacity(64);

    for stroke in strokes {
        accepted_pts.clear();
        let mut run = Vec::new();
        for &p in &stroke {
            if !is_too_close(p, &grid) {
                run.push(p);
                accepted_pts.push(p);
            } else if run.len() >= 2 {
                out.push(std::mem::take(&mut run));
            } else {
                run.clear();
            }
        }
        if run.len() >= 2 {
            out.push(run);
        }
        // Register all points of this stroke into the spatial grid for subsequent strokes
        for &p in &accepted_pts {
            let (c, r) = slot(p);
            grid[r * cols + c].push(p);
        }
    }

    out
}

fn clamp_ray_to_region(clamp_region: &Region, origin: Pt, dir: Pt, r: f64) -> f64 {
    let target = (origin.0 + r * dir.0, origin.1 + r * dir.1);
    if clamp_region.covers_point(target) {
        return r;
    }
    let mut lo = 0.0;
    let mut hi = r;
    for _ in 0..8 {
        let mid = 0.5 * (lo + hi);
        let p = (origin.0 + mid * dir.0, origin.1 + mid * dir.1);
        if clamp_region.covers_point(p) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo * 0.96
}

fn angle_diff(a: f64, b: f64) -> f64 {
    let mut d = (a - b) % TAU;
    if d > std::f64::consts::PI {
        d -= TAU;
    } else if d < -std::f64::consts::PI {
        d += TAU;
    }
    d
}

fn polyline_len(pts: &[Pt]) -> f64 {
    pts.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum()
}

fn order_strokes_for_plotter(mut strokes: Strokes) -> Strokes {
    if strokes.len() <= 2 {
        return strokes;
    }
    let mut ordered = Vec::with_capacity(strokes.len());
    ordered.push(strokes.swap_remove(0));

    while !strokes.is_empty() {
        let cur_end = *ordered.last().unwrap().last().unwrap();
        let mut best_idx = 0;
        let mut best_dist2 = f64::MAX;
        let mut best_rev = false;

        for (i, s) in strokes.iter().enumerate() {
            let a = s[0];
            let b = *s.last().unwrap();
            let da2 = (a.0 - cur_end.0).powi(2) + (a.1 - cur_end.1).powi(2);
            let db2 = (b.0 - cur_end.0).powi(2) + (b.1 - cur_end.1).powi(2);
            if da2 < best_dist2 {
                best_dist2 = da2;
                best_idx = i;
                best_rev = false;
            }
            if db2 < best_dist2 {
                best_dist2 = db2;
                best_idx = i;
                best_rev = true;
            }
        }
        let mut next = strokes.swap_remove(best_idx);
        if best_rev {
            next.reverse();
        }
        ordered.push(next);
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (60.0, 0.0), (60.0, 60.0), (0.0, 60.0)])
    }

    fn params(edge: EdgeMode) -> IsolineStackParams {
        IsolineStackParams { seed: 3, crown: 6.0, bubble: 2.0, coverage: 0.8, slices: 8, edge, ..Default::default() }
    }

    #[test]
    fn draws_inside_the_region() {
        let host = square();
        let out = fill_isoline_stack_with_coverage(&host, &params(EdgeMode::Clip));
        assert!(out.strokes.len() > 20, "only {} strokes", out.strokes.len());
        for p in out.strokes.iter().flatten() {
            assert!(host.covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn inside_mode_never_touches_the_boundary() {
        let host = square();
        let out = fill_isoline_stack_with_coverage(&host, &params(EdgeMode::Inside));
        assert!(!out.strokes.is_empty());
        for p in out.strokes.iter().flatten() {
            assert!(host.dist_to_boundary(*p) > 0.05, "{p:?} reaches the boundary");
        }
    }

    #[test]
    fn overflow_mode_crosses_the_boundary_but_not_far() {
        let host = square();
        let m = 1.5;
        let out = fill_isoline_stack_with_coverage(&host, &params(EdgeMode::Overflow(m)));
        let outside: Vec<&Pt> = out.strokes.iter().flatten().filter(|p| !host.covers_point(**p)).collect();
        assert!(!outside.is_empty(), "nothing bulges past the border");
        for p in outside {
            assert!(host.dist_to_boundary(*p) <= m + 1e-6, "{p:?} too far out");
        }
    }

    #[test]
    fn light_makes_the_shadow_side_denser_than_the_lit_side() {
        let crowns = vec![Crown { at: (30.0, 30.0), radius: 8.0, height: 6.0 }];
        let lit = IsolineStackParams {
            crowns: CrownSource::Crowns(crowns),
            light: Some(Light { from_deg: 0.0, strength: 0.9, hatch: 0.0, ..Light::default() }),
            jitter: 0.0,
            ..params(EdgeMode::Clip)
        };
        let out = fill_isoline_stack_with_coverage(&square(), &lit);
        let ink_past = |x_min: f64, x_max: f64| -> f64 {
            out.strokes
                .iter()
                .flat_map(|s| s.windows(2))
                .filter(|w| w[0].0 >= x_min && w[0].0 < x_max && w[1].0 >= x_min && w[1].0 < x_max)
                .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
                .sum()
        };
        let shadow = ink_past(0.0, 24.0);
        let sunlit = ink_past(36.0, 60.0);
        assert!(shadow > sunlit * 1.3, "shadow {shadow:.1} should clearly outweigh the lit side {sunlit:.1}");
    }

    #[test]
    fn lit_side_has_more_gaps_in_the_rings() {
        let crowns = vec![Crown { at: (30.0, 30.0), radius: 10.0, height: 8.0 }];
        let base = IsolineStackParams { crowns: CrownSource::Crowns(crowns), jitter: 0.0, ..params(EdgeMode::Clip) };
        let lit = IsolineStackParams { light: Some(Light { from_deg: 0.0, strength: 0.9, ..Light::default() }), ..base };
        let strokes = fill_isoline_stack_with_coverage(&square(), &lit).strokes;
        let ink = |x0: f64, x1: f64| {
            strokes
                .iter()
                .flat_map(|s| s.windows(2))
                .filter(|w| {
                    let x = (w[0].0 + w[1].0) * 0.5;
                    x >= x0 && x <= x1
                })
                .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
                .sum::<f64>()
        };
        assert!(ink(36.0, 60.0) < ink(0.0, 24.0) * 0.85, "lit side should read sparser than shadow");
    }

    #[test]
    fn a_higher_wider_bubble_hides_the_rings_under_it() {
        let crowns = vec![
            Crown { at: (25.0, 30.0), radius: 10.0, height: 3.0 },
            Crown { at: (35.0, 30.0), radius: 6.0, height: 14.0 },
        ];
        let p = IsolineStackParams { crowns: CrownSource::Crowns(crowns), jitter: 0.0, ..params(EdgeMode::Clip) };
        let out = fill_isoline_stack_with_coverage(&square(), &p);
        let core = Region::from_ring(&[(34.0, 29.0), (36.0, 29.0), (36.0, 31.0), (34.0, 31.0)]);
        let inside_core = out.strokes.iter().flatten().filter(|q| core.covers_point(**q)).count();
        assert!(out.strokes.len() > 5);
        assert!(inside_core < 40, "{inside_core} points under the top bubble");
    }

    #[test]
    fn custom_params_control_jaggedness_and_coverage() {
        let host = square();
        let smooth = FillParams::new(6.0).seed(5).custom("jaggedness", 0.1).custom("coverage", 0.5);
        let jagged = FillParams::new(6.0).seed(5).custom("jaggedness", 1.8).custom("coverage", 0.5);
        let fine_period = FillParams::new(6.0)
            .seed(5)
            .custom("jitter_intensity", 1.0)
            .custom("jitter_period", 0.40)
            .custom("coverage", 0.5);
        let broad_period = FillParams::new(6.0)
            .seed(5)
            .custom("jitter_intensity", 1.0)
            .custom("jitter_period", 2.0)
            .custom("coverage", 0.5);
        let out_s = fill_isoline_stack_params(&host, &smooth);
        let out_j = fill_isoline_stack_params(&host, &jagged);
        let out_fine = fill_isoline_stack_params(&host, &fine_period);
        let out_broad = fill_isoline_stack_params(&host, &broad_period);
        assert!(!out_s.strokes.is_empty());
        assert!(!out_j.strokes.is_empty());
        assert_ne!(out_s.strokes, out_j.strokes);
        assert_ne!(out_fine.strokes, out_broad.strokes);
        // Smaller period jitters more points along each contour -> more vertices overall.
        let pts_fine: usize = out_fine.strokes.iter().map(|s| s.len()).sum();
        let pts_broad: usize = out_broad.strokes.iter().map(|s| s.len()).sum();
        assert!(
            pts_fine > pts_broad,
            "fine period ({pts_fine} pts) should jitter more points than broad period ({pts_broad} pts)"
        );
    }

    #[test]
    fn coverage_mask_is_closed_and_covers_crowns_despite_open_sunlit_rims() {
        let host = square();
        let crowns = vec![Crown { at: (30.0, 30.0), radius: 10.0, height: 8.0 }];
        let p = IsolineStackParams {
            crowns: CrownSource::Crowns(crowns),
            jitter: 0.9,
            light: Some(Light { from_deg: -45.0, strength: 0.9, ..Light::default() }),
            ..params(EdgeMode::Clip)
        };
        let out = fill_isoline_stack_with_coverage(&host, &p);
        assert!(!out.coverage.is_empty(), "coverage mask must be non-empty");
        assert!(out.coverage.covers_point((30.0, 30.0)), "coverage mask must enclose crown interior");
        for ring in out.coverage.all_rings_mm() {
            assert!(ring.len() >= 3, "every mask ring must be a valid closed polygon contour");
        }
    }
}
