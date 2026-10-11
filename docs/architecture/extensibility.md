# Extensibility — user-supplied patterns, tiles and artwork

The paths by which a caller supplies their own geometry instead of choosing a
built-in fill. All share one contract: the engine clips to the region, the
caller does not.

## `pattern.rs` — your own function

`fill_pattern(region, seed, anchor, draw)` calls `draw` **once** with:

- `Canvas`: `region`, `bounds`, `seed`, `anchor`, and `origin` (the resolved
  anchor point). The pattern returns lines anywhere on the page.
- `Tools`: the pieces the built-in fills are made of, already set to the
  canvas region and seed — `noise(x, y, octaves)`, `scatter(spacing)`,
  `scatter_field(dense, sparse, field)`, `hatch(spacing, angle)`. A pattern
  can ignore it. `Tools::on(region)` re-points the same tools at a piece or an
  inset; `Tools::anchored(anchor)` changes the lattice placement.

The engine keeps the parts inside the shape with **one** `clip_strokes` over
the whole region. Splitting the region into pieces costs time, so it only
happens when the pattern asks for `Region::parts`.

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
