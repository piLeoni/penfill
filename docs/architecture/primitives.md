# Primitive layers

Small modules every fill builds on, plus one post-process (`hand.rs`). All deterministic, no external RNG
crate.

## `random.rs`

- `Rng`: SplitMix64, seeded. `new(seed)` mixes the seed; `next_u64`, `unit`
  (`[0,1)`), and ranged helpers.
- `poisson_disk(region, spacing, seed)`: blue-noise points, no two closer than
  `spacing` — the base of every scattered fill.
- `poisson_disk_varying`: spacing that varies with a field (gravel edge,
  stipple field).

**Invariant**: the same seed gives the same drawing on every platform. This is
why the crate ships its own RNG rather than depending on `rand`.

## `noise.rs`

- `Perlin`: gradient noise, `Clone` (clones share the same permutation, so
  `Tools::on` can hand a piece the same noise field).
- `noise(x, y)` roughly `[-1, 1]`, one bump per unit; `fbm(x, y, octaves)`
  sums octaves, each twice as fine and half as strong.

Used for drift: stipple clouds, marble/wood warping, patchy shading.

## `field.rs`

Fills from user maths — the shared engine for the texture family.

- `fill_isolines(region, cell, levels, f)`: contour lines of any `f(x, y)` at
  the given levels, sampled on a `cell` mm grid (smaller = smoother, slower).
  Per-level crossing lists keyed by grid edge; a cell only visits levels
  between its lowest and highest corner. Segments are chained into long
  polylines; closed contours come back closed. Ambiguous saddle cases
  (case 5/10) are disambiguated by the cell-centre value.
- `levels_every(lo, hi, step)`: convenience for evenly spaced levels.
- `fill_profile(region, spacing, angle, anchor, profile)`: parallel lines bent
  by `y = g(x)` — waves, zigzags, sawtooth, anything periodic or not.

`textures.rs` is built entirely on `fill_isolines`: marble veins, wood grain,
cracked ground are all contour lines of a noise-driven field. `field.rs` also
provides the trimmed `_into` streaming variants.

## `hand.rs`

`hand(&Strokes, &HandParams) -> Strokes`: a pure post-process that redraws any
strokes as if by hand. Region-free, so it runs after clipping, masking and
merging (draw.land calls it as `compose.hand`). `HandParams::default()` is a
steady hand; `HandParams::sketch()` adds crossing corners and pen lifts.

Effects, all in mm, each off at `0`: `wobble`/`wavelength` (one shared Perlin
displacement field), `tremor` (1-D noise along each stroke), `bow` (lopsided
arch per straight run, capped at `BOW_CAP_MM`), `overshoot` (open ends),
`close` (rings stop short or overlap and drift), `reach` (long lines split
into overlapping strokes), `corners` (sharp corners between edges ≥
`4 × corners`, min 1.5mm, drawn as crossing lines), `retrace`/`retrace_offset`
(share of strokes ≥ 3mm drawn a second time with their own bows and ends).

**Invariants**: deterministic and order/direction independent (each stroke's
RNG is keyed on its endpoints and vertex count); points that coincide get the
same `wobble` displacement, so joins stay joined; vertices are kept and
segments densified to `STEP_MM` (0.4mm). Preview: `cargo run --release
--example hand_demo` (also writes `doodle-trees.svg`).
