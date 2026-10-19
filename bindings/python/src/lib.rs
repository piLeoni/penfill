//! Python bindings: `import penfill`.
//!
//! Points come in as any sequence of `(x, y)` pairs (tuples, lists, numpy
//! rows) and go out as lists of `(x, y)` tuples. Everything is in mm.

use penfill_core as pf;
use pf::geo::{Pt, Region as CoreRegion, Strokes};
use pf::pattern::{EdgeMode, FillParams};
use pyo3::exceptions::{PyIndexError, PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::cell::RefCell;

fn point(obj: &Bound<'_, PyAny>) -> PyResult<Pt> {
    let xy: Vec<f64> = obj.extract()?;
    if xy.len() < 2 {
        return Err(PyValueError::new_err("each point needs an x and a y"));
    }
    Ok((xy[0], xy[1]))
}

fn points(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Pt>> {
    let mut out = Vec::new();
    for item in obj.try_iter()? {
        out.push(point(&item?)?);
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
        Ok(Region {
            inner: CoreRegion::from_polygons(&[rings]),
        })
    }

    /// Several polygons, each written as `[outer, hole, hole, ...]`.
    #[staticmethod]
    fn from_polygons(polygons: &Bound<'_, PyAny>) -> PyResult<Self> {
        let polys: Vec<Vec<Vec<Pt>>> = polygons
            .try_iter()?
            .map(|p| stroke_list(&p?))
            .collect::<PyResult<_>>()?;
        Ok(Region {
            inner: CoreRegion::from_polygons(&polys),
        })
    }

    /// Build a polygon region around `strokes` inflated by `radius` mm.
    #[staticmethod]
    fn around_strokes(strokes: &Bound<'_, PyAny>, radius: f64) -> PyResult<Self> {
        Ok(Region {
            inner: CoreRegion::around_strokes(&stroke_list(strokes)?, radius),
        })
    }

    /// Shrunk by `mm` (mitred corners).
    fn inset(&self, mm: f64) -> Self {
        Region {
            inner: self.inner.inset(mm),
        }
    }

    /// Grown by `mm` (round corners).
    fn outset(&self, mm: f64) -> Self {
        Region {
            inner: self.inner.outset(mm),
        }
    }

    fn union(&self, other: &Region) -> Self {
        Region {
            inner: self.inner.union(&other.inner),
        }
    }

    fn intersection(&self, other: &Region) -> Self {
        Region {
            inner: self.inner.intersection(&other.inner),
        }
    }

    fn difference(&self, other: &Region) -> Self {
        Region {
            inner: self.inner.difference(&other.inner),
        }
    }

    /// Cut any polylines to the region.
    fn clip(&self, strokes: &Bound<'_, PyAny>) -> PyResult<Strokes> {
        Ok(self.inner.clip_strokes(&stroke_list(strokes)?))
    }

    /// Clip and apply perimeter edge fraying (`fray` in mm) to any polylines.
    #[pyo3(signature = (strokes, spacing, fray, seed = 0))]
    fn fray(
        &self,
        strokes: &Bound<'_, PyAny>,
        spacing: f64,
        fray: f64,
        seed: u64,
    ) -> PyResult<Strokes> {
        Ok(pf::fray::fray_strokes(
            &self.inner,
            stroke_list(strokes)?,
            spacing,
            fray,
            seed,
        ))
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
        self.inner
            .parts()
            .into_iter()
            .map(|inner| Region { inner })
            .collect()
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
        format!(
            "Region({} rings, bounds=({a:.2}, {b:.2}, {c:.2}, {d:.2}))",
            self.inner.all_rings_mm().len()
        )
    }
}

/// Output of a fill with its polygon `mask`: `out.strokes` and `out.mask`
/// (also unpacks as `strokes, mask = out`).
#[pyclass(module = "penfill", frozen)]
struct FillOutput {
    #[pyo3(get)]
    strokes: Strokes,
    #[pyo3(get)]
    mask: Py<Region>,
}

impl FillOutput {
    fn from_core(py: Python<'_>, out: pf::FillOutput) -> PyResult<Self> {
        Ok(FillOutput {
            strokes: out.strokes,
            mask: Py::new(py, Region { inner: out.mask })?,
        })
    }
}

#[pymethods]
impl FillOutput {
    fn __len__(&self) -> usize {
        2
    }

    fn __getitem__<'py>(&self, py: Python<'py>, idx: isize) -> PyResult<Bound<'py, PyAny>> {
        match idx {
            0 | -2 => Ok(self.strokes.clone().into_pyobject(py)?.into_any()),
            1 | -1 => Ok(self.mask.clone_ref(py).into_bound(py).into_any()),
            _ => Err(PyIndexError::new_err("FillOutput index out of range (0=strokes, 1=mask)")),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let rings = self.mask.borrow(py).inner.all_rings_mm().len();
        format!(
            "FillOutput(strokes={}, mask=Region({rings} rings))",
            self.strokes.len()
        )
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
        other => {
            return Err(PyValueError::new_err(format!(
                "anchor must be 'local' or 'global', not {other:?}"
            )))
        }
    };
    Ok(pf::Anchor { origin, shift })
}

/// Parse `edge`: `"clip"`, `"inside"`, `"overflow"`, `("overflow", mm)`,
/// `{"overflow": mm}`, or a positive float `mm`.
fn edge_of(edge: Option<&Bound<'_, PyAny>>) -> PyResult<EdgeMode> {
    let Some(obj) = edge else {
        return Ok(EdgeMode::Clip);
    };
    if obj.is_none() {
        return Ok(EdgeMode::Clip);
    }
    if let Ok(s) = obj.extract::<String>() {
        return match s.as_str() {
            "clip" => Ok(EdgeMode::Clip),
            "inside" | "adaptive" => Ok(EdgeMode::Inside),
            "overflow" => Ok(EdgeMode::Overflow(2.0)),
            other => Err(PyValueError::new_err(format!(
                "edge must be 'clip', 'inside', or ('overflow', mm), not {other:?}"
            ))),
        };
    }
    if let Ok(mm) = obj.extract::<f64>() {
        return Ok(EdgeMode::Overflow(mm.max(0.0)));
    }
    if let Ok((tag, mm)) = obj.extract::<(String, f64)>() {
        if tag == "overflow" {
            return Ok(EdgeMode::Overflow(mm.max(0.0)));
        }
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        if let Some(val) = dict.get_item("overflow")? {
            let mm: f64 = val.extract()?;
            return Ok(EdgeMode::Overflow(mm.max(0.0)));
        }
    }
    Err(PyValueError::new_err(
        "edge must be 'clip', 'inside', ('overflow', mm), or {'overflow': mm}",
    ))
}

#[allow(clippy::too_many_arguments)]
fn make_params(
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'_, PyAny>>,
    fray: f64,
) -> PyResult<FillParams> {
    Ok(FillParams::new(spacing)
        .angle(angle)
        .seed(seed)
        .anchor(anchor_of(anchor, shift)?)
        .inset(inset)
        .edge(edge_of(edge)?)
        .fray(fray.max(0.0)))
}

fn apply_custom(params: &mut FillParams, custom: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    if let Some(dict) = custom {
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            let val: f64 = v.extract()?;
            params.custom.set(&key, val);
        }
    }
    Ok(())
}

fn finish_output<'py>(
    py: Python<'py>,
    out: pf::FillOutput,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    if with_mask {
        Ok(Py::new(py, FillOutput::from_core(py, out)?)?.into_bound(py).into_any())
    } else {
        Ok(out.strokes.into_pyobject(py)?.into_any())
    }
}

// ---- universal registry entry points ----

/// Run any built-in pattern by name with unified parameters (`spacing`, `angle`,
/// `seed`, `inset`, `edge`, `fray`, `anchor`, `shift`, `custom`, `with_mask`).
#[pyfunction]
#[pyo3(signature = (region, name, spacing = 2.0, *, angle = 0.0, seed = 0, inset = None, edge = None, fray = 0.0, anchor = "local", shift = (0.0, 0.0), custom = None, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn fill<'py>(
    py: Python<'py>,
    region: &Region,
    name: &str,
    spacing: f64,
    angle: f64,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    anchor: &str,
    shift: (f64, f64),
    custom: Option<&Bound<'py, PyDict>>,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let mut params = make_params(spacing, angle, seed, anchor, shift, inset, edge, fray)?;
    apply_custom(&mut params, custom)?;
    if with_mask {
        let out = pf::fill_output_by_name(name, &region.inner, &params)
            .ok_or_else(|| PyKeyError::new_err(format!("no fill named {name:?}")))?;
        finish_output(py, out, true)
    } else {
        let strokes = pf::fill_by_name(name, &region.inner, &params)
            .ok_or_else(|| PyKeyError::new_err(format!("no fill named {name:?}")))?;
        Ok(strokes.into_pyobject(py)?.into_any())
    }
}

/// Run any built-in pattern by name and always return a `FillOutput(strokes, mask)`.
#[pyfunction]
#[pyo3(signature = (region, name, spacing = 2.0, *, angle = 0.0, seed = 0, inset = None, edge = None, fray = 0.0, anchor = "local", shift = (0.0, 0.0), custom = None))]
#[allow(clippy::too_many_arguments)]
fn fill_output<'py>(
    py: Python<'py>,
    region: &Region,
    name: &str,
    spacing: f64,
    angle: f64,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    anchor: &str,
    shift: (f64, f64),
    custom: Option<&Bound<'py, PyDict>>,
) -> PyResult<FillOutput> {
    let mut params = make_params(spacing, angle, seed, anchor, shift, inset, edge, fray)?;
    apply_custom(&mut params, custom)?;
    let out = pf::fill_output_by_name(name, &region.inner, &params)
        .ok_or_else(|| PyKeyError::new_err(format!("no fill named {name:?}")))?;
    FillOutput::from_core(py, out)
}

// ---- lines and paths ----

macro_rules! registry_pyfill {
    ($(#[$doc:meta])* $fn_name:ident, $name_lit:literal) => {
        $(#[$doc])*
        #[pyfunction]
        #[pyo3(signature = (region, spacing, angle = 0.0, inset = None, anchor = "local", shift = (0.0, 0.0), *, seed = 0, edge = None, fray = 0.0, with_mask = false))]
        #[allow(clippy::too_many_arguments)]
        fn $fn_name<'py>(
            py: Python<'py>,
            region: &Region,
            spacing: f64,
            angle: f64,
            inset: Option<f64>,
            anchor: &str,
            shift: (f64, f64),
            seed: u64,
            edge: Option<&Bound<'py, PyAny>>,
            fray: f64,
            with_mask: bool,
        ) -> PyResult<Bound<'py, PyAny>> {
            fill(py, region, $name_lit, spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
        }
    };
}

registry_pyfill!(
    /// Parallel hatch, one stroke per fragment.
    rectilinear,
    "rectilinear"
);
registry_pyfill!(
    /// Hatch joined into serpentines; the pen lifts only where a join would leave the shape.
    zigzag,
    "zigzag"
);

/// One-direction hatch over the whole region.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn lines<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "lines", spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
}

/// Two hatches at 90°.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn grid<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "grid", spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
}

/// Three hatches at 60° sharing every crossing.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn triangles<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "triangles", spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
}

/// Closed rings following the outline inwards.
#[pyfunction]
#[pyo3(signature = (region, spacing, inset = None, *, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn concentric<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = FillParams::new(spacing)
        .seed(seed)
        .edge(edge_of(edge)?)
        .fray(fray.max(0.0));
    let out = pf::fill_pattern_output(&region.inner, &params, |c, _| {
        pf::concentric::fill_concentric(&c.limit, spacing, inset)
    });
    finish_output(py, out, with_mask)
}

/// One continuous path winding inwards.
#[pyfunction]
#[pyo3(signature = (region, spacing, *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn spiral<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "spiral", spacing, 0.0, seed, inset, edge, fray, "local", (0.0, 0.0), None, with_mask)
}

/// Space-filling Hilbert curve, one continuous path clipped to the region.
#[pyfunction]
#[pyo3(signature = (region, spacing, inset = None, *, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn hilbert<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = FillParams::new(spacing)
        .seed(seed)
        .edge(edge_of(edge)?)
        .fray(fray.max(0.0));
    let out = pf::fill_pattern_output(&region.inner, &params, |c, _| {
        pf::hilbert::fill_hilbert(&c.limit, spacing, inset)
    });
    finish_output(py, out, with_mask)
}

// ---- lattices ----

/// Hexagon walls, each drawn once; `spacing` is flat to flat.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, line_width = 0.0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn honeycomb<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    line_width: f64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(spacing, angle, seed, anchor, shift, inset, edge, fray)?;
    let out = pf::fill_pattern_output(&region.inner, &params, |c, _| {
        pf::honeycomb::fill_honeycomb(&c.limit, spacing, angle, line_width, c.anchor)
    });
    finish_output(py, out, with_mask)
}

/// Short marks on a lattice; `lattice` is "square" or "hex".
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, mark = None, lattice = "square", anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn dots<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    mark: Option<f64>,
    lattice: &str,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let lattice = match lattice {
        "square" => pf::dotswaves::Lattice::Square,
        "hex" | "hexagonal" => pf::dotswaves::Lattice::Hexagonal,
        other => {
            return Err(PyValueError::new_err(format!(
                "lattice must be 'square' or 'hex', not {other:?}"
            )))
        }
    };
    let params = make_params(spacing, angle, seed, anchor, shift, inset, edge, fray)?;
    let out = pf::dotswaves::fill_dots_params(&region.inner, &params, mark, lattice);
    finish_output(py, out, with_mask)
}

/// Parallel sine waves.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn waves<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(spacing, angle, seed, anchor, shift, inset, edge, fray)?;
    let out = pf::dotswaves::fill_waves_params(&region.inner, &params);
    finish_output(py, out, with_mask)
}

/// Ogee: mirrored waves closing into diamonds.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn wave_grid<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(spacing, angle, seed, anchor, shift, inset, edge, fray)?;
    let out = pf::dotswaves::fill_wave_grid_params(&region.inner, &params);
    finish_output(py, out, with_mask)
}

/// Fish scales; each row of arcs is one stroke.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn scales<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "scales", spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
}

/// Rays from the anchor (the middle of the bounds by default); `centre`
/// is a shorthand for `anchor="global", shift=centre`.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, centre = None, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn sunburst<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    centre: Option<(f64, f64)>,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let (anchor_name, shift_val) = match centre {
        Some((x, y)) => ("global", (x, y)),
        None => (anchor, shift),
    };
    fill(
        py,
        region,
        "sunburst",
        spacing,
        angle,
        seed,
        inset,
        edge,
        fray,
        anchor_name,
        shift_val,
        None,
        with_mask,
    )
}

// ---- scatter and nature ----

/// Evenly spread short marks.
#[pyfunction]
#[pyo3(signature = (region, spacing, mark = None, seed = 0, *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn stipple<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    mark: Option<f64>,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(spacing, 0.0, seed, "local", (0.0, 0.0), inset, edge, fray)?;
    let out = pf::fill_pattern_output(&region.inner, &params, |c, t| {
        t.set_mask_halo((spacing * 0.45).max(0.25));
        pf::stipple::fill_stipple(&c.limit, spacing, mark.unwrap_or(spacing * 0.3), seed)
    });
    finish_output(py, out, with_mask)
}

/// Stipple whose density drifts with Perlin noise.
#[pyfunction]
#[pyo3(signature = (region, dense, sparse, mark = None, feature = 12.0, seed = 0, *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn stipple_noise<'py>(
    py: Python<'py>,
    region: &Region,
    dense: f64,
    sparse: f64,
    mark: Option<f64>,
    feature: f64,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(dense, 0.0, seed, "local", (0.0, 0.0), inset, edge, fray)?;
    let out = pf::fill_pattern_output(&region.inner, &params, |c, t| {
        t.set_mask_halo((dense * 0.55).max(0.25));
        pf::stipple::fill_stipple_noise(&c.limit, dense, sparse, mark.unwrap_or(dense * 0.4), feature, seed)
    });
    finish_output(py, out, with_mask)
}

/// Stipple dense along the wall, fading inwards over `falloff` mm.
#[pyfunction]
#[pyo3(signature = (region, dense, sparse, mark = None, falloff = 8.0, rough = 0.5, seed = 0, *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn stipple_edge<'py>(
    py: Python<'py>,
    region: &Region,
    dense: f64,
    sparse: f64,
    mark: Option<f64>,
    falloff: f64,
    rough: f64,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(dense, 0.0, seed, "local", (0.0, 0.0), inset, edge, fray)?;
    let out = pf::fill_pattern_output(&region.inner, &params, |c, t| {
        t.set_mask_halo((dense * 0.55).max(0.25));
        pf::stipple::fill_stipple_edge(&c.limit, dense, sparse, mark.unwrap_or(dense * 0.4), falloff, rough, seed)
    });
    finish_output(py, out, with_mask)
}

/// Stipple with density from `field(x, y)`: 0 is `dense`, 1 is `sparse`.
#[pyfunction]
#[pyo3(signature = (region, dense, sparse, field, mark = None, seed = 0, *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn stipple_field<'py>(
    py: Python<'py>,
    region: &Region,
    dense: f64,
    sparse: f64,
    field: &Bound<'py, PyAny>,
    mark: Option<f64>,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(dense, 0.0, seed, "local", (0.0, 0.0), inset, edge, fray)?;
    let out = with_callback(field, |f| {
        pf::fill_pattern_output(&region.inner, &params, |c, t| {
            t.set_mask_halo((dense * 0.55).max(0.25));
            pf::stipple::fill_stipple_field(
                &c.limit,
                dense,
                sparse,
                mark.unwrap_or(dense * 0.4),
                seed,
                |p| f(&[p.0, p.1]),
            )
        })
    })?;
    finish_output(py, out, with_mask)
}

/// Rounded Voronoi stones; `edge="inside"` (default) shrinks stones into
/// corners, `edge="clip"` (or `clipped=True`) cuts uniform stones at the wall,
/// and `edge=("overflow", mm)` lets whole stones protrude past the wall.
#[pyfunction]
#[pyo3(signature = (region, spacing, seed = 0, clipped = false, *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn gravel<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    seed: u64,
    clipped: bool,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let edge_mode = if let Some(e) = edge {
        edge_of(Some(e))?
    } else if clipped {
        EdgeMode::Clip
    } else {
        EdgeMode::Inside
    };
    let params = FillParams::new(spacing)
        .seed(seed)
        .inset(inset)
        .edge(edge_mode)
        .fray(fray.max(0.0));
    let out = pf::gravel::fill_gravel_params(&region.inner, &params);
    finish_output(py, out, with_mask)
}

/// Grass tufts as a repeating hatch tile.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn grass<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "grass", spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
}

/// Hand-drawn cross-hatching.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0, *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn scribble<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "scribble", spacing, angle, seed, inset, edge, fray, "local", (0.0, 0.0), None, with_mask)
}

/// Random quarter-circle tiles joined into winding paths.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn truchet<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "truchet", spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
}

// ---- materials & textures ----

/// Marble veins.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn marble<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "marble", spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
}

/// Wood growth rings bending around knots.
#[pyfunction]
#[pyo3(signature = (region, spacing, angle = 0.0, seed = 0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn wood<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    fill(py, region, "wood", spacing, angle, seed, inset, edge, fray, anchor, shift, None, with_mask)
}

/// Parched ground / mud cracks (rounded Voronoi cells).
#[pyfunction]
#[pyo3(signature = (region, spacing, seed = 0, *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn cracks<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(spacing, 0.0, seed, "local", (0.0, 0.0), inset, edge, fray)?;
    let out = pf::textures::fill_cracks_params(&region.inner, &params);
    finish_output(py, out, with_mask)
}

/// Terrazzo chips; about a third are dark, hatched `hatch` mm apart.
#[pyfunction]
#[pyo3(signature = (region, spacing, hatch = None, seed = 0, *, inset = None, edge = None, fray = 0.0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn terrazzo<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    hatch: Option<f64>,
    seed: u64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(spacing, 0.0, seed, "local", (0.0, 0.0), inset, edge, fray)?;
    let out = pf::terrazzo::fill_terrazzo_params(&region.inner, &params, hatch);
    finish_output(py, out, with_mask)
}

/// Checkerboard, dark squares hatched `hatch` mm apart.
#[pyfunction]
#[pyo3(signature = (region, spacing, hatch = None, angle = 0.0, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn chess<'py>(
    py: Python<'py>,
    region: &Region,
    spacing: f64,
    hatch: Option<f64>,
    angle: f64,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let params = make_params(spacing, angle, seed, anchor, shift, inset, edge, fray)?;
    let out = pf::fill_pattern_output(&region.inner, &params, |c, _| {
        pf::chess::fill_chess(&c.limit, spacing, hatch.unwrap_or(spacing / 8.0), angle, c.anchor)
    });
    finish_output(py, out, with_mask)
}

// ---- nature & organic fills ----

macro_rules! nature_pyfill {
    ($(#[$doc:meta])* $fn_name:ident, $name_lit:literal) => {
        $(#[$doc])*
        #[pyfunction]
        #[pyo3(signature = (region, spacing, *, angle = 0.0, seed = 0, inset = None, edge = None, fray = 0.0, anchor = "local", shift = (0.0, 0.0), custom = None, with_mask = false))]
        #[allow(clippy::too_many_arguments)]
        fn $fn_name<'py>(
            py: Python<'py>,
            region: &Region,
            spacing: f64,
            angle: f64,
            seed: u64,
            inset: Option<f64>,
            edge: Option<&Bound<'py, PyAny>>,
            fray: f64,
            anchor: &str,
            shift: (f64, f64),
            custom: Option<&Bound<'py, PyDict>>,
            with_mask: bool,
        ) -> PyResult<Bound<'py, PyAny>> {
            fill(py, region, $name_lit, spacing, angle, seed, inset, edge, fray, anchor, shift, custom, with_mask)
        }
    };
}

nature_pyfill!(
    /// Small rounded stones on a Poisson grid.
    pebbles,
    "pebbles"
);
nature_pyfill!(
    /// Short blades and V-shaped sprouts.
    leaves,
    "leaves"
);
nature_pyfill!(
    /// Sparse sprout hatch: offset rows of single strokes and V marks.
    sprouts,
    "sprouts"
);
nature_pyfill!(
    /// Grass as short strokes aligned to a smooth vector field.
    fluid_grass,
    "fluid_grass"
);
nature_pyfill!(
    /// Lily pads: merged overlapping circle outlines with centre dots.
    lily_pads,
    "lily_pads"
);
nature_pyfill!(
    /// Top-down tree canopies: clustered wobbly lobes with centre dots.
    canopy,
    "canopy"
);
nature_pyfill!(
    /// Clustered columns of short curved grass/sedge blades and V-tufts.
    tufts,
    "tufts"
);
nature_pyfill!(
    /// Scattered branched twigs.
    twigs,
    "twigs"
);
nature_pyfill!(
    /// Parallel meandering stems with alternating side-shoots.
    vines,
    "vines"
);
nature_pyfill!(
    /// Single-line Voronoi polygonal crack network.
    cell_net,
    "cell_net"
);
nature_pyfill!(
    /// Flowing wave streamlines integrated around spiral eddies.
    eddies,
    "eddies"
);
nature_pyfill!(
    /// Calm water swells with lens-shaped interference bulges.
    swells,
    "swells"
);
nature_pyfill!(
    /// Wind-blown water ripples with organic noise breaks.
    ripples,
    "ripples"
);
nature_pyfill!(
    /// Space-colonization branching networks grown from roots.
    growth,
    "growth"
);
nature_pyfill!(
    /// Nested contour rings around metaballs or noise mountains.
    isoline_blobs,
    "isoline_blobs"
);
nature_pyfill!(
    /// Stacked 3D metaball crowns with hidden-line occlusion.
    isoline_stack,
    "isoline_stack"
);

// ---- CAD patterns ----

/// Hatch with an AutoCAD `.pat` pattern: a built-in `name` (see
/// `PATTERNS`), or a pattern of that name in `source`, the text of a
/// `.pat` file. `scale` is mm per pattern unit.
#[pyfunction]
#[pyo3(signature = (region, name, scale = 1.0, angle = 0.0, source = None, anchor = "local", shift = (0.0, 0.0), *, inset = None, edge = None, fray = 0.0, seed = 0, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn pat<'py>(
    py: Python<'py>,
    region: &Region,
    name: &str,
    scale: f64,
    angle: f64,
    source: Option<&str>,
    anchor: &str,
    shift: (f64, f64),
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    seed: u64,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let pattern = match source {
        Some(text) => pf::pat::parse_pat(text)
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name)),
        None => pf::pat::builtin(name),
    };
    let pattern = pattern.ok_or_else(|| PyKeyError::new_err(format!("no pattern named {name:?}")))?;
    let params = make_params(scale, angle, seed, anchor, shift, inset, edge, fray)?;
    let out = pf::fill_pattern_output(&region.inner, &params, |c, _| {
        pf::pat::fill_pat(&c.limit, &pattern, scale, angle, c.anchor)
    });
    finish_output(py, out, with_mask)
}

/// Names of the patterns in a `.pat` file's text.
#[pyfunction]
fn pat_names(source: &str) -> Vec<String> {
    pf::pat::parse_pat(source).into_iter().map(|p| p.name).collect()
}

// ---- bring your own ----

/// What `pattern` hands your function: the region to fill, its limit envelope,
/// bounds, spacing, angle, seed, anchor origin, and fray band.
#[pyclass(module = "penfill", frozen)]
struct Canvas {
    #[pyo3(get)]
    region: Py<Region>,
    #[pyo3(get)]
    limit: Py<Region>,
    #[pyo3(get)]
    bounds: (f64, f64, f64, f64),
    #[pyo3(get)]
    spacing: f64,
    #[pyo3(get)]
    angle: f64,
    #[pyo3(get)]
    seed: u64,
    /// The anchor resolved for this region: the point to build a lattice through.
    #[pyo3(get)]
    origin: Pt,
    #[pyo3(get)]
    fray: f64,
    custom: pf::CustomParams,
}

#[pymethods]
impl Canvas {
    /// Read a pattern-specific custom parameter, falling back to `default`.
    #[pyo3(signature = (key, default = 0.0))]
    fn param(&self, key: &str, default: f64) -> f64 {
        self.custom.get_or(key, default)
    }

    fn __repr__(&self) -> String {
        let (a, b, c, d) = self.bounds;
        format!(
            "Canvas(bounds=({a:.2}, {b:.2}, {c:.2}, {d:.2}), spacing={}, angle={}, seed={}, fray={})",
            self.spacing, self.angle, self.seed, self.fray
        )
    }
}

/// The toolkit `pattern` hands your function next to the canvas: what the
/// built-in fills are made of, set to the canvas region and seed.
#[pyclass(module = "penfill", frozen)]
struct Tools {
    region: CoreRegion,
    limit: CoreRegion,
    params: FillParams,
}

impl Tools {
    fn core(&self) -> pf::pattern::Tools<'_> {
        pf::pattern::Tools::from_params(&self.region, self.limit.clone(), &self.params)
    }
}

#[pymethods]
impl Tools {
    /// The same tools for another region, e.g. a piece from `region.parts()`.
    #[pyo3(signature = (region, seed = None))]
    fn on(&self, region: &Region, seed: Option<u64>) -> Tools {
        let mut params = self.params;
        if let Some(s) = seed {
            params.seed = s;
        }
        let limit = params.resolve_limit(&region.inner);
        Tools {
            region: region.inner.clone(),
            limit,
            params,
        }
    }

    /// Smooth Perlin noise, roughly -1 to 1, one bump per unit; each extra
    /// octave adds detail twice as fine and half as strong.
    #[pyo3(signature = (x, y, octaves = 1))]
    fn noise(&self, x: f64, y: f64, octaves: usize) -> f64 {
        self.core().noise(x, y, octaves.max(1))
    }

    /// Smooth border-fade factor in `[0, 1]` at `point`: `0.0` at the boundary
    /// and `1.0` once deeper than `fray` inside.
    #[pyo3(signature = (point, fray = None))]
    fn whole_at(&self, point: &Bound<'_, PyAny>, fray: Option<f64>) -> PyResult<f64> {
        let p = crate::point(point)?;
        let core = self.core();
        Ok(match fray {
            Some(f) => core.whole_at_band(p, self.params.spacing, f),
            None => core.whole_at(p),
        })
    }

    /// Distance ramp from the wall (`0` at wall, `1` past `falloff` mm),
    /// roughened by `rough` (`0..=1`).
    #[pyo3(signature = (point, falloff = 8.0, rough = 0.5))]
    fn border_ramp(&self, point: &Bound<'_, PyAny>, falloff: f64, rough: f64) -> PyResult<f64> {
        Ok(self.core().border_ramp(crate::point(point)?, falloff, rough))
    }

    /// Whether a discrete element of `radius` at `centre` fits according to
    /// the active `edge` mode.
    fn element_fits(&self, centre: &Bound<'_, PyAny>, radius: f64) -> PyResult<bool> {
        Ok(self.core().element_fits(crate::point(centre)?, radius))
    }

    /// Random points in the region, no two closer than `spacing` mm.
    fn scatter(&self, spacing: f64) -> Vec<Pt> {
        self.core().scatter(spacing)
    }

    /// Random points `dense` mm apart where `field(x, y)` is 0 and `sparse`
    /// mm apart where it is 1.
    fn scatter_field(&self, dense: f64, sparse: f64, field: &Bound<'_, PyAny>) -> PyResult<Vec<Pt>> {
        with_callback(field, |f| {
            self.core().scatter_field(dense, sparse, |p| f(&[p.0, p.1]))
        })
    }

    /// Parallel lines `spacing` mm apart, cut to the region right up to its walls.
    #[pyo3(signature = (spacing, angle = 0.0))]
    fn hatch(&self, spacing: f64, angle: f64) -> Strokes {
        self.core().hatch(spacing, angle)
    }

    /// Bounded Voronoi cell rings `[(index, centre, ring), ...]` for `centres`.
    #[pyo3(signature = (centres, reach = None))]
    fn voronoi_cells(&self, centres: &Bound<'_, PyAny>, reach: Option<f64>) -> PyResult<Vec<Vec<Pt>>> {
        let pts = points(centres)?;
        let r = reach.unwrap_or(self.params.spacing * 2.5);
        Ok(self
            .core()
            .voronoi_cells(&pts, r)
            .into_iter()
            .map(|(_, _, ring)| ring)
            .collect())
    }

    /// Deduplicated shared Voronoi edges `[(a, b), ...]` between `centres`.
    #[pyo3(signature = (centres, reach = None))]
    fn voronoi_edges(&self, centres: &Bound<'_, PyAny>, reach: Option<f64>) -> PyResult<Vec<(Pt, Pt)>> {
        let pts = points(centres)?;
        let r = reach.unwrap_or(self.params.spacing * 2.5);
        Ok(self
            .core()
            .voronoi_edges(&pts, r)
            .into_iter()
            .map(|(_, _, a, b)| (a, b))
            .collect())
    }

    /// Closed circle polyline of radius `r` around `centre`.
    fn circle(&self, centre: &Bound<'_, PyAny>, r: f64) -> PyResult<Vec<Pt>> {
        Ok(self.core().circle(crate::point(centre)?, r))
    }

    /// Irregular foliage/canopy lobe outline around `centre`.
    #[pyo3(signature = (centre, r, phase = 0.0, rough = 0.14))]
    fn lobe(&self, centre: &Bound<'_, PyAny>, r: f64, phase: f64, rough: f64) -> PyResult<Vec<Pt>> {
        Ok(self.core().lobe(crate::point(centre)?, r, phase, rough))
    }

    /// Circular arc around `centre` of radius `r` from `a0` to `a1` radians.
    #[pyo3(signature = (centre, r, a0, a1, steps = 16))]
    fn arc(
        &self,
        centre: &Bound<'_, PyAny>,
        r: f64,
        a0: f64,
        a1: f64,
        steps: usize,
    ) -> PyResult<Vec<Pt>> {
        Ok(self.core().arc(crate::point(centre)?, r, a0, a1, steps))
    }

    /// Straight tick centred at `centre` at `angle_rad` radians with half-length `half`.
    fn tick(&self, centre: &Bound<'_, PyAny>, angle_rad: f64, half: f64) -> PyResult<Vec<Pt>> {
        Ok(self.core().tick(crate::point(centre)?, angle_rad, half))
    }

    /// Orthogonal `+` cross centred at `centre`.
    #[pyo3(signature = (centre, half, twist_rad = 0.0))]
    fn cross(&self, centre: &Bound<'_, PyAny>, half: f64, twist_rad: f64) -> PyResult<Strokes> {
        Ok(self.core().cross(crate::point(centre)?, half, twist_rad))
    }

    /// Diagonal `×` cross centred at `centre`.
    #[pyo3(signature = (centre, half, twist_rad = 0.0))]
    fn x_cross(&self, centre: &Bound<'_, PyAny>, half: f64, twist_rad: f64) -> PyResult<Strokes> {
        Ok(self.core().x_cross(crate::point(centre)?, half, twist_rad))
    }

    /// Chaikin corner-cutting smoothing over `rounds` passes.
    #[pyo3(signature = (pts, rounds = 2))]
    fn smooth(&self, pts: &Bound<'_, PyAny>, rounds: usize) -> PyResult<Vec<Pt>> {
        Ok(self.core().smooth(&points(pts)?, rounds))
    }

    /// Resample a polyline so consecutive vertices are at most `step` mm apart.
    fn resample(&self, pts: &Bound<'_, PyAny>, step: f64) -> PyResult<Vec<Pt>> {
        Ok(self.core().resample(&points(pts)?, step))
    }

    /// Record a closed ring (in mm) as part of this pattern's semantic mask.
    fn add_mask_ring(&self, ring: &Bound<'_, PyAny>) -> PyResult<()> {
        self.core().add_mask_ring(&points(ring)?);
        Ok(())
    }

    /// Record a `Region` as part of this pattern's semantic mask.
    fn add_mask(&self, region: &Region) {
        self.core().add_mask(&region.inner);
    }

    fn __repr__(&self) -> String {
        format!("Tools(seed={}, spacing={})", self.params.seed, self.params.spacing)
    }
}

/// Whether `draw` takes a second argument for the tools.
fn wants_tools(py: Python<'_>, draw: &Bound<'_, PyAny>) -> bool {
    let count = || -> PyResult<bool> {
        let params = py
            .import("inspect")?
            .call_method1("signature", (draw,))?
            .getattr("parameters")?;
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
/// page and they come back clipped (and optionally frayed) to the region.
/// `tools` is optional: `draw(canvas)` works too.
#[pyfunction]
#[pyo3(signature = (region, draw, seed = 0, anchor = "local", shift = (0.0, 0.0), *, spacing = 2.0, angle = 0.0, inset = None, edge = None, fray = 0.0, custom = None, with_mask = false))]
#[allow(clippy::too_many_arguments)]
fn pattern<'py>(
    py: Python<'py>,
    region: Py<Region>,
    draw: &Bound<'py, PyAny>,
    seed: u64,
    anchor: &str,
    shift: (f64, f64),
    spacing: f64,
    angle: f64,
    inset: Option<f64>,
    edge: Option<&Bound<'py, PyAny>>,
    fray: f64,
    custom: Option<&Bound<'py, PyDict>>,
    with_mask: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let mut params = make_params(spacing, angle, seed, anchor, shift, inset, edge, fray)?;
    apply_custom(&mut params, custom)?;
    let inner = region.get().inner.clone();
    let with_tools = wants_tools(py, draw);
    let error: RefCell<Option<PyErr>> = RefCell::new(None);
    let run_cb = |c: &pf::Canvas, _t: &pf::Tools| -> Strokes {
        let step = || -> PyResult<Strokes> {
            let reg_py = Py::new(py, Region { inner: c.region.clone() })?;
            let lim_py = Py::new(py, Region { inner: c.limit.clone() })?;
            let canvas = Canvas {
                region: reg_py,
                limit: lim_py,
                bounds: c.bounds,
                spacing: c.spacing,
                angle: c.angle,
                seed: c.seed,
                origin: c.origin,
                fray: c.fray,
                custom: c.custom,
            };
            let result = if with_tools {
                let tools = Tools {
                    region: c.region.clone(),
                    limit: c.limit.clone(),
                    params,
                };
                draw.call1((canvas, tools))
            } else {
                draw.call1((canvas,))
            };
            stroke_list(&result?)
        };
        match step() {
            Ok(strokes) => strokes,
            Err(e) => {
                *error.borrow_mut() = Some(e);
                vec![]
            }
        }
    };
    let out = if with_mask {
        pf::fill_pattern_output(&inner, &params, run_cb)
    } else {
        pf::FillOutput {
            strokes: pf::fill_pattern_with(&inner, &params, run_cb),
            mask: CoreRegion::default(),
        }
    };
    match error.into_inner() {
        Some(e) => Err(e),
        None => finish_output(py, out, with_mask),
    }
}

/// Contour lines of `f(x, y)` at `levels`, sampled every `cell` mm.
#[pyfunction]
#[pyo3(signature = (region, f, levels, cell = 0.2))]
fn isolines(region: &Region, f: &Bound<'_, PyAny>, levels: Vec<f64>, cell: f64) -> PyResult<Strokes> {
    with_callback(f, |f| {
        pf::field::fill_isolines(&region.inner, cell, &levels, |p| f(&[p.0, p.1]))
    })
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
    with_callback(f, |f| {
        pf::field::fill_profile(&region.inner, spacing, angle, anchor, |x| f(&[x]))
    })
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
    Ok(pf::tile::fill_tile(
        &region.inner,
        &stroke_list(strokes)?,
        width,
        height,
        angle,
        row_shift,
        anchor_of(anchor, shift)?,
    ))
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
    Ok(pf::svg::fill_svg_tile(
        &region.inner,
        &drawing,
        width,
        angle,
        row_shift,
        anchor_of(anchor, shift)?,
    ))
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
    Ok(pf::svg::to_svg(
        &stroke_list(strokes)?,
        outline.map(|r| &r.inner),
        stroke_width,
    ))
}

#[pymodule]
fn penfill(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("PATTERNS", pf::pat::BUILTIN.to_vec())?;
    m.add("FILLS", pf::all_fill_names())?;
    m.add_class::<Region>()?;
    m.add_class::<FillOutput>()?;
    m.add_function(wrap_pyfunction!(fill, m)?)?;
    m.add_function(wrap_pyfunction!(fill_output, m)?)?;
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
    m.add_function(wrap_pyfunction!(pebbles, m)?)?;
    m.add_function(wrap_pyfunction!(leaves, m)?)?;
    m.add_function(wrap_pyfunction!(sprouts, m)?)?;
    m.add_function(wrap_pyfunction!(fluid_grass, m)?)?;
    m.add_function(wrap_pyfunction!(lily_pads, m)?)?;
    m.add_function(wrap_pyfunction!(canopy, m)?)?;
    m.add_function(wrap_pyfunction!(tufts, m)?)?;
    m.add_function(wrap_pyfunction!(twigs, m)?)?;
    m.add_function(wrap_pyfunction!(vines, m)?)?;
    m.add_function(wrap_pyfunction!(cell_net, m)?)?;
    m.add_function(wrap_pyfunction!(eddies, m)?)?;
    m.add_function(wrap_pyfunction!(swells, m)?)?;
    m.add_function(wrap_pyfunction!(ripples, m)?)?;
    m.add_function(wrap_pyfunction!(growth, m)?)?;
    m.add_function(wrap_pyfunction!(isoline_blobs, m)?)?;
    m.add_function(wrap_pyfunction!(isoline_stack, m)?)?;
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
