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
| `dotswaves.rs` | `fill_dots`, `fill_dots_with_lattice`, `fill_waves`, `fill_waves_frayed`, `fill_wave_grid`, `fill_wave_grid_frayed` | `Lattice::{Square, Hexagonal}`; `STEP = 0.1` wave march; optional perimeter-aware `fray` |
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
| `nature.rs` | `fill_pebbles`, `fill_leaves`, `fill_sprouts`, `fill_lily_pads`, `fill_canopy`, `fill_fluid_grass`, `fill_tufts(_frayed)`, `fill_twigs(_frayed)`, `fill_vines(_frayed)` | site-plan vector textures; `fill_leaves` delegates to `fill_sprouts` |

## Branching (space colonization)

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `growth.rs` | `fill_growth` | dendritic networks grown toward a Poisson-disk field of attractors (Runions et al. 2007); river deltas, veins, cracks, roots, lichen |

Each step, every surviving attractor pulls its nearest node (any node, not
just leaves) within `reach`; pulled nodes advance one `step` along the
averaged pull, and an attractor is consumed once a node lands within `kill`
of it. Forks come from one node being pulled two ways over successive steps;
`MAX_KIDS` (3) caps the branches leaving a node, so a root forks into trunks
instead of fanning out into a starburst. `RootSource::{Generate(n), Every(d),
At(pts)}` picks where networks start; `Every` puts at least one root in every
part of the region. Twigs end up about 2–2.5× `spacing` apart (draw.land
maps its `spacing` to that twig gap).

Each attractor caches its nearest node (`Tree::best`); a new node only
updates the attractors in its grid neighbourhood, so a step costs O(live
attractors). Termination: a node pulled to a zero net direction, or whose
next point lands within `0.3·step` of an existing node, is *stuck*, and the
attractors holding it are dropped. This replaced a stall counter: without it
an attractor could orbit just outside `kill` forever.

Output: `branches` splits the tree into polylines (each follows its child
with the most leaves; other children start at the junction), drops side
spurs of ≤3 nodes, and Chaikin-smooths them (`smooth` rounds). With `pen > 0`
a branch carrying `thicken·k²` leaves gets `k` (≤ `max_passes`) parallel
passes a side, `pen` apart, tapering into the centre line, so trunks read
dark and twigs hairline.

Demo: `examples/growth_demo.rs` → `examples/gallery/growth-{branching,fine,jittery}.svg`.

## Isoline blobs (design / in progress)

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `isoline_blobs.rs` | `fill_isoline_blobs`, `fill_isoline_blobs_with_coverage` | flat: contours of one 2D noise field; returns mask `coverage` — see [isoline-blob-fills.md](isoline-blob-fills.md) |
| `isoline_stack.rs` | `fill_isoline_stack`, `fill_isoline_stack_with_coverage` | 3D: crowns of metaball spheres cut into slices, upper slices hide lower ones (overhang); `EdgeMode::{Clip, Inside, Overflow(mm)}` |

## Texture (built on `field.rs` and flow/Voronoi)

| Module | Fill(s) | Notes |
| --- | --- | --- |
| `textures.rs` | `fill_marble`, `fill_wood`, `fill_cracks`, `fill_cell_net(_frayed)`, `fill_eddies(_frayed)`, `fill_ripples(_frayed)`, `fill_swells(_frayed)` | contour lines, flow streamlines, overlapping swells, and cellular nets |
| `fray.rs` | `whole_at`, `fray_strokes` | perimeter-aware edge fraying: survival weight from `Region::dist_to_boundary` + Perlin wobble breaks strokes into progressively shorter dashes/dots before the boundary |

- Marble: veins at `angle`, domain-warped twice (slow drift + folded ripple),
  each vein drawn as its two edges so it pinches and swells. Sampled finer
  than the thinnest vein or it breaks into dashes.
- Wood: long lines along `angle` with knots (~1 per 900 mm²) the grain flows
  around; knot radius is relative to the board, not the line spacing.
- Cracks: rounded Voronoi cells with wide gaps — cellular dry-earth tile.
- Cell net: single-wall Voronoi network where shared edges are drawn once with 1–2 organic kinks and exact 3-way junctions.
- Eddies: continuous 2D vector field of undulating wave streamlines and Archimedean spiral vortices placed via Jobard-Lefer seeding so streamlines hug the spirals with no empty gaps.
- Ripples: staggered rows of medium-length wave strokes driven by a shared 2D swell field.
- Swells: overlapping wave hills / dunes rendered foreground-to-background with a 1D horizon occlusion buffer and internal ribs that fan across each swell's slope.

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
