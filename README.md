# penfill

![Zigzag on four shapes. Each colour is one pen-down; the labels compare the stroke count with plain hatching](docs/img/hero.svg)

**Fills and hatches for pen plotters, for Python and Node — and an extensible
engine.** Give penfill a shape in millimetres and it returns the lines a pen
should draw to fill it: serpentine zigzags that barely lift the pen (each
colour above is one pen-down), spirals, lattices, stipple, gravel, grass,
marble, AutoCAD `.pat` hatches, dithered photos, and patterns you build
yourself. Every line stays inside the shape and goes around its holes.

Zigzag, stipple, dither and the rest are not a closed list. They are built
with the same architecture you get from [`pattern`](#build-your-own-pattern):
a **canvas** (the shape, its bounds, the seed), an optional **tools**
kit (noise, scatter, hatch — what the built-ins use), you return strokes
anywhere on the page, and the engine clips them to the outline. Add a fill
that behaves like a built-in one, or ignore the toolkit and use your own code.

## Install

```sh
pip install penfill       # Python ≥ 3.9
npm install penfill       # Node ≥ 18, TypeScript types included
cargo add penfill         # Rust
```

The engine is written in Rust. Python and Node get prebuilt binaries for
Linux, macOS and Windows, so there's nothing to compile, and the three
languages produce exactly the same lines.

## Quick start

**Python**

```python
import penfill

square = [(0, 0), (80, 0), (80, 60), (0, 60)]
hole = [(30, 20), (50, 20), (50, 40), (30, 40)]
shape = penfill.Region(square, holes=[hole])

strokes = penfill.zigzag(shape, 1.0, angle=45)   # polylines, in drawing order
open("zigzag.svg", "w").write(penfill.to_svg(strokes, outline=shape))
```

**Node**

```js
import { Region, zigzag, toSvg } from 'penfill'
import { writeFileSync } from 'node:fs'

const square = [[0, 0], [80, 0], [80, 60], [0, 60]]
const hole = [[30, 20], [50, 20], [50, 40], [30, 40]]
const shape = new Region(square, [hole])

const strokes = zigzag(shape, 1.0, { angle: 45 })  // polylines, in drawing order
writeFileSync('zigzag.svg', toSvg(strokes, shape))
```

**Rust**

```rust
use penfill::{geo::Region, zigzag::fill_zigzag, Anchor};

let square = vec![(0.0, 0.0), (80.0, 0.0), (80.0, 60.0), (0.0, 60.0)];
let hole = vec![(30.0, 20.0), (50.0, 20.0), (50.0, 40.0), (30.0, 40.0)];
let shape = Region::from_polygons(&[vec![square, hole]]);

let strokes = fill_zigzag(&shape, 1.0, 45.0, Anchor::LOCAL);
```

That rectangle with a hole comes out as 2 strokes. Draw each stroke with the
pen down from its first point to its last, then lift. `to_svg` writes an SVG
in millimetres to look at or to send to your plotter software (vpype,
Inkscape's AxiDraw extension, Saxi…).

## And about forty more

![Six fills on a shape with a hole: concentric, wave grid, scales, gravel, terrazzo, truchet](docs/img/tour.svg)

- [How it works](#how-it-works)
- [Lines and paths](#lines-and-paths)
- [Lattices](#lattices)
- [Scatter and nature](#scatter-and-nature)
- [Materials](#materials)
- [CAD hatch patterns](#cad-hatch-patterns)
- [Pictures: dithering](#pictures-dithering)
- [Placing a picture: size, position, repeat](#placing-a-picture-size-position-repeat)
- [Build your own pattern](#build-your-own-pattern)
- [Any shape](#any-shape)
- [Gallery](#gallery)
- [Development](#development)
- [Roadmap](#roadmap)
- [Lineage and license](#lineage-and-license)

## How it works

Every fill takes a **region** and a few numbers and returns **strokes**.

- **A region** is a filled area in millimetres: one or more outer rings,
each with any number of holes. `Region(outer, holes=[...])` makes one
polygon; `Region.from_polygons([[outer, hole, ...], ...])` makes several.
Regions can be shrunk (`inset`), grown (`outset`), intersected and
subtracted. The geometry runs on [i_overlay](https://github.com/iShape-Rust/iOverlay),
pure Rust (the engine under the `geo` crate).
- **Strokes** are a list of polylines, each a list of `(x, y)` points in mm.
Each polyline is one pen-down, and they come in drawing order.

The parameters mean the same thing everywhere:

- **`spacing`** is the distance in mm between neighbouring lines, cells or
marks. Use roughly your pen width or more.
- **`angle`** is in degrees. It turns the pattern, not the shape.
- **`inset`** (on the line fills) is how far from the wall the lines stop;
by default half the spacing. Pattern fills run right up to the region you
give them, so pass `region.inset(0.8)` for a margin.
- **`seed`** controls the random fills. The same seed always gives the same
drawing, so your plots can be reproduced.
- **`anchor`** and **`shift`** (on every fill with a lattice: the hatches,
lattices, grass, truchet, materials, chess, `.pat`, tiles, `profile` and
`pattern`) say where the lattice sits. `anchor="local"`, the default, places
it on the shape itself, usually centred. `anchor="global"` puts one lattice
on the whole page, so neighbouring regions filled separately meet with no
seam, as if they were one shape. `shift=(x, y)` moves the lattice that many
mm from where its anchor puts it; with `"global"` it is the page point a
lattice line passes through. In Rust it's the `Anchor` argument:
`Anchor::LOCAL`, `Anchor::GLOBAL`, `Anchor::local(x, y)`,
`Anchor::global(x, y)`. With `"global"`, `lines`, `grid` and `triangles`
run right up to the walls; `rectilinear` and `zigzag` still stop `inset`
from them, so pass `inset=0` for regions that share a border.

**Naming across languages.** The tables below use the Python names. Node
uses the same names in camelCase, with the optional arguments in an options
object: `penfill.wave_grid(r, 2, angle=30)` in Python is
`waveGrid(r, 2, { angle: 30 })` in Node. In Rust each fill is
`penfill::<module>::fill_<name>`, e.g. `penfill::dotswaves::fill_wave_grid`.

penfill only generates geometry. It doesn't draw the outline, sort strokes
between separate fills or write G-code. Those belong to your plotting
pipeline.

**Extensibility.** Named fills like `zigzag` or `stipple_edge` are Rust
functions that turn a region into strokes. Custom work goes through
[`pattern`](#build-your-own-pattern), which uses the same clipping and the
same optional [`tools`](#build-your-own-pattern) (Poisson scatter, Perlin
noise, hatch lines, distance-to-wall). Built-ins could be rewritten on top
of that API; you can mix built-ins and your own logic in one callback.

## Lines and paths

![rectilinear, zigzag, grid, triangles, concentric, hilbert](docs/img/lines.svg)

The classic slicer infills. Rectilinear and zigzag come from an exact
scanline sweep, so holes are always respected. Concentric, spiral and Hilbert are
built from successive insets of the shape.


| Function                                            | What it draws                                                                                                          |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `zigzag(region, spacing, angle=0, inset=None)`      | Hatch joined into serpentines by turns along the outline, or short hops where it wanders off. Where neither fits, the pen lifts |
| `rectilinear(region, spacing, angle=0, inset=None)` | Parallel hatch, one stroke per line fragment                                                                           |
| `lines(region, spacing, angle=0)`                   | One-direction hatch over the whole region                                                                              |
| `grid(region, spacing, angle=0)`                    | Two hatches at 90°                                                                                                     |
| `triangles(region, spacing, angle=0)`               | Three hatches at 60° that all pass through the same crossings                                                          |
| `concentric(region, spacing, inset=None)`           | Closed rings following the outline inwards, plus a centreline in parts too thin for a ring                             |
| `spiral(region, spacing)`                           | One continuous path winding inwards, so the pen stays down. A shape that splits into lobes gets one spiral per lobe    |
| `hilbert(region, spacing, inset=None)`              | Space-filling Hilbert curve: one long path that visits the interior cell by cell, clipped to the walls                   |


## Lattices

![honeycomb, dots hex, waves, wave grid, scales, sunburst](docs/img/lattices.svg)

Regular patterns laid across the whole shape and clipped to it.


| Function                                                      | What it draws                                                                                     |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `honeycomb(region, spacing, angle=0, line_width=0)`           | Hexagon walls, each drawn once. `spacing` is flat to flat; `line_width` opens a gap between cells |
| `dots(region, spacing, angle=0, mark=None, lattice="square")` | Short marks on a square or `"hex"` lattice centred in the shape                                   |
| `waves(region, spacing, angle=0)`                             | Parallel sine waves                                                                               |
| `wave_grid(region, spacing, angle=0)`                         | Ogee: mirrored waves that close into diamonds                                                     |
| `scales(region, spacing, angle=0)`                            | Fish scales or roof tiles; each row of arcs is one stroke                                         |
| `sunburst(region, spacing, angle=0, centre=None)`             | Rays from a centre (the middle of the shape, or the page point `centre`), staggered so they stay about `spacing` apart |


## Scatter and nature

![stipple, stipple edge, gravel, gravel clipped, grass, scribble](docs/img/scatter.svg)

Random but reproducible. Placement uses Poisson-disk sampling, so marks are
evenly spread without looking like a grid.


| Function                                                                       | What it draws                                                                                                                            |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `stipple(region, spacing, mark=None, seed=0)`                                  | Evenly spread short marks                                                                                                                |
| `stipple_noise(region, dense, sparse, mark=None, feature=12, seed=0)`          | Density drifts with Perlin noise, for cloudy tone                                                                                        |
| `stipple_edge(region, dense, sparse, mark=None, falloff=8, rough=0.5, seed=0)` | Dense along the wall, fading inwards over `falloff` mm with a noisy front: the cartographic shading for lakes and coasts                 |
| `stipple_field(region, dense, sparse, field, mark=None, seed=0)`               | Density from your own function; see [Build your own pattern](#build-your-own-pattern)                                                    |
| `gravel(region, spacing, seed=0, clipped=False)`                               | Rounded Voronoi stones. Near the wall the stones get smaller to fit into corners; `clipped=True` cuts uniform stones at the wall instead |
| `grass(region, spacing, angle=0, seed=0)`                                      | Grass tufts as a repeating hatch tile, cut by the wall                                                                                   |
| `pebbles(region, spacing, seed=0)`                                             | Small rounded stones — circle outlines on a Poisson grid (lighter than gravel)                                                           |
| `leaves(region, spacing, seed=0)`                                              | Sparse sprout tile — offset rows of blades and V marks (alias: same as `sprouts` in WASM)                                                |
| `fluid_grass(region, spacing, seed=0)`                                         | Loose curved grass — single blades, V-tufts and fans on a jittered Poisson field                                                         |
| `lily_pads(region, spacing, seed=0)`                                           | Equal circles with centre dots; overlapping pads merge to one outline                                                                    |
| `canopy(region, spacing, seed=0)`                                              | Top-down canopies — clustered wobbly lobes with a centre dot per lobe (`spacing` ≈ distance between trees)                               |
| `scribble(region, spacing, angle=0, seed=0)`                                   | Hand-drawn cross-hatching with wobbly, broken lines                                                                                      |
| `truchet(region, cell, angle=0, seed=0)`                                       | Random quarter-circle tiles that join into long winding paths                                                                            |


## Materials

![marble, wood, cracks, terrazzo, chess, truchet](docs/img/materials.svg)

Textures built from noise fields and contour lines.


| Function                                     | What it draws                                                                                                                                 |
| -------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `marble(region, spacing, angle=0, seed=0)`   | Veins from domain-warped noise                                                                                                                |
| `wood(region, spacing, angle=0, seed=0)`     | Growth rings bending around knots placed inside the shape                                                                                     |
| `cracks(region, spacing, seed=0)`            | Parched ground / mud cracks — wobbly net along Voronoi edges                                                                                    |
| `terrazzo(region, size, hatch=None, seed=0)` | Chips of many sizes that never overlap. About a third are dark, drawn with an open hatch or cross-hatch `hatch` mm apart (default `size / 6`) |
| `chess(region, cell, spacing=None, angle=0)` | Checkerboard with the dark squares hatched                                                                                                    |


## Geology (map symbols)

![granite, arena, breccia, coal, gneiss, ferrous](docs/img/geology.svg)

![diorite, conglomerate, masonry, diatom, amphibolite, hornblende](docs/img/geology-2.svg)

Cartographic lithology hatches built from one small pattern language: a base
lattice, a mark, and jitter, clipped to the region. The names follow the
lithologies you find on a geological sheet.

| Igneous | Clastic | Bedded | Organic | Metamorphic / special |
| ------- | ------- | ------ | ------- | --------------------- |
| `granite` `+` | `arena` dots | `beds` wavy horizons | `diatom` ring motifs | `gneiss` aligned dashes |
| `granite2` `×` | `conglomerate` round pebbles | `laminae` fine waves | `diatom2` denser rings | `bentonite` dash columns |
| `diorite` `T` | `conglomerate2` flat pebbles | `coal` dashed seams | `coquina` shell spirals | `amphibolite` vertical zigzag |
| `gabbro` `V` | `breccia` angular fragments | `masonry` running-bond brick | | `ferrous` checkerboard |
| `brackets` `└┘` | `breccia2` blobs | | | `lava` vertical bars |
| `dacite` crystal dashes | `shards` sparse shards | | | `hornblende` crystal capsules |
| `feldspar` dense strokes | `drift` tilted pebbles | | | `diabase` dense hatch \| `fill` cross hatch |
| | `silty_soil` small cells | | | `ophitic` laths + pyroxene grain |

All take `(region, spacing, seed=0)`; `dacite`, `feldspar`, `beds`, `laminae`,
`coal`, `gneiss`, `bentonite`, `diabase`, `ophitic`, `erosion`, `mulch`,
`layered_soil`, `clay_soil`, `fill` and `hornblende` also take an `angle` in
degrees.

### Ground and stone

![fine gravel, coarse stone, pebble scatter, mixed rocks, cracked earth, grainy soil](docs/img/ground.svg)

![erosion lines, stone cluster, gravelly soil, mulch, clay soil, pebbled soil](docs/img/ground-2.svg)

Loose materials and weathered surfaces, for the areas between outcrops.

| | | | |
| --- | --- | --- | --- |
| `fine_gravel` dots + rings | `coarse_stone` grained cells | `pebble_scatter` spaced ovals | `mixed_rocks` varied cells |
| `cracked_earth` cells + grain | `grainy_soil` fine dots | `erosion` parallel waves | `stone_cluster` tight pebbles |
| `gravelly_soil` short dashes | `mulch` staggered dashes | `mulched_soil` finer rows | `layered_soil` stacked horizons |
| `topsoil` mixed dots | `pebbled_soil` squares | `loose_soil` open specks | `silty_soil` tight cells |
| `clay_soil` broad waves | `compacted_soil` thin waves | `sandy_soil` fine grain | `dry_cracked` mud cracks |

These are all `(region, spacing, seed=0)`; `erosion`, `mulch`, `layered_soil`
and `clay_soil` also take an `angle`.


## CAD hatch patterns

![brick, herringbone, parquet, earth, ansi37, cross](docs/img/cad.svg)

The AutoCAD `.pat` format: each pattern is a set of line families with an
angle, an origin, an offset between lines and an optional dash sequence.
penfill has 14 built in, written for this project in the style of the
classics: `BRICK`, `HERRINGBONE`, `BASKET`, `PARQUET`, `ANSI31`, `ANSI32`,
`ANSI33`, `ANSI37`, `EARTH`, `CROSS`, `DASH`, `STAIRS`, `GRATE`, `ANGLE`.
Any `.pat` file you already have loads too.

```python
bricks = penfill.pat(shape.inset(0.8), "BRICK", scale=1.6)       # 1 pattern unit = 1.6 mm

source = open("acad.pat").read()
print(penfill.pat_names(source))
mine = penfill.pat(shape, "AR-PARQ1", scale=0.5, source=source)
```

## Pictures: dithering

![A lit ball, then dithered five ways: Bayer dots, halftone lines at 45°, Floyd–Steinberg lines, Atkinson dots, Jarvis lines 1 mm apart](docs/img/dither.svg)

A pen can't draw grey, so `dither` lays a picture over the shape (scaled to
cover it and centred, unless you [place it](#placing-a-picture-size-position-repeat))
and decides cell by cell whether to ink it. The cells are
`resolution` mm long along a line and `line_spacing` mm apart, and the grid
can be turned by `angle` while the picture stays put.

- **`marks="lines"`** (the default) joins dark cells next to each other into
one stroke and runs the lines back and forth, like a line engraving. It
lifts the pen far less than dots do.
- **`marks="dots"`** draws a short tick on each dark cell (`mark` mm long).


| `method`                               | Look                                                                                                |
| -------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `threshold`                            | Black or white at 50 %, posterised                                                                  |
| `random`                               | White-noise threshold, grainy like film                                                             |
| `bayer`, `bayer2` … `bayer16`          | Ordered dither, the crosshatch of early computer graphics                                           |
| `halftone`, `halftone2` … `halftone16` | Clustered dots that grow with the darkness, like newspaper print. Try it with `angle=45`            |
| `floyd-steinberg` (default)            | Error diffusion, the classic                                                                        |
| `atkinson`                             | Error diffusion that drops a quarter of the error: crisper, lighter shadows, the old Macintosh look |
| `jarvis`, `stucki`, `sierra`           | Error diffusion over a wider neighbourhood: smoother gradients                                      |


`gamma` above 1 darkens the midtones and below 1 lightens them; `invert=True`
inks the light parts.

```python
strokes = penfill.dither(shape, "portrait.jpg", resolution=0.5, method="atkinson", marks="dots")
strokes = penfill.dither(shape, "portrait.jpg", resolution=0.3, line_spacing=0.8, angle=30)
```

The picture can be a file path or the file's bytes (PNG, JPEG, GIF, BMP,
WebP). In Python it can also be a numpy array or nested lists of pixels
(grey or RGB, 0–1 or 0–255); in Node, `{ width, height, data }`, which is
what a canvas `ImageData` already is. Image row 0 goes at the smallest y, as
in SVG.

```js
const strokes = dither(shape, readFileSync('portrait.jpg'), { method: 'halftone6', angle: 45, resolution: 0.6 })
```

## Placing a picture: size, position, repeat

![The picture covering a doughnut, contained in a star, repeated every 14 mm, repeated along the top only; an SVG flower repeated every 9 mm and contained in a star](docs/img/placement.svg)

`dither` and `svg_fill` place their picture the way CSS places a
`background-image`, measured against the shape's bounding box, then clip
it to the outline. Where no copy of the picture lands, the paper stays blank.


| Option     | Values                                                                                                                                               | Like CSS              |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| `size`     | `"cover"`, `"contain"`, `"fill"` (stretch to the box), a width (`14`, `"14mm"`, `"50%"`), or width and height (`"20 auto"`, `"auto 30"`, `(40, 10)`) | `background-size`     |
| `position` | `"center"`, `"top left"`, `"bottom"`, `"25% 75%"`, `"5 10"` (mm from the top left corner)                                                            | `background-position` |
| `repeat`   | `"no-repeat"` (default), `"repeat"`, `"repeat-x"`, `"repeat-y"`                                                                                      | `background-repeat`   |


Numbers are millimetres, percentages are of the bounding box (for `position`,
the CSS meaning: `"100%"` puts the picture's far edge on the box's far edge).
"Top" is the smallest y, as in SVG. `dither` defaults to `cover` and
`svg_fill` to `contain`; both start centred and don't repeat.

```python
# a photo, tiled every 14 mm, starting from the top left corner
strokes = penfill.dither(shape, "portrait.jpg", size=14, position="top left", repeat="repeat")

# an SVG, fitted inside the shape
strokes = penfill.svg_fill(shape, open("flower.svg").read(), size="contain")

# a band along the top edge
strokes = penfill.dither(shape, "portrait.jpg", size=16, position="top", repeat="repeat-x")
```

```js
const strokes = svgFill(shape, flower, { size: 9, repeat: 'repeat' })
const band = dither(shape, 'portrait.jpg', { size: '100% 20%', position: 'top' })
```

In Rust, build a `Placement` with `Placement::parse(size, position, repeat)`
(or by hand from `Size`, `Length` and `Repeat`), then set
`DitherOptions::placement` or call `fill_svg_placed`.

## Build your own pattern

![sin x · cos y isolines, ripples from three sources, sawtooth profile, SVG tile, SVG clipped as is](docs/img/custom.svg)

This is how penfill is meant to grow. The forty-odd built-in fills are
implementations on the same footing as your code: geometry in, strokes out,
clipped to the region. [`pattern`](#build-your-own-pattern) is the public
entry point for that architecture — not a separate “custom” mode.

**The core API: `pattern`.** You write a function; penfill calls it once
with two arguments and cuts whatever it returns to the shape, around the
holes, in one pass.

```python
def my_pattern(canvas, tools):
    ...
    return strokes          # lines anywhere on the page

strokes = penfill.pattern(shape, my_pattern, seed=0)
```

- **`canvas`** is where you are drawing, in the same millimetres as the
  shape: `canvas.region` (the shape itself), `canvas.bounds`
  (`min_x, min_y, max_x, max_y`) and `canvas.seed`. Ask the region anything:
  `distance(point)` from the nearest wall or hole, `contains(point)`,
  `parts()` for the separate pieces, `inset`, `outset`, `clip`.
- **`tools`** is the same optional toolkit the built-in fills use internally
  (stipple, edge-weighted stipple, noise-driven scatter, …), already bound to
  the canvas region and seed. Use it to stay aligned with penfill, or ignore
  it and call numpy, your own libraries, anything. If your function takes
  only `canvas`, penfill doesn't pass `tools`.

| `tools.` | What it gives you |
|---|---|
| `noise(x, y, octaves=1)` | Smooth Perlin noise, roughly −1 to 1, one bump per unit |
| `scatter(spacing)` | Random points in the shape that keep their distance |
| `scatter_field(dense, sparse, field)` | The same, `dense` mm apart where `field(x, y)` is 0, `sparse` where it is 1 |
| `hatch(spacing, angle=0)` | Parallel lines cut to the shape, right up to the walls |
| `on(region, seed=None)` | The same tools for another region, e.g. one of `canvas.region.parts()` |

You can ignore the geometry and cover the bounding box, call built-in fills
and mix them, or go piece by piece. Splitting the shape costs time, so it
only happens when you call `parts()`.

```python
import math

def circles(canvas, tools):
    x0, y0, x1, y1 = canvas.bounds
    ring = lambda x, y: [(x + math.cos(a / 6), y + math.sin(a / 6)) for a in range(38)]
    return [ring(x, y) for x, y in tools.scatter(3.0)]

def one_angle_per_piece(canvas, tools):
    return [s for i, part in enumerate(canvas.region.parts())
            for s in tools.on(part).hatch(1.0, angle=30 * i)]

strokes = penfill.pattern(shape, circles, seed=7)
```

Because built-ins and custom patterns share `tools`, a built-in can be
recreated from the outside. This is `stipple_edge`, dot for dot:

```python
def my_stipple_edge(canvas, tools):
    def density(x, y):
        near_wall = min(canvas.region.distance((x, y)) / 8.0, 1.0)
        return near_wall + 0.5 * tools.noise(x / 8.0, y / 8.0, octaves=2) * 0.5
    return penfill.stipple_field(canvas.region, 0.8, 4.0, density, seed=canvas.seed)

assert penfill.pattern(shape, my_stipple_edge, seed=1) == penfill.stipple_edge(shape, 0.8, 4.0, seed=1)
```

In Node it's the same with camelCase names; `distance` takes `(x, y)`:

```js
const strokes = pattern(shape, (canvas, tools) =>
  tools.scatter(3).map(([x, y]) => [[x - 0.5, y], [x + 0.5, y]]), { seed: 7 })
```

In Rust, `pattern::fill_pattern(&region, seed, anchor, |canvas, tools| ...)`
gets a `pattern::Canvas` and a `pattern::Tools`. `canvas.origin` is the
anchor resolved for this region, the point to build your lattice through;
`tools.hatch` already uses it.

**A function of position, drawn as contour lines.** `isolines` samples
`f(x, y)` every `cell` mm and draws the levels you ask for, joined into long
strokes.

```python
from math import sin, cos

strokes = penfill.isolines(shape, lambda x, y: sin(x / 3) * cos(y / 3), penfill.levels_every(-1, 1, 0.25))
```

**A profile, repeated.** `profile` draws `y = f(x)` again every `spacing` mm,
turned by `angle`. Sawtooth, square waves and seismograph traces are one line
each.

```js
const strokes = profile(shape, 2.5, (x) => ((x / 4) % 1) * 1.8, { angle: 20 })
```

**A density function for stippling.** `stipple_field` takes a function that
returns 0 where the marks should be densest and 1 where sparsest.

```python
gradient = penfill.stipple_field(shape, 0.6, 3.0, lambda x, y: x / 80)
```

**An SVG, repeated as a tile.** `svg_tile` reads paths (lines, cubic and
quadratic curves), lines, polylines, polygons, rectangles, circles and
ellipses, with nested transforms. It scales the drawing to a tile `width` mm
wide, shifts every other row by `row_shift` tiles, turns the pattern and
clips it.

```python
flower = open("flower.svg").read()
strokes = penfill.svg_tile(shape, flower, 9.0, angle=15, row_shift=0.5)
```

**An SVG, placed like a CSS background.** `svg_fill` takes the same `size`,
`position` and `repeat` as `dither` ([see above](#placing-a-picture-size-position-repeat)).

**An SVG, clipped as is.** Read its lines with `svg_strokes`, scaled to a
width, and cut them with `region.clip`. `clip` works on any strokes, including
ones you generated yourself.

```python
strokes = shape.clip(penfill.svg_strokes(flower, width=60))
```

**A tile of your own strokes.** `tile(region, strokes, width, height, angle=0, row_shift=0)` repeats any polylines drawn in a `width × height` box, like
wallpaper.

## Any shape

![One concentric fill on a square, a doughnut, a blob, a star and a shape in two separate parts](docs/img/shapes.svg)

Holes, sharp spikes, thin necks and shapes in several separate parts are the
normal case, not an edge case. The gallery draws every fill on these five
shapes.

## Gallery

[`examples/gallery/`](examples/gallery) has one SVG per fill, showing it at
several angles or spacings on every test shape, and
[`gallery.svg`](examples/gallery/gallery.svg) puts them all side by side.
[`cad.svg`](examples/gallery/cad.svg) shows every built-in `.pat` pattern and
[`custom.svg`](examples/gallery/custom.svg) custom and composable fills, and
[`dither.svg`](examples/gallery/dither.svg) every dither method as dots and
as lines, and [`placement.svg`](examples/gallery/placement.svg) the main
`size`/`position`/`repeat` cases on a picture and on an SVG.

## Development

```
src/               the Rust engine (the `penfill` crate)
bindings/python/   PyO3 module, built with maturin
bindings/node/     napi-rs addon
examples/gallery.rs  draws the gallery and the pictures in this README
```

```sh
cargo test --package penfill                           # engine
cargo run --release --example gallery                  # gallery + README images

cd bindings/python && maturin develop --release && pytest
cd bindings/node && npm install && npm run build && npm test
```

GitHub Actions runs the Rust, Python, Node and WebAssembly test suites on
every push and pull request. A `v*` tag builds release artifacts for GitHub
and publishes the **`penfill`** crate to [crates.io](https://crates.io/crates/penfill)
(`CARGO_REGISTRY_TOKEN` in repo secrets). PyPI and npm publish stay off for
now (see `.github/workflows/release.yml` and [docs/RELEASE.md](docs/RELEASE.md)).

### Browser playground (WebAssembly)

```sh
./scripts/build-wasm.sh
cd web && npm install && npm run dev
```

See [web/README.md](web/README.md).

## Roadmap

- [x] Geometry kernel (i_overlay regions, insets, clipping)
- [x] Slicer fills: zigzag, rectilinear, grid, triangles, concentric, spiral
- [x] Lattices, scatter, materials and `.pat` hatch patterns
- [x] Custom fills: isolines, profiles, stroke tiles, SVG tiles and clipping
- [x] Extensible patterns (`pattern`, same architecture as the built-in fills)
- [x] Pictures: ordered, halftone and error-diffusion dithering, as dots or lines
- [x] Pictures and SVGs placed like a CSS background (`size`, `position`, `repeat`)
- [x] Python and Node bindings, CI and release workflows
- [x] WebAssembly build and browser playground (`web/`, `./scripts/build-wasm.sh`)
- [x] crates.io (`cargo add penfill` after the first `v*` tag publish)
- [ ] PyPI and npm (deferred)
- [x] Hilbert curve fill (`hilbert`)
- [ ] More space-filling paths (Peano, Gosper, …)
- [ ] Houndstooth, Greek key, seigaiha
- [ ] Pictures as hatching whose spacing or line width follows the tone

## Lineage and license

penfill grew out of postcards, which draws map postcards on a pen plotter;
dots and waves come from there. Everything else, including the built-in
`.pat` definitions, was written for penfill.

The line fills, zigzag, concentric and spiral are standard techniques
(scanline polygon filling, boustrophedon paths, offset contours). penfill's
implementation of them was written from a specification of their behaviour,
[docs/clean-room/spec.md](docs/clean-room/spec.md), without reference to any
slicer's source.

penfill is licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.
