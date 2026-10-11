//! Quick preview: `cargo run --example isoline_blobs_demo`
//! Writes `examples/gallery/isoline-blobs-*.svg` (120x120 mm square each).

use penfill::geo::Region;
use penfill::isoline_blobs::{default_params, fill_isoline_blobs_with_coverage, IsolineFieldParams};
use penfill::{FieldSite, FieldSource};

fn main() {
    let region = Region::from_ring(&[(0.0, 0.0), (120.0, 0.0), (120.0, 120.0), (0.0, 120.0)]);

    // Scattered, clearly separated clusters of varying size — the defaults.
    let scattered = default_params(16.0, 0.7, 42);
    write_preview(&region, &scattered, "examples/gallery/isoline-blobs-scattered.svg");

    // Denser: lower threshold fuses neighbours into connected masses.
    let dense = IsolineFieldParams {
        seed: 42,
        source: FieldSource::Noise,
        feature_size: 16.0,
        contour_spacing: 0.9,
        warp: 0.45,
        threshold: -0.1,
    };
    write_preview(&region, &dense, "examples/gallery/isoline-blobs-dense.svg");

    // Explicit sites (e.g. real tree positions) instead of procedural noise.
    let sites = IsolineFieldParams {
        seed: 1,
        source: FieldSource::Sites(vec![
            FieldSite { at: (20.0, 20.0), weight: 1.0, radius: 7.0 },
            FieldSite { at: (33.0, 24.0), weight: 1.0, radius: 7.0 },
            FieldSite { at: (70.0, 80.0), weight: 1.0, radius: 9.0 },
            FieldSite { at: (85.0, 70.0), weight: 1.0, radius: 7.0 },
            FieldSite { at: (95.0, 95.0), weight: 0.8, radius: 5.0 },
        ]),
        feature_size: 16.0,
        contour_spacing: 0.7,
        warp: 0.25,
        threshold: 0.2,
    };
    write_preview(&region, &sites, "examples/gallery/isoline-blobs-sites.svg");
}

fn write_preview(region: &Region, params: &IsolineFieldParams, path: &str) {
    let out = fill_isoline_blobs_with_coverage(region, params);
    let svg = render(region, &out.strokes, 120.0, 120.0);
    std::fs::write(path, svg).expect("write svg");
    eprintln!(
        "wrote {path} ({} strokes, coverage loops {})",
        out.strokes.len(),
        out.coverage.all_rings_mm().len()
    );
}

fn render(region: &Region, strokes: &penfill::Strokes, w: f64, h: f64) -> String {
    let pad = 6.0;
    let vw = w + 2.0 * pad;
    let vh = h + 2.0 * pad;
    let mut s = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w}mm' height='{h}mm' viewBox='{x0} {y0} {vw} {vh}'>\n",
        x0 = -pad,
        y0 = -pad,
    );
    s.push_str("<rect x='0' y='0' width='100%' height='100%' fill='#f4f0e6' transform='translate(-6,-6)' />\n");
    s.push_str("<g fill='none' stroke='#999' stroke-width='0.15'>\n");
    for ring in region.all_rings_mm() {
        if ring.len() < 2 {
            continue;
        }
        s.push_str(&format!("<path d='{}'/>\n", path_d(&ring, true)));
    }
    s.push_str("</g>\n");
    s.push_str("<g fill='none' stroke='#111' stroke-width='0.12' stroke-linecap='round' stroke-linejoin='round'>\n");
    for pts in strokes {
        if pts.len() < 2 {
            continue;
        }
        let closed = pts.first() == pts.last();
        s.push_str(&format!("<path d='{}'/>\n", path_d(pts, closed)));
    }
    s.push_str("</g>\n</svg>\n");
    s
}

fn path_d(pts: &[(f64, f64)], close: bool) -> String {
    let mut d = format!("M {:.3},{:.3}", pts[0].0, pts[0].1);
    for p in &pts[1..] {
        d.push_str(&format!(" L {:.3},{:.3}", p.0, p.1));
    }
    if close {
        d.push_str(" Z");
    }
    d
}
