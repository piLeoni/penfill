# Tooling — gallery sheets and the plot-sheet tool

Everything that turns the library into viewable/printable output. None of it
is part of the library's public Rust API.

## Gallery sheets — `examples/gallery.rs`

`cargo run --example gallery --release` (requires the `image` feature) generates
the complete vector reference gallery in `examples/gallery/*.svg` and the
README illustration strips in `docs/img/*.svg`.

### Declarative catalog (`catalog()`)

Instead of per-pattern closures, `examples/gallery.rs` defines a compact
declarative table (`catalog() -> Vec<Pattern>`) covering all 99 catalog entries
grouped into 8 `Category` families (`Lines`, `Lattices`, `Scatter`, `Materials`,
`Organic`, `Geology`, `Ground`, `Cad`). Rendering dispatches through
`penfill::fill_by_name` and `penfill::fill_output_by_name` using `FillParams`.

### Clean test shapes (`gallery_shapes()`)

All gallery sheets and README strips use smooth, generous `52 × 52 mm` test
geometries so patterns read clearly without spiky polygon artifacts:
- `square` — `52 × 52 mm` square
- `card` — `52 × 52 mm` rounded rectangle (`r = 5 mm`, 64 vertices)
- `ring` — smooth 96-vertex circular ring (`R = 26 mm`) with a 64-vertex inner hole (`r = 9 mm`)
- `island` — smooth 96-vertex harmonic island contour (`R ≈ 25 mm`)
- `pond_island` — smooth organic island with an interior pond hole
- `star` — clean 5-pointed star (`R = 27 mm`, `r = 13.5 mm`)
- `archipelago` — two-island multi-polygon region with a hole

### Generated sheets (`examples/gallery/`)

1. **Per-pattern sheets (`examples/gallery/<slug>.svg`)**:
   - 3 rows (`0.9 mm`, `1.6 mm`, `2.6 mm`, paired with `0° / +20° / +45°` for angled fills)
   - 4 columns:
     1. `square (unfrayed)` — solid wall border
     2. `ring + hole (unfrayed)` — solid wall border around outer ring and hole
     3. `frayed (with guide)` — organic island with `fray = 8.5 mm` and a dashed boundary guide
     4. `frayed (ink only)` — the same frayed fill with no boundary line, floating on paper
2. **Categorized visual reference (`examples/gallery/gallery.svg`)**:
   - Vertical 7-column poster grouped by `Category`, showing each pattern side-by-side on a crisp square and on a frayed organic contour (`fray = 8.5 mm`) with its name and description underneath.
3. **Alphabetical index (`examples/gallery/all-patterns.svg`)**:
   - 10-column alphabetical grid of all 99 catalog patterns.
4. **Universal edge fraying showcase (`examples/gallery/fray.svg`)**:
   - Compares `fray = 0 mm`, `fray = 4.5 mm`, `fray = 9.0 mm (with guide)`, and `fray = 9.0 mm (ink only)` across 12 patterns spanning continuous lines, flow streamlines, discrete marks, botanicals, and closed Voronoi cells.
5. **Boundary modes & polygon masks showcase (`examples/gallery/edges-and-masks.svg`)**:
   - Compares `EdgeMode::Clip`, `EdgeMode::Inside`, `EdgeMode::Overflow(3.5)`, `FillOutput.mask` polygon silhouettes, and multi-layer background knockouts (`below.difference(&out.mask)`).
6. **Specialized sheets (`cad.svg`, `custom.svg`, `dither.svg`, `placement.svg`)** and **README strips (`docs/img/*.svg`)**.

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
