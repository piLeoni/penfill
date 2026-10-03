//! WebAssembly API for the penfill playground (minimal surface, SVG out).

use penfill::geo::{Region as CoreRegion, Strokes};
use penfill::gravel::GravelEdge;
use penfill::svg::to_svg;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Region {
    inner: CoreRegion,
}

fn ring(flat: &[f64]) -> Result<Vec<(f64, f64)>, JsValue> {
    if flat.len() < 6 || flat.len() % 2 != 0 {
        return Err(JsValue::from_str("a ring needs at least three (x, y) pairs"));
    }
    Ok(flat.chunks(2).map(|c| (c[0], c[1])).collect())
}

#[wasm_bindgen]
impl Region {
    /// One outer ring: `[x0, y0, x1, y1, …]`, closed automatically.
    #[wasm_bindgen(constructor)]
    pub fn new(outer: Vec<f64>) -> Result<Region, JsValue> {
        Ok(Region {
            inner: CoreRegion::from_ring(&ring(&outer)?),
        })
    }

    /// Outer ring plus hole rings. `holes` is a JS array of flat rings.
    pub fn with_holes(outer: Vec<f64>, holes: JsValue) -> Result<Region, JsValue> {
        let mut polys = vec![ring(&outer)?];
        if !holes.is_undefined() && !holes.is_null() {
            let arr = js_sys::Array::from(&holes);
            for h in arr.iter() {
                polys.push(ring(&js_sys::Float64Array::from(h).to_vec())?);
            }
        }
        Ok(Region {
            inner: CoreRegion::from_polygons(&[polys]),
        })
    }
}

fn svg(region: &CoreRegion, strokes: Strokes) -> String {
    to_svg(&strokes, Some(region), 0.35)
}

fn seed_u64(seed: u32) -> u64 {
    seed as u64
}

const PLAYGROUND_FILLS: &[&str] = &[
    "zigzag",
    "rectilinear",
    "lines",
    "grid",
    "triangles",
    "concentric",
    "spiral",
    "hilbert",
    "honeycomb",
    "dots",
    "dots_hex",
    "waves",
    "wave_grid",
    "scales",
    "sunburst",
    "stipple",
    "stipple_noise",
    "stipple_edge",
    "gravel",
    "gravel_clipped",
    "grass",
    "scribble",
    "truchet",
    "marble",
    "wood",
    "cracks",
    "terrazzo",
    "chess",
];

fn hatch_inset(inset_mm: f64) -> Option<f64> {
    if inset_mm > 0.0 {
        Some(inset_mm)
    } else {
        None
    }
}

fn strokes_for(
    region: &CoreRegion,
    fill: &str,
    spacing: f64,
    angle: f64,
    seed: u32,
    inset_mm: f64,
) -> Result<Strokes, JsValue> {
    let s = spacing;
    let a = angle;
    let seed = seed_u64(seed);
    let inset = hatch_inset(inset_mm);
    const L: penfill::Anchor = penfill::Anchor::LOCAL;
    let pattern_region = if inset_mm > 0.0 {
        region.inset(inset_mm)
    } else {
        region.clone()
    };
    Ok(match fill {
        "zigzag" => penfill::zigzag::fill_zigzag_inset(region, s, a, inset, L),
        "rectilinear" => penfill::rectilinear::fill_rectilinear(region, s, a, inset, L),
        "concentric" => penfill::concentric::fill_concentric(region, s, inset),
        "spiral" => penfill::spiral::fill_spiral(&pattern_region, s),
        "hilbert" => penfill::hilbert::fill_hilbert(region, s, inset),
        "honeycomb" => penfill::honeycomb::fill_honeycomb(&pattern_region, s, a, 0.0, L),
        "dots" => penfill::dotswaves::fill_dots_with_lattice(
            &pattern_region,
            s,
            a,
            s * 0.2,
            penfill::dotswaves::Lattice::Square,
            L,
        ),
        "dots_hex" => penfill::dotswaves::fill_dots_with_lattice(
            &pattern_region,
            s,
            a,
            s * 0.2,
            penfill::dotswaves::Lattice::Hexagonal,
            L,
        ),
        "lines" => penfill::grid::fill_lines(&pattern_region, s, a, L),
        "grid" => penfill::grid::fill_grid(&pattern_region, s, a, L),
        "triangles" => penfill::grid::fill_triangles(&pattern_region, s, a, L),
        "waves" => penfill::dotswaves::fill_waves(&pattern_region, s, a, L),
        "wave_grid" => penfill::dotswaves::fill_wave_grid(&pattern_region, s, a, L),
        "scales" => penfill::scales::fill_scales(&pattern_region, s, a, L),
        "sunburst" => penfill::radial::fill_sunburst(&pattern_region, s, a, L),
        "stipple" => penfill::stipple::fill_stipple(&pattern_region, s, s * 0.3, seed),
        "stipple_noise" => {
            penfill::stipple::fill_stipple_noise(&pattern_region, s, s * 2.5, s * 0.4, 12.0, seed)
        }
        "stipple_edge" => {
            penfill::stipple::fill_stipple_edge(&pattern_region, s, s * 4.0, s * 0.4, 8.0, 0.5, seed)
        }
        "gravel" => penfill::gravel::fill_gravel_with(&pattern_region, s, seed, GravelEdge::Adaptive),
        "gravel_clipped" => {
            penfill::gravel::fill_gravel_with(&pattern_region, s, seed, GravelEdge::Clipped)
        }
        "grass" => penfill::grass::fill_grass(&pattern_region, s, a, seed, L),
        "scribble" => penfill::scribble::fill_scribble(&pattern_region, s, a, seed),
        "truchet" => penfill::truchet::fill_truchet(&pattern_region, s.max(0.5), a, seed, L),
        "marble" => penfill::textures::fill_marble(&pattern_region, s, a, seed, L),
        "wood" => penfill::textures::fill_wood(&pattern_region, s, a, seed, L),
        "cracks" => penfill::textures::fill_cracks(&pattern_region, s * 4.0, seed),
        "terrazzo" => penfill::terrazzo::fill_terrazzo(&pattern_region, s, s / 6.0, seed),
        "chess" => penfill::chess::fill_chess(&pattern_region, s, s / 8.0, a, L),
        other if other.starts_with("pat:") => {
            let name = other.strip_prefix("pat:").unwrap_or("");
            let pattern = penfill::pat::builtin(name)
                .ok_or_else(|| JsValue::from_str("unknown .pat pattern"))?;
            penfill::pat::fill_pat(&pattern_region, &pattern, s, a, L)
        }
        _ => return Err(JsValue::from_str("unknown fill")),
    })
}

/// Run a named fill from the playground UI.
#[wasm_bindgen(js_name = render)]
pub fn render(
    region: &Region,
    fill: &str,
    spacing: f64,
    angle: f64,
    seed: u32,
    inset_mm: f64,
) -> Result<String, JsValue> {
    Ok(svg(
        &region.inner,
        strokes_for(&region.inner, fill, spacing, angle, seed, inset_mm)?,
    ))
}

/// Names of fills exposed to the playground UI.
#[wasm_bindgen(js_name = fillNames)]
pub fn fill_names() -> js_sys::Array {
    let out = js_sys::Array::new();
    for name in PLAYGROUND_FILLS {
        out.push(&JsValue::from_str(name));
    }
    for name in penfill::pat::BUILTIN {
        out.push(&JsValue::from_str(&format!("pat:{name}")));
    }
    out
}
