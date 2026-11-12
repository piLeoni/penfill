# Tooling — gallery sheets and the plot-sheet tool

Everything that turns the library into viewable/printable output. None of it
is part of the library's public Rust API.

## Gallery sheets — `examples/gallery.rs`

`cargo run --example gallery --release` (requires the `image` feature, on by
default) draws every picture in `README.md` (`docs/img/*.svg`) and the sheets
in `examples/gallery/`.

**The pattern pictures come from the catalogue itself**, so they cannot drift
from the library: the material strips iterate `catalog::MATERIALS` by
`Group`, the generator strip iterates `catalog::generators()`, the symbol
strip iterates `catalog::marks::NAMES`, and every swatch is drawn with
`catalog::fill` / `fill_output`. A name or option the catalogue no longer
knows panics with the catalogue's own message instead of leaving a stale
image. Hand-picked lists remain only for illustrations (the options strip,
fray and edge examples, the tour), and those also go through `catalog::fill`.

Swatches: each pattern is drawn at its **default spacing** on a rounded card
about 14 spacings across (`swatch_side`, 14–70 mm), so every swatch shows the
same number of elements on screen; ring-following patterns get an island with
a pond instead. The ink is a 0.25 mm pen at the swatch's scale, clamped to
what reads on screen. Coordinates are written in sheet units with one decimal
and points closer than half a unit to the last one dropped, so the README
strips stay a few hundred KB (the busiest, vegetation and generators, about
1 MB). Everything is drawn y-down (page coordinates), so upright symbols stand
up and `light_angle` reads as on a page.

### Outputs

- `docs/img/{water,vegetation,ground,rock,built,sky,plain}.svg`: every
  material of each group (sky: `clouds` and three variations), rows of at
  most six, balanced.
- `docs/img/generators.svg` (every generator at its defaults),
  `docs/img/options.svg` (a dozen option settings), `docs/img/symbols.svg`
  (every mark in the symbol library).
- `docs/img/{hero,tour,shapes,fray,edges-and-masks,cad,dither,placement,custom}.svg`:
  the feature strips (`hero`: `serpentine` coloured per pen-down against the
  stroke count of `lines`; `cad`: `tile` with `pat`; `custom`: isolines, a
  profile, `scatter` and `tile` with your own `motif`, an SVG tile).
- `examples/gallery/gallery.svg`: the whole catalogue, a section per group
  with every material's description, then the generators, the options and
  the symbols.
- `examples/gallery/{fray,edges-and-masks,cad,custom,dither,placement}.svg`:
  row-and-column sheets of variations across the 52 mm test shapes
  (`square`, `card`, `ring`, `island`, `pond_island`, `star`, `archipelago`).

Other demos write their own files into `examples/gallery/`:
`hand_demo` (`hand-demo`, `doodle-trees`, `curl-waves`), `clouds_demo`
(`clouds-variations`), `growth_demo` (`growth-*`), `isoline_blobs_demo`
(`isoline-*`, `forest-*`). Delete a file there only if nothing writes it.

Check the output by rasterizing it: `rsvg-convert -w 1600 -b white x.svg -o x.png`.

## Catalogue review sheet — `examples/catalog_sheet.rs`

A contact sheet for working on patterns. With no pattern arguments it draws
every material and every generator at its defaults; otherwise a pattern name
then `key=value` options per cell, `|` between cells (values parse as
numbers, `a,b` lists, `true`/`false`, or words; `spacing`, `angle`, `seed`,
`fray`, `inset`, `edge` set the standard fields). Each cell is labelled with
its options, stroke count and time; an error is printed in the cell.

```sh
cargo run --release --example catalog_sheet -- out.svg
cargo run --release --example catalog_sheet -- out.svg hatch cross=2 '|' hatch dash=0.8,0.4 dash_jitter=0.6
cargo run --release --example catalog_sheet -- out.svg --shape=blob --global --cell=40 --cols=4 --pen=0.3
cargo run --release --example catalog_sheet -- --json   # catalog::describe()
```

## Plot-sheet tool — `tools/plot_sheet.py`

Draws a page of `penfill` patterns side by side into one SVG, ready for a pen
plotter; each pattern gets a labelled rectangle. Uses the **Python binding**,
all units mm.

```sh
pip install penfill
python tools/plot_sheet.py tools/plot_sheet.example.json -o sheet.svg
```

- Config is JSON; only `patterns` is required. `tools/plot_sheet.example.json`
  is the 152.4 × 101.6 mm (6 × 4 in, A5 landscape) reference.
- Each entry names a binding function (`getattr(penfill, name)`) and passes the arguments it takes (`spacing`,
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
