# penfill for Node

**Fills and hatches for pen plotters.** Give penfill a shape in millimetres
and it returns the polylines a pen should draw to fill it: serpentine
zigzags that barely lift the pen, spirals, lattices, stipple, gravel, grass,
marble, AutoCAD `.pat` hatches, dithered photos, your own functions or your own SVG tiles.
Every line stays inside the shape and goes around its holes.

![Zigzag fills, one colour per pen-down, next to how many strokes plain hatching needs](https://raw.githubusercontent.com/pileoni/penfill/master/docs/img/hero.svg)

```sh
npm install penfill
```

```js
import { Region, zigzag, toSvg } from 'penfill'
import { writeFileSync } from 'node:fs'

const square = [[0, 0], [80, 0], [80, 60], [0, 60]]
const hole = [[30, 20], [50, 20], [50, 40], [30, 40]]
const shape = new Region(square, [hole])

const strokes = zigzag(shape, 1.0, { angle: 45 })  // array of polylines, in drawing order
writeFileSync('zigzag.svg', toSvg(strokes, shape))
```

Photos and SVGs are placed like a CSS background, then clipped to the shape:

```js
import { dither } from 'penfill'

const photo = dither(shape, 'portrait.jpg', { size: 14, repeat: 'repeat', method: 'atkinson' })
```

The engine is written in Rust and ships as a prebuilt native addon for
Linux, macOS and Windows, with TypeScript types. Nothing to compile.

Every fill, with pictures: **[github.com/pileoni/penfill](https://github.com/pileoni/penfill)**

License: MIT OR Apache-2.0.
