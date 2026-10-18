//! WebAssembly API for the penfill playground (minimal surface, SVG out).

use penfill::geo::{Region as CoreRegion, Strokes};
use penfill::svg::to_svg;
use penfill::{all_fill_names, fill_by_name, EdgeMode, FillParams};
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

fn parse_edge(edge: Option<String>, overflow_mm: Option<f64>) -> Result<EdgeMode, JsValue> {
    match edge.as_deref().unwrap_or("clip") {
        "clip" => Ok(EdgeMode::Clip),
        "inside" => Ok(EdgeMode::Inside),
        "overflow" => Ok(EdgeMode::Overflow(overflow_mm.unwrap_or(2.0).max(0.0))),
        other => Err(JsValue::from_str(&format!(
            "edge must be 'clip', 'inside', or 'overflow', got {other:?}"
        ))),
    }
}

fn strokes_for(
    region: &CoreRegion,
    fill: &str,
    spacing: f64,
    angle: f64,
    seed: u32,
    inset_mm: f64,
    fray_mm: f64,
    edge: EdgeMode,
) -> Result<Strokes, JsValue> {
    let inset = (inset_mm > 0.0).then_some(inset_mm);
    let params = FillParams::new(spacing)
        .angle(angle)
        .seed(seed as u64)
        .inset(inset)
        .edge(edge)
        .fray(fray_mm.max(0.0));
    fill_by_name(fill, region, &params).ok_or_else(|| JsValue::from_str("unknown fill"))
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
    fray_mm: Option<f64>,
    edge: Option<String>,
    overflow_mm: Option<f64>,
) -> Result<String, JsValue> {
    let edge_mode = parse_edge(edge, overflow_mm)?;
    Ok(svg(
        &region.inner,
        strokes_for(
            &region.inner,
            fill,
            spacing,
            angle,
            seed,
            inset_mm,
            fray_mm.unwrap_or(0.0),
            edge_mode,
        )?,
    ))
}

/// Names of fills exposed to the playground UI.
#[wasm_bindgen(js_name = fillNames)]
pub fn fill_names() -> js_sys::Array {
    let out = js_sys::Array::new();
    for name in all_fill_names() {
        out.push(&JsValue::from_str(&name));
    }
    out
}
