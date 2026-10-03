"""Fills and hatches for pen plotters. All lengths are in millimetres."""

from typing import Any, Callable, Iterable, Literal, Optional, Sequence, Union

Point = tuple[float, float]
PointLike = Sequence[float]
Stroke = list[Point]
Strokes = list[Stroke]
CssValue = Union[str, float, tuple[float, float]]
Anchor = Literal["local", "global"]
"""Where a lattice pattern sits: "local" places it on its own shape, "global"
gives the page one lattice, so neighbouring regions line up with no seam.
`shift` (mm) moves it from there."""
"""A CSS string ("cover", "top left", "40 30", "50%"), or mm as a number or pair."""

__version__: str
PATTERNS: list[str]
"""Names of the built-in `.pat` patterns."""

class Region:
    """A filled area: outer rings with optional holes."""

    def __init__(self, outer: Iterable[PointLike], holes: Optional[Iterable[Iterable[PointLike]]] = None) -> None: ...
    @staticmethod
    def from_polygons(polygons: Iterable[Iterable[Iterable[PointLike]]]) -> Region:
        """Several polygons, each written as `[outer, hole, hole, ...]`."""
    def inset(self, mm: float) -> Region: ...
    def outset(self, mm: float) -> Region: ...
    def intersection(self, other: Region) -> Region: ...
    def difference(self, other: Region) -> Region: ...
    def clip(self, strokes: Iterable[Iterable[PointLike]]) -> Strokes:
        """Cut any polylines to the region."""
    def contains(self, point: PointLike) -> bool: ...
    def distance(self, point: PointLike) -> float:
        """How far `point` is from the nearest wall or hole, mm."""
    @property
    def bounds(self) -> tuple[float, float, float, float]: ...
    @property
    def rings(self) -> list[list[Point]]: ...
    @property
    def is_empty(self) -> bool: ...
    def parts(self) -> list[Region]:
        """The separate pieces, each with its own holes. Call it only when a
        pattern needs pieces one by one: it costs a pass over the rings."""

class Canvas:
    """What `pattern` hands your function."""

    @property
    def region(self) -> Region:
        """Everything to fill, holes included."""
    @property
    def bounds(self) -> tuple[float, float, float, float]:
        """`(min_x, min_y, max_x, max_y)` of the whole region."""
    @property
    def seed(self) -> int: ...
    @property
    def origin(self) -> Point:
        """The anchor resolved for this region: the point to build a lattice through."""

# lines and paths
def rectilinear(region: Region, spacing: float, angle: float = 0.0, inset: Optional[float] = None, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def zigzag(region: Region, spacing: float, angle: float = 0.0, inset: Optional[float] = None, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def lines(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def grid(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def triangles(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def concentric(region: Region, spacing: float, inset: Optional[float] = None) -> Strokes: ...
def spiral(region: Region, spacing: float) -> Strokes: ...
def hilbert(region: Region, spacing: float, inset: Optional[float] = None) -> Strokes: ...

# lattices
def honeycomb(region: Region, spacing: float, angle: float = 0.0, line_width: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def dots(region: Region, spacing: float, angle: float = 0.0, mark: Optional[float] = None, lattice: str = "square", anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def waves(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def wave_grid(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def scales(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def sunburst(region: Region, spacing: float, angle: float = 0.0, centre: Optional[Point] = None, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...

# scatter and nature
def stipple(region: Region, spacing: float, mark: Optional[float] = None, seed: int = 0) -> Strokes: ...
def stipple_noise(region: Region, dense: float, sparse: float, mark: Optional[float] = None, feature: float = 12.0, seed: int = 0) -> Strokes: ...
def stipple_edge(region: Region, dense: float, sparse: float, mark: Optional[float] = None, falloff: float = 8.0, rough: float = 0.5, seed: int = 0) -> Strokes: ...
def stipple_field(region: Region, dense: float, sparse: float, field: Callable[[float, float], float], mark: Optional[float] = None, seed: int = 0) -> Strokes: ...
def gravel(region: Region, spacing: float, seed: int = 0, clipped: bool = False) -> Strokes: ...
def grass(region: Region, spacing: float, angle: float = 0.0, seed: int = 0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def scribble(region: Region, spacing: float, angle: float = 0.0, seed: int = 0) -> Strokes: ...
def truchet(region: Region, cell: float, angle: float = 0.0, seed: int = 0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...

# materials
def marble(region: Region, spacing: float, angle: float = 0.0, seed: int = 0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def wood(region: Region, spacing: float, angle: float = 0.0, seed: int = 0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def cracks(region: Region, spacing: float, seed: int = 0) -> Strokes: ...
def terrazzo(region: Region, size: float, hatch: Optional[float] = None, seed: int = 0) -> Strokes: ...
def chess(region: Region, cell: float, spacing: Optional[float] = None, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...

# CAD patterns
def pat(region: Region, name: str, scale: float = 1.0, angle: float = 0.0, source: Optional[str] = None, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def pat_names(source: str) -> list[str]: ...

# bring your own
class Tools:
    """The toolkit `pattern` hands your function next to the canvas: what the
    built-in fills are made of, set to the canvas region and seed. Optional:
    use it, or anything else."""

    def on(self, region: Region, seed: Optional[int] = None) -> Tools:
        """The same tools for another region, e.g. a piece from `region.parts()`."""
    def noise(self, x: float, y: float, octaves: int = 1) -> float:
        """Smooth Perlin noise, roughly -1 to 1, one bump per unit."""
    def scatter(self, spacing: float) -> list[Point]:
        """Random points in the region, no two closer than `spacing` mm."""
    def scatter_field(self, dense: float, sparse: float, field: Callable[[float, float], float]) -> list[Point]:
        """Random points `dense` mm apart where `field(x, y)` is 0 and `sparse` mm apart where it is 1."""
    def hatch(self, spacing: float, angle: float = 0.0) -> Strokes:
        """Parallel lines `spacing` mm apart, cut to the region right up to its
        walls, placed by the pattern's anchor."""

def pattern(
    region: Region,
    draw: Union[Callable[[Canvas], Iterable[Iterable[PointLike]]], Callable[[Canvas, Tools], Iterable[Iterable[PointLike]]]],
    seed: int = 0,
    anchor: Anchor = "local",
    shift: Point = (0.0, 0.0),
) -> Strokes:
    """Your own pattern: `draw(canvas, tools)` returns lines anywhere on the
    page and they come back clipped to the region. `tools` is optional:
    `draw(canvas)` works too."""

def isolines(region: Region, f: Callable[[float, float], float], levels: Sequence[float], cell: float = 0.2) -> Strokes: ...
def levels_every(lo: float, hi: float, step: float) -> list[float]: ...
def profile(region: Region, spacing: float, f: Callable[[float], float], angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def tile(region: Region, strokes: Iterable[Iterable[PointLike]], width: float, height: float, angle: float = 0.0, row_shift: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def svg_tile(region: Region, svg: str, width: float, angle: float = 0.0, row_shift: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0)) -> Strokes: ...
def svg_strokes(svg: str, width: Optional[float] = None) -> Strokes: ...
def dither(
    region: Region,
    image: Union[str, bytes, Any],
    resolution: float = 0.5,
    line_spacing: Optional[float] = None,
    method: str = "floyd-steinberg",
    marks: str = "lines",
    angle: float = 0.0,
    gamma: float = 1.0,
    invert: bool = False,
    mark: Optional[float] = None,
    seed: int = 0,
    size: Optional[CssValue] = None,
    position: Optional[CssValue] = None,
    repeat: str = "no-repeat",
) -> Strokes:
    """A picture dithered inside the region.

    The picture is placed like a CSS background: `size` ("cover" by default,
    "contain", "fill", a width in mm, "40 30", "50%"), `position` ("center",
    "top left", "25% 75%", mm offsets) and `repeat` ("no-repeat", "repeat",
    "repeat-x", "repeat-y"). Where no copy falls there is paper.

    `image` is a file path, the bytes of a PNG/JPEG/GIF/BMP/WebP file, or rows
    of pixels (a numpy array or nested lists, grey or RGB(A), 0–1 or 0–255).
    `method`: threshold, random, bayer (bayer2–bayer16), halftone
    (halftone2–halftone16), floyd-steinberg, atkinson, jarvis, stucki, sierra.
    `marks`: "lines" joins dark cells along each line into strokes; "dots"
    draws a tick per dark cell.
    """

def svg_fill(
    region: Region,
    svg: str,
    size: Optional[CssValue] = None,
    position: Optional[CssValue] = None,
    repeat: str = "no-repeat",
) -> Strokes:
    """An SVG drawing placed like a CSS background ("contain", centred, no
    repeat by default) and cut by the region."""

def to_svg(strokes: Iterable[Iterable[PointLike]], outline: Optional[Region] = None, stroke_width: float = 0.3) -> str: ...
