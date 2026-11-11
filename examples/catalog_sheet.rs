//! A contact sheet of catalogue patterns, for looking at them.
//!
//! ```sh
//! # Every material, every generator at its defaults:
//! cargo run --release --example catalog_sheet -- out.svg
//! # Chosen cells: a pattern name, then key=value options (numbers, a,b lists,
//! # true/false, words). `|` starts a new cell.
//! cargo run --release --example catalog_sheet -- out.svg hatch cross=2 '|' hatch dash=0.8,0.4 dash_jitter=0.6
//! # Options for the whole sheet: --cell=30 (mm), --cols=6, --pen=0.25,
//! # --shape=square|blob (a lobed island with a lake), --global (anchor),
//! # --json (print the catalogue description instead).
//! ```
//!
//! Each cell is labelled with what it draws; open the SVG, or rasterize it
//! (`rsvg-convert -w 1600 -b white out.svg -o out.png`).

use penfill::catalog::{self, Spec, Value};
use penfill::{Anchor, Region};

struct Cell {
    name: String,
    spec: Spec,
    label: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--json") {
        print!("{}", catalog::describe());
        return;
    }
    let mut out = None;
    let mut size = 30.0;
    let mut cols = 6usize;
    let mut pen = 0.25;
    let mut blob = false;
    let mut global = false;
    let mut words = Vec::new();
    for a in &args {
        if let Some(v) = a.strip_prefix("--cell=") {
            size = v.parse().expect("--cell=<mm>");
        } else if let Some(v) = a.strip_prefix("--cols=") {
            cols = v.parse().expect("--cols=<n>");
        } else if let Some(v) = a.strip_prefix("--pen=") {
            pen = v.parse().expect("--pen=<mm>");
        } else if a == "--shape=blob" {
            blob = true;
        } else if a == "--global" {
            global = true;
        } else if out.is_none() {
            out = Some(a.clone());
        } else {
            words.push(a.clone());
        }
    }
    let out = out.unwrap_or_else(|| "catalog.svg".into());

    let mut cells: Vec<Cell> = Vec::new();
    if words.is_empty() {
        for m in catalog::MATERIALS {
            cells.push(Cell { name: m.name.into(), spec: Spec::default(), label: format!("{} · {}", m.name, m.group.id()) });
        }
        for g in catalog::generators() {
            cells.push(Cell { name: g.name.into(), spec: Spec::default(), label: format!("{} (generator)", g.name) });
        }
    } else {
        for group in words.split(|w| w == "|") {
            let Some((name, opts)) = group.split_first() else { continue };
            let mut spec = Spec::default();
            for kv in opts {
                let (k, v) = kv.split_once('=').unwrap_or_else(|| panic!("option {kv:?} is not key=value"));
                match k {
                    "spacing" => spec.spacing = Some(v.parse().expect("spacing")),
                    "angle" => spec.angle = Some(v.parse().expect("angle")),
                    "seed" => spec.seed = v.parse().expect("seed"),
                    "fray" => spec.fray = v.parse().expect("fray"),
                    "inset" => spec.inset = Some(v.parse().expect("inset")),
                    "edge" => {
                        spec.edge = Some(match v {
                            "inside" => penfill::pattern::EdgeMode::Inside,
                            "clip" => penfill::pattern::EdgeMode::Clip,
                            m => penfill::pattern::EdgeMode::Overflow(m.parse().expect("edge: inside, clip or a length")),
                        })
                    }
                    _ => spec.options.set(k, parse(v)),
                }
            }
            cells.push(Cell { name: name.clone(), spec, label: group.join(" ") });
        }
    }

    let gap = 6.0;
    let label_h = 5.0;
    let rows = cells.len().div_ceil(cols);
    let w = cols as f64 * (size + gap) + gap;
    let h = rows as f64 * (size + gap + label_h) + gap;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n\
         <style>path{{fill:none;stroke:#000;stroke-width:{pen};stroke-linecap:round;stroke-linejoin:round}}\
         text{{font:2.2px sans-serif;fill:#333}} .err{{fill:#c00}} .edge{{stroke:#bbb;stroke-width:0.1}}</style>\n"
    );
    for (i, cell) in cells.iter().enumerate() {
        let (cx, cy) = ((i % cols) as f64, (i / cols) as f64);
        let x = gap + cx * (size + gap);
        let y = gap + cy * (size + gap + label_h);
        let region = if blob { island(x, y, size) } else { square(x, y, size) };
        let mut spec = cell.spec.clone();
        if global {
            spec.anchor = Anchor::GLOBAL;
        }
        let started = std::time::Instant::now();
        let result = catalog::fill(&cell.name, &region, &spec);
        let ms = started.elapsed().as_millis();
        for ring in region.all_rings_mm() {
            svg += &format!("<path class=\"edge\" d=\"{}Z\"/>\n", path(&ring));
        }
        match result {
            Ok(o) => {
                let d: String = o.strokes.iter().filter(|s| s.len() >= 2).map(|s| path(s)).collect();
                svg += &format!("<path d=\"{d}\"/>\n");
                svg += &format!(
                    "<text x=\"{x}\" y=\"{}\">{} · {} strokes · {ms} ms</text>\n",
                    y + size + 3.2,
                    escape(&cell.label),
                    o.strokes.len()
                );
            }
            Err(e) => {
                svg += &format!("<text class=\"err\" x=\"{x}\" y=\"{}\">{}</text>\n", y + size + 3.2, escape(&format!("{}: {e}", cell.label)));
            }
        }
    }
    svg += "</svg>\n";
    std::fs::write(&out, svg).expect("write the sheet");
    eprintln!("{} cells → {out}", cells.len());
}

fn parse(v: &str) -> Value {
    if v == "true" || v == "false" {
        return Value::Flag(v == "true");
    }
    if let Ok(n) = v.parse::<f64>() {
        return Value::Num(n);
    }
    if v.contains(',') {
        if let Ok(ns) = v.split(',').map(|p| p.parse::<f64>()).collect::<Result<Vec<_>, _>>() {
            return Value::Nums(ns);
        }
    }
    Value::Text(v.to_string())
}

fn square(x: f64, y: f64, s: f64) -> Region {
    Region::from_ring(&[(x, y), (x + s, y), (x + s, y + s), (x, y + s)])
}

/// A lobed island with a lake: corners, concave bays and a hole.
fn island(x: f64, y: f64, s: f64) -> Region {
    let c = (x + s / 2.0, y + s / 2.0);
    let ring = |r: f64, wob: f64, n: usize| -> Vec<(f64, f64)> {
        (0..n)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / n as f64;
                let rr = r * (1.0 + wob * (3.0 * a).sin() + wob * 0.5 * (5.0 * a + 1.0).cos());
                (c.0 + rr * a.cos(), c.1 + rr * a.sin())
            })
            .collect()
    };
    Region::from_polygons(&[vec![ring(s * 0.46, 0.08, 96), ring(s * 0.12, 0.15, 48)]])
}

fn path(pts: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (k, (x, y)) in pts.iter().enumerate() {
        d += &format!("{}{:.2} {:.2}", if k == 0 { "M" } else { "L" }, x, y);
    }
    d
}

fn escape(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
