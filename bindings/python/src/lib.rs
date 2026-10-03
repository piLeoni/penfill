//! Python bindings: `import penfill`.
//!
//! Points come in as any sequence of `(x, y)` pairs (tuples, lists, numpy
//! rows) and go out as lists of `(x, y)` tuples. Everything is in mm.

use penfill_core as pf;
use pf::geo::{Pt, Region as CoreRegion, Strokes};
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use std::cell::RefCell;

fn points(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Pt>> {
    let mut out = Vec::new();
    for item in obj.try_iter()? {
        let xy: Vec<f64> = item?.extract()?;
        if xy.len() < 2 {
            return Err(PyValueError::new_err("each point needs an x and a y"));
        }
        out.push((xy[0], xy[1]));
    }
    Ok(out)
}

fn stroke_list(obj: &Bound<'_, PyAny>) -> PyResult<Strokes> {
    obj.try_iter()?.map(|s| points(&s?)).collect()
}

/// A filled area in mm: outer rings with optional holes.
#[pyclass(module = "penfill", frozen, skip_from_py_object)]
#[derive(Clone)]
struct Region {
    inner: CoreRegion,
}

#[pymethods]
impl Region {
    /// One polygon: an outer ring and optional holes.
    #[new]
    #[pyo3(signature = (outer, holes = None))]
    fn new(outer: &Bound<'_, PyAny>, holes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut rings = vec![points(outer)?];
        if let Some(h) = holes {
            rings.extend(stroke_list(h)?);
        }
        Ok(Region { inner: CoreRegion::from_polygons(&[rings]) })
    }

    /// Several polygons, each written as `[outer, hole, hole, ...]`.
    #[staticmethod]
    fn from_polygons(polygons: &Bound<'_, PyAny>) -> PyResult<Self> {
        let polys: Vec<Vec<Vec<Pt>>> = polygons.try_iter()?.map(|p| stroke_list(&p?)).collect::<PyResult<_>>()?;
        Ok(Region { inner: CoreRegion::from_polygons(&polys) })
    }

    /// Shrunk by `mm` (mitred corners).
    fn inset(&self, mm: f64) -> Self {
        Region { inner: self.inner.inset(mm) }
    }

    /// Grown by `mm` (round corners).
    fn outset(&self, mm: f64) -> Self {
        Region { inner: self.inner.outset(mm) }
    }

    fn intersection(&self, other: &Region) -> Self {
        Region { inner: self.inner.intersection(&other.inner) }
    }

    fn difference(&self, other: &Region) -> Self {
        Region { inner: self.inner.difference(&other.inner) }
    }

    /// Cut any polylines to the region.
    fn clip(&self, strokes: &Bound<'_, PyAny>) -> PyResult<Strokes> {
        Ok(self.inner.clip_strokes(&stroke_list(strokes)?))
    }

    fn contains(&self, point: &Bound<'_, PyAny>) -> PyResult<bool> {
        let xy: Vec<f64> = point.extract()?;
        Ok(xy.len() >= 2 && self.inner.covers_point((xy[0], xy[1])))
    }

    /// How far `point` is from the nearest wall or hole, mm.
    fn distance(&self, point: &Bound<'_, PyAny>) -> PyResult<f64> {
        let xy: Vec<f64> = point.extract()?;
        match xy.as_slice() {
            [x, y, ..] => Ok(self.inner.dist_to_boundary((*x, *y))),
            _ => Err(PyValueError::new_err("a point needs an x and a y")),
        }
    }

    /// `(min_x, min_y, max_x, max_y)`.
    #[getter]
    fn bounds(&self) -> (f64, f64, f64, f64) {
        self.inner.bounds_mm()
    }

    /// The separate pieces, each with its own holes. Costs a pass over the
    /// rings, so call it only when a pattern needs pieces one by one.
    fn parts(&self) -> Vec<Region> {
        self.inner.parts().into_iter().map(|inner| Region { inner }).collect()
    }

    /// Every ring, outers and holes.
    #[getter]
    fn rings(&self) -> Vec<Vec<Pt>> {
        self.inner.all_rings_mm()
    }

    #[getter]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn __repr__(&self) -> String {
        let (a, b, c, d) = self.inner.bounds_mm();
        format!("Region({} rings, bounds=({a:.2}, {b:.2}, {c:.2}, {d:.2}))", self.inner.all_rings_mm().len())
    }
}

/// Runs a fill that calls back into Python; the first exception raised by
/// the callback is re-raised once the fill returns.
fn with_callback<'py, T>(
    f: &Bound<'py, PyAny>,
    run: impl FnOnce(&dyn Fn(&[f64]) -> f64) -> T,
) -> PyResult<T> {
    let error: RefCell<Option<PyErr>> = RefCell::new(None);
    let call = |args: &[f64]| -> f64 {
        if error.borrow().is_some() {
            return f64::NAN;
        }
        let result = match args {
            [x] => f.call1((*x,)),
            [x, y] => f.call1((*x, *y)),
            _ => unreachable!(),
        };
        match result.and_then(|v| v.extract::<f64>()) {
            Ok(v) => v,
            Err(e) => {
                *error.borrow_mut() = Some(e);
                f64::NAN
            }
        }
    };
    let out = run(&call);
    match error.into_inner() {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// `anchor="local"`: the pattern placed on its own shape; `"global"`: one
/// lattice for the page, so neighbouring regions line up. `shift` moves it, mm.
fn anchor_of(origin: &str, shift: (f64, f64)) -> PyResult<pf::Anchor> {
    let origin = match origin {
        "local" => pf::Origin::Local,
        "global" => pf::Origin::Global,
        other => return Err(PyValueError::new_err(format!("anchor must be 'local' or 'global', not {other:?}"))),
    };
    Ok(pf::Anchor { origin, shift })
}

// ---- lines and paths ----

/// Parallel hatch, one stroke per fragment.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, inset = None, anchor = "local", shift = (0.0, 0.0)))]
fn rectilinear(region: &Region, spacing: f64, angle: f64, inset: Option<f64>, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::rectilinear::fill_rectilinear(&region.inner, spacing, angle, inset, anchor_of(anchor, shift)?))
}

/// Hatch joined into serpentines; the pen lifts only where a join would leave the shape.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, inset = None, anchor = "local", shift = (0.0, 0.0)))]
fn zigzag(region: &Region, spacing: f64, angle: f64, inset: Option<f64>, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::zigzag::fill_zigzag_inset(&region.inner, spacing, angle, inset, anchor_of(anchor, shift)?))
}

/// One-direction hatch over the whole region.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn lines(region: &Region, spacing: f64, angle: f64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::grid::fill_lines(&region.inner, spacing, angle, anchor_of(anchor, shift)?))
}

/// Two hatches at 90°.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn grid(region: &Region, spacing: f64, angle: f64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::grid::fill_grid(&region.inner, spacing, angle, anchor_of(anchor, shift)?))
}

/// Three hatches at 60° sharing every crossing.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn triangles(region: &Region, spacing: f64, angle: f64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::grid::fill_triangles(&region.inner, spacing, angle, anchor_of(anchor, shift)?))
}

/// Closed rings following the outline inwards.
#[pyfunction]
#[pyo3(signature = (region, spacing, inset = None))]
fn concentric(region: &Region, spacing: f64, inset: Option<f64>) -> Strokes {
    pf::concentric::fill_concentric(&region.inner, spacing, inset)
}

/// One continuous path winding inwards.
#[pyfunction]
fn spiral(region: &Region, spacing: f64) -> Strokes {
    pf::spiral::fill_spiral(&region.inner, spacing)
}

/// Space-filling Hilbert curve, one continuous path clipped to the region.
#[pyfunction]
#[pyo3(signature = (region, spacing, inset = None))]
fn hilbert(region: &Region, spacing: f64, inset: Option<f64>) -> Strokes {
    pf::hilbert::fill_hilbert(&region.inner, spacing, inset)
}

// ---- lattices ----

/// Hexagon walls, each drawn once; `spacing` is flat to flat.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, line_width = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn honeycomb(region: &Region, spacing: f64, angle: f64, line_width: f64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::honeycomb::fill_honeycomb(&region.inner, spacing, angle, line_width, anchor_of(anchor, shift)?))
}

/// Short marks on a lattice; `lattice` is "square" or "hex".
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, mark = None, lattice = "square", anchor = "local", shift = (0.0, 0.0)))]
fn dots(
    region: &Region,
    spacing: f64,
    angle: f64,
    mark: Option<f64>,
    lattice: &str,
    anchor: &str,
    shift: (f64, f64),
) -> PyResult<Strokes> {
    let lattice = match lattice {
        "square" => pf::dotswaves::Lattice::Square,
        "hex" | "hexagonal" => pf::dotswaves::Lattice::Hexagonal,
        other => return Err(PyValueError::new_err(format!("lattice must be 'square' or 'hex', not {other:?}"))),
    };
    Ok(pf::dotswaves::fill_dots_with_lattice(&region.inner, spacing, angle, mark.unwrap_or(spacing * 0.2), lattice, anchor_of(anchor, shift)?))
}

/// Parallel sine waves.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn waves(region: &Region, spacing: f64, angle: f64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::dotswaves::fill_waves(&region.inner, spacing, angle, anchor_of(anchor, shift)?))
}

/// Ogee: mirrored waves closing into diamonds.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn wave_grid(region: &Region, spacing: f64, angle: f64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::dotswaves::fill_wave_grid(&region.inner, spacing, angle, anchor_of(anchor, shift)?))
}

/// Fish scales; each row of arcs is one stroke.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn scales(region: &Region, spacing: f64, angle: f64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::scales::fill_scales(&region.inner, spacing, angle, anchor_of(anchor, shift)?))
}

/// Rays from the anchor (the middle of the bounds by default); `centre`
/// is a shorthand for `anchor="global", shift=centre`.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, centre = None, anchor = "local", shift = (0.0, 0.0)))]
fn sunburst(
    region: &Region,
    spacing: f64,
    angle: f64,
    centre: Option<(f64, f64)>,
    anchor: &str,
    shift: (f64, f64),
) -> PyResult<Strokes> {
    let anchor = match centre {
        Some((x, y)) => pf::Anchor::global(x, y),
        None => anchor_of(anchor, shift)?,
    };
    Ok(pf::radial::fill_sunburst(&region.inner, spacing, angle, anchor))
}

// ---- scatter and nature ----

/// Evenly spread short marks.
#[pyfunction]
#[pyo3(signature = (region, spacing, mark = None, seed = 0))]
fn stipple(region: &Region, spacing: f64, mark: Option<f64>, seed: u64) -> Strokes {
    pf::stipple::fill_stipple(&region.inner, spacing, mark.unwrap_or(spacing * 0.3), seed)
}

/// Stipple whose density drifts with Perlin noise.
#[pyfunction]
#[pyo3(signature = (region, dense, sparse, mark = None, feature = 12.0, seed = 0))]
fn stipple_noise(region: &Region, dense: f64, sparse: f64, mark: Option<f64>, feature: f64, seed: u64) -> Strokes {
    pf::stipple::fill_stipple_noise(&region.inner, dense, sparse, mark.unwrap_or(dense * 0.4), feature, seed)
}

/// Stipple dense along the wall, fading inwards over `falloff` mm.
#[pyfunction]
#[pyo3(signature = (region, dense, sparse, mark = None, falloff = 8.0, rough = 0.5, seed = 0))]
fn stipple_edge(
    region: &Region,
    dense: f64,
    sparse: f64,
    mark: Option<f64>,
    falloff: f64,
    rough: f64,
    seed: u64,
) -> Strokes {
    pf::stipple::fill_stipple_edge(&region.inner, dense, sparse, mark.unwrap_or(dense * 0.4), falloff, rough, seed)
}

/// Stipple with density from `field(x, y)`: 0 is `dense`, 1 is `sparse`.
#[pyfunction]
#[pyo3(signature = (region, dense, sparse, field, mark = None, seed = 0))]
fn stipple_field(
    region: &Region,
    dense: f64,
    sparse: f64,
    field: &Bound<'_, PyAny>,
    mark: Option<f64>,
    seed: u64,
) -> PyResult<Strokes> {
    with_callback(field, |f| {
        pf::stipple::fill_stipple_field(&region.inner, dense, sparse, mark.unwrap_or(dense * 0.4), seed, |p| f(&[p.0, p.1]))
    })
}

/// Rounded Voronoi stones; `clipped=True` cuts uniform stones at the wall
/// instead of shrinking them into the corners.
#[pyfunction]
#[pyo3(signature = (region, spacing, seed = 0, clipped = false))]
fn gravel(region: &Region, spacing: f64, seed: u64, clipped: bool) -> Strokes {
    let edge = if clipped { pf::gravel::GravelEdge::Clipped } else { pf::gravel::GravelEdge::Adaptive };
    pf::gravel::fill_gravel_with(&region.inner, spacing, seed, edge)
}

/// Grass tufts as a repeating hatch tile.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0, anchor = "local", shift = (0.0, 0.0)))]
fn grass(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::grass::fill_grass(&region.inner, spacing, angle, seed, anchor_of(anchor, shift)?))
}

/// Hand-drawn cross-hatching.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0))]
fn scribble(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    pf::scribble::fill_scribble(&region.inner, spacing, angle, seed)
}

/// Random quarter-circle tiles joined into winding paths.
#[pyfunction]
#[pyo3(signature = (region, cell, angle = 0.0, seed = 0, anchor = "local", shift = (0.0, 0.0)))]
fn truchet(region: &Region, cell: f64, angle: f64, seed: u64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::truchet::fill_truchet(&region.inner, cell, angle, seed, anchor_of(anchor, shift)?))
}

// ---- materials ----

/// Marble veins.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0, anchor = "local", shift = (0.0, 0.0)))]
fn marble(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::textures::fill_marble(&region.inner, spacing, angle, seed, anchor_of(anchor, shift)?))
}

/// Wood growth rings bending around knots.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0, anchor = "local", shift = (0.0, 0.0)))]
fn wood(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::textures::fill_wood(&region.inner, spacing, angle, seed, anchor_of(anchor, shift)?))
}

/// Parched ground / mud cracks (Voronoi edge network).
#[pyfunction]
#[pyo3(signature = (region, spacing, seed = 0))]
fn cracks(region: &Region, spacing: f64, seed: u64) -> Strokes {
    pf::textures::fill_cracks(&region.inner, spacing, seed)
}

/// Terrazzo chips; about a third are dark, hatched `hatch` mm apart.
#[pyfunction]
#[pyo3(signature = (region, size, hatch = None, seed = 0))]
fn terrazzo(region: &Region, size: f64, hatch: Option<f64>, seed: u64) -> Strokes {
    pf::terrazzo::fill_terrazzo(&region.inner, size, hatch.unwrap_or(size / 6.0), seed)
}

/// Checkerboard, dark squares hatched `spacing` mm apart.
#[pyfunction]
#[pyo3(signature = (region, cell, spacing = None, angle = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn chess(region: &Region, cell: f64, spacing: Option<f64>, angle: f64, anchor: &str, shift: (f64, f64)) -> PyResult<Strokes> {
    Ok(pf::chess::fill_chess(&region.inner, cell, spacing.unwrap_or(cell / 8.0), angle, anchor_of(anchor, shift)?))
}

// ---- CAD patterns ----

/// Hatch with an AutoCAD `.pat` pattern: a built-in `name` (see
/// `PATTERNS`), or a pattern of that name in `source`, the text of a
/// `.pat` file. `scale` is mm per pattern unit.
#[pyfunction]
#[pyo3(signature = (region, name, scale = 1.0, angle = 0.0, source = None, anchor = "local", shift = (0.0, 0.0)))]
fn pat(
    region: &Region,
    name: &str,
    scale: f64,
    angle: f64,
    source: Option<&str>,
    anchor: &str,
    shift: (f64, f64),
) -> PyResult<Strokes> {
    let pattern = match source {
        Some(text) => pf::pat::parse_pat(text).into_iter().find(|p| p.name.eq_ignore_ascii_case(name)),
        None => pf::pat::builtin(name),
    };
    let pattern = pattern.ok_or_else(|| PyKeyError::new_err(format!("no pattern named {name:?}")))?;
    Ok(pf::pat::fill_pat(&region.inner, &pattern, scale, angle, anchor_of(anchor, shift)?))
}

/// Names of the patterns in a `.pat` file's text.
#[pyfunction]
fn pat_names(source: &str) -> Vec<String> {
    pf::pat::parse_pat(source).into_iter().map(|p| p.name).collect()
}

// ---- bring your own ----

/// What `pattern` hands your function: the region to fill, its bounds and
/// the seed.
#[pyclass(module = "penfill", frozen)]
struct Canvas {
    #[pyo3(get)]
    region: Py<Region>,
    #[pyo3(get)]
    bounds: (f64, f64, f64, f64),
    #[pyo3(get)]
    seed: u64,
    /// The anchor resolved for this region: the point to build a lattice through.
    #[pyo3(get)]
    origin: Pt,
}

#[pymethods]
impl Canvas {
    fn __repr__(&self) -> String {
        let (a, b, c, d) = self.bounds;
        format!("Canvas(bounds=({a:.2}, {b:.2}, {c:.2}, {d:.2}), seed={})", self.seed)
    }
}

/// The toolkit `pattern` hands your function next to the canvas: what the
/// built-in fills are made of, set to the canvas region and seed.
#[pyclass(module = "penfill", frozen)]
struct Tools {
    region: CoreRegion,
    seed: u64,
    perlin: pf::noise::Perlin,
    anchor: pf::Anchor,
}

impl Tools {
    fn core(&self) -> pf::pattern::Tools<'_> {
        pf::pattern::Tools::new(&self.region, self.seed).anchored(self.anchor)
    }
}

#[pymethods]
impl Tools {
    /// The same tools for another region, e.g. a piece from `region.parts()`.
    #[pyo3(signature = (region, seed = None))]
    fn on(&self, region: &Region, seed: Option<u64>) -> Tools {
        let seed = seed.unwrap_or(self.seed);
        Tools { region: region.inner.clone(), seed, perlin: pf::noise::Perlin::new(seed), anchor: self.anchor }
    }

    /// Smooth Perlin noise, roughly -1 to 1, one bump per unit; each extra
    /// octave adds detail twice as fine and half as strong.
    #[pyo3(signature = (x, y, octaves = 1))]
    fn noise(&self, x: f64, y: f64, octaves: usize) -> f64 {
        self.perlin.fbm(x, y, octaves.max(1))
    }

    /// Random points in the region, no two closer than `spacing` mm.
    fn scatter(&self, spacing: f64) -> Vec<Pt> {
        self.core().scatter(spacing)
    }

    /// Random points `dense` mm apart where `field(x, y)` is 0 and `sparse`
    /// mm apart where it is 1.
    fn scatter_field(&self, dense: f64, sparse: f64, field: &Bound<'_, PyAny>) -> PyResult<Vec<Pt>> {
        with_callback(field, |f| self.core().scatter_field(dense, sparse, |p| f(&[p.0, p.1])))
    }

    /// Parallel lines `spacing` mm apart, cut to the region right up to its walls.
    #[pyo3(signature = (spacing, angle = 0.0))]
    fn hatch(&self, spacing: f64, angle: f64) -> Strokes {
        self.core().hatch(spacing, angle)
    }

    fn __repr__(&self) -> String {
        format!("Tools(seed={})", self.seed)
    }
}

/// Whether `draw` takes a second argument for the tools.
fn wants_tools(py: Python<'_>, draw: &Bound<'_, PyAny>) -> bool {
    let count = || -> PyResult<bool> {
        let params = py.import("inspect")?.call_method1("signature", (draw,))?.getattr("parameters")?;
        let mut positional = 0;
        for p in params.call_method0("values")?.try_iter()? {
            let kind: String = p?.getattr("kind")?.getattr("name")?.extract()?;
            match kind.as_str() {
                "VAR_POSITIONAL" => return Ok(true),
                "POSITIONAL_ONLY" | "POSITIONAL_OR_KEYWORD" => positional += 1,
                _ => {}
            }
        }
        Ok(positional >= 2)
    };
    count().unwrap_or(true)
}

/// Your own pattern: `draw(canvas, tools)` returns lines anywhere on the
/// page and they come back clipped to the region. `tools` is optional:
/// `draw(canvas)` works too.
#[pyfunction]
#[pyo3(signature = (region, draw, seed = 0, anchor = "local", shift = (0.0, 0.0)))]
fn pattern(
    py: Python<'_>,
    region: Py<Region>,
    draw: &Bound<'_, PyAny>,
    seed: u64,
    anchor: &str,
    shift: (f64, f64),
) -> PyResult<Strokes> {
    let anchor = anchor_of(anchor, shift)?;
    let inner = region.get().inner.clone();
    let with_tools = wants_tools(py, draw);
    let error: RefCell<Option<PyErr>> = RefCell::new(None);
    let out = pf::pattern::fill_pattern(&inner, seed, anchor, |c, _| {
        let canvas = Canvas { region: region.clone_ref(py), bounds: c.bounds, seed: c.seed, origin: c.origin };
        let result = if with_tools {
            let tools = Tools { region: inner.clone(), seed, perlin: pf::noise::Perlin::new(seed), anchor };
            draw.call1((canvas, tools))
        } else {
            draw.call1((canvas,))
        };
        match result.and_then(|s| stroke_list(&s)) {
            Ok(strokes) => strokes,
            Err(e) => {
                *error.borrow_mut() = Some(e);
                vec![]
            }
        }
    });
    match error.into_inner() {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// Contour lines of `f(x, y)` at `levels`, sampled every `cell` mm.
#[pyfunction]
#[pyo3(signature = (region, f, levels, cell = 0.2))]
fn isolines(region: &Region, f: &Bound<'_, PyAny>, levels: Vec<f64>, cell: f64) -> PyResult<Strokes> {
    with_callback(f, |f| pf::field::fill_isolines(&region.inner, cell, &levels, |p| f(&[p.0, p.1])))
}

/// `levels` helper: `lo, lo + step, ...` up to `hi`.
#[pyfunction]
fn levels_every(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    pf::field::levels_every(lo, hi, step)
}

/// `y = f(x)` drawn again every `spacing` mm, turned by `angle`.
#[pyfunction]
#[pyo3(signature = (region, spacing, f, angle = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn profile(
    region: &Region,
    spacing: f64,
    f: &Bound<'_, PyAny>,
    angle: f64,
    anchor: &str,
    shift: (f64, f64),
) -> PyResult<Strokes> {
    let anchor = anchor_of(anchor, shift)?;
    with_callback(f, |f| pf::field::fill_profile(&region.inner, spacing, angle, anchor, |x| f(&[x])))
}

/// Repeat `strokes`, drawn in a `width × height` box, like wallpaper.
#[pyfunction]
#[pyo3(signature = (region, strokes, width, height, angle = 0.0, row_shift = 0.0, anchor = "local", shift = (0.0, 0.0)))]
#[allow(clippy::too_many_arguments)]
fn tile(
    region: &Region,
    strokes: &Bound<'_, PyAny>,
    width: f64,
    height: f64,
    angle: f64,
    row_shift: f64,
    anchor: &str,
    shift: (f64, f64),
) -> PyResult<Strokes> {
    Ok(pf::tile::fill_tile(&region.inner, &stroke_list(strokes)?, width, height, angle, row_shift, anchor_of(anchor, shift)?))
}

/// Repeat an SVG drawing as a tile `width` mm wide.
#[pyfunction]
#[pyo3(signature = (region, svg, width, angle = 0.0, row_shift = 0.0, anchor = "local", shift = (0.0, 0.0)))]
fn svg_tile(
    region: &Region,
    svg: &str,
    width: f64,
    angle: f64,
    row_shift: f64,
    anchor: &str,
    shift: (f64, f64),
) -> PyResult<Strokes> {
    let drawing = pf::svg::parse_svg(svg).map_err(PyValueError::new_err)?;
    Ok(pf::svg::fill_svg_tile(&region.inner, &drawing, width, angle, row_shift, anchor_of(anchor, shift)?))
}

/// The line work of an SVG as strokes, scaled to `width` mm if given.
#[pyfunction]
#[pyo3(signature = (svg, width = None))]
fn svg_strokes(svg: &str, width: Option<f64>) -> PyResult<Strokes> {
    let drawing = pf::svg::parse_svg(svg).map_err(PyValueError::new_err)?;
    Ok(match width {
        Some(w) => drawing.fit_width(w).strokes,
        None => drawing.strokes,
    })
}

/// A path, the bytes of an image file, or rows of pixels (numpy arrays and
/// nested lists; grey or RGB(A); 0–1, or 0–255 if any value is above 1).
fn image_arg(obj: &Bound<'_, PyAny>) -> PyResult<pf::dither::Greyscale> {
    use pyo3::types::{PyByteArray, PyBytes, PyString};
    if let Ok(path) = obj.cast::<PyString>() {
        return pf::dither::Greyscale::open(path.extract::<String>()?).map_err(PyValueError::new_err);
    }
    if obj.is_instance_of::<PyBytes>() || obj.is_instance_of::<PyByteArray>() {
        let bytes: Vec<u8> = obj.extract()?;
        return pf::dither::Greyscale::decode(&bytes).map_err(PyValueError::new_err);
    }
    let (mut data, mut width, mut height) = (Vec::new(), 0usize, 0usize);
    let mut row_samples = None;
    for row in obj.try_iter()? {
        let row = row?;
        let before = data.len();
        match row.extract::<Vec<f64>>() {
            Ok(grey) => {
                width = grey.len();
                data.extend(grey);
            }
            Err(_) => {
                let pixels: Vec<Vec<f64>> = row.extract()?;
                width = pixels.len();
                pixels.into_iter().for_each(|p| data.extend(p));
            }
        }
        if *row_samples.get_or_insert(data.len() - before) != data.len() - before {
            return Err(PyValueError::new_err("image rows must all be the same length"));
        }
        height += 1;
    }
    let max = if data.iter().any(|&v| v > 1.0) { 255.0 } else { 1.0 };
    pf::dither::Greyscale::from_channels(width, height, &data, max).map_err(PyValueError::new_err)
}

/// A CSS value: a string as it is, a number or a pair of numbers as mm.
fn css(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = obj.extract::<String>() {
        return Ok(s);
    }
    if let Ok(v) = obj.extract::<f64>() {
        return Ok(v.to_string());
    }
    let pair: Vec<f64> = obj.extract()?;
    Ok(pair.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" "))
}

fn placement(
    size: Option<&Bound<'_, PyAny>>,
    position: Option<&Bound<'_, PyAny>>,
    repeat: &str,
    default_size: &str,
) -> PyResult<pf::placement::Placement> {
    let size = size.map(css).transpose()?.unwrap_or_else(|| default_size.into());
    let position = position.map(css).transpose()?.unwrap_or_else(|| "center".into());
    pf::placement::Placement::parse(&size, &position, repeat).map_err(PyValueError::new_err)
}

/// An SVG drawing placed like a CSS background and cut by the region.
/// `size`: "contain", "cover", "fill", a width in mm, or (width, height).
/// `position`: "center", "top left", "25% 75%", or (x, y) in mm.
/// `repeat`: "no-repeat", "repeat", "repeat-x", "repeat-y".
#[pyfunction]
#[pyo3(signature = (region, svg, size = None, position = None, repeat = "no-repeat"))]
fn svg_fill(
    region: &Region,
    svg: &str,
    size: Option<&Bound<'_, PyAny>>,
    position: Option<&Bound<'_, PyAny>>,
    repeat: &str,
) -> PyResult<Strokes> {
    let place = placement(size, position, repeat, "contain")?;
    let drawing = pf::svg::parse_svg(svg).map_err(PyValueError::new_err)?;
    Ok(pf::svg::fill_svg_placed(&region.inner, &drawing, &place))
}

/// A picture dithered inside the region: cells `resolution` mm long,
/// lines `line_spacing` mm apart (default = resolution). `method`:
/// threshold, random, bayer (bayer2–bayer16), halftone (halftone2–16),
/// floyd-steinberg, atkinson, jarvis, stucki, sierra. `marks`: "lines"
/// (dark runs joined into strokes) or "dots" (a tick per dark cell).
/// `size`, `position`, `repeat` place the picture like a CSS background
/// (default: cover, centred, no repeat).
#[pyfunction]
#[pyo3(signature = (region, image, resolution = 0.5, line_spacing = None, method = "floyd-steinberg", marks = "lines", angle = 0.0, gamma = 1.0, invert = false, mark = None, seed = 0, size = None, position = None, repeat = "no-repeat"))]
#[allow(clippy::too_many_arguments)]
fn dither(
    region: &Region,
    image: &Bound<'_, PyAny>,
    resolution: f64,
    line_spacing: Option<f64>,
    method: &str,
    marks: &str,
    angle: f64,
    gamma: f64,
    invert: bool,
    mark: Option<f64>,
    seed: u64,
    size: Option<&Bound<'_, PyAny>>,
    position: Option<&Bound<'_, PyAny>>,
    repeat: &str,
) -> PyResult<Strokes> {
    let img = image_arg(image)?;
    let base = pf::dither::DitherOptions::new(resolution);
    let opts = pf::dither::DitherOptions {
        line_spacing: line_spacing.unwrap_or(resolution),
        method: method.parse().map_err(PyValueError::new_err)?,
        marks: marks.parse().map_err(PyValueError::new_err)?,
        angle,
        gamma,
        invert,
        mark: mark.unwrap_or(base.mark),
        seed,
        placement: placement(size, position, repeat, "cover")?,
        ..base
    };
    Ok(pf::dither::fill_dither(&region.inner, &img, &opts))
}

/// Strokes as an SVG document in mm, with the region's outline in grey.
#[pyfunction]
#[pyo3(signature = (strokes, outline = None, stroke_width = 0.3))]
fn to_svg(strokes: &Bound<'_, PyAny>, outline: Option<&Region>, stroke_width: f64) -> PyResult<String> {
    Ok(pf::svg::to_svg(&stroke_list(strokes)?, outline.map(|r| &r.inner), stroke_width))
}

#[pymodule]
fn penfill(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("PATTERNS", pf::pat::BUILTIN.to_vec())?;
    m.add_class::<Region>()?;
    m.add_function(wrap_pyfunction!(rectilinear, m)?)?;
    m.add_function(wrap_pyfunction!(zigzag, m)?)?;
    m.add_function(wrap_pyfunction!(lines, m)?)?;
    m.add_function(wrap_pyfunction!(grid, m)?)?;
    m.add_function(wrap_pyfunction!(triangles, m)?)?;
    m.add_function(wrap_pyfunction!(concentric, m)?)?;
    m.add_function(wrap_pyfunction!(spiral, m)?)?;
    m.add_function(wrap_pyfunction!(hilbert, m)?)?;
    m.add_function(wrap_pyfunction!(honeycomb, m)?)?;
    m.add_function(wrap_pyfunction!(dots, m)?)?;
    m.add_function(wrap_pyfunction!(waves, m)?)?;
    m.add_function(wrap_pyfunction!(wave_grid, m)?)?;
    m.add_function(wrap_pyfunction!(scales, m)?)?;
    m.add_function(wrap_pyfunction!(sunburst, m)?)?;
    m.add_function(wrap_pyfunction!(stipple, m)?)?;
    m.add_function(wrap_pyfunction!(stipple_noise, m)?)?;
    m.add_function(wrap_pyfunction!(stipple_edge, m)?)?;
    m.add_function(wrap_pyfunction!(stipple_field, m)?)?;
    m.add_function(wrap_pyfunction!(gravel, m)?)?;
    m.add_function(wrap_pyfunction!(grass, m)?)?;
    m.add_function(wrap_pyfunction!(scribble, m)?)?;
    m.add_function(wrap_pyfunction!(truchet, m)?)?;
    m.add_function(wrap_pyfunction!(marble, m)?)?;
    m.add_function(wrap_pyfunction!(wood, m)?)?;
    m.add_function(wrap_pyfunction!(cracks, m)?)?;
    m.add_function(wrap_pyfunction!(terrazzo, m)?)?;
    m.add_function(wrap_pyfunction!(chess, m)?)?;
    m.add_function(wrap_pyfunction!(pat, m)?)?;
    m.add_function(wrap_pyfunction!(pat_names, m)?)?;
    m.add_function(wrap_pyfunction!(isolines, m)?)?;
    m.add_function(wrap_pyfunction!(levels_every, m)?)?;
    m.add_function(wrap_pyfunction!(profile, m)?)?;
    m.add_function(wrap_pyfunction!(tile, m)?)?;
    m.add_function(wrap_pyfunction!(svg_tile, m)?)?;
    m.add_function(wrap_pyfunction!(svg_strokes, m)?)?;
    m.add_function(wrap_pyfunction!(dither, m)?)?;
    m.add_function(wrap_pyfunction!(svg_fill, m)?)?;
    m.add_class::<Canvas>()?;
    m.add_function(wrap_pyfunction!(pattern, m)?)?;
    m.add_class::<Tools>()?;
    m.add_function(wrap_pyfunction!(to_svg, m)?)?;
    Ok(())
}
