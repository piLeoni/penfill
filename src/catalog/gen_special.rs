//! The specialists: generators with a look of their own — rings from the
//! boundary, hand scribble, the space-filling and tiled curves, radiating
//! rays, the natural textures (wood, marble, the seas), the vegetation
//! symbols, and the four that cover only part of their area (growth, the
//! isoline crowns and blobs, clouds) and say which part.
//!
//! Each one wraps an algorithm module and converts at the boundary, so a
//! user only ever gives lengths in mm and one `spacing` that means the same
//! as everywhere else: the distance between neighbouring elements, centre
//! to centre.

use super::points::{sites, Arrange};
use super::schema::{Args, Default, Kind, Opt, CONTRAST, DENSITY, FEATURE, TOWARD};
use super::tone::Tone;
use super::Generator;
use crate::geo::{Pt, Region, Strokes};
use crate::pattern::{Canvas, Tools};
use crate::random::Rng;

// ---------------------------------------------------------------------------
// Option builders
// ---------------------------------------------------------------------------

const fn number(key: &'static str, min: f64, max: f64, default: f64, doc: &'static str) -> Opt {
    Opt { key, kind: Kind::Number { min, max }, default: Default::Num(default), doc }
}

const fn count(key: &'static str, min: i64, max: i64, default: f64, doc: &'static str) -> Opt {
    Opt { key, kind: Kind::Count { min, max }, default: Default::Num(default), doc }
}

const fn length(key: &'static str, default: Default, doc: &'static str) -> Opt {
    Opt { key, kind: Kind::Length, default, doc }
}

const fn flag(key: &'static str, doc: &'static str) -> Opt {
    Opt { key, kind: Kind::Flag, default: Default::Flag(false), doc }
}

/// The anchored places for elements `pitch` apart over `within`, each with
/// its own randomness: what the scattered specialists put their marks at.
fn anchored_sites(c: &Canvas, t: &Tools, within: &Region, pitch: f64) -> Vec<(Pt, Rng)> {
    sites(t, c.seed, within, pitch, Arrange::Blue, 0.0, c.angle).into_iter().map(|s| (s.at, s.rng)).collect()
}

// ---------------------------------------------------------------------------
// rings
// ---------------------------------------------------------------------------

const FROM: Opt = Opt {
    key: "from",
    kind: Kind::Choice(&["outline", "edges"]),
    default: Default::Text("outline"),
    doc: "What the rings follow: `outline`, the outer edge inset again and again (holes stop the rings; thin parts get a centreline); `edges`, every edge, holes included, each pushing rings of its own (a network of streets round blocks fills evenly).",
};
const SPIRAL: Opt = flag(
    "spiral",
    "One continuous path winding inward a spacing per lap, instead of closed rings: the fewest pen lifts. Follows the outline; holes cut it.",
);

pub static RINGS: Generator = Generator {
    name: "rings",
    about: "Rings from the boundary inward, a spacing apart: concentric with the outline, offset from every edge, or one continuous spiral.",
    spacing: 0.8,
    angle: 0.0,
    options: &[&FROM, &SPIRAL],
    own_mask: false,
    inset: 0.0,
    draw: draw_rings,
};

fn draw_rings(c: &Canvas, _t: &Tools, a: &Args) -> Strokes {
    let s = c.spacing;
    if a.flag("spiral") {
        return crate::spiral::fill_spiral(&c.limit, s);
    }
    match a.word("from") {
        "edges" => crate::concentric::fill_offsets(&c.limit, s, None),
        _ => crate::concentric::fill_concentric(&c.limit, s, None),
    }
}

// ---------------------------------------------------------------------------
// scribble
// ---------------------------------------------------------------------------

const LAYERS: Opt = count("layers", 1, 3, 2.0, "Layers of scribble, each turned about 50° from the first: 1 a light shade, 3 the darkest.");
const LOOSE: Opt = number("loose", 0.0, 1.0, 0.5, "How messy, 0 (tidy, even zigzags) to 1 (wandering ends, uneven passes, tilted patches).");
const STROKE: Opt = length("length", Default::Rel(9.0), "The length of one pass of the pen, end to end, in mm.");

pub static SCRIBBLE: Generator = Generator {
    name: "scribble",
    about: "Hand shading with a fine pen: loose zigzag strokes with rounded turns, in overlapping patches, layered at different angles.",
    spacing: 1.0,
    angle: 30.0,
    options: &[&LAYERS, &LOOSE, &STROKE, &DENSITY, &CONTRAST, &FEATURE, &TOWARD],
    own_mask: false,
    inset: 0.0,
    draw: draw_scribble,
};

fn draw_scribble(c: &Canvas, _t: &Tools, a: &Args) -> Strokes {
    let layers = a.count("layers").clamp(1, 3) as usize;
    let p = crate::scribble::Scribble { spacing: c.spacing, angle: c.angle, layers, loose: a.num("loose"), length: a.num("length") };
    let tone = Tone::new(a, c.region, c.seed);
    let keep = |at: Pt, layer: usize| {
        if tone.is_even() {
            return true;
        }
        // Like hatch's engraving tone: the first layer thins patch by patch,
        // later layers only come in where it is darker.
        let r = Rng::new(crate::anchor::cell_seed(c.seed ^ 0x5c7, (at.0 * 8.0) as i64, (at.1 * 8.0) as i64)).unit();
        tone.at(at) >= (layer as f64 + r) / layers as f64
    };
    crate::scribble::scribble(&c.limit, c.origin, &p, c.seed, keep)
}

// ---------------------------------------------------------------------------
// Curves and tilings
// ---------------------------------------------------------------------------

pub static HILBERT: Generator = Generator {
    name: "hilbert",
    about: "One unbroken space-filling Hilbert curve visiting the whole area cell by cell; cells between half a spacing and a spacing.",
    spacing: 1.0,
    angle: 0.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_hilbert,
};

fn draw_hilbert(c: &Canvas, _t: &Tools, _a: &Args) -> Strokes {
    crate::hilbert::fill_hilbert(&c.limit, c.spacing, None)
}

pub static TRUCHET: Generator = Generator {
    name: "truchet",
    about: "Truchet tiles: square cells a spacing wide, each with two quarter arcs turned at random, chaining into long winding paths.",
    spacing: 2.0,
    angle: 0.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_truchet,
};

fn draw_truchet(c: &Canvas, _t: &Tools, _a: &Args) -> Strokes {
    crate::truchet::fill_truchet(&c.limit, c.spacing.max(0.3), c.angle, c.seed, c.anchor)
}

const WALLS: Opt = length("walls", Default::Num(0.0), "Draw every wall twice, this far apart, in mm (0: single walls, the hexagons touching).");

pub static HONEYCOMB: Generator = Generator {
    name: "honeycomb",
    about: "Hexagonal cells a spacing across (flat to flat), each wall drawn once, or doubled `walls` apart.",
    spacing: 2.0,
    angle: 0.0,
    options: &[&WALLS],
    own_mask: false,
    inset: 0.0,
    draw: draw_honeycomb,
};

fn draw_honeycomb(c: &Canvas, _t: &Tools, a: &Args) -> Strokes {
    let walls = a.num("walls").clamp(0.0, c.spacing * 0.9);
    crate::honeycomb::fill_honeycomb(&c.limit, c.spacing, c.angle, walls, c.anchor)
}

pub static SCALES: Generator = Generator {
    name: "scales",
    about: "Fish scales: rows of half circles a spacing apart, each row resting on the tips of the one below; roof tiles and drawn water.",
    spacing: 1.5,
    angle: 0.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_scales,
};

fn draw_scales(c: &Canvas, _t: &Tools, _a: &Args) -> Strokes {
    crate::scales::fill_scales(&c.limit, c.spacing, c.angle, c.anchor)
}

const CENTRE: Opt = Opt {
    key: "centre",
    kind: Kind::Lengths,
    default: Default::None,
    doc: "Where the rays come from, `[x, y]` in page mm. Default: the middle of the shape. Give the same centre to neighbouring areas and they share one burst.",
};

pub static SUNBURST: Generator = Generator {
    name: "sunburst",
    about: "Rays from a centre, about a spacing apart at every radius: new rays join in rings where the gaps open, so the centre never crowds.",
    spacing: 1.0,
    angle: 0.0,
    options: &[&CENTRE],
    own_mask: false,
    inset: 0.0,
    draw: draw_sunburst,
};

fn draw_sunburst(c: &Canvas, _t: &Tools, a: &Args) -> Strokes {
    // A burst belongs to its shape: even with a global anchor its default
    // centre is the shape's middle, not the page origin.
    let own = if c.anchor.is_global() { crate::anchor::centre(c.region) } else { c.origin };
    let centre = a.pair("centre").unwrap_or(own);
    crate::radial::fill_sunburst_from(&c.limit, c.spacing, c.angle, centre)
}

// ---------------------------------------------------------------------------
// Natural textures
// ---------------------------------------------------------------------------

const KNOTS: Opt = number("knots", 0.0, 4.0, 1.0, "How many knots the grain flows round, on average per square of 30 spacings (0: straight grain).");

pub static WOOD: Generator = Generator {
    name: "wood",
    about: "Wood grain: long lines a spacing apart running along the angle, wandering slowly and flowing round knots.",
    spacing: 1.0,
    angle: 0.0,
    options: &[&KNOTS],
    own_mask: false,
    inset: 0.0,
    draw: draw_wood,
};

fn draw_wood(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    crate::textures::wood(c, t, a.num("knots"))
}

pub static MARBLE: Generator = Generator {
    name: "marble",
    about: "Marble: veins about a spacing apart drifting across the slab at the angle, warped by turbulence, each drawn as its two edges so it pinches and swells.",
    spacing: 2.0,
    angle: 30.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_marble,
};

fn draw_marble(c: &Canvas, t: &Tools, _a: &Args) -> Strokes {
    crate::textures::draw_marble(c, t)
}

pub static SWELLS: Generator = Generator {
    name: "swells",
    about: "Overlapping swells or dunes: crest lines layered back to front, each hiding the ones behind and filled with lines that follow its slope.",
    spacing: 0.5,
    angle: 0.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_swells,
};

fn draw_swells(c: &Canvas, _t: &Tools, _a: &Args) -> Strokes {
    crate::textures::fill_swells_frayed(&c.limit, c.spacing, c.angle, c.seed, c.anchor, 0.0)
}

pub static EDDIES: Generator = Generator {
    name: "eddies",
    about: "Currents: evenly spaced streamlines of a flow that undulates and winds into spiral eddies.",
    spacing: 0.8,
    angle: 0.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_eddies,
};

fn draw_eddies(c: &Canvas, _t: &Tools, _a: &Args) -> Strokes {
    crate::textures::fill_eddies_frayed(&c.limit, c.spacing, c.angle, c.seed, c.anchor, 0.0)
}

pub static RIPPLES: Generator = Generator {
    name: "ripples",
    about: "Ripples: rows of broken wavy strokes a spacing apart, their crests bending together over a slow swell.",
    spacing: 1.0,
    angle: 0.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_ripples,
};

fn draw_ripples(c: &Canvas, _t: &Tools, _a: &Args) -> Strokes {
    crate::textures::fill_ripples_frayed(&c.limit, c.spacing, c.angle, c.seed, c.anchor, 0.0)
}

// ---------------------------------------------------------------------------
// Vegetation symbols
// ---------------------------------------------------------------------------

pub static CANOPY: Generator = Generator {
    name: "canopy",
    about: "Tree canopies from above: clusters of wobbly lobes, a dot at each lobe's heart, trees a spacing apart.",
    spacing: 3.0,
    angle: 0.0,
    options: &[],
    own_mask: true,
    inset: 0.0,
    draw: draw_canopy,
};

fn draw_canopy(c: &Canvas, t: &Tools, _a: &Args) -> Strokes {
    let s = c.spacing;
    let at = anchored_sites(c, t, &c.region.outset(s), s);
    // A tree's lobes reach about its own size; neighbours just touch.
    crate::nature::canopy_at(c, t, s / 1.05, at)
}

pub static TWIGS: Generator = Generator {
    name: "twigs",
    about: "Scattered twigs a spacing apart: a kinked stick with short side shoots at acute angles; brushwood, scrub, forest litter.",
    spacing: 2.5,
    angle: 0.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_twigs,
};

fn draw_twigs(c: &Canvas, t: &Tools, _a: &Args) -> Strokes {
    let s = c.spacing;
    let at = anchored_sites(c, t, &c.region.outset(s * 0.6), s);
    crate::nature::twigs_at(c, t, s / 1.18, at)
}

const COVERAGE_TREES: Opt = number("coverage", 0.0, 1.0, 0.85, "The share of places that get a tree, 0 to 1: lower leaves clearings.");
const LOOPS: Opt = number("loops", 0.2, 3.0, 1.0, "How many curls run round each crown's rim, as a multiple of the usual count.");
const SHADOW: Opt = number("shadow", 0.0, 1.0, 0.8, "The share of crowns with a crescent of shade on the side away from the light.");
const LIGHT_FROM: Opt = number(
    "light_angle",
    -360.0,
    360.0,
    225.0,
    "Where the light comes from, in page degrees (0: from the right, 90: from below; 225, the map maker's top left).",
);

pub static DOODLE_TREES: Generator = Generator {
    name: "doodle-trees",
    about: "Trees as a hand doodles them on a sketch map: each crown (a spacing across) one curly stroke round a lumpy circle, with a crescent of shade; nearer crowns hide those behind.",
    spacing: 4.0,
    angle: 0.0,
    options: &[&COVERAGE_TREES, &LOOPS, &SHADOW, &LIGHT_FROM],
    own_mask: true,
    inset: 0.0,
    draw: draw_doodle_trees,
};

fn draw_doodle_trees(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    let d = c.spacing;
    let look = crate::doodle::Doodle {
        diameter: d,
        coverage: a.num("coverage"),
        loops: a.num("loops"),
        shadow: a.num("shadow"),
        light_deg: a.num("light_angle"),
    };
    // Places a little closer than a crown, so neighbours overlap.
    let at = anchored_sites(c, t, &t.placement_field(d * 0.5), d * 0.72);
    crate::doodle::doodle_at(t, &look, at)
}

// ---------------------------------------------------------------------------
// Covering generators: they draw over part of the area and register what
// they cover as the mask. Each algorithm takes the edge mode itself; the
// runner applies inset, clipping and fray once, so they are asked for none.
// ---------------------------------------------------------------------------

const ROOTS: Opt = count("roots", 0, 10_000, 0.0, "How many roots the branches grow from (0: one per `root_spacing`).");
const ROOT_SPACING: Opt = length("root_spacing", Default::Rel(12.0), "Roots about this far apart, each draining its own basin, in mm.");
const STEP: Opt = length("step", Default::Rel(0.16), "How far a branch advances per step, in mm.");
const REACH: Opt = length("reach", Default::Rel(3.2), "A point farther than this from every branch tip pulls nothing, in mm.");
const KILL: Opt = length("kill", Default::Rel(0.4), "A point this close to a tip is reached and stops pulling, in mm.");
const GROWTH_JITTER: Opt = number("jitter", 0.0, 2.0, 0.1, "How much each step wobbles off its course, 0 (straight) up.");
const PEN: Opt = length("pen", Default::Num(0.15), "The pen's width in mm: a trunk's extra passes sit this far apart, side by side (0 draws every branch as one line).");
const TRUNK: Opt = count("trunk", 0, 8, 3.0, "Most extra passes either side of a trunk: thick near the roots, a hairline at the twigs.");
const COVERAGE_GROWTH: Opt = number("coverage", 0.0, 1.0, 1.0, "How far each tree reaches toward its neighbours, 0 to 1: below 1 leaves gaps between them.");
const VARIATION: Opt = number("variation", 0.0, 1.0, 0.35, "How much trees differ in size, 0 (alike) to 1 (saplings beside big crowns).");
const MAX_STEPS: Opt = count("max_steps", 1, 1_000_000, 20_000.0, "A cap on growth steps, so a pathological shape cannot hang.");

pub static GROWTH: Generator = Generator {
    name: "growth",
    about: "Branches grown toward the space they have to fill (space colonisation): river deltas, veins, roots, cracks, lichen; twigs a spacing apart.",
    spacing: 1.0,
    angle: 0.0,
    options: &[&ROOTS, &ROOT_SPACING, &STEP, &REACH, &KILL, &GROWTH_JITTER, &PEN, &TRUNK, &COVERAGE_GROWTH, &VARIATION, &MAX_STEPS],
    own_mask: true,
    inset: 0.0,
    draw: draw_growth,
};

fn draw_growth(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    use crate::growth::{fill_growth_with_coverage, GrowthParams, RootSource};
    // Twigs end up about 2.5 attractor gaps apart.
    let gap = c.spacing * 0.4;
    let roots = match a.count("roots") {
        n if n > 0 => RootSource::Generate(n as usize),
        _ => RootSource::Every(a.num("root_spacing").max(gap)),
    };
    let p = GrowthParams {
        seed: c.seed,
        roots,
        spacing: gap,
        step: a.num("step").max(0.02),
        reach: a.num("reach").max(gap),
        kill: a.num("kill").max(0.02),
        jitter: a.num("jitter"),
        max_steps: a.count("max_steps").max(1) as usize,
        pen: a.num("pen").max(0.0),
        max_passes: a.count("trunk").max(0) as usize,
        coverage: a.num("coverage"),
        variation: a.num("variation"),
        edge: c.edge,
        fray: 0.0,
        ..GrowthParams::default()
    };
    let out = fill_growth_with_coverage(c.region, &p);
    t.add_mask(&out.coverage);
    out.strokes
}

const HATCHED: Opt = flag(
    "hatched",
    "Shade the crowns with hatching on the side away from the light, for an engraved look. Changes the defaults: `light` 0.85, `hatch` 0.45 mm, `ticks` 0.65.",
);
const COVERAGE_CROWNS: Opt = number("coverage", 0.0, 1.0, 0.6, "How patchy the crowns are, 0 to 1: low leaves clearings, 1 fills everywhere the shape allows.");
const BUBBLE: Opt = length("bubble", Default::Rel(0.15), "The size of a leaf cluster on a crown's rim, in mm: smaller is lumpier.");
const SLICES: Opt = count("slices", 2, 18, 9.0, "Contour tiers through a crown: more draws more rings.");
const STACK_JITTER: Opt = number("jitter", 0.0, 2.5, 1.1, "How ragged the contours are: 0 smooth boughs, 1.1 crisp leaves, 1.5 and up deep serrations.");
const JITTER_PERIOD: Opt = number("jitter_period", 0.2, 4.0, 0.68, "The length of the contours' serration: lower is finer teeth, higher broad reaches.");
const TICKS: Opt = Opt {
    key: "ticks",
    kind: Kind::Number { min: 0.0, max: 1.5 },
    default: Default::None,
    doc: "Short slope ticks across the contours at bough clefts and in shade, 0 (none) to 1.5. Default 0.42; 0.65 when `hatched` or with a `hatch`.",
};
const ELONGATION: Opt = number("elongation", 0.0, 1.0, 0.18, "How drawn out the crowns are, as if wind-swept, 0 (round) to 1.");
const LOBES: Opt = number("lobes", 0.0, 2.5, 1.0, "How deep the clefts between boughs are, and how much a crown splits into sub-crowns.");
const LIGHT: Opt = Opt {
    key: "light",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::None,
    doc: "How strong the sunlight is, 0 (none: every side alike) to 1: lit sides open up, shaded sides close in. Default 0.82; 0.85 when `hatched`.",
};
const LIGHT_ELEVATION: Opt = number("light_elevation", 5.0, 90.0, 35.0, "How high the sun stands, in degrees: lower casts longer shade.");
const HATCH: Opt = length("hatch", Default::None, "The shading hatch's spacing in mm, 0 for none. Default 0; 0.45 when `hatched`.");

pub static ISOLINE_STACK: Generator = Generator {
    name: "isoline-stack",
    about: "Tree crowns drawn as stacked contour tiers, lit from one side; `spacing` is a crown's diameter, and the crowns are the mask.",
    spacing: 8.0,
    angle: 0.0,
    options: &[
        &HATCHED,
        &COVERAGE_CROWNS,
        &BUBBLE,
        &SLICES,
        &STACK_JITTER,
        &JITTER_PERIOD,
        &TICKS,
        &ELONGATION,
        &LOBES,
        &LIGHT,
        &LIGHT_FROM,
        &LIGHT_ELEVATION,
        &HATCH,
    ],
    own_mask: true,
    inset: 0.0,
    draw: draw_isoline_stack,
};

fn draw_isoline_stack(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    use crate::isoline_stack::{fill_isoline_stack_with_coverage, CrownSource, IsolineStackParams, Light};
    let hatched = a.flag("hatched");
    let hatch = a.opt_num("hatch").unwrap_or(if hatched { 0.45 } else { 0.0 }).max(0.0);
    let strength = a.opt_num("light").unwrap_or(if hatched { 0.85 } else { 0.82 });
    let light = (strength > 0.0).then(|| Light {
        from_deg: a.num("light_angle"),
        strength,
        elevation_deg: a.num("light_elevation"),
        hatch,
    });
    // `spacing` is the crown's diameter; the algorithm wants its radius.
    let crown = (c.spacing / 2.0).max(0.6);
    let p = IsolineStackParams {
        seed: c.seed,
        crowns: CrownSource::Generate,
        crown,
        bubble: a.num("bubble").max(0.3),
        coverage: a.num("coverage"),
        slices: a.count("slices").clamp(2, 18) as usize,
        jitter: a.num("jitter"),
        jitter_period: a.num("jitter_period"),
        elongation: a.num("elongation"),
        lobes: a.num("lobes"),
        ticks: a.opt_num("ticks").unwrap_or(if hatch > 0.01 { 0.65 } else { 0.42 }),
        edge: c.edge,
        fray: 0.0,
        light,
    };
    let out = fill_isoline_stack_with_coverage(c.region, &p);
    t.add_mask(&out.coverage);
    out.strokes
}

const CONTOUR: Opt = length("contour", Default::Rel(0.045), "The distance between nested contour lines inside a blob, in mm.");
const WARP: Opt = number("warp", 0.0, 2.0, 0.45, "How much the field curls and drifts, 0 (round blobs) up.");
const WARP_PERIOD: Opt = number("warp_period", 0.2, 4.0, 1.0, "The size of the warp's waves: lower is finer, higher broader.");
const THRESHOLD: Opt = number("threshold", 0.0, 1.0, 0.15, "Where ink begins on the field: higher keeps only the tallest blobs, lower fuses them into larger masses.");

pub static ISOLINE_BLOBS: Generator = Generator {
    name: "isoline-blobs",
    about: "Blobs a spacing apart, each filled with nested contour rings like a height map; the blobs are the mask.",
    spacing: 10.0,
    angle: 0.0,
    options: &[&CONTOUR, &WARP, &WARP_PERIOD, &THRESHOLD],
    own_mask: true,
    inset: 0.0,
    draw: draw_isoline_blobs,
};

fn draw_isoline_blobs(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    let p = crate::IsolineFieldParams {
        seed: c.seed,
        source: crate::FieldSource::Noise,
        feature_size: c.spacing,
        contour_spacing: a.num("contour").max(0.02),
        warp: a.num("warp"),
        warp_period: a.num("warp_period"),
        threshold: a.num("threshold"),
        edge: c.edge,
        fray: 0.0,
    };
    let out = crate::fill_isoline_blobs_with_coverage(c.region, &p);
    t.add_mask(&out.coverage);
    out.strokes
}

const CELL: Opt = length("cell", Default::Rel(8.0), "The size of the cloud masses and the clear sky between them, in mm.");
const COVERAGE_SKY: Opt = number("coverage", 0.0, 1.0, 0.5, "The share of the sky that is cloud.");
const CLOUD_WARP: Opt = number("warp", 0.0, 1.0, 0.25, "How much the masses curl and drift, 0 (plain blobs) to 1.");
const PUFFS: Opt = number("puffs", 0.3, 3.0, 1.0, "How closely the puffs crowd: about 0.6 few and round, 2 many and lumpy.");
const ELONGATE: Opt = number("elongate", 0.0, 1.0, 0.35, "How drawn out each puff is along the run of its cloud, 0 (round) to 1.");
const STRETCH: Opt = number("stretch", 0.0, 0.95, 0.25, "How far the masses are drawn out along the wind (toward cloud streets).");
const WIND: Opt = number("wind", -360.0, 360.0, 15.0, "The wind's direction on the page, in degrees (0: left to right).");
const CLOUD_LIGHT_FROM: Opt = number("light_angle", -360.0, 360.0, 225.0, "Where the sun is, in page degrees (0: from the right, 90: from below; 225, the top left).");
const CLOUD_ELEVATION: Opt = number("light_elevation", 5.0, 90.0, 30.0, "How high the sun stands, in degrees: lower shades more of each heap.");
const HEIGHT: Opt = number("height", 0.0, 2.0, 1.0, "How tall the clouds heap up against their width: 1 a cumulus, 0.3 a flat sheet.");
const SHADE: Opt = number("shade", 0.0, 1.0, 0.45, "How much of the cloud is shaded, 0 (none) to 1.");
const CLOUD_HATCH: Opt = length("hatch", Default::Num(0.45), "The shade's spacing at its deepest, between dots or lines, in mm.");
const SHADING: Opt = Opt {
    key: "shading",
    kind: Kind::Choice(&["dots", "lines"]),
    default: Default::Text("dots"),
    doc: "Shade in stippled `dots` (denser where deeper) or hatched `lines`.",
};
const DOT: Opt = length("dot", Default::Num(0.12), "Each shading dot's length, in mm (a pen's dot is a short stroke).");

pub static CLOUDS: Generator = Generator {
    name: "clouds",
    about: "Cumulus seen from above: puffs a spacing across heaped on bodies that rise and fall, shaded away from the sun; the clouds are the mask.",
    spacing: 5.0,
    angle: 0.0,
    options: &[
        &CELL,
        &COVERAGE_SKY,
        &CLOUD_WARP,
        &PUFFS,
        &ELONGATE,
        &STRETCH,
        &WIND,
        &CLOUD_LIGHT_FROM,
        &CLOUD_ELEVATION,
        &HEIGHT,
        &SHADE,
        &CLOUD_HATCH,
        &SHADING,
        &DOT,
    ],
    own_mask: true,
    inset: 0.0,
    draw: draw_clouds,
};

fn draw_clouds(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    use crate::clouds::{fill_clouds_with_coverage, CloudParams};
    let p = CloudParams {
        seed: c.seed,
        size: c.spacing,
        cell: a.num("cell").max(c.spacing),
        coverage: a.num("coverage"),
        warp: a.num("warp"),
        puffs: a.num("puffs"),
        elongate: a.num("elongate"),
        stretch: a.num("stretch"),
        wind_deg: a.num("wind"),
        light_deg: a.num("light_angle"),
        light_elevation: a.num("light_elevation"),
        height: a.num("height"),
        shade: a.num("shade"),
        hatch: a.num("hatch").max(0.05),
        dots: a.word("shading") != "lines",
        dot: a.num("dot").max(0.0),
    };
    let out = fill_clouds_with_coverage(c.region, &p);
    t.add_mask(&out.coverage);
    out.strokes
}

// ---------------------------------------------------------------------------
// solid
// ---------------------------------------------------------------------------

pub static SOLID: Generator = Generator {
    name: "solid",
    about: "A flat fill with no pen strokes: the host paints the area (an SVG fill, a screen preview); plotters draw nothing. Its mask is the whole area.",
    spacing: 1.0,
    angle: 0.0,
    options: &[],
    own_mask: false,
    inset: 0.0,
    draw: draw_solid,
};

fn draw_solid(c: &Canvas, t: &Tools, _a: &Args) -> Strokes {
    t.add_mask(&c.limit);
    crate::solid::fill_solid(&c.limit)
}

#[cfg(test)]
mod tests {
    use crate::catalog::{fill, fill_output, Spec};
    use crate::geo::{polyline_length, Region, Strokes};
    use crate::Anchor;

    const ALL: [&str; 20] = [
        "rings",
        "scribble",
        "hilbert",
        "truchet",
        "honeycomb",
        "scales",
        "sunburst",
        "wood",
        "marble",
        "swells",
        "eddies",
        "ripples",
        "canopy",
        "twigs",
        "doodle-trees",
        "growth",
        "isoline-stack",
        "isoline-blobs",
        "clouds",
        "solid",
    ];

    fn rect(x0: f64, y0: f64, w: f64, h: f64) -> Region {
        Region::from_ring(&[(x0, y0), (x0 + w, y0), (x0 + w, y0 + h), (x0, y0 + h)])
    }

    fn ink(s: &Strokes) -> f64 {
        s.iter().map(|p| polyline_length(p)).sum()
    }

    fn strokes(name: &str, region: &Region, spec: &Spec) -> Strokes {
        fill(name, region, spec).unwrap_or_else(|e| panic!("{name}: {e}")).strokes
    }

    #[test]
    fn every_specialist_draws_at_its_defaults_the_same_twice() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        for name in ALL {
            let spec = Spec::default().seed(5);
            let a = fill_output(name, &sq, &spec).unwrap();
            if name == "solid" {
                // No ink: the host paints the area, and the mask says where.
                assert!(a.strokes.is_empty());
                assert!(a.mask.covers_point((10.0, 10.0)));
                continue;
            }
            assert!(ink(&a.strokes) > 20.0, "{name} drew {} mm", ink(&a.strokes));
            assert_eq!(a.strokes, strokes(name, &sq, &spec), "{name} is not deterministic");
        }
    }

    #[test]
    fn wood_draws_far_from_the_page_origin() {
        for anchor in [Anchor::LOCAL, Anchor::GLOBAL] {
            for (x, y) in [(500.0, 300.0), (-800.0, 1200.0)] {
                let far = ink(&strokes("wood", &rect(x, y, 20.0, 20.0), &Spec::default().anchor(anchor)));
                // About 21 grain lines across 20 mm.
                assert!(far > 300.0, "wood at ({x}, {y}), {anchor:?}: {far} mm");
            }
        }
    }

    #[test]
    fn anchored_specialists_leave_no_seam() {
        let at = Spec::default().anchor(Anchor::GLOBAL).seed(3);
        for name in ["scribble", "truchet", "honeycomb", "scales", "sunburst", "wood", "marble"] {
            // A sunburst is shared when the areas are given one centre.
            let at = if name == "sunburst" { at.clone().option("centre", super::super::Value::Nums(vec![200.0, 80.0])) } else { at.clone() };
            let whole = ink(&strokes(name, &rect(210.0, 90.0, 40.0, 20.0), &at));
            let halves = ink(&strokes(name, &rect(210.0, 90.0, 20.0, 20.0), &at)) + ink(&strokes(name, &rect(230.0, 90.0, 20.0, 20.0), &at));
            assert!((whole - halves).abs() < 0.02 * whole, "{name}: whole {whole} vs halves {halves}");
        }
    }

    #[test]
    fn covering_generators_mask_what_they_cover() {
        let sq = rect(0.0, 0.0, 40.0, 40.0);
        for name in ["growth", "isoline-stack", "isoline-blobs", "clouds", "canopy", "doodle-trees"] {
            let out = fill_output(name, &sq, &Spec::default().seed(1)).unwrap();
            assert!(!out.mask.is_empty(), "{name}: no mask");
            // The mask is what the marks cover: some of the square, not all of it.
            let probes = (0..20).flat_map(|i| (0..20).map(move |j| (1.0 + 2.0 * i as f64, 1.0 + 2.0 * j as f64)));
            let covered = probes.filter(|p| out.mask.covers_point(*p)).count();
            assert!(covered > 0, "{name}: mask covers nothing");
            if name != "growth" {
                assert!(covered < 400, "{name}: mask is the whole square");
            }
        }
    }

    #[test]
    fn rings_follow_the_outline_or_every_edge() {
        let holed = Region::from_polygons(&[vec![
            vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)],
            vec![(12.0, 12.0), (18.0, 12.0), (18.0, 18.0), (12.0, 18.0)],
        ]]);
        let outline = ink(&strokes("rings", &holed, &Spec::default()));
        let edges = ink(&strokes("rings", &holed, &Spec::default().option("from", "edges")));
        let spiral = strokes("rings", &holed, &Spec::default().option("spiral", true));
        assert!(outline > 300.0 && edges > 300.0 && !spiral.is_empty());
        assert_ne!(outline, edges);
        assert!(fill("rings", &holed, &Spec::default().option("from", "middle")).is_err());
    }

    #[test]
    fn scribble_layers_and_tone() {
        let sq = rect(0.0, 0.0, 30.0, 30.0);
        let layers = |n: f64| ink(&strokes("scribble", &sq, &Spec::default().option("layers", n)));
        let (one, two, three) = (layers(1.0), layers(2.0), layers(3.0));
        assert!(two > one * 1.7 && three > two * 1.3, "{one} {two} {three}");
        let toned = ink(&strokes("scribble", &sq, &Spec::default().option("layers", 3.0).option("density", "gradient").option("contrast", 1.0)));
        assert!(toned < three * 0.75, "tone kept {toned} of {three}");
    }

    #[test]
    fn sunburst_rays_come_from_the_centre() {
        let sq = rect(0.0, 0.0, 30.0, 30.0);
        for (spec, c) in [(Spec::default(), (15.0, 15.0)), (Spec::default().option("centre", super::super::Value::Nums(vec![2.0, 28.0])), (2.0, 28.0))] {
            let rays = strokes("sunburst", &sq, &spec);
            assert!(rays.len() > 40);
            for r in &rays {
                let (a, b) = (r[0], r[r.len() - 1]);
                let cross = (a.0 - c.0) * (b.1 - c.1) - (a.1 - c.1) * (b.0 - c.0);
                assert!(cross.abs() < 1e-6 * (1.0 + polyline_length(r) * 30.0), "ray {a:?}–{b:?} misses {c:?}");
            }
        }
    }

    #[test]
    fn honeycomb_doubles_its_walls() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let single = ink(&strokes("honeycomb", &sq, &Spec::default()));
        let double = ink(&strokes("honeycomb", &sq, &Spec::default().option("walls", 0.3)));
        assert!(double > single * 1.6, "{single} → {double}");
    }

    #[test]
    fn isoline_stack_hatched_shades_and_spacing_is_the_crown() {
        let sq = rect(0.0, 0.0, 40.0, 40.0);
        let plain = ink(&strokes("isoline-stack", &sq, &Spec::default().seed(2)));
        let hatched = ink(&strokes("isoline-stack", &sq, &Spec::default().seed(2).option("hatched", true)));
        assert!(hatched > plain, "{plain} → {hatched}");
        // Bigger crowns, fewer of them: the outline ink drops.
        let big = ink(&strokes("isoline-stack", &sq, &Spec::default().seed(2).spacing(16.0)));
        assert!(big < plain, "{plain} vs {big}");
    }
}
