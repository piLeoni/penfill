# Geometry kernel

`src/geo.rs` is the crate's geometry kernel, backed by `i_overlay` (offsetting
and boolean clipping, pure Rust). It is the one place that answers "what is
inside the shape" and "where does this line cross the wall".

## Units and invariants

- All coordinates are **page millimetres**. `Pt = (f64, f64)`;
  `Strokes = Vec<Vec<Pt>>` is a list of open polylines.
- `Region` stores `shapes: Vec<Vec<Vec<[f64; 2]>>>` — each piece is an outer
  contour followed by its holes — plus a lazily-built, shared `Cutter`
  (`OnceLock<Arc<Cutter>>`). Regions never mutate, so clones share the cutter.
- `Region::from_polygons` normalizes: outers are unioned, holes subtracted,
  winding forced counter-clockwise, collinear vertices dropped. Input winding
  does not matter.
- `MITER_ANGLE` (0.4027 rad ≈ 2·asin(1/5)) reproduces shapely's default miter
  limit; `ARC_STEP` (0.1 rad) is the arc segmentation for round joins/caps.
- `MIN_SEGMENT_MM` (1e-6): strokes below this are rounding artefacts, dropped
  by `segment()` and the cutter.

## Why i_overlay

i_overlay snaps each operation's input to an integer grid fitted to its
bounds, which removes the floating-point robustness problems (the `buffer(0)`
repair dance the Python original needed — see `docs/clean-room/spec.md`). The
kernel covers exactly what fills needed from shapely in the Python version
(`fills/_common.py`): region normalization/repair, mitred insetting, and
point-in-region testing.

## `Region` API (the surface fills depend on)

- Construction: `from_polygons`, `from_ring`.
- Queries: `is_empty`, `bounds_mm`, `all_rings_mm`, `parts`, `covers_point`,
  `segment_inside`, `segment_inside_relaxed`, `dist_to_boundary`.
- Boolean / offset: `inset` (mitred), `outset` (round), `union`, `difference`,
  `intersection`, `around_strokes`.
- `from_rings_even_odd`: rings filled even-odd (nested rings alternate
  inside/outside) — for regions bounded by a field's contour lines.
- Clipping: `clip_segment`, `clip_polyline`, `clip_strokes`.
- Routing: `boundary_route` (walk region contours from `a` to `b`) — the
  connector that lets a serpentine survive on non-rectangular outlines.

### Non-obvious contracts

- `covers_point` counts even-odd across **all** contours, holes included;
  points exactly on a boundary count as covered (`ON_EDGE_MM` slack).
- `segment_inside` samples **midpoints only** (never endpoints). Testing
  endpoints reads a boundary point as outside and rejects every legitimate
  connector; `segment_inside_relaxed` additionally nudges boundary samples
  toward the segment midpoint.
- `parts()` splits into pieces; an island inside a hole is a piece of its own.

## `Cutter`

`Cutter` buckets a region's edges on a grid sized so each segment meets only
nearby edges. It implements the same inside/outside answers as sampling:
- `inside`: even-odd ray cast to +x over the cells right of `p`; each crossing
  is counted in exactly one cell so multi-cell edges count once.
- `near_edge(p, eps)`, `distance(p)` (ring-by-ring outward search).
- `polyline(pts)`: clips a polyline, keeping inside runs joined where they
  continue; skips sub-`MIN_SEGMENT_MM` fragments.

`clip_*` and `covers_point` all go through `Cutter`, so they agree by
construction. `geo.rs` tests assert this against dense sampling
(`cutter_matches_the_sampling_clip`) and against a brute-force distance.

## Scanline generation is NOT boolean clipping

`sweep.rs` deliberately does **not** use `Region`/i_overlay for scanlines: it
is the textbook scanline polygon fill.

- Each line of a family is cut against every edge of every ring at once; the
  cuts sorted along the line pair up (even-odd, so holes need no special case).
- Edges are half-open in the across direction (count at/above lower end,
  strictly below upper end) so a line through a vertex cuts once and an edge
  lying along the line not at all.
- Lines are found by sorting edges by their lower end and sweeping an active
  list — about one pass over the edges plus the cuts found.

Entry points: `sweep_lines`, `sweep_lines_through`, `sweep_anchored`,
`sweep_rows`, and the fragment form `sweep_anchored_fragments*`. `OnEdge`
records which outline edge each end lies on, which the outline-following turns
in `zigzag.rs` use.

`grid.rs` builds `fill_lines` / `fill_grid` / `fill_triangles` as one, two or
three sweep families. Note the invariant: lines keep half a spacing off the
walls **except** on a global anchor, where neighbouring regions must meet with
no gap and lines run right up to the outline.

## Call sites worth knowing

- `pattern.rs::fill_pattern` clips arbitrary pattern output with one
  `clip_strokes` over the whole region.
- `stream::Clipped` clips each stroke as it arrives (never holds raw geometry).
- Insets drive the shape-following fills (`concentric`, `spiral`,
  `rectilinear`, `hilbert`, `zigzag`).
