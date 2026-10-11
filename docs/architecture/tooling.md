# Tooling — gallery sheets and the plot-sheet tool

Everything that turns the library into viewable/printable output. None of it
is part of the library's public Rust API.

## Gallery sheets — `examples/gallery.rs`

One SVG per pattern (`examples/gallery/<pattern>.svg`), plus `gallery.svg`,
the comparison table. Vector, so lines stay sharp when zoomed. These are the
images the README links.

- `cargo run --example gallery --release` regenerates them. The example
  requires the `image` feature (`[[example]] required-features = ["image"]` in
  `Cargo.toml`).
- Three styles, each pairing a spacing with an angle: `FINE_MM = 0.8`,
  `MID_MM = 1.6`, `COARSE_MM = 2.8`; border is half the spacing so the coarse
  row sits further off the wall. Angle steps are 0 / +20 / +45 — a hex grid
  repeats every 60°, so 60° and 90° would just redraw the rows above.
- `const SEED: u64 = 7` so the scattered fills don't change between runs.
- Shapes (`shapes()`): square, doughnut, blob, star, and a deliberately
  awkward "worst" outline to stress the fills.
- Palette (`PAPER`, `WALL`, `INK`, `LABEL`) is chosen to read in print without
  primary colours.
- The full fill surface is imported at the top of the file — this example is
  also the crate's broadest compile-time integration test of the public API.

The `examples/gallery/*.svg` and `docs/img/*.svg` outputs are committed for
the README; regenerate them when a fill's geometry changes.

## Plot-sheet tool — `tools/plot_sheet.py`

Draws a page of penfill patterns side by side into one SVG, ready for a pen
plotter; each pattern gets a labelled rectangle. Uses the **Python binding**,
all units mm.

```sh
pip install penfill
python tools/plot_sheet.py tools/plot_sheet.example.json -o sheet.svg
```

- Config is JSON; only `patterns` is required. `tools/plot_sheet.example.json`
  is the 152.4 × 101.6 mm (6 × 4 in, A5 landscape) reference.
- Each entry names a fill and passes the arguments it takes (`spacing`,
  `angle`, `seed`, `mark`, `lattice`, `cell`, `pat`, `scale`, `line_width`,
  `size`, `hatch`, `centre`). **An unknown key is an error**, so a typo is
  caught instead of silently ignored.
- Labels are drawn with a single-stroke font (`tools/plotfont.py`) as plain
  pen paths, not `<text>`.
- Every cell is its own Inkscape layer in reading order, named `1 zigzag  1
  mm`, so AxiDraw (or any layer-by-layer plotter) finishes one cell before
  starting the next.
- `tools/README.md` is the full key/default table; keep it in sync when adding
  a config key.

## CI

`.github/workflows/ci.yml` runs the Rust tests and the Python binding tests.
Python tests include `test_penfill.py` (binding) and `test_plot_sheet.py`
(tool output), so a change that alters tool output surfaces there.
