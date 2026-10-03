# Clean-room specification: line fills, zigzag, concentric, spiral

This document specifies six modules of penfill — `sweep`, `rectilinear`,
`grid`, `zigzag`, `concentric`, `spiral` — by what they must do. It was
written from penfill's own behaviour, tests and documentation, for an
implementation written afresh from it.

**Rules for the implementer.** Work only from this document, the stubs and
tests in `src/`, the rest of penfill's source (`geo.rs`, `anchor.rs` and the
other modules are not part of this rewrite and may be read and used), and the
published literature listed at the end. Do not read earlier versions of these
six files (no `git log -p`, `git show`, `git diff` against older commits,
other branches or worktrees, or other copies on disk such as
`../penfill/src`), the Python prototypes under `../postcards/reference`, or
the source of any slicer or CAM program, and do not search the web for
them.

## Shared ground

- Units are millimetres. `Pt = (f64, f64)`. `Strokes = Vec<Vec<Pt>>`: open
  polylines in drawing order. A closed ring drawn as a stroke repeats its
  first point at the end.
- `Region` (in `geo.rs`) is a set of polygons with holes. Useful methods:
  `from_ring`, `from_polygons`, `is_empty`, `bounds_mm`, `all_rings_mm`
  (every ring of every polygon, outer contours counter-clockwise with
  positive signed area, holes clockwise with negative area, not closed — the
  first point is not repeated), `parts` (each polygon with its holes as its
  own region), `inset(d)` (shrink by `d`, mitred corners; a part thinner than
  `2d` vanishes), `outset(d)` (grow, round corners), `difference`,
  `around_strokes(strokes, r)` (everything within `r` of the strokes),
  `covers_point`, `segment_inside(a, b, samples)`,
  `segment_inside_relaxed(a, b, samples, nudge)`, `dist_to_boundary`.
  `geo::polyline_length` and `geo::MIN_SEGMENT_MM` (1e-3) are there too.
- `Anchor` (in `anchor.rs`): `Anchor::LOCAL` (the pattern's own placement),
  `Anchor::local(dx, dy)` (own placement moved by a shift),
  `Anchor::global(x, y)` (a lattice point at that page position).
  `anchor.resolve(p)` turns the pattern's own reference point `p` into the
  lattice point; `anchor.is_global()`; `anchor::centre(region)` is the middle
  of the region's bounding box.
- Angles are degrees, counter-clockwise from +x. For a line family at angle
  `θ`, with `s = sin θ`, `c = cos θ`, a page point `(x, y)` has an
  along-coordinate `u = x·c + y·s` and an across-coordinate
  `v = −x·s + y·c`; the inverse is `x = u·c − v·s`, `y = u·s + v·c`. Lines
  of the family are the sets `v = const`.
- Every public function returns nothing (an empty vector) for an empty region
  or a spacing that is not positive.

## sweep

Parallel lines clipped to a region by the even-odd rule: the textbook
scanline polygon fill (see Foley et al.), one line at a time.

**Clipping one line** at across-coordinate `v`: take every edge of every ring
(edge `k` of a ring runs from point `k` to point `(k + 1) mod n`). An edge
from `p` to `q` crosses the line when `v` lies in the half-open interval
between their across-coordinates: `v(p) ≤ v < v(q)` or `v(q) ≤ v < v(p)`. A
horizontal edge (in the line's frame) therefore never crosses, and a vertex
on the line is counted once. The crossing's along-coordinate is the linear
interpolation along the edge. Sort the crossings by along-coordinate and
pair them first with second, third with fourth, and so on: each pair bounds
a piece of the line inside the region. Drop pieces shorter than
`MIN_SEGMENT_MM`. Each piece is a two-point stroke from its lower
along-coordinate to its higher, both points mapped back to the page.

**Order.** Lines in increasing `v`; pieces of one line in increasing `u`.

**Placement, local (`sweep_lines`).** Let `vmin`, `vmax` be the extreme
across-coordinates of all ring points. Draw `count = ⌊(vmax − vmin) /
spacing⌋ + 1` lines, centred on the extent: the first at `(vmin + vmax) / 2
− (count − 1) · spacing / 2`, then one every `spacing`.

**Placement through a point (`sweep_lines_through`).** With `a = v(through)`,
lines lie at `a + k · spacing` for whole `k`: the first at
`a + ⌊(vmin − a) / spacing⌋ · spacing`, and `⌊(vmax − first) / spacing⌋ + 1`
of them.

**Anchored (`sweep_anchored`).** `Anchor::LOCAL` gives exactly
`sweep_lines`. Any other anchor gives `sweep_lines_through` the point
`anchor.resolve(P)`, where `P` is the point of the first local line at
along-coordinate 0: `P = (−first · s, first · c)`. A local shift therefore
moves the symmetric layout; a global anchor runs a line through the page
point.

**With edges (`sweep_anchored_fragments`).** The same pieces in the same
order as `sweep_anchored`, each with the `OnEdge { ring, edge }` of the edge
each end was cut from (`ends[0]` for `line[0]`), `ring` indexing
`region.all_rings_mm()`.

## rectilinear

- `fill_rectilinear(region, spacing, angle, inset, anchor)`: inset the region
  by `inset` (default `spacing / 2`), then `sweep_anchored`. Nothing if the
  inset region is empty.
- `fill_rectilinear_no_inset`: `sweep_anchored` on the region as it is.

## grid

- `fill_lines(region, spacing, angle, anchor)`: the region inset by
  `spacing / 2`, except with a global anchor, where the region is used as it
  is (regions sharing a page lattice must meet without a gap); then
  `sweep_anchored`.
- `fill_grid`: `fill_lines` at `angle`, followed by `fill_lines` at
  `angle + 90`.
- `fill_triangles`: the same target region as `fill_lines`; the point
  `anchor.resolve(centre(target))`; then `sweep_lines_through` that point at
  `angle`, `angle + 60` and `angle + 120`, in that order. Every crossing of
  two families then lies on a line of the third, closing equilateral
  triangles of height `spacing`.

## zigzag

The lines of a hatch joined into serpentine (boustrophedon) paths, as in
boustrophedon coverage (Choset and Pignon) and zigzag pocketing (Held): the
pen runs along one line, turns, runs back along the next.

**Lines.** `fill_zigzag_inset(region, spacing, angle, inset, anchor)`: the
target is `region.inset(inset)` (default `spacing / 2`); the lines are
`sweep_anchored_fragments(target, spacing, angle, anchor)`. `fill_zigzag`
is `fill_zigzag_inset` with the default inset. Every line is drawn exactly
once, whole, as part of one path.

**Columns (`bands`).** Lines are grouped into columns, stacks of lines on
consecutive scan rows that overlap along the line direction. In the
family's frame each line has a row (its `v`) and a span `[lo, hi]` of
along-coordinates. Visit the lines by increasing `v`, ties by increasing
`lo`. A line may extend an existing column whose last line lies on an
earlier row (strictly smaller `v`, by more than 1e-9) no more than
`1.5 · spacing` away, and whose span links with this line's span:

- they overlap by a positive amount that is at least
  `min(0.5 · spacing, 0.5 · shorter span length)`; or
- they do not overlap, but the gap between them is at most
  `0.15 · spacing` (two short chords near a sharp tip at 45°).

Among the columns it may extend, the line joins the one whose last line it
overlaps most; if none, it starts a new column. Each line joins at most one
column and each column grows by at most one line per row — there is no
transitive merging, so a star does not collapse into one column. Columns are
returned ordered by their first line's `v`, then `lo`; each column lists its
lines in visiting order.

**Walking a column.** The first line is drawn as the sweep gives it. Each
following line is drawn starting from whichever of its ends is nearer the
current path's end (on a tie, as the sweep gives it). Between the two, the
path turns:

0. *Doubling back.* When the rows are adjacent (their across-coordinates
   differ by more than 1e-9 and at most `1.65 · spacing`), the previous end
   and the next start lie more than `DOUBLE_BACK · spacing` (1.5 spacings)
   apart along the lines, and the line reaching further reaches less than
   that past the other's end, any turn would run back along nearly all of
   one line in a thin Z: the pen lifts.
1. *Along the outline.* When the previous end and the next start lie on the
   same ring of the target (by their `OnEdge`) and on adjacent rows, the
   turn follows that ring from one point to the other, whichever way round
   is shorter, passing through every ring corner in between. It is taken
   when its length is at most `MAX_HOP_TIMES_SPACING · spacing`
   (6 spacings) and none of its points strays more than
   `STRIP_SLACK · spacing` (0.05 spacings) past the two rows: a turn that
   leaves the strip between them runs beside or across other lines.
2. *Straight.* Otherwise a straight segment, when it is at most 6 spacings
   long, the rows are adjacent as above, it stays inside the original
   region (`region.segment_inside_relaxed(a, b, 32, 0.08)`), and the seven
   points at eighths along it each lie within `inset + 2 · spacing` of the
   original region's boundary (a long diagonal across the interior between
   two spikes is not a turn).
3. Otherwise the pen lifts: the path ends and a new one begins with this
   line.

A point closer than `MIN_SEGMENT_MM` to the last point of the path is not
added. The paths of each column follow one another, columns in order.

The cap keeps turns local: following the outline any further would draw
extra hatch along the walls of stars and blobs.

## concentric

Rings inset from the outer wall, one spacing apart, and a single centreline
where a part is too thin for a ring.

**Per piece.** `fill_concentric(region, spacing, inset)` treats each of
`region.parts()` on its own (an island in a hole of another part keeps its
own rings). It returns the rings of every piece, then the centrelines of
every piece.

**Rings.** Holes do not grow rings of their own: start from the piece's
outer contour with its holes filled (`solid_outers`), inset by `inset`
(default `spacing / 2`), and inset that by one more spacing each round. In
each round, look at every outer contour (positive area, at least 3 points)
of the current offset:

- if the contour, as a region, vanishes when inset by `0.25 · spacing`, it is
  too thin for a loop (a loop would be two lines on top of each other): draw
  its centreline instead;
- else, if all its edges lie in the material (`contour_inside`), draw it as a
  closed ring;
- else it has stepped into a hole: skip it.

Stop when a round draws nothing or the offset is empty (and in any case
after 10,000 rounds). Rings come out from the outside in.

**What the next ring misses.** In each round, the parts of the current
offset that the next ring will not reach — a spike tip much farther than a
spacing from the next ring; a right-angle corner, 0.71 spacings away, is
fine — get centrelines. Take the band `current.inset(spacing / 2)`, the next
ring's own band `band.inset(spacing / 2)`, and the leftover
`band − next.outset(0.75 · spacing)`; every outer contour of the leftover
gets a centreline.

**Centreline of a thin piece.** Find the piece's long axis (the direction of
greatest spread of its outline points: the principal axis of their
covariance). Cut the piece across that axis every `spacing / 4` and take
the midpoint and width of each cut. A cut position yielding more than one
piece is a fork (a V-shaped end) and is dropped. Chain the midpoints in
order along the axis, each joining the chain whose end is nearest, within
`0.6 · spacing`, or starting a new chain. Drop chains of fewer than two
points or whose widest cut is under `0.15 · spacing`. Split a chain wherever
a midpoint falls outside the original region, keeping runs of at least two
points, and drop strokes shorter than `0.3 · spacing`.

**Helpers** used by other modules:

- `solid_outers(region)`: every outer contour (positive area, at least 3
  points) as a polygon of its own, holes dropped; an empty region if none.
- `contour_inside(region, ring)`: every edge of the ring (closed; a repeated
  last point is ignored) passes `region.segment_inside(a, b, 16)`; false for
  fewer than 3 distinct points.
- `signed_area(ring)`: the shoelace area, positive counter-clockwise.
- `gap_fill(region, strokes, spacing, inset)`: inside `region.inset(inset)`,
  what lies farther than `0.75 · spacing` from every stroke
  (`Region::around_strokes`); each outer contour of that leftover that is
  thin — it vanishes when inset by `0.6 · spacing` — gets a centreline as
  above. Wide leftovers (a band round a hole the fill stopped at on purpose)
  are left alone.

## spiral

One path per lobe winding inward, lap after lap, each lap one spacing inside
the last, without crossing itself or another path, never leaving the
region. Spiral toolpaths for pockets are treated in the CAM literature (Held
and Spielberger; Bieterman and Sandstrom). Requirements:

- **Per piece**, like concentric: `fill_spiral(region, spacing)` handles each
  of `region.parts()` on its own, holes filled for the offsets
  (`solid_outers`) but respected for containment.
- **Follows the offsets.** The path's distance from the wall grows steadily:
  on a sharp shape the tips pull in faster than the sides, as the inset
  contours do, and the path follows the inset contours rather than
  interpolating straight across a spike. A practical guide is a stack of
  inset contours finer than the spacing (8 per spacing works), the first at
  `spacing / 2`, contours thinner than half a spacing left out, and each lap
  descending through one spacing of them as it goes round.
- **Lobes.** Where the offsets split into several contours, each lobe gets
  its own spiral; one lap cannot serve two lobes without cutting between
  them. A lobe's spiral starts a full spacing inside its parent's last lap,
  so the two never run side by side as a doubled line.
- **Sharp tips.** When one lap would drag the outer edge far inward (some
  point of the first contour ends up more than 1.5 spacings in after a lap's
  worth of descent), trace the first contour once, then wind from a spacing
  inside it.
- **Containment.** Every segment stays inside the region; a step that would
  leave it splits the path there.
- **No crossings.** No two segments of the output cross (touching or grazing
  within 0.1 mm does not count). Small knots where a lap doubles back near a
  sharp tip are cut out at the crossing.
- **Leftovers.** What the laps leave empty and thin (spike tips, the very
  centre) gets centrelines (`gap_fill` with inset `spacing / 2`).
- **Joins.** Finally, paths whose ends lie within `1.5 · spacing` are joined,
  closest pairs first, when the connecting segment stays inside the region
  (`segment_inside(a, b, 8)`) and crosses no drawn segment.
- `project(p, a, b)` returns the parameter of `p`'s projection on segment
  `a`–`b`, clamped to `[0, 1]`, and the distance from `p` to that point.

## Acceptance

The tests in the six files must pass unchanged (more may be added), with the
rest of penfill's tests, `cargo clippy --all-targets` clean, and the gallery
(`cargo run --release --example gallery`) drawing each fill as the
documentation describes.

## Literature

- J. D. Foley, A. van Dam, S. K. Feiner, J. F. Hughes, *Computer Graphics:
  Principles and Practice*, 2nd ed., Addison-Wesley, 1990: §3.6, filling
  polygons (scanlines, even-odd rule, half-open edges).
- H. Choset, P. Pignon, "Coverage Path Planning: The Boustrophedon Cellular
  Decomposition", *Field and Service Robotics*, 1998.
- M. Held, *On the Computational Geometry of Pocket Machining*, LNCS 500,
  Springer, 1991 (zigzag and contour-parallel tool paths).
- M. Held, C. Spielberger, "A smooth spiral tool path for high speed
  machining of 2D pockets", *Computer-Aided Design* 41(7), 2009.
- M. Bieterman, D. Sandstrom, "A curvilinear tool-path method for pocket
  machining", *Journal of Manufacturing Science and Engineering* 125(4),
  2003.
