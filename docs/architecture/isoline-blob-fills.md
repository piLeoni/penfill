# Isoline blob fills (implementation notes)

**Product spec:** draw.land [isoline-blob-fills.md](../../../drawland/docs/architecture/isoline-blob-fills.md). **Reference images:** `drawland/docs/architecture/img/isoline-blob-fills/`.

**Status:** implemented in `src/isoline_blobs.rs`, exported as `fill_isoline_blobs` / `fill_isoline_blobs_with_coverage`.

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

## Preview

`examples/isoline_blobs_demo.rs` (`cargo run --example isoline_blobs_demo`) writes scattered/dense/sites SVGs to `examples/gallery/`; `isoline-blobs-scattered.svg` is the one copied into draw.land's docs as `example-generated.png`.

## Open follow-ups

- Light bias (tilt `threshold`/level spacing by a direction) — not implemented.
- Streaming (`_into` + `Sink`) — not implemented; whole-field sampling today, fine at map-fill scales tried so far, revisit if a host area makes memory a concern.
- Bindings (Python/Node/WASM) — not exposed yet; follow the pattern of other `field`-based fills (`textures::fill_marble`) once the Rust API is considered stable.

## Gallery

Row added in `examples/gallery.rs` under slug `isoline-blobs` (uses `default_params`); regenerate with `cargo run --example gallery --features image`.
