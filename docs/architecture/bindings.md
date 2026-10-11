# Bindings

The Rust crate is the engine; three bindings expose it, plus a web playground.
All three are workspace members (`Cargo.toml` `[workspace] members`), all are
**thin** translations over `penfill_core` with no logic of their own.

## Naming and dependency layout

| Crate | Package name | Depends on core via |
| --- | --- | --- |
| core | `penfill` | — |
| Python | `penfill-python` (lib name `penfill`) | `penfill-core = { package = "penfill", path = "../.." }` |
| Node | `penfill-node` | `penfill-core = { package = "penfill", path = "../.." }` |
| wasm | `penfill-wasm` | `penfill = { path = "../..", default-features = false }` |

The wasm binding disables default features (no `image`), so `Greyscale::decode`
is unavailable there; the profile sets `opt-level = "s"` for size.

## Per-binding conventions

- **Python** (`bindings/python/src/lib.rs`): points come in as any sequence of
  `(x, y)` pairs (tuples, lists, numpy rows), go out as lists of `(x, y)`
  tuples. `Region` is a `#[pyclass]` (frozen, `Clone`) wrapping
  `CoreRegion`. Stubs in `penfill.pyi`; tests in `tests/` (pure Python, no
  Rust build) plus `test_plot_sheet.py` for the tool.
- **Node** (`bindings/node/src/lib.rs`): points are `[x, y]` arrays, strokes
  are arrays of points, all mm. Required parameters are positional; the rest
  go in an options object. Types in `index.d.ts`.
- **wasm** (`bindings/wasm/src/lib.rs`): minimal surface, rings as flat
  `[x0, y0, x1, y1, …]` arrays, **SVG out** — it exists to drive the web
  playground. Consumed by `web/` (Vite + React; `usePenfill.ts`).

## Adding a fill to a binding

A new core fill is not automatically exposed. Each binding lists its API by
hand; add the wrapper in every binding that should carry it, and add a stub /
type. Keep the wrapper free of logic — call the core, translate types.

## Release

`.github/workflows/ci.yml` builds/tests; `release.yml` publishes. The
Python binding ships as a prebuilt wheel for Linux/macOS/Windows for every
Python ≥ 3.9 (nothing to compile for the user). `docs/RELEASE.md` covers the
release steps.
