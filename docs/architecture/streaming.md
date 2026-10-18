# Streaming — `src/stream.rs`

Generating strokes in batches, clipping and handing them on without ever
holding the whole drawing in memory.

## Why

Every classic fill builds its strokes all at once and clips once at the end.
On a big output — a 50 km strip of dots at 0.5 mm — that is millions of
polylines in a single `Vec` before a single one reaches the plotter, and the
peak is proportional to the *whole* canvas. In a browser (4 GB wasm heap) that
aborts.

## Design

A `Sink` flips the flow: generators **push** strokes into it as they make
them, and the consumer keeps only what it is willing to hold. The unit of work
is a **row** (or a batch of rows) of the generator's own lattice, so no
geometric grid is imposed and there are no seams to stitch: the strokes are
exactly the ones the one-shot version produces, emitted earlier.

## API

- `trait Sink { fn stroke(&mut self, Vec<Pt>); fn batch(&mut self, Strokes) }`.
  `batch` defaults to looping `stroke`; a sink wanting whole rows overrides it.
- `impl Sink for Strokes` — appends (`batch` extends wholesale).
- `impl Sink for &mut S` — forwarding.
- `Clipped<'a, S>` — clips each stroke to a region as it arrives, so the
  generator's raw geometry never lingers.
- `Frayed<'a, S>` — clips and applies perimeter edge fraying (`fray` mm) to each
  stroke as it arrives, matching `fray_strokes` (`frayed_sink_matches_fray_strokes`).
- `Meter { strokes, points }` — the cheap probe: counts without keeping.
- `meter(region, fill)` — estimate a fill's stroke count before running it.
- `row_batches(count, batch)` — split a row span into `(start, end)` pairs,
  the batching primitive for `Sink::batch`.

## `_into` entry points

The streaming variants follow one naming rule: `<fill>_into(..., sink: &mut
dyn Sink)`, and the classic `<fill>(...)` is a thin wrapper that calls the
`_into` form with a `Strokes` sink. Examples across the crate:
`sweep_anchored_into`, `fill_rectilinear_into`, `fill_lines_into`,
`fill_grid_into`, `fill_dots_with_lattice_into`, `fill_waves_into`,
`fill_profile_into`, `sweep_anchored_fragments_into`,
`cut_rows_into`.

Doc test in `stream.rs` pins the contract: a fill run into a `Strokes` sink
must equal the one-shot fill.

## Equivalence invariant

`clipped_sink_matches_a_clip_over_everything` and
`meter_counts_what_collect_keeps` in `stream.rs` assert that streaming +
clipping produces the same strokes as clipping everything at the end. Any new
streamed fill must preserve this: streaming changes *when* strokes are
emitted, never *which*.
