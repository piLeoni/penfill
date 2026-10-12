//! Quick preview: `cargo run --release --example growth_demo`
//! Writes `examples/gallery/growth-*.svg`.

use penfill::geo::{Pt, Region};
use penfill::growth::{fill_growth, GrowthParams, RootSource};

fn main() {
    let disc = disc(60.0, 60.0, 55.0);
    let t = std::time::Instant::now();
    let out = fill_growth(&disc, &GrowthParams { seed: 5, ..Default::default() });
    eprintln!("growth-branching: {:?}", t.elapsed());
    write("examples/gallery/growth-branching.svg", &disc, &out);

    let fine = GrowthParams { seed: 5, spacing: 0.6, step: 0.25, reach: 5.0, kill: 0.6, pen: 0.12, roots: RootSource::Every(25.0), ..Default::default() };
    let out = fill_growth(&disc, &fine);
    write("examples/gallery/growth-fine.svg", &disc, &out);

    let jittery = GrowthParams { seed: 5, jitter: 0.5, ..Default::default() };
    let out = fill_growth(&disc, &jittery);
    write("examples/gallery/growth-jittery.svg", &disc, &out);
}

fn disc(cx: f64, cy: f64, r: f64) -> Region {
    let n = 96;
    let ring: Vec<Pt> = (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    Region::from_ring(&ring)
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
    s.push_str(&format!("<rect x='{}' y='{}' width='{w}' height='{h}' fill='#ffffff'/>\n", x0 - pad, y0 - pad));
    s.push_str("<g fill='none' stroke='#111' stroke-width='0.15' stroke-linecap='round' stroke-linejoin='round'>\n");
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
