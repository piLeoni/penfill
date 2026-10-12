# Architecture index

Routing table for the penfill architecture knowledge base. Read only the
subsystem doc(s) a task touches, then confirm behaviour against the cited
source files. Source code is authoritative.

| Subsystem | Doc | Primary sources |
| --- | --- | --- |
| Geometry kernel | [geometry.md](geometry.md) | `src/geo.rs`, `src/sweep.rs`, `src/grid.rs` |
| Fills | [fills.md](fills.md) | `src/zigzag.rs`, `src/spiral.rs`, `src/concentric.rs`, `src/rectilinear.rs`, `src/tile.rs`, `src/litho.rs`, `src/nature.rs`, `src/textures.rs`, `src/growth.rs`, and the remaining `src/*.rs` fill modules |
| Isoline blob fills | [isoline-blob-fills.md](isoline-blob-fills.md) | `src/isoline_blobs.rs`, `src/isoline_stack.rs`, `src/field.rs`, `src/noise.rs` |
| Placement | [placement.md](placement.md) | `src/anchor.rs`, `src/placement.rs` |
| Primitives | [primitives.md](primitives.md) | `src/random.rs`, `src/noise.rs`, `src/field.rs` |
| Extensibility | [extensibility.md](extensibility.md) | `src/pattern.rs`, `src/tile.rs`, `src/pat.rs`, `src/svg.rs` |
| Streaming | [streaming.md](streaming.md) | `src/stream.rs`, `*_into` entry points |
| Bindings | [bindings.md](bindings.md) | `bindings/python/src/lib.rs`, `bindings/node/src/lib.rs`, `bindings/wasm/src/lib.rs`, `web/` |
| Tooling | [tooling.md](tooling.md) | `examples/gallery.rs`, `tools/plot_sheet.py`, `tools/plotfont.py` |

## Cross-cutting facts

- **Units**: millimetres everywhere; `Pt = (f64, f64)`, `Strokes = Vec<Vec<Pt>>`.
- **Public surface**: re-exports in `src/lib.rs` — `Anchor`, `Origin`,
  `Region`, `Strokes`, `polyline_length`, `fill_solid`, `Clipped`, `Meter`,
  `Sink`.
- **`MIN_SEGMENT_MM`** (`src/geo.rs`): strokes shorter than this are dropped
  as rounding artefacts, not ink.
- **Seeding**: every random fill takes `seed: u64`; the same seed gives the
  same drawing on every platform (no external RNG).

## Decisions

Recorded architectural decisions live in [decisions.md](decisions.md).
