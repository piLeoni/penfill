//! Clouds from above (`clouds`): one sheet of panels, a parameter changing
//! across each row.
//!
//!   cargo run --release --example clouds_demo

use penfill::clouds::{fill_clouds_with_coverage, CloudParams};
use penfill::Region;

fn main() {
    let panels: Vec<(&str, CloudParams)> = vec![
        ("default (sun 30° high, height 1)", CloudParams::default()),
        ("scattered (coverage 0.3)", CloudParams { coverage: 0.3, ..Default::default() }),
        ("broken (coverage 0.7)", CloudParams { coverage: 0.7, ..Default::default() }),
        ("flat sheet (height 0.35)", CloudParams { height: 0.35, ..Default::default() }),
        ("towering (height 1.8)", CloudParams { height: 1.8, ..Default::default() }),
        ("low sun (elevation 15°)", CloudParams { light_elevation: 15.0, ..Default::default() }),
        ("high sun (elevation 55°)", CloudParams { light_elevation: 55.0, ..Default::default() }),
        ("wind-drawn (stretch 0.8)", CloudParams { stretch: 0.8, wind_deg: -20.0, ..Default::default() }),
        ("no shade", CloudParams { shade: 0.0, ..Default::default() }),
    ];
    let w = 120.0;
    let gap = 14.0;
    let cols = 3;
    let rows = panels.len().div_ceil(cols);
    let (sw, sh) = (cols as f64 * (w + gap) + gap, rows as f64 * (w + gap) + gap);
    let mut s = format!("<svg xmlns='http://www.w3.org/2000/svg' width='{sw}mm' height='{sh}mm' viewBox='0 0 {sw} {sh}'>\n<rect width='{sw}' height='{sh}' fill='#fbfaf6'/>\n");
    for (i, (name, p)) in panels.iter().enumerate() {
        let (x, y) = (gap + (i % cols) as f64 * (w + gap), gap + (i / cols) as f64 * (w + gap));
        let sky = Region::from_ring(&[(x, y), (x + w, y), (x + w, y + w), (x, y + w)]);
        let t = std::time::Instant::now();
        let out = fill_clouds_with_coverage(&sky, p);
        println!("{name}: {} strokes, {:.0} ms", out.strokes.len(), t.elapsed().as_secs_f64() * 1000.0);
        s.push_str(&format!("<rect x='{x}' y='{y}' width='{w}' height='{w}' fill='none' stroke='#bbb' stroke-width='0.2'/>\n"));
        s.push_str(&format!("<text x='{x}' y='{}' font-family='Helvetica' font-size='4' fill='#555'>{name}</text>\n", y - 3.0));
        // The footprint, faint, to show what a map would clear.
        s.push_str("<g fill='#dde8f2' stroke='none'>\n");
        // One path, even-odd, so the clear cells inside a ring of cloud stay clear.
        let rings: Vec<String> = out.coverage.all_rings_mm().iter().map(|r| format!("{} Z", d(r))).collect();
        s.push_str(&format!("<path fill-rule='evenodd' d='{}'/>\n", rings.join(" ")));
        s.push_str("</g>\n<g fill='none' stroke='#1a1a1a' stroke-width='0.2' stroke-linecap='round' stroke-linejoin='round'>\n");
        for stroke in &out.strokes {
            s.push_str(&format!("<path d='{}'/>\n", d(stroke)));
        }
        s.push_str("</g>\n");
    }
    s.push_str("</svg>\n");
    let path = "examples/gallery/clouds-variations.svg";
    std::fs::write(path, s).unwrap();
    println!("wrote {path}");
}

fn d(pts: &[(f64, f64)]) -> String {
    pts.iter()
        .enumerate()
        .map(|(i, (x, y))| format!("{}{x:.2},{y:.2}", if i == 0 { "M" } else { "L" }))
        .collect::<Vec<_>>()
        .join(" ")
}
