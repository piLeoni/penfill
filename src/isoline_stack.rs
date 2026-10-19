//! Layered forest & vegetation canopy contours seen from above.
//!
//! Models each tree crown as a 3D cluster of overlapping, jagged foliage masses
//! (`fronde` / sub-boughs) rather than smooth concentric target rings or simple
//! parallel curve offsets:
//! 1. **Jagged, concave-convex internal tiers** — every contour (from the outer
//!    canopy skirt to the innermost upper bough tufts) maintains leaf-scale
//!    jaggedness (`jitter` / `jaggedness`), deep inward concavities (bough
//!    clefts), and outward convexities with **independent angular phases,
//!    centers, and clefts** at each height level.
//! 2. **Overlapping 3D sub-boughs & broken crescent fronde** — inside the outer
//!    skirt, 2–5 offset sub-bough puffs step upward toward the crown peak; upper
//!    puffs cut across lower puffs and inner contours open into jagged C-arcs
//!    and scalloped crescents.
//! 3. **Gradual stochastic edge scattering** — between dense forest groves and
//!    open clearings, tree probability and crown size taper smoothly through a
//!    scattering transition zone of medium trees, shrubs, and isolated bushes.
//! 4. **Fast analytic LUT occlusion + spatial de-overlap** — instead of running
//!    dozens of expensive polygon boolean unions (`i_overlay`) across every
//!    slice, each 3D foliage mass precomputes a 72-bin polar radius lookup table
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
    /// 0 – 2+: jaggedness (`frastagliatezza`) of outer and inner contours.
    /// - `0.0`: smooth rounded lobes
    /// - `0.85`: botanical oak/deciduous scallops & deep bough concavities
    /// - `1.5+`: sharply serrated, spiky foliage contours
    pub jitter: f64,
    /// 0 – 1: crown aspect-ratio elongation / wind-swept ellipticity (`0.15` default).
    pub elongation: f64,
    /// 0 – 2+: prominence of deep bough clefts and compound sub-boughs (`1.0` default).
    pub lobes: f64,
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
            jitter: 0.88,
            elongation: 0.15,
            lobes: 1.0,
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
    /// - `"jaggedness"` or `"jitter"`: contour jaggedness (`0.0..=2.2`, default `0.88`)
    /// - `"coverage"`: forest coverage (`0.0..=1.0`, default `0.6`)
    /// - `"slices"`: vertical contour layers (`2..=18`, default `9`)
    /// - `"bubble"`: leaf-tuft scale in mm
    /// - `"elongation"`: crown elliptical elongation (`0.0..=1.0`, default `0.15`)
    /// - `"lobes"`: bough cleft depth & compound sub-crown lobing (`0.0..=2.5`, default `1.0`)
    /// - `"light"`: directional light strength (`0.0..=1.0`, default `0.82`; `0.0` = off)
    /// - `"light_angle"`: directional light angle in degrees (default `-45.0`)
    pub fn from_fill_params(params: &FillParams, default_crown: f64) -> Self {
        let crown = params.custom.get_or("crown", default_crown).max(0.6);
        let bubble = params.custom.get_or("bubble", (crown * 0.28).max(0.30));
        let coverage = params.custom.get_or("coverage", 0.60).clamp(0.0, 1.0);
        let slices = params.custom.get_or("slices", 9.0).round().clamp(2.0, 18.0) as usize;
        let jitter = params
            .custom
            .get("jaggedness")
            .or_else(|| params.custom.get("jitter"))
            .unwrap_or(0.88);
        let elongation = params.custom.get_or("elongation", 0.15).clamp(0.0, 1.0);
        let lobes = params.custom.get_or("lobes", 1.0).clamp(0.0, 2.5);
        let light_strength = params.custom.get_or("light", 0.82).clamp(0.0, 1.0);
        let light = (light_strength > 0.01).then(|| Light {
            from_deg: params.custom.get_or("light_angle", -45.0),
            strength: light_strength,
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
            elongation,
            lobes,
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

/// One 3D jagged contour layer belonging either to a tree's outer skirt or to
/// one of its internal overlapping sub-bough puffs (`fronde`).
#[derive(Clone, Debug)]
struct JaggedLayer {
    /// Tree id this layer belongs to.
    tree_id: usize,
    /// Center of this specific contour layer.
    center: Pt,
    /// 3D elevation of this layer (higher occludes lower).
    z_occlude: f64,
    /// Whether this layer is the base footprint of a tree/bough (contributes to `coverage` mask).
    is_base_footprint: bool,
    /// Whether this layer is an open-interior foliage puff (draws only its rim, occluding inside).
    is_open_arc: bool,
    /// Angular window `(center_angle, half_span)` when `is_open_arc` is true.
    arc_window: (f64, f64),
    /// Normalized height `u in [0, 1]` within its tree.
    u_norm: f64,
    /// Precomputed polar radius at `LUT_BINS` evenly spaced angles in `[0, TAU)`
    /// for $O(1)$ analytical occlusion testing without polygon booleans.
    lut: [f64; LUT_BINS],
    min_r: f64,
    max_r: f64,
    /// Smooth contour vertices in mm.
    ring: Vec<Pt>,
}

impl JaggedLayer {
    /// Fast $O(1)$ analytical test: does this layer cover point `p` (shrunk by `margin`)?
    #[inline]
    fn covers(&self, p: Pt, margin: f64) -> bool {
        let dx = p.0 - self.center.0;
        let dy = p.1 - self.center.1;
        let d2 = dx * dx + dy * dy;
        let max_eff = (self.max_r - margin).max(0.0);
        if d2 >= max_eff * max_eff {
            return false;
        }
        if !self.is_open_arc {
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
        if self.is_open_arc {
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
            if other.z_occlude <= self_layer.z_occlude + 1e-6 {
                continue;
            }
            if other.tree_id == self_layer.tree_id {
                // Within the same tree, an upper closed contour layer only clips a lower layer
                // where a convex lobe of the upper layer genuinely overhangs a concave bay of
                // the lower layer; `deoverlap_strokes` cleans up any remaining close contacts.
                if !other.is_open_arc && other.covers(p, 0.03 * self_layer.max_r) {
                    return true;
                }
                continue;
            }
            // Different tree: higher tree occludes lower neighboring tree
            if other.covers(p, overlap_slack) {
                return true;
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
            p.bubble,
            p.elongation,
            p.lobes,
            light,
            clamp_region,
            perlin,
            rng,
            false,
        );
    }
    layers
}

/// Generate a forest with gradual stochastic scattering between dense groves and clearings.
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
            let min_r = (r0 * 0.32).max(0.22);
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
    // Candidate spacing allows natural crown overlap in grove cores without overcrowding.
    let step = (r0 * (1.42 - 0.30 * cov)).max(0.34);
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
    let min_ring_step = (r0 * 0.115).clamp(0.24, 0.48);
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
        let size_factor = if local_density > 0.68 {
            rng.range(0.78, 1.28)
        } else if local_density > 0.35 {
            if rng.unit() < 0.45 {
                rng.range(0.62, 0.96)
            } else {
                rng.range(0.36, 0.64)
            }
        } else if rng.unit() < 0.28 {
            rng.range(0.50, 0.78)
        } else {
            rng.range(0.25, 0.48)
        };

        let mut r_tree = r0 * size_factor;
        if let Some(clamp) = clamp_region {
            if matches!(p.edge, EdgeMode::Inside) {
                r_tree = r_tree.min(clamp.dist_to_boundary(center) * 0.92);
            }
        }
        if r_tree < (r0 * 0.14).max(0.16) {
            continue;
        }

        let max_rings = ((r_tree * 0.92) / min_ring_step).floor() as usize;
        let target_rings = ((base_slices as f64) * (r_tree / r0).powf(0.75)).round() as usize;
        let rings = target_rings.clamp(3, max_rings.max(3)).min(12);

        let z_base = r0 * rng.range(0.0, 0.65) * local_density;
        let z_top = z_base + r_tree * rng.range(1.1, 1.85) + r0 * 0.45 * local_density;

        let is_multi_bough = r_tree >= r0 * 0.55 && p.jitter > 0.05 && p.lobes > 0.35;
        append_tree_layers(
            &mut layers,
            tree_id,
            center,
            r_tree,
            z_base,
            z_top,
            rings,
            p.jitter,
            p.bubble,
            p.elongation,
            p.lobes,
            light,
            clamp_region,
            perlin,
            rng,
            is_multi_bough,
        );
        tree_id += 1;

        // Spawn detached satellite shrubs/trees primarily in the transition & clearing scatter
        // zones (`local_density < 0.75`) so groves transition gradually into open space!
        let n_satellites = if local_density < 0.75 && local_density > 0.22 {
            1 + (rng.next_u64() as usize % 2)
        } else if local_density >= 0.75 && rng.unit() < 0.35 {
            1
        } else {
            0
        };

        let base_ang = rng.range(0.0, TAU);
        for s_idx in 0..n_satellites {
            let ang = base_ang + (s_idx as f64) * 2.3 + rng.range(-0.65, 0.65);
            let dist_mult = rng.range(0.88, 1.65);
            let sat_c = (center.0 + r_tree * dist_mult * ang.cos(), center.1 + r_tree * dist_mult * ang.sin());
            if !seed_region.covers_point(sat_c) {
                continue;
            }
            let mut r_sat = r_tree * rng.range(0.32, 0.58);
            if let Some(clamp) = clamp_region {
                if matches!(p.edge, EdgeMode::Inside) {
                    r_sat = r_sat.min(clamp.dist_to_boundary(sat_c) * 0.90);
                }
            }
            if r_sat < (r0 * 0.14).max(0.16) {
                continue;
            }
            let sat_max_rings = ((r_sat * 0.90) / min_ring_step).floor() as usize;
            let sat_rings = sat_max_rings.clamp(2, 5);
            let sat_z_base = z_base * 0.6;
            let sat_z_top = sat_z_base + r_sat * rng.range(0.9, 1.5);

            append_tree_layers(
                &mut layers,
                tree_id,
                sat_c,
                r_sat,
                sat_z_base,
                sat_z_top,
                sat_rings,
                p.jitter * 1.05,
                p.bubble,
                p.elongation,
                p.lobes,
                light,
                clamp_region,
                perlin,
                rng,
                false,
            );
            tree_id += 1;
        }
    }

    layers
}

/// Build the 3D jagged layers of a single tree crown:
/// - Outer skirt layers wrap the whole crown with deep concave bough clefts and leaf scallops.
/// - Internal layers step all the way from `1.00 * radius` (`k = 0`) down to `0.10 * radius`
///   (`k = rings - 1`), shifting toward the shadow/aspect flank and opening on the sunlit flank
///   into cascading C-arcs so trees never look like concentric artichokes!
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
    bubble: f64,
    elongation: f64,
    lobes: f64,
    light: Option<(Pt, f64)>,
    clamp_region: Option<&Region>,
    perlin: &Perlin,
    rng: &mut Rng,
    multi_bough: bool,
) {
    let mut local_rng = Rng::new(rng.next_u64().wrapping_add((tree_id as u64).wrapping_mul(0x9E3779B97F4A7C15)));
    let j = jitter.clamp(0.0, 2.5);
    let elong = elongation.clamp(0.0, 1.0);
    let lobe_scale = lobes.clamp(0.0, 2.5);

    // Overall crown drift (eccentricity + light-driven shadow shift).
    // Even when `light` is low or `None`, organic crowns (`j > 0.05`) maintain a natural 3D
    // aspect tilt so inner tiers pack asymmetrically instead of forming concentric artichokes.
    let nat_ang = local_rng.range(0.0, TAU);
    let nat_mag = if j > 0.0 {
        radius * local_rng.range(0.14, 0.24) * j.min(1.0)
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
        drift_dx = drift_dx * (1.0 - blend) - lx * radius * 0.24 * eff_s;
        drift_dy = drift_dy * (1.0 - blend) - ly * radius * 0.24 * eff_s;
        shadow_dir = (-ly).atan2(-lx) + local_rng.range(-0.22, 0.22) * (1.0 - 0.65 * strength);
    }

    let tooth_mm = (bubble * 0.52).clamp(radius * 0.11, radius * 0.36).max(0.24);
    let base_oval_phase = local_rng.range(0.0, TAU);
    let base_oval_amp = if j > 0.0 || elong > 0.01 {
        (elong * 0.36 + local_rng.range(0.04, 0.11) * j.min(1.2)).clamp(0.0, 0.45)
    } else {
        0.0
    };
    let noise_base = (local_rng.range(-80.0, 80.0), local_rng.range(-80.0, 80.0));

    // Step 1: Full contour stack from outer skirt (`k = 0`, `u = 0.0`) all the way to the
    // inner crown apex (`k = rings - 1`, `u = 1.0`, `rad_scale = 0.10`)!
    let denom = (rings.saturating_sub(1).max(1)) as f64;
    for k in 0..rings {
        let u = if rings <= 1 { 0.0 } else { (k as f64) / denom };
        let rad_scale = (1.0 - 0.90 * u).max(0.09);
        let r_mean = radius * rad_scale;

        // Lateral wander at level `k` so successive layers shift across internal boughs
        let wander_ang = nat_ang + (k as f64) * 2.1 + local_rng.range(-0.5, 0.5);
        let wander_mag = if j > 0.0 && k > 0 {
            radius * 0.06 * (u.sqrt() * (1.0 - 0.50 * u)) * j.min(1.2)
        } else {
            0.0
        };
        let skirt_w = (1.0 - u).powi(2);
        let cx = center.0 + skirt_dx * skirt_w + drift_dx * u.powf(0.85) + wander_mag * wander_ang.cos();
        let cy = center.1 + skirt_dy * skirt_w + drift_dy * u.powf(0.85) + wander_mag * wander_ang.sin();

        let z_layer = z_base + (z_top - z_base) * (0.04 + 0.94 * u);

        // Whenever `j > 0.05` and `k >= 1`, inner tiers open on their sunlit/anti-aspect flank
        // into scalloped C-arcs so no tree ever renders as closed 360° concentric artichoke rings!
        let light_s = light.map(|(_, s)| s).unwrap_or(0.0);
        let (is_open_arc, arc_window) = if j > 0.05 && k >= 1 {
            let open_dir = shadow_dir + local_rng.range(-0.38, 0.38);
            // Lower luminosity -> wider C-arcs (`2.1..2.65 rad` half-span);
            // Higher luminosity -> crisper crescent C-arcs (`1.45..2.25 rad` half-span).
            let span_base = 2.45 - 0.58 * light_s - 0.35 * u;
            let half_span = (span_base + local_rng.range(-0.22, 0.22)).clamp(1.25, 2.68);
            (true, (open_dir, half_span))
        } else {
            (false, (0.0, TAU))
        };

        let layer = build_jagged_ring_layer(
            tree_id,
            (cx, cy),
            r_mean,
            z_layer,
            u,
            k == 0,
            is_open_arc,
            arc_window,
            j,
            lobe_scale,
            tooth_mm,
            base_oval_amp,
            base_oval_phase + (k as f64) * 0.42 * (1.0 - 0.65 * elong),
            noise_base,
            k as u64,
            clamp_region,
            perlin,
            &mut local_rng,
        );
        out.push(layer);
    }

    // Step 2: Internal overlapping sub-bough puffs (`fronde`).
    // These add off-center scalloped C-arcs between the main tiers for extra 3D bough complexity.
    if multi_bough && rings >= 5 {
        let n_boughs = if lobe_scale > 1.35 { 3 } else { 2 };
        let bough_base_ang = local_rng.range(0.0, TAU);

        for b_idx in 0..n_boughs {
            let b_ang = bough_base_ang + (b_idx as f64) * (TAU / n_boughs as f64) + local_rng.range(-0.45, 0.45);
            let b_dist = radius * local_rng.range(0.22, 0.44) * lobe_scale.clamp(0.6, 1.35);
            let b_center = (
                center.0 + b_dist * b_ang.cos() + 0.35 * drift_dx,
                center.1 + b_dist * b_ang.sin() + 0.35 * drift_dy,
            );
            let b_radius = radius * local_rng.range(0.20, 0.35);
            let b_u = (0.28 + 0.24 * (b_idx as f64)).clamp(0.20, 0.85);
            let z_b = z_base + (z_top - z_base) * (0.04 + 0.94 * b_u);

            let arc_dir = shadow_dir + local_rng.range(-0.65, 0.65);
            let arc_win = (arc_dir, local_rng.range(1.30, 1.95));

            let layer = build_jagged_ring_layer(
                tree_id,
                b_center,
                b_radius,
                z_b,
                b_u,
                false,
                true,
                arc_win,
                j * 1.05,
                lobe_scale,
                tooth_mm * 0.95,
                base_oval_amp * 1.15,
                base_oval_phase + local_rng.range(-0.4, 0.4),
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

/// Construct a single `JaggedLayer` by sampling multi-scale 2D Perlin FBM along non-shrinking
/// circles in noise space (shifted per layer `layer_salt`) combined with botanical scalloped
/// cusps and deep inward bough clefts, then applying asymmetric smoothing so contours are
/// organically jagged (`frastagliati`) and fast for a pen plotter.
#[allow(clippy::too_many_arguments)]
fn build_jagged_ring_layer(
    tree_id: usize,
    center: Pt,
    r_mean: f64,
    z_occlude: f64,
    u_norm: f64,
    is_base_footprint: bool,
    is_open_arc: bool,
    arc_window: (f64, f64),
    jaggedness: f64,
    lobe_scale: f64,
    tooth_mm: f64,
    oval_amp: f64,
    oval_phase: f64,
    noise_base: Pt,
    layer_salt: u64,
    clamp_region: Option<&Region>,
    perlin: &Perlin,
    rng: &mut Rng,
) -> JaggedLayer {
    let j = jaggedness.clamp(0.0, 2.5);
    let lobes = lobe_scale.clamp(0.0, 2.5);
    let s = layer_salt as f64;

    // 1. Step the noise-space origin from layer `k` to `k + 1`:
    //    - Macro boughs shift moderately (`0.52` per layer) so overall crown lobes evolve.
    //    - Meso concavities/convexities (`1.05` per layer) and micro tufts (`1.75` per layer)
    //      change substantially between consecutive layers so inner rings are NEVER parallel
    //      offsets of the outer ring!
    let macro_orig = (noise_base.0 + s * 0.52, noise_base.1 - s * 0.46);
    let meso_orig = (noise_base.0 + 31.7 + s * 1.05, noise_base.1 - 27.3 + s * 0.92);
    let micro_orig = (noise_base.0 - 47.1 + s * 1.75, noise_base.1 + 53.9 - s * 1.55);

    // 2. Noise-space circle radii `rho_macro`, `rho_meso`, `rho_micro` DO NOT collapse to 0
    //    on inner rings (`u_norm -> 1`), guaranteeing that inner rings have the exact same
    //    organic concavities and convexities as the outer perimeter!
    let rho_macro = 1.20;
    let rho_meso = 2.15 * (1.0 - 0.22 * u_norm);
    let rho_micro = (3.6 * (1.0 - 0.35 * u_norm)).max(2.0);

    // 3. Botanical scalloped puffs (`|sin|^gamma` with rounded tops and sharp inward V-cusps)
    let circum = TAU * r_mean;
    let n_scallops = (((circum / (tooth_mm * 1.55)).round() as usize).clamp(4, 20)) as f64;
    let scallop_phase = rng.range(0.0, TAU);
    let warp_phase = rng.range(0.0, TAU);

    let inner_boost = 1.0 + 0.25 * u_norm;
    let macro_amp = (0.20 * j.min(1.4) * lobes.clamp(0.35, 1.6) * inner_boost).min(0.42);
    let meso_amp = (0.22 * j.powf(0.92) * lobes.clamp(0.40, 1.5) * inner_boost).min(0.46);
    let micro_amp = (0.12 * j.powf(0.95) * inner_boost).min(0.30);
    let scallop_amp = (0.12 * j.powf(0.90) * inner_boost).min(0.30);

    // 2 to 3 deep fjord-like concave bays (`insenature`) at independent angles per layer
    let cleft_base = rng.range(0.0, TAU);
    let cleft_scale = (j.min(1.5) * lobes * inner_boost).min(1.65);
    let clefts = [
        (
            cleft_base,
            rng.range(0.15, 0.26) * cleft_scale,
            rng.range(0.22, 0.38),
        ),
        (
            (cleft_base + rng.range(1.6, 2.7)) % TAU,
            rng.range(0.13, 0.23) * cleft_scale,
            rng.range(0.20, 0.34),
        ),
        (
            (cleft_base + rng.range(3.6, 4.9)) % TAU,
            rng.range(0.11, 0.20) * cleft_scale,
            rng.range(0.18, 0.32),
        ),
    ];

    let mut raw_r = [0.0f64; LUT_BINS];
    for (i, r_slot) in raw_r.iter_mut().enumerate().take(LUT_BINS) {
        let theta = TAU * (i as f64) / (LUT_BINS as f64);
        let (ct, st) = (theta.cos(), theta.sin());

        let oval = oval_amp * (2.0 * (theta - oval_phase)).cos();

        // Macro bough lobes (3..5 asymmetric boughs from 2D Perlin circle)
        let n_mac = perlin.fbm(macro_orig.0 + rho_macro * ct, macro_orig.1 + rho_macro * st, 2);

        // Meso bays & peninsulas (6..10 irregular concavities/convexities from 2D Perlin circle).
        let n_mes_raw = perlin.fbm(meso_orig.0 + rho_meso * ct, meso_orig.1 + rho_meso * st, 2);
        let n_mes = if n_mes_raw < 0.0 { 1.35 * n_mes_raw } else { n_mes_raw };

        // Micro leaf tufts from 2D Perlin circle
        let n_mic = perlin.noise(micro_orig.0 + rho_micro * ct, micro_orig.1 + rho_micro * st);

        // Scalloped leaf puffs (`|sin|^0.62` has rounded outward arches and crisp inward cusps!)
        let theta_warp = theta + 0.35 * n_mac + 0.20 * (2.0 * theta + warp_phase).sin();
        let puff_wave = (0.5 * n_scallops * theta_warp + scallop_phase).sin().abs().powf(0.62) - 0.56;
        let scallop = scallop_amp * puff_wave * (0.65 + 0.65 * (n_mac + 0.5).clamp(0.1, 1.2));

        // Deep inward concave bays between major boughs
        let mut cleft = 0.0;
        for &(c_ang, c_depth, c_width) in &clefts {
            let d_ang = angle_diff(theta, c_ang).abs();
            if d_ang < c_width {
                let t = 1.0 - d_ang / c_width;
                cleft -= c_depth * t * t * (3.0 - 2.0 * t);
            }
        }

        let factor = (1.0 + oval + macro_amp * n_mac + meso_amp * n_mes + micro_amp * n_mic + scallop + cleft)
            .clamp(0.30, 1.68);
        *r_slot = (r_mean * factor).max(0.07);
    }

    // Two-pass asymmetric radial smoothing: rounds outward lobes into clean, plotter-friendly
    // botanical curves while preserving sharp inward concave cusps between boughs/scallops!
    let smooth_w = (0.24 - 0.11 * (j - 0.85).max(0.0)).clamp(0.05, 0.24);
    for _ in 0..2 {
        let prev_arr = raw_r;
        for i in 0..LUT_BINS {
            let prev = prev_arr[(i + LUT_BINS - 1) % LUT_BINS];
            let curr = prev_arr[i];
            let next = prev_arr[(i + 1) % LUT_BINS];
            raw_r[i] = if curr < prev && curr < next {
                curr * (1.0 - 0.35 * smooth_w) + 0.175 * smooth_w * (prev + next)
            } else {
                curr * (1.0 - 2.0 * smooth_w) + smooth_w * (prev + next)
            };
        }
    }

    let mut lut = [0.0f64; LUT_BINS];
    let mut min_r = f64::MAX;
    let mut max_r = 0.0f64;

    for i in 0..LUT_BINS {
        let mut r = raw_r[i];
        let theta = TAU * (i as f64) / (LUT_BINS as f64);
        let (ct, st) = (theta.cos(), theta.sin());
        if let Some(clamp) = clamp_region {
            r = clamp_ray_to_region(clamp, center, (ct, st), r);
        }

        lut[i] = r;
        min_r = min_r.min(r);
        max_r = max_r.max(r);
    }

    // Adaptive vertex count proportional to circumference so small inner rings do not emit
    // hundreds of micro-segments on a pen plotter!
    let n_pts = ((TAU * r_mean / 0.24).round() as usize).clamp(18, LUT_BINS);
    let mut ring = Vec::with_capacity(n_pts);
    for k in 0..n_pts {
        let pos = (k as f64 / n_pts as f64) * (LUT_BINS as f64);
        let idx0 = (pos.floor() as usize) % LUT_BINS;
        let idx1 = (idx0 + 1) % LUT_BINS;
        let frac = pos - pos.floor();
        let r = lut[idx0] * (1.0 - frac) + lut[idx1] * frac;
        let theta = TAU * (k as f64) / (n_pts as f64);
        ring.push((center.0 + r * theta.cos(), center.1 + r * theta.sin()));
    }

    JaggedLayer {
        tree_id,
        center,
        z_occlude,
        is_base_footprint,
        is_open_arc,
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
    // Slight negative/zero overlap slack (`0.02 * crown`): allows lines to meet cleanly
    // at upper layer boundaries without boolean polygon clipping overhead.
    let overlap_slack = (p.crown * 0.015).clamp(0.0, 0.12);

    let mut strokes: Strokes = Vec::with_capacity(layers.len() * 2);

    for (idx, layer) in layers.iter().enumerate() {
        let n = layer.ring.len();
        if n < 3 {
            continue;
        }

        // Determine which vertices around the ring are visible (not hidden by a higher
        // layer, not outside an open-arc window, and not in a sunlit highlight gap).
        let is_vertex_visible = |i: usize, pt: Pt| -> bool {
            let theta = TAU * (i as f64) / (n as f64);

            // 1. Open crescent / fronda arc window on inner bough layers
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
                    let dx = pt.0 - layer.center.0;
                    let dy = pt.1 - layer.center.1;
                    let len = dx.hypot(dy);
                    if len > 1e-6 {
                        let facing_sun = (dx * lx + dy * ly) / len;
                        if !layer.is_base_footprint {
                            let thresh = (0.68 - 0.62 * strength * (0.35 + 0.65 * layer.u_norm)).clamp(0.02, 0.80);
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

            // 3. Fast $O(1)$ analytical 3D occlusion check against higher layers
            !occluder.is_occluded(pt, idx, layers, overlap_slack)
        };

        let mut vis = [false; LUT_BINS];
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

    strokes
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
        let out_s = fill_isoline_stack_params(&host, &smooth);
        let out_j = fill_isoline_stack_params(&host, &jagged);
        assert!(!out_s.strokes.is_empty());
        assert!(!out_j.strokes.is_empty());
        assert_ne!(out_s.strokes, out_j.strokes);
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
