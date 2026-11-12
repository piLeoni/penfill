//! Gallery sheets and README illustrations for `penfill`.
//!
//! ```sh
//! cargo run --release --example gallery
//! ```
//!
//! Every pattern picture comes from the catalogue itself, so the pictures
//! cannot drift from what the library draws: the materials by group
//! (`catalog::MATERIALS`), the generators (`catalog::generators()`) and the
//! symbol library (`catalog::marks::NAMES`), each drawn with `catalog::fill`.
//! A name that stops existing fails the run instead of leaving a stale image.
//!
//! Writes:
//! - `examples/gallery/gallery.svg`: the whole catalogue by group, every
//!   material with its description, then every generator and every symbol;
//! - `examples/gallery/{fray,edges-and-masks,cad,custom,dither,placement}.svg`:
//!   feature sheets, rows of variations across shapes;
//! - `docs/img/*.svg`: the strips in `README.md`, one per material group
//!   (`water`, `vegetation`, `ground`, `rock`, `built`, `sky`, `plain`), the
//!   generators, the options, the symbols, and the feature strips (`hero`,
//!   `tour`, `shapes`, `fray`, `edges-and-masks`, `cad`, `dither`,
//!   `placement`, `custom`).

use std::f64::consts::{PI, TAU};

use penfill::catalog::{self, marks, Group, Spec, Value};
use penfill::geo::{polyline_length, Pt, Region, Strokes};
use penfill::{
    dither::{fill_dither, Dither, DitherOptions, Greyscale, Marks},
    field::{fill_isolines, fill_profile, levels_every},
    pat::BUILTIN as PAT_BUILTIN,
    placement::Placement,
    svg::{fill_svg_placed, fill_svg_tile, parse_svg},
    Anchor, EdgeMode,
};

const SEED: u64 = 7;
const FRAY_MM: f64 = 8.5;

/// Warm paper, warm-gray wall, deep ink fill, muted label ink.
const PAPER: &str = "#faf8f4";
const WALL: &str = "#c9c0b5";
const GUIDE: &str = "#c7b8a8";
const INK: &str = "#231f1c";
const BG_INK: &str = "#8c8278";
const MASK_FILL: &str = "#e8dfd3";
const MASK_STROKE: &str = "#a38f78";
const LABEL: &str = "#5c554e";
const HEADING: &str = "#2a2520";

/// Muted inks for the pen-down picture (`hero.svg`).
const PENS: [&str; 5] = ["#231f1c", "#b0573a", "#3f6b8a", "#6f8a3a", "#8a4f7a"];

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

fn circle_ring(cx: f64, cy: f64, r: f64, n: usize) -> Vec<Pt> {
    (0..n)
        .map(|i| {
            let a = TAU * (i as f64) / (n as f64);
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// A rounded square `w × h` with corner radius `r`, centred at `(cx, cy)`.
fn rounded_rect(cx: f64, cy: f64, w: f64, h: f64, r: f64) -> Region {
    let r = r.min(w * 0.45).min(h * 0.45);
    let (hw, hh) = (w * 0.5 - r, h * 0.5 - r);
    let corners = [(cx + hw, cy + hh, 0.0), (cx - hw, cy + hh, PI * 0.5), (cx - hw, cy - hh, PI), (cx + hw, cy - hh, PI * 1.5)];
    let mut ring = Vec::with_capacity(64);
    for &(ox, oy, a0) in &corners {
        for k in 0..16 {
            let a = a0 + (PI * 0.5) * (k as f64 / 15.0);
            ring.push((ox + r * a.cos(), oy + r * a.sin()));
        }
    }
    Region::from_ring(&ring)
}

/// A card `side` mm across: the shape every catalogue swatch is drawn in.
fn card(side: f64) -> Region {
    rounded_rect(side * 0.5, side * 0.5, side, side, side * 0.08)
}

/// A smooth organic contour (gentle low-frequency harmonics).
fn organic_island(cx: f64, cy: f64, r: f64, phase: f64) -> Region {
    let n = 96;
    let ring: Vec<Pt> = (0..n)
        .map(|k| {
            let a = TAU * (k as f64) / (n as f64);
            let w = 1.0 + 0.11 * (2.0 * a + phase).sin() + 0.07 * (3.0 * a - phase * 0.7).cos() + 0.04 * (5.0 * a + 0.4).sin();
            (cx + r * w * a.cos(), cy + r * w * a.sin())
        })
        .collect();
    Region::from_ring(&ring)
}

/// An organic island with a pond.
fn island_with_pond(cx: f64, cy: f64, r: f64) -> Region {
    let outer: Vec<Pt> = (0..96)
        .map(|k| {
            let a = TAU * (k as f64) / 96.0;
            let w = 1.0 + 0.12 * (2.0 * a + 0.4).sin() + 0.07 * (3.0 * a - 0.5).cos();
            (cx + r * w * a.cos(), cy + r * w * a.sin())
        })
        .collect();
    let hole: Vec<Pt> = (0..64)
        .map(|k| {
            let a = TAU * (k as f64) / 64.0;
            let w = 1.0 + 0.10 * (2.0 * a - 0.3).cos();
            (cx + r * 0.16 + r * 0.30 * w * a.cos(), cy - r * 0.08 + r * 0.28 * w * a.sin())
        })
        .collect();
    Region::from_polygons(&[vec![outer, hole]])
}

fn five_star(cx: f64, cy: f64, r_outer: f64, r_inner: f64) -> Region {
    let pts: Vec<Pt> = (0..10)
        .map(|i| {
            let a = -PI * 0.5 + PI * (i as f64) / 5.0;
            let r = if i % 2 == 0 { r_outer } else { r_inner };
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    Region::from_ring(&pts)
}

/// The 52 × 52 mm test shapes of the feature sheets.
struct Shapes {
    square: Region,
    card: Region,
    ring: Region,
    island: Region,
    pond_island: Region,
    star: Region,
    archipelago: Region,
}

fn shapes() -> Shapes {
    let archipelago = {
        let main = organic_island(18.0, 27.0, 16.5, 0.3).all_rings_mm().into_iter().next().unwrap();
        let hole = circle_ring(18.0, 27.0, 5.2, 48);
        let islet = organic_island(42.0, 23.0, 10.0, 1.7).all_rings_mm().into_iter().next().unwrap();
        Region::from_polygons(&[vec![main, hole], vec![islet]])
    };
    Shapes {
        square: Region::from_ring(&[(0.0, 0.0), (52.0, 0.0), (52.0, 52.0), (0.0, 52.0)]),
        card: card(52.0),
        ring: Region::from_polygons(&[vec![circle_ring(26.0, 26.0, 26.0, 96), circle_ring(26.0, 26.0, 9.0, 64)]]),
        island: organic_island(26.0, 26.0, 25.0, 0.6),
        pond_island: island_with_pond(26.0, 26.0, 25.0),
        star: five_star(26.0, 26.0, 27.0, 13.5),
        archipelago,
    }
}

// ---------------------------------------------------------------------------
// Drawing from the catalogue
// ---------------------------------------------------------------------------

/// The strokes of catalogue pattern `name`; a name or option the catalogue
/// no longer knows stops the run with the catalogue's own message.
fn draw(name: &str, region: &Region, spec: Spec) -> Strokes {
    catalog::fill(name, region, &spec).unwrap_or_else(|e| panic!("gallery: {e}")).strokes
}

/// The side of the card a pattern of this spacing is shown on, in mm: about
/// 14 spacings across, so every swatch reads at the same density on screen.
fn swatch_side(spacing: f64) -> f64 {
    (spacing * 14.0).clamp(14.0, 70.0)
}

/// The width a 0.25 mm pen draws at `scale` sheet units per mm, kept
/// within what reads on screen.
fn pen(scale: f64, max: f64) -> f64 {
    (0.25 * scale).clamp(0.35, max)
}

/// The spacing that shows `name` on a `side` mm shape as its swatch shows it
/// on its own card.
fn spacing_for(name: &str, side: f64) -> f64 {
    let d = catalog::default_spacing(name).unwrap_or_else(|| panic!("gallery: no pattern {name}"));
    d * side / swatch_side(d)
}

/// `name` at its default spacing on its own card.
fn swatch(name: &str, label: impl Into<String>, about: Option<&str>, spec: Spec) -> Tile {
    let spacing = spec.spacing.or_else(|| catalog::default_spacing(name)).unwrap();
    let side = swatch_side(spacing);
    // Rings follow the outline: show them on a shape with a bay and a pond.
    let follows_outline = catalog::generators_of(name).iter().any(|g| g.name == "rings");
    let region = if follows_outline { island_with_pond(side * 0.5, side * 0.5, side * 0.45) } else { card(side) };
    let out = catalog::fill_output(name, &region, &spec.seed(SEED)).unwrap_or_else(|e| panic!("gallery: {e}"));
    let mut t = Tile::new(label, region, out.strokes);
    t.about = about.map(String::from);
    if name == "solid" {
        // Solid draws no strokes: the host paints its mask.
        t.solid = Some(out.mask);
    }
    t
}

// ---------------------------------------------------------------------------
// SVG output
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Wall {
    Solid,
    Guide,
    None,
}

/// One picture: a shape, its strokes, and what to draw round them.
struct Tile {
    label: String,
    about: Option<String>,
    region: Region,
    strokes: Strokes,
    wall: Wall,
    /// Background strokes in a lighter ink (a knocked-out lower layer).
    bg: Option<Strokes>,
    /// A mask shown as a tinted area.
    mask: Option<Region>,
    /// A mask shown as solid ink (the `solid` fill).
    solid: Option<Region>,
    /// One colour per stroke, in turn.
    inks: &'static [&'static str],
    /// A picture under the strokes (`data:` URI).
    image: Option<String>,
}

impl Tile {
    fn new(label: impl Into<String>, region: Region, strokes: Strokes) -> Tile {
        Tile { label: label.into(), about: None, region, strokes, wall: Wall::Solid, bg: None, mask: None, solid: None, inks: &[INK], image: None }
    }
    fn wall(mut self, wall: Wall) -> Tile {
        self.wall = wall;
        self
    }
}

fn num(v: f64) -> String {
    let s = format!("{v:.1}");
    if s == "-0.0" {
        "0".into()
    } else if let Some(t) = s.strip_suffix(".0") {
        t.into()
    } else {
        s
    }
}

/// Maps a shape's millimetres into a box on the sheet.
#[derive(Clone, Copy)]
struct Fit {
    scale: f64,
    dx: f64,
    dy: f64,
}

impl Fit {
    fn new(region: &Region, x: f64, y: f64, w: f64, h: f64, pad: f64) -> Fit {
        let (minx, miny, maxx, maxy) = region.bounds_mm();
        let scale = ((w - 2.0 * pad) / (maxx - minx).max(1e-6)).min((h - 2.0 * pad) / (maxy - miny).max(1e-6));
        Fit { scale, dx: x + w * 0.5 - scale * (minx + maxx) * 0.5, dy: y + h * 0.5 - scale * (miny + maxy) * 0.5 }
    }
    fn pt(&self, p: Pt) -> (f64, f64) {
        (self.dx + self.scale * p.0, self.dy + self.scale * p.1)
    }
    /// Path data in sheet units, one decimal, dropping points closer than
    /// a fraction of a pixel to the last one kept.
    fn d(&self, strokes: &[Vec<Pt>], closed: bool) -> String {
        let mut d = String::new();
        for s in strokes.iter().filter(|s| s.len() >= 2) {
            let mut last = self.pt(s[0]);
            d += &format!("M{} {}", num(last.0), num(last.1));
            for (i, &p) in s.iter().enumerate().skip(1) {
                let q = self.pt(p);
                if i + 1 < s.len() && (q.0 - last.0).hypot(q.1 - last.1) < 0.5 {
                    continue;
                }
                d += &format!("L{} {}", num(q.0), num(q.1));
                last = q;
            }
            if closed {
                d.push('Z');
            }
        }
        d
    }
}

/// Draw `t` into the box `(x, y, w, h)`; `ink` is the stroke width in sheet units.
fn draw_tile(svg: &mut String, t: &Tile, x: f64, y: f64, w: f64, h: f64, max_ink: f64) {
    let f = Fit::new(&t.region, x, y, w, h, 6.0);
    let ink = pen(f.scale, max_ink);
    let rings = t.region.all_rings_mm();
    if let Some(img) = &t.image {
        let (minx, miny, maxx, maxy) = t.region.bounds_mm();
        let (a, b) = (f.pt((minx, miny)), f.pt((maxx, maxy)));
        svg.push_str(&format!("<image x='{}' y='{}' width='{}' height='{}' href='{img}'/>\n", num(a.0), num(a.1), num(b.0 - a.0), num(b.1 - a.1)));
    }
    if let Some(m) = &t.solid {
        svg.push_str(&format!("<path style='fill:{INK}' fill-rule='evenodd' d='{}'/>\n", f.d(&m.all_rings_mm(), true)));
    }
    if let Some(m) = &t.mask {
        svg.push_str(&format!(
            "<path style='fill:{MASK_FILL}' fill-rule='evenodd' stroke='{MASK_STROKE}' stroke-width='{}' d='{}'/>\n",
            num(ink * 0.6),
            f.d(&m.all_rings_mm(), true)
        ));
    }
    if let Some(bg) = &t.bg {
        svg.push_str(&format!("<path stroke='{BG_INK}' stroke-width='{:.2}' d='{}'/>\n", ink * 0.75, f.d(bg, false)));
    }
    match t.wall {
        Wall::Solid => svg.push_str(&format!("<path stroke='{WALL}' stroke-width='{:.2}' d='{}'/>\n", ink * 0.9, f.d(&rings, true))),
        Wall::Guide => svg.push_str(&format!(
            "<path stroke='{GUIDE}' stroke-width='{:.2}' stroke-dasharray='3 2.4' d='{}'/>\n",
            ink * 0.8,
            f.d(&rings, true)
        )),
        Wall::None => {}
    }
    if t.inks.len() <= 1 {
        svg.push_str(&format!("<path stroke='{INK}' stroke-width='{ink:.2}' d='{}'/>\n", f.d(&t.strokes, false)));
    } else {
        for (k, s) in t.strokes.iter().enumerate() {
            svg.push_str(&format!("<path stroke='{}' stroke-width='{ink:.2}' d='{}'/>\n", t.inks[k % t.inks.len()], f.d(std::slice::from_ref(s), false)));
        }
    }
}

fn svg_open(w: f64, h: f64, px: f64) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w} {h}' width='{}' height='{}'>\n\
         <rect width='{w}' height='{h}' fill='{PAPER}'/>\n\
         <style>path{{fill:none;stroke-linecap:round;stroke-linejoin:round}}text{{font-family:Georgia,serif;fill:{LABEL}}}.h{{fill:{HEADING};font-weight:bold}}</style>\n",
        num(w * px),
        num(h * px)
    )
}

fn escape(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// `text` broken into lines of at most `width` characters, at most `max` lines.
fn wrap(text: &str, width: usize, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur += word;
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max {
        lines.truncate(max);
        let last = lines.last_mut().unwrap();
        while last.chars().count() > width.saturating_sub(1) {
            last.pop();
        }
        *last = format!("{}…", last.trim_end_matches([',', ':', ' ']));
    }
    lines
}

// ---------------------------------------------------------------------------
// Tile grids: README strips and the catalogue sheet
// ---------------------------------------------------------------------------

const TILE_W: f64 = 124.0;
const TILE_H: f64 = 116.0;
const NAME_H: f64 = 18.0;
const ABOUT_LINE: f64 = 8.0;
const ABOUT_LINES: usize = 3;
const STRIP_INK: f64 = 0.9;

struct Section {
    title: Option<String>,
    about: Option<String>,
    tiles: Vec<Tile>,
}

/// Columns that spread `n` tiles over rows of at most `max` as evenly as
/// possible (13 → 5, 5, 3 rather than 6, 6, 1).
fn balanced_cols(n: usize, max: usize) -> usize {
    let rows = n.div_ceil(max).max(1);
    n.div_ceil(rows)
}

/// Sections of tiles, `cols` across; a short last row is centred. With
/// `about`, each tile's description is printed under its name.
fn render_grid(sections: &[Section], cols: usize, about: bool) -> String {
    let margin = 8.0;
    let width = cols as f64 * TILE_W + 2.0 * margin;
    let mut body = String::new();
    let mut y = margin;
    for s in sections {
        let about = about && s.tiles.iter().any(|t| t.about.is_some());
        let row_h = TILE_H + NAME_H + if about { ABOUT_LINE * ABOUT_LINES as f64 + 2.0 } else { 0.0 };
        if let Some(title) = &s.title {
            body += &format!("<text class='h' x='{}' y='{}' font-size='15'>{}</text>\n", margin + 6.0, num(y + 16.0), escape(title));
            y += 22.0;
            if let Some(a) = &s.about {
                body += &format!("<text x='{}' y='{}' font-size='9'>{}</text>\n", margin + 6.0, num(y + 8.0), escape(a));
                y += 14.0;
            }
        }
        for (r, row) in s.tiles.chunks(cols).enumerate() {
            let x0 = margin + (cols - row.len()) as f64 * TILE_W * 0.5;
            let ty = y + r as f64 * row_h;
            for (c, t) in row.iter().enumerate() {
                let tx = x0 + c as f64 * TILE_W;
                draw_tile(&mut body, t, tx, ty, TILE_W, TILE_H, STRIP_INK);
                body += &format!(
                    "<text x='{}' y='{}' font-size='10' text-anchor='middle' style='fill:{HEADING}'>{}</text>\n",
                    num(tx + TILE_W * 0.5),
                    num(ty + TILE_H + 9.0),
                    escape(&t.label)
                );
                if about {
                    for (k, line) in wrap(t.about.as_deref().unwrap_or(""), 34, ABOUT_LINES).iter().enumerate() {
                        body += &format!(
                            "<text x='{}' y='{}' font-size='6.4' text-anchor='middle'>{}</text>\n",
                            num(tx + TILE_W * 0.5),
                            num(ty + TILE_H + NAME_H + 4.0 + k as f64 * ABOUT_LINE),
                            escape(line)
                        );
                    }
                }
            }
        }
        y += s.tiles.len().div_ceil(cols) as f64 * row_h + 10.0;
    }
    let height = y + margin - 10.0;
    format!("{}{body}</svg>\n", svg_open(width, height, 2.0))
}

/// One untitled section as a README strip, rows at most six across.
fn strip(tiles: Vec<Tile>) -> String {
    let cols = balanced_cols(tiles.len(), 6);
    render_grid(&[Section { title: None, about: None, tiles }], cols, false)
}

// ---------------------------------------------------------------------------
// The catalogue's sections
// ---------------------------------------------------------------------------

/// Every material of `group`, at its own spacing.
fn group_tiles(group: Group, about: bool) -> Vec<Tile> {
    catalog::MATERIALS
        .iter()
        .filter(|m| m.group == group)
        .map(|m| swatch(m.name, m.name, about.then_some(m.about), Spec::default()))
        .collect()
}

/// Every generator at its defaults.
fn generator_tiles(about: bool) -> Vec<Tile> {
    catalog::generators().iter().map(|g| swatch(g.name, g.name, about.then_some(g.about), Spec::default())).collect()
}

/// Every symbol in the library, scattered upright on a small card.
fn symbol_tiles() -> Vec<Tile> {
    marks::NAMES
        .iter()
        .map(|&name| {
            let region = card(26.0);
            let spec = Spec::default()
                .spacing(6.5)
                .seed(SEED)
                .option("motif", name)
                .option("mark", 4.4)
                .option("arrange", "hex")
                .option("jitter", 0.15)
                .option("rotate", "upright");
            Tile::new(name, region.clone(), draw("scatter", &region, spec))
        })
        .collect()
}

/// The shared vocabulary and the generators' own options, a cell each.
fn option_tiles() -> Vec<Tile> {
    let cases: Vec<(&str, &str, Spec)> = vec![
        ("hatch", "hatch · cross 2, dash", Spec::default().spacing(1.0).option("cross", 2.0).option("dash", Value::Nums(vec![1.4, 0.6])).option("dash_jitter", 0.6)),
        ("hatch", "hatch · density edge", Spec::default().spacing(0.8).angle(30.0).option("cross", 2.0).option("density", "edge")),
        ("hatch", "hatch · wobble, vary", Spec::default().spacing(1.0).option("wobble", 1.2).option("vary", 0.7)),
        ("scatter", "scatter · tree in rows", Spec::default().spacing(3.0).option("motif", "tree").option("arrange", "rows").option("rotate", "upright").option("mark", 2.0)),
        ("scatter", "scatter · tuft, noise", Spec::default().spacing(2.4).option("motif", "tuft").option("density", "noise").option("mark", 1.4)),
        ("scatter", "scatter · gradient", Spec::default().spacing(1.0).option("motif", "dot").option("density", "gradient").option("contrast", 1.0)),
        ("cells", "cells · stones, vary", Spec::default().spacing(3.0).option("vary", 0.8)),
        ("cells", "cells · net", Spec::default().spacing(3.0).option("style", "net")),
        ("cells", "cells · cracks, dots", Spec::default().spacing(3.5).option("style", "cracks").option("inner", "dots")),
        ("tile", "tile · herringbone", Spec::default().spacing(1.4).option("motif", "herringbone")),
        ("waves", "waves · scallop, cross 2", Spec::default().spacing(1.5).option("shape", "scallop").option("cross", 2.0)),
        ("waves", "waves · curl", Spec::default().spacing(1.6).option("shape", "curl")),
    ];
    cases.into_iter().map(|(g, label, spec)| swatch(g, label, None, spec)).collect()
}

/// Clouds, the one sky material, at its defaults and with a few options.
fn sky_tiles(about: bool) -> Vec<Tile> {
    let about = |t: &str| about.then(|| t.to_string());
    let mut tiles = group_tiles(Group::Sky, false);
    for t in &mut tiles {
        t.about = catalog::material(&t.label).and_then(|m| about(m.about));
    }
    for (label, spec) in [
        ("clouds · coverage 0.75", Spec::default().option("coverage", 0.75)),
        ("clouds · shading lines", Spec::default().option("shading", "lines")),
        ("clouds · flat, windy", Spec::default().option("height", 0.4).option("stretch", 0.7)),
    ] {
        tiles.push(swatch("clouds", label, None, spec));
    }
    tiles
}

fn gallery_sheet() -> String {
    let mut sections: Vec<Section> = Vec::new();
    for g in Group::ALL {
        let tiles = if g == Group::Sky { sky_tiles(true) } else { group_tiles(g, true) };
        sections.push(Section { title: Some(g.title().into()), about: Some(g.about().into()), tiles });
    }
    sections.push(Section {
        title: Some("Generators".into()),
        about: Some("The algorithms under every material, each at its own defaults; every option is open to set.".into()),
        tiles: generator_tiles(true),
    });
    sections.push(Section {
        title: Some("Options".into()),
        about: Some("A few of the generators' options: the same vocabulary on every pattern.".into()),
        tiles: option_tiles(),
    });
    sections.push(Section {
        title: Some("Symbols".into()),
        about: Some("The marks scatter draws by name (motif); pass your own polylines for anything else.".into()),
        tiles: symbol_tiles(),
    });
    render_grid(&sections, 6, true)
}

// ---------------------------------------------------------------------------
// Feature sheets (`examples/gallery/*.svg`): rows of variations
// ---------------------------------------------------------------------------

const CELL_W: f64 = 84.0;
const CELL_H: f64 = 74.0;
const LABEL_W: f64 = 120.0;
const HEADER_H: f64 = 24.0;
const TITLE_H: f64 = 38.0;

fn render_sheet(title: &str, note: &str, columns: &[&str], rows: &[(String, Vec<Tile>)]) -> String {
    let longest = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0) as f64;
    let label_w = LABEL_W.max(28.0 + longest * 4.6);
    let width = label_w + columns.len() as f64 * CELL_W + 20.0;
    let height = TITLE_H + HEADER_H + rows.len() as f64 * CELL_H + 16.0;
    let mut svg = svg_open(width, height, 1.5);
    svg += &format!("<text x='14' y='18' font-size='12' class='h'>{}</text>\n", escape(title));
    svg += &format!("<text x='14' y='32' font-size='8.5'>{}</text>\n", escape(note));
    for (i, name) in columns.iter().enumerate() {
        let x = label_w + (i as f64 + 0.5) * CELL_W;
        svg += &format!("<text x='{}' y='{}' font-size='8.5' text-anchor='middle'>{}</text>\n", num(x), TITLE_H + HEADER_H - 7.0, escape(name));
    }
    for (ri, (label, cells)) in rows.iter().enumerate() {
        let y = TITLE_H + HEADER_H + ri as f64 * CELL_H;
        svg += &format!("<text x='14' y='{}' font-size='8.5'>{}</text>\n", num(y + CELL_H * 0.5 + 3.0), escape(label));
        for (ci, cell) in cells.iter().enumerate() {
            draw_tile(&mut svg, cell, label_w + ci as f64 * CELL_W, y, CELL_W, CELL_H, 0.5);
        }
    }
    svg += "</svg>\n";
    svg
}

/// The patterns the fray pictures use: lines, marks and closed cells.
const FRAYED: [(&str, &str); 12] = [
    ("serpentine", "continuous line"),
    ("waves", "continuous wave"),
    ("swells", "layered swells"),
    ("eddies", "flow lines"),
    ("dots", "lattice marks"),
    ("stipple", "scattered marks"),
    ("grass", "tufts"),
    ("twigs", "branched twigs"),
    ("gravel", "stones"),
    ("honeycomb", "hex lattice"),
    ("cracked-earth", "crack net"),
    ("canopy", "foliage lobes"),
];

fn frayed(name: &str, region: &Region, fray: f64) -> Strokes {
    draw(name, region, Spec::default().spacing(spacing_for(name, 52.0)).seed(SEED).fray(fray))
}

fn fray_rows(s: &Shapes) -> Vec<(String, Vec<Tile>)> {
    FRAYED
        .iter()
        .map(|&(name, kind)| {
            let s9 = frayed(name, &s.island, 9.0);
            let cells = vec![
                Tile::new("", s.island.clone(), frayed(name, &s.island, 0.0)),
                Tile::new("", s.island.clone(), frayed(name, &s.island, 4.5)).wall(Wall::Guide),
                Tile::new("", s.island.clone(), s9.clone()).wall(Wall::Guide),
                Tile::new("", s.island.clone(), s9).wall(Wall::None),
            ];
            (format!("{name} ({kind})"), cells)
        })
        .collect()
}

/// A background hatch over `region` grown by 4.5 mm, with `mask` cut out.
fn knockout(region: &Region, mask: &Region, name: &str, spacing: f64, angle: f64) -> Strokes {
    let below = region.outset(4.5).difference(&mask.outset(0.35));
    draw(name, &below, Spec::default().spacing(spacing).angle(angle).anchor(Anchor::GLOBAL).inset(0.4))
}

fn edge_rows(s: &Shapes) -> Vec<(String, Vec<Tile>)> {
    let region = &s.pond_island;
    [("canopy", 5.2, 0.0), ("gravel", 4.6, 0.0), ("isoline-stack", 7.0, 0.0), ("pebbles", 4.0, 0.0), ("grass (frayed)", 3.0, 6.5)]
        .into_iter()
        .map(|(label, spacing, fray)| {
            let name = label.split(' ').next().unwrap();
            let spec = |edge: EdgeMode| Spec::default().spacing(spacing).seed(SEED).fray(fray).edge(edge);
            let over_edge = if fray > 0.0 { EdgeMode::Clip } else { EdgeMode::Overflow(3.5) };
            let over = catalog::fill_output(name, region, &spec(over_edge)).unwrap();
            let bg = knockout(region, &over.mask, "lines", 1.3, 35.0);
            let mut masked = Tile::new("", region.clone(), over.strokes.clone()).wall(Wall::Guide);
            masked.mask = Some(over.mask.clone());
            let mut knocked = Tile::new("", region.clone(), over.strokes.clone()).wall(Wall::Guide);
            knocked.bg = Some(bg);
            let cells = vec![
                Tile::new("", region.clone(), draw(name, region, spec(EdgeMode::Clip))),
                Tile::new("", region.clone(), draw(name, region, spec(EdgeMode::Inside))),
                Tile::new("", region.clone(), over.strokes).wall(Wall::Guide),
                masked,
                knocked,
            ];
            (label.to_string(), cells)
        })
        .collect()
}

/// The CAD scale that shows each built-in `.pat` at a readable size.
fn pat_scale(name: &str) -> f64 {
    match name {
        "PARQUET" => 1.0,
        "BASKET" => 1.3,
        "ANSI31" | "ANSI37" | "GRATE" => 2.0,
        "EARTH" => 3.2,
        _ => 1.6,
    }
}

fn pat_fill(name: &str, region: &Region, fray: f64) -> Strokes {
    draw("tile", region, Spec::default().spacing(pat_scale(name)).option("pat", name).inset(0.8).fray(fray).seed(SEED))
}

fn cad_rows(s: &Shapes) -> Vec<(String, Vec<Tile>)> {
    PAT_BUILTIN
        .iter()
        .map(|&name| {
            let fr = pat_fill(name, &s.island, FRAY_MM);
            let cells = vec![
                Tile::new("", s.square.clone(), pat_fill(name, &s.square, 0.0)),
                Tile::new("", s.ring.clone(), pat_fill(name, &s.ring, 0.0)),
                Tile::new("", s.island.clone(), fr.clone()).wall(Wall::Guide),
                Tile::new("", s.island.clone(), fr).wall(Wall::None),
            ];
            (format!("pat: {name}"), cells)
        })
        .collect()
}

const FLOWER_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20">
  <circle cx="10" cy="10" r="1.6"/>
  <g transform="translate(10 10)">
    <ellipse cx="0" cy="-4.6" rx="1.7" ry="2.8"/>
    <ellipse cx="0" cy="-4.6" rx="1.7" ry="2.8" transform="rotate(90)"/>
    <ellipse cx="0" cy="-4.6" rx="1.7" ry="2.8" transform="rotate(180)"/>
    <ellipse cx="0" cy="-4.6" rx="1.7" ry="2.8" transform="rotate(270)"/>
  </g>
  <path d="M0 0 Q 2.5 2.5 0 5 M20 15 Q 17.5 17.5 20 20"/>
</svg>"#;

/// A little house, 4 mm wide, in mm around the origin (up is negative y): a motif of your own.
fn house() -> Strokes {
    vec![vec![(-2.0, 2.0), (-2.0, -0.8), (0.0, -2.6), (2.0, -0.8), (2.0, 2.0), (-2.0, 2.0)], vec![(-0.6, 2.0), (-0.6, 0.6), (0.6, 0.6), (0.6, 2.0)]]
}

/// A tile of your own: a wave with a dot under each crest, 4 × 2 mm.
fn wave_tile() -> Strokes {
    let wave: Vec<Pt> = (0..=24).map(|k| (-2.0 + 4.0 * k as f64 / 24.0, -0.5 * (PI * k as f64 / 12.0).cos())).collect();
    vec![wave, vec![(0.0, 0.55), (0.0, 0.75)]]
}

type Custom = Box<dyn Fn(&Region) -> Strokes>;

fn custom_fills() -> Vec<(&'static str, Custom)> {
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let big = parse_svg(FLOWER_SVG).unwrap();
    vec![
        (
            "sin x · cos y",
            Box::new(|r: &Region| {
                let levels: Vec<f64> = levels_every(-1.0, 1.0, 0.25).iter().map(|l| l + 0.125).collect();
                fill_isolines(&r.inset(0.8), 0.25, &levels, |p| (p.0 / 3.5).sin() * (p.1 / 3.5).cos())
            }),
        ),
        (
            "ripples, 3 sources",
            Box::new(|r: &Region| {
                let (minx, miny, maxx, maxy) = r.bounds_mm();
                let src = [(minx, miny), (maxx, miny + (maxy - miny) * 0.4), ((minx + maxx) * 0.5, maxy)];
                fill_isolines(&r.inset(0.8), 0.25, &levels_every(-2.5, 2.5, 1.0), move |p| src.iter().map(|s| ((p.0 - s.0).hypot(p.1 - s.1) / 1.5).sin()).sum())
            }),
        ),
        ("profile: sawtooth", Box::new(|r: &Region| fill_profile(&r.inset(0.8), 2.5, 20.0, Anchor::LOCAL, |x| (x / 4.0).rem_euclid(1.0) * 1.8))),
        (
            "scatter, your motif",
            Box::new(|r: &Region| draw("scatter", r, Spec::default().spacing(7.0).seed(SEED).option("motif", house()).option("arrange", "hex").option("rotate", "upright"))),
        ),
        ("tile, your motif", Box::new(|r: &Region| draw("tile", r, Spec::default().spacing(2.0).option("motif", wave_tile()).option("size", Value::Nums(vec![4.0, 2.6])).option("stagger", 0.5)))),
        ("svg tile", Box::new(move |r: &Region| fill_svg_tile(&r.inset(0.8), &flower, 10.0, 15.0, 0.5, Anchor::LOCAL))),
        (
            "svg clipped as is",
            Box::new(move |r: &Region| {
                let (minx, miny, maxx, maxy) = r.bounds_mm();
                let size = (maxx - minx).max(maxy - miny) * 1.15;
                let art = big.fit_width(size);
                let (ox, oy) = ((minx + maxx - size) * 0.5, (miny + maxy - size) * 0.5);
                let placed: Strokes = art.strokes.iter().map(|s| s.iter().map(|p| (p.0 + ox, p.1 + oy)).collect()).collect();
                r.clip_strokes(&placed)
            }),
        ),
    ]
}

fn custom_rows(s: &Shapes) -> Vec<(String, Vec<Tile>)> {
    let regions = [&s.square, &s.ring, &s.pond_island, &s.star];
    custom_fills()
        .into_iter()
        .map(|(label, f)| (label.to_string(), regions.iter().map(|&r| Tile::new("", r.clone(), f(r))).collect()))
        .collect()
}

/// A lit ball on a table: the picture the dither sheets use (y down).
fn picture() -> Greyscale {
    let n = 240;
    let (cx, cy, r) = (0.46, 0.44, 0.3);
    let light = {
        let (x, y, z): (f64, f64, f64) = (-0.55, -0.6, 0.58);
        let len = (x * x + y * y + z * z).sqrt();
        (x / len, y / len, z / len)
    };
    let px = (0..n * n)
        .map(|k| {
            let (x, y) = ((k % n) as f64 / n as f64, (k / n) as f64 / n as f64);
            let mut v = if y < 0.7 { 0.92 - 0.25 * y } else { 0.62 - 0.5 * (y - 0.7) };
            let (sx, sy) = ((x - cx - 0.12) / 0.34, (y - 0.78) / 0.07);
            v *= 1.0 - 0.55 * (-(sx * sx + sy * sy)).exp();
            let (dx, dy) = ((x - cx) / r, (y - cy) / r);
            let d2 = dx * dx + dy * dy;
            if d2 < 1.0 {
                let dz = (1.0 - d2).sqrt();
                let lambert = (dx * light.0 + dy * light.1 + dz * light.2).max(0.0);
                v = (0.06 + 0.8 * lambert + 0.5 * lambert.powi(40)).min(1.0);
            }
            v
        })
        .collect();
    Greyscale::new(n, n, px).unwrap()
}


const DITHER_METHODS: [(&str, Dither); 10] = [
    ("threshold", Dither::Threshold),
    ("random", Dither::Random),
    ("bayer 4", Dither::Bayer(4)),
    ("bayer 8", Dither::Bayer(8)),
    ("halftone 6", Dither::Halftone(6)),
    ("floyd-steinberg", Dither::FloydSteinberg),
    ("atkinson", Dither::Atkinson),
    ("jarvis", Dither::JarvisJudiceNinke),
    ("stucki", Dither::Stucki),
    ("sierra", Dither::Sierra),
];

fn dither_rows(s: &Shapes) -> Vec<(String, Vec<Tile>)> {
    let img = picture();
    DITHER_METHODS
        .iter()
        .map(|&(label, method)| {
            let base = DitherOptions { method, seed: SEED, ..DitherOptions::new(0.55) };
            let variants = [
                (&s.square, DitherOptions { marks: Marks::Dots, ..base }),
                (&s.square, base),
                (&s.square, DitherOptions { angle: 45.0, ..base }),
                (&s.square, DitherOptions { resolution: 0.35, line_spacing: 1.1, ..base }),
                (&s.star, DitherOptions { line_spacing: 0.75, ..base }),
            ];
            (label.to_string(), variants.into_iter().map(|(r, o)| Tile::new("", r.clone(), fill_dither(r, &img, &o))).collect())
        })
        .collect()
}

fn placement_rows(s: &Shapes) -> Vec<(String, Vec<Tile>)> {
    let img = picture();
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let regions = [&s.square, &s.ring, &s.pond_island, &s.star];
    let mut rows: Vec<(String, Vec<Tile>)> = [
        ("cover", "cover", "center", "no-repeat"),
        ("contain", "contain", "center", "no-repeat"),
        ("fill (stretched)", "fill", "center", "no-repeat"),
        ("14 mm, repeat", "14", "center", "repeat"),
        ("16 mm, repeat-x, top", "16", "top", "repeat-x"),
        ("24 mm, top left", "24", "top left", "no-repeat"),
    ]
    .iter()
    .map(|&(label, size, position, repeat)| {
        let o = DitherOptions { placement: Placement::parse(size, position, repeat).unwrap(), ..DitherOptions::new(0.45) };
        (format!("picture · {label}"), regions.iter().map(|&r| Tile::new("", r.clone(), fill_dither(r, &img, &o))).collect())
    })
    .collect();
    for (label, size, position, repeat) in [("contain", "contain", "center", "no-repeat"), ("10 mm, repeat", "10", "center", "repeat"), ("12 mm, repeat-y, left", "12", "left", "repeat-y")] {
        let place = Placement::parse(size, position, repeat).unwrap();
        rows.push((format!("svg · {label}"), regions.iter().map(|&r| Tile::new("", r.clone(), fill_svg_placed(r, &flower, &place))).collect()));
    }
    rows
}

fn picture_data_uri(img: &Greyscale) -> String {
    let luma = image::GrayImage::from_fn(img.width() as u32, img.height() as u32, |x, y| image::Luma([(img.pixel(x as usize, y as usize) * 255.0).round() as u8]));
    let mut png = Vec::new();
    luma.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("data:image/png;base64,");
    for chunk in png.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | (b[2] as u32);
        for k in 0..4 {
            out.push(if k <= chunk.len() { B64[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// README strips (`docs/img/*.svg`)
// ---------------------------------------------------------------------------

fn write(path: &str, svg: String) {
    std::fs::write(path, &svg).unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote {path} ({} KB)", svg.len() / 1024);
}

fn readme_strips(s: &Shapes) {
    let dir = "docs/img";
    std::fs::create_dir_all(dir).unwrap();

    // The catalogue, group by group.
    for g in Group::ALL {
        let tiles = if g == Group::Sky { sky_tiles(false) } else { group_tiles(g, false) };
        write(&format!("{dir}/{}.svg", g.id()), strip(tiles));
    }
    write(&format!("{dir}/generators.svg"), strip(generator_tiles(false)));
    write(&format!("{dir}/options.svg"), strip(option_tiles()));
    let symbols = symbol_tiles();
    let cols = balanced_cols(symbols.len(), 8);
    write(&format!("{dir}/symbols.svg"), render_grid(&[Section { title: None, about: None, tiles: symbols }], cols, false));

    // Hero: a serpentine coloured per pen-down, against plain hatching.
    let hero: Vec<Tile> = [&s.ring, &s.pond_island, &s.star, &s.archipelago]
        .iter()
        .map(|&r| {
            let spec = Spec::default().spacing(1.6).inset(0.8);
            let zig = draw("serpentine", r, spec.clone());
            let hatch = draw("lines", r, spec).len();
            let mut t = Tile::new(format!("{} strokes (hatch: {hatch})", zig.len()), r.clone(), zig);
            t.inks = &PENS;
            t
        })
        .collect();
    write(&format!("{dir}/hero.svg"), strip(hero));

    // Tour: six materials on a ring with a hole.
    let tour: Vec<Tile> = ["concentric", "wave-net", "scallops", "gravel", "swells", "woodland"]
        .iter()
        .map(|&name| Tile::new(name, s.ring.clone(), draw(name, &s.ring, Spec::default().spacing(spacing_for(name, 52.0)).seed(SEED))))
        .collect();
    write(&format!("{dir}/tour.svg"), strip(tour));

    let shapes: Vec<Tile> = [("square", &s.square), ("rounded card", &s.card), ("ring (hole)", &s.ring), ("island + pond", &s.pond_island), ("star", &s.star), ("archipelago", &s.archipelago)]
        .iter()
        .map(|&(label, r)| Tile::new(label, r.clone(), draw("concentric", r, Spec::default().spacing(1.6).inset(0.8))))
        .collect();
    write(&format!("{dir}/shapes.svg"), strip(shapes));

    // Fray: lines, marks and closed cells fading out at the edge.
    let fray: Vec<Tile> = [("serpentine", Wall::Guide), ("waves", Wall::None), ("dots", Wall::Guide), ("gravel", Wall::None), ("grass", Wall::Guide), ("cracked-earth", Wall::None)]
        .iter()
        .map(|&(name, wall)| Tile::new(format!("{name} · fray 9"), s.island.clone(), frayed(name, &s.island, 9.0)).wall(wall))
        .collect();
    write(&format!("{dir}/fray.svg"), strip(fray));

    // Edge modes and masks.
    let r = &s.pond_island;
    let gravel = |edge: EdgeMode| catalog::fill_output("gravel", r, &Spec::default().spacing(4.8).seed(SEED).edge(edge)).unwrap();
    let over = gravel(EdgeMode::Overflow(3.5));
    let canopy = catalog::fill_output("canopy", r, &Spec::default().spacing(5.4).seed(SEED).edge(EdgeMode::Overflow(3.5))).unwrap();
    let mut masked = Tile::new("FillOutput.mask", r.clone(), over.strokes.clone()).wall(Wall::Guide);
    masked.mask = Some(over.mask.clone());
    let mut gravel_ko = Tile::new("gravel + hatch knockout", r.clone(), over.strokes.clone()).wall(Wall::Guide);
    gravel_ko.bg = Some(knockout(r, &over.mask, "lines", 1.35, 35.0));
    let mut canopy_ko = Tile::new("canopy + wave knockout", r.clone(), canopy.strokes).wall(Wall::Guide);
    canopy_ko.bg = Some(knockout(r, &canopy.mask, "waves", 1.5, 20.0));
    let edges = vec![
        Tile::new("EdgeMode::Clip", r.clone(), gravel(EdgeMode::Clip).strokes),
        Tile::new("EdgeMode::Inside", r.clone(), gravel(EdgeMode::Inside).strokes),
        Tile::new("EdgeMode::Overflow", r.clone(), over.strokes).wall(Wall::Guide),
        masked,
        gravel_ko,
        canopy_ko,
    ];
    write(&format!("{dir}/edges-and-masks.svg"), strip(edges));

    // CAD hatches through `tile` with `pat`.
    let cad: Vec<Tile> = [("BRICK", 1.8), ("HERRINGBONE", 1.8), ("PARQUET", 1.1), ("EARTH", 3.4), ("ANSI37", 2.2), ("CROSS", 1.8)]
        .iter()
        .map(|&(name, scale)| Tile::new(format!("pat: {name}"), s.card.clone(), draw("tile", &s.card, Spec::default().spacing(scale).option("pat", name).inset(0.8))))
        .collect();
    write(&format!("{dir}/cad.svg"), strip(cad));

    // Dither: the picture, then five ways to draw it.
    let img = picture();
    let base = DitherOptions { seed: SEED, ..DitherOptions::new(0.55) };
    let mut dithers = vec![{
        let mut t = Tile::new("the picture", s.square.clone(), vec![]).wall(Wall::None);
        t.image = Some(picture_data_uri(&img));
        t
    }];
    for (label, o) in [
        ("bayer 4, dots", DitherOptions { method: Dither::Bayer(4), marks: Marks::Dots, ..base }),
        ("halftone, lines at 45°", DitherOptions { method: Dither::Halftone(6), angle: 45.0, ..base }),
        ("floyd-steinberg, lines", base),
        ("atkinson, dots", DitherOptions { method: Dither::Atkinson, marks: Marks::Dots, ..base }),
        ("jarvis, lines 1 mm apart", DitherOptions { method: Dither::JarvisJudiceNinke, resolution: 0.35, line_spacing: 1.1, ..base }),
    ] {
        dithers.push(Tile::new(label, s.square.clone(), fill_dither(&s.square, &img, &o)));
    }
    write(&format!("{dir}/dither.svg"), strip(dithers));

    // Placement, like a CSS background.
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let place = |size: &str, position: &str, repeat: &str| Placement::parse(size, position, repeat).unwrap();
    let dith = |r: &Region, p: Placement| fill_dither(r, &img, &DitherOptions { placement: p, ..DitherOptions::new(0.55) });
    let placed = vec![
        Tile::new("cover", s.ring.clone(), dith(&s.ring, place("cover", "center", "no-repeat"))),
        Tile::new("contain", s.star.clone(), dith(&s.star, place("contain", "center", "no-repeat"))),
        Tile::new("16 mm, repeat", s.card.clone(), dith(&s.card, place("16", "center", "repeat"))),
        Tile::new("18 mm, repeat-x, top", s.square.clone(), dith(&s.square, place("18", "top", "repeat-x"))),
        Tile::new("svg, 11 mm, repeat", s.ring.clone(), fill_svg_placed(&s.ring, &flower, &place("11", "center", "repeat"))),
        Tile::new("svg, contain", s.star.clone(), fill_svg_placed(&s.star, &flower, &place("contain", "center", "no-repeat"))),
    ];
    write(&format!("{dir}/placement.svg"), strip(placed));

    // Your own patterns.
    let custom: Vec<Tile> = custom_fills().into_iter().filter(|(l, _)| *l != "svg clipped as is").map(|(label, f)| Tile::new(label, s.card.clone(), f(&s.card))).collect();
    write(&format!("{dir}/custom.svg"), strip(custom));
}

fn main() {
    let s = shapes();
    let dir = "examples/gallery";
    std::fs::create_dir_all(dir).unwrap();

    let gallery = gallery_sheet();
    write(&format!("{dir}/gallery.svg"), gallery);

    write(
        &format!("{dir}/fray.svg"),
        render_sheet(
            "edge fraying (fray)",
            "the pattern thins out toward the outline: lines break into dashes, marks drop and shrink, cells open",
            &["fray 0 (hard edge)", "fray 4.5 mm", "fray 9 mm (guide)", "fray 9 mm (ink only)"],
            &fray_rows(&s),
        ),
    );
    write(
        &format!("{dir}/edges-and-masks.svg"),
        render_sheet(
            "edge modes (EdgeMode) and masks (FillOutput)",
            "Clip · Inside · Overflow(3.5) · the mask a fill covers · a lower layer with the mask cut out",
            &["Clip", "Inside", "Overflow(3.5)", "FillOutput.mask", "knockout"],
            &edge_rows(&s),
        ),
    );
    write(
        &format!("{dir}/cad.svg"),
        render_sheet(
            "CAD hatches (.pat) through tile",
            "every built-in .pat: catalog::fill(\"tile\", …, spacing = pattern unit, pat = name or .pat text)",
            &["square", "ring + hole", "frayed (guide)", "frayed (ink only)"],
            &cad_rows(&s),
        ),
    );
    write(
        &format!("{dir}/custom.svg"),
        render_sheet(
            "build your own",
            "fill_isolines(f) · fill_profile(g) · scatter and tile with your polylines as motif · fill_svg_tile(svg) · Region::clip_strokes",
            &["square", "ring (hole)", "island + pond", "star"],
            &custom_rows(&s),
        ),
    );
    write(
        &format!("{dir}/dither.svg"),
        render_sheet(
            "dither",
            "a picture dithered on 0.55 mm cells · dots: a tick per dark cell · lines: dark runs joined back and forth",
            &["dots", "lines", "lines at 45°", "lines 1 mm apart", "clipped by a star"],
            &dither_rows(&s),
        ),
    );
    write(
        &format!("{dir}/placement.svg"),
        render_sheet(
            "placement",
            "a picture or an SVG placed like a CSS background (size · position · repeat) and cut by the outline",
            &["square", "ring (hole)", "island + pond", "star"],
            &placement_rows(&s),
        ),
    );

    readme_strips(&s);

    let materials = catalog::MATERIALS.len();
    let generators = catalog::generators().len();
    let ink: f64 = catalog::names()
        .iter()
        .filter(|n| **n != "solid")
        .map(|n| draw(n, &s.square, Spec::default().spacing(spacing_for(n, 52.0)).seed(SEED)).iter().map(|p| polyline_length(p)).sum::<f64>())
        .sum();
    println!("done: {materials} materials, {generators} generators, {} symbols · {ink:.0} mm of ink for one 52 mm square of each", marks::NAMES.len());
}
