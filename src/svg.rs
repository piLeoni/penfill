//! Bring your own artwork: read the line work out of an SVG, then use it
//! as a repeating hatch tile ([`fill_svg_tile`]) or cut it to a region as
//! it is ([`Region::clip_strokes`]).
//!
//! Only geometry is read — `path`, `line`, `polyline`, `polygon`, `rect`,
//! `circle` and `ellipse`, with nested `transform`s. Curves are flattened
//! into short segments. Fills, strokes widths, text and images are ignored:
//! a plotter only draws lines. Coordinates keep SVG's y-down convention.

use crate::anchor::Anchor;
use crate::geo::{Pt, Region, Strokes};
use crate::placement::Placement;
use crate::tile::fill_tile;
use std::str::FromStr;

/// Line work read from an SVG, in the file's user units.
#[derive(Debug, Clone)]
pub struct SvgDrawing {
    pub strokes: Strokes,
    /// `(x, y, width, height)`: the `viewBox`, or the `width`/`height`
    /// attributes, or the bounds of the drawing — the first one present.
    pub view_box: (f64, f64, f64, f64),
}

impl SvgDrawing {
    /// The drawing scaled so its view box is `width_mm` wide, with the view
    /// box's corner moved to the origin.
    pub fn fit_width(&self, width_mm: f64) -> SvgDrawing {
        let (x, y, w, h) = self.view_box;
        let k = if w > 0.0 { width_mm / w } else { 1.0 };
        SvgDrawing {
            strokes: self.strokes.iter().map(|s| s.iter().map(|p| ((p.0 - x) * k, (p.1 - y) * k)).collect()).collect(),
            view_box: (0.0, 0.0, w * k, h * k),
        }
    }
}

/// Parse the SVG text. Errors only on malformed XML; unknown elements are
/// skipped.
pub fn parse_svg(text: &str) -> Result<SvgDrawing, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let mut strokes: Strokes = Vec::new();
    walk(root, IDENTITY, &mut strokes);
    let num = |name: &str| root.attribute(name).and_then(leading_number);
    let view_box = root
        .attribute("viewBox")
        .and_then(|v| svgtypes::ViewBox::from_str(v).ok())
        .map(|v| (v.x, v.y, v.w, v.h))
        .or_else(|| Some((0.0, 0.0, num("width")?, num("height")?)))
        .unwrap_or_else(|| {
            let pts = strokes.iter().flatten();
            let minx = pts.clone().map(|p| p.0).fold(f64::MAX, f64::min);
            let miny = pts.clone().map(|p| p.1).fold(f64::MAX, f64::min);
            let maxx = pts.clone().map(|p| p.0).fold(f64::MIN, f64::max);
            let maxy = pts.map(|p| p.1).fold(f64::MIN, f64::max);
            if minx > maxx { (0.0, 0.0, 0.0, 0.0) } else { (minx, miny, maxx - minx, maxy - miny) }
        });
    Ok(SvgDrawing { strokes, view_box })
}

/// The SVG repeated as a hatch: its view box becomes a tile `width_mm`
/// wide (height in proportion), rows shifted by `row_shift` tiles, the
/// whole pattern turned by `angle`, cut by the region.
pub fn fill_svg_tile(
    region: &Region,
    svg: &SvgDrawing,
    width_mm: f64,
    angle: f64,
    row_shift: f64,
    anchor: Anchor,
) -> Strokes {
    let fitted = svg.fit_width(width_mm);
    let (_, _, w, h) = fitted.view_box;
    fill_tile(region, &fitted.strokes, w, h, angle, row_shift, anchor)
}

/// The SVG placed like a CSS background — sized, positioned, repeated or
/// not ([`Placement`]) against the region's bounds — and cut by the region.
pub fn fill_svg_placed(region: &Region, svg: &SvgDrawing, placement: &Placement) -> Strokes {
    let (vx, vy, vw, vh) = svg.view_box;
    if region.is_empty() || vw <= 0.0 || vh <= 0.0 {
        return vec![];
    }
    let mut placed: Strokes = Vec::new();
    for t in placement.tiles((vw, vh), region.bounds_mm()) {
        let (kx, ky) = (t.w / vw, t.h / vh);
        placed.extend(svg.strokes.iter().map(|s| s.iter().map(|p| (t.x + (p.0 - vx) * kx, t.y + (p.1 - vy) * ky)).collect()));
    }
    region.clip_strokes(&placed)
}

/// Strokes as an SVG document sized in millimetres, for a quick look or
/// to hand to plotter software. `outline`, when given, is drawn in grey
/// under the strokes. Coordinates are written as they are (SVG is y-down).
pub fn to_svg(strokes: &Strokes, outline: Option<&Region>, stroke_width: f64) -> String {
    let rings = outline.map(|r| r.all_rings_mm()).unwrap_or_default();
    let pts = strokes.iter().chain(rings.iter()).flatten();
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        (minx, miny, maxx, maxy) = (minx.min(p.0), miny.min(p.1), maxx.max(p.0), maxy.max(p.1));
    }
    if minx > maxx {
        (minx, miny, maxx, maxy) = (0.0, 0.0, 1.0, 1.0);
    }
    let pad = stroke_width.max(0.0) * 2.0 + 1.0;
    let (x, y, w, h) = (minx - pad, miny - pad, maxx - minx + 2.0 * pad, maxy - miny + 2.0 * pad);
    let path = |s: &Vec<Pt>, close: bool| {
        let d: Vec<String> = s.iter().map(|p| format!("{:.3},{:.3}", p.0, p.1)).collect();
        format!("<path d='M{}{}'/>\n", d.join("L"), if close { "Z" } else { "" })
    };
    let mut out = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w:.3}mm' height='{h:.3}mm' viewBox='{x:.3} {y:.3} {w:.3} {h:.3}'>\n\
         <g fill='none' stroke-linecap='round' stroke-linejoin='round' stroke-width='{stroke_width}'>\n"
    );
    if !rings.is_empty() {
        out.push_str("<g stroke='#b7aea3'>\n");
        rings.iter().for_each(|r| out.push_str(&path(r, true)));
        out.push_str("</g>\n");
    }
    out.push_str("<g stroke='#2a2622'>\n");
    strokes.iter().filter(|s| !s.is_empty()).for_each(|s| out.push_str(&path(s, false)));
    out.push_str("</g>\n</g>\n</svg>\n");
    out
}

/// 2×3 affine matrix `[a, b, c, d, e, f]` as in SVG: x' = a·x + c·y + e.
type Affine = [f64; 6];
const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn compose(m: Affine, n: Affine) -> Affine {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

fn apply(m: Affine, p: Pt) -> Pt {
    (m[0] * p.0 + m[2] * p.1 + m[4], m[1] * p.0 + m[3] * p.1 + m[5])
}

fn walk(node: roxmltree::Node, parent: Affine, out: &mut Strokes) {
    let tag = node.tag_name().name();
    if matches!(tag, "defs" | "clipPath" | "mask" | "symbol" | "marker" | "pattern" | "metadata" | "title" | "desc" | "style") {
        return;
    }
    let m = match node.attribute("transform").and_then(|t| svgtypes::Transform::from_str(t).ok()) {
        Some(t) => compose(parent, [t.a, t.b, t.c, t.d, t.e, t.f]),
        None => parent,
    };
    let num = |name: &str| node.attribute(name).and_then(leading_number).unwrap_or(0.0);
    let mut local: Strokes = Vec::new();
    match tag {
        "path" => {
            if let Some(d) = node.attribute("d") {
                local = path_strokes(d);
            }
        }
        "line" => local.push(vec![(num("x1"), num("y1")), (num("x2"), num("y2"))]),
        "polyline" | "polygon" => {
            if let Some(pts) = node.attribute("points") {
                let mut line: Vec<Pt> = svgtypes::PointsParser::from(pts).collect();
                if tag == "polygon" && line.len() > 2 {
                    line.push(line[0]);
                }
                local.push(line);
            }
        }
        "rect" => {
            let (x, y, w, h) = (num("x"), num("y"), num("width"), num("height"));
            local.push(vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]);
        }
        "circle" => local.push(ellipse(num("cx"), num("cy"), num("r"), num("r"))),
        "ellipse" => local.push(ellipse(num("cx"), num("cy"), num("rx"), num("ry"))),
        _ => {}
    }
    out.extend(local.into_iter().filter(|s| s.len() >= 2).map(|s| s.into_iter().map(|p| apply(m, p)).collect()));
    for child in node.children().filter(|c| c.is_element()) {
        walk(child, m, out);
    }
}

fn leading_number(s: &str) -> Option<f64> {
    let end = s.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))).unwrap_or(s.len());
    s[..end].parse().ok()
}

fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Vec<Pt> {
    const N: usize = 48;
    (0..=N)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / N as f64;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

/// Path data to polylines; every `M` starts a new one.
fn path_strokes(d: &str) -> Strokes {
    use svgtypes::SimplePathSegment as S;
    let mut out: Strokes = Vec::new();
    let mut cur: Vec<Pt> = Vec::new();
    let mut start = (0.0, 0.0);
    for seg in svgtypes::SimplifyingPathParser::from(d).flatten() {
        let last = cur.last().copied().unwrap_or(start);
        match seg {
            S::MoveTo { x, y } => {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                start = (x, y);
                cur = vec![start];
            }
            S::LineTo { x, y } => cur.push((x, y)),
            S::CurveTo { x1, y1, x2, y2, x, y } => {
                let (p1, p2, p3) = ((x1, y1), (x2, y2), (x, y));
                let n = steps(&[last, p1, p2, p3]);
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let u = 1.0 - t;
                    let (a, b, c, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    cur.push((
                        a * last.0 + b * p1.0 + c * p2.0 + e * p3.0,
                        a * last.1 + b * p1.1 + c * p2.1 + e * p3.1,
                    ));
                }
            }
            S::Quadratic { x1, y1, x, y } => {
                let (p1, p2) = ((x1, y1), (x, y));
                let n = steps(&[last, p1, p2]);
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * last.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
                        u * u * last.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
                    ));
                }
            }
            S::ClosePath => {
                cur.push(start);
                out.push(std::mem::take(&mut cur));
                cur = vec![start];
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Segments for a curve: more for longer control polygons, 4 to 64.
fn steps(ctrl: &[Pt]) -> usize {
    let len: f64 = ctrl.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
    // Relative to the curve's own size, so any unit system works.
    let chord = (ctrl[ctrl.len() - 1].0 - ctrl[0].0).hypot(ctrl[ctrl.len() - 1].1 - ctrl[0].1);
    let bend = if chord > 0.0 { len / chord } else { 2.0 };
    ((8.0 * bend).ceil() as usize).clamp(4, 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
        <g transform="translate(5 5)">
          <path d="M -2 0 L 2 0 M 0 -2 L 0 2"/>
          <circle cx="0" cy="0" r="1"/>
        </g>
        <rect x="0" y="0" width="10" height="10" fill="none"/>
        <path d="M0 10 C 3 7, 7 7, 10 10"/>
      </svg>"#;

    #[test]
    fn reads_shapes_transforms_and_curves() {
        let s = parse_svg(SVG).unwrap();
        assert_eq!(s.view_box, (0.0, 0.0, 10.0, 10.0));
        assert_eq!(s.strokes.len(), 5, "{:?}", s.strokes.iter().map(|x| x.len()).collect::<Vec<_>>());
        assert_eq!(s.strokes[0], vec![(3.0, 5.0), (7.0, 5.0)], "translate applied");
        let curve = s.strokes.last().unwrap();
        assert!(curve.len() > 4, "curve flattened into {} points", curve.len());
        let dip = curve.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        assert!((dip - 7.75).abs() < 0.05, "curve dips to {dip}");
    }

    #[test]
    fn placed_like_a_css_background() {
        let dot = parse_svg("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10'><rect x='4' y='4' width='2' height='2'/></svg>").unwrap();
        let sq = Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)]);
        let count = |size: &str, pos: &str, repeat: &str| fill_svg_placed(&sq, &dot, &Placement::parse(size, pos, repeat).unwrap()).len();
        assert_eq!(count("contain", "center", "no-repeat"), 1);
        assert_eq!(count("10", "top left", "repeat"), 16);
        assert_eq!(count("10", "top left", "repeat-y"), 4);
        let one = fill_svg_placed(&sq, &dot, &Placement::parse("20", "right bottom", "no-repeat").unwrap());
        assert!(one[0].iter().all(|p| p.0 >= 28.0 && p.1 >= 28.0), "square sits in the far corner: {:?}", one[0]);
    }

    #[test]
    fn written_svg_reads_back() {
        let strokes = vec![vec![(1.0, 2.0), (5.0, 2.0)], vec![(0.0, 0.0), (3.0, 4.0), (6.0, 0.0)]];
        let square = Region::from_ring(&[(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)]);
        let text = to_svg(&strokes, Some(&square), 0.3);
        let back = parse_svg(&text).unwrap();
        assert_eq!(back.strokes.len(), 3, "two strokes and the outline");
        assert!(back.strokes.iter().flatten().any(|p| (p.0 - 3.0).abs() < 1e-3 && (p.1 - 4.0).abs() < 1e-3));
    }

    #[test]
    fn svg_tile_hatches_a_region() {
        let s = parse_svg(SVG).unwrap();
        let sq = Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]);
        let out = fill_svg_tile(&sq, &s, 6.0, 15.0, 0.5, Anchor::LOCAL);
        assert!(out.len() > 60, "only {} strokes", out.len());
        for p in out.iter().flatten() {
            assert!(sq.covers_point(*p), "{p:?} outside");
        }
    }
}
