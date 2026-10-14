//! Preview sheet for the 6 new textures/nature patterns, both unfrayed and frayed.
//! Run: `cargo run --release --example six_patterns_demo`

use penfill::geo::{Pt, Region, Strokes};
use penfill::nature::{fill_tufts_frayed, fill_twigs_frayed, fill_vines_frayed};
use penfill::textures::{fill_cell_net_frayed, fill_eddies_frayed, fill_ripples_frayed, fill_swells_frayed};
use penfill::Anchor;

fn main() {
    let patterns: Vec<(
        &str,
        Box<dyn Fn(&Region, f64) -> Strokes>,
    )> = vec![
        (
            "cell-net",
            Box::new(|r, fray| fill_cell_net_frayed(r, 4.2, 7, fray)),
        ),
        (
            "tufts",
            Box::new(|r, fray| fill_tufts_frayed(r, 2.3, 0.0, 7, fray)),
        ),
        (
            "eddies",
            Box::new(|r, fray| fill_eddies_frayed(r, 1.15, 0.0, 7, Anchor::LOCAL, fray)),
        ),
        (
            "twigs",
            Box::new(|r, fray| fill_twigs_frayed(r, 3.8, 7, fray)),
        ),
        (
            "ripples",
            Box::new(|r, fray| fill_ripples_frayed(r, 1.1, 0.0, 7, Anchor::LOCAL, fray)),
        ),
        (
            "vines",
            Box::new(|r, fray| fill_vines_frayed(r, 3.2, 90.0, 7, Anchor::LOCAL, fray)),
        ),
        (
            "swells",
            Box::new(|r, fray| fill_swells_frayed(r, 1.05, 0.0, 7, Anchor::LOCAL, fray)),
        ),
    ];

    let cell_w = 56.0;
    let cell_h = 56.0;
    let gap = 10.0;
    let pad = 12.0;
    let cols = 3; // col 0: square unfrayed, col 1: organic blob frayed (with faint boundary), col 2: organic blob frayed (no boundary)
    let rows = patterns.len();

    let total_w = pad * 2.0 + cols as f64 * cell_w + (cols - 1) as f64 * gap + 32.0;
    let total_h = pad * 2.0 + rows as f64 * cell_h + (rows - 1) as f64 * gap + 14.0;

    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{total_w}mm' height='{total_h}mm' viewBox='0 0 {total_w} {total_h}'>\n"
    );
    svg.push_str(&format!(
        "<rect x='0' y='0' width='{total_w}' height='{total_h}' fill='#faf8f5'/>\n"
    ));

    // Column headers
    let headers = ["Unfrayed (square)", "Frayed edge (guide)", "Frayed edge (ink only)"];
    for (c, h) in headers.iter().enumerate() {
        let cx = pad + 32.0 + c as f64 * (cell_w + gap) + cell_w * 0.5;
        svg.push_str(&format!(
            "<text x='{cx:.1}' y='10' font-family='sans-serif' font-size='3.8' fill='#555' text-anchor='middle'>{h}</text>\n"
        ));
    }

    for (row_idx, (name, gen)) in patterns.iter().enumerate() {
        let y0 = pad + 10.0 + row_idx as f64 * (cell_h + gap);
        let x_label = pad + 28.0;
        let cy_label = y0 + cell_h * 0.5;
        svg.push_str(&format!(
            "<text x='{x_label:.1}' y='{cy_label:.1}' font-family='monospace' font-size='4.2' font-weight='bold' fill='#222' text-anchor='end' dominant-baseline='middle'>{name}</text>\n"
        ));

        // Col 0: Square unfrayed
        let x0 = pad + 32.0;
        let sq = Region::from_ring(&[
            (x0, y0),
            (x0 + cell_w, y0),
            (x0 + cell_w, y0 + cell_h),
            (x0, y0 + cell_h),
        ]);
        let strokes_unfrayed = gen(&sq, 0.0);
        svg.push_str(&format!(
            "<rect x='{x0:.2}' y='{y0:.2}' width='{cell_w:.2}' height='{cell_h:.2}' fill='#ffffff' stroke='#bbb' stroke-width='0.2'/>\n"
        ));
        draw_strokes(&mut svg, &strokes_unfrayed, "#222", 0.22);

        // Col 1 & 2: Organic pond/island shape with fray = 7.5mm
        for col in 1..=2 {
            let bx = pad + 32.0 + col as f64 * (cell_w + gap);
            let blob = organic_blob(bx + cell_w * 0.5, y0 + cell_h * 0.5, cell_w * 0.46, row_idx as u64);
            if col == 1 {
                for ring in blob.all_rings_mm() {
                    svg.push_str(&format!(
                        "<path d='{} Z' fill='#ffffff' stroke='#d4c5b9' stroke-width='0.25' stroke-dasharray='1.0,0.8'/>\n",
                        d(&ring)
                    ));
                }
            }
            let strokes_frayed = gen(&blob, 8.5);
            draw_strokes(&mut svg, &strokes_frayed, "#222", 0.22);
        }
    }

    svg.push_str("</svg>\n");
    let out_path = "/tmp/six_patterns_demo.svg";
    std::fs::write(out_path, svg).expect("write svg");
    eprintln!("wrote {out_path}");
}

fn organic_blob(cx: f64, cy: f64, r: f64, seed: u64) -> Region {
    let n = 72;
    let mut ring = Vec::with_capacity(n);
    let p = seed as f64 * 1.1;
    for k in 0..n {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        let w = 1.0
            + 0.12 * (2.0 * a + p).sin()
            + 0.08 * (3.0 * a - p * 0.7).cos()
            + 0.05 * (5.0 * a + 0.4).sin();
        ring.push((cx + r * w * a.cos(), cy + r * w * a.sin()));
    }
    Region::from_ring(&ring)
}

fn draw_strokes(svg: &mut String, strokes: &Strokes, color: &str, width: f64) {
    svg.push_str(&format!(
        "<g fill='none' stroke='{color}' stroke-width='{width}' stroke-linecap='round' stroke-linejoin='round'>\n"
    ));
    for pts in strokes.iter().filter(|p| p.len() >= 2) {
        svg.push_str(&format!("<path d='{}'/>\n", d(pts)));
    }
    svg.push_str("</g>\n");
}

fn d(pts: &[Pt]) -> String {
    let mut out = format!("M {:.3},{:.3}", pts[0].0, pts[0].1);
    for p in &pts[1..] {
        out.push_str(&format!(" L {:.3},{:.3}", p.0, p.1));
    }
    out
}
