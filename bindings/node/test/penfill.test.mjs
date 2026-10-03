import { test } from 'node:test'
import assert from 'node:assert/strict'
import * as pf from '../index.js'

const SQUARE = [[0, 0], [40, 0], [40, 40], [0, 40]]
const HOLE = [[15, 15], [25, 15], [25, 25], [15, 25]]
const doughnut = new pf.Region(SQUARE, [HOLE])
const hole = new pf.Region(HOLE).inset(0.05)

function assertInside(strokes) {
  assert.ok(strokes.length > 0, 'empty fill')
  for (const s of strokes) {
    for (const [x, y] of s) {
      assert.ok(x >= -1e-6 && x <= 40 + 1e-6 && y >= -1e-6 && y <= 40 + 1e-6, `(${x}, ${y}) outside`)
      assert.ok(!hole.contains(x, y), `(${x}, ${y}) is in the hole`)
    }
  }
}

test('region basics', () => {
  assert.deepEqual(doughnut.bounds.map(Math.round), [0, 0, 40, 40])
  assert.equal(doughnut.rings.length, 2)
  assert.ok(doughnut.contains(5, 5) && !doughnut.contains(20, 20))
  assert.ok(doughnut.inset(30).isEmpty)
  const two = pf.Region.fromPolygons([[[[0, 0], [10, 0], [10, 10]]], [[[20, 0], [30, 0], [30, 10]]]])
  assert.equal(two.rings.length, 2)
})

test('zigzag joins the hatch', () => {
  const zig = pf.zigzag(doughnut, 1, { angle: 45 })
  const hatch = pf.rectilinear(doughnut, 1, { angle: 45 })
  assert.ok(zig.length > 0 && zig.length < hatch.length / 4, `${zig.length} vs ${hatch.length}`)
  assertInside(zig)
})

test('every fill draws inside', () => {
  const fills = {
    lines: () => pf.lines(doughnut, 1.5),
    grid: () => pf.grid(doughnut, 2, { angle: 10 }),
    triangles: () => pf.triangles(doughnut, 3),
    concentric: () => pf.concentric(doughnut, 1.5),
    spiral: () => pf.spiral(doughnut, 1.5),
    hilbert: () => pf.hilbert(doughnut, 1.5),
    honeycomb: () => pf.honeycomb(doughnut, 4),
    dots: () => pf.dots(doughnut, 2, { lattice: 'hex' }),
    waves: () => pf.waves(doughnut, 2),
    waveGrid: () => pf.waveGrid(doughnut, 3),
    scales: () => pf.scales(doughnut, 3),
    sunburst: () => pf.sunburst(doughnut, 2),
    stipple: () => pf.stipple(doughnut, 1.5, { seed: 3 }),
    stippleNoise: () => pf.stippleNoise(doughnut, 1, 4),
    stippleEdge: () => pf.stippleEdge(doughnut, 1, 4),
    gravel: () => pf.gravel(doughnut, 4),
    gravelClipped: () => pf.gravel(doughnut, 4, { clipped: true }),
    grass: () => pf.grass(doughnut, 3),
    scribble: () => pf.scribble(doughnut, 2),
    truchet: () => pf.truchet(doughnut, 4),
    marble: () => pf.marble(doughnut, 5),
    wood: () => pf.wood(doughnut, 2),
    cracks: () => pf.cracks(doughnut, 6),
    terrazzo: () => pf.terrazzo(doughnut, 6),
    chess: () => pf.chess(doughnut, 6),
    brick: () => pf.pat(doughnut, 'BRICK', { scale: 2 }),
  }
  for (const [name, fill] of Object.entries(fills)) {
    try {
      assertInside(fill())
    } catch (e) {
      throw new Error(`${name}: ${e.message}`)
    }
  }
})

test('seeds are deterministic', () => {
  assert.deepEqual(pf.gravel(doughnut, 4, { seed: 5 }), pf.gravel(doughnut, 4, { seed: 5 }))
  assert.notDeepEqual(pf.gravel(doughnut, 4, { seed: 5 }), pf.gravel(doughnut, 4, { seed: 6 }))
})

test('patterns', () => {
  assert.ok(pf.patterns().includes('HERRINGBONE'))
  const source = '*STRIPES, just lines\n0, 0,0, 0,2\n'
  assert.deepEqual(pf.patNames(source), ['STRIPES'])
  assert.ok(pf.pat(new pf.Region(SQUARE), 'stripes', { source }).length > 0)
  assert.throws(() => pf.pat(doughnut, 'NOPE'), /no pattern/)
})

test('your own functions', () => {
  const lines = pf.isolines(doughnut, (x, y) => Math.sin(x / 3) * Math.cos(y / 3), pf.levelsEvery(-0.9, 0.9, 0.3))
  assertInside(lines)
  assertInside(pf.profile(doughnut, 2, (x) => (x / 4) % 1))
  assertInside(pf.stippleField(doughnut, 1, 4, (x) => x / 40))
  assert.throws(() => pf.isolines(doughnut, () => { throw new Error('boom') }, [0]), /boom/)
})

test('dither', () => {
  const w = 32
  // Black left, white right; exact 8-bit steps so the RGBA copy below matches.
  const grey = Array.from({ length: w * w }, (_, k) => Math.round(((k % w) / (w - 1)) * 255) / 255)
  const ink = (strokes) => {
    let left = 0, right = 0
    for (const s of strokes) {
      const len = Math.abs(s.at(-1)[0] - s[0][0]) + 0.1
      if (s[0][0] < 14) left += len
      if (s[0][0] > 26) right += len
    }
    return [left, right]
  }
  for (const method of ['threshold', 'random', 'bayer8', 'halftone', 'floyd-steinberg', 'atkinson', 'jarvis', 'stucki', 'sierra']) {
    for (const marks of ['lines', 'dots']) {
      const strokes = pf.dither(doughnut, { width: w, height: w, data: grey }, { method, marks })
      assertInside(strokes)
      const [left, right] = ink(strokes)
      assert.ok(left > 3 * right, `${method} ${marks}: ${left} vs ${right}`)
    }
  }
  // Canvas-style RGBA bytes give the same drawing as the grey values.
  const rgba = new Uint8ClampedArray(w * w * 4)
  grey.forEach((v, k) => rgba.set([v * 255, v * 255, v * 255, 255], k * 4))
  assert.deepEqual(pf.dither(doughnut, { width: w, height: w, data: rgba }), pf.dither(doughnut, { width: w, height: w, data: grey }))
  assert.throws(() => pf.dither(doughnut, { width: w, height: w, data: grey }, { method: 'nope' }), /unknown dither/)
  assert.throws(() => pf.dither(doughnut, Buffer.from('not an image')))
})

test('placed like a CSS background', () => {
  const square = new pf.Region(SQUARE)
  const black = { width: 1, height: 1, data: [0] }
  const ink = (strokes) => strokes.reduce((sum, s) => sum + Math.abs(s.at(-1)[0] - s[0][0]), 0)
  const full = ink(pf.dither(square, black))
  const corner = pf.dither(square, black, { size: 20, position: 'top left' })
  assert.ok(Math.abs(ink(corner) / full - 0.25) < 0.01)
  assert.ok(corner.flat().every(([x, y]) => x <= 20 + 1e-6 && y <= 20 + 1e-6))
  const row = pf.dither(square, black, { size: [40, 10], position: 'top', repeat: 'repeat-x' })
  assert.ok(Math.abs(ink(row) / full - 0.25) < 0.01)
  assert.throws(() => pf.dither(square, black, { repeat: 'tile' }), /repeat/)

  const dot = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'><rect x='4' y='4' width='2' height='2'/></svg>"
  assert.equal(pf.svgFill(square, dot).length, 1)
  assert.equal(pf.svgFill(square, dot, { size: 10, position: 'top left', repeat: 'repeat' }).length, 16)
  assertInside(pf.svgFill(doughnut, dot, { size: '5', repeat: 'repeat' }))
})

test('svg in and out', () => {
  const svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'><circle cx='5' cy='5' r='3'/></svg>"
  assertInside(pf.svgTile(doughnut, svg, 5, { rowShift: 0.5 }))
  const art = pf.svgStrokes(svg, 40)
  const clipped = doughnut.clip(art)
  assertInside(clipped)
  const doc = pf.toSvg(clipped, doughnut)
  assert.ok(doc.startsWith('<svg') && doc.includes('mm'))
  assert.throws(() => pf.svgTile(doughnut, '<svg', 5))
  assertInside(pf.tile(doughnut, [[[0, 1], [2, 1]], [[1, 0], [1, 2]]], 3, 3, { angle: 15 }))
})

test('your own pattern', () => {
  let seen
  const circles = (canvas) => {
    seen = { bounds: canvas.bounds, seed: canvas.seed, region: canvas.region instanceof pf.Region }
    const [x0, y0, x1, y1] = canvas.bounds
    const out = []
    for (let x = x0 - 5; x < x1 + 5; x += 4)
      for (let y = y0 - 5; y < y1 + 5; y += 4)
        out.push(Array.from({ length: 38 }, (_, a) => [x + Math.cos(a / 6), y + Math.sin(a / 6)]))
    return out
  }
  const strokes = pf.pattern(doughnut, circles, { seed: 3 })
  assert.deepEqual(seen, { bounds: [0, 0, 40, 40], seed: 3, region: true })
  assert.ok(strokes.length > 0)
  assertInside(strokes)
  assert.deepEqual(pf.pattern(doughnut, (c) => pf.zigzag(c.region, 1)), pf.zigzag(doughnut, 1))

  const two = pf.Region.fromPolygons([[SQUARE, HOLE], [[[50, 0], [60, 0], [60, 10], [50, 10]]]])
  assert.deepEqual(two.parts().map((p) => p.rings.length).sort(), [1, 2])
  assert.throws(() => pf.pattern(two, () => { throw new Error('boom') }), /boom/)
  assert.throws(() => pf.pattern(two, () => 42))
})

test('the tools the fills are made of', () => {
  assert.equal(doughnut.distance(5, 20), 5)
  assert.equal(doughnut.distance(20, 12), 3)
  const got = {}
  const lines = pf.pattern(doughnut, (canvas, tools) => {
    assert.equal(tools.noise(0.3, 0.7), tools.noise(0.3, 0.7))
    assert.notEqual(tools.noise(0.3, 0.7), tools.on(canvas.region, 2).noise(0.3, 0.7))
    assert.ok(Math.abs(tools.noise(3.3, 1.2, 3)) <= 1.5)
    got.points = tools.scatter(3)
    got.field = tools.scatterField(1, 5, (x) => x / 40)
    got.piece = tools.on(canvas.region.inset(5)).scatter(3)
    return tools.hatch(2, 30)
  }, { seed: 4 })
  assertInside(lines)
  assert.ok(got.points.length > 50 && got.points.every(([x, y]) => doughnut.contains(x, y)))
  assert.ok(got.field.filter(([x]) => x < 20).length > 2 * got.field.filter(([x]) => x >= 20).length)
  assert.ok(got.piece.every(([x]) => x >= 5 - 1e-6 && x <= 35 + 1e-6))

  const edge = (canvas, tools) =>
    pf.stippleField(canvas.region, 0.8, 4, (x, y) =>
      Math.min(canvas.region.distance(x, y) / 8, 1) + 0.5 * tools.noise(x / 8, y / 8, 2) * 0.5,
      { mark: 0.32, seed: canvas.seed })
  assert.deepEqual(pf.pattern(doughnut, edge, { seed: 1 }), pf.stippleEdge(doughnut, 0.8, 4, { mark: 0.32, falloff: 8, rough: 0.5, seed: 1 }))
})

test('a global anchor lines two regions up into one lattice', () => {
  const whole = new pf.Region([[0, 0], [40, 0], [40, 20], [0, 20]])
  const left = new pf.Region([[0, 0], [20, 0], [20, 20], [0, 20]])
  const right = new pf.Region([[20, 0], [40, 0], [40, 20], [20, 20]])
  const opts = { angle: 30, anchor: 'global', shift: [0.3, 0.7] }
  // a mark the border cuts in two starts again on the cut line
  const marks = (r) =>
    pf.dots(r, 1.5, opts).filter((s) => Math.abs(s[0][0] - 20) > 1e-6).map((s) => s[0].map((v) => v.toFixed(3)).join()).sort()
  assert.deepEqual([...marks(left), ...marks(right)].sort(), marks(whole))
  const local = (r) => pf.dots(r, 1.5, { angle: 30 }).map((s) => s[0].map((v) => v.toFixed(3)).join())
  assert.notDeepEqual([...local(left), ...local(right)].sort(), local(whole).sort())
  assert.throws(() => pf.dots(whole, 1.5, { anchor: 'page' }), /anchor must be/)
})
