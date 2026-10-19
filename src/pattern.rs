//! Your own pattern, on the same terms as the built-in ones — and the
//! standard library that the built-in patterns themselves are built with.
//!
//! The engine calls a pattern once with two things:
//!
//! - a [`Canvas`]: where it is drawing — the target [`Region`], outer clipping
//!   `limit` (after [`EdgeMode`]), bounds, `spacing`, `angle`, `seed`,
//!   [`Anchor`], `edge`, and `fray`.
//! - [`Tools`]: the standard-library toolkit, already bound to the canvas
//!   region, seed, anchor, edge mode, and fray band.
//!
//! The pattern returns strokes in page millimetres, and the engine clips and
//! (when `fray > 0`) smoothly frays them at the effective boundary, while
//! optionally assembling the polygon [`FillOutput::mask`] for layer masking.

use std::cell::RefCell;
use std::rc::Rc;

use crate::anchor::{cell_seed, centre, indices, Anchor, Frame};
use crate::fray::{border_ramp, fray_strokes, resample, smooth, whole_at};
use crate::geo::{Pt, Region, Strokes};
use crate::noise::Perlin;
use crate::random::{poisson_disk, Rng};

/// How a pattern's elements, blobs, or lines meet the region's boundary.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum EdgeMode {
    /// Ignore the boundary during generation and cut strokes cleanly at the wall.
    #[default]
    Clip,
    /// Keep elements strictly inside without slicing them open: contours close
    /// inside the wall and discrete shapes/cells pack or filter wholly inside.
    Inside,
    /// Allow elements or blobs rooted in/on the region to bulge or extend
    /// outside the boundary by up to `mm` millimetres without being cut at the wall.
    Overflow(f64),
}

/// Standardized parameters shared across pattern fills.
/// Inline key-value map (`Copy`, zero heap allocation) for pattern-specific
/// parameters (`custom: { jaggedness: 1.2, coverage: 0.7, scatter: 0.4, ... }`)
/// passed alongside the standardized [`FillParams`] fields.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct CustomParams {
    entries: [([u8; 16], f64); 8],
    len: usize,
}

impl CustomParams {
    pub const fn new() -> Self {
        Self {
            entries: [([0u8; 16], 0.0); 8],
            len: 0,
        }
    }

    fn pack_key(key: &str) -> [u8; 16] {
        let mut buf = [0u8; 16];
        let bytes = key.as_bytes();
        let n = bytes.len().min(16);
        buf[..n].copy_from_slice(&bytes[..n]);
        buf
    }

    /// Insert or update a custom parameter (`key` up to 16 ASCII bytes).
    pub fn set(&mut self, key: &str, value: f64) {
        let k = Self::pack_key(key);
        for i in 0..self.len {
            if self.entries[i].0 == k {
                self.entries[i].1 = value;
                return;
            }
        }
        if self.len < self.entries.len() {
            self.entries[self.len] = (k, value);
            self.len += 1;
        }
    }

    /// Look up a custom parameter by name.
    pub fn get(&self, key: &str) -> Option<f64> {
        let k = Self::pack_key(key);
        for i in 0..self.len {
            if self.entries[i].0 == k {
                return Some(self.entries[i].1);
            }
        }
        None
    }

    /// Look up a custom parameter by name, falling back to `default`.
    pub fn get_or(&self, key: &str, default: f64) -> f64 {
        self.get(key).unwrap_or(default)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillParams {
    /// Primary pitch, line spacing, cell size, or stone size in mm.
    pub spacing: f64,
    /// Pattern orientation in degrees.
    pub angle: f64,
    /// Deterministic RNG seed.
    pub seed: u64,
    /// Lattice placement (`Anchor::LOCAL` or `Anchor::GLOBAL` + shift).
    pub anchor: Anchor,
    /// Optional hard wall offset inward in mm before filling.
    pub inset: Option<f64>,
    /// How elements meet the boundary (`Clip`, `Inside`, or `Overflow(mm)`).
    pub edge: EdgeMode,
    /// Smooth perimeter disappearing band in mm (`0.0` = hard cut).
    pub fray: f64,
    /// Pattern-specific custom variables (`custom: { ... }`).
    pub custom: CustomParams,
}

impl Default for FillParams {
    fn default() -> Self {
        FillParams {
            spacing: 2.0,
            angle: 0.0,
            seed: 0,
            anchor: Anchor::LOCAL,
            inset: None,
            edge: EdgeMode::Clip,
            fray: 0.0,
            custom: CustomParams::new(),
        }
    }
}

impl FillParams {
    pub fn new(spacing: f64) -> Self {
        Self {
            spacing,
            ..Self::default()
        }
    }

    pub fn angle(mut self, angle: f64) -> Self {
        self.angle = angle;
        self
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn inset(mut self, inset: impl Into<Option<f64>>) -> Self {
        self.inset = inset.into();
        self
    }

    pub fn inset_opt(mut self, inset: Option<f64>) -> Self {
        self.inset = inset;
        self
    }

    pub fn edge(mut self, edge: EdgeMode) -> Self {
        self.edge = edge;
        self
    }

    pub fn fray(mut self, fray: f64) -> Self {
        self.fray = fray;
        self
    }

    /// Set a pattern-specific custom parameter (e.g. `.custom("jaggedness", 1.2)`).
    pub fn custom(mut self, key: &str, value: f64) -> Self {
        self.custom.set(key, value);
        self
    }

    /// Read a pattern-specific custom parameter, or `default` if unset.
    pub fn get_custom(&self, key: &str, default: f64) -> f64 {
        self.custom.get_or(key, default)
    }

    /// Resolve the base region after applying `inset` (or `default_inset` if `None`).
    pub fn resolve_region(&self, region: &Region, default_inset: f64) -> Region {
        let ins = self.inset.unwrap_or(default_inset);
        if ins > 0.0 {
            region.inset(ins)
        } else {
            region.clone()
        }
    }

    /// Resolve the outer clipping envelope for `base` given `self.edge`.
    pub fn resolve_limit(&self, base: &Region) -> Region {
        match self.edge {
            EdgeMode::Overflow(m) if m > 0.0 => base.outset(m),
            _ => base.clone(),
        }
    }
}

/// Linework plus the polygon mask (`Region`) occupied by the fill, ready to
/// subtract from lower layers (`below.difference(&output.mask)`).
#[derive(Clone, Debug, Default)]
pub struct FillOutput {
    /// Open polylines in draw order (mm).
    pub strokes: Strokes,
    /// Polygon mask (mm) of the area covered by this fill.
    pub mask: Region,
}

#[derive(Default)]
struct MaskCollector {
    rings: Vec<Vec<Pt>>,
    regions: Vec<Region>,
    halo: Option<f64>,
}

/// Where a pattern is drawing. All coordinates are page millimetres, the
/// same ones the region was built in.
pub struct Canvas<'a> {
    /// Everything to fill (after `inset`), holes included.
    pub region: &'a Region,
    /// The outer clipping limit: equals `region` for `Clip`/`Inside`, or
    /// `region.outset(m)` for `EdgeMode::Overflow(m)`.
    pub limit: Region,
    /// Bounding box of `region`: `(min_x, min_y, max_x, max_y)`.
    pub bounds: (f64, f64, f64, f64),
    /// Primary pattern spacing in mm.
    pub spacing: f64,
    /// Pattern orientation in degrees.
    pub angle: f64,
    /// For patterns with randomness: same seed, same drawing.
    pub seed: u64,
    /// Where the pattern's lattice should sit ([`Anchor`]).
    pub anchor: Anchor,
    /// The anchor resolved for this region: the point to build a lattice
    /// through. Locally the middle of `bounds`, globally a page point.
    pub origin: Pt,
    /// How elements meet the boundary (`Clip`, `Inside`, or `Overflow(mm)`).
    pub edge: EdgeMode,
    /// Smooth perimeter disappearing band in mm (`0.0` = off).
    pub fray: f64,
    /// Pattern-specific custom variables (`custom: { ... }`).
    pub custom: CustomParams,
}

impl<'a> Canvas<'a> {
    /// Read a custom parameter from `self.custom`, falling back to `default`.
    pub fn param(&self, key: &str, default: f64) -> f64 {
        self.custom.get_or(key, default)
    }
}

/// The standard-library toolkit: what the built-in fills are made of, set to
/// one region, seed, anchor, edge mode, and fray band.
#[derive(Clone)]
pub struct Tools<'a> {
    region: &'a Region,
    limit: Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    perlin: Perlin,
    fray_noise: Perlin,
    anchor: Anchor,
    edge: EdgeMode,
    fray: f64,
    mask: Option<Rc<RefCell<MaskCollector>>>,
    custom_fray: std::cell::Cell<bool>,
}

impl<'a> Tools<'a> {
    pub fn new(region: &'a Region, seed: u64) -> Tools<'a> {
        Tools {
            region,
            limit: region.clone(),
            spacing: 2.0,
            angle: 0.0,
            seed,
            perlin: Perlin::new(seed),
            fray_noise: Perlin::new(seed.wrapping_add(0xF4A7)),
            anchor: Anchor::LOCAL,
            edge: EdgeMode::Clip,
            fray: 0.0,
            mask: None,
            custom_fray: std::cell::Cell::new(false),
        }
    }

    pub fn from_params(region: &'a Region, limit: Region, params: &FillParams) -> Tools<'a> {
        let mask = ACTIVE_MASK.with(|m| m.borrow().clone());
        Tools {
            region,
            limit,
            spacing: params.spacing,
            angle: params.angle,
            seed: params.seed,
            perlin: Perlin::new(params.seed),
            fray_noise: Perlin::new(params.seed.wrapping_add(0xF4A7)),
            anchor: params.anchor,
            edge: params.edge,
            fray: params.fray,
            mask,
            custom_fray: std::cell::Cell::new(false),
        }
    }

    /// The same tools with lattices placed by `anchor`.
    pub fn anchored(self, anchor: Anchor) -> Tools<'a> {
        Tools { anchor, ..self }
    }

    /// The same tools with a different primary `spacing`.
    pub fn with_spacing(self, spacing: f64) -> Tools<'a> {
        Tools { spacing, ..self }
    }

    /// The same tools with a different `edge` mode.
    pub fn with_edge(self, edge: EdgeMode) -> Tools<'a> {
        let limit = match edge {
            EdgeMode::Overflow(m) if m > 0.0 => self.region.outset(m),
            _ => self.region.clone(),
        };
        Tools { edge, limit, ..self }
    }

    /// The same tools with a different `fray` band (mm).
    pub fn with_fray(self, fray: f64) -> Tools<'a> {
        Tools { fray, ..self }
    }

    /// The same tools for another region (a piece from `parts`, an inset).
    pub fn on<'b>(&self, region: &'b Region) -> Tools<'b> {
        let limit = match self.edge {
            EdgeMode::Overflow(m) if m > 0.0 => region.outset(m),
            _ => region.clone(),
        };
        Tools {
            region,
            limit,
            spacing: self.spacing,
            angle: self.angle,
            seed: self.seed,
            perlin: self.perlin.clone(),
            fray_noise: self.fray_noise.clone(),
            anchor: self.anchor,
            edge: self.edge,
            fray: self.fray,
            mask: self.mask.clone(),
            custom_fray: std::cell::Cell::new(self.custom_fray.get()),
        }
    }

    /// The same tools with a derived seed (for multi-layer fills).
    pub fn seeded(&self, salt: u64) -> Tools<'a> {
        let seed = self.seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        Tools {
            region: self.region,
            limit: self.limit.clone(),
            spacing: self.spacing,
            angle: self.angle,
            seed,
            perlin: Perlin::new(seed),
            fray_noise: Perlin::new(seed.wrapping_add(0xF4A7)),
            anchor: self.anchor,
            edge: self.edge,
            fray: self.fray,
            mask: self.mask.clone(),
            custom_fray: std::cell::Cell::new(self.custom_fray.get()),
        }
    }

    // -----------------------------------------------------------------------
    // RNG & Noise
    // -----------------------------------------------------------------------

    /// Deterministic RNG stream for this canvas seed.
    pub fn rng(&self) -> Rng {
        Rng::new(self.seed)
    }

    /// Deterministic RNG stream salted with `salt`.
    pub fn rng_salted(&self, salt: u64) -> Rng {
        Rng::new(self.seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// Deterministic RNG for lattice cell `(i, j)`. Guarantees global-anchor
    /// continuity across adjacent regions.
    pub fn cell_rng(&self, i: i64, j: i64) -> Rng {
        Rng::new(cell_seed(self.seed, i, j))
    }

    /// Smooth Perlin noise, roughly -1 to 1, one bump per unit; each extra
    /// octave adds detail twice as fine and half as strong.
    pub fn noise(&self, x: f64, y: f64, octaves: usize) -> f64 {
        self.perlin.fbm(x, y, octaves.max(1))
    }

    // -----------------------------------------------------------------------
    // Perimeter & Boundary Helpers
    // -----------------------------------------------------------------------

    /// Smooth `[0, 1]` survival weight at `p` for the current `fray` band.
    /// Returns `1.0` when `fray <= 0.0` and `p` is inside the limit region.
    pub fn whole_at(&self, p: Pt) -> f64 {
        whole_at(&self.limit, p, self.spacing, self.fray, &self.fray_noise)
    }

    /// Mark that this pattern handles [`Canvas::fray`] directly at the motif
    /// level (e.g. scaling and dropping multi-stroke tufts or branched twigs
    /// around their shared root), so [`fill_pattern_output`] will clip to
    /// `limit` without running post-pass stroke fragmentation.
    pub fn claim_fray(&self) {
        self.custom_fray.set(true);
    }

    /// Smooth `[0, 1]` survival weight at `p` with custom `spacing` and `fray`.
    pub fn whole_at_band(&self, p: Pt, spacing: f64, fray: f64) -> f64 {
        whole_at(&self.limit, p, spacing, fray, &self.fray_noise)
    }

    /// Smooth `[0, 1]` border ramp (`0.0` at boundary, `1.0` at `falloff` mm
    /// inside), modulated by `rough` (`0..1`) Perlin noise.
    pub fn border_ramp(&self, p: Pt, falloff: f64, rough: f64) -> f64 {
        border_ramp(self.region, p, falloff, rough, &self.perlin)
    }

    /// Whether a discrete element centred at `center` with outer radius `radius`
    /// should be placed under the current [`EdgeMode`]:
    /// - `Clip`: true if any part of `radius` can reach `region`.
    /// - `Inside`: true only if `center` is inside `region` and at least `radius`
    ///   from the boundary (so the shape is never sliced open).
    /// - `Overflow(m)`: true if `center` is inside `region` and its protrusion
    ///   past the boundary does not exceed `m`.
    pub fn element_fits(&self, center: Pt, radius: f64) -> bool {
        match self.edge {
            EdgeMode::Clip => {
                self.region.covers_point(center) || self.region.dist_to_boundary(center) <= radius
            }
            EdgeMode::Inside => {
                self.region.covers_point(center) && self.region.dist_to_boundary(center) >= radius
            }
            EdgeMode::Overflow(m) => {
                self.region.covers_point(center)
                    && self.limit.covers_point(center)
                    && (m <= 0.0 || self.limit.dist_to_boundary(center) >= radius)
            }
        }
    }

    /// Domain region over which to seed/scatter centres of elements of size
    /// `pad` under the current [`EdgeMode`].
    pub fn placement_field(&self, pad: f64) -> Region {
        match self.edge {
            EdgeMode::Clip => self.region.outset(pad),
            EdgeMode::Inside => self.region.inset(pad * 0.85),
            EdgeMode::Overflow(_) => self.region.clone(),
        }
    }

    // -----------------------------------------------------------------------
    // Lattices & Coordinate Frames
    // -----------------------------------------------------------------------

    /// Anchored coordinate frame rotated by `angle` degrees.
    pub fn frame(&self, angle: f64) -> Frame {
        Frame::anchored(self.anchor, centre(self.region), angle)
    }

    /// Iterate an anchored lattice with column pitch `dx`, row pitch `dy`,
    /// rotated by `angle` degrees, odd-row shift `row_shift` (in mm), and
    /// uniform `jitter` (in mm). Only points inside `region` are visited, and
    /// each cell `(i, j)` receives its own deterministic [`Rng`] via [`cell_seed`].
    pub fn grid(
        &self,
        dx: f64,
        dy: f64,
        angle: f64,
        row_shift: f64,
        jitter: f64,
        mut f: impl FnMut(&mut Rng, i64, i64, Pt),
    ) {
        if dx <= 0.0 || dy <= 0.0 || self.region.is_empty() {
            return;
        }
        let frame = self.frame(angle);
        let pad = dx.max(dy) + jitter.abs();
        let (u0, u1, v0, v1) = frame.span(&self.limit, pad);
        for j in indices(v0, v1, dy) {
            let shift = if j.rem_euclid(2) == 1 { row_shift } else { 0.0 };
            let v = j as f64 * dy;
            for i in indices(u0, u1, dx) {
                let mut rng = self.cell_rng(i, j);
                let u = i as f64 * dx + shift;
                let (ju, jv) = if jitter > 0.0 {
                    (rng.range(-jitter, jitter), rng.range(-jitter, jitter))
                } else {
                    (0.0, 0.0)
                };
                let c = frame.world((u + ju, v + jv));
                if self.region.covers_point(c) {
                    f(&mut rng, i, j, c);
                }
            }
        }
    }

    /// Iterate all cells of an anchored lattice covering the region plus `margin`
    /// (in frame coordinates), passing `(frame, &mut cell_rng, i, j, (u, v))`.
    pub fn grid_cells(
        &self,
        dx: f64,
        dy: f64,
        angle: f64,
        row_shift: f64,
        margin: f64,
        mut f: impl FnMut(Frame, &mut Rng, i64, i64, Pt),
    ) {
        if dx <= 0.0 || dy <= 0.0 || self.region.is_empty() {
            return;
        }
        let frame = self.frame(angle);
        let (u0, u1, v0, v1) = frame.span(&self.limit, margin);
        for j in indices(v0, v1, dy) {
            let shift = if j.rem_euclid(2) == 1 { row_shift } else { 0.0 };
            let v = j as f64 * dy;
            for i in indices(u0, u1, dx) {
                let mut rng = self.cell_rng(i, j);
                let u = i as f64 * dx + shift;
                f(frame, &mut rng, i, j, (u, v));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Scanlines, Curves, Fields & Scatters
    // -----------------------------------------------------------------------

    /// Parallel lines `spacing` mm apart, cut to the region right up to its
    /// walls, placed by the anchor.
    pub fn hatch(&self, spacing: f64, angle: f64) -> Strokes {
        crate::sweep::sweep_anchored(self.region, spacing, angle, self.anchor)
    }

    /// Parallel lines `spacing` mm apart at `angle`, each bent by `profile(x)`.
    pub fn profile(&self, spacing: f64, angle: f64, profile: impl Fn(f64) -> f64) -> Strokes {
        crate::field::fill_profile(self.region, spacing, angle, self.anchor, profile)
    }

    /// Contour lines of scalar field `f(pt)` at `levels`, sampled at `cell` mm.
    pub fn isolines(&self, cell: f64, levels: &[f64], f: impl Fn(Pt) -> f64) -> Strokes {
        crate::field::fill_isolines(self.region, cell, levels, f)
    }

    /// Random points in the region, no two closer than `spacing` mm.
    pub fn scatter(&self, spacing: f64) -> Vec<Pt> {
        poisson_disk(self.region, spacing, self.seed)
    }

    /// Random points `dense` mm apart where `field` is 0 and `sparse` mm
    /// apart where it is 1 (bidirectional: either `dense` or `sparse` may be smaller).
    pub fn scatter_field(&self, dense: f64, sparse: f64, field: impl Fn(Pt) -> f64) -> Vec<Pt> {
        crate::stipple::scatter_field(self.region, dense, sparse, self.seed, field)
    }

    // -----------------------------------------------------------------------
    // Voronoi & Cellular Primitives
    // -----------------------------------------------------------------------

    /// Compute bounded Voronoi cell polygons for `centres`, considering neighbours
    /// within `reach` mm. Returns `(index, centre, cell_ring)` for each valid cell.
    pub fn voronoi_cells(&self, centres: &[Pt], reach: f64) -> Vec<(usize, Pt, Vec<Pt>)> {
        let reach2 = reach * reach;
        let box_r = reach * 0.8;
        let mut out = Vec::with_capacity(centres.len());
        for (i, &c) in centres.iter().enumerate() {
            let mut cell = vec![
                (c.0 - box_r, c.1 - box_r),
                (c.0 + box_r, c.1 - box_r),
                (c.0 + box_r, c.1 + box_r),
                (c.0 - box_r, c.1 + box_r),
            ];
            for (j, &o) in centres.iter().enumerate() {
                if i == j || (o.0 - c.0).powi(2) + (o.1 - c.1).powi(2) > reach2 {
                    continue;
                }
                cell = clip_half_plane(&cell, c, o);
                if cell.len() < 3 {
                    break;
                }
            }
            if cell.len() >= 3 {
                out.push((i, c, cell));
            }
        }
        out
    }

    /// Compute deduplicated shared Voronoi edges `(i, j, a, b)` with `i < j`
    /// for `centres` within `reach` mm. Adjacent cells share a single wall and
    /// 3-way junctions meet cleanly.
    pub fn voronoi_edges(&self, centres: &[Pt], reach: f64) -> Vec<(usize, usize, Pt, Pt)> {
        let reach2 = reach * reach;
        let box_r = reach * 0.85;
        let mut edges = Vec::new();
        for (i, &c) in centres.iter().enumerate() {
            let mut cell: Vec<(Pt, Option<usize>)> = vec![
                ((c.0 - box_r, c.1 - box_r), None),
                ((c.0 + box_r, c.1 - box_r), None),
                ((c.0 + box_r, c.1 + box_r), None),
                ((c.0 - box_r, c.1 + box_r), None),
            ];
            for (j, &o) in centres.iter().enumerate() {
                if i == j || (o.0 - c.0).powi(2) + (o.1 - c.1).powi(2) > reach2 {
                    continue;
                }
                cell = clip_half_plane_tagged(&cell, c, o, j);
                if cell.len() < 3 {
                    break;
                }
            }
            if cell.len() < 3 {
                continue;
            }
            let n = cell.len();
            for k in 0..n {
                let (a, tag) = cell[k];
                let (b, _) = cell[(k + 1) % n];
                if let Some(j) = tag {
                    if i < j && (b.0 - a.0).hypot(b.1 - a.1) >= 1e-4 {
                        edges.push((i, j, a, b));
                    }
                }
            }
        }
        edges
    }

    // -----------------------------------------------------------------------
    // Marks, Shapes & Polyline Operators
    // -----------------------------------------------------------------------

    /// Closed circle polyline of radius `r` around `c`.
    pub fn circle(&self, c: Pt, r: f64) -> Vec<Pt> {
        const STEPS: usize = 24;
        let mut pts: Vec<Pt> = (0..STEPS)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / STEPS as f64;
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            })
            .collect();
        if let Some(&first) = pts.first() {
            pts.push(first);
        }
        pts
    }

    /// Irregular foliage/canopy lobe outline around `c`.
    pub fn lobe(&self, c: Pt, r: f64, phase: f64, rough: f64) -> Vec<Pt> {
        const STEPS: usize = 28;
        let mut pts: Vec<Pt> = (0..STEPS)
            .map(|k| {
                let t = std::f64::consts::TAU * k as f64 / STEPS as f64;
                let w = (7.0 * t + phase).sin() * 0.55
                    + (11.0 * t - phase * 1.3).sin() * 0.35
                    + (17.0 * t + phase * 0.7).sin() * 0.22;
                let rr = r * (1.0 + rough * w);
                (c.0 + rr * t.cos(), c.1 + rr * t.sin())
            })
            .collect();
        if let Some(&first) = pts.first() {
            pts.push(first);
        }
        pts
    }

    /// Circular arc around `c` of radius `r` from `a0` to `a1` radians.
    pub fn arc(&self, c: Pt, r: f64, a0: f64, a1: f64, steps: usize) -> Vec<Pt> {
        let steps = steps.max(2);
        (0..=steps)
            .map(|k| {
                let a = a0 + (a1 - a0) * (k as f64 / steps as f64);
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            })
            .collect()
    }

    /// Straight tick centred at `c` at `angle_rad` radians with half-length `half`.
    pub fn tick(&self, c: Pt, angle_rad: f64, half: f64) -> Vec<Pt> {
        let (sa, ca) = angle_rad.sin_cos();
        vec![(c.0 - ca * half, c.1 - sa * half), (c.0 + ca * half, c.1 + sa * half)]
    }

    /// Orthogonal `+` cross centred at `c` with arm half-length `half` and `twist_rad`.
    pub fn cross(&self, c: Pt, half: f64, twist_rad: f64) -> Strokes {
        vec![
            self.tick(c, std::f64::consts::FRAC_PI_2 + twist_rad, half),
            self.tick(c, twist_rad, half),
        ]
    }

    /// Diagonal `×` cross centred at `c` with arm half-length `half` and `twist_rad`.
    pub fn x_cross(&self, c: Pt, half: f64, twist_rad: f64) -> Strokes {
        vec![
            self.tick(c, std::f64::consts::FRAC_PI_4 + twist_rad, half),
            self.tick(c, -std::f64::consts::FRAC_PI_4 + twist_rad, half),
        ]
    }

    /// Chaikin corner-cutting smoothing over `rounds` passes.
    pub fn smooth(&self, pts: &[Pt], rounds: usize) -> Vec<Pt> {
        smooth(pts, rounds)
    }

    /// Resample a polyline so consecutive vertices are at most `step` mm apart.
    pub fn resample(&self, pts: &[Pt], step: f64) -> Vec<Pt> {
        resample(pts, step)
    }

    /// Explicitly clip and (when `fray > 0`) fray `strokes` against the current
    /// limit region. Note: [`fill_pattern`] and [`fill_pattern_with`] already do
    /// this automatically on the returned `Strokes`.
    pub fn fray(&self, strokes: Strokes) -> Strokes {
        if self.fray > 0.0 {
            fray_strokes(&self.limit, strokes, self.spacing, self.fray, self.seed)
        } else {
            self.limit.clip_strokes(&strokes)
        }
    }

    // -----------------------------------------------------------------------
    // Mask / Coverage Registration (Zero-cost when mask is not requested)
    // -----------------------------------------------------------------------

    /// Whether the caller requested a polygon `mask` ([`FillOutput`]).
    pub fn wants_mask(&self) -> bool {
        self.mask.is_some()
    }

    /// Record a closed ring (in mm) as part of this pattern's semantic mask
    /// (e.g. a pebble, Voronoi stone, tree canopy lobe, or lily pad).
    /// No-op when the caller only requested `Strokes`.
    pub fn add_mask_ring(&self, ring: &[Pt]) {
        if let Some(m) = &self.mask {
            if ring.len() >= 3 {
                m.borrow_mut().rings.push(ring.to_vec());
            }
        }
    }

    /// Record multiple outer rings as part of this pattern's semantic mask.
    pub fn add_mask_rings(&self, rings: &[Vec<Pt>]) {
        if let Some(m) = &self.mask {
            let mut m = m.borrow_mut();
            for r in rings {
                if r.len() >= 3 {
                    m.rings.push(r.clone());
                }
            }
        }
    }

    /// Record an already-built [`Region`] as part of this pattern's semantic mask.
    pub fn add_mask(&self, region: &Region) {
        if let Some(m) = &self.mask {
            if !region.is_empty() {
                m.borrow_mut().regions.push(region.clone());
            }
        }
    }

    /// Request that if no explicit mask polygons are added, the mask should be
    /// built as a stroke halo of `radius` mm around the output strokes.
    pub fn set_mask_halo(&self, radius: f64) {
        if let Some(m) = &self.mask {
            m.borrow_mut().halo = Some(radius);
        }
    }
}

/// Keep the part of `poly` closer to `c` than to `o`.
pub(crate) fn clip_half_plane(poly: &[Pt], c: Pt, o: Pt) -> Vec<Pt> {
    let m = ((c.0 + o.0) / 2.0, (c.1 + o.1) / 2.0);
    let n = (o.0 - c.0, o.1 - c.1);
    let side = |p: Pt| (p.0 - m.0) * n.0 + (p.1 - m.1) * n.1;
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
            let t = sa / (sa - sb);
            out.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
        }
    }
    out
}

/// Keep the part of `poly` closer to `c` than to `o`, recording which
/// neighbour (`Some(j)`) created each edge.
pub(crate) fn clip_half_plane_tagged(
    poly: &[(Pt, Option<usize>)],
    c: Pt,
    o: Pt,
    tag: usize,
) -> Vec<(Pt, Option<usize>)> {
    let m = ((c.0 + o.0) / 2.0, (c.1 + o.1) / 2.0);
    let n = (o.0 - c.0, o.1 - c.1);
    let side = |p: Pt| (p.0 - m.0) * n.0 + (p.1 - m.1) * n.1;
    let mut out = Vec::new();
    let len = poly.len();
    for i in 0..len {
        let (a, tag_a) = poly[i];
        let (b, _) = poly[(i + 1) % len];
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            if sb <= 0.0 {
                out.push((a, tag_a));
            } else {
                out.push((a, tag_a));
                let t = sa / (sa - sb);
                out.push(((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)), Some(tag)));
            }
        } else if sb <= 0.0 {
            let t = sa / (sa - sb);
            out.push(((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)), tag_a));
        }
    }
    out
}

/// Calls `draw` once with the canvas and the tools, and clips what it
/// returns to the region.
pub fn fill_pattern(
    region: &Region,
    seed: u64,
    anchor: Anchor,
    draw: impl FnOnce(&Canvas, &Tools) -> Strokes,
) -> Strokes {
    let params = FillParams::new(2.0).seed(seed).anchor(anchor);
    fill_pattern_with(region, &params, draw)
}

/// Calls `draw` once with [`FillParams`], applying `inset`, [`EdgeMode`]
/// envelope clipping, and (when `params.fray > 0`) universal edge fraying.
pub fn fill_pattern_with(
    region: &Region,
    params: &FillParams,
    draw: impl FnOnce(&Canvas, &Tools) -> Strokes,
) -> Strokes {
    run_pattern(region, params, false, draw).strokes
}

/// Like [`fill_pattern_with`], also returning the polygon [`FillOutput::mask`]
/// occupied by the fill.
pub fn fill_pattern_output(
    region: &Region,
    params: &FillParams,
    draw: impl FnOnce(&Canvas, &Tools) -> Strokes,
) -> FillOutput {
    run_pattern(region, params, true, draw)
}

thread_local! {
    static ACTIVE_MASK: RefCell<Option<Rc<RefCell<MaskCollector>>>> = const { RefCell::new(None) };
}

fn run_pattern(
    region: &Region,
    params: &FillParams,
    collect_mask: bool,
    draw: impl FnOnce(&Canvas, &Tools) -> Strokes,
) -> FillOutput {
    if region.is_empty() {
        return FillOutput::default();
    }
    let target = params.resolve_region(region, 0.0);
    if target.is_empty() {
        return FillOutput::default();
    }
    let limit = params.resolve_limit(&target);
    let origin = params.anchor.resolve(centre(&target));
    let canvas = Canvas {
        region: &target,
        limit: limit.clone(),
        bounds: target.bounds_mm(),
        spacing: params.spacing,
        angle: params.angle,
        seed: params.seed,
        anchor: params.anchor,
        origin,
        edge: params.edge,
        fray: params.fray,
        custom: params.custom,
    };
    let prev_mask = ACTIVE_MASK.with(|m| m.borrow().clone());
    let mask_rc = if collect_mask {
        let rc = Rc::new(RefCell::new(MaskCollector::default()));
        ACTIVE_MASK.with(|m| *m.borrow_mut() = Some(rc.clone()));
        Some(rc)
    } else {
        prev_mask.clone()
    };
    let mut tools = Tools::from_params(&target, limit.clone(), params);
    tools.mask = mask_rc.clone();

    let raw = draw(&canvas, &tools);

    if collect_mask {
        ACTIVE_MASK.with(|m| *m.borrow_mut() = prev_mask);
    }

    let strokes = if params.fray > 0.0 && !tools.custom_fray.get() {
        fray_strokes(&limit, raw, params.spacing.max(0.5), params.fray, params.seed)
    } else {
        limit.clip_strokes(&raw)
    };

    let mask = if collect_mask {
        let rc = mask_rc.expect("collect_mask initialized mask_rc");
        let m = rc.borrow();
        let mut combined = Region::default();
        if !m.rings.is_empty() {
            let polys: Vec<Vec<Vec<Pt>>> = m.rings.iter().map(|r| vec![r.clone()]).collect();
            combined = Region::from_polygons(&polys);
        }
        for reg in &m.regions {
            combined = combined.union(reg);
        }
        if !combined.is_empty() {
            let clipped_mask = combined.intersection(&limit);
            if params.fray > 0.0 && !strokes.is_empty() {
                let halo = m.halo.unwrap_or((params.spacing * 0.55).max(0.25));
                clipped_mask.intersection(&Region::around_strokes(&strokes, halo))
            } else {
                clipped_mask
            }
        } else if strokes.is_empty() {
            Region::default()
        } else if params.fray > 0.0
            || m.halo.is_some()
            || matches!(params.edge, EdgeMode::Inside | EdgeMode::Overflow(_))
        {
            let halo = m.halo.unwrap_or((params.spacing * 0.55).max(0.25));
            Region::around_strokes(&strokes, halo).intersection(&limit)
        } else {
            limit
        }
    } else {
        Region::default()
    };

    FillOutput { strokes, mask }
}

// ---------------------------------------------------------------------------
// Unified Pattern Registry
// ---------------------------------------------------------------------------

/// Core built-in pattern names (excluding `litho::BUILTIN` and `pat:<NAME>`,
/// which [`fill_by_name`] and [`all_fill_names`] also support).
pub const FILL_NAMES: &[&str] = &[
    "zigzag",
    "rectilinear",
    "lines",
    "grid",
    "triangles",
    "concentric",
    "spiral",
    "hilbert",
    "honeycomb",
    "dots",
    "dots_hex",
    "waves",
    "wave_grid",
    "scales",
    "sunburst",
    "stipple",
    "stipple_noise",
    "stipple_edge",
    "gravel",
    "gravel_clipped",
    "grass",
    "scribble",
    "truchet",
    "marble",
    "wood",
    "cracks",
    "cell_net",
    "eddies",
    "swells",
    "ripples",
    "terrazzo",
    "chess",
    "pebbles",
    "leaves",
    "sprouts",
    "fluid_grass",
    "lily_pads",
    "canopy",
    "tufts",
    "twigs",
    "vines",
    "growth",
    "isoline_blobs",
    "isoline_stack",
    "solid",
];

/// Every fill name supported by [`fill_by_name`] and [`fill_output_by_name`],
/// including [`FILL_NAMES`], [`crate::litho::BUILTIN`], and `pat:<NAME>`.
pub fn all_fill_names() -> Vec<String> {
    let mut out: Vec<String> = FILL_NAMES.iter().map(|s| (*s).to_string()).collect();
    for name in crate::litho::BUILTIN {
        out.push((*name).to_string());
    }
    for name in crate::pat::BUILTIN {
        out.push(format!("pat:{name}"));
    }
    out
}

/// Dispatch any built-in pattern by name with unified [`FillParams`] (`spacing`,
/// `angle`, `seed`, `anchor`, `inset`, `edge`, `fray`).
pub fn fill_by_name(name: &str, region: &Region, params: &FillParams) -> Option<Strokes> {
    fill_named(name, region, params, false).map(|o| o.strokes)
}

/// Dispatch any built-in pattern by name with unified [`FillParams`] and return
/// both [`FillOutput::strokes`] and the polygon [`FillOutput::mask`].
pub fn fill_output_by_name(name: &str, region: &Region, params: &FillParams) -> Option<FillOutput> {
    fill_named(name, region, params, true)
}

fn fill_named(
    name: &str,
    region: &Region,
    params: &FillParams,
    collect_mask: bool,
) -> Option<FillOutput> {
    // Check that `name` is known before running `run_pattern`.
    let is_pat = name
        .strip_prefix("pat:")
        .is_some_and(|n| crate::pat::builtin(n).is_some());
    let is_litho = crate::litho::BUILTIN.contains(&name);
    let is_core = FILL_NAMES.contains(&name);
    if !is_core && !is_litho && !is_pat {
        return None;
    }

    if name == "solid" {
        let target = params.resolve_region(region, 0.0);
        let limit = params.resolve_limit(&target);
        return Some(FillOutput {
            strokes: crate::solid::fill_solid(&limit),
            mask: if collect_mask { limit } else { Region::default() },
        });
    }

    // Sub-params passed to inner `_params` functions have `inset: None` and
    // `fray: 0.0` because the outer `run_pattern` applies `inset`, `limit`,
    // and `fray` once on the final result.
    let inner = FillParams {
        inset: None,
        fray: 0.0,
        ..*params
    };

    Some(run_pattern(region, params, collect_mask, |c, t| {
        let s = c.spacing;
        let a = c.angle;
        let seed = c.seed;
        let anchor = c.anchor;
        match name {
            "zigzag" => crate::zigzag::fill_zigzag_inset(&c.limit, s, a, None, anchor),
            "rectilinear" => crate::rectilinear::fill_rectilinear(&c.limit, s, a, None, anchor),
            "lines" => crate::grid::fill_lines(&c.limit, s, a, anchor),
            "grid" => crate::grid::fill_grid(&c.limit, s, a, anchor),
            "triangles" => crate::grid::fill_triangles(&c.limit, s, a, anchor),
            "concentric" => crate::concentric::fill_concentric(&c.limit, s, None),
            "spiral" => crate::spiral::fill_spiral(&c.limit, s),
            "hilbert" => crate::hilbert::fill_hilbert(&c.limit, s, None),
            "honeycomb" => crate::honeycomb::fill_honeycomb(&c.limit, s, a, 0.0, anchor),
            "dots" => {
                t.set_mask_halo((s * 0.35).max(0.25));
                crate::dotswaves::fill_dots_with_lattice(
                    &c.limit,
                    s,
                    a,
                    s * 0.2,
                    crate::dotswaves::Lattice::Square,
                    anchor,
                )
            }
            "dots_hex" => {
                t.set_mask_halo((s * 0.35).max(0.25));
                crate::dotswaves::fill_dots_with_lattice(
                    &c.limit,
                    s,
                    a,
                    s * 0.2,
                    crate::dotswaves::Lattice::Hexagonal,
                    anchor,
                )
            }
            "waves" => crate::dotswaves::fill_waves_params(c.region, &inner).strokes,
            "wave_grid" => crate::dotswaves::fill_wave_grid_params(c.region, &inner).strokes,
            "scales" => crate::scales::fill_scales(&c.limit, s, a, anchor),
            "sunburst" => crate::radial::fill_sunburst(&c.limit, s, a, anchor),
            "stipple" => {
                t.set_mask_halo((s * 0.45).max(0.25));
                crate::stipple::fill_stipple(&c.limit, s, s * 0.3, seed)
            }
            "stipple_noise" => {
                t.set_mask_halo((s * 0.55).max(0.25));
                crate::stipple::fill_stipple_noise(&c.limit, s, s * 2.5, s * 0.4, 12.0, seed)
            }
            "stipple_edge" => {
                t.set_mask_halo((s * 0.55).max(0.25));
                crate::stipple::fill_stipple_edge(&c.limit, s, s * 4.0, s * 0.4, 8.0, 0.5, seed)
            }
            "gravel" => {
                let g_params = match inner.edge {
                    EdgeMode::Clip => inner.edge(EdgeMode::Inside),
                    _ => inner,
                };
                crate::gravel::fill_gravel_params(c.region, &g_params).strokes
            }
            "gravel_clipped" => {
                crate::gravel::fill_gravel_params(c.region, &inner.edge(EdgeMode::Clip)).strokes
            }
            "grass" => crate::grass::fill_grass(&c.limit, s, a, seed, anchor),
            "scribble" => crate::scribble::fill_scribble(&c.limit, s, a, seed),
            "truchet" => crate::truchet::fill_truchet(&c.limit, s.max(0.5), a, seed, anchor),
            "marble" => crate::textures::draw_marble(c, t),
            "wood" => crate::textures::draw_wood(c, t),
            "cracks" => crate::textures::fill_cracks_params(
                c.region,
                &FillParams {
                    spacing: s * 4.0,
                    ..inner
                },
            )
            .strokes,
            "cell_net" => crate::textures::draw_cell_net(c, t),
            "eddies" => crate::textures::fill_eddies_params(c.region, &inner).strokes,
            "swells" => crate::textures::fill_swells_params(c.region, &inner).strokes,
            "ripples" => crate::textures::fill_ripples_params(c.region, &inner).strokes,
            "terrazzo" => crate::terrazzo::draw_terrazzo(c, t, s / 6.0),
            "chess" => crate::chess::fill_chess(&c.limit, s, s / 8.0, a, anchor),
            "pebbles" => crate::nature::draw_pebbles(c, t),
            "leaves" => crate::nature::draw_sprouts(c, t),
            "sprouts" => crate::nature::draw_sprouts(c, t),
            "fluid_grass" => crate::nature::draw_fluid_grass(c, t),
            "lily_pads" => crate::nature::draw_lily_pads(c, t),
            "canopy" => crate::nature::draw_canopy(c, t),
            "tufts" => crate::nature::draw_tufts(c, t),
            "twigs" => crate::nature::draw_twigs(c, t),
            "vines" => crate::nature::draw_vines(c, t),
            "growth" => crate::growth::fill_growth_params(c.region, &inner).strokes,
            "isoline_blobs" => {
                let mut p =
                    crate::isoline_blobs::default_params((s * 4.0).max(2.0), s.max(0.25), seed);
                p.edge = c.edge;
                p.fray = 0.0;
                let out = crate::isoline_blobs::fill_isoline_blobs_with_coverage(c.region, &p);
                t.add_mask(&out.coverage);
                out.strokes
            }
            "isoline_stack" => {
                let crown = (s * 4.2).max(3.6);
                let p = crate::isoline_stack::IsolineStackParams::from_fill_params(&inner, crown);
                let out = crate::isoline_stack::fill_isoline_stack_with_coverage(c.region, &p);
                t.add_mask(&out.coverage);
                out.strokes
            }
            other if other.starts_with("pat:") => {
                let pat_name = other.strip_prefix("pat:").unwrap_or("");
                if let Some(pattern) = crate::pat::builtin(pat_name) {
                    crate::pat::fill_pat(&c.limit, &pattern, s, a, anchor)
                } else {
                    vec![]
                }
            }
            other => crate::litho::fill_by_name(other, &c.limit, s, a, seed).unwrap_or_default(),
        }
    }))
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
        let per_piece = fill_pattern(&region, 0, Anchor::LOCAL, |c, t| {
            c.region.parts().iter().flat_map(|p| t.on(p).hatch(1.5, 30.0)).collect()
        });
        assert!((length(&whole) - length(&direct)).abs() < 1e-6 * length(&direct));
        assert!((length(&per_piece) - length(&direct)).abs() < 0.01 * length(&direct));
    }

    #[test]
    fn stipple_edge_rebuilt_from_the_tools() {
        let region = doughnut_and_square();
        let mine = fill_pattern(&region, 5, Anchor::LOCAL, |c, t| {
            crate::stipple::fill_stipple_field(c.region, 0.8, 4.0, 0.3, c.seed, |p| {
                t.border_ramp(p, 8.0, 0.5)
            })
        });
        assert_eq!(mine, crate::stipple::fill_stipple_edge(&region, 0.8, 4.0, 0.3, 8.0, 0.5, 5));
    }

    #[test]
    fn fill_pattern_with_supports_fray_and_mask() {
        let region = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let params = FillParams::new(2.0).fray(6.0).seed(9);
        let out = fill_pattern_output(&region, &params, |_, t| t.hatch(2.0, 0.0));
        assert!(!out.strokes.is_empty());
        assert!(!out.mask.is_empty());
    }

    #[test]
    fn overflow_edge_mode_allows_elements_past_border_and_captures_mask() {
        let region = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let params = FillParams::new(4.0).edge(EdgeMode::Overflow(3.0));
        let out = fill_pattern_output(&region, &params, |_, t| {
            let ring = t.circle((19.0, 10.0), 2.5);
            t.add_mask_ring(&ring);
            vec![ring]
        });
        // Circle at x=19 with r=2.5 reaches x=21.5 (inside 3.0 mm overflow limit).
        let max_x = out.strokes.iter().flatten().map(|p| p.0).fold(f64::MIN, f64::max);
        assert!(max_x > 21.0, "expected overflow past x=20, got {max_x}");
        assert!(out.mask.covers_point((21.0, 10.0)), "mask should cover overhanging lobe");
    }

    #[test]
    fn registry_dispatches_and_frays_across_families() {
        let region = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        for name in ["lines", "dots", "pebbles", "canopy", "gravel", "granite", "pat:ANSI31"] {
            let clean = fill_by_name(name, &region, &FillParams::new(3.0).seed(7)).unwrap();
            let frayed = fill_output_by_name(
                name,
                &region,
                &FillParams::new(3.0).seed(7).fray(6.0),
            )
            .unwrap();
            assert!(!clean.is_empty(), "{name} should produce strokes");
            assert!(
                length(&frayed.strokes) < length(&clean),
                "{name} with fray should have less total ink than unfrayed"
            );
            assert!(!frayed.mask.is_empty(), "{name} should export a non-empty mask");
        }
        assert!(fill_by_name("nonexistent_fill", &region, &FillParams::new(3.0)).is_none());
    }
}
