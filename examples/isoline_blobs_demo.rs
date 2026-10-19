//! Quick preview: `cargo run --release --example isoline_blobs_demo`
//! Writes `examples/gallery/isoline-*.svg` and `examples/gallery/forest-canopy-showcase.svg`.

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
        light: Some(Light { from_deg: -45.0, strength: 0.85, ..Light::default() }),
        ..Default::default()
    };
    let out = fill_isoline_stack_with_coverage(&park, &lit);
    write("examples/gallery/isoline-stack-lit.svg", &park, &out.strokes);

    write_forest_showcase("examples/gallery/forest-canopy-showcase.svg");
    write_density_and_light_variations("examples/gallery/forest-variations-density-light.svg");
    write_species_and_mask_variations("examples/gallery/forest-variations-species-mask.svg");
}

/// Sheet 1: 2x3 grid comparing:
/// - Row 1: Tree intensity / coverage (10% vs 45% vs 100% trees)
/// - Row 2: Luminosity / directional sunlight strength (0.25 vs 0.65 vs 0.95)
fn write_density_and_light_variations(path: &str) {
    let w = 310.0;
    let h = 212.0;
    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}px' height='{}px' viewBox='0 0 {w} {h}'>\n",
        (w * 4.5) as usize,
        (h * 4.5) as usize,
    );
    s.push_str(&format!("<rect x='0' y='0' width='{w}' height='{h}' fill='#f6f2e8'/>\n"));
    s.push_str("<text x='10' y='9' font-family='Georgia, serif' font-size='4.4' font-weight='bold' fill='#2a251f'>Variazioni di Intensità (10% – 100% alberi) e Luminosità Solare</text>\n");

    let rect_panel = |x0: f64, y0: f64, pw: f64, ph: f64| -> Region {
        Region::from_ring(&[(x0, y0), (x0 + pw, y0), (x0 + pw, y0 + ph), (x0, y0 + ph)])
    };

    let pw = 92.0;
    let ph = 84.0;
    let cols = [10.0, 109.0, 208.0];
    let rows = [19.0, 118.0];

    let panels = [
        (
            cols[0],
            rows[0],
            "1A. Intensità 10% (coverage = 0.10)",
            IsolineStackParams {
                seed: 42,
                crown: 6.2,
                coverage: 0.10,
                slices: 9,
                jitter: 0.88,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            cols[1],
            rows[0],
            "1B. Intensità 45% (coverage = 0.45)",
            IsolineStackParams {
                seed: 42,
                crown: 6.2,
                coverage: 0.45,
                slices: 9,
                jitter: 0.88,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            cols[2],
            rows[0],
            "1C. Intensità 100% (coverage = 1.00)",
            IsolineStackParams {
                seed: 42,
                crown: 6.2,
                coverage: 1.00,
                slices: 9,
                jitter: 0.88,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            cols[0],
            rows[1],
            "2A. Bassa Luminosità (light = 0.25)",
            IsolineStackParams {
                seed: 19,
                crown: 6.4,
                coverage: 0.65,
                slices: 10,
                jitter: 0.88,
                edge: EdgeMode::Inside,
                light: Some(Light { from_deg: -45.0, strength: 0.25, ..Light::default() }),
                ..Default::default()
            },
        ),
        (
            cols[1],
            rows[1],
            "2B. Media Luminosità (light = 0.65)",
            IsolineStackParams {
                seed: 19,
                crown: 6.4,
                coverage: 0.65,
                slices: 10,
                jitter: 0.88,
                edge: EdgeMode::Inside,
                light: Some(Light { from_deg: -45.0, strength: 0.65, ..Light::default() }),
                ..Default::default()
            },
        ),
        (
            cols[2],
            rows[1],
            "2C. Alta Luminosità (light = 0.95)",
            IsolineStackParams {
                seed: 19,
                crown: 6.4,
                coverage: 0.65,
                slices: 10,
                jitter: 0.88,
                edge: EdgeMode::Inside,
                light: Some(Light { from_deg: -45.0, strength: 0.95, ..Light::default() }),
                ..Default::default()
            },
        ),
    ];

    for (px, py, title, params) in panels {
        let host = rect_panel(px, py, pw, ph);
        let out = fill_isoline_stack_with_coverage(&host, &params);
        s.push_str(&format!(
            "<text x='{px}' y='{}' font-family='Georgia, serif' font-size='3.1' fill='#3b352c'>{title} · {} tratti</text>\n",
            py - 2.2,
            out.strokes.len()
        ));
        s.push_str(&format!(
            "<rect x='{px}' y='{py}' width='{pw}' height='{ph}' fill='none' stroke='#c8bfb0' stroke-width='0.18' stroke-dasharray='1 1'/>\n"
        ));
        s.push_str("<g fill='none' stroke='#1f1c18' stroke-width='0.18' stroke-linecap='round' stroke-linejoin='round'>\n");
        for stroke in &out.strokes {
            s.push_str(&format!("<path d='{}'/>\n", d(stroke)));
        }
        s.push_str("</g>\n");
    }

    s.push_str("</svg>\n");
    std::fs::write(path, s).unwrap();
    println!("wrote {path}");
}

/// Sheet 2: 2x3 grid comparing:
/// - Row 1: Jittering / jaggedness variations (0.28 soft park vs 0.88 deciduous oak vs 1.65 spiky conifer)
/// - Row 2: Shape variations (`lobes`, `elongation`) + Closed coverage mask (`out.coverage`) cutting underlying terrain
fn write_species_and_mask_variations(path: &str) {
    let w = 310.0;
    let h = 212.0;
    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}px' height='{}px' viewBox='0 0 {w} {h}'>\n",
        (w * 4.5) as usize,
        (h * 4.5) as usize,
    );
    s.push_str(&format!("<rect x='0' y='0' width='{w}' height='{h}' fill='#f6f2e8'/>\n"));
    s.push_str("<text x='10' y='9' font-family='Georgia, serif' font-size='4.4' font-weight='bold' fill='#2a251f'>Variazioni di Jittering, Forma Vegetazionale e Maschera Chiusa (coverage)</text>\n");

    let rect_panel = |x0: f64, y0: f64, pw: f64, ph: f64| -> Region {
        Region::from_ring(&[(x0, y0), (x0 + pw, y0), (x0 + pw, y0 + ph), (x0, y0 + ph)])
    };

    let pw = 92.0;
    let ph = 84.0;
    let cols = [10.0, 109.0, 208.0];
    let rows = [19.0, 118.0];

    let panels = [
        (
            cols[0],
            rows[0],
            "3A. Jitter Morbido (0.48) · Chiome Tondeggianti",
            false,
            IsolineStackParams {
                seed: 27,
                crown: 6.4,
                bubble: 2.2,
                coverage: 0.62,
                slices: 9,
                jitter: 0.48,
                lobes: 0.65,
                elongation: 0.08,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            cols[1],
            rows[0],
            "3B. Medio Jitter (0.88) · Querceto / Faggio",
            false,
            IsolineStackParams {
                seed: 27,
                crown: 6.4,
                bubble: 1.8,
                coverage: 0.62,
                slices: 9,
                jitter: 0.88,
                lobes: 1.05,
                elongation: 0.15,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            cols[2],
            rows[0],
            "3C. Alto Jitter (1.65) · Conifere / Abetaia",
            false,
            IsolineStackParams {
                seed: 27,
                crown: 6.0,
                bubble: 1.15,
                coverage: 0.62,
                slices: 10,
                jitter: 1.65,
                lobes: 0.70,
                elongation: 0.10,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            cols[0],
            rows[1],
            "4A. Chiome Lobate Composte (lobes = 1.75)",
            false,
            IsolineStackParams {
                seed: 31,
                crown: 6.6,
                bubble: 1.7,
                coverage: 0.58,
                slices: 10,
                jitter: 1.05,
                lobes: 1.75,
                elongation: 0.20,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            cols[1],
            rows[1],
            "4B. Chiome Allungate (elongation = 0.65)",
            false,
            IsolineStackParams {
                seed: 31,
                crown: 6.5,
                bubble: 1.8,
                coverage: 0.58,
                slices: 9,
                jitter: 0.92,
                lobes: 1.0,
                elongation: 0.65,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            cols[2],
            rows[1],
            "4C. Maschera Chiusa (coverage) Taglia Sfondo",
            true,
            IsolineStackParams {
                seed: 31,
                crown: 6.6,
                bubble: 1.8,
                coverage: 0.55,
                slices: 10,
                jitter: 0.92,
                edge: EdgeMode::Inside,
                light: Some(Light { from_deg: -45.0, strength: 0.90, ..Light::default() }),
                ..Default::default()
            },
        ),
    ];

    for (px, py, title, show_mask_cut, params) in panels {
        let host = rect_panel(px, py, pw, ph);
        let out = fill_isoline_stack_with_coverage(&host, &params);
        s.push_str(&format!(
            "<text x='{px}' y='{}' font-family='Georgia, serif' font-size='3.05' fill='#3b352c'>{title}</text>\n",
            py - 2.2,
        ));
        s.push_str(&format!(
            "<rect x='{px}' y='{py}' width='{pw}' height='{ph}' fill='none' stroke='#c8bfb0' stroke-width='0.18' stroke-dasharray='1 1'/>\n"
        ));

        if show_mask_cut {
            // Draw diagonal terrain hatching clipped against `host.difference(&out.coverage)`
            // AND draw a subtle green tint of `out.coverage` so it is 100% clear that `out.coverage`
            // is a closed polygon surface even where outer tree perimeters are open on the sunlit side!
            s.push_str("<g fill='#e2eadc' stroke='#7c9a6d' stroke-width='0.12' stroke-dasharray='0.6 0.5'>\n");
            for ring in out.coverage.all_rings_mm() {
                s.push_str(&format!("<path d='{} Z'/>\n", d(&ring)));
            }
            s.push_str("</g>\n");

            let mut bg_lines = Vec::new();
            let mut y = py + 1.1;
            while y < py + ph {
                bg_lines.push(vec![(px, y), (px + pw, y)]);
                y += 1.25;
            }
            let bg_visible = host.difference(&out.coverage).clip_strokes(&bg_lines);
            s.push_str("<g fill='none' stroke='#9e9280' stroke-width='0.15' stroke-linecap='round'>\n");
            for l in &bg_visible {
                s.push_str(&format!("<path d='{}'/>\n", d(l)));
            }
            s.push_str("</g>\n");
        }

        s.push_str("<g fill='none' stroke='#1f1c18' stroke-width='0.18' stroke-linecap='round' stroke-linejoin='round'>\n");
        for stroke in &out.strokes {
            s.push_str(&format!("<path d='{}'/>\n", d(stroke)));
        }
        s.push_str("</g>\n");
    }

    s.push_str("</svg>\n");
    std::fs::write(path, s).unwrap();
    println!("wrote {path}");
}

/// Generates a 2x2 showcase sheet comparing sparse groves, dense canopy,
/// directional sunlight, and DrawLand `city-grain` map-scale canopy with mask occlusion.
fn write_forest_showcase(path: &str) {
    let w = 290.0;
    let h = 210.0;
    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}px' height='{}px' viewBox='0 0 {w} {h}'>\n",
        (w * 4.5) as usize,
        (h * 4.5) as usize,
    );
    s.push_str(&format!("<rect x='0' y='0' width='{w}' height='{h}' fill='#f6f2e8'/>\n"));

    let rect_panel = |x0: f64, y0: f64, pw: f64, ph: f64| -> Region {
        Region::from_ring(&[(x0, y0), (x0 + pw, y0), (x0 + pw, y0 + ph), (x0, y0 + ph)])
    };

    let panels = [
        (
            12.0,
            18.0,
            128.0,
            82.0,
            "1. Grove-to-Clearing Scattering (coverage = 0.48, jaggedness = 0.88)",
            IsolineStackParams {
                seed: 19,
                crown: 6.2,
                coverage: 0.48,
                slices: 10,
                jitter: 0.88,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            150.0,
            18.0,
            128.0,
            82.0,
            "2. High Jaggedness / Serrated Foliage (coverage = 0.58, jaggedness = 1.45)",
            IsolineStackParams {
                seed: 19,
                crown: 6.4,
                coverage: 0.58,
                slices: 10,
                jitter: 1.45,
                edge: EdgeMode::Inside,
                ..Default::default()
            },
        ),
        (
            12.0,
            114.0,
            128.0,
            82.0,
            "3. Directional Sunlight + 3D Overlap (light = -45 deg, jaggedness = 0.95)",
            IsolineStackParams {
                seed: 19,
                crown: 6.5,
                coverage: 0.65,
                slices: 11,
                jitter: 0.95,
                edge: EdgeMode::Inside,
                light: Some(Light { from_deg: -45.0, strength: 0.85, ..Light::default() }),
                ..Default::default()
            },
        ),
        (
            150.0,
            114.0,
            128.0,
            82.0,
            "4. DrawLand city-grain scale (2.4mm crown, overflow + mask over terrain)",
            IsolineStackParams {
                seed: 7,
                crown: 2.4,
                bubble: 1.0,
                coverage: 0.72,
                slices: 7,
                jitter: 0.95,
                edge: EdgeMode::Overflow(0.8),
                light: Some(Light { from_deg: -45.0, strength: 0.82, ..Light::default() }),
                ..Default::default()
            },
        ),
    ];

    for (idx, (px, py, pw, ph, title, params)) in panels.into_iter().enumerate() {
        let host = rect_panel(px, py, pw, ph);
        let out = fill_isoline_stack_with_coverage(&host, &params);

        s.push_str(&format!(
            "<text x='{px}' y='{}' font-family='Georgia, serif' font-size='3.4' fill='#3b352c'>{title} · {} strokes</text>\n",
            py - 2.5,
            out.strokes.len()
        ));
        s.push_str(&format!(
            "<rect x='{px}' y='{py}' width='{pw}' height='{ph}' fill='none' stroke='#c8bfb0' stroke-width='0.18' stroke-dasharray='1 1'/>\n"
        ));

        // On panel 4, draw background terrain hatching occluded by `out.coverage` to show mask export!
        if idx == 3 {
            let mut bg_lines = Vec::new();
            let mut y = py + 1.2;
            while y < py + ph {
                bg_lines.push(vec![(px, y), (px + pw, y)]);
                y += 1.4;
            }
            let visible_bg = out.coverage.outset(0.25).clip_strokes_mode(&bg_lines, true);
            s.push_str("<g fill='none' stroke='#b8afa0' stroke-width='0.12' stroke-linecap='round'>\n");
            for l in &visible_bg {
                s.push_str(&format!("<path d='{}'/>\n", d(l)));
            }
            s.push_str("</g>\n");
        }

        let sw = if idx == 3 { 0.14 } else { 0.18 };
        s.push_str(&format!(
            "<g fill='none' stroke='#1f1c18' stroke-width='{sw}' stroke-linecap='round' stroke-linejoin='round'>\n"
        ));
        for stroke in &out.strokes {
            s.push_str(&format!("<path d='{}'/>\n", d(stroke)));
        }
        s.push_str("</g>\n");
    }

    s.push_str("</svg>\n");
    std::fs::write(path, s).unwrap();
    println!("wrote {path}");
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
    s.push_str("</g>\n<g fill='none' stroke='#1a1a1a' stroke-width='0.18' stroke-linecap='round' stroke-linejoin='round'>\n");
    for stroke in strokes {
        s.push_str(&format!("<path d='{}'/>\n", d(stroke)));
    }
    s.push_str("</g>\n</svg>\n");
    std::fs::write(path, s).unwrap();
    println!("wrote {path} ({} strokes)", strokes.len());
}

fn d(pts: &[Pt]) -> String {
    let mut out = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        out.push_str(&format!("{}{x:.2} {y:.2}", if i == 0 { "M" } else { "L" }));
    }
    out
}
