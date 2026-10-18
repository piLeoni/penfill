# Architectural decisions

Durable decisions and the reasoning behind them. Append new entries; don't
rewrite history. Source code is authoritative — if an entry no longer matches
the code, either fix the code or supersede the entry here.

## Units: millimetres everywhere

All coordinates are page millimetres (`Pt = (f64, f64)`). Mirrors the
postcards fill contract (`docs/clean-room/spec.md`). No unit conversion happens
inside the crate.

## Geometry kernel: i_overlay, not a hand-rolled/clipper binding

`i_overlay` snaps each operation to an integer grid fitted to its bounds,
which removes the floating-point robustness problems the Python original
handled with a `buffer(0)` repair dance. Pure Rust, no C dependency. See
`geometry.md`.

## Scanlines do not use boolean clipping

`sweep.rs` implements the textbook scanline fill directly: sort edges by lower
end, sweep an active list, pair cuts even-odd. This is deliberate — a sweep
costs about one pass over the edges plus the cuts found, where boolean clipping
per line would be far more expensive. Holes need no special case because of the
even-odd pairing. See `geometry.md`.

## Fills return open polylines, in draw order

`Strokes = Vec<Vec<Pt>>`. The library never closes paths or orders strokes for
shortest travel; that is the consumer's (vpype's) job. `fill_solid` returns no
strokes at all — flat fills are the host's business.

## Determinism by seed, with the crate's own RNG

Every random fill takes `seed: u64` and the crate ships SplitMix64 (`random.rs`)
rather than depending on `rand`, so the same seed gives the same drawing on
every platform. Per-cell choices use `cell_seed` so a global lattice stays
continuous across separately-filled regions.

## Streaming is an additive layer, not a rewrite

`stream.rs` (`Sink`, `Clipped`, `Meter`) and the `*_into` entry points let a
generator emit rows under a bounded consumer. The one-shot `fill_*` functions
remain the canonical implementations and call the `_into` forms. Invariant:
streaming changes *when* strokes are emitted, never *which*. See
`streaming.md`.

## `.pat` hatches are drawn from the page origin

A `.pat` hatch is defined in page coordinates, so its local placement is
already the page one and `anchor.shift` merely moves it. This is why `.pat` is
excluded from the local-shift continuity test in `anchor.rs`.

## `.pat` built-in definitions are derived, not vendored

The 14 built-in patterns in `pat.rs` are derived from the brick / section /
symbol geometry in this repo, not copied from a vendor file.

## Anchor: local by default, global for shared lattices

`Anchor::LOCAL` (pattern placed on its own shape) is the default and preserves
historic behaviour. `Anchor::GLOBAL` gives several regions one lattice so they
meet without a seam. Global anchors also change the wall inset for line fills
(`grid.rs`): lines run right up to the outline so neighbours meet.

## Deliberately awkward test shapes

`geo.rs` and `anchor.rs` test against stars, doughnuts and islands-in-holes.
These exist to catch wall-crossing connectors and seams, which are the failure
modes the geometry kernel and `zigzag` turns are built to avoid. Meanwhile,
`examples/gallery.rs` uses clean, generous `52 × 52 mm` shapes (square,
rounded card, smooth ring, harmonic island, island with pond, 5-point star, and
archipelago) so every pattern and its frayed perimeter read clearly.

## Unified pattern contract: `Canvas`, `Tools`, `EdgeMode`, `fray`, and `FillOutput`

Every built-in pattern and user-supplied pattern shares `FillParams` (`spacing`,
`angle`, `seed`, `anchor`, `inset`, `edge: EdgeMode::{Clip, Inside, Overflow(mm)}`,
`fray: f64`) and the `Canvas` + `Tools` standard library (`pattern.rs`).
- Universal perimeter fraying (`fray.rs`) runs after clipping on any pattern
  with `fray > 0` (unless the pattern calls `tools.claim_fray()` to perform
  motif-coherent fraying around shared roots/centres), adapting automatically
  to continuous polylines, short discrete marks, and closed cell rings.
- Every pattern can export a polygon knockout silhouette (`FillOutput { strokes, mask }`)
  via `tools.add_mask_ring` / `tools.add_mask` (or automatic `Region::around_strokes`
  buffering for line/mark/frayed fills) so foreground layers can cut lower
  layers cleanly via `below.difference(&out.mask)`.

