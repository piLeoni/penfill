# Catalog — generators and materials

The pattern catalogue is two layers:

- **Generators** are the algorithms: a handful of ways to put lines in an
  area (`hatch`, `scatter`, `cells`, `tile`, `waves`, `rings`, …). Each one
  takes the shared vocabulary below plus its own options, and **describes
  itself**: every option has a kind, a default and a sentence of
  documentation, so hosts validate input against it and generate their
  reference from it.
- **Materials** are named recipes over generators: `gravel`, `marsh`,
  `limestone`, `orchard`. A material is one or more layers, each a generator
  with settings, grouped by what it is for (water, vegetation, ground, rock,
  built, plain). Most users pick a material; anyone can drop down to the
  generator and set every option.

Sources: `src/catalog/` (`mod.rs` the entry points, `value.rs` options,
`schema.rs` self-description, `field.rs` tone, `marks.rs` the symbol library,
`gen_*.rs` the generators, `materials.rs` the table).

## Why

The previous registry was 117 flat names. About half were the same five
algorithms with different constants baked in and a rock name attached, so the
catalogue was large to read and small in what it could do: no way to say
"these marks, sparser near the shore", no custom symbol, `spacing` meaning a
different thing on every pattern (draw.land carried a per-name correction
table), and the geology fills ignored the anchor, so neighbouring areas did
not line up. The catalogue replaces that registry; it is not layered on top of
it.

## The shared vocabulary

Every generator reads these the same way.

| Option | Meaning |
| --- | --- |
| `spacing` | Distance between neighbouring elements, centre to centre, in mm: line pitch, mark pitch, cell size. A material has its own default; giving one scales the whole material (every length in a material is stored as a multiple of its spacing). |
| `angle` | Rotation of the pattern in degrees. |
| `seed` | Deterministic randomness. |
| `anchor` | `local` (centred on the shape) or `global` (page origin: neighbouring areas meet without a seam, and a card moved over the map keeps its marks). Every generator honours it, scatter included: random placement is per anchored cell, never per region. |
| `inset`, `edge`, `fray` | As in `FillParams`: a wall margin, how elements meet the boundary (`clip`, `inside` keeps marks whole, `overflow`), and the perimeter fade. |
| `mark` | The size of one element (a dot's length, a symbol's height), in mm. |
| `jitter` | 0–1: how far elements wander from their place. |
| `vary` | 0–1: how much element sizes (or line gaps) differ. |
| `density` | The tone field: `even` (default), `edge` (denser toward the boundary), `centre`, `noise` (patches), `gradient` (toward `toward` degrees). With `contrast` (0–1, how far the tone swings) and `feature` (mm, the size of noise patches). Generators that can thin or add elements follow it: scatter keeps fewer marks, hatch breaks lines and drops cross layers (engraving tone). |

Lengths a user gives are millimetres. Lengths in the materials table are
multiples of the material's spacing, resolved when the material is built.

## Generators

| Generator | Draws | Its own options |
| --- | --- | --- |
| `hatch` | Parallel lines; with `cross` 2 or 3 a grid or a triad | `cross`, `join` (one serpentine stroke), `dash` `[on, off]`, `dash_jitter`, `wobble` + `wobble_length` (bedding), `vary` (uneven gaps) |
| `scatter` | One mark at each of many points | `arrange` (`blue`, `hex`, `square`, `rows`, `clusters`), `motif` (a symbol name or your own polylines), `rotate` (`random`, `aligned`, degrees), `vary`, `jitter` |
| `cells` | A Voronoi tessellation | `style` (`stones`, `net`, `cracks`), `gap`, `round`, `vary`, `inner` (a mark inside each cell) |
| `tile` | A motif repeated on a lattice | `motif` (a name such as `brick`, or your own polylines), `size` `[w, h]`, `shift` (row offset), `pat` (CAD `.pat` text) |
| `waves` | Wavy parallel lines | `shape` (`sine`, `scallop`, `zigzag`, `curl`, `flow`), `wavelength`, `amplitude`, `cross` |
| `rings` | Rings from the boundary inward | `from` (`outline`: holes stop them; `edges`: every edge, holes included), `spiral` |
| `scribble` | Hand hatching in loose overlapping strokes | `layers`, `loose` |
| Specialists | `hilbert`, `truchet`, `sunburst`, `honeycomb`, `scales`, `wood`, `marble`, `swells`, `eddies`, `ripples`, `isoline-stack`, `isoline-blobs`, `clouds`, `growth`, `doodle-trees`, `canopy`, `twigs`, `solid` | Their own, as before |

## Materials

A material is `{ name, group, about, spacing, layers }`; each layer is a
generator name and option overrides. Groups, in the order a map maker reaches
for them: **water**, **vegetation**, **ground**, **rock**, **built**,
**plain** (lines, grids, dots, rings: the abstract hatches). Rock materials
follow the conventions of geological maps (sandstone dots, shale dashes,
limestone in brick bond, granite crosses, volcanic vees), so they read as the
symbols a geologist expects rather than as invented textures.

A user's options override every layer's (a layer that does not know an option
ignores it), and `spacing` scales the whole recipe.

## Self-description

`catalog::describe()` returns every generator's options (key, kind, default,
doc) and every material (group, about, spacing, layers) as data. Hosts:

- validate a fill's options against the generator's schema and name the valid
  keys in the error, instead of ignoring a typo;
- generate their reference and swatch pages from it (draw.land's docs do);
- test that their own type declarations list exactly the catalogue's names.

## Custom patterns

From a host that only sends data (draw.land profiles): `scatter` and `tile`
take `motif` as polylines in mm around the origin, `tile` takes `.pat` text,
and every generator's options are open, so a new look is a new combination,
not a code change. From Rust, Python and Node, `fill_pattern_with` and the
`Canvas` / `Tools` contract (see [extensibility.md](extensibility.md)) remain
the way to write a pattern in code.

## Invariants

- Every generator clips to the region and honours `anchor`, `edge`, `inset`
  and `fray`.
- Same options, same seed → the same strokes on every platform.
- With `anchor: global`, two regions that share an edge draw one continuous
  pattern across it (tested for every generator).
- Every material draws something in a 20 mm square at its default spacing
  (tested), and its default spacing reads at a 0.2–0.3 mm pen.
