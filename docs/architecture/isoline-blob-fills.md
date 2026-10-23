# Isoline blob fills (implementation notes)

**Product spec:** draw.land [isoline-blob-fills.md](../../../drawland/docs/architecture/isoline-blob-fills.md). **Reference images:** `drawland/docs/architecture/img/isoline-blob-fills/`.

**Status:** two fills. `src/isoline_blobs.rs` — flat contours of one 2D field. `src/isoline_stack.rs` — 3D stacked slices with overhang and boundary modes (see "Stacked slices" below).

## What it is

**One continuous scalar field over the whole region**, sliced into many isolines in a single `field::fill_isolines` call. Marching squares merges contours wherever two nearby basins overlap above `threshold`, so neighbouring masses **fuse** into one lobed silhouette for free — no per-site generation, no explicit union of separate shapes.

An earlier version built an independent height field *per site* and unioned their outlines for coverage; that produced stamped, overlaid shapes that never read as one connected mass and did not match the reference art. Don't reintroduce that shape — the single-field approach is load-bearing for the "clusters fuse, not overlay" requirement.

```rust
h(p) = fbm(warp(p) / feature_size)                          // ambient texture
     + Σ sites: weight · exp(-|p - site|² / radius²)        // optional known positions
```

`warp(p)` samples `p` offset by a second independent noise field before the ambient lookup — this is the jitter, applied to the field's geometry rather than to each stroke after the fact.

## API

```rust
pub enum FieldSource {
    Noise,                      // procedural only
    Sites(Vec<FieldSite>),      // only given positions raise the field
    NoiseAndSites(Vec<FieldSite>),
}

pub struct FieldSite { pub at: Pt, pub weight: f64, pub radius: f64 }

pub struct IsolineFieldParams {
    pub seed: u64,
    pub source: FieldSource,
    pub feature_size: f64,     // mm; ambient wavelength / cluster spacing
    pub contour_spacing: f64,  // mm between nested rings
    pub warp: f64,             // domain-warp strength
    pub threshold: f64,        // ink begins here; lower = denser/more fused
}

pub struct FillWithCoverage { pub strokes: Strokes, pub coverage: Region }

pub fn fill_isoline_blobs_with_coverage(region: &Region, p: &IsolineFieldParams) -> FillWithCoverage;
pub fn fill_isoline_blobs(region: &Region, feature_size: f64, contour_spacing: f64, seed: u64) -> Strokes;
pub fn default_params(feature_size: f64, contour_spacing: f64, seed: u64) -> IsolineFieldParams;
```

## Primitives used

| Piece | Module | Role |
| --- | --- | --- |
| Isoline extraction | `field::fill_isolines`, `levels_every` | Contours of `h`, and of coverage at a single level |
| Noise / warp | `noise::Perlin` | Ambient texture + domain warp |
| Boolean intersect | `Region::intersection`, `Region::from_polygons` | Clamp coverage to host |

No `random::poisson_disk` — site placement for the procedural case comes from where the noise field itself peaks, not a separate scatter step.

## Coverage rule (implemented)

`coverage_from_threshold`: a second `fill_isolines` call at the single level `threshold`, keeping only **closed** loops (`first == last`), unioned via `Region::from_polygons`, intersected with the host. This is exactly the silhouette a cartographic contour map's lowest line traces around connected high ground — reused here as the mask boundary.

## Tests (`src/isoline_blobs.rs`)

- `noise_field_produces_nested_lines` — sanity: output exists, stays inside host.
- `lower_threshold_grows_and_fuses_coverage` — density/connectivity knob behaves monotonically.
- `two_close_sites_fuse_into_one_loop` / `far_sites_stay_separate` — the fuse-vs-stamp behaviour, asserted directly on `coverage.all_rings_mm().len()`.

## Stacked slices (`src/isoline_stack.rs`)

Models each tree crown from above as a 3D cluster of overlapping, jagged foliage tiers and sub-boughs rather than Wyvill metaball bubbles or concentric parallel offsets:

- **Asymmetric multi-bough structural envelope (`BoughLobe`)**: each crown generates 3–5 asymmetric primary boughs with flat-crested `(1 - u^4)` plateaus, uneven angular widths, and V-clefts that recede at different elevation cutoffs `max_u`, plus same-elevation crown fusion (`SpatialOccluder`) and companion sub-crowns so touching trees merge into kidney-, L-, and sprawling grove silhouettes rather than round polka-dot bubbles.
- **Categorized jittering — Period vs Intensity (`build_jagged_ring_layer`)**:
  - **Jitter Period (`jitter_period`, custom `"jitter_period"` / `"period"`)** controls *how many control points* (`n_ctrl`) around each contour are sampled and displaced (`step_mm ∝ crown * jitter_period`), as well as the spatial wavelength in 2D Perlin space.
  - **Jitter Intensity (`jitter`, custom `"jitter"` / `"jitter_intensity"` / `"intensity"` / `"jaggedness"`)** controls *how far* those control points are displaced (combining flat-crested outward FBM facets, deepened inward notches, discrete ridge kicks, and tangential shifts), connected with a single asymmetric bevel pass (`0.08` on inward notches, `0.22` on outward facets) and zero sinusoidal bubble waves.
- **Directional sunlight & 3D C-arc billows (`Light`, enabled by default)**: `IsolineStackParams::default()` enables `Light { from_deg: -45.0, strength: 0.82, elevation_deg: 35.0, hatch: 0.0 }`. Upper tiers drift toward the shadowed flank (`(-lx, -ly)`) and open into cascading C-arcs on the sunlit flank (`is_open_arc` + `facing_sun > thresh`, stopping at `rad_scale >= 0.22` to avoid pinhole center loops), plus 1–2 broad internal bough-crease arcs.
- **Optional perpendicular slope ticks (`ticks` / `Light.hatch`, `append_perpendicular_ticks`)**: short cross-contour hachure strokes along the local inward normal at bough cleft cusps and shadowed slopes, stopping ~70–82% across the inter-tier gap so `deoverlap_strokes` keeps them crisp.
- **Gradual grove-to-clearing scattering (`generate_forest_layers`)**: Poisson-disk candidates are ranked by a multi-scale grove field + stochastic scatter with transition width scaled by `coverage.sqrt()`, tapering both acceptance probability and crown radius (`size_factor`) from dense grove cores (`coverage = 1.0`) down to sparse 10% pioneer groves (`coverage = 0.10`).
- **Fast $O(1)$ analytical LUT occlusion + spatial de-overlap (`SpatialOccluder`, `deoverlap_strokes`)**: each `JaggedLayer` precomputes a 144-bin polar radius lookup table (`lut`) indexed in a 2D uniform grid for $O(1)$ hidden-line removal and same-level crown fusion (with 4-step bisection at occlusion crossings), followed by a spatial-hash `deoverlap_strokes` pass and greedy nearest-endpoint ordering for fast pen-plotter traversal.
- **Watertight closed `coverage` mask (`FillWithCoverage.coverage` / `FillOutput.mask`)**: even when outer perimeters (`is_base_footprint`) have sunlit highlight breaks in `out.strokes`, `out.coverage` is unioned from the un-gapped 360° closed `layer.ring` footprints (`Region::from_polygons`), guaranteeing a 100% closed polygon surface for DrawLand / downstream layer clipping (and can also be stroked directly if a continuous 360° outer silhouette is desired).

### Parameters & `CustomParams` keys

`IsolineStackParams` and `FillParams.custom` (`fill_isoline_stack_params`) support:
- `crown` (`"crown"` or `FillParams.spacing`): typical crown radius in mm.
- `bubble` (`"bubble"`): leaf-cluster scale in mm.
- `coverage` (`"coverage"`): `0.0..=1.0` share of the region covered by crowns (`0.10` = 10% sparse trees, `1.00` = 100% continuous canopy).
- `slices` (`"slices"`): vertical contour tiers (`2..=18`, default `9`).
- `jitter` (`"jitter"`, `"jitter_intensity"`, `"intensity"`, or `"jaggedness"`): jitter displacement intensity (`0.0..=2.5`, default `1.10`).
- `jitter_period` (`"jitter_period"` or `"period"`): jitter spatial wavelength / point spacing (`0.2..=4.0`, default `0.68`; smaller = more points jittered).
- `elongation` (`"elongation"`): crown elliptical anisotropy (`0.0..=1.0`, default `0.18`).
- `lobes` (`"lobes"`): depth and prominence of major bough clefts and compound sub-crowns (`0.0..=2.5`, default `1.0`).
- `ticks` (`"ticks"`, `"perpendicular"`, or `"hachures"`): density of perpendicular cross-contour slope ticks (`0.0..=1.5`, default `0.42`).
- `light` (`"light"` strength `0.0..=1.0`, default `0.82`; `"light_angle"` degrees, default `-45.0`).

Tests: `draws_inside_the_region`, `inside_mode_never_touches_the_boundary`, `overflow_mode_crosses_the_boundary_but_not_far`, `light_makes_the_shadow_side_denser_than_the_lit_side`, `lit_side_has_more_gaps_in_the_rings`, `a_higher_wider_bubble_hides_the_rings_under_it`, `custom_params_control_jaggedness_and_coverage`, `coverage_mask_is_closed_and_covers_crowns_despite_open_sunlit_rims`.

## Preview

`examples/isoline_blobs_demo.rs` (`cargo run --release --example isoline_blobs_demo`) writes `isoline-blobs-scattered.svg`, `isoline-stack-{clip,inside,overflow,dense,lit}.svg`, `forest-canopy-showcase.svg`, `forest-variations-density-light.svg`, and `forest-variations-species-mask.svg` to `examples/gallery/`.

