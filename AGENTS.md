# penfill — agent entry point

Infill / traversal pattern generation for pen plotters, CNC toolpaths and
laser engravers. Give it a shape in millimetres, get back the polylines a
pen should draw to fill it. Rust core, with Python, Node and wasm bindings.

## Units and the core contract

Everything is **millimetres**. Every fill takes a [`Region`] and its own
parameters and returns open polylines (`Strokes = Vec<Vec<(f64, f64)>>`) in
draw order. Lines stay inside the region and go around its holes.

## Repository map

| Area | Where | Doc |
| --- | --- | --- |
| Geometry kernel: `Region`, clipping, insetting, routing | `src/geo.rs` | `docs/architecture/geometry.md` |
| Scanlines / hatching primitives | `src/sweep.rs`, `src/grid.rs` | `docs/architecture/geometry.md` |
| Lattice placement (`Anchor`) and region-relative placement | `src/anchor.rs`, `src/placement.rs` | `docs/architecture/placement.md` |
| The ~50 pattern fills | `src/*.rs` | `docs/architecture/fills.md` |
| Zero-noise primitives: RNG, Perlin, field/isolines | `src/random.rs`, `src/noise.rs`, `src/field.rs` | `docs/architecture/primitives.md` |
| User-supplied patterns and tiles | `src/pattern.rs`, `src/tile.rs`, `src/pat.rs`, `src/svg.rs` | `docs/architecture/extensibility.md` |
| Streaming large fills (`Sink`, `Clipped`, `Meter`) | `src/stream.rs` | `docs/architecture/streaming.md` |
| Binding layer (Python / Node / wasm) | `bindings/*/src/lib.rs` | `docs/architecture/bindings.md` |
| Gallery sheets, plot-sheet tooling | `examples/gallery.rs`, `tools/` | `docs/architecture/tooling.md` |

## Documentation rules

- `docs/architecture/INDEX.md` routes to subsystem docs. Read only the
  subsystem(s) a task touches, then confirm against the cited source files.
- Source code is authoritative. If a doc conflicts with the code, fix the doc.
- Update the minimal set of docs when a change alters a subsystem's purpose,
  entry points, data flow, public contracts, or invariants. See
  `.cursor/rules/knowledge-base.mdc`.

## Build and test

```sh
cargo test                              # core crate
cargo run --example gallery --release   # regenerate example/gallery/*.svg
```
