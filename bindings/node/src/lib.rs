//! Node bindings: `import { Region, zigzag } from 'penfill'`.
//!
//! Points are `[x, y]` arrays, strokes are arrays of points, all in mm.
//! Required parameters are positional; the rest go in an options object.

use napi::bindgen_prelude::*;
use napi_derive::napi;
use penfill_core as pf;
use pf::geo::{Pt, Region as CoreRegion, Strokes};
use std::cell::RefCell;

type JsStrokes = Vec<Vec<Vec<f64>>>;

fn pts(v: &[Vec<f64>]) -> Result<Vec<Pt>> {
    v.iter()
        .map(|p| match p.as_slice() {
            [x, y, ..] => Ok((*x, *y)),
            _ => Err(Error::from_reason("each point needs an x and a y")),
        })
        .collect()
}

fn strokes_in(v: &[Vec<Vec<f64>>]) -> Result<Strokes> {
    v.iter().map(|s| pts(s)).collect()
}

fn out(s: Strokes) -> JsStrokes {
    s.into_iter().map(|line| line.into_iter().map(|(x, y)| vec![x, y]).collect()).collect()
}

/// A filled area in mm: outer rings with optional holes.
#[napi]
pub struct Region {
    inner: CoreRegion,
}

#[napi]
impl Region {
    /// One polygon: an outer ring and optional holes.
    #[napi(constructor, ts_args_type = "outer: Array<[number, number]>, holes?: Array<Array<[number, number]>>")]
    pub fn new(outer: Vec<Vec<f64>>, holes: Option<Vec<Vec<Vec<f64>>>>) -> Result<Self> {
        let mut rings = vec![pts(&outer)?];
        if let Some(h) = holes {
            rings.extend(strokes_in(&h)?);
        }
        Ok(Region { inner: CoreRegion::from_polygons(&[rings]) })
    }

    /// Several polygons, each written as `[outer, hole, hole, ...]`.
    #[napi(factory, ts_args_type = "polygons: Array<Array<Array<[number, number]>>>")]
    pub fn from_polygons(polygons: Vec<Vec<Vec<Vec<f64>>>>) -> Result<Self> {
        let polys: Vec<Vec<Vec<Pt>>> = polygons.iter().map(|p| strokes_in(p)).collect::<Result<_>>()?;
        Ok(Region { inner: CoreRegion::from_polygons(&polys) })
    }

    /// Shrunk by `mm` (mitred corners).
    #[napi]
    pub fn inset(&self, mm: f64) -> Region {
        Region { inner: self.inner.inset(mm) }
    }

    /// Grown by `mm` (round corners).
    #[napi]
    pub fn outset(&self, mm: f64) -> Region {
        Region { inner: self.inner.outset(mm) }
    }

    #[napi]
    pub fn intersection(&self, other: &Region) -> Region {
        Region { inner: self.inner.intersection(&other.inner) }
    }

    #[napi]
    pub fn difference(&self, other: &Region) -> Region {
        Region { inner: self.inner.difference(&other.inner) }
    }

    /// Cut any polylines to the region.
    #[napi(ts_args_type = "strokes: Array<Array<[number, number]>>", ts_return_type = "Array<Array<[number, number]>>")]
    pub fn clip(&self, strokes: Vec<Vec<Vec<f64>>>) -> Result<JsStrokes> {
        Ok(out(self.inner.clip_strokes(&strokes_in(&strokes)?)))
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
        self.inner.parts().into_iter().map(|inner| Region { inner }).collect()
    }
}

/// Runs a fill that calls back into JS; the first exception thrown by the
/// callback is rethrown once the fill returns.
fn with_callback<T>(call: impl Fn(&[f64]) -> Result<f64>, run: impl FnOnce(&dyn Fn(&[f64]) -> f64) -> T) -> Result<T> {
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
        Some(other) => return Err(Error::from_reason(format!("anchor must be 'local' or 'global', not {other:?}"))),
    };
    let shift = match shift.as_deref() {
        None => (0.0, 0.0),
        Some([x, y]) => (*x, *y),
        Some(_) => return Err(Error::from_reason("shift must be [x, y]")),
    };
    Ok(pf::Anchor { origin, shift })
}

#[napi(object)]
#[derive(Default)]
pub struct LineOptions {
    /// Degrees. Default 0.
    pub angle: Option<f64>,
    /// Gap between the wall and the lines, mm. Default half the spacing.
    pub inset: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

#[napi(object)]
#[derive(Default)]
pub struct AngleOptions {
    /// Degrees. Default 0.
    pub angle: Option<f64>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

#[napi(object)]
#[derive(Default)]
pub struct SeedOptions {
    /// Degrees. Default 0.
    pub angle: Option<f64>,
    /// Same seed, same drawing. Default 0.
    pub seed: Option<u32>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

macro_rules! angle_fill {
    ($(#[$doc:meta])* $js:ident, $path:path) => {
        $(#[$doc])*
        #[napi(ts_return_type = "Array<Array<[number, number]>>")]
        pub fn $js(region: &Region, spacing: f64, options: Option<AngleOptions>) -> Result<JsStrokes> {
            let o = options.unwrap_or_default();
            Ok(out($path(&region.inner, spacing, o.angle.unwrap_or(0.0), anchor_of(&o.anchor, &o.shift)?)))
        }
    };
}

macro_rules! seed_fill {
    ($(#[$doc:meta])* $js:ident, $path:path) => {
        $(#[$doc])*
        #[napi(ts_return_type = "Array<Array<[number, number]>>")]
        pub fn $js(region: &Region, spacing: f64, options: Option<SeedOptions>) -> Result<JsStrokes> {
            let o = options.unwrap_or_default();
            let anchor = anchor_of(&o.anchor, &o.shift)?;
            Ok(out($path(&region.inner, spacing, o.angle.unwrap_or(0.0), o.seed.unwrap_or(0) as u64, anchor)))
        }
    };
}

// ---- lines and paths ----

/// Parallel hatch, one stroke per fragment.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn rectilinear(region: &Region, spacing: f64, options: Option<LineOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    Ok(out(pf::rectilinear::fill_rectilinear(&region.inner, spacing, o.angle.unwrap_or(0.0), o.inset, anchor_of(&o.anchor, &o.shift)?)))
}

/// Hatch joined into serpentines; the pen lifts only where a join would leave the shape.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn zigzag(region: &Region, spacing: f64, options: Option<LineOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    Ok(out(pf::zigzag::fill_zigzag_inset(&region.inner, spacing, o.angle.unwrap_or(0.0), o.inset, anchor_of(&o.anchor, &o.shift)?)))
}

angle_fill!(
    /// One-direction hatch over the whole region.
    lines,
    pf::grid::fill_lines
);
angle_fill!(
    /// Two hatches at 90°.
    grid,
    pf::grid::fill_grid
);
angle_fill!(
    /// Three hatches at 60° sharing every crossing.
    triangles,
    pf::grid::fill_triangles
);

#[napi(object)]
pub struct ConcentricOptions {
    /// Gap between the wall and the first ring, mm. Default half the spacing.
    pub inset: Option<f64>,
}

/// Closed rings following the outline inwards.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn concentric(region: &Region, spacing: f64, options: Option<ConcentricOptions>) -> JsStrokes {
    out(pf::concentric::fill_concentric(&region.inner, spacing, options.and_then(|o| o.inset)))
}

/// One continuous path winding inwards.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn spiral(region: &Region, spacing: f64) -> JsStrokes {
    out(pf::spiral::fill_spiral(&region.inner, spacing))
}

/// Space-filling Hilbert curve, one continuous path clipped to the region.
#[napi]
pub fn hilbert(region: &Region, spacing: f64, options: Option<LineOptions>) -> JsStrokes {
    let inset = options.and_then(|o| o.inset);
    out(pf::hilbert::fill_hilbert(&region.inner, spacing, inset))
}

// ---- lattices ----

#[napi(object)]
#[derive(Default)]
pub struct HoneycombOptions {
    pub angle: Option<f64>,
    /// Gap between neighbouring cells, mm. Default 0 (cells touch).
    pub line_width: Option<f64>,
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
    let anchor = anchor_of(&o.anchor, &o.shift)?;
    Ok(out(pf::honeycomb::fill_honeycomb(&region.inner, spacing, o.angle.unwrap_or(0.0), o.line_width.unwrap_or(0.0), anchor)))
}

#[napi(object)]
#[derive(Default)]
pub struct DotsOptions {
    pub angle: Option<f64>,
    /// Mark length, mm. Default 0.2 × spacing.
    pub mark: Option<f64>,
    /// `"square"` (default) or `"hex"`.
    pub lattice: Option<String>,
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
        Some(other) => return Err(Error::from_reason(format!("lattice must be 'square' or 'hex', not {other:?}"))),
    };
    let mark = o.mark.unwrap_or(spacing * 0.2);
    Ok(out(pf::dotswaves::fill_dots_with_lattice(&region.inner, spacing, o.angle.unwrap_or(0.0), mark, lattice, anchor_of(&o.anchor, &o.shift)?)))
}

angle_fill!(
    /// Parallel sine waves.
    waves,
    pf::dotswaves::fill_waves
);
angle_fill!(
    /// Ogee: mirrored waves closing into diamonds.
    wave_grid,
    pf::dotswaves::fill_wave_grid
);
angle_fill!(
    /// Fish scales; each row of arcs is one stroke.
    scales,
    pf::scales::fill_scales
);

#[napi(object)]
#[derive(Default)]
pub struct SunburstOptions {
    pub angle: Option<f64>,
    /// Where the rays start, on the page: shorthand for `anchor: 'global',
    /// shift: centre`. Default the middle of the bounds.
    #[napi(ts_type = "[number, number]")]
    pub centre: Option<Vec<f64>>,
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
    Ok(out(pf::radial::fill_sunburst(&region.inner, spacing, o.angle.unwrap_or(0.0), anchor)))
}

// ---- scatter and nature ----

#[napi(object)]
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
}

fn stipple_opts(o: Option<StippleOptions>) -> StippleOptions {
    o.unwrap_or(StippleOptions { mark: None, seed: None, feature: None, falloff: None, rough: None })
}

/// Evenly spread short marks.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn stipple(region: &Region, spacing: f64, options: Option<StippleOptions>) -> JsStrokes {
    let o = stipple_opts(options);
    out(pf::stipple::fill_stipple(&region.inner, spacing, o.mark.unwrap_or(spacing * 0.3), o.seed.unwrap_or(0) as u64))
}

/// Stipple whose density drifts with Perlin noise.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn stipple_noise(region: &Region, dense: f64, sparse: f64, options: Option<StippleOptions>) -> JsStrokes {
    let o = stipple_opts(options);
    let mark = o.mark.unwrap_or(dense * 0.4);
    out(pf::stipple::fill_stipple_noise(&region.inner, dense, sparse, mark, o.feature.unwrap_or(12.0), o.seed.unwrap_or(0) as u64))
}

/// Stipple dense along the wall, fading inwards.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn stipple_edge(region: &Region, dense: f64, sparse: f64, options: Option<StippleOptions>) -> JsStrokes {
    let o = stipple_opts(options);
    let mark = o.mark.unwrap_or(dense * 0.4);
    let (falloff, rough) = (o.falloff.unwrap_or(8.0), o.rough.unwrap_or(0.5));
    out(pf::stipple::fill_stipple_edge(&region.inner, dense, sparse, mark, falloff, rough, o.seed.unwrap_or(0) as u64))
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
    let o = stipple_opts(options);
    let mark = o.mark.unwrap_or(dense * 0.4);
    let seed = o.seed.unwrap_or(0) as u64;
    with_callback(
        |a| field.call((a[0], a[1]).into()),
        |f| out(pf::stipple::fill_stipple_field(&region.inner, dense, sparse, mark, seed, |p| f(&[p.0, p.1]))),
    )
}

#[napi(object)]
pub struct GravelOptions {
    pub seed: Option<u32>,
    /// Cut uniform stones at the wall instead of shrinking them into the corners.
    pub clipped: Option<bool>,
}

/// Rounded Voronoi stones.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn gravel(region: &Region, spacing: f64, options: Option<GravelOptions>) -> JsStrokes {
    let (seed, clipped) = options.map(|o| (o.seed.unwrap_or(0), o.clipped.unwrap_or(false))).unwrap_or((0, false));
    let edge = if clipped { pf::gravel::GravelEdge::Clipped } else { pf::gravel::GravelEdge::Adaptive };
    out(pf::gravel::fill_gravel_with(&region.inner, spacing, seed as u64, edge))
}

seed_fill!(
    /// Grass tufts as a repeating hatch tile.
    grass,
    pf::grass::fill_grass
);
/// Hand-drawn cross-hatching.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn scribble(region: &Region, spacing: f64, options: Option<SeedOptions>) -> JsStrokes {
    let o = options.unwrap_or_default();
    out(pf::scribble::fill_scribble(&region.inner, spacing, o.angle.unwrap_or(0.0), o.seed.unwrap_or(0) as u64))
}
seed_fill!(
    /// Random quarter-circle tiles joined into winding paths; `spacing` is the cell size.
    truchet,
    pf::truchet::fill_truchet
);

// ---- materials ----

seed_fill!(
    /// Marble veins.
    marble,
    pf::textures::fill_marble
);
seed_fill!(
    /// Wood growth rings bending around knots.
    wood,
    pf::textures::fill_wood
);

#[napi(object)]
pub struct CracksOptions {
    pub seed: Option<u32>,
}

/// Parched ground / mud cracks.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn cracks(region: &Region, spacing: f64, options: Option<CracksOptions>) -> JsStrokes {
    let seed = options.and_then(|o| o.seed).unwrap_or(0);
    out(pf::textures::fill_cracks(&region.inner, spacing, seed as u64))
}

#[napi(object)]
pub struct TerrazzoOptions {
    /// Hatch spacing in the dark chips, mm. Default size / 6.
    pub hatch: Option<f64>,
    pub seed: Option<u32>,
}

/// Terrazzo chips; about a third are dark and hatched.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn terrazzo(region: &Region, size: f64, options: Option<TerrazzoOptions>) -> JsStrokes {
    let (hatch, seed) = options.map(|o| (o.hatch, o.seed.unwrap_or(0))).unwrap_or((None, 0));
    out(pf::terrazzo::fill_terrazzo(&region.inner, size, hatch.unwrap_or(size / 6.0), seed as u64))
}

#[napi(object)]
#[derive(Default)]
pub struct ChessOptions {
    /// Hatch spacing in the dark squares, mm. Default cell / 8.
    pub spacing: Option<f64>,
    pub angle: Option<f64>,
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
pub fn chess(region: &Region, cell: f64, options: Option<ChessOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let anchor = anchor_of(&o.anchor, &o.shift)?;
    Ok(out(pf::chess::fill_chess(&region.inner, cell, o.spacing.unwrap_or(cell / 8.0), o.angle.unwrap_or(0.0), anchor)))
}

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
        Some(text) => pf::pat::parse_pat(text).into_iter().find(|p| p.name.eq_ignore_ascii_case(&name)),
        None => pf::pat::builtin(&name),
    };
    let pattern = pattern.ok_or_else(|| Error::from_reason(format!("no pattern named {name:?}")))?;
    Ok(out(pf::pat::fill_pat(&region.inner, &pattern, o.scale.unwrap_or(1.0), o.angle.unwrap_or(0.0), anchor_of(&o.anchor, &o.shift)?)))
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
    seed: u32,
    anchor: pf::Anchor,
    perlin: pf::noise::Perlin,
}

impl Tools {
    fn make(region: CoreRegion, seed: u32, anchor: pf::Anchor) -> Tools {
        Tools { region, seed, anchor, perlin: pf::noise::Perlin::new(seed as u64) }
    }

    fn core(&self) -> pf::pattern::Tools<'_> {
        pf::pattern::Tools::new(&self.region, self.seed as u64).anchored(self.anchor)
    }
}

fn points_out(p: Vec<Pt>) -> Vec<Vec<f64>> {
    p.into_iter().map(|(x, y)| vec![x, y]).collect()
}

#[napi]
impl Tools {
    /// The same tools for another region, e.g. a piece from `region.parts()`.
    #[napi]
    pub fn on(&self, region: &Region, seed: Option<u32>) -> Tools {
        Tools::make(region.inner.clone(), seed.unwrap_or(self.seed), self.anchor)
    }

    /// Smooth Perlin noise, roughly -1 to 1, one bump per unit; each extra
    /// octave adds detail twice as fine and half as strong.
    #[napi]
    pub fn noise(&self, x: f64, y: f64, octaves: Option<u32>) -> f64 {
        self.perlin.fbm(x, y, octaves.unwrap_or(1).max(1) as usize)
    }

    /// Random points in the region, no two closer than `spacing` mm.
    #[napi(ts_return_type = "Array<[number, number]>")]
    pub fn scatter(&self, spacing: f64) -> Vec<Vec<f64>> {
        points_out(self.core().scatter(spacing))
    }

    /// Random points `dense` mm apart where `field(x, y)` is 0 and `sparse`
    /// mm apart where it is 1.
    #[napi(ts_return_type = "Array<[number, number]>")]
    pub fn scatter_field(&self, dense: f64, sparse: f64, field: Function<FnArgs<(f64, f64)>, f64>) -> Result<Vec<Vec<f64>>> {
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
}

/// What `pattern` hands your function: the region to fill, its bounds and
/// the seed.
#[napi]
pub struct Canvas {
    region: CoreRegion,
    bounds: (f64, f64, f64, f64),
    seed: u32,
    origin: Pt,
}

#[napi]
impl Canvas {
    #[napi(getter)]
    pub fn region(&self) -> Region {
        Region { inner: self.region.clone() }
    }

    /// `[minX, minY, maxX, maxY]` of the whole region.
    #[napi(getter, ts_return_type = "[number, number, number, number]")]
    pub fn bounds(&self) -> Vec<f64> {
        let (a, b, c, d) = self.bounds;
        vec![a, b, c, d]
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
}

#[napi(object)]
#[derive(Default)]
pub struct PatternOptions {
    /// Passed on to your function as `canvas.seed`. Default 0.
    pub seed: Option<u32>,
    /// `"local"` (default) sits the lattice on this shape; `"global"` gives the
    /// page one lattice, so neighbouring regions meet with no seam.
    #[napi(ts_type = "'local' | 'global'")]
    pub anchor: Option<String>,
    /// Moves the lattice from its anchor, mm.
    #[napi(ts_type = "[number, number]")]
    pub shift: Option<Vec<f64>>,
}

/// Your own pattern: `draw(canvas, tools)` returns lines anywhere on the
/// page and they come back clipped to the region. `tools` is optional.
#[napi(
    ts_args_type = "region: Region, draw: (canvas: Canvas, tools: Tools) => Array<Array<[number, number]>>, options?: PatternOptions",
    ts_return_type = "Array<Array<[number, number]>>"
)]
pub fn pattern(region: &Region, draw: Function<FnArgs<(Canvas, Tools)>, JsStrokes>, options: Option<PatternOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let (seed, anchor) = (o.seed.unwrap_or(0), anchor_of(&o.anchor, &o.shift)?);
    let error: RefCell<Option<Error>> = RefCell::new(None);
    let strokes = pf::pattern::fill_pattern(&region.inner, seed as u64, anchor, |c, _| {
        let canvas = Canvas { region: c.region.clone(), bounds: c.bounds, seed, origin: c.origin };
        let tools = Tools::make(c.region.clone(), seed, anchor);
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
pub fn profile(region: &Region, spacing: f64, f: Function<f64, f64>, options: Option<AngleOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let (angle, anchor) = (o.angle.unwrap_or(0.0), anchor_of(&o.anchor, &o.shift)?);
    with_callback(|a| f.call(a[0]), |g| out(pf::field::fill_profile(&region.inner, spacing, angle, anchor, |x| g(&[x]))))
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
pub fn tile(region: &Region, strokes: Vec<Vec<Vec<f64>>>, width: f64, height: f64, options: Option<TileOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or_default();
    let anchor = anchor_of(&o.anchor, &o.shift)?;
    let (angle, row_shift) = (o.angle.unwrap_or(0.0), o.row_shift.unwrap_or(0.0));
    Ok(out(pf::tile::fill_tile(&region.inner, &strokes_in(&strokes)?, width, height, angle, row_shift, anchor)))
}

/// Repeat an SVG drawing as a tile `width` mm wide.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn svg_tile(region: &Region, svg: String, width: f64, options: Option<TileOptions>) -> Result<JsStrokes> {
    let drawing = pf::svg::parse_svg(&svg).map_err(Error::from_reason)?;
    let o = options.unwrap_or_default();
    let anchor = anchor_of(&o.anchor, &o.shift)?;
    let (angle, row_shift) = (o.angle.unwrap_or(0.0), o.row_shift.unwrap_or(0.0));
    Ok(out(pf::svg::fill_svg_tile(&region.inner, &drawing, width, angle, row_shift, anchor)))
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
        Some(Either3::C(pair)) => pair.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" "),
    }
}

fn placement(
    size: Option<Either3<String, f64, Vec<f64>>>,
    position: Option<Either3<String, f64, Vec<f64>>>,
    repeat: Option<String>,
    default_size: &str,
) -> Result<pf::placement::Placement> {
    pf::placement::Placement::parse(&css(size, default_size), &css(position, "center"), repeat.as_deref().unwrap_or("no-repeat"))
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
pub fn svg_fill(region: &Region, svg: String, options: Option<PlaceOptions>) -> Result<JsStrokes> {
    let o = options.unwrap_or(PlaceOptions { size: None, position: None, repeat: None });
    let place = placement(o.size, o.position, o.repeat, "contain")?;
    let drawing = pf::svg::parse_svg(&svg).map_err(Error::from_reason)?;
    Ok(out(pf::svg::fill_svg_placed(&region.inner, &drawing, &place)))
}

/// A picture dithered inside the region. `image` is a file path, the bytes
/// of a PNG/JPEG/GIF/BMP/WebP file, or raw pixels.
#[napi(ts_return_type = "Array<Array<[number, number]>>")]
pub fn dither(region: &Region, image: Either3<String, Buffer, RawImage>, options: Option<DitherOptions>) -> Result<JsStrokes> {
    use pf::dither::Greyscale;
    let img = match image {
        Either3::A(path) => Greyscale::open(&path),
        Either3::B(bytes) => Greyscale::decode(&bytes),
        Either3::C(raw) => {
            let (w, h) = (raw.width as usize, raw.height as usize);
            match raw.data {
                Either3::A(d) => Greyscale::from_channels(w, h, &d.iter().map(|&v| v as f64).collect::<Vec<_>>(), 255.0),
                Either3::B(d) => Greyscale::from_channels(w, h, &d.iter().map(|&v| v as f64).collect::<Vec<_>>(), 255.0),
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
        method: o.method.as_deref().map_or(Ok(base.method), str::parse).map_err(Error::from_reason)?,
        marks: o.marks.as_deref().map_or(Ok(base.marks), str::parse).map_err(Error::from_reason)?,
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
#[napi(ts_args_type = "strokes: Array<Array<[number, number]>>, outline?: Region | null, strokeWidth?: number")]
pub fn to_svg(strokes: Vec<Vec<Vec<f64>>>, outline: Option<&Region>, stroke_width: Option<f64>) -> Result<String> {
    Ok(pf::svg::to_svg(&strokes_in(&strokes)?, outline.map(|r| &r.inner), stroke_width.unwrap_or(0.3)))
}
