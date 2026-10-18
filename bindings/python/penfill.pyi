"""Fills and hatches for pen plotters. All lengths are in millimetres."""

from typing import Any, Callable, Iterable, Literal, Optional, Sequence, Union

Point = tuple[float, float]
PointLike = Sequence[float]
Stroke = list[Point]
Strokes = list[Stroke]
CssValue = Union[str, float, tuple[float, float]]
Anchor = Literal["local", "global"]
EdgeSpec = Union[Literal["clip", "inside", "overflow"], tuple[Literal["overflow"], float], dict[str, float], float]
"""Boundary mode: `"clip"` (default), `"inside"` (whole elements only), or
`("overflow", mm)` / `{"overflow": mm}` (whole elements may overhang up to `mm`)."""

__version__: str
PATTERNS: list[str]
"""Names of the built-in `.pat` patterns."""
FILLS: list[str]
"""Names of all built-in fills supported by `fill` and `fill_output`."""

class Region:
    """A filled area: outer rings with optional holes."""

    def __init__(self, outer: Iterable[PointLike], holes: Optional[Iterable[Iterable[PointLike]]] = None) -> None: ...
    @staticmethod
    def from_polygons(polygons: Iterable[Iterable[Iterable[PointLike]]]) -> Region:
        """Several polygons, each written as `[outer, hole, hole, ...]`."""
    @staticmethod
    def around_strokes(strokes: Iterable[Iterable[PointLike]], radius: float) -> Region:
        """Build a polygon region around `strokes` inflated by `radius` mm."""
    def inset(self, mm: float) -> Region: ...
    def outset(self, mm: float) -> Region: ...
    def union(self, other: Region) -> Region: ...
    def intersection(self, other: Region) -> Region: ...
    def difference(self, other: Region) -> Region: ...
    def clip(self, strokes: Iterable[Iterable[PointLike]]) -> Strokes:
        """Cut any polylines to the region."""
    def fray(self, strokes: Iterable[Iterable[PointLike]], spacing: float, fray: float, seed: int = 0) -> Strokes:
        """Clip and apply perimeter edge fraying (`fray` in mm) to any polylines."""
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

class FillOutput:
    """Output of a fill with its polygon `mask`: `out.strokes` and `out.mask`
    (also unpacks as `strokes, mask = out`)."""

    @property
    def strokes(self) -> Strokes: ...
    @property
    def mask(self) -> Region: ...
    def __len__(self) -> int: ...
    def __getitem__(self, idx: int) -> Any: ...

class Canvas:
    """What `pattern` hands your function."""

    @property
    def region(self) -> Region:
        """Everything to fill, holes included (after `inset`)."""
    @property
    def limit(self) -> Region:
        """Effective outer clipping envelope (outset in `overflow` mode)."""
    @property
    def bounds(self) -> tuple[float, float, float, float]:
        """`(min_x, min_y, max_x, max_y)` of the whole region."""
    @property
    def spacing(self) -> float: ...
    @property
    def angle(self) -> float: ...
    @property
    def seed(self) -> int: ...
    @property
    def origin(self) -> Point:
        """The anchor resolved for this region: the point to build a lattice through."""
    @property
    def fray(self) -> float: ...

# universal registry entry points
def fill(
    region: Region,
    name: str,
    spacing: float = 2.0,
    *,
    angle: float = 0.0,
    seed: int = 0,
    inset: Optional[float] = None,
    edge: Optional[EdgeSpec] = None,
    fray: float = 0.0,
    anchor: Anchor = "local",
    shift: Point = (0.0, 0.0),
    with_mask: bool = False,
) -> Union[Strokes, FillOutput]: ...

def fill_output(
    region: Region,
    name: str,
    spacing: float = 2.0,
    *,
    angle: float = 0.0,
    seed: int = 0,
    inset: Optional[float] = None,
    edge: Optional[EdgeSpec] = None,
    fray: float = 0.0,
    anchor: Anchor = "local",
    shift: Point = (0.0, 0.0),
) -> FillOutput: ...

# lines and paths
def rectilinear(region: Region, spacing: float, angle: float = 0.0, inset: Optional[float] = None, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, seed: int = 0, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def zigzag(region: Region, spacing: float, angle: float = 0.0, inset: Optional[float] = None, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, seed: int = 0, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def lines(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def grid(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def triangles(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def concentric(region: Region, spacing: float, inset: Optional[float] = None, *, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def spiral(region: Region, spacing: float, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def hilbert(region: Region, spacing: float, inset: Optional[float] = None, *, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...

# lattices
def honeycomb(region: Region, spacing: float, angle: float = 0.0, line_width: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def dots(region: Region, spacing: float, angle: float = 0.0, mark: Optional[float] = None, lattice: str = "square", anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def waves(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def wave_grid(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def scales(region: Region, spacing: float, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def sunburst(region: Region, spacing: float, angle: float = 0.0, centre: Optional[Point] = None, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...

# scatter and nature
def stipple(region: Region, spacing: float, mark: Optional[float] = None, seed: int = 0, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def stipple_noise(region: Region, dense: float, sparse: float, mark: Optional[float] = None, feature: float = 12.0, seed: int = 0, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def stipple_edge(region: Region, dense: float, sparse: float, mark: Optional[float] = None, falloff: float = 8.0, rough: float = 0.5, seed: int = 0, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def stipple_field(region: Region, dense: float, sparse: float, field: Callable[[float, float], float], mark: Optional[float] = None, seed: int = 0, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def gravel(region: Region, spacing: float, seed: int = 0, clipped: bool = False, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def grass(region: Region, spacing: float, angle: float = 0.0, seed: int = 0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def scribble(region: Region, spacing: float, angle: float = 0.0, seed: int = 0, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def truchet(region: Region, spacing: float, angle: float = 0.0, seed: int = 0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...

# materials, textures & nature
def marble(region: Region, spacing: float, angle: float = 0.0, seed: int = 0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def wood(region: Region, spacing: float, angle: float = 0.0, seed: int = 0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def cracks(region: Region, spacing: float, seed: int = 0, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def terrazzo(region: Region, spacing: float, hatch: Optional[float] = None, seed: int = 0, *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def chess(region: Region, spacing: float, hatch: Optional[float] = None, angle: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def pebbles(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def leaves(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def sprouts(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def fluid_grass(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def lily_pads(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def canopy(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def tufts(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def twigs(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def vines(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def cell_net(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def eddies(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def swells(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def ripples(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def growth(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def isoline_blobs(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def isoline_stack(region: Region, spacing: float, *, angle: float = 0.0, seed: int = 0, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, anchor: Anchor = "local", shift: Point = (0.0, 0.0), with_mask: bool = False) -> Union[Strokes, FillOutput]: ...

# CAD patterns
def pat(region: Region, name: str, scale: float = 1.0, angle: float = 0.0, source: Optional[str] = None, anchor: Anchor = "local", shift: Point = (0.0, 0.0), *, inset: Optional[float] = None, edge: Optional[EdgeSpec] = None, fray: float = 0.0, seed: int = 0, with_mask: bool = False) -> Union[Strokes, FillOutput]: ...
def pat_names(source: str) -> list[str]: ...

# bring your own
class Tools:
    """The toolkit `pattern` hands your function next to the canvas: what the
    built-in fills are made of, set to the canvas region and seed."""

    def on(self, region: Region, seed: Optional[int] = None) -> Tools:
        """The same tools for another region, e.g. a piece from `region.parts()`."""
    def noise(self, x: float, y: float, octaves: int = 1) -> float:
        """Smooth Perlin noise, roughly -1 to 1, one bump per unit."""
    def whole_at(self, point: PointLike, fray: Optional[float] = None) -> float:
        """Smooth border-fade factor in `[0, 1]` at `point`."""
    def border_ramp(self, point: PointLike, falloff: float = 8.0, rough: float = 0.5) -> float:
        """Distance ramp from the wall (`0` at wall, `1` past `falloff` mm)."""
    def element_fits(self, centre: PointLike, radius: float) -> bool:
        """Whether a discrete element of `radius` at `centre` fits the active `edge` mode."""
    def scatter(self, spacing: float) -> list[Point]:
        """Random points in the region, no two closer than `spacing` mm."""
    def scatter_field(self, dense: float, sparse: float, field: Callable[[float, float], float]) -> list[Point]:
        """Random points `dense` mm apart where `field(x, y)` is 0 and `sparse` mm apart where it is 1."""
    def hatch(self, spacing: float, angle: float = 0.0) -> Strokes:
        """Parallel lines `spacing` mm apart, cut to the region right up to its walls."""
    def voronoi_cells(self, centres: Iterable[PointLike], reach: Optional[float] = None) -> Strokes:
        """Bounded Voronoi cell rings for `centres`."""
    def voronoi_edges(self, centres: Iterable[PointLike], reach: Optional[float] = None) -> list[tuple[Point, Point]]:
        """Deduplicated shared Voronoi edges `(a, b)` between `centres`."""
    def circle(self, centre: PointLike, r: float) -> Stroke:
        """Closed circle polyline of radius `r` around `centre`."""
    def lobe(self, centre: PointLike, r: float, phase: float = 0.0, rough: float = 0.14) -> Stroke:
        """Irregular foliage/canopy lobe outline around `centre`."""
    def arc(self, centre: PointLike, r: float, a0: float, a1: float, steps: int = 16) -> Stroke:
        """Circular arc around `centre` of radius `r` from `a0` to `a1` radians."""
    def tick(self, centre: PointLike, angle_rad: float, half: float) -> Stroke:
        """Straight tick centred at `centre` at `angle_rad` radians with half-length `half`."""
    def cross(self, centre: PointLike, half: float, twist_rad: float = 0.0) -> Strokes:
        """Orthogonal `+` cross centred at `centre`."""
    def x_cross(self, centre: PointLike, half: float, twist_rad: float = 0.0) -> Strokes:
        """Diagonal `×` cross centred at `centre`."""
    def smooth(self, pts: Iterable[PointLike], rounds: int = 2) -> Stroke:
        """Chaikin corner-cutting smoothing over `rounds` passes."""
    def resample(self, pts: Iterable[PointLike], step: float) -> Stroke:
        """Resample a polyline so consecutive vertices are at most `step` mm apart."""
    def add_mask_ring(self, ring: Iterable[PointLike]) -> None:
        """Record a closed ring (in mm) as part of this pattern's semantic mask."""
    def add_mask(self, region: Region) -> None:
        """Record a `Region` as part of this pattern's semantic mask."""

def pattern(
    region: Region,
    draw: Union[Callable[[Canvas], Iterable[Iterable[PointLike]]], Callable[[Canvas, Tools], Iterable[Iterable[PointLike]]]],
    seed: int = 0,
    anchor: Anchor = "local",
    shift: Point = (0.0, 0.0),
    *,
    spacing: float = 2.0,
    angle: float = 0.0,
    inset: Optional[float] = None,
    edge: Optional[EdgeSpec] = None,
    fray: float = 0.0,
    with_mask: bool = False,
) -> Union[Strokes, FillOutput]:
    """Your own pattern: `draw(canvas, tools)` returns lines anywhere on the
    page and they come back clipped (and optionally frayed) to the region."""

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
) -> Strokes: ...
def svg_fill(
    region: Region,
    svg: str,
    size: Optional[CssValue] = None,
    position: Optional[CssValue] = None,
    repeat: str = "no-repeat",
) -> Strokes: ...
def to_svg(strokes: Iterable[Iterable[PointLike]], outline: Optional[Region] = None, stroke_width: float = 0.3) -> str: ...
