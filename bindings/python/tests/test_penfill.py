import math
import struct
import zlib

import pytest

import penfill

SQUARE = [(0, 0), (40, 0), (40, 40), (0, 40)]
HOLE = [(15, 15), (25, 15), (25, 25), (15, 25)]


@pytest.fixture
def doughnut():
    return penfill.Region(SQUARE, holes=[HOLE])


def inside(region, strokes):
    shrunk_hole = penfill.Region(HOLE).inset(0.05)
    for s in strokes:
        for x, y in s:
            assert -1e-6 <= x <= 40 + 1e-6 and -1e-6 <= y <= 40 + 1e-6
            assert not shrunk_hole.contains((x, y)), f"({x}, {y}) is in the hole"


def test_region_basics(doughnut):
    assert doughnut.bounds == pytest.approx((0, 0, 40, 40))
    assert len(doughnut.rings) == 2
    assert not doughnut.is_empty
    assert doughnut.contains((5, 5)) and not doughnut.contains((20, 20))
    assert doughnut.inset(30).is_empty
    assert "2 rings" in repr(doughnut)


def test_points_as_lists_or_tuples():
    a = penfill.Region([[0, 0], [10, 0], [10, 10]])
    b = penfill.Region(((0, 0), (10, 0), (10, 10)))
    assert a.bounds == b.bounds


def test_from_polygons_two_parts():
    left = [(0, 0), (10, 0), (10, 10), (0, 10)]
    right = [(20, 0), (30, 0), (30, 10), (20, 10)]
    r = penfill.Region.from_polygons([[left], [right]])
    assert r.contains((5, 5)) and r.contains((25, 5)) and not r.contains((15, 5))


def test_zigzag_joins_the_hatch(doughnut):
    zig = penfill.zigzag(doughnut, 1.0, 45)
    hatch = penfill.rectilinear(doughnut, 1.0, 45)
    assert 0 < len(zig) < len(hatch) / 4
    inside(doughnut, zig)
    assert isinstance(zig[0][0], tuple)


@pytest.mark.parametrize(
    "fill",
    [
        lambda r: penfill.lines(r, 1.5),
        lambda r: penfill.grid(r, 2, 10),
        lambda r: penfill.triangles(r, 3),
        lambda r: penfill.concentric(r, 1.5),
        lambda r: penfill.spiral(r, 1.5),
        lambda r: penfill.honeycomb(r, 4),
        lambda r: penfill.dots(r, 2, lattice="hex"),
        lambda r: penfill.waves(r, 2),
        lambda r: penfill.wave_grid(r, 3),
        lambda r: penfill.scales(r, 3),
        lambda r: penfill.sunburst(r, 2),
        lambda r: penfill.stipple(r, 1.5, seed=3),
        lambda r: penfill.stipple_noise(r, 1, 4),
        lambda r: penfill.stipple_edge(r, 1, 4),
        lambda r: penfill.gravel(r, 4),
        lambda r: penfill.gravel(r, 4, clipped=True),
        lambda r: penfill.grass(r, 3),
        lambda r: penfill.scribble(r, 2),
        lambda r: penfill.truchet(r, 4),
        lambda r: penfill.marble(r, 5),
        lambda r: penfill.wood(r, 2),
        lambda r: penfill.cracks(r, 6),
        lambda r: penfill.terrazzo(r, 6),
        lambda r: penfill.chess(r, 6),
        lambda r: penfill.pat(r, "BRICK", 2),
    ],
)
def test_every_fill_draws_inside(doughnut, fill):
    strokes = fill(doughnut)
    assert strokes, "empty fill"
    inside(doughnut, strokes)


def test_seed_is_deterministic(doughnut):
    assert penfill.gravel(doughnut, 4, seed=5) == penfill.gravel(doughnut, 4, seed=5)
    assert penfill.gravel(doughnut, 4, seed=5) != penfill.gravel(doughnut, 4, seed=6)


def test_patterns():
    assert "HERRINGBONE" in penfill.PATTERNS
    source = "*STRIPES, just lines\n0, 0,0, 0,2\n"
    assert penfill.pat_names(source) == ["STRIPES"]
    r = penfill.Region(SQUARE)
    assert penfill.pat(r, "stripes", source=source)
    with pytest.raises(KeyError):
        penfill.pat(r, "NOPE")


def test_isolines_and_profile(doughnut):
    lines = penfill.isolines(doughnut, lambda x, y: math.sin(x / 3) * math.cos(y / 3), penfill.levels_every(-0.9, 0.9, 0.3))
    assert lines
    inside(doughnut, lines)
    assert penfill.profile(doughnut, 2, lambda x: (x / 4) % 1)


def test_callback_errors_propagate(doughnut):
    def bad(x, y):
        raise RuntimeError("boom")

    with pytest.raises(RuntimeError, match="boom"):
        penfill.isolines(doughnut, bad, [0.0])
    with pytest.raises(TypeError):
        penfill.stipple_field(doughnut, 1, 4, lambda x, y: "not a number")


def test_svg_in_and_out(doughnut):
    svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'><circle cx='5' cy='5' r='3'/></svg>"
    tiles = penfill.svg_tile(doughnut, svg, 5)
    assert tiles
    inside(doughnut, tiles)
    art = penfill.svg_strokes(svg, width=40)
    assert max(x for s in art for x, _ in s) == pytest.approx(32, abs=0.1)
    clipped = doughnut.clip(art)
    inside(doughnut, clipped)
    doc = penfill.to_svg(clipped, outline=doughnut)
    assert doc.startswith("<svg") and "mm" in doc
    with pytest.raises(ValueError):
        penfill.svg_tile(doughnut, "<svg", 5)


RAMP = [[x / 31 for x in range(32)] for _ in range(32)]  # black on the left, white on the right


def grey_png(rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", len(rows[0]), len(rows), 8, 0, 0, 0, 0)
    pixels = zlib.compress(b"".join(b"\x00" + bytes(r) for r in rows))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", pixels) + chunk(b"IEND", b"")


def left_right_ink(strokes):
    left = sum(abs(s[-1][0] - s[0][0]) + 0.1 for s in strokes if s[0][0] < 14)
    right = sum(abs(s[-1][0] - s[0][0]) + 0.1 for s in strokes if s[0][0] > 26)
    return left, right


@pytest.mark.parametrize("method", ["threshold", "random", "bayer8", "halftone", "floyd-steinberg", "atkinson", "jarvis", "stucki", "sierra"])
@pytest.mark.parametrize("marks", ["lines", "dots"])
def test_dither_follows_the_picture(doughnut, method, marks):
    strokes = penfill.dither(doughnut, RAMP, resolution=0.5, method=method, marks=marks)
    inside(doughnut, strokes)
    left, right = left_right_ink(strokes)
    assert left > 3 * right


def test_dither_inputs(tmp_path, doughnut):
    rgb = [[(v * 255, v * 255, v * 255) for v in row] for row in RAMP]
    assert penfill.dither(doughnut, rgb) == penfill.dither(doughnut, RAMP)
    png = grey_png([[0, 255]])  # black, white
    from_bytes = penfill.dither(doughnut, png)
    (tmp_path / "ramp.png").write_bytes(png)
    assert penfill.dither(doughnut, str(tmp_path / "ramp.png")) == from_bytes
    left, right = left_right_ink(from_bytes)
    assert left > 3 * right
    assert penfill.dither(doughnut, RAMP, invert=True) != penfill.dither(doughnut, RAMP)
    with pytest.raises(ValueError, match="unknown dither"):
        penfill.dither(doughnut, RAMP, method="nope")
    with pytest.raises(ValueError):
        penfill.dither(doughnut, b"not an image")
    with pytest.raises(ValueError, match="same length"):
        penfill.dither(doughnut, [[0, 1], [0]])


def test_dither_placed_like_css():
    square = penfill.Region(SQUARE)
    black = [[0.0]]
    ink = lambda strokes: sum(abs(s[-1][0] - s[0][0]) for s in strokes)
    full = ink(penfill.dither(square, black))
    corner = penfill.dither(square, black, size=20, position="top left")
    assert ink(corner) == pytest.approx(full / 4, rel=0.03)
    assert all(x <= 20 + 1e-6 and y <= 20 + 1e-6 for s in corner for x, y in s)
    row = penfill.dither(square, black, size=(40, 10), position="top", repeat="repeat-x")
    assert ink(row) == pytest.approx(full / 4, rel=0.03)
    assert penfill.dither(square, RAMP, size="10", repeat="repeat") != penfill.dither(square, RAMP)
    with pytest.raises(ValueError, match="repeat"):
        penfill.dither(square, black, repeat="tile")
    with pytest.raises(ValueError, match="size"):
        penfill.dither(square, black, size="huge")


def test_svg_fill_placed(doughnut):
    dot = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'><rect x='4' y='4' width='2' height='2'/></svg>"
    square = penfill.Region(SQUARE)
    assert len(penfill.svg_fill(square, dot)) == 1
    assert len(penfill.svg_fill(square, dot, size=10, position="top left", repeat="repeat")) == 16
    assert len(penfill.svg_fill(square, dot, size="10", position="top left", repeat="repeat-y")) == 4
    inside(doughnut, penfill.svg_fill(doughnut, dot, size=5, repeat="repeat"))


def test_tile(doughnut):
    cross = [[(0, 1), (2, 1)], [(1, 0), (1, 2)]]
    assert penfill.tile(doughnut, cross, 3, 3, angle=15, row_shift=0.5)


def test_pattern_draws_anywhere_and_is_clipped(doughnut):
    seen = {}

    def circles(canvas):
        seen["bounds"], seen["seed"] = canvas.bounds, canvas.seed
        assert isinstance(canvas.region, penfill.Region)
        x0, y0, x1, y1 = canvas.bounds
        return [[(x + math.cos(a / 6), y + math.sin(a / 6)) for a in range(38)]
                for x in range(int(x0) - 5, int(x1) + 5, 4) for y in range(int(y0) - 5, int(y1) + 5, 4)]

    strokes = penfill.pattern(doughnut, circles, seed=3)
    assert seen == {"bounds": pytest.approx((0, 0, 40, 40)), "seed": 3}
    assert strokes
    inside(doughnut, strokes)
    assert penfill.pattern(doughnut, lambda c: penfill.zigzag(c.region, 1.0)) == penfill.zigzag(doughnut, 1.0)


def test_pattern_parts_and_errors():
    two = penfill.Region.from_polygons([[SQUARE, HOLE], [[(50, 0), (60, 0), (60, 10), (50, 10)]]])
    parts = two.parts()
    assert sorted(len(p.rings) for p in parts) == [1, 2]
    per_piece = penfill.pattern(two, lambda c: [s for p in c.region.parts() for s in penfill.lines(p, 2.0)])
    assert len(per_piece) == len(penfill.lines(two, 2.0))

    def broken(canvas):
        raise RuntimeError("boom")

    with pytest.raises(RuntimeError, match="boom"):
        penfill.pattern(two, broken)
    with pytest.raises(TypeError):
        penfill.pattern(two, lambda c: 42)


def test_the_tools_the_fills_are_made_of(doughnut):
    assert doughnut.distance((5, 20)) == pytest.approx(5)
    assert doughnut.distance((20, 12)) == pytest.approx(3)
    got = {}

    def draw(canvas, tools):
        assert tools.noise(0.3, 0.7) == tools.noise(0.3, 0.7)
        assert tools.noise(0.3, 0.7) != tools.on(canvas.region, seed=2).noise(0.3, 0.7)
        assert abs(tools.noise(3.3, 1.2, octaves=3)) <= 1.5
        got["points"] = tools.scatter(3)
        got["field"] = tools.scatter_field(1, 5, lambda x, y: x / 40)
        got["piece"] = tools.on(canvas.region.inset(5)).scatter(3)
        return tools.hatch(2, angle=30)

    inside(doughnut, penfill.pattern(doughnut, draw, seed=4))
    assert len(got["points"]) > 50 and all(doughnut.contains(p) for p in got["points"])
    field = got["field"]
    assert len([p for p in field if p[0] < 20]) > 2 * len([p for p in field if p[0] >= 20])
    assert all(5 - 1e-6 <= x <= 35 + 1e-6 for x, _ in got["piece"])
    assert penfill.pattern(doughnut, lambda canvas: []) == []
    assert penfill.pattern(doughnut, lambda *args: [[(1, 1), (39, 39)]])


def test_stipple_edge_rebuilt_from_the_tools(doughnut):
    def edge(canvas, tools):
        def density(x, y):
            near = min(canvas.region.distance((x, y)) / 8.0, 1.0)
            return near + 0.5 * tools.noise(x / 8.0, y / 8.0, octaves=2) * 0.5
        return penfill.stipple_field(canvas.region, 0.8, 4.0, density, mark=0.32, seed=canvas.seed)

    assert penfill.pattern(doughnut, edge, seed=1) == penfill.stipple_edge(doughnut, 0.8, 4.0, mark=0.32, falloff=8.0, rough=0.5, seed=1)


def test_a_global_anchor_lines_two_regions_up_into_one_lattice():
    whole = penfill.Region([(0, 0), (40, 0), (40, 20), (0, 20)])
    halves = [penfill.Region([(0, 0), (20, 0), (20, 20), (0, 20)]), penfill.Region([(20, 0), (40, 0), (40, 20), (20, 20)])]

    def marks(r, **kw):
        # a mark the border cuts in two starts again on the cut line
        return sorted(tuple(round(v, 3) for v in s[0]) for s in penfill.dots(r, 1.5, angle=30, **kw) if abs(s[0][0] - 20) > 1e-6)

    kw = dict(anchor="global", shift=(0.3, 0.7))
    assert sorted(m for h in halves for m in marks(h, **kw)) == marks(whole, **kw)
    assert sorted(m for h in halves for m in marks(h)) != marks(whole)
    with pytest.raises(ValueError, match="anchor must be"):
        penfill.dots(whole, 1.5, anchor="page")
