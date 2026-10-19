# Extensibility — user-supplied patterns, tiles and artwork

The paths by which a caller supplies their own geometry instead of choosing a
built-in fill. All share one contract: the engine clips to the region, the
caller does not.

## `pattern.rs` — your own function and the unified pattern contract

`fill_pattern(region, seed, anchor, draw)`, `fill_pattern_with(region, params, draw)`,
and `fill_pattern_output(region, params, draw) -> FillOutput { strokes, mask }`
call `draw` **once** with:

- `FillParams`: standardized parameters shared across patterns (`spacing`,
  `angle`, `seed`, `anchor`, `inset`, `edge: EdgeMode::{Clip, Inside, Overflow(mm)}`,
  `fray: f64`, `custom: CustomParams`).
- `Canvas`: `region` (after `inset`), `limit` (effective outer clipping envelope,
  outset by `mm` in `EdgeMode::Overflow(mm)`), `bounds`, `spacing`, `angle`,
  `seed`, `anchor`, `origin` (the resolved anchor point), `edge`, `fray`, and
  `custom` (`canvas.param(key, default)`).
- `Tools`: the standard library that built-in fills themselves are built with:
  - **Scoping**: `on(region)`, `anchored(anchor)`, `with_spacing(s)`,
    `with_edge(edge)`, `with_fray(f)`, `seeded(seed)`
  - **Random & Noise**: `rng()`, `rng_salted(salt)`, `cell_rng(i, j)`,
    `noise(x, y, octaves)`
  - **Boundary & Fray**: `whole_at(p)`, `whole_at_band(p, spacing, fray)`,
    `border_ramp(p, falloff, rough)`, `element_fits(centre, radius)`,
    `placement_field(pad)`, `claim_fray()` (marks that the pattern handles `fray`
    at the motif level so `run_pattern` clips without post-pass stroke chopping)
  - **Lattices & Fields**: `frame(angle)`, `grid(dx, dy, angle, stagger, pad, visit)`,
    `grid_cells(...)`, `hatch(spacing, angle)`, `profile(...)`, `isolines(...)`
  - **Scatter & Voronoi**: `scatter(spacing)`, `scatter_field(dense, sparse, field)`
    (bidirectional: `dense` and `sparse` can go either way),
    `voronoi_cells(centres, reach)`, `voronoi_edges(centres, reach)`
  - **Marks & Polylines**: `circle`, `lobe`, `arc`, `tick`, `cross`, `x_cross`,
    `smooth`, `resample`, `fray`
  - **Polygon Mask Export**: `add_mask_ring(&ring)`, `add_mask(&region)`,
    `set_mask_halo(radius)` — zero-cost no-op when only `Strokes` are requested
    (`fill_pattern_with` / `fill_by_name`), unioned into `FillOutput.mask` when
    `fill_pattern_output` / `fill_output_by_name` is called. Nested pattern calls
    inside `fill_pattern_output` automatically inherit the active mask collector.

The engine clips (and when `params.fray > 0` and `!tools.claim_fray()`, applies
universal geometry-adaptive edge fraying across long lines, discrete marks/dots,
and small closed rings) against `canvas.limit` at the end.

The unified pattern registry (`FILL_NAMES`, `all_fill_names()`,
`fill_by_name(name, region, params)`, `fill_output_by_name(name, region, params)`)
dispatches all built-in geometric, lattice, scatter, nature, texture, isoline,
`litho::BUILTIN`, and `pat:<NAME>` patterns through this contract.

**Invariant** (`built_in_fills_compose_whole_or_per_piece`): a built-in fill
used through `fill_pattern` must equal the direct fill, whether drawn whole or
per piece.

## `tile.rs` — any drawing repeated like wallpaper

`fill_tile(region, tile, width, height, angle, row_shift, anchor)`. The tile
is polylines in mm laid out in a `width × height` box whose lower-left corner
is the origin; copies go on a grid, each row shifted by `row_shift × width`
(0.5 = brick bond), the whole grid turned by `angle`. Lines crossing the tile
box are fine — they overlap the neighbour, as in a CAD hatch (the `spill`
computation widens the sampling span to allow for it).

## `pat.rs` — CAD `.pat` hatch patterns

The AutoCAD/LibreCAD/QCAD `.pat` format: a list of line families, each
`angle, x0, y0, dx, dy [, dash1, dash2, …]`. Dashes alternate pen down
(positive), pen up (negative), dots (0); no dashes = solid. `*NAME,
description` starts a pattern; `;` is a comment.

- `parse_pat(text)` parses any `.pat` text; malformed lines are skipped.
- `builtin(name)` returns one of `BUILTIN` (14 names: BRICK, HERRINGBONE,
  BASKET, PARQUET, ANSI31–37, EARTH, CROSS, DASH, STAIRS, GRATE, ANGLE).
  **The vendor definitions are derived here from the geometry, not copied from
  a vendor file.**
- `fill_pat(region, pattern, scale, angle, anchor)`.

**Placement invariant**: a `.pat` hatch is drawn from the **page origin**, so
its local placement is already the page one; `anchor.shift` moves it. This is
why `.pat` is excluded from the local-shift test in `anchor.rs`. Zero-length
dashes are dots, drawn as a 0.1 mm tick; families with `delta.1 ≈ 0` are
skipped (no across-spacing → nothing to repeat).

## `svg.rs` — bring your own artwork

- `parse_svg(...) -> SvgDrawing { strokes, view_box }`. Only geometry is read:
  `path`, `line`, `polyline`, `polygon`, `rect`, `circle`, `ellipse`, with
  nested `transform`s. Curves are flattened to short segments. Fills, stroke
  widths, text and images are **ignored** — a plotter only draws lines.
  Coordinates keep SVG's y-down convention.
- `SvgDrawing::fit_width(width_mm)` scales the view box and moves its corner to
  the origin.
- `fill_svg_tile(...)`: use the drawing as a repeating hatch tile.
- `fill_svg_placed(...)`: place it with a `Placement` (CSS-background style).
- `Region::clip_strokes(...)`: cut artwork as it is.

## `dither.rs` — pictures as pen work

A greyscale image laid over the region and dithered on a grid of `resolution`
mm along a line and `line_spacing` mm between lines. A plotter can't draw
grey, so every cell is inked or not.

- Dark cells become marks two ways: `Marks::Dots` (a short tick per dark cell
  — a pen needs a stroke to leave a dot) or `Marks::Lines` (adjacent dark
  cells along a line join into one stroke and lines run back and forth —
  line-engraving look, far fewer pen lifts).
- The picture is placed like a CSS background (`Placement`): by default it
  covers the region's bounds, centred. Where no copy falls there is paper, and
  everything is cut by the outline.
- Image row 0 is the top (smallest y, SVG convention); `Greyscale::flipped()`
  for y-up coordinates.
- `Greyscale::decode` is behind the `image` feature (default on) and handles
  PNG/JPEG/GIF/BMP/WebP.
