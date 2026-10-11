//! Gallery sheets for the README and for printing.
//!
//! One SVG per pattern (`examples/gallery/<pattern>.svg`). `gallery.svg` is
//! the comparison table. Vector, so the lines stay sharp when you zoom.

use penfill::geo::{polyline_length, Region, Strokes};
use penfill::Anchor;
use penfill::{
    chess::fill_chess,
    concentric::fill_concentric,
    dither::{fill_dither, Dither, DitherOptions, Greyscale, Marks},
    placement::Placement,
    dotswaves::{fill_dots, fill_dots_with_lattice, fill_wave_grid, fill_waves, Lattice},
    field::{fill_isolines, fill_profile, levels_every},
    radial::fill_sunburst,
    svg::{fill_svg_placed, fill_svg_tile, parse_svg},
    terrazzo::fill_terrazzo,
    litho::{
        fill_amphibolite, fill_arena, fill_beds, fill_benton, fill_breccia, fill_breccia2, fill_brackets,
        fill_coal, fill_conglomerate, fill_conglomerate2, fill_coquina, fill_coarse_stone, fill_cracked_earth,
        fill_dacite, fill_diabase, fill_diatom, fill_diorite, fill_drift, fill_erosion, fill_feldspar, fill_ferrous,
        fill_fill, fill_fine_gravel, fill_gabbro, fill_gneiss, fill_grainy_soil, fill_granite,
        fill_granite2, fill_gravelly_soil, fill_hornblende, fill_laminae, fill_lava, fill_layered_soil,
        fill_masonry, fill_mixed_rocks, fill_mulch, fill_ophitic, fill_pebble_scatter,
        fill_pebbled_soil, fill_shards, fill_silty_soil, fill_stone_cluster,
        fill_chalky_soil, fill_clay_soil, fill_dry_cracked, fill_topsoil,
        fill_loose_soil, fill_compacted_soil, fill_sandy_soil, fill_mulched_soil, fill_debris_soil,
        fill_sandstone,
    },
    nature::{fill_canopy, fill_fluid_grass, fill_leaves, fill_lily_pads, fill_pebbles},
    textures::{fill_cracks, fill_marble, fill_wood},
    grass::fill_grass,
    gravel::{fill_gravel, fill_gravel_with, GravelEdge},
    grid::{fill_grid, fill_triangles},
    honeycomb::fill_honeycomb,
    pat::{builtin, fill_pat, BUILTIN},
    rectilinear::fill_rectilinear,
    scales::fill_scales,
    scribble::fill_scribble,
    hilbert::fill_hilbert,
    spiral::fill_spiral,
    stipple::{fill_stipple, fill_stipple_edge, fill_stipple_noise},
    truchet::fill_truchet,
    zigzag::fill_zigzag_inset,
};

type Filler = Box<dyn Fn(&Region) -> Strokes>;
type Strip<'a> = (&'a str, Vec<(String, &'a Region, Strokes)>);

/// Seed for the scattered fills, so the sheets don't change between runs.
const SEED: u64 = 7;

/// Three styles. Each one pairs a spacing with an angle, and the border
/// is half the spacing, so the coarse row sits further off the wall.
/// The angle steps are 0 / +20 / +45: a hex grid repeats every 60°, so
/// 60° and 90° would just redraw the rows above.
const FINE_MM: f64 = 0.8;
const MID_MM: f64 = 1.6;
const COARSE_MM: f64 = 2.8;
const ZIGZAG_ANGLE: f64 = 0.0;

fn border_for(spacing: f64) -> f64 {
    spacing / 2.0
}

/// Warm paper, warm-gray wall, ink fill. Readable in print without primary colors.
const PAPER: &str = "#f6f3ee";
const WALL: &str = "#b7aea3";
const INK: &str = "#2a2622";
const LABEL: &str = "#6b645c";

const CELL_W: f64 = 78.0;
const CELL_H: f64 = 66.0;
const LABEL_W: f64 = 96.0;
const HEADER_H: f64 = 22.0;
const TITLE_H: f64 = 36.0;

fn shapes() -> Vec<(&'static str, Region)> {
    let square = Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)]);

    let doughnut = {
        let outer = circle(30.0, 30.0, 28.0, 8);
        let hole = circle(30.0, 30.0, 9.0, 48);
        Region::from_polygons(&[vec![outer, hole]])
    };

    let blob = Region::from_polygons(&[vec![vec![
        (10.0, 6.0),
        (34.0, 2.0),
        (52.0, 14.0),
        (56.0, 30.0),
        (44.0, 44.0),
        (24.0, 46.0),
        (8.0, 38.0),
        (2.0, 22.0),
    ]]]);

    let star = {
        let mut pts = Vec::new();
        let spikes = 11usize;
        for i in 0..2 * spikes {
            let a = std::f64::consts::PI * (i as f64) / (spikes as f64);
            let r = if i % 2 == 0 { 28.0 } else { 12.0 };
            pts.push((30.0 + r * a.cos(), 30.0 + r * a.sin()));
        }
        Region::from_ring(&pts)
    };

    let worst = {
        let angry = vec![
            (2.0, 8.0),
            (18.0, 2.0),
            (28.0, 14.0),
            (22.0, 16.0),
            (34.0, 22.0),
            (16.0, 28.0),
            (24.0, 42.0),
            (8.0, 36.0),
            (4.0, 44.0),
            (0.0, 26.0),
        ];
        let outer = circle(62.0, 24.0, 16.0, 8);
        let hole = circle(62.0, 24.0, 6.0, 32);
        Region::from_polygons(&[vec![angry], vec![outer, hole]])
    };

    vec![
        ("square", square),
        ("doughnut", doughnut),
        ("blob", blob),
        ("star", star),
        ("worst", worst),
    ]
}

fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

struct Pattern {
    name: &'static str,
    slug: &'static str,
    /// Shown under the title of this pattern's own sheet.
    note: &'static str,
    has_angle: bool,
    default_angle: f64,
    /// Spacing used in the comparison table. The coarse style, so the cell reads.
    preview_spacing: f64,
    fill: fn(&Region, f64, f64, f64) -> Strokes,
}

/// Same entries, but in name order so the catalog sheet reads like an index.
fn patterns_sorted() -> Vec<Pattern> {
    let mut p = patterns();
    p.sort_by_key(|p| p.name.to_lowercase());
    p
}

fn patterns() -> Vec<Pattern> {
    vec![
        Pattern {
            name: "amphibolite",
            slug: "amphibolite",
            note: "continuous vertical zigzag",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_amphibolite(&r.inset(inset), s * 1.6, SEED),
        },
        Pattern {
            name: "arena",
            slug: "arena",
            note: "fine dot stipple",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, _, inset| fill_arena(&r.inset(inset), s * 0.9, SEED),
        },
        Pattern {
            name: "basket",
            slug: "basket",
            note: "CAD .pat basket weave",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_pat(&r.inset(inset), &builtin("BASKET").unwrap(), s, a, Anchor::LOCAL),
        },
        Pattern {
            name: "beds",
            slug: "beds",
            note: "continuous wavy horizons",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_beds(&r.inset(inset), s * 1.4, a, SEED),
        },
        Pattern {
            name: "bentonite",
            slug: "bentonite",
            note: "vertical dashed columns",
            has_angle: true,
            default_angle: 90.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_benton(&r.inset(inset), s * 1.4, a, SEED),
        },
        Pattern {
            name: "brackets",
            slug: "brackets",
            note: "orthogonal └ ┘ marks",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_brackets(&r.inset(inset), s * 1.2, SEED),
        },
        Pattern {
            name: "breccia",
            slug: "breccia",
            note: "angular Voronoi fragments, narrow gaps",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_breccia(&r.inset(inset), s * 2.2, SEED),
        },
        Pattern {
            name: "breccia blobs",
            slug: "breccia-blobs",
            note: "rounded low-frequency fragments",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_breccia2(&r.inset(inset), s * 2.0, SEED),
        },
        Pattern {
            name: "bricks",
            slug: "bricks",
            note: "CAD .pat running bond, brick height 2 × spacing",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| {
                fill_pat(&r.inset(inset), &builtin("BRICK").unwrap(), s * 2.0, a, Anchor::LOCAL)
            },
        },
        Pattern {
            name: "canopy",
            slug: "canopy",
            note: "clustered wobbly lobes with a centre dot per lobe",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_canopy(&r.inset(inset), s * 2.5, SEED),
        },
        Pattern {
            name: "chalky soil",
            slug: "chalky-soil",
            note: "pale granular dots with a very open cell net",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_chalky_soil(&r.inset(inset), s * 1.1, SEED)
        },
        Pattern {
            name: "chess",
            slug: "chess",
            note: "squares 4 × spacing, dark ones outlined and hatched at half the spacing",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_chess(&r.inset(inset), s * 4.0, s * 0.5, a, Anchor::LOCAL),
        },
        Pattern {
            name: "clay soil",
            slug: "clay-soil",
            note: "smooth broad waves",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, a, inset| fill_clay_soil(&r.inset(inset), s * 0.7, a, SEED),
        },
        Pattern {
            name: "coal",
            slug: "coal",
            note: "dashed bedding seams",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_coal(&r.inset(inset), s * 1.2, a, SEED),
        },
        Pattern {
            name: "coarse stone",
            slug: "coarse-stone",
            note: "a few large rounded block outlines",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_coarse_stone(&r.inset(inset), s * 1.6, SEED),
        },
        Pattern {
            name: "compacted soil",
            slug: "compacted-soil",
            note: "tight thin waves pressed together",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, a, inset| fill_compacted_soil(&r.inset(inset), s * 1.2, a, SEED)
        },
        Pattern {
            name: "concentric",
            slug: "concentric",
            note: "inset rings",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_concentric(r, s, Some(inset)),
        },
        Pattern {
            name: "conglomerate",
            slug: "conglomerate",
            note: "round pebbles on a staggered grid",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_conglomerate(&r.inset(inset), s * 1.4, SEED),
        },
        Pattern {
            name: "conglomerate flat",
            slug: "conglomerate-flat",
            note: "horizontally squashed pebbles",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_conglomerate2(&r.inset(inset), s * 1.4, SEED),
        },
        Pattern {
            name: "coquina",
            slug: "coquina",
            note: "shell spirals in rows",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_coquina(&r.inset(inset), s * 2.0, SEED),
        },
        Pattern {
            name: "cracked earth",
            slug: "cracked-earth",
            note: "fine dots with a tight angular cell net on top",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_cracked_earth(&r.inset(inset), s * 1.4, SEED),
        },
        Pattern {
            name: "cracks",
            slug: "cracks",
            note: "parched ground: rounded cellular tile (wide grout), ~4 × spacing across",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_cracks(&r.inset(inset), s * 4.0, SEED),
        },
        Pattern {
            name: "dacite",
            slug: "dacite",
            note: "short crystal dashes at the angle",
            has_angle: true,
            default_angle: 35.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_dacite(&r.inset(inset), s * 0.8, a, SEED),
        },
        Pattern {
            name: "debris soil",
            slug: "debris-soil",
            note: "tiny chips and specks from crushed rock",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_debris_soil(&r.inset(inset), s * 1.2, SEED)
        },
        Pattern {
            name: "diabase",
            slug: "diabase",
            note: "dense parallel hatch (angle = direction)",
            has_angle: true,
            default_angle: -45.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_diabase(&r.inset(inset), s * 0.6, a, SEED),
        },
        Pattern {
            name: "diagonal",
            slug: "diagonal",
            note: "rectilinear at 45°",
            has_angle: true,
            default_angle: 45.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_rectilinear(r, s, a, Some(inset), Anchor::LOCAL),
        },
        Pattern {
            name: "diatom",
            slug: "diatom",
            note: "concentric micro-motifs: circle + inner ring + dot",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_diatom(&r.inset(inset), s * 2.0, SEED),
        },
        Pattern {
            name: "diorite",
            slug: "diorite",
            note: "igneous: T marks",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_diorite(&r.inset(inset), s * 1.2, SEED),
        },
        Pattern {
            name: "dots",
            slug: "dots",
            note: "square lattice centred on the shape, marks a fifth of the pitch",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_dots(&r.inset(inset), s, a, s * 0.2, Anchor::LOCAL),
        },
        Pattern {
            name: "dots hex",
            slug: "dots-hex",
            note: "hexagonal lattice centred on the shape, marks a fifth of the pitch",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| {
                fill_dots_with_lattice(&r.inset(inset), s, a, s * 0.2, Lattice::Hexagonal, Anchor::LOCAL)
            },
        },
        Pattern {
            name: "drift",
            slug: "drift",
            note: "sparse tilted pebbles",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_drift(&r.inset(inset), s * 2.0, SEED),
        },
        Pattern {
            name: "dry cracked soil",
            slug: "dry-cracked-soil",
            note: "wide mud-crack polygons with rounded gaps",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_dry_cracked(&r.inset(inset), s * 1.1, SEED)
        },
        Pattern {
            name: "erosion lines",
            slug: "erosion-lines",
            note: "many low-amplitude parallel waves",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, a, inset| fill_erosion(&r.inset(inset), s * 0.5, a, SEED),
        },
        Pattern {
            name: "feldspar",
            slug: "feldspar",
            note: "dense short crystal strokes",
            has_angle: true,
            default_angle: -45.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_feldspar(&r.inset(inset), s * 0.9, a, SEED),
        },
        Pattern {
            name: "ferrous",
            slug: "ferrous",
            note: "checkerboard cells",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_ferrous(&r.inset(inset), s * 1.0, SEED),
        },
        Pattern {
            name: "fill",
            slug: "fill",
            note: "woven cross hatch",
            has_angle: true,
            default_angle: 45.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_fill(&r.inset(inset), s * 0.8, a, SEED),
        },
        Pattern {
            name: "fine gravel",
            slug: "fine-gravel",
            note: "sparse dots with scattered small rings",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_fine_gravel(&r.inset(inset), s * 1.5, SEED),
        },
        Pattern {
            name: "fluid grass",
            slug: "fluid-grass",
            note: "short strokes aligned to a smooth vector field",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_fluid_grass(&r.inset(inset), s * 1.35, SEED),
        },
        Pattern {
            name: "gabbro",
            slug: "gabbro",
            note: "igneous: alternating V marks",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_gabbro(&r.inset(inset), s * 1.2, SEED),
        },
        Pattern {
            name: "gneiss",
            slug: "gneiss",
            note: "foliation: aligned dash rows (strike = angle)",
            has_angle: true,
            default_angle: 30.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_gneiss(&r.inset(inset), s * 1.6, a, SEED),
        },
        Pattern {
            name: "grainy soil",
            slug: "grainy-soil",
            note: "dense fine dot field",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, _, inset| fill_grainy_soil(&r.inset(inset), s * 0.55, SEED),
        },
        Pattern {
            name: "granite",
            slug: "granite",
            note: "igneous: + on a staggered grid",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_granite(&r.inset(inset), s * 1.2, SEED),
        },
        Pattern {
            name: "granite ×",
            slug: "granite-x",
            note: "igneous: diagonal × grain",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_granite2(&r.inset(inset), s * 1.2, SEED),
        },
        Pattern {
            name: "grass",
            slug: "grass",
            note: "CAD hatch: a 4 × 4 tile of tufts 2 × spacing apart, repeated and cut by the wall",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_grass(&r.inset(inset), s * 2.0, a, SEED, Anchor::LOCAL),
        },
        Pattern {
            name: "gravel",
            slug: "gravel",
            note: "rounded Voronoi stones about 2.5 × spacing across, smaller toward the wall",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_gravel(&r.inset(inset), s * 2.5, SEED),
        },
        Pattern {
            name: "gravel clipped",
            slug: "gravel-clipped",
            note: "same stones at full size, simply cut by the wall",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| {
                fill_gravel_with(&r.inset(inset), s * 2.5, SEED, GravelEdge::Clipped)
            },
        },
        Pattern {
            name: "gravelly soil",
            slug: "gravelly-soil",
            note: "dense short dashes at scattered angles",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, _, inset| fill_gravelly_soil(&r.inset(inset), s, SEED),
        },
        Pattern {
            name: "grid",
            slug: "grid",
            note: "two hatches, 90° apart",
            has_angle: true,
            default_angle: 45.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, _| fill_grid(r, s, a, Anchor::LOCAL),
        },
        Pattern {
            name: "herringbone",
            slug: "herringbone",
            note: "CAD .pat herringbone, brick width 2 × spacing",
            has_angle: true,
            default_angle: 45.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| {
                fill_pat(&r.inset(inset), &builtin("HERRINGBONE").unwrap(), s * 2.0, a, Anchor::LOCAL)
            },
        },
        Pattern {
            name: "hilbert",
            slug: "hilbert",
            note: "space-filling curve, one pen-down path",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_hilbert(r, s, Some(inset)),
        },
        Pattern {
            name: "honeycomb",
            slug: "honeycomb",
            note: "hexagon cells",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_honeycomb(&r.inset(inset), s, a, 0.0, Anchor::LOCAL),
        },
        Pattern {
            name: "hornblende",
            slug: "hornblende",
            note: "narrow crystal capsules along the angle",
            has_angle: true,
            default_angle: 40.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_hornblende(&r.inset(inset), s * 1.4, a, SEED),
        },
        Pattern {
            name: "laminae",
            slug: "laminae",
            note: "fine wavy laminations",
            has_angle: true,
            default_angle: 15.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_laminae(&r.inset(inset), s * 1.4, a, SEED),
        },
        Pattern {
            name: "lava",
            slug: "lava",
            note: "vertical bars of varying width",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_lava(&r.inset(inset), s * 1.4, SEED),
        },
        Pattern {
            name: "layered soil",
            slug: "layered-soil",
            note: "stacked horizons with straighter partings",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, a, inset| fill_layered_soil(&r.inset(inset), s * 1.1, a, SEED)
        },
        Pattern {
            name: "lily pads",
            slug: "lily-pads",
            note: "equal circles with centre dots; overlapping pads merge to one outline",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_lily_pads(&r.inset(inset), s * 1.4, SEED),
        },
        Pattern {
            name: "loose soil",
            slug: "loose-soil",
            note: "scattered rings and dots, mostly paper",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, _, inset| fill_loose_soil(&r.inset(inset), s * 1.1, SEED)
        },
        Pattern {
            name: "marble",
            slug: "marble",
            note: "turbulent veins about 4 × spacing apart, each drawn as its two edges",
            has_angle: true,
            default_angle: 30.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_marble(&r.inset(inset), s * 4.0, a, SEED, Anchor::LOCAL),
        },
        Pattern {
            name: "masonry",
            slug: "masonry",
            note: "running-bond brick",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_masonry(&r.inset(inset), s * 1.0, SEED),
        },
        Pattern {
            name: "mixed rocks",
            slug: "mixed-rocks",
            note: "stone cells across a wide size range",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_mixed_rocks(&r.inset(inset), s * 1.6, SEED),
        },
        Pattern {
            name: "mulch",
            slug: "mulch",
            note: "staggered rows of short dashes",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_mulch(&r.inset(inset), s * 0.7, a, SEED),
        },
        Pattern {
            name: "mulched soil",
            slug: "mulched-soil",
            note: "rows of short dashes, finer and denser than mulch",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_mulched_soil(&r.inset(inset), s * 1.1, a, SEED)
        },
        Pattern {
            name: "ophitic",
            slug: "ophitic",
            note: "plagioclase laths crossed by pyroxene grain",
            has_angle: true,
            default_angle: -45.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_ophitic(&r.inset(inset), s * 0.9, a, SEED),
        },
        Pattern {
            name: "organic topsoil",
            slug: "organic-topsoil",
            note: "dots in three size classes",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, _, inset| fill_topsoil(&r.inset(inset), s * 1.4, SEED)
        },
        Pattern {
            name: "pebble scatter",
            slug: "pebble-scatter",
            note: "well-spaced tilted ovals",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_pebble_scatter(&r.inset(inset), s * 1.7, SEED),
        },
        Pattern {
            name: "pebbled soil",
            slug: "pebbled-soil",
            note: "sparse small squares and diamonds",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_pebbled_soil(&r.inset(inset), s * 0.85, SEED)
        },
        Pattern {
            name: "pebbles",
            slug: "pebbles",
            note: "small circle stones on a Poisson grid — lighter than gravel",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_pebbles(&r.inset(inset), s * 1.2, SEED),
        },
        Pattern {
            name: "rectilinear",
            slug: "rectilinear",
            note: "parallel lines, one stroke each",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_rectilinear(r, s, a, Some(inset), Anchor::LOCAL),
        },
        Pattern {
            name: "sand",
            slug: "sand",
            note: "fine sand: dense small dots, tighter than arena",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, _, inset| fill_arena(&r.inset(inset), s * 0.55, SEED),
        },
        Pattern {
            name: "sandstone",
            slug: "sandstone",
            note: "dot grain with short cross-beds",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_sandstone(&r.inset(inset), s * 1.2, a, SEED),
        },
        Pattern {
            name: "sandy soil",
            slug: "sandy-soil",
            note: "fine dense dot grain",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: FINE_MM,
            fill: |r, s, _, inset| fill_sandy_soil(&r.inset(inset), s * 1.2, SEED)
        },
        Pattern {
            name: "scales",
            slug: "scales",
            note: "rows of half circles resting on the row below, rows 1.5 × spacing apart, one stroke per row",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_scales(&r.inset(inset), s * 1.5, a, Anchor::LOCAL),
        },
        Pattern {
            name: "scribble",
            slug: "scribble",
            note: "hand-drawn cross-hatch, jittered and broken",
            has_angle: true,
            default_angle: 30.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_scribble(&r.inset(inset), s, a, SEED),
        },
        Pattern {
            name: "shards",
            slug: "shards",
            note: "sparse angular shards",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_shards(&r.inset(inset), s * 2.0, SEED),
        },
        Pattern {
            name: "silty soil",
            slug: "silty-soil",
            note: "small tight polygonal cells",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_silty_soil(&r.inset(inset), s * 2.0, SEED)
        },
        Pattern {
            name: "spiral",
            slug: "spiral",
            note: "concentric rings joined into one path",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, _| fill_spiral(r, s),
        },
        Pattern {
            name: "sprouts",
            slug: "sprouts",
            note: "seamless tile — offset rows of short blades and V-shaped sprouts",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_leaves(&r.inset(inset), s * 1.5, SEED),
        },
        Pattern {
            name: "stipple",
            slug: "stipple",
            note: "Poisson-disk dots, never closer than the spacing",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_stipple(&r.inset(inset), s, s * 0.3, SEED),
        },
        Pattern {
            name: "stipple edge",
            slug: "stipple-edge",
            note: "dense at the walls, thinning over 8 mm, roughened by noise",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| {
                fill_stipple_edge(&r.inset(inset), s * 0.6, s * 3.0, s * 0.25, 8.0, 0.5, SEED)
            },
        },
        Pattern {
            name: "stipple noise",
            slug: "stipple-noise",
            note: "density drifts with Perlin noise, 0.6–3 × spacing apart, blobs about 12 mm",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| {
                fill_stipple_noise(&r.inset(inset), s * 0.6, s * 3.0, s * 0.25, 12.0, SEED)
            },
        },
        Pattern {
            name: "stone cluster",
            slug: "stone-cluster",
            note: "tightly packed, varied pebbles",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_stone_cluster(&r.inset(inset), s, SEED),
        },
        Pattern {
            name: "sunburst",
            slug: "sunburst",
            note: "rays from the centre, staggered so they stay about the spacing apart",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_sunburst(&r.inset(inset), s, a, Anchor::LOCAL),
        },
        Pattern {
            name: "terrazzo",
            slug: "terrazzo",
            note: "chips up to 4 × spacing, a third of them dark and hatched",
            has_angle: false,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, _, inset| fill_terrazzo(&r.inset(inset), s * 4.0, s * 0.7, SEED),
        },
        Pattern {
            name: "triangles",
            slug: "triangles",
            note: "three hatches 60° apart meeting at shared points, triangle height 1.5 × spacing",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            // fill_triangles keeps half its own pitch (0.75 s) off the wall;
            // growing by 0.25 s first leaves the gallery's usual s / 2.
            fill: |r, s, a, _| fill_triangles(&r.outset(s * 0.25), s * 1.5, a, Anchor::LOCAL),
        },
        Pattern {
            name: "truchet",
            slug: "truchet",
            note: "random quarter arcs, cells 3 × spacing",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_truchet(&r.inset(inset), s * 3.0, a, SEED, Anchor::LOCAL),
        },
        Pattern {
            name: "wave grid",
            slug: "wave-grid",
            note: "ogee: mirrored waves closing into curved diamonds, rows 1.5 × spacing apart",
            has_angle: true,
            default_angle: 90.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_wave_grid(&r.inset(inset), s * 1.5, a, Anchor::LOCAL),
        },
        Pattern {
            name: "waves",
            slug: "waves",
            note: "wavy hatch",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_waves(&r.inset(inset), s, a, Anchor::LOCAL),
        },
        Pattern {
            name: "wood",
            slug: "wood",
            note: "grain lines 1.5 × spacing apart, wandering, flowing around knots",
            has_angle: true,
            default_angle: 0.0,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_wood(&r.inset(inset), s * 1.5, a, SEED, Anchor::LOCAL),
        },
        Pattern {
            name: "zigzag",
            slug: "zigzag",
            note: "serpentine hatch, pen up when the join is too long",
            has_angle: true,
            default_angle: ZIGZAG_ANGLE,
            preview_spacing: COARSE_MM,
            fill: |r, s, a, inset| fill_zigzag_inset(r, s, a, Some(inset), Anchor::LOCAL),
        },
    ]
}

/// A flower motif, the demo tile for SVG import.
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

/// Every built-in `.pat` pattern, one per row, at a scale that reads.
fn cad_rows<'a>(shapes: &'a [(&'static str, Region)]) -> Vec<(String, Vec<Cell<'a>>)> {
    BUILTIN
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
            let cells = shapes
                .iter()
                .map(|(_, region)| Cell { region, strokes: fill_pat(&region.inset(0.8), &pattern, scale, 0.0, Anchor::LOCAL) })
                .collect();
            (name.to_lowercase(), cells)
        })
        .collect()
}

/// Bring-your-own examples: maths, a profile, an SVG tile, an SVG as is.
fn custom_rows<'a>(shapes: &'a [(&'static str, Region)]) -> Vec<(String, Vec<Cell<'a>>)> {
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let examples: Vec<(&str, Filler)> = vec![
        (
            "sin x · cos y",
            Box::new(|r: &Region| {
                // Levels off zero: the zero set of sin·cos is a straight grid.
                let levels: Vec<f64> = levels_every(-1.0, 1.0, 0.25).iter().map(|l| l + 0.125).collect();
                fill_isolines(&r.inset(1.0), 0.2, &levels, |p| (p.0 / 3.0).sin() * (p.1 / 3.0).cos())
            }),
        ),
        (
            "ripples, 3 sources",
            Box::new(|r: &Region| {
                let (minx, miny, maxx, maxy) = r.bounds_mm();
                let src = [(minx, miny), (maxx, miny + (maxy - miny) * 0.4), ((minx + maxx) / 2.0, maxy)];
                fill_isolines(&r.inset(1.0), 0.2, &levels_every(-2.5, 2.5, 1.0), move |p| {
                    src.iter().map(|s| ((p.0 - s.0).hypot(p.1 - s.1) / 1.3).sin()).sum()
                })
            }),
        ),
        (
            "profile: sawtooth",
            Box::new(|r: &Region| fill_profile(&r.inset(1.0), 2.5, 20.0, Anchor::LOCAL, |x| (x / 4.0).rem_euclid(1.0) * 1.8)),
        ),
        ("svg tile", Box::new(move |r: &Region| fill_svg_tile(&r.inset(1.0), &flower, 9.0, 15.0, 0.5, Anchor::LOCAL))),
    ];
    let big = parse_svg(FLOWER_SVG).unwrap();
    let mut rows: Vec<(String, Vec<Cell>)> = examples
        .iter()
        .map(|(label, f)| {
            let cells = shapes.iter().map(|(_, region)| Cell { region, strokes: f(region) }).collect();
            (label.to_string(), cells)
        })
        .collect();
    let as_is = shapes
        .iter()
        .map(|(_, region)| {
            let (minx, miny, maxx, maxy) = region.bounds_mm();
            let size = (maxx - minx).max(maxy - miny) * 1.15;
            let art = big.fit_width(size);
            let (ox, oy) = ((minx + maxx - size) / 2.0, (miny + maxy - size) / 2.0);
            let placed: Strokes = art.strokes.iter().map(|s| s.iter().map(|p| (p.0 + ox, p.1 + oy)).collect()).collect();
            Cell { region, strokes: region.clip_strokes(&placed) }
        })
        .collect();
    rows.push(("svg clipped as is".to_string(), as_is));
    rows
}

/// A lit ball on a floor, drawn here so the repository needs no photo.
/// Row 0 is the top of the picture.
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
            // Wall above the horizon, floor below, both shading downwards.
            let mut v = if y < 0.7 { 0.92 - 0.25 * y } else { 0.62 - 0.5 * (y - 0.7) };
            // Soft shadow on the floor, off to the lower right.
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

const DITHER_COLUMNS: [&str; 5] = ["dots", "lines", "lines at 45°", "lines 1 mm apart", "clipped by a star"];

/// Every dither method, as dots and as lines, on the square (and the star).
fn dither_rows<'a>(shapes: &'a [(&'static str, Region)]) -> Vec<(String, Vec<Cell<'a>>)> {
    let shape = |name: &str| &shapes.iter().find(|(n, _)| *n == name).unwrap().1;
    let (square, star) = (shape("square"), shape("star"));
    // The gallery draws y up; the picture's top row belongs at the top.
    let img = picture().flipped();
    DITHER_METHODS
        .iter()
        .map(|&(label, method)| {
            let base = DitherOptions { method, seed: SEED, ..DitherOptions::new(0.5) };
            let variants = [
                (square, DitherOptions { marks: Marks::Dots, ..base }),
                (square, base),
                (square, DitherOptions { angle: 45.0, ..base }),
                (square, DitherOptions { resolution: 0.3, line_spacing: 1.0, ..base }),
                (star, DitherOptions { line_spacing: 0.7, ..base }),
            ];
            let cells = variants.into_iter().map(|(region, o)| Cell { region, strokes: fill_dither(region, &img, &o) }).collect();
            (label.to_string(), cells)
        })
        .collect()
}

/// CSS-style placements: (label, size, position, repeat).
const PLACEMENTS: [(&str, &str, &str, &str); 6] = [
    ("cover", "cover", "center", "no-repeat"),
    ("contain", "contain", "center", "no-repeat"),
    ("fill (stretched)", "fill", "center", "no-repeat"),
    ("12 mm, repeat", "12", "center", "repeat"),
    ("14 mm, repeat-x, top", "14", "top", "repeat-x"),
    ("20 mm, top left", "20", "top left", "no-repeat"),
];

/// The picture (and the flower SVG) placed like a CSS background.
fn placement_rows<'a>(shapes: &'a [(&'static str, Region)]) -> Vec<(String, Vec<Cell<'a>>)> {
    let img = picture();
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let mut rows: Vec<(String, Vec<Cell>)> = PLACEMENTS
        .iter()
        .map(|&(label, size, position, repeat)| {
            let o = DitherOptions { placement: Placement::parse(size, position, repeat).unwrap(), ..DitherOptions::new(0.4) };
            let cells = shapes.iter().map(|(_, region)| Cell { region, strokes: fill_dither(region, &img, &o) }).collect();
            (format!("picture · {label}"), cells)
        })
        .collect();
    for (label, size, position, repeat) in [("contain", "contain", "center", "no-repeat"), ("8 mm, repeat", "8", "center", "repeat"), ("10 mm, repeat-y, left", "10", "left", "repeat-y")] {
        let place = Placement::parse(size, position, repeat).unwrap();
        let cells = shapes.iter().map(|(_, region)| Cell { region, strokes: fill_svg_placed(region, &flower, &place) }).collect();
        rows.push((format!("svg · {label}"), cells));
    }
    rows
}

/// The picture as a PNG data URI, to show next to its dithers.
fn picture_data_uri(img: &Greyscale) -> String {
    let luma = image::GrayImage::from_fn(img.width() as u32, img.height() as u32, |x, y| {
        image::Luma([(img.pixel(x as usize, y as usize) * 255.0).round() as u8])
    });
    let mut png = Vec::new();
    luma.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("data:image/png;base64,");
    for chunk in png.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for k in 0..4 {
            out.push(if k <= chunk.len() { B64[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

struct Cell<'a> {
    region: &'a Region,
    strokes: Strokes,
}

fn render_sheet(
    title: &str,
    note: &str,
    columns: &[&str],
    rows: &[(String, Vec<Cell>)],
) -> String {
    render_sheet_y(title, note, columns, rows, false)
}

/// `y_down`: draw y pointing down, as SVG and plotters do, so "top" in a
/// placement reads as top. The other sheets draw y up.
fn render_sheet_y(title: &str, note: &str, columns: &[&str], rows: &[(String, Vec<Cell>)], y_down: bool) -> String {
    let n_cols = columns.len() as f64;
    let n_rows = rows.len() as f64;
    // Room for the longest row label at 8 px Georgia (about 4.6 px a character).
    let longest = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0) as f64;
    let label_w = LABEL_W.max(12.0 + longest * 4.6 + 12.0);
    let width = label_w + n_cols * CELL_W + 16.0;
    let height = TITLE_H + HEADER_H + n_rows * CELL_H + 12.0;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 0 {width} {height}'>\n"
    ));
    svg.push_str(&format!(
        "<rect width='100%' height='100%' fill='{PAPER}'/>\n"
    ));
    svg.push_str(&format!(
        "<style>text{{font-family:Georgia,serif;fill:{LABEL}}}</style>\n"
    ));
    svg.push_str(&format!(
        "<text x='12' y='16' font-size='11'>{title}</text>\n"
    ));
    svg.push_str(&format!(
        "<text x='12' y='30' font-size='8'>{note}</text>\n"
    ));

    for (i, name) in columns.iter().enumerate() {
        let x = label_w + (i as f64 + 0.5) * CELL_W;
        let y = TITLE_H + HEADER_H - 6.0;
        svg.push_str(&format!(
            "<text x='{x}' y='{y}' font-size='8' text-anchor='middle'>{name}</text>\n"
        ));
    }

    for (ri, (label, cells)) in rows.iter().enumerate() {
        let y = TITLE_H + HEADER_H + (ri as f64 + 0.5) * CELL_H + 3.0;
        svg.push_str(&format!(
            "<text x='12' y='{y}' font-size='8'>{label}</text>\n"
        ));
        for (ci, cell) in cells.iter().enumerate() {
            let ox = label_w + ci as f64 * CELL_W;
            let oy = TITLE_H + HEADER_H + ri as f64 * CELL_H;
            append_cell(&mut svg, cell.region, &cell.strokes, ox, oy, y_down);
        }
    }

    svg.push_str("</svg>\n");
    svg
}

fn append_cell(svg: &mut String, region: &Region, strokes: &Strokes, ox: f64, oy: f64, y_down: bool) {
            let (minx, miny, maxx, maxy) = region.bounds_mm();
    let pad = 8.0;
            let w = (maxx - minx).max(1e-6);
            let h = (maxy - miny).max(1e-6);
    let scale = ((CELL_W - 2.0 * pad) / w).min((CELL_H - 2.0 * pad) / h);
            let cx = (minx + maxx) / 2.0;
            let cy = (miny + maxy) / 2.0;
    let sx = ox + CELL_W / 2.0;
    let sy = oy + CELL_H / 2.0;
    let xform = format!(
        "translate({sx:.2},{sy:.2}) scale({scale:.4},{neg:.4}) translate({tcx:.2},{tcy:.2})",
        neg = if y_down { scale } else { -scale },
        tcx = -cx,
        tcy = -cy,
    );
    let sw_fill = 0.22 / scale;
    let sw_wall = 0.28 / scale;

            svg.push_str(&format!(
        "<g transform='{xform}' fill='none' stroke='{WALL}' stroke-width='{sw_wall}' stroke-linejoin='round'>\n"
    ));
    for ring in region.all_rings_mm() {
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

                svg.push_str(&format!(
        "<g transform='{xform}' fill='none' stroke='{INK}' stroke-width='{sw_fill}' stroke-linecap='round' stroke-linejoin='round'>\n"
    ));
    for pts in strokes {
        let d = pts
            .iter()
            .map(|p| format!("{:.3},{:.3}", p.0, p.1))
            .collect::<Vec<_>>()
            .join(" L ");
        svg.push_str(&format!("<path d='M {d}'/>\n"));
    }
    svg.push_str("</g>\n");
}

fn variant_rows<'a>(
    pattern: &Pattern,
    shapes: &'a [(&'static str, Region)],
) -> Vec<(String, Vec<Cell<'a>>)> {
    let mut styles = vec![(FINE_MM, pattern.default_angle)];
    if pattern.has_angle {
        // At 45° the scale centres line up on a square grid again and the
        // arches stop reading; hanging sideways they stay scales.
        let last = if pattern.slug == "scales" { 90.0 } else { 45.0 };
        styles.push((MID_MM, pattern.default_angle + 20.0));
        styles.push((COARSE_MM, pattern.default_angle + last));
    } else {
        styles.push((COARSE_MM, pattern.default_angle));
    }
    let specs: Vec<(String, f64, f64, f64)> = styles
        .into_iter()
        .map(|(spacing, angle)| {
            let label = if pattern.has_angle {
                format!("{spacing} mm · {angle:.0}°")
            } else {
                format!("{spacing} mm · border {:.1} mm", border_for(spacing))
            };
            (label, spacing, angle, border_for(spacing))
        })
        .collect();

    specs
        .into_iter()
        .map(|(label, spacing, angle, inset)| {
            let cells = shapes
                .iter()
                .map(|(_, region)| Cell {
                    region,
                    strokes: (pattern.fill)(region, spacing, angle, inset),
                })
                .collect();
            (label, cells)
        })
        .collect()
}

/// README strip cell, in SVG units. Strokes are drawn much heavier than on
/// the sheets: a README shows the whole strip at about page width.
const STRIP_W: f64 = 120.0;
const STRIP_H: f64 = 112.0;
const STRIP_LABEL: f64 = 18.0;
const STRIP_INK: f64 = 0.8;
const STRIP_WALL: f64 = 0.9;

/// One row of cells with a label under each, as a standalone SVG.
fn render_strip(cells: &[(String, &Region, Strokes)]) -> String {
    render_strip_in(cells, &[INK])
}

/// Muted inks for telling pen-downs apart: stroke i gets `PENS[i % len]`.
const PENS: [&str; 5] = ["#2a2622", "#b0573a", "#3f6b8a", "#6f8a3a", "#8a4f7a"];

fn render_strip_in(cells: &[(String, &Region, Strokes)], inks: &[&str]) -> String {
    render_strip_y(cells, inks, false)
}

/// `y_down` as in [`render_sheet_y`].
fn render_strip_y(cells: &[(String, &Region, Strokes)], inks: &[&str], y_down: bool) -> String {
    let w = STRIP_W * cells.len() as f64;
    let h = STRIP_H + STRIP_LABEL;
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w} {h}' width='{w2}' height='{h2}'>\n\
         <rect width='{w}' height='{h}' fill='{PAPER}'/>\n",
        w2 = w * 2.0,
        h2 = h * 2.0,
    );
    let pt = |p: &(f64, f64)| format!("{:.2},{:.2}", p.0, p.1);
    for (i, (label, region, strokes)) in cells.iter().enumerate() {
        let ox = i as f64 * STRIP_W;
        let (minx, miny, maxx, maxy) = region.bounds_mm();
        let pad = 7.0;
        let scale = ((STRIP_W - 2.0 * pad) / (maxx - minx).max(1e-6)).min((STRIP_H - 2.0 * pad) / (maxy - miny).max(1e-6));
        let xform = format!(
            "translate({:.2},{:.2}) scale({scale:.4},{:.4}) translate({:.3},{:.3})",
            ox + STRIP_W / 2.0,
            STRIP_H / 2.0,
            if y_down { scale } else { -scale },
            -(minx + maxx) / 2.0,
            -(miny + maxy) / 2.0,
        );
        svg.push_str(&format!(
            "<g transform='{xform}' fill='none' stroke-linecap='round' stroke-linejoin='round'>\n<g stroke='{WALL}' stroke-width='{:.4}'>\n",
            STRIP_WALL / scale
        ));
        for ring in region.all_rings_mm() {
            let d: Vec<String> = ring.iter().map(pt).collect();
            svg.push_str(&format!("<path d='M{}Z'/>\n", d.join("L")));
        }
        svg.push_str(&format!("</g>\n<g stroke='{INK}' stroke-width='{:.4}'>\n", STRIP_INK / scale));
        for (k, s) in strokes.iter().enumerate() {
            let d: Vec<String> = s.iter().map(pt).collect();
            let ink = inks[k % inks.len()];
            let colour = if ink == INK { String::new() } else { format!(" stroke='{ink}'") };
            svg.push_str(&format!("<path{colour} d='M{}'/>\n", d.join("L")));
        }
        svg.push_str("</g>\n</g>\n");
        svg.push_str(&format!(
            "<text x='{:.1}' y='{:.1}' font-family='Georgia, serif' font-size='10' fill='{LABEL}' text-anchor='middle'>{label}</text>\n",
            ox + STRIP_W / 2.0,
            STRIP_H + 8.0
        ));
    }
    svg.push_str("</svg>\n");
    svg
}

const GRID_COLS: usize = 12;
const GRID_CELL_W: f64 = 70.0;
const GRID_PATTERN_H: f64 = 70.0;
const GRID_LABEL_H: f64 = 22.0;
const GRID_PAD: f64 = 6.0;

/// CADA / sheet aliases that reuse the same fill as another tile — one cell, both names.
const GRID_SKIP_SLUGS: &[&str] = &[];

/// Spacing tuned for a ~40 mm catalog square (coarser than the comparison table).
fn grid_spacing_for(p: &Pattern) -> f64 {
    let dense = matches!(
        p.slug,
        "arena"
            | "grainy-soil"
            | "gravelly-soil"
            | "diabase"
            | "feldspar"
            | "dacite"
            | "ophitic"
            | "erosion-lines"
            | "compacted-soil"
            | "clay-soil"
            | "layered-soil"
            | "organic-topsoil"
            | "loose-soil"
            | "stipple"
            | "stipple-noise"
            | "stipple-edge"
            | "dots"
            | "dots-hex"
            | "hilbert"
            | "stone-cluster"
            | "fill"
            | "pebbled-soil"
            | "hornblende"
    );
    if dense {
        return COARSE_MM * 1.35;
    }
    if p.preview_spacing <= FINE_MM {
        return COARSE_MM;
    }
    if p.preview_spacing <= MID_MM {
        return COARSE_MM;
    }
    p.preview_spacing
}

fn grid_label(p: &Pattern) -> String {
    p.name.to_string()
}

/// Every pattern once, on a square, name underneath — no shape matrix.
///
/// The tiles are sorted by name so the sheet reads like an index: you can
/// find a pattern without scanning the whole page.
fn render_pattern_grid(patterns: &[Pattern], region: &Region) -> String {
    let mut patterns: Vec<&Pattern> = patterns
        .iter()
        .filter(|p| !GRID_SKIP_SLUGS.contains(&p.slug))
        .collect();
    patterns.sort_by_key(|p| grid_label(p).to_lowercase());
    let cols = GRID_COLS;
    let rows = (patterns.len() + cols - 1) / cols;
    let w = GRID_CELL_W * cols as f64;
    let row_h = GRID_PATTERN_H + GRID_LABEL_H;
    let h = row_h * rows as f64 + 16.0;
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w} {h}' width='{w2}' height='{h2}'>\n\
         <rect width='{w}' height='{h}' fill='{PAPER}'/>\n\
         <style>text{{font-family:Georgia,serif;fill:{LABEL}}}</style>\n",
        w2 = w * 1.5,
        h2 = h * 1.5,
    );
    let pt = |p: &(f64, f64)| format!("{:.2},{:.2}", p.0, p.1);
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let fit = ((GRID_CELL_W - 2.0 * GRID_PAD) / (maxx - minx).max(1e-6))
        .min((GRID_PATTERN_H - 2.0 * GRID_PAD) / (maxy - miny).max(1e-6));

    for (idx, pattern) in patterns.iter().enumerate() {
        let col = idx % cols;
        let row = idx / cols;
        let ox = col as f64 * GRID_CELL_W;
        let oy = row as f64 * row_h + 8.0;
        let spacing = grid_spacing_for(pattern);
        let strokes = (pattern.fill)(
            region,
            spacing,
            pattern.default_angle,
            border_for(spacing),
        );
        let xform = format!(
            "translate({:.2},{:.2}) scale({fit:.4},{:.4}) translate({:.3},{:.3})",
            ox + GRID_CELL_W / 2.0,
            oy + GRID_PATTERN_H / 2.0,
            -fit,
            -(minx + maxx) / 2.0,
            -(miny + maxy) / 2.0,
        );
        svg.push_str(&format!(
            "<g transform='{xform}' fill='none' stroke-linecap='round' stroke-linejoin='round'>\n\
             <g stroke='{WALL}' stroke-width='{:.4}'>\n",
            STRIP_WALL / fit
        ));
        for ring in region.all_rings_mm() {
            let d: Vec<String> = ring.iter().map(pt).collect();
            svg.push_str(&format!("<path d='M{}Z'/>\n", d.join("L")));
        }
        svg.push_str(&format!("</g>\n<g stroke='{INK}' stroke-width='{:.4}'>\n", STRIP_INK / fit));
        for s in &strokes {
            let d: Vec<String> = s.iter().map(pt).collect();
            svg.push_str(&format!("<path d='M{}'/>\n", d.join("L")));
        }
        svg.push_str("</g>\n</g>\n");
        let label = grid_label(pattern);
        svg.push_str(&format!(
            "<text x='{:.1}' y='{:.1}' font-size='7.5' text-anchor='middle'>{}</text>\n",
            ox + GRID_CELL_W / 2.0,
            oy + GRID_PATTERN_H + 14.0,
            label,
        ));
    }
    svg.push_str("</svg>\n");
    svg
}

/// The strips the README is illustrated with, in `docs/img/`.
fn write_readme_strips(shapes: &[(&'static str, Region)], patterns: &[Pattern]) {
    let dir = "docs/img";
    std::fs::create_dir_all(dir).unwrap();
    let shape = |name: &str| &shapes.iter().find(|(n, _)| *n == name).unwrap().1;
    let by_slug = |slug: &str| patterns.iter().find(|p| p.slug == slug).unwrap_or_else(|| panic!("no pattern {slug}"));
    let cell = |slug: &str, region: &'static str, spacing: f64| {
        let p = by_slug(slug);
        let r = shape(region);
        (p.name.to_string(), r, (p.fill)(r, spacing, p.default_angle, border_for(spacing)))
    };
    let row = |region: &'static str, fills: &[(&str, f64)]| -> Vec<(String, &Region, Strokes)> {
        fills.iter().map(|&(slug, spacing)| cell(slug, region, spacing)).collect()
    };
    let strips: Vec<Strip> = vec![
        (
            "tour",
            row(
                "doughnut",
                &[("concentric", MID_MM), ("wave-grid", MID_MM), ("scales", MID_MM), ("gravel", MID_MM), ("terrazzo", MID_MM), ("truchet", MID_MM)],
            ),
        ),
        (
            "lines",
            row(
                "blob",
                &[("rectilinear", MID_MM), ("zigzag", MID_MM), ("grid", COARSE_MM), ("triangles", COARSE_MM), ("concentric", MID_MM), ("hilbert", MID_MM)],
            ),
        ),
        (
            "lattices",
            row(
                "blob",
                &[("honeycomb", COARSE_MM), ("dots-hex", MID_MM), ("waves", MID_MM), ("wave-grid", MID_MM), ("scales", MID_MM), ("sunburst", MID_MM)],
            ),
        ),
        (
            "scatter",
            row(
                "blob",
                &[("stipple", MID_MM), ("stipple-edge", MID_MM), ("gravel", MID_MM), ("gravel-clipped", MID_MM), ("grass", MID_MM), ("scribble", MID_MM)],
            ),
        ),
        (
            "materials",
            row(
                "blob",
                &[("marble", FINE_MM), ("wood", MID_MM), ("cracks", MID_MM), ("terrazzo", MID_MM), ("chess", MID_MM), ("truchet", MID_MM)],
            ),
        ),
        (
            "geology",
            row(
                "blob",
                &[
                    ("granite", COARSE_MM),
                    ("arena", FINE_MM),
                    ("breccia", COARSE_MM),
                    ("coal", COARSE_MM),
                    ("gneiss", COARSE_MM),
                    ("ferrous", COARSE_MM),
                ],
            ),
        ),
        (
            "geology-2",
            row(
                "blob",
                &[
                    ("diorite", COARSE_MM),
                    ("conglomerate", COARSE_MM),
                    ("masonry", COARSE_MM),
                    ("diatom", COARSE_MM),
                    ("amphibolite", COARSE_MM),
                    ("hornblende", COARSE_MM),
                ],
            ),
        ),
        (
            "ground",
            row(
                "blob",
                &[
                    ("fine-gravel", FINE_MM),
                    ("coarse-stone", COARSE_MM),
                    ("pebble-scatter", COARSE_MM),
                    ("mixed-rocks", COARSE_MM),
                    ("cracked-earth", COARSE_MM),
                    ("grainy-soil", FINE_MM),
                ],
            ),
        ),
        (
            "ground-2",
            row(
                "blob",
                &[
                    ("erosion-lines", FINE_MM),
                    ("stone-cluster", COARSE_MM),
                    ("gravelly-soil", FINE_MM),
                    ("mulch", COARSE_MM),
                    ("clay-soil", FINE_MM),
                    ("pebbled-soil", COARSE_MM),
                ],
            ),
        ),
        (
            "cad",
            [("BRICK", 1.6), ("HERRINGBONE", 1.6), ("PARQUET", 1.0), ("EARTH", 3.2), ("ANSI37", 2.0), ("CROSS", 1.6)]
                .iter()
                .map(|&(name, scale)| {
                    let r = shape("blob");
                    (name.to_lowercase(), r, fill_pat(&r.inset(0.8), &builtin(name).unwrap(), scale, 0.0, Anchor::LOCAL))
                })
                .collect(),
        ),
        (
            "shapes",
            shapes.iter().map(|(name, r)| (name.to_string(), r, fill_concentric(r, MID_MM, Some(border_for(MID_MM))))).collect(),
        ),
    ];
    for (name, cells) in &strips {
        let path = format!("{dir}/{name}.svg");
        std::fs::write(&path, render_strip(cells)).unwrap();
        println!("wrote {path}");
    }
    // Zigzag, one colour per pen-down, against how many strokes a plain
    // hatch at the same spacing needs.
    let hero: Vec<(String, &Region, Strokes)> = ["doughnut", "blob", "star", "worst"]
        .iter()
        .map(|&name| {
            let r = shape(name);
            let border = Some(border_for(MID_MM));
            let zig = fill_zigzag_inset(r, MID_MM, 0.0, border, Anchor::LOCAL);
            let hatch = fill_rectilinear(r, MID_MM, 0.0, border, Anchor::LOCAL).len();
            (format!("{} strokes (hatch: {hatch})", zig.len()), r, zig)
        })
        .collect();
    std::fs::write(format!("{dir}/hero.svg"), render_strip_in(&hero, &PENS)).unwrap();
    println!("wrote {dir}/hero.svg");
    // The picture itself, then five ways to plot it.
    let square = shape("square");
    let img = picture();
    let upright = img.flipped();
    let base = DitherOptions { seed: SEED, ..DitherOptions::new(0.5) };
    let dithers = [
        ("bayer 4, dots", DitherOptions { method: Dither::Bayer(4), marks: Marks::Dots, ..base }),
        ("halftone, lines at 45°", DitherOptions { method: Dither::Halftone(6), angle: 45.0, ..base }),
        ("floyd-steinberg, lines", base),
        ("atkinson, dots", DitherOptions { method: Dither::Atkinson, marks: Marks::Dots, ..base }),
        ("jarvis, lines 1 mm apart", DitherOptions { method: Dither::JarvisJudiceNinke, resolution: 0.3, line_spacing: 1.0, ..base }),
    ];
    let mut cells: Vec<(String, &Region, Strokes)> = vec![("the picture".to_string(), square, vec![])];
    cells.extend(dithers.iter().map(|(label, o)| (label.to_string(), square, fill_dither(square, &upright, o))));
    let side = (STRIP_W - 14.0).min(STRIP_H - 14.0);
    let backdrop = format!(
        "<image x='{:.2}' y='{:.2}' width='{side:.2}' height='{side:.2}' href='{}'/>\n</svg>\n",
        (STRIP_W - side) / 2.0,
        (STRIP_H - side) / 2.0,
        picture_data_uri(&img)
    );
    let strip = render_strip(&cells).replacen("</svg>\n", &backdrop, 1);
    std::fs::write(format!("{dir}/dither.svg"), strip).unwrap();
    println!("wrote {dir}/dither.svg");

    // The picture as a background of real shapes, drawn y-down like SVG.
    let flower = parse_svg(FLOWER_SVG).unwrap();
    let place = |size: &str, position: &str, repeat: &str| Placement::parse(size, position, repeat).unwrap();
    let dith = |region: &Region, p: Placement| fill_dither(region, &img, &DitherOptions { placement: p, ..DitherOptions::new(0.5) });
    let backgrounds: Vec<(String, &Region, Strokes)> = vec![
        ("cover".into(), shape("doughnut"), dith(shape("doughnut"), place("cover", "center", "no-repeat"))),
        ("contain".into(), shape("star"), dith(shape("star"), place("contain", "center", "no-repeat"))),
        ("14 mm, repeat".into(), shape("blob"), dith(shape("blob"), place("14", "center", "repeat"))),
        ("16 mm, repeat-x, top".into(), shape("square"), dith(shape("square"), place("16", "top", "repeat-x"))),
        ("svg, 9 mm, repeat".into(), shape("doughnut"), fill_svg_placed(shape("doughnut"), &flower, &place("9", "center", "repeat"))),
        ("svg, contain".into(), shape("star"), fill_svg_placed(shape("star"), &flower, &place("contain", "center", "no-repeat"))),
    ];
    std::fs::write(format!("{dir}/placement.svg"), render_strip_y(&backgrounds, &[INK], true)).unwrap();
    println!("wrote {dir}/placement.svg");

    let custom: Vec<(String, &Region, Strokes)> =
        custom_rows(shapes).into_iter().map(|(label, cells)| (label, cells[2].region, cells.into_iter().nth(2).unwrap().strokes)).collect();
    std::fs::write(format!("{dir}/custom.svg"), render_strip(&custom)).unwrap();
    println!("wrote {dir}/custom.svg");
}

fn main() {
    let shapes = shapes();
    let patterns = patterns();
    let shape_names: Vec<&str> = shapes.iter().map(|(n, _)| *n).collect();
    let dir = "examples/gallery";
    std::fs::create_dir_all(dir).unwrap();

    let mut strokes_total = 0usize;
    let mut mm_total = 0.0;

    for pattern in &patterns {
        let rows = variant_rows(pattern, &shapes);
        for (_, cells) in &rows {
            for cell in cells {
                strokes_total += cell.strokes.len();
                mm_total += cell.strokes.iter().map(|p| polyline_length(p)).sum::<f64>();
            }
        }
        let note = format!(
            "{} · border is half the spacing",
            pattern.note
        );
        let svg = render_sheet(pattern.name, &note, &shape_names, &rows);
        let path = format!("{dir}/{}.svg", pattern.slug);
        std::fs::write(&path, svg).unwrap();
        println!("wrote {path}");
    }

    let pattern_names: Vec<&str> = patterns.iter().map(|p| p.name).collect();
    let mut table_rows = Vec::new();
    for (name, region) in &shapes {
        let cells = patterns
            .iter()
            .map(|p| Cell {
                region,
                strokes: (p.fill)(
                    region,
                    p.preview_spacing,
                    p.default_angle,
                    border_for(p.preview_spacing),
                ),
            })
            .collect::<Vec<Cell>>();
        table_rows.push(((*name).to_string(), cells));
    }
    for (_, cells) in &table_rows {
        for cell in cells {
            strokes_total += cell.strokes.len();
            mm_total += cell.strokes.iter().map(|p| polyline_length(p)).sum::<f64>();
        }
    }
    let note = format!(
        "coarse {COARSE_MM} mm · border {:.1} mm · wall gray, fill ink",
        border_for(COARSE_MM)
    );
    let table = render_sheet("penfill", &note, &pattern_names, &table_rows);
    std::fs::write(format!("{dir}/gallery.svg"), table).unwrap();

    let square = shapes.iter().find(|(n, _)| *n == "square").map(|(_, r)| r).expect("square");
    let all = render_pattern_grid(&patterns_sorted(), square);
    std::fs::write(format!("{dir}/all-patterns.svg"), all).unwrap();
    println!("wrote {dir}/all-patterns.svg");

    let cad = render_sheet(
        "CAD patterns",
        "every built-in .pat hatch · parse_pat reads any AutoCAD / LibreCAD / QCAD file",
        &shape_names,
        &cad_rows(&shapes),
    );
    std::fs::write(format!("{dir}/cad.svg"), cad).unwrap();
    println!("wrote {dir}/cad.svg");
    let custom = render_sheet(
        "your own",
        "fill_isolines(f) · fill_profile(g) · fill_svg_tile(svg) · Region::clip_strokes(any drawing)",
        &shape_names,
        &custom_rows(&shapes),
    );
    std::fs::write(format!("{dir}/custom.svg"), custom).unwrap();
    println!("wrote {dir}/custom.svg");

    let dither = render_sheet(
        "dither",
        "a picture dithered on 0.5 mm cells · dots: a tick per dark cell · lines: dark runs joined, back and forth",
        &DITHER_COLUMNS,
        &dither_rows(&shapes),
    );
    std::fs::write(format!("{dir}/dither.svg"), dither).unwrap();
    println!("wrote {dir}/dither.svg");

    let placed = render_sheet_y(
        "placement",
        "a picture or an SVG placed like a CSS background — size · position · repeat — and cut by the outline (drawn y-down, as in SVG)",
        &shape_names,
        &placement_rows(&shapes),
        true,
    );
    std::fs::write(format!("{dir}/placement.svg"), placed).unwrap();
    println!("wrote {dir}/placement.svg");

    write_readme_strips(&shapes, &patterns);
    for slug in [
        "rectilinear",
        "diagonal",
        "zigzag",
        "grid",
        "honeycomb",
        "concentric",
        "spiral",
        "dots",
        "dots-hex",
        "waves",
        "gallery",
    ] {
        let png = format!("{dir}/{slug}.png");
        if std::path::Path::new(&png).exists() {
            std::fs::remove_file(&png).ok();
        }
    }
    println!(
        "table: {dir}/gallery.svg · {strokes_total} strokes, {mm_total:.1} mm of ink"
    );
}
