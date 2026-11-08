//! Preview sheet for the hand-drawn modifier: the same little "map" drawn
//! precisely, with the default hand, and with the looser sketch preset.
//! Run: `cargo run --release --example hand_demo` → `examples/gallery/hand-demo.svg`

use penfill::concentric::fill_concentric;
use penfill::geo::{Pt, Region, Strokes};
use penfill::rectilinear::fill_rectilinear;
use penfill::{hand, Anchor, HandParams};

const CELL: f64 = 80.0;
const GAP: f64 = 10.0;
const PAD: f64 = 10.0;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<Pt> {
    vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]
}

fn circle(cx: f64, cy: f64, r: f64) -> Vec<Pt> {
    let n = 64;
    (0..=n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// A small map-like scene in a CELL × CELL box at the origin.
fn scene() -> Strokes {
    let mut s: Strokes = Vec::new();
    // Blocks of buildings.
    for (bx, by) in [(4.0, 4.0), (30.0, 4.0), (4.0, 34.0)] {
        for i in 0..3 {
            for j in 0..2 {
                s.push(rect(bx + i as f64 * 7.5, by + j as f64 * 11.0, 6.0, 9.0));
            }
        }
    }
    // Roads: edges of a street grid.
    for y in [26.0, 30.0, 56.0, 60.0] {
        s.push(vec![(0.0, y), (CELL, y)]);
    }
    for x in [26.0, 29.0] {
        s.push(vec![(x, 0.0), (x, CELL)]);
    }
    // A winding path.
    s.push((0..=40).map(|i| {
        let t = i as f64 / 40.0;
        (30.0 + t * 50.0, 68.0 + 6.0 * (t * 9.0).sin())
    }).collect());
    // A pond with concentric shoreline lines, and a hatched park.
    let pond = Region::from_ring(&circle(62.0, 42.0, 10.0));
    s.extend(fill_concentric(&pond, 1.6, None));
    s.push(circle(62.0, 42.0, 10.0));
    let park = Region::from_ring(&rect(34.0, 34.0, 14.0, 18.0));
    s.extend(fill_rectilinear(&park, 1.2, 45.0, None, Anchor::LOCAL));
    s.push(rect(34.0, 34.0, 14.0, 18.0));
    s
}

fn main() {
    let variants: Vec<(&str, Option<HandParams>)> = vec![
        ("precise", None),
        ("hand (default)", Some(HandParams::default())),
        ("hand sketch()", Some(HandParams::sketch())),
    ];
    let w = PAD * 2.0 + variants.len() as f64 * CELL + (variants.len() - 1) as f64 * GAP;
    let h = PAD * 2.0 + CELL + 8.0;
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w}mm' height='{h}mm' viewBox='0 0 {w} {h}'>\n\
         <rect width='{w}' height='{h}' fill='#faf8f5'/>\n"
    );
    let base = scene();
    for (i, (name, params)) in variants.iter().enumerate() {
        let ox = PAD + i as f64 * (CELL + GAP);
        let oy = PAD + 8.0;
        svg.push_str(&format!(
            "<text x='{:.1}' y='{:.1}' font-family='monospace' font-size='4' fill='#333'>{name}</text>\n",
            ox,
            PAD + 3.0
        ));
        let moved: Strokes = base.iter().map(|l| l.iter().map(|p| (p.0 + ox, p.1 + oy)).collect()).collect();
        let lines = match params {
            Some(p) => hand(&moved, p),
            None => moved,
        };
        svg.push_str("<g fill='none' stroke='#1b1b1b' stroke-width='0.25' stroke-linecap='round' stroke-linejoin='round'>\n");
        for l in &lines {
            if l.len() < 2 {
                continue;
            }
            let d: Vec<String> = l.iter().map(|p| format!("{:.3},{:.3}", p.0, p.1)).collect();
            svg.push_str(&format!("<polyline points='{}'/>\n", d.join(" ")));
        }
        svg.push_str("</g>\n");
    }
    svg.push_str("</svg>\n");
    let path = "examples/gallery/hand-demo.svg";
    std::fs::write(path, svg).expect("write svg");
    println!("wrote {path}");

    // Doodled trees, with the steady hand on top.
    let park = Region::from_ring(&rect(5.0, 5.0, 110.0, 50.0));
    let trees = hand(&penfill::doodle::fill_doodle_trees(&park, 4.0, 9), &HandParams { overshoot: 0.0, close: 0.0, tremor: 0.06, ..HandParams::default() });
    let mut svg = String::from("<svg xmlns='http://www.w3.org/2000/svg' width='120mm' height='60mm' viewBox='0 0 120 60'>\n<rect width='120' height='60' fill='#faf8f5'/>\n<g fill='none' stroke='#1b1b1b' stroke-width='0.2' stroke-linecap='round' stroke-linejoin='round'>\n");
    for l in &trees {
        let d: Vec<String> = l.iter().map(|p| format!("{:.3},{:.3}", p.0, p.1)).collect();
        svg.push_str(&format!("<polyline points='{}'/>\n", d.join(" ")));
    }
    svg.push_str("</g>\n</svg>\n");
    std::fs::write("examples/gallery/doodle-trees.svg", svg).expect("write svg");
    println!("wrote examples/gallery/doodle-trees.svg");

    // Curling waves: precise, then by hand.
    let sea = Region::from_ring(&rect(0.0, 0.0, 100.0, 40.0));
    let precise = if std::env::var("FLOW").is_ok() {
        penfill::waves_flow::fill_flow_waves(&sea, 1.6, 0.0, 4)
    } else if std::env::var("SCALLOP").is_ok() {
        penfill::waves_scallop::fill_scallop_waves(&sea, 6.0, 0.0)
    } else {
        penfill::waves_curl::fill_curl_waves(&sea, 6.0, 0.0)
    };
    let by_hand = hand(&precise, &HandParams { wobble: 0.5, bow: 0.02, overshoot: 0.15, corners: 0.0, reach: 0.0, ..HandParams::sketch() });
    let mut svg = String::from("<svg xmlns='http://www.w3.org/2000/svg' width='210mm' height='50mm' viewBox='0 0 210 50'>\n<rect width='210' height='50' fill='#faf8f5'/>\n<g fill='none' stroke='#1b1b1b' stroke-width='0.3' stroke-linecap='round' stroke-linejoin='round'>\n");
    for (dx, lines) in [(5.0, &precise), (110.0, &by_hand)] {
        for l in lines.iter() {
            let d: Vec<String> = l.iter().map(|p| format!("{:.3},{:.3}", p.0 + dx, p.1 + 5.0)).collect();
            svg.push_str(&format!("<polyline points='{}'/>\n", d.join(" ")));
        }
    }
    svg.push_str("</g>\n</svg>\n");
    std::fs::write("examples/gallery/curl-waves.svg", svg).expect("write svg");
    println!("wrote examples/gallery/curl-waves.svg");
}
