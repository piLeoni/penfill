//! Quick preview: `cargo run --release --example isoline_blobs_demo`
//! Writes `examples/gallery/isoline-*.svg`.

use penfill::geo::{Pt, Region};
use penfill::isoline_blobs::{default_params, fill_isoline_blobs_with_coverage};
use penfill::isoline_stack::{fill_isoline_stack_with_coverage, EdgeMode, IsolineStackParams, Light};

fn main() {
    let square = Region::from_ring(&[(0.0, 0.0), (120.0, 0.0), (120.0, 120.0), (0.0, 120.0)]);
    let flat = fill_isoline_blobs_with_coverage(&square, &default_params(16.0, 0.7, 42));
    write("examples/gallery/isoline-blobs-scattered.svg", &square, &flat.strokes);

    let park = park();
    for (name, edge) in [("clip", EdgeMode::Clip), ("inside", EdgeMode::Inside), ("overflow", EdgeMode::Overflow(2.5))] {
        let p = IsolineStackParams { seed: 42, edge, ..Default::default() };
        let out = fill_isoline_stack_with_coverage(&park, &p);
        write(&format!("examples/gallery/isoline-stack-{name}.svg"), &park, &out.strokes);
    }
    let dense = IsolineStackParams { seed: 42, coverage: 1.0, edge: EdgeMode::Inside, ..Default::default() };
    let out = fill_isoline_stack_with_coverage(&park, &dense);
    write("examples/gallery/isoline-stack-dense.svg", &park, &out.strokes);

    let lit = IsolineStackParams {
        seed: 42,
        edge: EdgeMode::Inside,
        light: Some(Light { from_deg: -45.0, strength: 0.85 }),
        ..Default::default()
    };
    let out = fill_isoline_stack_with_coverage(&park, &lit);
    write("examples/gallery/isoline-stack-lit.svg", &park, &out.strokes);
}

/// An irregular park outline with a pond-shaped hole, ~120 mm across.
fn park() -> Region {
    let blob = |cx: f64, cy: f64, r: f64, wob: f64, n: usize| -> Vec<Pt> {
        (0..n)
            .map(|k| {
                let t = std::f64::consts::TAU * k as f64 / n as f64;
                let rr = r * (1.0 + wob * ((3.0 * t).sin() * 0.6 + (5.0 * t + 1.0).cos() * 0.4));
                (cx + rr * t.cos(), cy + rr * t.sin())
            })
            .collect()
    };
    Region::from_polygons(&[vec![blob(60.0, 60.0, 52.0, 0.18, 90), blob(72.0, 52.0, 11.0, 0.15, 40)]])
}

fn write(path: &str, region: &Region, strokes: &penfill::Strokes) {
    let (x0, y0, x1, y1) = region.bounds_mm();
    let pad = 8.0;
    let (w, h) = (x1 - x0 + 2.0 * pad, y1 - y0 + 2.0 * pad);
    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w}mm' height='{h}mm' viewBox='{} {} {w} {h}'>\n",
        x0 - pad,
        y0 - pad
    );
    s.push_str(&format!("<rect x='{}' y='{}' width='{w}' height='{h}' fill='#f4f0e6'/>\n", x0 - pad, y0 - pad));
    s.push_str("<g fill='none' stroke='#c33' stroke-width='0.15' stroke-dasharray='1 1'>\n");
    for ring in region.all_rings_mm() {
        s.push_str(&format!("<path d='{} Z'/>\n", d(&ring)));
    }
    s.push_str("</g>\n<g fill='none' stroke='#111' stroke-width='0.14' stroke-linecap='round' stroke-linejoin='round'>\n");
    for pts in strokes.iter().filter(|p| p.len() >= 2) {
        s.push_str(&format!("<path d='{}'/>\n", d(pts)));
    }
    s.push_str("</g>\n</svg>\n");
    std::fs::write(path, s).expect("write svg");
    eprintln!("wrote {path} ({} strokes)", strokes.len());
}

fn d(pts: &[Pt]) -> String {
    let mut out = format!("M {:.3},{:.3}", pts[0].0, pts[0].1);
    for p in &pts[1..] {
        out.push_str(&format!(" L {:.3},{:.3}", p.0, p.1));
    }
    out
}
