//! Node bindings: `import { Region, zigzag, fill, fillOutput } from 'penfill'`.
//!
//! Points are `[x, y]` arrays, strokes are arrays of points, all in mm.
//! Required parameters are positional; the rest go in an options object.

use napi::bindgen_prelude::*;
use napi_derive::napi;
use penfill_core as pf;
use pf::geo::{Pt, Region as CoreRegion, Strokes};
use pf::pattern::{EdgeMode, FillParams};
use std::cell::RefCell;

type JsStrokes = Vec<Vec<Vec<f64>>>;

fn pt_in(p: &[f64]) -> Result<Pt> {
    match p {
        [x, y, ..] => Ok((*x, *y)),
        _ => Err(Error::from_reason("each point needs an x and a y")),
    }
}

fn pts(v: &[Vec<f64>]) -> Result<Vec<Pt>> {
    v.iter().map(|p| pt_in(p)).collect()
}

fn strokes_in(v: &[Vec<Vec<f64>>]) -> Result<Strokes> {
    v.iter().map(|s| pts(s)).collect()
}

fn out(s: Strokes) -> JsStrokes {
    s.into_iter()
        .map(|line| line.into_iter().map(|(x, y)| vec![x, y]).collect())
        .collect()
}

fn points_out(p: Vec<Pt>) -> Vec<Vec<f64>> {
    p.into_iter().map(|(x, y)| vec![x, y]).collect()
}

/// A filled area in mm: outer rings with optional holes.
#[napi]
pub struct Region {
    inner: CoreRegion,
}

#[napi]
impl Region {
    /// One polygon: an outer ring and optional holes.
    #[napi(
        constructor,
        ts_args_type = "outer: Array<[number, number]>, holes?: Array<Array<[number, number]>>"
    )]
    pub fn new(outer: Vec<Vec<f64>>, holes: Option<Vec<Vec<Vec<f64>>>>) -> Result<Self> {
        let mut rings = vec![pts(&outer)?];
        if let Some(h) = holes {
            rings.extend(strokes_in(&h)?);
        }
        Ok(Region {
            inner: CoreRegion::from_polygons(&[rings]),
        })
    }

    /// Several polygons, each written as `[outer, hole, hole, ...]`.
    #[napi(
        factory,
        ts_args_type = "polygons: Array<Array<Array<[number, number]>>>"
    )]
    pub fn from_polygons(polygons: Vec<Vec<Vec<Vec<f64>>>>) -> Result<Self> {
        let polys: Vec<Vec<Vec<Pt>>> = polygons
            .iter()
            .map(|p| strokes_in(p))
            .collect::<Result<_>>()?;
        Ok(Region {
            inner: CoreRegion::from_polygons(&polys),
        })
    }

    /// Build a polygon region around `strokes` inflated by `radius` mm.
    #[napi(
        factory,
        ts_args_type = "strokes: Array<Array<[number, number]>>, radius: number"
    )]
    pub fn around_strokes(strokes: Vec<Vec<Vec<f64>>>, radius: f64) -> Result<Self> {
        Ok(Region {
            inner: CoreRegion::around_strokes(&strokes_in(&strokes)?, radius),
        })
    }

    /// Shrunk by `mm` (mitred corners).
    #[napi]
    pub fn inset(&self, mm: f64) -> Region {
        Region {
            inner: self.inner.inset(mm),
        }
    }

    /// Grown by `mm` (round corners).
    #[napi]
    pub fn outset(&self, mm: f64) -> Region {
        Region {
            inner: self.inner.outset(mm),
        }
    }

    #[napi]
    pub fn union(&self, other: &Region) -> Region {
        Region {
            inner: self.inner.union(&other.inner),
        }
    }

    #[napi]
    pub fn intersection(&self, other: &Region) -> Region {
        Region {
            inner: self.inner.intersection(&other.inner),
        }
    }

    #[napi]
    pub fn difference(&self, other: &Region) -> Region {
        Region {
            inner: self.inner.difference(&other.inner),
        }
    }

    /// Cut any polylines to the region.
    #[napi(
        ts_args_type = "strokes: Array<Array<[number, number]>>",
        ts_return_type = "Array<Array<[number, number]>>"
    )]
    pub fn clip(&self, strokes: Vec<Vec<Vec<f64>>>) -> Result<JsStrokes> {
        Ok(out(self.inner.clip_strokes(&strokes_in(&strokes)?)))
    }

    /// Clip and apply perimeter edge fraying (`fray` in mm) to any polylines.
    #[napi(
        ts_args_type = "strokes: Array<Array<[number, number]>>, spacing: number, fray: number, seed?: number",
        ts_return_type = "Array<Array<[number, number]>>"
    )]
    pub fn fray(
        &self,
        strokes: Vec<Vec<Vec<f64>>>,
        spacing: f64,
        fray: f64,
        seed: Option<u32>,
    ) -> Result<JsStrokes> {
        Ok(out(pf::fray::fray_strokes(
            &self.inner,
            strokes_in(&strokes)?,
            spacing,
            fray,
            seed.unwrap_or(0) as u64,
        )))
    }

    #[napi]
    pub fn contains(&self, x: f64, y: f64) -> bool {
        self.inner.covers_point((x, y))
    }

    /// How far `(x, y)` is from the nearest wall or hole, mm.
    #[napi]
    pub fn distance(&self, x: f64, y: f64) -> f64 {
        self.inner.dist_to_boundary((x, y))
    }

    /// `[minX, minY, maxX, maxY]`.
    #[napi(getter, ts_return_type = "[number, number, number, number]")]
    pub fn bounds(&self) -> Vec<f64> {
        let (a, b, c, d) = self.inner.bounds_mm();
        vec![a, b, c, d]
    }

    /// Every ring, outers and holes.
    #[napi(getter, ts_return_type = "Array<Array<[number, number]>>")]
    pub fn rings(&self) -> JsStrokes {
        out(self.inner.all_rings_mm())
    }

    #[napi(getter)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// The separate pieces, each with its own holes. Costs a pass over the
    /// rings, so call it only when a pattern needs pieces one by one.
    #[napi]
    pub fn parts(&self) -> Vec<Region> {
        self.inner
            .parts()
            .into_iter()
            .map(|inner| Region { inner })
            .collect()
    }
}

/// Output of a fill with its polygon `mask`: `{ strokes, mask: Region }`.
#[napi]
pub struct FillOutput {
    strokes: JsStrokes,
    mask: CoreRegion,
}

#[napi]
impl FillOutput {
    #[napi(getter, ts_return_type = "Array<Array<[number, number]>>")]
    pub fn strokes(&self) -> JsStrokes {
        self.strokes.clone()
    }

    #[napi(getter)]
    pub fn mask(&self) -> Region {
        Region {
            inner: self.mask.clone(),
        }
    }
}

impl From<pf::FillOutput> for FillOutput {
    fn from(o: pf::FillOutput) -> Self {
        FillOutput {
            strokes: out(o.strokes),
            mask: o.mask,
        }
    }
}

/// Runs a fill that calls back into JS; the first exception thrown by the
/// callback is rethrown once the fill returns.
fn with_callback<T>(
    call: impl Fn(&[f64]) -> Result<f64>,
    run: impl FnOnce(&dyn Fn(&[f64]) -> f64) -> T,
) -> Result<T> {
    let error: RefCell<Option<Error>> = RefCell::new(None);
    let wrapped = |args: &[f64]| -> f64 {
        if error.borrow().is_some() {
            return f64::NAN;
        }
        call(args).unwrap_or_else(|e| {
            *error.borrow_mut() = Some(e);
            f64::NAN
        })
    };
    let result = run(&wrapped);
    match error.into_inner() {
        Some(e) => Err(e),
        None => Ok(result),
    }
}

fn anchor_of(anchor: &Option<String>, shift: &Option<Vec<f64>>) -> Result<pf::Anchor> {
    let origin = match anchor.as_deref() {
        None | Some("local") => pf::Origin::Local,
        Some("global") => pf::Origin::Global,
        Some(other) => {
            return Err(Error::from_reason(format!(
                "anchor must be 'local' or 'global', not {other:?}"
            )))
        }
    };
    let shift = match shift.as_deref() {
        None => (0.0, 0.0),
        Some([x, y]) => (*x, *y),
        Some(_) => return Err(Error::from_reason("shift must be [x, y]")),
    };
    Ok(pf::Anchor { origin, shift })
}

fn edge_of(edge: &Option<String>, overflow: Option<f64>) -> Result<EdgeMode> {
    match edge.as_deref() {
        None | Some("clip") => Ok(EdgeMode::Clip),
        Some("inside") | Some("adaptive") => Ok(EdgeMode::Inside),
        Some("overflow") => Ok(EdgeMode::Overflow(overflow.unwrap_or(2.0).max(0.0))),
        Some(other) => Err(Error::from_reason(format!(
            "edge must be 'clip', 'inside', or 'overflow', not {other:?}"
        ))),
    }
}

/// Unified options accepted by built-in fills and `fill` / `fillOutput`.
#[napi(object)]
#[derive(Default, Clone)]
pub struct FillOptions {
    /// Degrees. Default 0.
    pub angle: Option<f64>,
    /// Deterministic RNG seed. Default 0.
    pub seed: Option<u32>,
    /// Gap between the wall and the fill, mm.
    pub inset: Option<f64>,
    /// Boundary mode: `"clip"` (default), `"inside"`, or `"overflow"`.
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    /// Max overhang distance in mm when `edge: "overflow"`. Default 2.
    pub overflow: Option<f64>,
    /// Smooth perimeter disappearing band in mm (`0` = off).
    pub fray: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
    /// Pattern-specific custom numeric parameters (e.g. `{ jaggedness: 1.2, coverage: 0.7 }`).
    pub custom: Option<std::collections::HashMap<String, f64>>,
}

impl FillOptions {
    fn to_params(&self, spacing: f64) -> Result<FillParams> {
        let mut params = FillParams::new(spacing)
            .angle(self.angle.unwrap_or(0.0))
            .seed(self.seed.unwrap_or(0) as u64)
            .anchor(anchor_of(&self.anchor, &self.shift)?)
            .inset(self.inset)
            .edge(edge_of(&self.edge, self.overflow)?)
            .fray(self.fray.unwrap_or(0.0).max(0.0));
        if let Some(custom) = &self.custom {
            for (k, v) in custom {
                params.custom.set(k, *v);
            }
        }
        Ok(params)
    }
}

pub type LineOptions = FillOptions;
pub type AngleOptions = FillOptions;
pub type SeedOptions = FillOptions;
pub type ConcentricOptions = FillOptions;
pub type CracksOptions = FillOptions;

/// Names of all built-in fills supported by `fill` and `fillOutput`.
#[napi]
pub fn fills() -> Vec<String> {
    pf::all_fill_names()
}

/// Run any built-in pattern by name with unified `FillOptions`.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn fill(
    region: &Region,
    name: String,
    spacing: Option<f64>,
    options: Option<FillOptions>,
) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let params = o.to_params(spacing.unwrap_or(2.0))?;
    let strokes = pf::fill_by_name(&name, &region.inner, &params)
        .ok_or_else(|| Error::from_reason(format!("no fill named {name:?}")))?;
    Ok(out(strokes))
}

/// Run any built-in pattern by name and return `{ strokes, mask: Region }`.
#[napi]
pub fn fill_output(
    region: &Region,
    name: String,
    spacing: Option<f64>,
    options: Option<FillOptions>,
) -> Result<FillOutput> {
    let o = options.unwrap_or_default();
    let params = o.to_params(spacing.unwrap_or(2.0))?;
    let res = pf::fill_output_by_name(&name, &region.inner, &params)
        .ok_or_else(|| Error::from_reason(format!("no fill named {name:?}")))?;
    Ok(res.into())
}

macro_rules! registry_js_fill {
    ($(#[$doc:meta])* $js:ident, $name_lit:literal) => {
        $(#[$doc])*
        #[napi(ts_return_type = "Array<Array<[number, number]>>")]
        pub fn $js(region: &Region, spacing: f64, options: Option<FillOptions>) -> Result<JsStrokes> {
            fill(region, $name_lit.to_string(), Some(spacing), options)
        }
    };
}

// ---- lines and paths ----

registry_js_fill!(
    /// Parallel hatch, one stroke per fragment.
    rectilinear,
    "rectilinear"
);
registry_js_fill!(
    /// Hatch joined into serpentines; the pen lifts only where a join would leave the shape.
    zigzag,
    "zigzag"
);
registry_js_fill!(
    /// One-direction hatch over the whole region.
    lines,
    "lines"
);
registry_js_fill!(
    /// Two hatches at 90°.
    grid,
    "grid"
);
registry_js_fill!(
    /// Three hatches at 60° sharing every crossing.
    triangles,
    "triangles"
);

/// Closed rings following the outline inwards.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn concentric(region: &Region, spacing: f64, options: Option<FillOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let params = FillParams::new(spacing)
        .seed(o.seed.unwrap_or(0) as u64)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::concentric::fill_concentric(&c.limit, spacing, o.inset)
    })))
}

registry_js_fill!(
    /// One continuous path winding inwards.
    spiral,
    "spiral"
);

/// Space-filling Hilbert curve, one continuous path clipped to the region.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn hilbert(region: &Region, spacing: f64, options: Option<FillOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let params = FillParams::new(spacing)
        .seed(o.seed.unwrap_or(0) as u64)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::hilbert::fill_hilbert(&c.limit, spacing, o.inset)
    })))
}

// ---- lattices ----

#[napi(object)]
#[derive(Default)]
pub struct HoneycombOptions {
    pub angle: Option<f64>,
    /// Gap between neighbouring cells, mm. Default 0 (cells touch).
    pub line_width: Option<f64>,
    pub seed: Option<u32>,
    pub inset: Option<f64>,
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    pub overflow: Option<f64>,
    pub fray: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

/// Hexagon walls, each drawn once; `spacing` is flat to flat.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn honeycomb(region: &Region, spacing: f64, options: Option<HoneycombOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let params = FillParams::new(spacing)
        .angle(o.angle.unwrap_or(0.0))
        .seed(o.seed.unwrap_or(0) as u64)
        .anchor(anchor_of(&o.anchor, &o.shift)?)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::honeycomb::fill_honeycomb(
            &c.limit,
            spacing,
            c.angle,
            o.line_width.unwrap_or(0.0),
            c.anchor,
        )
    })))
}

#[napi(object)]
#[derive(Default)]
pub struct DotsOptions {
    pub angle: Option<f64>,
    /// Mark length, mm. Default 0.2 × spacing.
    pub mark: Option<f64>,
    /// `"square"` (default) or `"hex"`.
    pub lattice: Option<String>,
    pub seed: Option<u32>,
    pub inset: Option<f64>,
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    pub overflow: Option<f64>,
    pub fray: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

/// Short marks on a square or hexagonal lattice.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn dots(region: &Region, spacing: f64, options: Option<DotsOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let lattice = match o.lattice.as_deref() {
        None | Some("square") => pf::dotswaves::Lattice::Square,
        Some("hex") | Some("hexagonal") => pf::dotswaves::Lattice::Hexagonal,
        Some(other) => {
            return Err(Error::from_reason(format!(
                "lattice must be 'square' or 'hex', not {other:?}"
            )))
        }
    };
    let params = FillParams::new(spacing)
        .angle(o.angle.unwrap_or(0.0))
        .seed(o.seed.unwrap_or(0) as u64)
        .anchor(anchor_of(&o.anchor, &o.shift)?)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(
        pf::dotswaves::fill_dots_params(&region.inner, &params, o.mark, lattice).strokes,
    ))
}

registry_js_fill!(
    /// Parallel sine waves.
    waves,
    "waves"
);
registry_js_fill!(
    /// Ogee: mirrored waves closing into diamonds.
    wave_grid,
    "wave_grid"
);
registry_js_fill!(
    /// Fish scales; each row of arcs is one stroke.
    scales,
    "scales"
);

#[napi(object)]
#[derive(Default)]
pub struct SunburstOptions {
    pub angle: Option<f64>,
    /// Where the rays start, on the page: shorthand for `anchor: 'global',
    /// shift: centre`. Default the middle of the bounds.
    #[napi(ts_type = "[number, number]")]
    pub centre: Option<Vec<f64>>,
    pub seed: Option<u32>,
    pub inset: Option<f64>,
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    pub overflow: Option<f64>,
    pub fray: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

/// Rays from a centre, about `spacing` mm apart all the way out.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn sunburst(region: &Region, spacing: f64, options: Option<SunburstOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let anchor = match o.centre.as_deref() {
        Some([x, y]) => pf::Anchor::global(*x, *y),
        Some(_) => return Err(Error::from_reason("centre must be [x, y]")),
        None => anchor_of(&o.anchor, &o.shift)?,
    };
    let params = FillParams::new(spacing)
        .angle(o.angle.unwrap_or(0.0))
        .seed(o.seed.unwrap_or(0) as u64)
        .anchor(anchor)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::radial::fill_sunburst(&c.limit, spacing, c.angle, c.anchor)
    })))
}

// ---- scatter and nature ----

#[napi(object)]
#[derive(Default)]
pub struct StippleOptions {
    /// Mark length, mm. Default 0.3 × spacing (0.4 × dense for the variants).
    pub mark: Option<f64>,
    pub seed: Option<u32>,
    /// `stippleNoise`: size of the noise blobs, mm. Default 12.
    pub feature: Option<f64>,
    /// `stippleEdge`: distance over which the density fades, mm. Default 8.
    pub falloff: Option<f64>,
    /// `stippleEdge`: 0–1, how much noise roughens the fade. Default 0.5.
    pub rough: Option<f64>,
    pub inset: Option<f64>,
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    pub overflow: Option<f64>,
    pub fray: Option<f64>,
}

/// Evenly spread short marks.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn stipple(region: &Region, spacing: f64, options: Option<StippleOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let seed = o.seed.unwrap_or(0) as u64;
    let params = FillParams::new(spacing)
        .seed(seed)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::stipple::fill_stipple(&c.limit, spacing, o.mark.unwrap_or(spacing * 0.3), seed)
    })))
}

/// Stipple whose density drifts with Perlin noise.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn stipple_noise(
    region: &Region,
    dense: f64,
    sparse: f64,
    options: Option<StippleOptions>,
) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let seed = o.seed.unwrap_or(0) as u64;
    let mark = o.mark.unwrap_or(dense * 0.4);
    let params = FillParams::new(dense)
        .seed(seed)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::stipple::fill_stipple_noise(&c.limit, dense, sparse, mark, o.feature.unwrap_or(12.0), seed)
    })))
}

/// Stipple dense along the wall, fading inwards.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn stipple_edge(
    region: &Region,
    dense: f64,
    sparse: f64,
    options: Option<StippleOptions>,
) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let seed = o.seed.unwrap_or(0) as u64;
    let mark = o.mark.unwrap_or(dense * 0.4);
    let (falloff, rough) = (o.falloff.unwrap_or(8.0), o.rough.unwrap_or(0.5));
    let params = FillParams::new(dense)
        .seed(seed)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::stipple::fill_stipple_edge(&c.limit, dense, sparse, mark, falloff, rough, seed)
    })))
}

/// Stipple with density from `field(x, y)`: 0 is `dense`, 1 is `sparse`.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn stipple_field(
    region: &Region,
    dense: f64,
    sparse: f64,
    field: Function<FnArgs<(f64, f64)>, f64>,
    options: Option<StippleOptions>,
) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let mark = o.mark.unwrap_or(dense * 0.4);
    let seed = o.seed.unwrap_or(0) as u64;
    let params = FillParams::new(dense)
        .seed(seed)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    with_callback(
        |a| field.call((a[0], a[1]).into()),
        |f| {
            out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
                pf::stipple::fill_stipple_field(&c.limit, dense, sparse, mark, seed, |p| f(&[p.0, p.1]))
            }))
        },
    )
}

#[napi(object)]
#[derive(Default)]
pub struct GravelOptions {
    pub seed: Option<u32>,
    /// Cut uniform stones at the wall instead of shrinking them into the corners.
    pub clipped: Option<bool>,
    pub inset: Option<f64>,
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    pub overflow: Option<f64>,
    pub fray: Option<f64>,
}

/// Rounded Voronoi stones.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn gravel(region: &Region, spacing: f64, options: Option<GravelOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let edge_mode = if o.edge.is_some() {
        edge_of(&o.edge, o.overflow)?
    } else if o.clipped.unwrap_or(false) {
        EdgeMode::Clip
    } else {
        EdgeMode::Inside
    };
    let params = FillParams::new(spacing)
        .seed(o.seed.unwrap_or(0) as u64)
        .inset(o.inset)
        .edge(edge_mode)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::gravel::fill_gravel_params(&region.inner, &params).strokes))
}

registry_js_fill!(
    /// Grass tufts as a repeating hatch tile.
    grass,
    "grass"
);
registry_js_fill!(
    /// Hand-drawn cross-hatching.
    scribble,
    "scribble"
);
registry_js_fill!(
    /// Random quarter-circle tiles joined into winding paths; `spacing` is the cell size.
    truchet,
    "truchet"
);

// ---- materials, textures & nature ----

registry_js_fill!(
    /// Marble veins.
    marble,
    "marble"
);
registry_js_fill!(
    /// Wood growth rings bending around knots.
    wood,
    "wood"
);

/// Parched ground / mud cracks.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn cracks(region: &Region, spacing: f64, options: Option<FillOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let params = o.to_params(spacing)?;
    Ok(out(pf::textures::fill_cracks_params(&region.inner, &params).strokes))
}

#[napi(object)]
#[derive(Default)]
pub struct TerrazzoOptions {
    /// Hatch spacing in the dark chips, mm. Default size / 6.
    pub hatch: Option<f64>,
    pub seed: Option<u32>,
    pub inset: Option<f64>,
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    pub overflow: Option<f64>,
    pub fray: Option<f64>,
}

/// Terrazzo chips; about a third are dark and hatched.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn terrazzo(region: &Region, spacing: f64, options: Option<TerrazzoOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let params = FillParams::new(spacing)
        .seed(o.seed.unwrap_or(0) as u64)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(
        pf::terrazzo::fill_terrazzo_params(&region.inner, &params, o.hatch).strokes,
    ))
}

#[napi(object)]
#[derive(Default)]
pub struct ChessOptions {
    /// Hatch spacing in the dark squares, mm. Default cell / 8.
    pub hatch: Option<f64>,
    /// Legacy alias for `hatch`.
    pub spacing: Option<f64>,
    pub angle: Option<f64>,
    pub seed: Option<u32>,
    pub inset: Option<f64>,
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    pub overflow: Option<f64>,
    pub fray: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

/// Checkerboard with the dark squares hatched.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn chess(region: &Region, spacing: f64, options: Option<ChessOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let hatch = o.hatch.or(o.spacing).unwrap_or(spacing / 8.0);
    let params = FillParams::new(spacing)
        .angle(o.angle.unwrap_or(0.0))
        .seed(o.seed.unwrap_or(0) as u64)
        .anchor(anchor_of(&o.anchor, &o.shift)?)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::chess::fill_chess(&c.limit, spacing, hatch, c.angle, c.anchor)
    })))
}

registry_js_fill!(
    /// Small rounded stones on a Poisson grid.
    pebbles,
    "pebbles"
);
registry_js_fill!(
    /// Short blades and V-shaped sprouts.
    leaves,
    "leaves"
);
registry_js_fill!(
    /// Sparse sprout hatch: offset rows of single strokes and V marks.
    sprouts,
    "sprouts"
);
registry_js_fill!(
    /// Grass as short strokes aligned to a smooth vector field.
    fluid_grass,
    "fluid_grass"
);
registry_js_fill!(
    /// Lily pads: merged overlapping circle outlines with centre dots.
    lily_pads,
    "lily_pads"
);
registry_js_fill!(
    /// Top-down tree canopies: clustered wobbly lobes with centre dots.
    canopy,
    "canopy"
);
registry_js_fill!(
    /// Clustered columns of short curved grass/sedge blades and V-tufts.
    tufts,
    "tufts"
);
registry_js_fill!(
    /// Scattered branched twigs.
    twigs,
    "twigs"
);
registry_js_fill!(
    /// Parallel meandering stems with alternating side-shoots.
    vines,
    "vines"
);
registry_js_fill!(
    /// Single-line Voronoi polygonal crack network.
    cell_net,
    "cell_net"
);
registry_js_fill!(
    /// Flowing wave streamlines integrated around spiral eddies.
    eddies,
    "eddies"
);
registry_js_fill!(
    /// Calm water swells with lens-shaped interference bulges.
    swells,
    "swells"
);
registry_js_fill!(
    /// Wind-blown water ripples with organic noise breaks.
    ripples,
    "ripples"
);
registry_js_fill!(
    /// Space-colonization branching networks grown from roots.
    growth,
    "growth"
);
registry_js_fill!(
    /// Nested contour rings around metaballs or noise mountains.
    isoline_blobs,
    "isoline_blobs"
);
registry_js_fill!(
    /// Stacked 3D metaball crowns with hidden-line occlusion.
    isoline_stack,
    "isoline_stack"
);

// ---- CAD patterns ----

/// Names of the built-in `.pat` patterns.
#[napi]
pub fn patterns() -> Vec<String> {
    pf::pat::BUILTIN.iter().map(|s| s.to_string()).collect()
}

#[napi(object)]
#[derive(Default)]
pub struct PatOptions {
    /// mm per pattern unit. Default 1.
    pub scale: Option<f64>,
    pub angle: Option<f64>,
    /// Text of a `.pat` file to take the pattern from instead of the built-ins.
    pub source: Option<String>,
    pub seed: Option<u32>,
    pub inset: Option<f64>,
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    pub overflow: Option<f64>,
    pub fray: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

/// Hatch with an AutoCAD `.pat` pattern.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn pat(region: &Region, name: String, options: Option<PatOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let pattern = match &o.source {
        Some(text) => pf::pat::parse_pat(text)
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(&name)),
        None => pf::pat::builtin(&name),
    };
    let pattern = pattern.ok_or_else(|| Error::from_reason(format!("no pattern named {name:?}")))?;
    let scale = o.scale.unwrap_or(1.0);
    let params = FillParams::new(scale)
        .angle(o.angle.unwrap_or(0.0))
        .seed(o.seed.unwrap_or(0) as u64)
        .anchor(anchor_of(&o.anchor, &o.shift)?)
        .inset(o.inset)
        .edge(edge_of(&o.edge, o.overflow)?)
        .fray(o.fray.unwrap_or(0.0).max(0.0));
    Ok(out(pf::fill_pattern_with(&region.inner, &params, |c, _| {
        pf::pat::fill_pat(&c.limit, &pattern, scale, c.angle, c.anchor)
    })))
}

/// Names of the patterns in a `.pat` file's text.
#[napi]
pub fn pat_names(source: String) -> Vec<String> {
    pf::pat::parse_pat(&source).into_iter().map(|p| p.name).collect()
}

// ---- bring your own ----

#[napi(object)]
pub struct IsolineOptions {
    /// Sampling step, mm. Default 0.2.
    pub cell: Option<f64>,
}

/// The toolkit `pattern` hands your function next to the canvas: what the
/// built-in fills are made of, set to the canvas region and seed.
#[napi]
pub struct Tools {
    region: CoreRegion,
    limit: CoreRegion,
    params: FillParams,
}

impl Tools {
    fn make(region: CoreRegion, limit: CoreRegion, params: FillParams) -> Tools {
        Tools {
            region,
            limit,
            params,
        }
    }

    fn core(&self) -> pf::pattern::Tools<'_> {
        pf::pattern::Tools::from_params(&self.region, self.limit.clone(), &self.params)
    }
}

#[napi]
impl Tools {
    /// The same tools for another region, e.g. a piece from `region.parts()`.
    #[napi]
    pub fn on(&self, region: &Region, seed: Option<u32>) -> Tools {
        let mut params = self.params;
        if let Some(s) = seed {
            params.seed = s as u64;
        }
        let limit = params.resolve_limit(&region.inner);
        Tools::make(region.inner.clone(), limit, params)
    }

    /// Smooth Perlin noise, roughly -1 to 1, one bump per unit; each extra
    /// octave adds detail twice as fine and half as strong.
    #[napi]
    pub fn noise(&self, x: f64, y: f64, octaves: Option<u32>) -> f64 {
        self.core().noise(x, y, octaves.unwrap_or(1).max(1) as usize)
    }

    /// Smooth border-fade factor in `[0, 1]` at `(x, y)`.
    #[napi]
    pub fn whole_at(&self, x: f64, y: f64, fray: Option<f64>) -> f64 {
        let core = self.core();
        match fray {
            Some(f) => core.whole_at_band((x, y), self.params.spacing, f),
            None => core.whole_at((x, y)),
        }
    }

    /// Distance ramp from the wall (`0` at wall, `1` past `falloff` mm),
    /// roughened by `rough` (`0..=1`).
    #[napi]
    pub fn border_ramp(&self, x: f64, y: f64, falloff: Option<f64>, rough: Option<f64>) -> f64 {
        self.core()
            .border_ramp((x, y), falloff.unwrap_or(8.0), rough.unwrap_or(0.5))
    }

    /// Whether a discrete element of `radius` at `(x, y)` fits according to
    /// the active `edge` mode.
    #[napi]
    pub fn element_fits(&self, x: f64, y: f64, radius: f64) -> bool {
        self.core().element_fits((x, y), radius)
    }

    /// Random points in the region, no two closer than `spacing` mm.
    #[napi(ts_return_type = "Array<[number, number]>")]
    pub fn scatter(&self, spacing: f64) -> Vec<Vec<f64>> {
        points_out(self.core().scatter(spacing))
    }

    /// Random points `dense` mm apart where `field(x, y)` is 0 and `sparse`
    /// mm apart where it is 1.
    #[napi(ts_return_type = "Array<[number, number]>")]
    pub fn scatter_field(
        &self,
        dense: f64,
        sparse: f64,
        field: Function<FnArgs<(f64, f64)>, f64>,
    ) -> Result<Vec<Vec<f64>>> {
        with_callback(
            |a| field.call((a[0], a[1]).into()),
            |g| points_out(self.core().scatter_field(dense, sparse, |p| g(&[p.0, p.1]))),
        )
    }

    /// Parallel lines `spacing` mm apart, cut to the region right up to its
    /// walls, placed by the pattern's anchor.
    #[napi(ts_return_type = "Array<Array<[number, number]>>")]
    pub fn hatch(&self, spacing: f64, angle: Option<f64>) -> JsStrokes {
        out(self.core().hatch(spacing, angle.unwrap_or(0.0)))
    }

    /// Bounded Voronoi cell rings for `centres`.
    #[napi(
        ts_args_type = "centres: Array<[number, number]>, reach?: number",
        ts_return_type = "Array<Array<[number, number]>>"
    )]
    pub fn voronoi_cells(&self, centres: Vec<Vec<f64>>, reach: Option<f64>) -> Result<JsStrokes> {
        let c = pts(&centres)?;
        let r = reach.unwrap_or(self.params.spacing * 2.5);
        Ok(out(self
            .core()
            .voronoi_cells(&c, r)
            .into_iter()
            .map(|(_, _, ring)| ring)
            .collect()))
    }

    /// Deduplicated shared Voronoi edges `[a, b]` between `centres`.
    #[napi(
        ts_args_type = "centres: Array<[number, number]>, reach?: number",
        ts_return_type = "Array<Array<[number, number]>>"
    )]
    pub fn voronoi_edges(&self, centres: Vec<Vec<f64>>, reach: Option<f64>) -> Result<JsStrokes> {
        let c = pts(&centres)?;
        let r = reach.unwrap_or(self.params.spacing * 2.5);
        Ok(out(self
            .core()
            .voronoi_edges(&c, r)
            .into_iter()
            .map(|(_, _, a, b)| vec![a, b])
            .collect()))
    }

    /// Closed circle polyline of radius `r` around `(x, y)`.
    #[napi(ts_return_type = "Array<[number, number]>")]
    pub fn circle(&self, x: f64, y: f64, r: f64) -> Vec<Vec<f64>> {
        points_out(self.core().circle((x, y), r))
    }

    /// Irregular foliage/canopy lobe outline around `(x, y)`.
    #[napi(ts_return_type = "Array<[number, number]>")]
    pub fn lobe(&self, x: f64, y: f64, r: f64, phase: Option<f64>, rough: Option<f64>) -> Vec<Vec<f64>> {
        points_out(
            self.core()
                .lobe((x, y), r, phase.unwrap_or(0.0), rough.unwrap_or(0.14)),
        )
    }

    /// Circular arc around `(x, y)` of radius `r` from `a0` to `a1` radians.
    #[napi(ts_return_type = "Array<[number, number]>")]
    pub fn arc(&self, x: f64, y: f64, r: f64, a0: f64, a1: f64, steps: Option<u32>) -> Vec<Vec<f64>> {
        points_out(
            self.core()
                .arc((x, y), r, a0, a1, steps.unwrap_or(16) as usize),
        )
    }

    /// Straight tick centred at `(x, y)` at `angleRad` radians with half-length `half`.
    #[napi(ts_return_type = "Array<[number, number]>")]
    pub fn tick(&self, x: f64, y: f64, angle_rad: f64, half: f64) -> Vec<Vec<f64>> {
        points_out(self.core().tick((x, y), angle_rad, half))
    }

    /// Orthogonal `+` cross centred at `(x, y)`.
    #[napi(ts_return_type = "Array<Array<[number, number]>>")]
    pub fn cross(&self, x: f64, y: f64, half: f64, twist_rad: Option<f64>) -> JsStrokes {
        out(self.core().cross((x, y), half, twist_rad.unwrap_or(0.0)))
    }

    /// Diagonal `×` cross centred at `(x, y)`.
    #[napi(ts_return_type = "Array<Array<[number, number]>>")]
    pub fn x_cross(&self, x: f64, y: f64, half: f64, twist_rad: Option<f64>) -> JsStrokes {
        out(self.core().x_cross((x, y), half, twist_rad.unwrap_or(0.0)))
    }

    /// Chaikin corner-cutting smoothing over `rounds` passes.
    #[napi(
        ts_args_type = "points: Array<[number, number]>, rounds?: number",
        ts_return_type = "Array<[number, number]>"
    )]
    pub fn smooth(&self, points: Vec<Vec<f64>>, rounds: Option<u32>) -> Result<Vec<Vec<f64>>> {
        Ok(points_out(
            self.core()
                .smooth(&pts(&points)?, rounds.unwrap_or(2) as usize),
        ))
    }

    /// Resample a polyline so consecutive vertices are at most `step` mm apart.
    #[napi(
        ts_args_type = "points: Array<[number, number]>, step: number",
        ts_return_type = "Array<[number, number]>"
    )]
    pub fn resample(&self, points: Vec<Vec<f64>>, step: f64) -> Result<Vec<Vec<f64>>> {
        Ok(points_out(self.core().resample(&pts(&points)?, step)))
    }

    /// Record a closed ring (in mm) as part of this pattern's semantic mask.
    #[napi(ts_args_type = "ring: Array<[number, number]>")]
    pub fn add_mask_ring(&self, ring: Vec<Vec<f64>>) -> Result<()> {
        self.core().add_mask_ring(&pts(&ring)?);
        Ok(())
    }

    /// Record a `Region` as part of this pattern's semantic mask.
    #[napi]
    pub fn add_mask(&self, region: &Region) {
        self.core().add_mask(&region.inner);
    }
}

/// What `pattern` hands your function: the region to fill, its limit envelope,
/// bounds, spacing, angle, seed, origin, and fray band.
#[napi]
pub struct Canvas {
    region: CoreRegion,
    limit: CoreRegion,
    bounds: (f64, f64, f64, f64),
    spacing: f64,
    angle: f64,
    seed: u32,
    origin: Pt,
    fray: f64,
    custom: pf::CustomParams,
}

#[napi]
impl Canvas {
    #[napi(getter)]
    pub fn region(&self) -> Region {
        Region {
            inner: self.region.clone(),
        }
    }

    #[napi(getter)]
    pub fn limit(&self) -> Region {
        Region {
            inner: self.limit.clone(),
        }
    }

    /// `[minX, minY, maxX, maxY]` of the whole region.
    #[napi(getter, ts_return_type = "[number, number, number, number]")]
    pub fn bounds(&self) -> Vec<f64> {
        let (a, b, c, d) = self.bounds;
        vec![a, b, c, d]
    }

    #[napi(getter)]
    pub fn spacing(&self) -> f64 {
        self.spacing
    }

    #[napi(getter)]
    pub fn angle(&self) -> f64 {
        self.angle
    }

    #[napi(getter)]
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// The anchor resolved for this region: the point to build a lattice through.
    #[napi(getter, ts_return_type = "[number, number]")]
    pub fn origin(&self) -> Vec<f64> {
        vec![self.origin.0, self.origin.1]
    }

    #[napi(getter)]
    pub fn fray(&self) -> f64 {
        self.fray
    }

    /// Read a pattern-specific custom parameter, falling back to `defaultValue` (or `0`).
    #[napi]
    pub fn param(&self, key: String, default_value: Option<f64>) -> f64 {
        self.custom.get_or(&key, default_value.unwrap_or(0.0))
    }
}

#[napi(object)]
#[derive(Default, Clone)]
pub struct PatternOptions {
    /// Primary spacing in mm (`canvas.spacing`). Default 2.
    pub spacing: Option<f64>,
    /// Orientation in degrees (`canvas.angle`). Default 0.
    pub angle: Option<f64>,
    /// Passed on to your function as `canvas.seed`. Default 0.
    pub seed: Option<u32>,
    /// Wall inset in mm.
    pub inset: Option<f64>,
    /// Boundary mode: `"clip"` (default), `"inside"`, or `"overflow"`.
    #[napi(ts_type = "'clip' | 'inside' | 'overflow'")]
    pub edge: Option<String>,
    /// Max overhang distance in mm when `edge: "overflow"`. Default 2.
    pub overflow: Option<f64>,
    /// Smooth perimeter disappearing band in mm (`0` = off).
    pub fray: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
    /// Pattern-specific custom numeric parameters (e.g. `{ jaggedness: 1.2, coverage: 0.7 }`).
    pub custom: Option<std::collections::HashMap<String, f64>>,
}

impl PatternOptions {
    fn to_params(&self) -> Result<FillParams> {
        let mut params = FillParams::new(self.spacing.unwrap_or(2.0))
            .angle(self.angle.unwrap_or(0.0))
            .seed(self.seed.unwrap_or(0) as u64)
            .anchor(anchor_of(&self.anchor, &self.shift)?)
            .inset(self.inset)
            .edge(edge_of(&self.edge, self.overflow)?)
            .fray(self.fray.unwrap_or(0.0).max(0.0));
        if let Some(custom) = &self.custom {
            for (k, v) in custom {
                params.custom.set(k, *v);
            }
        }
        Ok(params)
    }
}

/// Your own pattern: `draw(canvas, tools)` returns lines anywhere on the
/// page and they come back clipped (and optionally frayed) to the region.
#[napi(
    ts_args_type = "region: Region, draw: (canvas: Canvas, tools: Tools) => Array<Array<[number, number]>>, options?: PatternOptions",
    ts_return_type = "Array<Array<[number, number]>>"
)]
pub fn pattern(
    region: &Region,
    draw: Function<FnArgs<(Canvas, Tools)>, JsStrokes>,
    options: Option<PatternOptions>,
) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let params = o.to_params()?;
    let seed_u32 = o.seed.unwrap_or(0);
    let error: RefCell<Option<Error>> = RefCell::new(None);
    let strokes = pf::pattern::fill_pattern_with(&region.inner, &params, |c, _| {
        let canvas = Canvas {
            region: c.region.clone(),
            limit: c.limit.clone(),
            bounds: c.bounds,
            spacing: c.spacing,
            angle: c.angle,
            seed: seed_u32,
            origin: c.origin,
            fray: c.fray,
            custom: c.custom,
        };
        let tools = Tools::make(c.region.clone(), c.limit.clone(), params);
        match draw.call((canvas, tools).into()).and_then(|s| strokes_in(&s)) {
            Ok(s) => s,
            Err(e) => {
                *error.borrow_mut() = Some(e);
                vec![]
            }
        }
    });
    match error.into_inner() {
        Some(e) => Err(e),
        None => Ok(out(strokes)),
    }
}

/// Like `pattern`, also returning the polygon `mask` (`FillOutput`).
#[napi(
    ts_args_type = "region: Region, draw: (canvas: Canvas, tools: Tools) => Array<Array<[number, number]>>, options?: PatternOptions"
)]
pub fn pattern_output(
    region: &Region,
    draw: Function<FnArgs<(Canvas, Tools)>, JsStrokes>,
    options: Option<PatternOptions>,
) -> Result<FillOutput> {
    let o = options.unwrap_or_default();
    let params = o.to_params()?;
    let seed_u32 = o.seed.unwrap_or(0);
    let error: RefCell<Option<Error>> = RefCell::new(None);
    let res = pf::pattern::fill_pattern_output(&region.inner, &params, |c, _| {
        let canvas = Canvas {
            region: c.region.clone(),
            limit: c.limit.clone(),
            bounds: c.bounds,
            spacing: c.spacing,
            angle: c.angle,
            seed: seed_u32,
            origin: c.origin,
            fray: c.fray,
            custom: c.custom,
        };
        let tools = Tools::make(c.region.clone(), c.limit.clone(), params);
        match draw.call((canvas, tools).into()).and_then(|s| strokes_in(&s)) {
            Ok(s) => s,
            Err(e) => {
                *error.borrow_mut() = Some(e);
                vec![]
            }
        }
    });
    match error.into_inner() {
        Some(e) => Err(e),
        None => Ok(res.into()),
    }
}

/// Contour lines of `f(x, y)` at `levels`.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn isolines(
    region: &Region,
    f: Function<FnArgs<(f64, f64)>, f64>,
    levels: Vec<f64>,
    options: Option<IsolineOptions>,
) -> Result<JsStrokes> {
    let cell = options.and_then(|o| o.cell).unwrap_or(0.2);
    with_callback(
        |a| f.call((a[0], a[1]).into()),
        |g| out(pf::field::fill_isolines(&region.inner, cell, &levels, |p| g(&[p.0, p.1]))),
    )
}

/// `lo, lo + step, ...` up to `hi`, for `isolines`.
#[napi]
pub fn levels_every(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    pf::field::levels_every(lo, hi, step)
}

/// `y = f(x)` drawn again every `spacing` mm.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn profile(
    region: &Region,
    spacing: f64,
    f: Function<f64, f64>,
    options: Option<FillOptions>,
) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let (angle, anchor) = (o.angle.unwrap_or(0.0), anchor_of(&o.anchor, &o.shift)?);
    with_callback(
        |a| f.call(a[0]),
        |g| out(pf::field::fill_profile(&region.inner, spacing, angle, anchor, |x| g(&[x]))),
    )
}

#[napi(object)]
#[derive(Default)]
pub struct TileOptions {
    pub angle: Option<f64>,
    /// Shift of every other row, in tiles (0.5 = brick bond). Default 0.
    pub row_shift: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

/// Repeat `strokes`, drawn in a `width × height` box, like wallpaper.
#[napi(
    ts_args_type = "region: Region, strokes: Array<Array<[number, number]>>, width: number, height: number, options?: TileOptions",
    ts_return_type = "Array<Array<[number, number]>>"
)]
pub fn tile(
    region: &Region,
    strokes: Vec<Vec<Vec<f64>>>,
    width: f64,
    height: f64,
    options: Option<TileOptions>,
) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let anchor = anchor_of(&o.anchor, &o.shift)?;
    let (angle, row_shift) = (o.angle.unwrap_or(0.0), o.row_shift.unwrap_or(0.0));
    Ok(out(pf::tile::fill_tile(
        &region.inner,
        &strokes_in(&strokes)?,
        width,
        height,
        angle,
        row_shift,
        anchor,
    )))
}

/// Repeat an SVG drawing as a tile `width` mm wide.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn svg_tile(
    region: &Region,
    svg: String,
    width: f64,
    options: Option<TileOptions>,
) -> Result<JsStrokes> {
    let drawing = pf::svg::parse_svg(&svg).map_err(Error::from_reason)?;
    let o = options.unwrap_or_default();
    let anchor = anchor_of(&o.anchor, &o.shift)?;
    let (angle, row_shift) = (o.angle.unwrap_or(0.0), o.row_shift.unwrap_or(0.0));
    Ok(out(pf::svg::fill_svg_tile(
        &region.inner,
        &drawing,
        width,
        angle,
        row_shift,
        anchor,
    )))
}

/// The line work of an SVG as strokes, scaled to `width` mm if given.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn svg_strokes(svg: String, width: Option<f64>) -> Result<JsStrokes> {
    let drawing = pf::svg::parse_svg(&svg).map_err(Error::from_reason)?;
    Ok(out(match width {
        Some(w) => drawing.fit_width(w).strokes,
        None => drawing.strokes,
    }))
}

/// Pixels you already have, e.g. a canvas `ImageData`.
#[napi(object)]
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    /// Grey, grey + alpha, RGB or RGBA samples, row by row: 0–255 in a
    /// `Uint8Array` / `Uint8ClampedArray`, 0–1 in a plain array.
    pub data: Either3<Uint8Array, Uint8ClampedArray, Vec<f64>>,
}

#[napi(object)]
pub struct DitherOptions {
    /// Cell length along a line, mm. Default 0.5.
    pub resolution: Option<f64>,
    /// Distance between lines, mm. Default = resolution.
    pub line_spacing: Option<f64>,
    /// threshold, random, bayer (bayer2–bayer16), halftone (halftone2–16),
    /// floyd-steinberg (default), atkinson, jarvis, stucki, sierra.
    pub method: Option<String>,
    /// `"lines"` (default): dark runs joined into strokes. `"dots"`: a tick per dark cell.
    pub marks: Option<String>,
    /// Turns the grid of cells, degrees. The picture stays put.
    pub angle: Option<f64>,
    /// Above 1 darkens the midtones, below 1 lightens them. Default 1.
    pub gamma: Option<f64>,
    /// Ink where the picture is light.
    pub invert: Option<bool>,
    /// Tick length for dots, mm. Default 0.3 × resolution.
    pub mark: Option<f64>,
    /// For the random method.
    pub seed: Option<u32>,
    /// CSS `background-size`: "cover" (default), "contain", "fill", "40 30", "50%", or mm.
    #[napi(ts_type = "string | number | [number, number]")]
    pub size: Option<Either3<String, f64, Vec<f64>>>,
    /// CSS `background-position`: "center" (default), "top left", "25% 75%", or mm.
    #[napi(ts_type = "string | number | [number, number]")]
    pub position: Option<Either3<String, f64, Vec<f64>>>,
    /// "no-repeat" (default), "repeat", "repeat-x", "repeat-y".
    pub repeat: Option<String>,
}

fn css(v: Option<Either3<String, f64, Vec<f64>>>, default: &str) -> String {
    match v {
        None => default.to_string(),
        Some(Either3::A(s)) => s,
        Some(Either3::B(n)) => n.to_string(),
        Some(Either3::C(pair)) => pair
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn placement(
    size: Option<Either3<String, f64, Vec<f64>>>,
    position: Option<Either3<String, f64, Vec<f64>>>,
    repeat: Option<String>,
    default_size: &str,
) -> Result<pf::placement::Placement> {
    pf::placement::Placement::parse(
        &css(size, default_size),
        &css(position, "center"),
        repeat.as_deref().unwrap_or("no-repeat"),
    )
    .map_err(Error::from_reason)
}

#[napi(object)]
pub struct PlaceOptions {
    /// CSS `background-size`: "contain" (default), "cover", "fill", "40 30", "50%", or mm.
    #[napi(ts_type = "string | number | [number, number]")]
    pub size: Option<Either3<String, f64, Vec<f64>>>,
    /// CSS `background-position`: "center" (default), "top left", "25% 75%", or mm.
    #[napi(ts_type = "string | number | [number, number]")]
    pub position: Option<Either3<String, f64, Vec<f64>>>,
    /// "no-repeat" (default), "repeat", "repeat-x", "repeat-y".
    pub repeat: Option<String>,
}

/// An SVG drawing placed like a CSS background and cut by the region.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn svg_fill(
    region: &Region,
    svg: String,
    options: Option<PlaceOptions>,
) -> Result<JsStrokes> {
    let o = options.unwrap_or(PlaceOptions {
        size: None,
        position: None,
        repeat: None,
    });
    let place = placement(o.size, o.position, o.repeat, "contain")?;
    let drawing = pf::svg::parse_svg(&svg).map_err(Error::from_reason)?;
    Ok(out(pf::svg::fill_svg_placed(&region.inner, &drawing, &place)))
}

/// A picture dithered inside the region. `image` is a file path, the bytes
/// of a PNG/JPEG/GIF/BMP/WebP file, or raw pixels.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn dither(
    region: &Region,
    image: Either3<String, Buffer, RawImage>,
    options: Option<DitherOptions>,
) -> Result<JsStrokes> {
    use pf::dither::Greyscale;
    let img = match image {
        Either3::A(path) => Greyscale::open(&path),
        Either3::B(bytes) => Greyscale::decode(&bytes),
        Either3::C(raw) => {
            let (w, h) = (raw.width as usize, raw.height as usize);
            match raw.data {
                Either3::A(d) => Greyscale::from_channels(
                    w,
                    h,
                    &d.iter().map(|&v| v as f64).collect::<Vec<_>>(),
                    255.0,
                ),
                Either3::B(d) => Greyscale::from_channels(
                    w,
                    h,
                    &d.iter().map(|&v| v as f64).collect::<Vec<_>>(),
                    255.0,
                ),
                Either3::C(d) => Greyscale::from_channels(w, h, &d, 1.0),
            }
        }
    }
    .map_err(Error::from_reason)?;
    let o = options.unwrap_or(DitherOptions {
        resolution: None,
        line_spacing: None,
        method: None,
        marks: None,
        angle: None,
        gamma: None,
        invert: None,
        mark: None,
        seed: None,
        size: None,
        position: None,
        repeat: None,
    });
    let resolution = o.resolution.unwrap_or(0.5);
    let base = pf::dither::DitherOptions::new(resolution);
    let opts = pf::dither::DitherOptions {
        line_spacing: o.line_spacing.unwrap_or(resolution),
        method: o
            .method
            .as_deref()
            .map_or(Ok(base.method), str::parse)
            .map_err(Error::from_reason)?,
        marks: o
            .marks
            .as_deref()
            .map_or(Ok(base.marks), str::parse)
            .map_err(Error::from_reason)?,
        angle: o.angle.unwrap_or(0.0),
        gamma: o.gamma.unwrap_or(1.0),
        invert: o.invert.unwrap_or(false),
        mark: o.mark.unwrap_or(base.mark),
        seed: o.seed.unwrap_or(0) as u64,
        placement: placement(o.size, o.position, o.repeat, "cover")?,
        ..base
    };
    Ok(out(pf::dither::fill_dither(&region.inner, &img, &opts)))
}

/// Strokes as an SVG document in mm, with the region's outline in grey.
#[napi(
    ts_args_type = "strokes: Array<Array<[number, number]>>, outline?: Region | null, strokeWidth?: number"
)]
pub fn to_svg(
    strokes: Vec<Vec<Vec<f64>>>,
    outline: Option<&Region>,
    stroke_width: Option<f64>,
) -> Result<String> {
    Ok(pf::svg::to_svg(
        &strokes_in(&strokes)?,
        outline.map(|r| &r.inner),
        stroke_width.unwrap_or(0.3),
    ))
}
