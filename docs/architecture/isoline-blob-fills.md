# Isoline blob fills (implementation notes)

**Product spec:** draw.land [isoline-blob-fills.md](../../../drawland/docs/architecture/isoline-blob-fills.md). **Reference images:** `drawland/docs/architecture/img/isoline-blob-fills/` (start with `reference-scattered.png`).

## What to build

A generator that:

1. Places **blob sites** (Poisson scatter or caller-supplied `Vec<Pt>`).
2. For each site, defines a local scalar field `h(p)` and extracts **several isolines** via `field::fill_isolines` (or equivalent), chained and clipped.
3. Builds **coverage** as the union of blob envelopes (outer support of each mass), clipped to the host `Region`.
4. Returns `FillWithCoverage { strokes, coverage }` (type name TBD).

## Primitives to use (machinery only)

| Piece | Module | Role |
| --- | --- | --- |
| Isoline extraction | `field::fill_isolines`, `levels_every` | Contour lines of `h` |
| Noise / warp | `noise::Perlin` | Blob shape and jitter |
| Scatter | `random::poisson_disk`, `poisson_disk_varying` | Site placement |
| Clip | `Region::clip_polyline`, `clip_strokes` | Host boundary |
| Boolean union | `Region::from_polygons` + region ops | Coverage merge |

Do not ship this as a thin wrapper around unrelated fills; behaviour is tuned against the reference drawings.

## API sketch

```rust
pub struct IsolineBlobParams {
    pub seed: u64,
    pub sites: SiteSource,
    pub contour_spacing: f64,
    pub level_count: usize,
    pub blob_radius: f64,
    pub warp: f64,
    pub light: Option<LightBias>,
    pub layout: Layout, // Scattered | Dense
}

pub enum SiteSource {
    Generate { min_spacing: f64 },
    Points(Vec<Pt>),
}

pub fn fill_isoline_blobs(region: &Region, p: &IsolineBlobParams) -> FillWithCoverage;
```

Add `fill_isoline_blobs_into` when stroke count warrants streaming.

## Coverage rule (pick one, document in tests)

Recommended: envelope = polygon where `h(p) ≥ h_min` for that blob, or largest closed isoline loop at the lowest level — must match mask expectations in draw.land compose tests.

## Gallery

New row in `examples/gallery.rs` with side-by-side intent: sparse sheet + dense patch, parameters documented for manual comparison to reference art.

## Bindings

Expose after Rust API stabilises: Python/Node/WASM same as other fills; coverage as optional second return or `Region` export.
