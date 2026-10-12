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

The flat field above can only nest: every level sits inside the one below, so nothing ever overhangs. Vegetation needs upper layers that are **wider** than what is under them and hide it. `isoline_stack` gets that from a 3D model:

- Each crown is a dome of spheres (Wyvill metaballs, `(1 - d²/r²)³`, so touching spheres melt into one lumpy surface): higher and larger toward the middle. `bubble` sets sphere size (lumpiness), `crown` the crown size.
- The volume is cut into `slices` horizontal planes. Each slice's outline is one `fill_isolines` call at `ISO` over a frame around all spheres; per slice only the spheres it cuts are bucketed in a hash grid.
- Painter's order, top slice first: a slice's outline is clipped to *outside* the union of the slices above it (`frame.difference(&above)`), then the slice joins `above` (`Region::from_rings_even_odd`, `Region::union`). A crown standing higher than its neighbour hides that neighbour's rings where it overhangs.
- Jitter: the sample is domain-warped and the field multiplied by `1 + jitter·noise`, both noises shifted with `z` so the wobble changes from slice to slice.

`EdgeMode`:

| Mode | Placement | Guarantee |
| --- | --- | --- |
| `Clip` | crowns over the region grown by one crown | strokes cut at the boundary |
| `Inside` | crowns in the region plus a lining row `crown · 0.55` inside every ring (so narrow woods fill); spheres shrunk to fit | field faded to zero **at** the boundary: every outline closes inside, none is cut |
| `Overflow(m)` | plus a row of crowns standing on every boundary ring | field faded to zero `m` mm **outside**: outlines close within `m` past the border |

The fade (`Fade`) is a coarse grid of signed boundary distances, interpolated and smoothstepped over `bubble · 0.35`; it is evaluated at the *unwarped* sample, so the wobble cannot carry ink past the limit. Spheres are first shrunk so their support radius is at most `room / 0.95` (`fit_to_limit`): each one rounds off well before the limit and the fade only catches what merging pushes further. Letting spheres reach the limit makes neighbours merge into one outline pressed flat along the border, which reads as a cut.

`coverage` (0–1) thins generated crowns with low-frequency noise into woods and clearings; `CrownSource::Crowns` takes known crowns (position, radius, height) instead. The returned `coverage` region is the union of every slice — the canopy's footprint, for masks.

Tests: `draws_inside_the_region`, `inside_mode_never_touches_the_boundary`, `overflow_mode_crosses_the_boundary_but_not_far`, `a_higher_wider_bubble_hides_the_rings_under_it`.

Cost: `slices` × (frame area / cell²) field samples; ~4 s for a 120 mm park in release.

### Light (implemented)

`IsolineStackParams.light: Option<Light>` — `Light { from_deg, strength, elevation_deg, hatch }` (`Default`: 135°, 0.8, 35°, 0.4 mm). Two effects. First, sphere bias: one direction for the whole fill: in `crown_balls`, each satellite sphere's radius and height scale by `1 − shade · k` where `shade = strength · dot(unit offset from the crown's centre, light direction)`. Spheres on the far side from the light (`shade < 0`) swell and climb; spheres on the near side thin and flatten. The trunk sphere sits at the crown's own centre, so its offset is zero and it is never shaded. No change to sampling, slicing o`from_deg`: degrees, 0 = +x, 90 = +y, the direction light comes *from*. `strength`: 0–1 (clamped); 0.65 on radius, 0.5 on height at `strength = 1`. Test: `light_makes_the_shadow_side_denser_than_the_lit_side` (run with `hatch: 0`).

Second, shading (`hatch > 0`), via `Surface` in `isoline_stack.rs`: a heightfield on a grid of `(0.2·bubble).clamp(0.12, 0.5)` mm, where each cell takes the height of the topmost slice whose field covers it (`mark`, called per slice) and uncovered cells sit at ground level. `light` box-blurs it twice (radius `0.35·bubble`) so the terraces become a surface, then gives each cell Lambert darkness relative to a flat top under a sun at `elevation_deg`, plus cast shadows: marching toward the sun for up to `3·bubble`, fading with distance and kept half-tone so gaps between crowns don't go black. `hatching` draws lines at `from_deg − 45°`, `hatch` apart, anchored at the origin. Each row has its own darkness threshold (4-row ordered dither), so darker shade gets more rows; a crossing family joins in the deepest shade. `break_highlights` cuts outline runs where the surface faces the sun well above a flat top's light. Test: `shading_hatches_the_side_away_from_the_light`.

 it (a bulge that swells past the midline would otherwise get counted on the wrong side).

## Preview

`examples/isoline_blobs_demo.rs` (`cargo run --release --example isoline_blobs_demo`) writes `isoline-blobs-scattered.svg` and `isoline-stack-{clip,inside,overflow,dense,lit}.svg` (an irregular park with a pond hole) to `examples/gallery/`. Copies live in draw.land's docs.

## Open follow-ups

- Light on `isoline_blobs` (the flat field) — not implemented; only `isoline_stack` has `Light` today.
- Streaming (`_into` + `Sink`) — not implemented; whole-field sampling today, fine at map-fill scales tried so far, revisit if a host area makes memory a concern.
- Bindings (Python/Node/WASM) — not exposed yet; follow the pattern of other `field`-based fills (`textures::fill_marble`) once the Rust API is considered stable.

## Gallery

Row added in `examples/gallery.rs` under slug `isoline-blobs` (uses `default_params`); regenerate with `cargo run --example gallery --features image`.
