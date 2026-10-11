# Primitive layers

Three small modules every fill builds on. All deterministic, no external RNG
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
