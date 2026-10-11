# Fills

The bulk of the crate: ~50 pattern fills, one public `fill_*` per pattern,
each taking a `Region` (and its own parameters) and returning `Strokes` in
draw order. All clip to the region and go around holes.

Grouped by how they are built, because that determines which doc/code to read
when changing one.

## Direction-based (built on `sweep.rs`)

Parallel scanlines cut to the region — the first step of most lattice fills.

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `rectilinear.rs` | `fill_rectilinear` | one stroke per piece, unjoined; inset from walls |
| `grid.rs` | `fill_lines`, `fill_grid`, `fill_triangles` | one/two/three sweep families |
| `chess.rs` | `fill_chess` | dark squares outlined + hatched diagonally |
| `honeycomb.rs` | `fill_honeycomb` | hexagon walls, each drawn once, `line_width` gap |
| `radial.rs` | `fill_sunburst` | rays from the anchor, staggered starts so spacing stays even |

## Shape-following (built on inset contours)

These consume the region's inset contours and have **no anchor**.

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `concentric.rs` | `fill_concentric` | outer wall inset repeatedly; centreline where a part is too thin for a ring; `gap_fill`, `solid_outers`, `signed_area` helpers |
| `spiral.rs` | `fill_spiral` | one path per lobe, `STEPS = 8` inset contours per spacing; laps between neighbouring contours so tips pull in faster than sides |
| `zigzag.rs` | `fill_zigzag*` | sweep lines joined into serpentines; `MAX_HOP_TIMES_SPACING = 6.0` lifts the pen for long turns |
| `hilbert.rs` | `fill_hilbert` | space-filling Hilbert curve, one pen-down path, power-of-two grid, order clamped 1..12 |

`zigzag.rs` is the most intricate: lines are grouped into columns (stacks on
consecutive rows overlapping along the line direction) so a turn only joins
neighbours, and each column takes at most one line per row — a star or comb
splits into one column per arm. Turns follow the outline (via `OnEdge` from
`sweep.rs`) when both ends lie on the same ring and the way round stays
between the two rows; otherwise a short inside hop; otherwise pen up.

## Lattice / tile (anchor-driven)

Regular cells placed on an anchored `Frame`.

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `dotswaves.rs` | `fill_dots`, `fill_dots_with_lattice`, `fill_waves`, `fill_wave_grid` | `Lattice::{Square, Hexagonal}`; `STEP = 0.1` wave march |
| `scales.rs` | `fill_scales` | rows of half-circles, one continuous scalloped line per row (`ARC_STEPS = 12`) |
| `truchet.rs` | `fill_truchet` | two quarter arcs per cell, random orientation (`ARC_STEPS = 8`); arcs meet at cell-edge midpoints to chain into long paths |
| `grass.rs` | `fill_grass` | tile of 4×4 tufts, hand-drawn look, repeats exactly like a `.pat` |
| `tile.rs` | `fill_tile` | arbitrary `Strokes` tile on a `width × height` grid, `row_shift` brick bond, turned by `angle`; lines may spill over the box |

## Scattered (seeded, no lattice)

Poisson-disk / Voronoi based. Take no anchor.

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `stipple.rs` | `fill_stipple`, `fill_stipple_field`, `fill_stipple_edge`, `fill_stipple_noise` | dots on a Poisson grid, each a short mark at a random angle |
| `gravel.rs` | `fill_gravel`, `fill_gravel_with` | Voronoi stones, gaps and rounded corners; `GravelEdge::{Adaptive, Clipped}` at the wall |
| `terrazzo.rs` | `fill_terrazzo` | chips of many sizes, ~⅓ dark (hatched, never solid) |
| `scribble.rs` | `fill_scribble` | two layers (2nd turned ~70–110°), per-line tilt/spacing error, broken into strokes — hand hatching |
| `nature.rs` | `fill_pebbles`, `fill_leaves`, `fill_sprouts`, `fill_lily_pads`, `fill_canopy`, `fill_fluid_grass` | site-plan vector textures; `fill_leaves` delegates to `fill_sprouts` |

## Isoline blobs (design / in progress)

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `isoline_blobs.rs` | `fill_isoline_blobs`, `fill_isoline_blobs_with_coverage` | nested contours per scattered site; returns mask `coverage` — see [isoline-blob-fills.md](isoline-blob-fills.md) |

## Texture (built on `field.rs`)

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `textures.rs` | `fill_marble`, `fill_wood`, `fill_cracks` | contour lines of a noise-driven field |

- Marble: veins at `angle`, domain-warped twice (slow drift + folded ripple),
  each vein drawn as its two edges so it pinches and swells. Sampled finer
  than the thinnest vein or it breaks into dashes.
- Wood: long lines along `angle` with knots (~1 per 900 mm²) the grain flows
  around; knot radius is relative to the board, not the line spacing.
- Cracks: rounded Voronoi cells with wide gaps — cellular dry-earth tile.

## Geological (`litho.rs`, large)

A small internal **pattern language** (lattices, marks, modifiers) with a
module-level kit, then thin public entries named after the lithology: 54
`fill_*` entries — `fill_granite`, `fill_gneiss`, `fill_breccia`,
`fill_diorite`, `fill_sandstone`, `fill_coal`, `fill_lava`, `fill_masonry`,
the `*_soil` family, and so on. Every entry clips to the region.

Internal kit to know before editing:
- `stagger_grid` / `grid` / `inside_grid_seeded` — lattice iteration; the
  seeded form draws jitter and the mark builder from **one** RNG stream in a
  fixed order so the pattern stays reproducible.
- `rand(seed, salt)` — one seed for both shapes and index tie-breaks.
- Mark builders `tick`, `cross`, `x_cross`, `tee`, `vee`, `bracket`, … all
  route through `stroke(region, out, pts)` / `seg(...)` so clipping is the
  region's job and never the caller's.

## Special

- `solid.rs::fill_solid` — flat colour fills: **no ink paths**. Plotters and
  line-only exporters skip them; hosts that paint regions (SVG `fill`, screen
  preview) use the `Region` boundary itself. Re-exported at the crate root.
- `pattern.rs::fill_pattern` and `tile.rs`/`pat.rs`/`svg.rs` are the
  user-supplied-pattern entry points — see `extensibility.md`.
- `dither.rs` is image-driven — see `extensibility.md` for its placement
  contract (`Greyscale`, `Marks::{Dots, Lines}`, `Dither`).
