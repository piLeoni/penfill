//! Gallery sheets and README illustrations for `penfill`.
//!
//! Run:
//! ```sh
//! cargo run --release --example gallery
//! ```
//!
//! Generates:
//! - `examples/gallery/<slug>.svg` — per-pattern sheet showing 4 columns:
//!   1. `square` (unfrayed, crisp wall)
//!   2. `ring (hole)` (unfrayed, smooth circular ring + inner hole)
//!   3. `frayed (guide)` (organic contour with `fray = 8.5 mm` and dashed wall guide)
//!   4. `frayed (ink only)` (same frayed fill with no wall line, floating on paper)
//! - `examples/gallery/gallery.svg` — categorized visual reference sheet of all fills
//! - `examples/gallery/all-patterns.svg` — alphabetical index grid
//! - `examples/gallery/fray.svg` — universal edge fraying progression (`0 mm` → `4.5 mm` → `9 mm`)
//! - `examples/gallery/edges-and-masks.svg` — `EdgeMode::{Clip, Inside, Overflow}` & `FillOutput.mask` knockouts
//! - `examples/gallery/{cad,custom,dither,placement}.svg` — specialized feature sheets
//! - `docs/img/*.svg` — horizontal illustration strips embedded in `README.md`

use std::f64::consts::{PI, TAU};

use penfill::geo::{polyline_length, Pt, Region, Strokes};
use penfill::{
    dither::{fill_dither, Dither, DitherOptions, Greyscale, Marks},
    dotswaves::{fill_dots_with_lattice, Lattice},
    field::{fill_isolines, fill_profile, levels_every},
    fill_by_name, fill_output_by_name,
    fray::fray_strokes,
    gravel::fill_gravel_params,
    grid::fill_triangles,
    growth::{fill_growth, GrowthParams},
    isoline_blobs::{default_params, fill_isoline_blobs_with_coverage},
    isoline_stack::{fill_isoline_stack_with_coverage, IsolineStackParams, Light},
    pat::{builtin, fill_pat, BUILTIN as PAT_BUILTIN},
    placement::Placement,
    radial::fill_sunburst,
    rectilinear::fill_rectilinear,
    svg::{fill_svg_placed, fill_svg_tile, parse_svg},
    zigzag::fill_zigzag_inset,
    Anchor, EdgeMode, FillOutput, FillParams,
};

const SEED: u64 = 7;

const FINE_MM: f64 = 0.9;
const MID_MM: f64 = 1.6;
const COARSE_MM: f64 = 2.6;
const FRAY_MM: f64 = 8.5;

/// Warm paper, warm-gray wall, deep ink fill, muted label ink.
const PAPER: &str = "#faf8f4";
const CARD_BG: &str = "#ffffff";
const WALL: &str = "#b8afa4";
const GUIDE: &str = "#c7b8a8";
const INK: &str = "#231f1c";
const BG_INK: &str = "#8c8278";
const MASK_FILL: &str = "#e8dfd3";
const MASK_STROKE: &str = "#a38f78";
const LABEL: &str = "#5c554e";
const HEADING: &str = "#2a2520";

/// Muted inks for multi-stroke pen-down visualization (`hero.svg`).
const PENS: [&str; 5] = ["#231f1c", "#b0573a", "#3f6b8a", "#6f8a3a", "#8a4f7a"];

const CELL_W: f64 = 84.0;
const CELL_H: f64 = 74.0;
const LABEL_W: f64 = 104.0;
const HEADER_H: f64 = 24.0;
const TITLE_H: f64 = 38.0;

// ---------------------------------------------------------------------------
// Clean Gallery Shapes
// ---------------------------------------------------------------------------

fn circle_ring(cx: f64, cy: f64, r: f64, n: usize) -> Vec<Pt> {
    (0..n)
        .map(|i| {
            let a = TAU * (i as f64) / (n as f64);
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Smooth rounded square (`w × h` with corner radius `r`), centred at `(cx, cy)`.
fn rounded_rect(cx: f64, cy: f64, w: f64, h: f64, r: f64) -> Region {
    let r = r.min(w * 0.45).min(h * 0.45);
    let hw = w * 0.5 - r;
    let hh = h * 0.5 - r;
    let corners = [
        (cx + hw, cy + hh, 0.0),
        (cx - hw, cy + hh, PI * 0.5),
        (cx - hw, cy - hh, PI),
        (cx + hw, cy - hh, PI * 1.5),
    ];
    let mut ring = Vec::with_capacity(64);
    for &(ox, oy, a0) in &corners {
        for k in 0..16 {
            let a = a0 + (PI * 0.5) * (k as f64 / 15.0);
            ring.push((ox + r * a.cos(), oy + r * a.sin()));
        }
    }
    Region::from_ring(&ring)
}

/// Smooth organic contour (`96` vertices, gentle low-frequency harmonics).
fn organic_island(cx: f64, cy: f64, r: f64, phase: f64) -> Region {
    let n = 96;
    let ring: Vec<Pt> = (0..n)
        .map(|k| {
            let a = TAU * (k as f64) / (n as f64);
            let w = 1.0
                + 0.11 * (2.0 * a + phase).sin()
                + 0.07 * (3.0 * a - phase * 0.7).cos()
                + 0.04 * (5.0 * a + 0.4).sin();
            (cx + r * w * a.cos(), cy + r * w * a.sin())
        })
        .collect();
    Region::from_ring(&ring)
}

/// Smooth organic island with an interior pond hole.
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
            (cx + 4.0 + r * 0.30 * w * a.cos(), cy - 2.0 + r * 0.28 * w * a.sin())
        })
        .collect();
    Region::from_polygons(&[vec![outer, hole]])
}

/// Clean 5-pointed star with generous interior width so patterns read clearly.
fn five_star(cx: f64, cy: f64, r_outer: f64, r_inner: f64) -> Region {
    let spikes = 5usize;
    let mut pts = Vec::with_capacity(spikes * 2);
    for i in 0..(spikes * 2) {
        let a = PI * 0.5 + PI * (i as f64) / (spikes as f64);
        let r = if i % 2 == 0 { r_outer } else { r_inner };
        pts.push((cx + r * a.cos(), cy + r * a.sin()));
    }
    Region::from_ring(&pts)
}

/// Standard 52 × 52 mm test shapes used across sheets and strips.
struct GalleryShapes {
    square: Region,
    card: Region,
    ring: Region,
    island: Region,
    pond_island: Region,
    star: Region,
    archipelago: Region,
}

fn gallery_shapes() -> GalleryShapes {
    let square = Region::from_ring(&[(0.0, 0.0), (52.0, 0.0), (52.0, 52.0), (0.0, 52.0)]);
    let card = rounded_rect(26.0, 26.0, 52.0, 52.0, 5.0);
    let ring = Region::from_polygons(&[vec![
        circle_ring(26.0, 26.0, 26.0, 96),
        circle_ring(26.0, 26.0, 9.0, 64),
    ]]);
    let island = organic_island(26.0, 26.0, 25.0, 0.6);
    let pond_island = island_with_pond(26.0, 26.0, 25.0);
    let star = five_star(26.0, 26.0, 27.0, 13.5);
    let archipelago = {
        let main = organic_island(18.0, 27.0, 16.5, 0.3).all_rings_mm().into_iter().next().unwrap();
        let hole = circle_ring(18.0, 27.0, 5.2, 48);
        let islet = organic_island(42.0, 23.0, 10.0, 1.7).all_rings_mm().into_iter().next().unwrap();
        Region::from_polygons(&[vec![main, hole], vec![islet]])
    };
    GalleryShapes {
        square,
        card,
        ring,
        island,
        pond_island,
        star,
        archipelago,
    }
}

fn border_for(spacing: f64) -> f64 {
    spacing * 0.5
}

// ---------------------------------------------------------------------------
// Declarative Pattern Catalog
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Lines,
    Lattices,
    Scatter,
    Materials,
    Organic,
    Geology,
    Ground,
    Cad,
}

impl Category {
    const ALL: [Category; 8] = [
        Category::Lines,
        Category::Lattices,
        Category::Scatter,
        Category::Materials,
        Category::Organic,
        Category::Geology,
        Category::Ground,
        Category::Cad,
    ];

    fn title(self) -> &'static str {
        match self {
            Category::Lines => "Lines & Space-Filling Paths",
            Category::Lattices => "Lattices & Waves",
            Category::Scatter => "Scatter & Nature",
            Category::Materials => "Materials, Flow & Cellular Textures",
            Category::Organic => "Organic Contours & Differential Growth",
            Category::Geology => "Geology (Map Lithologies)",
            Category::Ground => "Ground & Weathered Surfaces",
            Category::Cad => "CAD Hatch Patterns (.pat)",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Category::Lines => "Scanline hatches, serpentine toolpaths, offset rings, spirals, and space-filling curves",
            Category::Lattices => "Regular geometric lattices, waves, scales, and radial rays",
            Category::Scatter => "Poisson-disk stippling and botanical motifs built on Canvas + Tools",
            Category::Materials => "Voronoi stone cells, streamlines, overlapping wave swells, and domain-warped veins",
            Category::Organic => "Metaball contour stacks, terrain isolines, and differential curve growth",
            Category::Geology => "Cartographic rock and lithology symbols on anchored cell-seeded grids",
            Category::Ground => "Soils, gravels, mud cracks, and weathered surface textures",
            Category::Cad => "Standard AutoCAD .pat line-family definitions",
        }
    }
}

struct Pattern {
    category: Category,
    name: &'static str,
    slug: &'static str,
    fill_key: &'static str,
    note: &'static str,
    scale: f64,
    has_angle: bool,
    default_angle: f64,
    preview_spacing: f64,
}

impl Pattern {
    fn render(&self, region: &Region, base_spacing: f64, angle: f64, inset: Option<f64>, fray: f64) -> Strokes {
        let s = (base_spacing * self.scale).max(0.25);
        let params = FillParams::new(s)
            .angle(angle)
            .seed(SEED)
            .anchor(Anchor::LOCAL)
            .inset(inset)
            .fray(fray);

        match self.fill_key {
            "diagonal" => fill_by_name("rectilinear", region, &params).unwrap_or_default(),
            "triangles" => {
                let target = match inset {
                    Some(d) if d > 0.0 => region.outset(s * 0.25 - d * 0.5),
                    _ => region.clone(),
                };
                let raw = fill_triangles(&target, s, angle, Anchor::LOCAL);
                if fray > 0.0 {
                    fray_strokes(&target, raw, s, fray, SEED)
                } else {
                    raw
                }
            }
            "sunburst" => {
                let target = inset.filter(|&d| d > 0.0).map_or_else(|| region.clone(), |d| region.inset(d));
                let raw = fill_sunburst(&target, s, angle, Anchor::LOCAL);
                if fray > 0.0 {
                    fray_strokes(&target, raw, s, fray, SEED)
                } else {
                    raw
                }
            }
            "dots_hex" => {
                let target = inset.filter(|&d| d > 0.0).map_or_else(|| region.clone(), |d| region.inset(d));
                let raw = fill_dots_with_lattice(&target, s, angle, s * 0.22, Lattice::Hexagonal, Anchor::LOCAL);
                if fray > 0.0 {
                    fray_strokes(&target, raw, s, fray, SEED)
                } else {
                    raw
                }
            }
            "sand" => fill_by_name("arena", region, &params).unwrap_or_default(),
            other => fill_by_name(other, region, &params)
                .unwrap_or_else(|| panic!("unknown fill_key: {other}")),
        }
    }
}

const fn p(
    category: Category,
    name: &'static str,
    slug: &'static str,
    fill_key: &'static str,
    scale: f64,
    has_angle: bool,
    default_angle: f64,
    preview_spacing: f64,
    note: &'static str,
) -> Pattern {
    Pattern {
        category,
        name,
        slug,
        fill_key,
        note,
        scale,
        has_angle,
        default_angle,
        preview_spacing,
    }
}

fn catalog() -> Vec<Pattern> {
    use Category::*;
    vec![
        // --- Lines & Space-Filling Paths ---
        p(Lines, "rectilinear", "rectilinear", "rectilinear", 1.0, true, 0.0, COARSE_MM, "parallel scanline hatch, one stroke per segment"),
        p(Lines, "diagonal", "diagonal", "diagonal", 1.0, true, 45.0, COARSE_MM, "rectilinear scanline hatch at 45°"),
        p(Lines, "zigzag", "zigzag", "zigzag", 1.0, true, 0.0, COARSE_MM, "serpentine hatch joined along the boundary"),
        p(Lines, "grid", "grid", "grid", 1.0, true, 45.0, COARSE_MM, "two orthogonal scanline hatches 90° apart"),
        p(Lines, "triangles", "triangles", "triangles", 1.5, true, 0.0, COARSE_MM, "three hatches at 60° meeting at shared vertices"),
        p(Lines, "concentric", "concentric", "concentric", 1.0, false, 0.0, COARSE_MM, "successive inward offset rings + medial axis"),
        p(Lines, "spiral", "spiral", "spiral", 1.0, false, 0.0, COARSE_MM, "offset rings linked into one continuous spiral per lobe"),
        p(Lines, "hilbert", "hilbert", "hilbert", 1.0, false, 0.0, COARSE_MM, "space-filling Hilbert curve visiting the interior"),

        // --- Lattices & Waves ---
        p(Lattices, "honeycomb", "honeycomb", "honeycomb", 1.0, true, 0.0, COARSE_MM, "hexagonal cell lattice, each shared wall drawn once"),
        p(Lattices, "dots", "dots", "dots", 1.0, true, 0.0, COARSE_MM, "short marks on a square lattice"),
        p(Lattices, "dots hex", "dots-hex", "dots_hex", 1.0, true, 0.0, COARSE_MM, "short marks on a hexagonal lattice"),
        p(Lattices, "waves", "waves", "waves", 1.0, true, 0.0, COARSE_MM, "parallel sinusoidal wave lines"),
        p(Lattices, "wave grid", "wave-grid", "wave_grid", 1.5, true, 90.0, COARSE_MM, "ogee: mirrored sine waves closing into curved diamonds"),
        p(Lattices, "scales", "scales", "scales", 1.5, true, 0.0, COARSE_MM, "staggered rows of semicircular scale arcs"),
        p(Lattices, "sunburst", "sunburst", "sunburst", 1.0, false, 0.0, COARSE_MM, "radial rays subdividing outward to keep constant pitch"),

        // --- Scatter & Nature ---
        p(Scatter, "stipple", "stipple", "stipple", 1.0, false, 0.0, COARSE_MM, "Poisson-disk stipple marks with minimum spacing"),
        p(Scatter, "stipple noise", "stipple-noise", "stipple_noise", 0.7, false, 0.0, COARSE_MM, "Poisson stipple modulated by low-frequency Perlin noise"),
        p(Scatter, "stipple edge", "stipple-edge", "stipple_edge", 0.65, false, 0.0, COARSE_MM, "cartographic water/coast stipple dense near the perimeter"),
        p(Scatter, "pebbles", "pebbles", "pebbles", 1.2, false, 0.0, COARSE_MM, "smooth rounded pebble rings on a Poisson grid"),
        p(Scatter, "sprouts", "sprouts", "sprouts", 1.5, false, 0.0, COARSE_MM, "staggered rows of short curved blades and V-sprouts"),
        p(Scatter, "fluid grass", "fluid-grass", "fluid_grass", 1.35, false, 0.0, COARSE_MM, "single blades, V-tufts, and fans aligned to a smooth vector field"),
        p(Scatter, "tufts", "tufts", "tufts", 1.15, true, 0.0, COARSE_MM, "organic bands of 1- and 2-blade grass tufts"),
        p(Scatter, "twigs", "twigs", "twigs", 1.7, false, 0.0, COARSE_MM, "scattered branched sticks with kinked spines and side shoots"),
        p(Scatter, "vines", "vines", "vines", 1.4, true, 90.0, COARSE_MM, "meandering parallel stems with alternating side barbs"),
        p(Scatter, "lily pads", "lily-pads", "lily_pads", 1.4, false, 0.0, COARSE_MM, "boolean-unioned circular pads with centre dots"),
        p(Scatter, "canopy", "canopy", "canopy", 2.4, false, 0.0, COARSE_MM, "clustered wobbly foliage lobes with interior stipple"),
        p(Scatter, "grass", "grass", "grass", 2.0, true, 0.0, COARSE_MM, "repeating 3-blade tuft lattice"),
        p(Scatter, "scribble", "scribble", "scribble", 1.0, true, 30.0, COARSE_MM, "hand-drawn jittered and broken cross-hatching"),

        // --- Materials, Flow & Cellular Textures ---
        p(Materials, "gravel", "gravel", "gravel", 2.5, false, 0.0, COARSE_MM, "rounded Voronoi stones shrinking near the boundary"),
        p(Materials, "gravel clipped", "gravel-clipped", "gravel_clipped", 2.5, false, 0.0, COARSE_MM, "uniform rounded Voronoi stones clipped at the boundary"),
        p(Materials, "terrazzo", "terrazzo", "terrazzo", 3.8, false, 0.0, COARSE_MM, "non-overlapping angular chips with a third cross-hatched"),
        p(Materials, "marble", "marble", "marble", 3.6, true, 30.0, FINE_MM, "turbulent domain-warped veins drawn as paired edges"),
        p(Materials, "wood", "wood", "wood", 1.5, true, 0.0, MID_MM, "grain contours flowing around interior knots"),
        p(Materials, "cracks", "cracks", "cracks", 4.0, false, 0.0, COARSE_MM, "rounded cellular mud-crack tiles with wide grout"),
        p(Materials, "cell net", "cell-net", "cell_net", 1.8, false, 0.0, COARSE_MM, "single-line Voronoi crack network with kinked shared walls"),
        p(Materials, "eddies", "eddies", "eddies", 0.55, true, 0.0, COARSE_MM, "Jobard-Lefer streamlines through undulating currents and vortices"),
        p(Materials, "swells", "swells", "swells", 0.50, true, 0.0, COARSE_MM, "overlapping wave hills with horizon occlusion and fanning ribs"),
        p(Materials, "ripples", "ripples", "ripples", 0.52, true, 0.0, COARSE_MM, "broken undulating wave rows in a shared 2D swell field"),
        p(Materials, "truchet", "truchet", "truchet", 2.8, true, 0.0, COARSE_MM, "random quarter-circle arc tiles forming winding paths"),
        p(Materials, "chess", "chess", "chess", 3.8, true, 0.0, COARSE_MM, "checkerboard lattice with hatched dark squares"),

        // --- Organic Contours & Differential Growth ---
        p(Organic, "isoline blobs", "isoline-blobs", "isoline_blobs", 0.75, false, 0.0, COARSE_MM, "nested eccentric contour rings around scattered sites"),
        p(Organic, "isoline stack", "isoline-stack", "isoline_stack", 0.85, false, 0.0, COARSE_MM, "layered forest canopy with concentric domes and ruffled foliage blobs"),
        p(Organic, "growth", "growth", "growth", 0.65, false, 0.0, COARSE_MM, "differential-growth meandering and branching curves"),

        // --- Geology (Map Lithologies) ---
        p(Geology, "amphibolite", "amphibolite", "amphibolite", 1.6, false, 0.0, COARSE_MM, "continuous vertical zigzag foliation"),
        p(Geology, "arena", "arena", "arena", 0.9, false, 0.0, FINE_MM, "fine dot stipple"),
        p(Geology, "beds", "beds", "beds", 1.4, true, 0.0, COARSE_MM, "continuous wavy sedimentary horizons"),
        p(Geology, "bentonite", "bentonite", "benton", 1.4, true, 90.0, COARSE_MM, "aligned dashed columns"),
        p(Geology, "brackets", "brackets", "brackets", 1.2, false, 0.0, COARSE_MM, "orthogonal └ ┘ crystal marks"),
        p(Geology, "breccia", "breccia", "breccia", 2.2, false, 0.0, COARSE_MM, "angular Voronoi rock fragments"),
        p(Geology, "breccia blobs", "breccia-blobs", "breccia2", 2.0, false, 0.0, COARSE_MM, "rounded low-frequency clasts"),
        p(Geology, "coal", "coal", "coal", 1.2, true, 0.0, COARSE_MM, "dashed bedding seams"),
        p(Geology, "conglomerate", "conglomerate", "conglomerate", 1.4, false, 0.0, COARSE_MM, "round pebbles on a staggered grid"),
        p(Geology, "conglomerate flat", "conglomerate-flat", "conglomerate2", 1.4, false, 0.0, COARSE_MM, "horizontally flattened pebbles"),
        p(Geology, "coquina", "coquina", "coquina", 2.0, false, 0.0, COARSE_MM, "shell spirals in staggered rows"),
        p(Geology, "dacite", "dacite", "dacite", 0.8, true, 35.0, COARSE_MM, "short oriented crystal dashes"),
        p(Geology, "diabase", "diabase", "diabase", 0.6, true, -45.0, COARSE_MM, "dense parallel igneous hatch"),
        p(Geology, "diatom", "diatom", "diatom", 2.0, false, 0.0, COARSE_MM, "concentric circle + inner ring + dot motifs"),
        p(Geology, "diorite", "diorite", "diorite", 1.2, false, 0.0, COARSE_MM, "igneous T marks on a staggered grid"),
        p(Geology, "drift", "drift", "drift", 2.0, false, 0.0, COARSE_MM, "sparse tilted glacial pebbles"),
        p(Geology, "feldspar", "feldspar", "feldspar", 0.9, true, -45.0, COARSE_MM, "dense short crystal strokes"),
        p(Geology, "ferrous", "ferrous", "ferrous", 1.0, false, 0.0, COARSE_MM, "ironstone checkerboard cells"),
        p(Geology, "fill", "fill", "fill", 0.8, true, 45.0, COARSE_MM, "woven cross-hatch"),
        p(Geology, "gabbro", "gabbro", "gabbro", 1.2, false, 0.0, COARSE_MM, "igneous alternating V marks"),
        p(Geology, "gneiss", "gneiss", "gneiss", 1.6, true, 30.0, COARSE_MM, "metamorphic foliation dash rows"),
        p(Geology, "granite", "granite", "granite", 1.2, false, 0.0, COARSE_MM, "igneous + marks on a staggered grid"),
        p(Geology, "granite ×", "granite-x", "granite2", 1.2, false, 0.0, COARSE_MM, "igneous diagonal × grain"),
        p(Geology, "hornblende", "hornblende", "hornblende", 1.4, true, 40.0, COARSE_MM, "narrow oriented crystal capsules"),
        p(Geology, "laminae", "laminae", "laminae", 1.4, true, 15.0, COARSE_MM, "fine wavy laminations"),
        p(Geology, "lava", "lava", "lava", 1.4, false, 0.0, COARSE_MM, "vertical columnar bars of varying width"),
        p(Geology, "masonry", "masonry", "masonry", 1.0, false, 0.0, COARSE_MM, "running-bond stone coursing"),
        p(Geology, "ophitic", "ophitic", "ophitic", 0.9, true, -45.0, COARSE_MM, "plagioclase laths crossed by pyroxene grain"),
        p(Geology, "sand", "sand", "sand", 0.55, false, 0.0, FINE_MM, "dense fine sand grain"),
        p(Geology, "sandstone", "sandstone", "sandstone", 1.2, true, 0.0, COARSE_MM, "dot grain with short cross-bedding"),
        p(Geology, "shards", "shards", "shards", 2.0, false, 0.0, COARSE_MM, "sparse angular volcanic shards"),

        // --- Ground & Weathered Surfaces ---
        p(Ground, "chalky soil", "chalky-soil", "chalky_soil", 1.1, false, 0.0, COARSE_MM, "granular dots with an open cell net"),
        p(Ground, "clay soil", "clay-soil", "clay_soil", 0.7, true, 0.0, FINE_MM, "smooth broad clay waves"),
        p(Ground, "coarse stone", "coarse-stone", "coarse_stone", 1.6, false, 0.0, COARSE_MM, "large rounded block outlines with interior grain"),
        p(Ground, "compacted soil", "compacted-soil", "compacted_soil", 1.2, true, 0.0, FINE_MM, "tight thin waves pressed together"),
        p(Ground, "cracked earth", "cracked-earth", "cracked_earth", 1.4, false, 0.0, COARSE_MM, "fine dot grain under an angular crack network"),
        p(Ground, "debris soil", "debris-soil", "debris_soil", 1.2, false, 0.0, COARSE_MM, "angular rock chips and fine specks"),
        p(Ground, "dry cracked soil", "dry-cracked-soil", "dry_cracked", 1.1, false, 0.0, COARSE_MM, "wide mud-crack polygons with rounded corners"),
        p(Ground, "erosion lines", "erosion-lines", "erosion", 0.5, true, 0.0, FINE_MM, "fine low-amplitude parallel erosion waves"),
        p(Ground, "fine gravel", "fine-gravel", "fine_gravel", 1.5, false, 0.0, COARSE_MM, "sparse dots mixed with small pebble rings"),
        p(Ground, "grainy soil", "grainy-soil", "grainy_soil", 0.55, false, 0.0, FINE_MM, "dense fine dot field"),
        p(Ground, "gravelly soil", "gravelly-soil", "gravelly_soil", 1.0, false, 0.0, FINE_MM, "dense short dashes at scattered angles"),
        p(Ground, "layered soil", "layered-soil", "layered_soil", 1.1, true, 0.0, FINE_MM, "stacked soil horizons with straighter partings"),
        p(Ground, "loose soil", "loose-soil", "loose_soil", 1.1, false, 0.0, FINE_MM, "scattered rings and dots"),
        p(Ground, "mixed rocks", "mixed-rocks", "mixed_rocks", 1.6, false, 0.0, COARSE_MM, "stone cells across a wide size range"),
        p(Ground, "mulch", "mulch", "mulch", 0.7, true, 0.0, COARSE_MM, "staggered rows of short organic dashes"),
        p(Ground, "mulched soil", "mulched-soil", "mulched_soil", 1.1, true, 0.0, COARSE_MM, "finer, denser rows of short mulch dashes"),
        p(Ground, "organic topsoil", "organic-topsoil", "topsoil", 1.4, false, 0.0, FINE_MM, "multi-scale dot grain in three size classes"),
        p(Ground, "pebble scatter", "pebble-scatter", "pebble_scatter", 1.7, false, 0.0, COARSE_MM, "well-spaced tilted oval pebbles"),
        p(Ground, "pebbled soil", "pebbled-soil", "pebbled_soil", 0.85, false, 0.0, COARSE_MM, "sparse small squares and diamonds"),
        p(Ground, "sandy soil", "sandy-soil", "sandy_soil", 1.2, false, 0.0, FINE_MM, "fine dense sandy dot grain"),
        p(Ground, "silty soil", "silty-soil", "silty_soil", 2.0, false, 0.0, COARSE_MM, "small tight polygonal silt cells"),
        p(Ground, "stone cluster", "stone-cluster", "stone_cluster", 1.0, false, 0.0, COARSE_MM, "tightly packed varied pebbles"),

        // --- CAD Hatch Patterns ---
        p(Cad, "basket", "basket", "pat:BASKET", 1.0, true, 0.0, COARSE_MM, "CAD .pat basket weave"),
        p(Cad, "bricks", "bricks", "pat:BRICK", 2.0, true, 0.0, COARSE_MM, "CAD .pat running bond brick"),
        p(Cad, "herringbone", "herringbone", "pat:HERRINGBONE", 2.0, true, 45.0, COARSE_MM, "CAD .pat herringbone"),
    ]
}

// ---------------------------------------------------------------------------
// SVG Rendering Primitives
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum WallStyle {
    Solid,
    DashedGuide,
    None,
}

struct Cell<'a> {
    region: &'a Region,
    strokes: Strokes,
    wall: WallStyle,
    bg_strokes: Option<Strokes>,
    mask_region: Option<Region>,
}

impl<'a> Cell<'a> {
    fn solid(region: &'a Region, strokes: Strokes) -> Self {
        Self {
            region,
            strokes,
            wall: WallStyle::Solid,
            bg_strokes: None,
            mask_region: None,
        }
    }

    fn guide(region: &'a Region, strokes: Strokes) -> Self {
        Self {
            region,
            strokes,
            wall: WallStyle::DashedGuide,
            bg_strokes: None,
            mask_region: None,
        }
    }

    fn ink_only(region: &'a Region, strokes: Strokes) -> Self {
        Self {
            region,
            strokes,
            wall: WallStyle::None,
            bg_strokes: None,
            mask_region: None,
        }
    }
}

fn append_cell(svg: &mut String, cell: &Cell, ox: f64, oy: f64, cell_w: f64, cell_h: f64, y_down: bool) {
    let (minx, miny, maxx, maxy) = cell.region.bounds_mm();
    let pad = 7.5;
    let w = (maxx - minx).max(1e-6);
    let h = (maxy - miny).max(1e-6);
    let scale = ((cell_w - 2.0 * pad) / w).min((cell_h - 2.0 * pad) / h);
    let cx = (minx + maxx) * 0.5;
    let cy = (miny + maxy) * 0.5;
    let sx = ox + cell_w * 0.5;
    let sy = oy + cell_h * 0.5;
    let xform = format!(
        "translate({sx:.2},{sy:.2}) scale({scale:.4},{neg:.4}) translate({tcx:.2},{tcy:.2})",
        neg = if y_down { scale } else { -scale },
        tcx = -cx,
        tcy = -cy,
    );
    let sw_fill = 0.25 / scale;
    let sw_wall = 0.30 / scale;

    // Optional mask polygon highlight (for edges-and-masks showcase)
    if let Some(ref mask) = cell.mask_region {
        svg.push_str(&format!(
            "<g transform='{xform}' fill='{MASK_FILL}' fill-rule='evenodd' stroke='{MASK_STROKE}' stroke-width='{:.4}' stroke-linejoin='round'>\n",
            0.22 / scale
        ));
        let mut d = String::new();
        for ring in mask.all_rings_mm() {
            if ring.len() < 3 {
                continue;
            }
            let pts = ring
                .iter()
                .map(|p| format!("{:.3},{:.3}", p.0, p.1))
                .collect::<Vec<_>>()
                .join(" L ");
            d.push_str(&format!("M {pts} Z "));
        }
        if !d.is_empty() {
            svg.push_str(&format!("<path d='{d}'/>\n"));
        }
        svg.push_str("</g>\n");
    }

    // Optional background layer strokes (for multi-layer knockout showcase)
    if let Some(ref bg) = cell.bg_strokes {
        svg.push_str(&format!(
            "<g transform='{xform}' fill='none' stroke='{BG_INK}' stroke-width='{:.4}' stroke-linecap='round' stroke-linejoin='round'>\n",
            0.20 / scale
        ));
        for pts in bg.iter().filter(|p| p.len() >= 2) {
            let d = pts
                .iter()
                .map(|p| format!("{:.3},{:.3}", p.0, p.1))
                .collect::<Vec<_>>()
                .join(" L ");
            svg.push_str(&format!("<path d='M {d}'/>\n"));
        }
        svg.push_str("</g>\n");
    }

    // Boundary wall (solid, dashed guide, or hidden)
    match cell.wall {
        WallStyle::Solid => {
            svg.push_str(&format!(
                "<g transform='{xform}' fill='none' stroke='{WALL}' stroke-width='{sw_wall:.4}' stroke-linejoin='round'>\n"
            ));
            for ring in cell.region.all_rings_mm() {
                if ring.len() < 2 {
                    continue;
                }
                let d = ring
                    .iter()
                    .map(|p| format!("{:.3},{:.3}", p.0, p.1))
                    .collect::<Vec<_>>()
                    .join(" L ");
                svg.push_str(&format!("<path d='M {d} Z'/>\n"));
            }
            svg.push_str("</g>\n");
        }
        WallStyle::DashedGuide => {
            let dash = format!("{:.2},{:.2}", 1.4 / scale, 1.1 / scale);
            svg.push_str(&format!(
                "<g transform='{xform}' fill='none' stroke='{GUIDE}' stroke-width='{sw_wall:.4}' stroke-dasharray='{dash}' stroke-linejoin='round'>\n"
            ));
            for ring in cell.region.all_rings_mm() {
                if ring.len() < 2 {
                    continue;
                }
                let d = ring
                    .iter()
                    .map(|p| format!("{:.3},{:.3}", p.0, p.1))
                    .collect::<Vec<_>>()
                    .join(" L ");
                svg.push_str(&format!("<path d='M {d} Z'/>\n"));
            }
            svg.push_str("</g>\n");
        }
        WallStyle::None => {}
    }

    // Foreground pattern strokes
    svg.push_str(&format!(
        "<g transform='{xform}' fill='none' stroke='{INK}' stroke-width='{sw_fill:.4}' stroke-linecap='round' stroke-linejoin='round'>\n"
    ));
    for pts in cell.strokes.iter().filter(|p| p.len() >= 2) {
        let d = pts
            .iter()
            .map(|p| format!("{:.3},{:.3}", p.0, p.1))
            .collect::<Vec<_>>()
            .join(" L ");
        svg.push_str(&format!("<path d='M {d}'/>\n"));
    }
    svg.push_str("</g>\n");
}

fn render_sheet(title: &str, note: &str, columns: &[&str], rows: &[(String, Vec<Cell>)]) -> String {
    render_sheet_y(title, note, columns, rows, false)
}

fn render_sheet_y(
    title: &str,
    note: &str,
    columns: &[&str],
    rows: &[(String, Vec<Cell>)],
    y_down: bool,
) -> String {
    let n_cols = columns.len() as f64;
    let n_rows = rows.len() as f64;
    let longest = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0) as f64;
    let label_w = LABEL_W.max(16.0 + longest * 4.8 + 14.0);
    let width = label_w + n_cols * CELL_W + 20.0;
    let height = TITLE_H + HEADER_H + n_rows * CELL_H + 16.0;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 0 {width} {height}'>\n"
    ));
    svg.push_str(&format!("<rect width='100%' height='100%' fill='{PAPER}'/>\n"));
    svg.push_str(&format!(
        "<style>text{{font-family:Georgia,serif;fill:{LABEL}}}.title{{fill:{HEADING};font-weight:bold}}</style>\n"
    ));
    svg.push_str(&format!(
        "<text x='14' y='18' font-size='12' class='title'>{title}</text>\n"
    ));
    svg.push_str(&format!("<text x='14' y='32' font-size='8.5'>{note}</text>\n"));

    for (i, name) in columns.iter().enumerate() {
        let x = label_w + (i as f64 + 0.5) * CELL_W;
        let y = TITLE_H + HEADER_H - 7.0;
        svg.push_str(&format!(
            "<text x='{x}' y='{y}' font-size='8.5' text-anchor='middle'>{name}</text>\n"
        ));
    }

    for (ri, (label, cells)) in rows.iter().enumerate() {
        let y = TITLE_H + HEADER_H + (ri as f64 + 0.5) * CELL_H + 3.0;
        svg.push_str(&format!("<text x='14' y='{y}' font-size='8.5'>{label}</text>\n"));
        for (ci, cell) in cells.iter().enumerate() {
            let ox = label_w + ci as f64 * CELL_W;
            let oy = TITLE_H + HEADER_H + ri as f64 * CELL_H;
            append_cell(&mut svg, cell, ox, oy, CELL_W, CELL_H, y_down);
        }
    }

    svg.push_str("</svg>\n");
    svg
}

// ---------------------------------------------------------------------------
// Per-Pattern Sheet: Square · Ring (Hole) · Frayed (Guide) · Frayed (Ink Only)
// ---------------------------------------------------------------------------

const PATTERN_SHEET_COLUMNS: [&str; 4] = [
    "square (unfrayed)",
    "ring + hole (unfrayed)",
    "frayed (with guide)",
    "frayed (ink only)",
];

fn pattern_sheet_rows<'a>(
    pattern: &Pattern,
    shapes: &'a GalleryShapes,
) -> Vec<(String, Vec<Cell<'a>>)> {
    let mut styles = vec![(FINE_MM, pattern.default_angle)];
    if pattern.has_angle {
        let last = if pattern.slug == "scales" { 90.0 } else { 45.0 };
        styles.push((MID_MM, pattern.default_angle + 20.0));
        styles.push((COARSE_MM, pattern.default_angle + last));
    } else {
        styles.push((MID_MM, pattern.default_angle));
        styles.push((COARSE_MM, pattern.default_angle));
    }

    styles
        .into_iter()
        .map(|(spacing, angle)| {
            let inset = border_for(spacing);
            let label = if pattern.has_angle {
                format!("{spacing:.1} mm · {angle:.0}°")
            } else {
                format!("{spacing:.1} mm")
            };
            let s_sq = pattern.render(&shapes.square, spacing, angle, Some(inset), 0.0);
            let s_ring = pattern.render(&shapes.ring, spacing, angle, Some(inset), 0.0);
            let s_fray = pattern.render(&shapes.island, spacing, angle, None, FRAY_MM);
            let cells = vec![
                Cell::solid(&shapes.square, s_sq),
                Cell::solid(&shapes.ring, s_ring),
                Cell::guide(&shapes.island, s_fray.clone()),
                Cell::ink_only(&shapes.island, s_fray),
            ];
            (label, cells)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Categorized Visual Reference Sheet (`gallery.svg`) & Alphabetical Index
// ---------------------------------------------------------------------------

fn catalog_spacing_for(p: &Pattern) -> f64 {
    if p.preview_spacing <= FINE_MM {
        MID_MM * 1.15
    } else {
        p.preview_spacing
    }
}

/// Categorized visual catalog (`examples/gallery/gallery.svg`): readable 7-column
/// layout grouped by pattern family with clear headings, crisp square + frayed
/// preview cards, and pattern labels.
fn render_categorized_gallery(patterns: &[Pattern], shapes: &GalleryShapes) -> String {
    const COLS: usize = 7;
    const CARD_W: f64 = 126.0;
    const CARD_H: f64 = 100.0;
    const PAD_X: f64 = 18.0;
    const SEC_HEADER_H: f64 = 38.0;

    let width = PAD_X * 2.0 + COLS as f64 * CARD_W;
    let mut total_h = 56.0;
    for cat in Category::ALL {
        let count = patterns.iter().filter(|p| p.category == cat).count();
        if count == 0 {
            continue;
        }
        let rows = (count + COLS - 1) / COLS;
        total_h += SEC_HEADER_H + rows as f64 * CARD_H + 14.0;
    }

    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{total_h}' viewBox='0 0 {width} {total_h}'>\n\
         <rect width='100%' height='100%' fill='{PAPER}'/>\n\
         <style>text{{font-family:Georgia,serif;fill:{LABEL}}}.h1{{fill:{HEADING};font-weight:bold;font-size:15px}}.h2{{fill:{HEADING};font-weight:bold;font-size:11.5px}}.sub{{font-size:8.5px}}.name{{fill:{HEADING};font-size:8.5px;font-weight:bold}}.meta{{font-size:7px}}</style>\n"
    );

    svg.push_str(&format!(
        "<text x='{PAD_X}' y='24' class='h1'>penfill — pattern catalog</text>\n\
         <text x='{PAD_X}' y='39' class='sub'>Every built-in fill grouped by family · left: crisp square · right: frayed organic perimeter (fray = {FRAY_MM:.1} mm)</text>\n"
    ));

    let mut y_cursor = 52.0;
    for cat in Category::ALL {
        let group: Vec<&Pattern> = patterns.iter().filter(|p| p.category == cat).collect();
        if group.is_empty() {
            continue;
        }
        svg.push_str(&format!(
            "<line x1='{PAD_X}' y1='{:.1}' x2='{:.1}' y2='{:.1}' stroke='{WALL}' stroke-width='0.6'/>\n\
             <text x='{PAD_X}' y='{:.1}' class='h2'>{}</text>\n\
             <text x='{PAD_X}' y='{:.1}' class='sub'>{}</text>\n",
            y_cursor,
            width - PAD_X,
            y_cursor,
            y_cursor + 16.0,
            cat.title(),
            y_cursor + 28.0,
            cat.subtitle(),
        ));
        y_cursor += SEC_HEADER_H;

        for (idx, pat) in group.iter().enumerate() {
            let col = idx % COLS;
            let row = idx / COLS;
            let cx = PAD_X + col as f64 * CARD_W;
            let cy = y_cursor + row as f64 * CARD_H;

            svg.push_str(&format!(
                "<rect x='{:.1}' y='{:.1}' width='{:.1}' height='{:.1}' rx='4' fill='{CARD_BG}' stroke='#e4ddd3' stroke-width='0.6'/>\n",
                cx + 3.0,
                cy + 2.0,
                CARD_W - 6.0,
                CARD_H - 6.0
            ));

            let spacing = catalog_spacing_for(pat);
            let sq_strokes = pat.render(
                &shapes.square,
                spacing,
                pat.default_angle,
                Some(border_for(spacing)),
                0.0,
            );
            let fray_strokes = pat.render(&shapes.island, spacing, pat.default_angle, None, FRAY_MM);

            let left_cell = Cell::solid(&shapes.square, sq_strokes);
            let right_cell = Cell::guide(&shapes.island, fray_strokes);
            append_cell(&mut svg, &left_cell, cx + 3.0, cy + 3.0, 60.0, 66.0, false);
            append_cell(&mut svg, &right_cell, cx + 61.0, cy + 3.0, 60.0, 66.0, false);

            svg.push_str(&format!(
                "<text x='{:.1}' y='{:.1}' class='name' text-anchor='middle'>{}</text>\n\
                 <text x='{:.1}' y='{:.1}' class='meta' text-anchor='middle'>{}</text>\n",
                cx + CARD_W * 0.5,
                cy + 78.0,
                pat.name,
                cx + CARD_W * 0.5,
                cy + 88.0,
                truncate_note(pat.note, 30),
            ));
        }

        let rows = (group.len() + COLS - 1) / COLS;
        y_cursor += rows as f64 * CARD_H + 14.0;
    }

    svg.push_str("</svg>\n");
    svg
}

fn truncate_note(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars - 1).collect();
        out.push('…');
        out
    }
}

/// Compact alphabetical index grid (`examples/gallery/all-patterns.svg`).
fn render_alphabetical_grid(patterns: &[Pattern], region: &Region) -> String {
    const COLS: usize = 10;
    const W: f64 = 78.0;
    const H_PAT: f64 = 72.0;
    const H_LBL: f64 = 20.0;

    let mut sorted: Vec<&Pattern> = patterns.iter().collect();
    sorted.sort_by_key(|p| p.name.to_lowercase());

    let rows = (sorted.len() + COLS - 1) / COLS;
    let width = W * COLS as f64 + 16.0;
    let row_h = H_PAT + H_LBL;
    let height = row_h * rows as f64 + 44.0;

    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {width} {height}' width='{width}' height='{height}'>\n\
         <rect width='100%' height='100%' fill='{PAPER}'/>\n\
         <style>text{{font-family:Georgia,serif;fill:{LABEL}}}.title{{fill:{HEADING};font-weight:bold}}</style>\n\
         <text x='12' y='18' font-size='12' class='title'>penfill — alphabetical index ({} patterns)</text>\n\
         <text x='12' y='31' font-size='8.5'>Sorted by name · see gallery.svg for categorized reference with frayed perimeters</text>\n",
        sorted.len()
    );

    for (idx, pat) in sorted.iter().enumerate() {
        let col = idx % COLS;
        let row = idx / COLS;
        let ox = 8.0 + col as f64 * W;
        let oy = 38.0 + row as f64 * row_h;
        let spacing = catalog_spacing_for(pat);
        let strokes = pat.render(
            region,
            spacing,
            pat.default_angle,
            Some(border_for(spacing)),
            0.0,
        );
        let cell = Cell::solid(region, strokes);
        append_cell(&mut svg, &cell, ox, oy, W, H_PAT, false);
        svg.push_str(&format!(
            "<text x='{:.1}' y='{:.1}' font-size='7.8' text-anchor='middle'>{}</text>\n",
            ox + W * 0.5,
            oy + H_PAT + 11.0,
            pat.name,
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

// ---------------------------------------------------------------------------
// Dedicated Feature Sheets: Fraying (`fray.svg`) & EdgeMode / Masks (`edges-and-masks.svg`)
// ---------------------------------------------------------------------------

const FRAY_COLUMNS: [&str; 4] = [
    "fray = 0 mm (hard border)",
    "fray = 4.5 mm (subtle fade)",
    "fray = 9.0 mm (with guide)",
    "fray = 9.0 mm (ink only)",
];

fn fray_showcase_rows<'a>(
    patterns: &[Pattern],
    shapes: &'a GalleryShapes,
) -> Vec<(String, Vec<Cell<'a>>)> {
    let featured = [
        ("zigzag", MID_MM, "continuous line"),
        ("waves", MID_MM, "continuous wave"),
        ("swells", COARSE_MM, "layered swells"),
        ("eddies", COARSE_MM, "flow streamlines"),
        ("dots-hex", MID_MM, "discrete marks"),
        ("stipple", MID_MM, "Poisson marks"),
        ("tufts", MID_MM, "botanical tufts"),
        ("twigs", MID_MM, "branched twigs"),
        ("gravel", MID_MM, "Voronoi stones"),
        ("honeycomb", COARSE_MM, "hex lattice"),
        ("cell-net", MID_MM, "polygonal crack net"),
        ("canopy", MID_MM, "foliage lobes"),
    ];

    featured
        .into_iter()
        .map(|(slug, spacing, kind)| {
            let pat = patterns
                .iter()
                .find(|p| p.slug == slug)
                .unwrap_or_else(|| panic!("missing pattern {slug}"));
            let s0 = pat.render(&shapes.island, spacing, pat.default_angle, None, 0.0);
            let s4 = pat.render(&shapes.island, spacing, pat.default_angle, None, 4.5);
            let s9 = pat.render(&shapes.island, spacing, pat.default_angle, None, 9.0);
            let cells = vec![
                Cell::solid(&shapes.island, s0),
                Cell::guide(&shapes.island, s4),
                Cell::guide(&shapes.island, s9.clone()),
                Cell::ink_only(&shapes.island, s9),
            ];
            (format!("{} ({kind})", pat.name), cells)
        })
        .collect()
}

const EDGE_MASK_COLUMNS: [&str; 5] = [
    "EdgeMode::Clip",
    "EdgeMode::Inside",
    "EdgeMode::Overflow(3.5)",
    "FillOutput.mask",
    "Layer Knockout",
];

fn edge_and_mask_rows<'a>(shapes: &'a GalleryShapes) -> Vec<(String, Vec<Cell<'a>>)> {
    let region = &shapes.pond_island;
    let bg_box = region.outset(4.5);
    let specs: [(&str, &str, f64); 5] = [
        ("canopy", "canopy", 5.2),
        ("gravel", "gravel", 4.6),
        ("isoline stack", "isoline_stack", 1.8),
        ("lily pads", "lily_pads", 2.8),
        ("tufts (frayed)", "tufts", 2.2),
    ];

    specs
        .into_iter()
        .map(|(label, name, spacing)| {
            let fray = if name == "tufts" { 6.5 } else { 0.0 };
            let base = FillParams::new(spacing).seed(SEED).fray(fray);
            let s_clip = fill_by_name(name, region, &base.edge(EdgeMode::Clip)).unwrap();
            let s_inside = fill_by_name(name, region, &base.edge(EdgeMode::Inside)).unwrap();
            let out_over: FillOutput = fill_output_by_name(
                name,
                region,
                &base.edge(if fray > 0.0 {
                    EdgeMode::Clip
                } else {
                    EdgeMode::Overflow(3.5)
                }),
            )
            .unwrap();

            // Multi-layer knockout: cut `out_over.mask` out of a background hatch!
            let knocked_region = bg_box.difference(&out_over.mask.outset(0.35));
            let bg_hatch = fill_rectilinear(&knocked_region, 1.3, 35.0, Some(0.4), Anchor::GLOBAL);

            let mask_cell = Cell {
                region,
                strokes: out_over.strokes.clone(),
                wall: WallStyle::DashedGuide,
                bg_strokes: None,
                mask_region: Some(out_over.mask),
            };
            let knockout_cell = Cell {
                region,
                strokes: out_over.strokes.clone(),
                wall: WallStyle::DashedGuide,
                bg_strokes: Some(bg_hatch),
                mask_region: None,
            };

            let cells = vec![
                Cell::solid(region, s_clip),
                Cell::solid(region, s_inside),
                Cell::guide(region, out_over.strokes),
                mask_cell,
                knockout_cell,
            ];
            (label.to_string(), cells)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// CAD, Custom, Dither & Placement Sheets
// ---------------------------------------------------------------------------

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

fn cad_rows<'a>(shapes: &'a GalleryShapes) -> Vec<(String, Vec<Cell<'a>>)> {
    PAT_BUILTIN
        .iter()
        .map(|&name| {
            let pattern = builtin(name).unwrap();
            let scale = match name {
                "PARQUET" => 1.0,
                "BASKET" => 1.3,
                "ANSI31" | "ANSI37" | "GRATE" => 2.0,
                "EARTH" => 3.2,
                _ => 1.6,
            };
            let s_sq = fill_pat(&shapes.square.inset(0.8), &pattern, scale, 0.0, Anchor::LOCAL);
            let s_ring = fill_pat(&shapes.ring.inset(0.8), &pattern, scale, 0.0, Anchor::LOCAL);
            let s_fray = fray_strokes(
                &shapes.island,
                fill_pat(&shapes.island, &pattern, scale, 0.0, Anchor::LOCAL),
                scale,
                FRAY_MM,
                SEED,
            );
            let cells = vec![
                Cell::solid(&shapes.square, s_sq),
                Cell::solid(&shapes.ring, s_ring),
                Cell::guide(&shapes.island, s_fray.clone()),
                Cell::ink_only(&shapes.island, s_fray),
            ];
            (name.to_lowercase(), cells)
        })
        .collect()
}

fn custom_rows<'a>(shapes: &'a GalleryShapes) -> Vec<(String, Vec<Cell<'a>>)> {
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let big = parse_svg(FLOWER_SVG).unwrap();
    let test_regions = [&shapes.square, &shapes.ring, &shapes.pond_island, &shapes.star];

    type CustomFn = Box<dyn Fn(&Region) -> Strokes>;
    let examples: Vec<(&str, CustomFn)> = vec![
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
                let src = [
                    (minx, miny),
                    (maxx, miny + (maxy - miny) * 0.4),
                    ((minx + maxx) * 0.5, maxy),
                ];
                fill_isolines(&r.inset(0.8), 0.25, &levels_every(-2.5, 2.5, 1.0), move |p| {
                    src.iter().map(|s| ((p.0 - s.0).hypot(p.1 - s.1) / 1.5).sin()).sum()
                })
            }),
        ),
        (
            "profile: sawtooth",
            Box::new(|r: &Region| {
                fill_profile(&r.inset(0.8), 2.5, 20.0, Anchor::LOCAL, |x| {
                    (x / 4.0).rem_euclid(1.0) * 1.8
                })
            }),
        ),
        (
            "svg tile",
            Box::new(move |r: &Region| {
                fill_svg_tile(&r.inset(0.8), &flower, 10.0, 15.0, 0.5, Anchor::LOCAL)
            }),
        ),
        (
            "svg clipped as is",
            Box::new(move |r: &Region| {
                let (minx, miny, maxx, maxy) = r.bounds_mm();
                let size = (maxx - minx).max(maxy - miny) * 1.15;
                let art = big.fit_width(size);
                let (ox, oy) = ((minx + maxx - size) * 0.5, (miny + maxy - size) * 0.5);
                let placed: Strokes = art
                    .strokes
                    .iter()
                    .map(|s| s.iter().map(|p| (p.0 + ox, p.1 + oy)).collect())
                    .collect();
                r.clip_strokes(&placed)
            }),
        ),
    ];

    examples
        .into_iter()
        .map(|(label, f)| {
            let cells = test_regions.iter().map(|&r| Cell::solid(r, f(r))).collect();
            (label.to_string(), cells)
        })
        .collect()
}

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
            let mut v = if y < 0.7 {
                0.92 - 0.25 * y
            } else {
                0.62 - 0.5 * (y - 0.7)
            };
            let (sx, sy) = ((x - cx - 0.12) / 0.34, (y - 0.78) / 0.07);
            v *= 1.0 - 0.55 * (-(sx * sx + sy * sy)).exp();
            let (dx, dy) = ((x - cx) / r, (y - cy) / r);
            let d2 = dx * dx + dy * dy;
            if d2 < 1.0 {
                let dz = (1.0 - d2).sqrt();
                let lambert = (dx * light.0 + dy * light.1 + dz * light.2).max(0.0);
                let spec = lambert.powi(40);
                v = (0.06 + 0.8 * lambert + 0.5 * spec).min(1.0);
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

const DITHER_COLUMNS: [&str; 5] = [
    "dots",
    "lines",
    "lines at 45°",
    "lines 1 mm apart",
    "clipped by a star",
];

fn dither_rows<'a>(shapes: &'a GalleryShapes) -> Vec<(String, Vec<Cell<'a>>)> {
    let img = picture().flipped();
    DITHER_METHODS
        .iter()
        .map(|&(label, method)| {
            let base = DitherOptions {
                method,
                seed: SEED,
                ..DitherOptions::new(0.55)
            };
            let variants = [
                (&shapes.square, DitherOptions { marks: Marks::Dots, ..base }),
                (&shapes.square, base),
                (&shapes.square, DitherOptions { angle: 45.0, ..base }),
                (
                    &shapes.square,
                    DitherOptions {
                        resolution: 0.35,
                        line_spacing: 1.1,
                        ..base
                    },
                ),
                (
                    &shapes.star,
                    DitherOptions {
                        line_spacing: 0.75,
                        ..base
                    },
                ),
            ];
            let cells = variants
                .into_iter()
                .map(|(region, o)| Cell::solid(region, fill_dither(region, &img, &o)))
                .collect();
            (label.to_string(), cells)
        })
        .collect()
}

const PLACEMENTS: [(&str, &str, &str, &str); 6] = [
    ("cover", "cover", "center", "no-repeat"),
    ("contain", "contain", "center", "no-repeat"),
    ("fill (stretched)", "fill", "center", "no-repeat"),
    ("14 mm, repeat", "14", "center", "repeat"),
    ("16 mm, repeat-x, top", "16", "top", "repeat-x"),
    ("24 mm, top left", "24", "top left", "no-repeat"),
];

fn placement_rows<'a>(shapes: &'a GalleryShapes) -> Vec<(String, Vec<Cell<'a>>)> {
    let img = picture();
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let regions = [&shapes.square, &shapes.ring, &shapes.pond_island, &shapes.star];
    let mut rows: Vec<(String, Vec<Cell>)> = PLACEMENTS
        .iter()
        .map(|&(label, size, position, repeat)| {
            let o = DitherOptions {
                placement: Placement::parse(size, position, repeat).unwrap(),
                ..DitherOptions::new(0.45)
            };
            let cells = regions
                .iter()
                .map(|&region| Cell::solid(region, fill_dither(region, &img, &o)))
                .collect();
            (format!("picture · {label}"), cells)
        })
        .collect();
    for (label, size, position, repeat) in [
        ("contain", "contain", "center", "no-repeat"),
        ("10 mm, repeat", "10", "center", "repeat"),
        ("12 mm, repeat-y, left", "12", "left", "repeat-y"),
    ] {
        let place = Placement::parse(size, position, repeat).unwrap();
        let cells = regions
            .iter()
            .map(|&region| Cell::solid(region, fill_svg_placed(region, &flower, &place)))
            .collect();
        rows.push((format!("svg · {label}"), cells));
    }
    rows
}

fn picture_data_uri(img: &Greyscale) -> String {
    let luma = image::GrayImage::from_fn(img.width() as u32, img.height() as u32, |x, y| {
        image::Luma([(img.pixel(x as usize, y as usize) * 255.0).round() as u8])
    });
    let mut png = Vec::new();
    luma.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("data:image/png;base64,");
    for chunk in png.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | (b[2] as u32);
        for k in 0..4 {
            out.push(if k <= chunk.len() {
                B64[(n >> (18 - 6 * k) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// README Strips (`docs/img/*.svg`)
// ---------------------------------------------------------------------------

const STRIP_W: f64 = 124.0;
const STRIP_H: f64 = 114.0;
const STRIP_LABEL: f64 = 20.0;
const STRIP_INK: f64 = 0.78;
const STRIP_WALL: f64 = 0.85;

struct StripCell<'a> {
    label: String,
    region: &'a Region,
    strokes: Strokes,
    wall: WallStyle,
    bg_strokes: Option<Strokes>,
    mask_region: Option<Region>,
}

impl<'a> StripCell<'a> {
    fn solid(label: impl Into<String>, region: &'a Region, strokes: Strokes) -> Self {
        Self {
            label: label.into(),
            region,
            strokes,
            wall: WallStyle::Solid,
            bg_strokes: None,
            mask_region: None,
        }
    }

    fn guide(label: impl Into<String>, region: &'a Region, strokes: Strokes) -> Self {
        Self {
            label: label.into(),
            region,
            strokes,
            wall: WallStyle::DashedGuide,
            bg_strokes: None,
            mask_region: None,
        }
    }
}

fn render_strip(cells: &[StripCell]) -> String {
    render_strip_y(cells, &[INK], false)
}

fn render_strip_y(cells: &[StripCell], inks: &[&str], y_down: bool) -> String {
    let w = STRIP_W * cells.len() as f64;
    let h = STRIP_H + STRIP_LABEL;
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w} {h}' width='{w2}' height='{h2}'>\n\
         <rect width='{w}' height='{h}' fill='{PAPER}'/>\n",
        w2 = w * 2.0,
        h2 = h * 2.0,
    );
    let pt = |p: &Pt| format!("{:.2},{:.2}", p.0, p.1);

    for (i, cell) in cells.iter().enumerate() {
        let ox = i as f64 * STRIP_W;
        let (minx, miny, maxx, maxy) = cell.region.bounds_mm();
        let pad = 8.0;
        let scale = ((STRIP_W - 2.0 * pad) / (maxx - minx).max(1e-6))
            .min((STRIP_H - 2.0 * pad) / (maxy - miny).max(1e-6));
        let xform = format!(
            "translate({:.2},{:.2}) scale({scale:.4},{:.4}) translate({:.3},{:.3})",
            ox + STRIP_W * 0.5,
            STRIP_H * 0.5,
            if y_down { scale } else { -scale },
            -(minx + maxx) * 0.5,
            -(miny + maxy) * 0.5,
        );

        svg.push_str(&format!(
            "<g transform='{xform}' fill='none' stroke-linecap='round' stroke-linejoin='round'>\n"
        ));

        if let Some(ref mask) = cell.mask_region {
            svg.push_str(&format!(
                "<g fill='{MASK_FILL}' fill-rule='evenodd' stroke='{MASK_STROKE}' stroke-width='{:.4}'>\n",
                0.65 / scale
            ));
            let mut d = String::new();
            for ring in mask.all_rings_mm() {
                if ring.len() >= 3 {
                    let pts: Vec<String> = ring.iter().map(pt).collect();
                    d.push_str(&format!("M{}Z ", pts.join("L")));
                }
            }
            if !d.is_empty() {
                svg.push_str(&format!("<path d='{d}'/>\n"));
            }
            svg.push_str("</g>\n");
        }

        if let Some(ref bg) = cell.bg_strokes {
            svg.push_str(&format!(
                "<g stroke='{BG_INK}' stroke-width='{:.4}'>\n",
                (STRIP_INK * 0.72) / scale
            ));
            for s in bg.iter().filter(|p| p.len() >= 2) {
                let d: Vec<String> = s.iter().map(pt).collect();
                svg.push_str(&format!("<path d='M{}'/>\n", d.join("L")));
            }
            svg.push_str("</g>\n");
        }

        match cell.wall {
            WallStyle::Solid => {
                svg.push_str(&format!(
                    "<g stroke='{WALL}' stroke-width='{:.4}'>\n",
                    STRIP_WALL / scale
                ));
                for ring in cell.region.all_rings_mm() {
                    let d: Vec<String> = ring.iter().map(pt).collect();
                    svg.push_str(&format!("<path d='M{}Z'/>\n", d.join("L")));
                }
                svg.push_str("</g>\n");
            }
            WallStyle::DashedGuide => {
                let dash = format!("{:.2},{:.2}", 3.2 / scale, 2.4 / scale);
                svg.push_str(&format!(
                    "<g stroke='{GUIDE}' stroke-width='{:.4}' stroke-dasharray='{dash}'>\n",
                    (STRIP_WALL * 0.85) / scale
                ));
                for ring in cell.region.all_rings_mm() {
                    let d: Vec<String> = ring.iter().map(pt).collect();
                    svg.push_str(&format!("<path d='M{}Z'/>\n", d.join("L")));
                }
                svg.push_str("</g>\n");
            }
            WallStyle::None => {}
        }

        svg.push_str(&format!(
            "<g stroke='{INK}' stroke-width='{:.4}'>\n",
            STRIP_INK / scale
        ));
        for (k, s) in cell.strokes.iter().filter(|p| p.len() >= 2).enumerate() {
            let d: Vec<String> = s.iter().map(pt).collect();
            let ink = inks[k % inks.len()];
            let colour = if ink == INK {
                String::new()
            } else {
                format!(" stroke='{ink}'")
            };
            svg.push_str(&format!("<path{colour} d='M{}'/>\n", d.join("L")));
        }
        svg.push_str("</g>\n</g>\n");

        svg.push_str(&format!(
            "<text x='{:.1}' y='{:.1}' font-family='Georgia, serif' font-size='10' fill='{LABEL}' text-anchor='middle'>{}</text>\n",
            ox + STRIP_W * 0.5,
            STRIP_H + 10.0,
            cell.label
        ));
    }
    svg.push_str("</svg>\n");
    svg
}

fn write_readme_strips(shapes: &GalleryShapes, patterns: &[Pattern]) {
    let dir = "docs/img";
    std::fs::create_dir_all(dir).unwrap();

    let by_slug = |slug: &str| {
        patterns
            .iter()
            .find(|p| p.slug == slug)
            .unwrap_or_else(|| panic!("no pattern {slug}"))
    };
    let card_cell = |slug: &str, spacing: f64| -> StripCell {
        let p = by_slug(slug);
        let strokes = p.render(
            &shapes.card,
            spacing,
            p.default_angle,
            Some(border_for(spacing)),
            0.0,
        );
        StripCell::solid(p.name, &shapes.card, strokes)
    };
    let card_row = |fills: &[(&str, f64)]| -> Vec<StripCell> {
        fills.iter().map(|&(slug, s)| card_cell(slug, s)).collect()
    };

    let tour_cells: Vec<StripCell> = [
        ("concentric", MID_MM),
        ("wave-grid", MID_MM),
        ("scales", MID_MM),
        ("gravel", MID_MM),
        ("swells", COARSE_MM),
        ("isoline-stack", MID_MM),
    ]
    .iter()
    .map(|&(slug, s)| {
        let p = by_slug(slug);
        StripCell::solid(
            p.name,
            &shapes.ring,
            p.render(&shapes.ring, s, p.default_angle, Some(border_for(s)), 0.0),
        )
    })
    .collect();

    let strips: Vec<(&str, Vec<StripCell>)> = vec![
        ("tour", tour_cells),
        (
            "lines",
            card_row(&[
                ("rectilinear", MID_MM),
                ("zigzag", MID_MM),
                ("grid", COARSE_MM),
                ("triangles", COARSE_MM),
                ("concentric", MID_MM),
                ("hilbert", MID_MM),
            ]),
        ),
        (
            "lattices",
            card_row(&[
                ("honeycomb", COARSE_MM),
                ("dots-hex", MID_MM),
                ("waves", MID_MM),
                ("wave-grid", MID_MM),
                ("scales", MID_MM),
                ("sunburst", MID_MM),
            ]),
        ),
        (
            "scatter",
            card_row(&[
                ("stipple", MID_MM),
                ("stipple-edge", MID_MM),
                ("pebbles", MID_MM),
                ("tufts", MID_MM),
                ("twigs", MID_MM),
                ("canopy", MID_MM),
            ]),
        ),
        (
            "nature",
            card_row(&[
                ("sprouts", MID_MM),
                ("fluid-grass", MID_MM),
                ("vines", MID_MM),
                ("lily-pads", MID_MM),
                ("grass", MID_MM),
                ("scribble", MID_MM),
            ]),
        ),
        (
            "materials",
            card_row(&[
                ("gravel", MID_MM),
                ("terrazzo", MID_MM),
                ("marble", FINE_MM),
                ("wood", MID_MM),
                ("cracks", MID_MM),
                ("cell-net", MID_MM),
            ]),
        ),
        (
            "flow",
            card_row(&[
                ("eddies", COARSE_MM),
                ("swells", COARSE_MM),
                ("ripples", COARSE_MM),
                ("truchet", MID_MM),
                ("chess", MID_MM),
                ("gravel-clipped", MID_MM),
            ]),
        ),
        (
            "organic",
            vec![
                StripCell::solid(
                    "isoline blobs",
                    &shapes.card,
                    fill_isoline_blobs_with_coverage(&shapes.card, &default_params(5.5, 0.9, SEED)).strokes,
                ),
                StripCell::solid(
                    "isoline stack",
                    &shapes.card,
                    fill_isoline_stack_with_coverage(
                        &shapes.card,
                        &IsolineStackParams {
                            seed: SEED,
                            crown: 6.5,
                            bubble: 2.0,
                            edge: EdgeMode::Inside,
                            ..Default::default()
                        },
                    )
                    .strokes,
                ),
                StripCell::solid(
                    "isoline stack (lit)",
                    &shapes.card,
                    fill_isoline_stack_with_coverage(
                        &shapes.card,
                        &IsolineStackParams {
                            seed: SEED,
                            crown: 6.5,
                            bubble: 2.0,
                            edge: EdgeMode::Inside,
                            light: Some(Light {
                                from_deg: -45.0,
                                strength: 0.85,
                                ..Light::default()
                            }),
                            ..Default::default()
                        },
                    )
                    .strokes,
                ),
                StripCell::solid(
                    "growth (meander)",
                    &shapes.card,
                    fill_growth(
                        &shapes.card,
                        &GrowthParams {
                            seed: SEED,
                            spacing: 1.4,
                            ..Default::default()
                        },
                    ),
                ),
                StripCell::solid(
                    "growth (fine)",
                    &shapes.card,
                    fill_growth(
                        &shapes.card,
                        &GrowthParams {
                            seed: SEED,
                            spacing: 0.9,
                            step: 0.35,
                            reach: 6.0,
                            kill: 0.75,
                            ..Default::default()
                        },
                    ),
                ),
                StripCell::guide(
                    "growth (frayed)",
                    &shapes.island,
                    fill_growth(
                        &shapes.island,
                        &GrowthParams {
                            seed: SEED,
                            spacing: 1.3,
                            fray: 8.5,
                            ..Default::default()
                        },
                    ),
                ),
            ],
        ),
        (
            "geology",
            card_row(&[
                ("granite", COARSE_MM),
                ("arena", FINE_MM),
                ("breccia", COARSE_MM),
                ("coal", COARSE_MM),
                ("gneiss", COARSE_MM),
                ("ferrous", COARSE_MM),
            ]),
        ),
        (
            "geology-2",
            card_row(&[
                ("diorite", COARSE_MM),
                ("conglomerate", COARSE_MM),
                ("masonry", COARSE_MM),
                ("diatom", COARSE_MM),
                ("amphibolite", COARSE_MM),
                ("hornblende", COARSE_MM),
            ]),
        ),
        (
            "ground",
            card_row(&[
                ("fine-gravel", FINE_MM),
                ("coarse-stone", COARSE_MM),
                ("pebble-scatter", COARSE_MM),
                ("mixed-rocks", COARSE_MM),
                ("cracked-earth", COARSE_MM),
                ("grainy-soil", FINE_MM),
            ]),
        ),
        (
            "ground-2",
            card_row(&[
                ("erosion-lines", FINE_MM),
                ("stone-cluster", COARSE_MM),
                ("gravelly-soil", FINE_MM),
                ("mulch", COARSE_MM),
                ("clay-soil", FINE_MM),
                ("pebbled-soil", COARSE_MM),
            ]),
        ),
        (
            "cad",
            [
                ("BRICK", 1.8),
                ("HERRINGBONE", 1.8),
                ("PARQUET", 1.1),
                ("EARTH", 3.4),
                ("ANSI37", 2.2),
                ("CROSS", 1.8),
            ]
            .iter()
            .map(|&(name, scale)| {
                StripCell::solid(
                    name.to_lowercase(),
                    &shapes.card,
                    fill_pat(
                        &shapes.card.inset(0.8),
                        &builtin(name).unwrap(),
                        scale,
                        0.0,
                        Anchor::LOCAL,
                    ),
                )
            })
            .collect(),
        ),
        (
            "shapes",
            [
                ("square", &shapes.square),
                ("rounded card", &shapes.card),
                ("ring (hole)", &shapes.ring),
                ("island + pond", &shapes.pond_island),
                ("star", &shapes.star),
                ("archipelago", &shapes.archipelago),
            ]
            .iter()
            .map(|&(name, r)| {
                StripCell::solid(
                    name,
                    r,
                    penfill::concentric::fill_concentric(r, MID_MM, Some(border_for(MID_MM))),
                )
            })
            .collect(),
        ),
    ];

    for (name, cells) in &strips {
        let path = format!("{dir}/{name}.svg");
        std::fs::write(&path, render_strip(cells)).unwrap();
        println!("wrote {path}");
    }

    // Universal Edge Fraying strip (`docs/img/fray.svg`)
    let fray_strip: Vec<StripCell> = [
        ("zigzag · frayed", "zigzag", MID_MM, WallStyle::DashedGuide),
        ("waves · ink only", "waves", MID_MM, WallStyle::None),
        ("dots hex · frayed", "dots-hex", MID_MM, WallStyle::DashedGuide),
        ("gravel · ink only", "gravel", MID_MM, WallStyle::None),
        ("tufts · frayed", "tufts", MID_MM, WallStyle::DashedGuide),
        ("cell net · ink only", "cell-net", MID_MM, WallStyle::None),
    ]
    .iter()
    .map(|&(label, slug, spacing, wall)| {
        let p = by_slug(slug);
        let strokes = p.render(&shapes.island, spacing, p.default_angle, None, 9.0);
        StripCell {
            label: label.to_string(),
            region: &shapes.island,
            strokes,
            wall,
            bg_strokes: None,
            mask_region: None,
        }
    })
    .collect();
    std::fs::write(format!("{dir}/fray.svg"), render_strip(&fray_strip)).unwrap();
    println!("wrote {dir}/fray.svg");

    // Boundary Modes & Polygon Mask Knockout strip (`docs/img/edges-and-masks.svg`)
    let edge_region = &shapes.pond_island;
    let base_g = FillParams::new(4.8).seed(SEED);
    let g_clip = fill_gravel_params(edge_region, &base_g.edge(EdgeMode::Clip)).strokes;
    let g_inside = fill_gravel_params(edge_region, &base_g.edge(EdgeMode::Inside)).strokes;
    let g_over = fill_gravel_params(edge_region, &base_g.edge(EdgeMode::Overflow(3.5)));
    let bg_box = edge_region.outset(4.5);
    let knocked = bg_box.difference(&g_over.mask.outset(0.35));
    let bg_hatch = fill_rectilinear(&knocked, 1.35, 35.0, Some(0.4), Anchor::GLOBAL);

    let canopy_over = fill_output_by_name(
        "canopy",
        edge_region,
        &FillParams::new(5.4).seed(SEED).edge(EdgeMode::Overflow(3.5)),
    )
    .unwrap();
    let knocked_canopy = bg_box.difference(&canopy_over.mask.outset(0.35));
    let bg_waves = fill_by_name(
        "waves",
        &knocked_canopy,
        &FillParams::new(1.5).angle(20.0).anchor(Anchor::GLOBAL),
    )
    .unwrap();

    let edge_strip = vec![
        StripCell::solid("EdgeMode::Clip", edge_region, g_clip),
        StripCell::solid("EdgeMode::Inside", edge_region, g_inside),
        StripCell::guide("EdgeMode::Overflow", edge_region, g_over.strokes.clone()),
        StripCell {
            label: "FillOutput.mask".into(),
            region: edge_region,
            strokes: g_over.strokes.clone(),
            wall: WallStyle::DashedGuide,
            bg_strokes: None,
            mask_region: Some(g_over.mask),
        },
        StripCell {
            label: "gravel + hatch knockout".into(),
            region: edge_region,
            strokes: g_over.strokes,
            wall: WallStyle::DashedGuide,
            bg_strokes: Some(bg_hatch),
            mask_region: None,
        },
        StripCell {
            label: "canopy + wave knockout".into(),
            region: edge_region,
            strokes: canopy_over.strokes,
            wall: WallStyle::DashedGuide,
            bg_strokes: Some(bg_waves),
            mask_region: None,
        },
    ];
    std::fs::write(format!("{dir}/edges-and-masks.svg"), render_strip(&edge_strip)).unwrap();
    println!("wrote {dir}/edges-and-masks.svg");

    // Hero strip: Zigzag colored per pen-down vs plain hatch stroke count
    let hero: Vec<StripCell> = [
        ("ring", &shapes.ring),
        ("island + pond", &shapes.pond_island),
        ("star", &shapes.star),
        ("archipelago", &shapes.archipelago),
    ]
    .iter()
    .map(|&(_name, r)| {
        let border = Some(border_for(MID_MM));
        let zig = fill_zigzag_inset(r, MID_MM, 0.0, border, Anchor::LOCAL);
        let hatch = fill_rectilinear(r, MID_MM, 0.0, border, Anchor::LOCAL).len();
        StripCell::solid(format!("{} strokes (hatch: {hatch})", zig.len()), r, zig)
    })
    .collect();
    std::fs::write(format!("{dir}/hero.svg"), render_strip_y(&hero, &PENS, false)).unwrap();
    println!("wrote {dir}/hero.svg");

    // Dither strip
    let img = picture();
    let upright = img.flipped();
    let base = DitherOptions {
        seed: SEED,
        ..DitherOptions::new(0.55)
    };
    let dithers = [
        (
            "bayer 4, dots",
            DitherOptions {
                method: Dither::Bayer(4),
                marks: Marks::Dots,
                ..base
            },
        ),
        (
            "halftone, lines at 45°",
            DitherOptions {
                method: Dither::Halftone(6),
                angle: 45.0,
                ..base
            },
        ),
        ("floyd-steinberg, lines", base),
        (
            "atkinson, dots",
            DitherOptions {
                method: Dither::Atkinson,
                marks: Marks::Dots,
                ..base
            },
        ),
        (
            "jarvis, lines 1 mm apart",
            DitherOptions {
                method: Dither::JarvisJudiceNinke,
                resolution: 0.35,
                line_spacing: 1.1,
                ..base
            },
        ),
    ];
    let mut dither_cells = vec![StripCell::solid("the picture", &shapes.square, vec![])];
    dither_cells.extend(dithers.iter().map(|(label, o)| {
        StripCell::solid(*label, &shapes.square, fill_dither(&shapes.square, &upright, o))
    }));
    let side = (STRIP_W - 16.0).min(STRIP_H - 16.0);
    let backdrop = format!(
        "<image x='{:.2}' y='{:.2}' width='{side:.2}' height='{side:.2}' href='{}'/>\n</svg>\n",
        (STRIP_W - side) * 0.5,
        (STRIP_H - side) * 0.5,
        picture_data_uri(&img)
    );
    let dither_svg = render_strip(&dither_cells).replacen("</svg>\n", &backdrop, 1);
    std::fs::write(format!("{dir}/dither.svg"), dither_svg).unwrap();
    println!("wrote {dir}/dither.svg");

    // Placement strip (drawn y-down like SVG)
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let place =
        |size: &str, position: &str, repeat: &str| Placement::parse(size, position, repeat).unwrap();
    let dith = |region: &Region, p: Placement| {
        fill_dither(
            region,
            &img,
            &DitherOptions {
                placement: p,
                ..DitherOptions::new(0.55)
            },
        )
    };
    let backgrounds: Vec<StripCell> = vec![
        StripCell::solid(
            "cover",
            &shapes.ring,
            dith(&shapes.ring, place("cover", "center", "no-repeat")),
        ),
        StripCell::solid(
            "contain",
            &shapes.star,
            dith(&shapes.star, place("contain", "center", "no-repeat")),
        ),
        StripCell::solid(
            "16 mm, repeat",
            &shapes.card,
            dith(&shapes.card, place("16", "center", "repeat")),
        ),
        StripCell::solid(
            "18 mm, repeat-x, top",
            &shapes.square,
            dith(&shapes.square, place("18", "top", "repeat-x")),
        ),
        StripCell::solid(
            "svg, 11 mm, repeat",
            &shapes.ring,
            fill_svg_placed(&shapes.ring, &flower, &place("11", "center", "repeat")),
        ),
        StripCell::solid(
            "svg, contain",
            &shapes.star,
            fill_svg_placed(&shapes.star, &flower, &place("contain", "center", "no-repeat")),
        ),
    ];
    std::fs::write(
        format!("{dir}/placement.svg"),
        render_strip_y(&backgrounds, &[INK], true),
    )
    .unwrap();
    println!("wrote {dir}/placement.svg");

    // Custom patterns strip
    let custom_strip: Vec<StripCell> = custom_rows(shapes)
        .into_iter()
        .map(|(label, mut cells)| {
            let cell = cells.remove(0);
            StripCell::solid(label, &shapes.card, cell.strokes)
        })
        .collect();
    std::fs::write(format!("{dir}/custom.svg"), render_strip(&custom_strip)).unwrap();
    println!("wrote {dir}/custom.svg");
}

// ---------------------------------------------------------------------------
// Main Entry Point
// ---------------------------------------------------------------------------

fn main() {
    let shapes = gallery_shapes();
    let patterns = catalog();
    let dir = "examples/gallery";
    std::fs::create_dir_all(dir).unwrap();

    let mut strokes_total = 0usize;
    let mut mm_total = 0.0;

    // 1. Individual pattern sheets (`examples/gallery/<slug>.svg`)
    for pattern in &patterns {
        let rows = pattern_sheet_rows(pattern, &shapes);
        for (_, cells) in &rows {
            for cell in cells {
                strokes_total += cell.strokes.len();
                mm_total += cell.strokes.iter().map(|p| polyline_length(p)).sum::<f64>();
            }
        }
        let note = format!(
            "{} · columns show unfrayed square, unfrayed ring, and frayed organic perimeter (fray = {FRAY_MM:.1} mm)",
            pattern.note
        );
        let svg = render_sheet(pattern.name, &note, &PATTERN_SHEET_COLUMNS, &rows);
        let path = format!("{dir}/{}.svg", pattern.slug);
        std::fs::write(&path, svg).unwrap();
        println!("wrote {path}");
    }

    // 2. Categorized visual reference sheet (`examples/gallery/gallery.svg`)
    let gallery_svg = render_categorized_gallery(&patterns, &shapes);
    std::fs::write(format!("{dir}/gallery.svg"), gallery_svg).unwrap();
    println!("wrote {dir}/gallery.svg");

    // 3. Alphabetical index grid (`examples/gallery/all-patterns.svg`)
    let all_svg = render_alphabetical_grid(&patterns, &shapes.square);
    std::fs::write(format!("{dir}/all-patterns.svg"), all_svg).unwrap();
    println!("wrote {dir}/all-patterns.svg");

    // 4. Universal Edge Fraying sheet (`examples/gallery/fray.svg`)
    let fray_svg = render_sheet(
        "universal edge fraying (fray)",
        "smooth perimeter disappearing across continuous lines, discrete marks, and closed cells",
        &FRAY_COLUMNS,
        &fray_showcase_rows(&patterns, &shapes),
    );
    std::fs::write(format!("{dir}/fray.svg"), fray_svg).unwrap();
    println!("wrote {dir}/fray.svg");

    // 5. Boundary Modes & Polygon Masks sheet (`examples/gallery/edges-and-masks.svg`)
    let edge_mask_svg = render_sheet(
        "boundary modes (EdgeMode) & polygon masks (FillOutput)",
        "Clip vs Inside vs Overflow(3.5) · FillOutput.mask silhouette · multi-layer knockout (below.difference(&out.mask))",
        &EDGE_MASK_COLUMNS,
        &edge_and_mask_rows(&shapes),
    );
    std::fs::write(format!("{dir}/edges-and-masks.svg"), edge_mask_svg).unwrap();
    println!("wrote {dir}/edges-and-masks.svg");

    // 6. CAD, Custom, Dither & Placement sheets
    let cad = render_sheet(
        "CAD patterns (.pat)",
        "every built-in .pat hatch · shown on square, ring, and frayed organic contour",
        &PATTERN_SHEET_COLUMNS,
        &cad_rows(&shapes),
    );
    std::fs::write(format!("{dir}/cad.svg"), cad).unwrap();
    println!("wrote {dir}/cad.svg");

    let custom_cols = ["square", "ring (hole)", "island + pond", "star"];
    let custom = render_sheet(
        "build your own",
        "fill_isolines(f) · fill_profile(g) · fill_svg_tile(svg) · Region::clip_strokes(any drawing)",
        &custom_cols,
        &custom_rows(&shapes),
    );
    std::fs::write(format!("{dir}/custom.svg"), custom).unwrap();
    println!("wrote {dir}/custom.svg");

    let dither = render_sheet(
        "dither",
        "a picture dithered on 0.55 mm cells · dots: a tick per dark cell · lines: dark runs joined back and forth",
        &DITHER_COLUMNS,
        &dither_rows(&shapes),
    );
    std::fs::write(format!("{dir}/dither.svg"), dither).unwrap();
    println!("wrote {dir}/dither.svg");

    let placed = render_sheet_y(
        "placement",
        "a picture or an SVG placed like a CSS background — size · position · repeat — and cut by the outline (drawn y-down, as in SVG)",
        &custom_cols,
        &placement_rows(&shapes),
        true,
    );
    std::fs::write(format!("{dir}/placement.svg"), placed).unwrap();
    println!("wrote {dir}/placement.svg");

    // 7. README illustration strips (`docs/img/*.svg`)
    write_readme_strips(&shapes, &patterns);

    println!(
        "done: {} patterns · {strokes_total} strokes, {mm_total:.1} mm of ink",
        patterns.len()
    );
}
