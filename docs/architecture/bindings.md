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
  `CoreRegion`; `FillOutput` wraps `{ strokes, mask: Region }` (and supports
  unpacking `strokes, mask = out`). Exposes `fill(region, name, ...)` and
  `fill_output(region, name, ...)` over `FILLS` alongside dedicated per-pattern
  functions, all supporting `inset`, `edge` (`"clip"`, `"inside"`,
  `("overflow", mm)`), `fray`, and `with_mask`. Stubs in `penfill.pyi`.
- **Node** (`bindings/node/src/lib.rs`): points are `[x, y]` arrays, strokes
  are arrays of points, all mm. Required parameters are positional; the rest
  go in `FillOptions` (`angle`, `seed`, `inset`, `edge`, `overflow`, `fray`,
  `anchor`, `shift`). Exposes `fill`, `fillOutput`, `fills()`, `pattern`,
  `patternOutput`, and dedicated fill functions. Types in `index.d.ts`.
- **wasm** (`bindings/wasm/src/lib.rs`): minimal surface, rings as flat
  `[x0, y0, x1, y1, …]` arrays, **SVG out** — delegates directly to
  `penfill::fill_by_name` and `penfill::all_fill_names()`, supporting `inset`,
  `fray`, and `edge` parameters. Consumed by `web/` (Vite + React).

## Adding a fill to a binding

Registering a new fill in `src/pattern.rs` (`FILL_NAMES` and `fill_named`)
automatically makes it available in `fill` / `fill_output` (Python), `fill` /
`fillOutput` / `fills()` (Node), and `render` / `fillNames()` (wasm). Dedicated
top-level convenience wrappers can also be added to `bindings/python/src/lib.rs`
(`penfill.pyi`) and `bindings/node/src/lib.rs` (`index.d.ts`).

## Release

`.github/workflows/ci.yml` builds/tests; `release.yml` publishes. The
Python binding ships as a prebuilt wheel for Linux/macOS/Windows for every
Python ≥ 3.9 (nothing to compile for the user). `docs/RELEASE.md` covers the
release steps.
