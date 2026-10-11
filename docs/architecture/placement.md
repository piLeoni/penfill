# Placement

Where a pattern's lattice sits, and where a whole drawing sits inside the
region. Two related but distinct mechanisms.

## `Anchor` (`src/anchor.rs`) — lattice placement

A repeating pattern is a lattice laid over the region. `Anchor` fixes **one
point** the lattice passes through:

- `Origin::Local` (default): the pattern's own placement on the shape (most
  centre on the region's bounds), moved by `shift`.
- `Origin::Global`: a lattice point at page `shift`. Regions filled with the
  same pattern/spacing/angle then share one lattice, so two areas side by side
  read as one and their shared border leaves no seam.

Constructors: `Anchor::LOCAL`, `Anchor::GLOBAL`, `local(x, y)`, `global(x, y)`.
`resolve(local)` returns the lattice point given the pattern's own reference
point; `is_global()` gates the no-inset wall behaviour in `grid.rs`.

`Frame` (internal) is a lattice frame on the page: an origin and a turn.
`Frame::anchored(anchor, local, angle)` builds one through the anchor;
`world`/`to_frame` map between pattern and page coordinates; `span(region,
margin)` gives the region's bounds in frame coordinates.

Helpers used across the lattice fills:
- `indices(lo, hi, pitch)` — lattice indices covering a span, one spare each
  side.
- `centre(region)` — the middle of the region's bounds, most patterns'
  reference point.
- `cell_seed(seed, i, j)` — a per-cell random choice. **Invariant**: the same
  cell gets the same answer whichever region asks, so a global lattice stays
  continuous.

Random scatters (stipple, gravel, terrazzo, cracks, scribble) and
shape-following fills (concentric, spiral, hilbert) have no lattice and take
**no anchor**.

`anchor.rs` holds the cross-cutting regression tests: every lattice pattern is
checked for seamlessness under a global anchor
(`a_global_lattice_has_no_seam_where_two_regions_meet`) and for moving with
the shape under a local shift (`a_local_shift_moves_the_lattice_with_the_shape`).
`.pat` hatches are excluded from the latter because they are drawn from the
page origin.

## `Placement` (`src/placement.rs`) — drawing placement

CSS-background-style placement of an imported drawing or image inside the
region: `size`, `position`, `repeat`, measured against the region's bounding
box, everything clipped to the outline. Lengths in millimetres.
Strings use CSS syntax:

- size: `cover`, `contain`, `fill`, `40`, `40 30`, `auto 30`, `50%`;
- position: `center`, `top left`, `right 10`, `25% 75%`;
- repeat: `no-repeat`, `repeat`, `repeat-x`, `repeat-y`.

"Top" is the smallest y, as in SVG. `Placement` is consumed by `dither.rs`
(bitmap) and `svg.rs` (vector artwork).
