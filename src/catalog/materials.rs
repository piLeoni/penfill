//! Materials: named recipes over generators, grouped by what they are for.
//!
//! Every length here is a multiple of the material's `spacing` ([`M::Rel`]),
//! so a user who sets `spacing` scales the whole recipe and it keeps its
//! proportions. A layer's `angle` is added to the user's, so turning a
//! material turns every layer together.

use super::value::Value;

/// What a material is for: the order a map maker reaches for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Water,
    Vegetation,
    Ground,
    Rock,
    Built,
    Sky,
    Plain,
}

impl Group {
    pub const ALL: [Group; 7] = [Group::Water, Group::Vegetation, Group::Ground, Group::Rock, Group::Built, Group::Sky, Group::Plain];

    pub fn id(self) -> &'static str {
        match self {
            Group::Water => "water",
            Group::Vegetation => "vegetation",
            Group::Ground => "ground",
            Group::Rock => "rock",
            Group::Built => "built",
            Group::Sky => "sky",
            Group::Plain => "plain",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Group::Water => "Water",
            Group::Vegetation => "Vegetation",
            Group::Ground => "Ground and soil",
            Group::Rock => "Rock",
            Group::Built => "Built",
            Group::Sky => "Sky",
            Group::Plain => "Plain hatches",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Group::Water => "Seas, lakes, rivers and wet ground.",
            Group::Vegetation => "Grass, woods, crops and gardens, in the symbols maps use for them.",
            Group::Ground => "Sand, soil, gravel and bare earth.",
            Group::Rock => "Rock in the conventions of geological maps: the symbols a geologist reads at a glance.",
            Group::Built => "Paving, brickwork, tiles and the textures of built surfaces.",
            Group::Sky => "Clouds seen from above, and their shadows.",
            Group::Plain => "Lines, grids, dots and rings: abstract hatches for anything.",
        }
    }
}

/// A value in a material's recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum M {
    /// A plain number (an angle, a share, a count).
    Num(f64),
    /// A length, as a multiple of the material's spacing.
    Rel(f64),
    /// Two lengths, as multiples of the spacing.
    Rels(f64, f64),
    Flag(bool),
    Text(&'static str),
}

impl M {
    pub(super) fn resolve(&self, spacing: f64) -> Value {
        match *self {
            M::Num(n) => Value::Num(n),
            M::Rel(k) => Value::Num(k * spacing),
            M::Rels(a, b) => Value::Nums(vec![a * spacing, b * spacing]),
            M::Flag(b) => Value::Flag(b),
            M::Text(t) => Value::Text(t.to_string()),
        }
    }

    pub(super) fn json(&self) -> String {
        match *self {
            M::Num(n) => format!("{n}"),
            M::Rel(k) => format!("{{ \"timesSpacing\": {k} }}"),
            M::Rels(a, b) => format!("{{ \"timesSpacing\": [{a}, {b}] }}"),
            M::Flag(b) => b.to_string(),
            M::Text(t) => format!("{t:?}"),
        }
    }
}

/// One layer of a material: a generator and the settings it draws with.
/// `spacing` (`M::Rel`) sets the layer's own spacing, `angle` (`M::Num`) its
/// turn, `edge` (`"inside"`, `"clip"`, or `M::Rel` for overflow) its edge mode.
pub struct Layer {
    pub generator: &'static str,
    pub set: &'static [(&'static str, M)],
}

/// A named recipe. A material with no layers is an alias: the generator of
/// the same name at its defaults, listed in a group without changing what
/// the name draws.
pub struct Material {
    pub name: &'static str,
    pub group: Group,
    /// One sentence: what it looks like and what it is for.
    pub about: &'static str,
    /// Default spacing in mm, chosen to read at a 0.2–0.3 mm pen.
    pub spacing: f64,
    pub layers: &'static [Layer],
}

impl Material {
    /// Whether this material is a generator listed under its own name.
    pub fn is_alias(&self) -> bool {
        self.layers.is_empty()
    }
}

const fn one(generator: &'static str, set: &'static [(&'static str, M)]) -> Layer {
    Layer { generator, set }
}

use M::{Flag, Num, Rel, Rels, Text};

/// The catalogue's materials, group by group.
pub static MATERIALS: &[Material] = &[
    // ── Water ──────────────────────────────────────────────────────────────
    alias("waves", Group::Water, "Even sine waves: the classic water fill."),
    Material { name: "scallops", group: Group::Water, about: "Rows of scallops, arcs dipping between sharp points: a decorative sea.", spacing: 1.0, layers: &[one("waves", &[("shape", Text("scallop"))])] },
    Material { name: "curls", group: Group::Water, about: "Waves with curling crests, like an old chart's sea.", spacing: 1.2, layers: &[one("waves", &[("shape", Text("curl"))])] },
    Material { name: "currents", group: Group::Water, about: "Long swell lines that bunch and spread without crossing, broken into strokes: open sea.", spacing: 0.8, layers: &[one("waves", &[("shape", Text("flow"))])] },
    Material { name: "wave-net", group: Group::Water, about: "Two families of waves crossing into a net.", spacing: 1.2, layers: &[one("waves", &[("cross", Num(2.0))])] },
    alias("swells", Group::Water, "Overlapping swells, each a bundle of curves: a lively sea."),
    alias("eddies", Group::Water, "Flowing lines round small whirls."),
    alias("ripples", Group::Water, "Short broken ripple lines."),
    Material {
        name: "shore-lines",
        group: Group::Water,
        about: "Engraved water lines, close along the shore and thinning out to open water.",
        spacing: 0.6,
        layers: &[one("hatch", &[("density", Text("edge")), ("contrast", Num(1.0)), ("feature", Rel(12.0)), ("inset", Num(0.1))])],
    },
    Material {
        name: "shallows",
        group: Group::Water,
        about: "Stipple dense along the shore, fading out to open water.",
        spacing: 0.55,
        layers: &[one("scatter", &[("mark", Rel(0.3)), ("density", Text("edge")), ("contrast", Num(0.95)), ("feature", Rel(16.0))])],
    },
    Material {
        name: "marsh",
        group: Group::Water,
        about: "The marsh symbol: tufts of reeds over short water lines.",
        spacing: 3.2,
        layers: &[one("scatter", &[("motif", Text("reed")), ("rotate", Text("upright")), ("mark", Rel(0.5)), ("vary", Num(0.3)), ("jitter", Num(0.6))])],
    },
    // ── Vegetation ─────────────────────────────────────────────────────────
    Material {
        name: "grass",
        group: Group::Vegetation,
        about: "Scattered grass tufts.",
        spacing: 3.2,
        layers: &[one("scatter", &[("motif", Text("tuft")), ("rotate", Text("upright")), ("mark", Rel(0.6)), ("vary", Num(0.4)), ("jitter", Num(0.7))])],
    },
    Material {
        name: "meadow",
        group: Group::Vegetation,
        about: "Grass tufts in patches with seed dots between: rough grassland.",
        spacing: 3.0,
        layers: &[
            one("scatter", &[("motif", Text("tuft")), ("rotate", Text("upright")), ("mark", Rel(0.65)), ("vary", Num(0.4)), ("jitter", Num(0.6)), ("density", Text("noise")), ("contrast", Num(0.85)), ("feature", Rel(5.0))]),
            one("scatter", &[("spacing", Rel(0.45)), ("mark", Rel(0.06)), ("density", Text("noise")), ("contrast", Num(0.7)), ("feature", Rel(4.0))]),
        ],
    },
    Material {
        name: "scrub",
        group: Group::Vegetation,
        about: "Low bushes scattered close: heath, scrub, brush.",
        spacing: 1.6,
        layers: &[one("scatter", &[("motif", Text("shrub")), ("rotate", Text("upright")), ("mark", Rel(0.62)), ("vary", Num(0.4)), ("jitter", Num(0.6))])],
    },
    Material {
        name: "woodland",
        group: Group::Vegetation,
        about: "The topographic woodland symbol: broadleaf trees standing apart.",
        spacing: 2.6,
        layers: &[one("scatter", &[("motif", Text("tree")), ("rotate", Text("upright")), ("mark", Rel(0.75)), ("vary", Num(0.35)), ("jitter", Num(0.5))])],
    },
    Material {
        name: "forest",
        group: Group::Vegetation,
        about: "Broadleaf trees close enough to touch: dense forest.",
        spacing: 2.0,
        layers: &[one("scatter", &[("motif", Text("tree")), ("rotate", Text("upright")), ("mark", Rel(0.95)), ("vary", Num(0.4)), ("jitter", Num(0.5)), ("overlap", Flag(true))])],
    },
    Material {
        name: "conifers",
        group: Group::Vegetation,
        about: "The conifer symbol: narrow firs standing apart.",
        spacing: 2.4,
        layers: &[one("scatter", &[("motif", Text("conifer")), ("rotate", Text("upright")), ("mark", Rel(0.8)), ("vary", Num(0.35)), ("jitter", Num(0.5))])],
    },
    Material {
        name: "orchard",
        group: Group::Vegetation,
        about: "Trees planted in rows.",
        spacing: 3.2,
        layers: &[one("scatter", &[("motif", Text("tree")), ("arrange", Text("rows")), ("rotate", Text("upright")), ("mark", Rel(0.55)), ("jitter", Num(0.15))])],
    },
    Material {
        name: "vineyard",
        group: Group::Vegetation,
        about: "Vines trained in close rows.",
        spacing: 1.6,
        layers: &[one("scatter", &[("motif", Text("vine")), ("arrange", Text("rows")), ("rotate", Text("upright")), ("mark", Rel(0.62))])],
    },
    Material {
        name: "fields",
        group: Group::Vegetation,
        about: "Furrows: long broken rows, as ploughed land or crops are drawn.",
        spacing: 1.0,
        layers: &[one("hatch", &[("dash", Rels(5.0, 0.6)), ("dash_jitter", Num(0.5)), ("wobble", Rel(0.12))])],
    },
    Material {
        name: "garden",
        group: Group::Vegetation,
        about: "Flowers scattered over sparse grass.",
        spacing: 3.0,
        layers: &[
            one("scatter", &[("motif", Text("flower")), ("rotate", Text("random")), ("mark", Rel(0.42)), ("vary", Num(0.4)), ("jitter", Num(0.6))]),
            one("scatter", &[("spacing", Rel(0.8)), ("motif", Text("tuft")), ("rotate", Text("upright")), ("mark", Rel(0.3)), ("vary", Num(0.4))]),
        ],
    },
    Material {
        name: "sprouts",
        group: Group::Vegetation,
        about: "Seedlings, two leaves each: young crops, nurseries.",
        spacing: 2.0,
        layers: &[one("scatter", &[("motif", Text("sprout")), ("rotate", Text("upright")), ("mark", Rel(0.45)), ("vary", Num(0.3)), ("jitter", Num(0.5))])],
    },
    Material {
        name: "leaves",
        group: Group::Vegetation,
        about: "Fallen leaves turned every way: leaf litter, a woodland floor.",
        spacing: 1.8,
        layers: &[one("scatter", &[("motif", Text("leaf")), ("mark", Rel(0.55)), ("vary", Num(0.4)), ("jitter", Num(0.5))])],
    },
    alias("isoline-stack", Group::Vegetation, "Tree crowns from above, as stacked contour slices with light and shade."),
    alias("canopy", Group::Vegetation, "Lobed crowns packed together: a canopy seen from above."),
    alias("doodle-trees", Group::Vegetation, "Loose doodled crowns with a shade crescent."),
    alias("twigs", Group::Vegetation, "Small branching twigs."),
    alias("growth", Group::Vegetation, "Branches grown toward the light: roots, veins, river deltas."),
    // ── Ground ─────────────────────────────────────────────────────────────
    Material { name: "sand", group: Group::Ground, about: "Fine even stipple.", spacing: 0.5, layers: &[one("scatter", &[("mark", Rel(0.28))])] },
    Material {
        name: "dunes",
        group: Group::Ground,
        about: "Stipple in drifts, dense and sparse: dunes, beaches, deserts.",
        spacing: 0.5,
        layers: &[one("scatter", &[("mark", Rel(0.3)), ("density", Text("noise")), ("contrast", Num(0.92)), ("feature", Rel(24.0))])],
    },
    Material {
        name: "soil",
        group: Group::Ground,
        about: "Grains and short ticks: bare soil, earth.",
        spacing: 1.2,
        layers: &[one("scatter", &[("spacing", Rel(0.55)), ("mark", Rel(0.12))]), one("scatter", &[("spacing", Rel(1.3)), ("motif", Text("tick")), ("mark", Rel(0.4)), ("vary", Num(0.5))])],
    },
    Material { name: "gravel", group: Group::Ground, about: "Small rounded stones packed together.", spacing: 1.6, layers: &[one("cells", &[])] },
    Material {
        name: "pebbles",
        group: Group::Ground,
        about: "Pebbles lying apart, each its own outline.",
        spacing: 2.0,
        layers: &[one("scatter", &[("motif", Text("pebble")), ("mark", Rel(0.6)), ("vary", Num(0.5)), ("jitter", Num(0.5)), ("edge", Text("inside"))])],
    },
    Material {
        name: "boulders",
        group: Group::Ground,
        about: "Rocks of every size, big boulders among small stones.",
        spacing: 2.4,
        layers: &[one("cells", &[("vary", Num(1.0))])],
    },
    Material {
        name: "scree",
        group: Group::Ground,
        about: "Angular fragments and grit: scree, talus, broken rock.",
        spacing: 1.6,
        layers: &[
            one("scatter", &[("motif", Text("shard")), ("mark", Rel(0.6)), ("vary", Num(0.6)), ("jitter", Num(0.5))]),
            one("scatter", &[("spacing", Rel(0.45)), ("mark", Rel(0.1)), ("density", Text("noise")), ("contrast", Num(0.6))]),
        ],
    },
    Material { name: "cracked-earth", group: Group::Ground, about: "A net of cracks seen from above: dried mud, salt flats.", spacing: 3.0, layers: &[one("cells", &[("style", Text("net")), ("gap", Rel(0.12))])] },
    Material { name: "dry-mud", group: Group::Ground, about: "Mud dried into rounded plates with wide cracks between.", spacing: 3.5, layers: &[one("cells", &[("style", Text("cracks"))])] },
    Material {
        name: "clay",
        group: Group::Ground,
        about: "Long dashes in uneven rows: clay, alluvium.",
        spacing: 0.8,
        layers: &[one("hatch", &[("dash", Rels(2.6, 0.5)), ("dash_jitter", Num(0.5)), ("vary", Num(0.3))])],
    },
    Material {
        name: "silt",
        group: Group::Ground,
        about: "Short fine dashes: silt, loess.",
        spacing: 0.7,
        layers: &[one("hatch", &[("dash", Rels(0.7, 0.55)), ("dash_jitter", Num(0.7))])],
    },
    Material {
        name: "peat",
        group: Group::Ground,
        about: "Broken wavering dashes: peat, bog, organic soil.",
        spacing: 1.0,
        layers: &[one("hatch", &[("dash", Rels(1.4, 0.8)), ("dash_jitter", Num(0.8)), ("wobble", Rel(0.3)), ("vary", Num(0.5))])],
    },
    Material {
        name: "layered-soil",
        group: Group::Ground,
        about: "Uneven bands that bend together: a soil profile, sediment in section.",
        spacing: 0.9,
        layers: &[one("hatch", &[("vary", Num(1.0)), ("wobble", Rel(0.35))])],
    },
    Material {
        name: "chalky-soil",
        group: Group::Ground,
        about: "A net of cells with grains inside: chalky or stony soil.",
        spacing: 3.5,
        layers: &[one("cells", &[("style", Text("net")), ("inner", Text("dots"))])],
    },
    // ── Rock ───────────────────────────────────────────────────────────────
    Material { name: "sandstone", group: Group::Rock, about: "Even dots: sandstone, in the geologist's convention.", spacing: 1.0, layers: &[one("scatter", &[("arrange", Text("hex")), ("jitter", Num(0.5)), ("mark", Rel(0.35))])] },
    Material {
        name: "siltstone",
        group: Group::Rock,
        about: "Rows of dashes with dots between: siltstone.",
        spacing: 1.4,
        layers: &[
            one("hatch", &[("dash", Rels(1.0, 1.0)), ("dash_jitter", Num(0.3))]),
            one("scatter", &[("spacing", Rel(0.7)), ("mark", Rel(0.12)), ("arrange", Text("rows")), ("jitter", Num(0.6))]),
        ],
    },
    Material { name: "shale", group: Group::Rock, about: "Rows of dashes: shale, mudstone.", spacing: 0.9, layers: &[one("hatch", &[("dash", Rels(2.0, 0.7)), ("dash_jitter", Num(0.45))])] },
    Material { name: "limestone", group: Group::Rock, about: "Blocks in brick bond: limestone.", spacing: 1.4, layers: &[one("tile", &[("motif", Text("brick"))])] },
    Material {
        name: "chalk",
        group: Group::Rock,
        about: "Wide blocks with fine grains: chalk.",
        spacing: 2.0,
        layers: &[one("tile", &[("motif", Text("brick"))]), one("scatter", &[("spacing", Rel(0.4)), ("mark", Rel(0.06))])],
    },
    Material {
        name: "conglomerate",
        group: Group::Rock,
        about: "Rounded pebbles of every size: conglomerate.",
        spacing: 1.8,
        layers: &[one("scatter", &[("motif", Text("ring")), ("mark", Rel(0.55)), ("vary", Num(0.7)), ("jitter", Num(0.6)), ("edge", Text("inside"))])],
    },
    Material {
        name: "breccia",
        group: Group::Rock,
        about: "Angular fragments: breccia.",
        spacing: 1.8,
        layers: &[one("scatter", &[("motif", Text("shard")), ("mark", Rel(0.65)), ("vary", Num(0.6)), ("jitter", Num(0.6))])],
    },
    Material { name: "coal", group: Group::Rock, about: "Inked solid by a close crosshatch: coal seams.", spacing: 0.35, layers: &[one("hatch", &[("cross", Num(2.0))])] },
    Material {
        name: "granite",
        group: Group::Rock,
        about: "Crosses and ticks scattered every way: granite and other intrusive rock.",
        spacing: 1.6,
        layers: &[
            one("scatter", &[("motif", Text("plus")), ("mark", Rel(0.42)), ("jitter", Num(0.5))]),
            one("scatter", &[("spacing", Rel(1.3)), ("motif", Text("tick")), ("mark", Rel(0.32))]),
        ],
    },
    Material {
        name: "basalt",
        group: Group::Rock,
        about: "Vees in rows: basalt and other volcanic rock.",
        spacing: 1.6,
        layers: &[one("scatter", &[("motif", Text("vee")), ("arrange", Text("hex")), ("rotate", Text("aligned")), ("mark", Rel(0.45))])],
    },
    Material {
        name: "tuff",
        group: Group::Rock,
        about: "Vees every way with grains between: tuff, volcanic ash.",
        spacing: 1.6,
        layers: &[
            one("scatter", &[("motif", Text("vee")), ("mark", Rel(0.4)), ("jitter", Num(0.5))]),
            one("scatter", &[("spacing", Rel(0.5)), ("mark", Rel(0.08))]),
        ],
    },
    Material {
        name: "gneiss",
        group: Group::Rock,
        about: "Long wavering broken bands: gneiss.",
        spacing: 1.1,
        layers: &[one("hatch", &[("wobble", Rel(0.45)), ("dash", Rels(3.0, 0.6)), ("dash_jitter", Num(0.6))])],
    },
    Material { name: "schist", group: Group::Rock, about: "Close zigzag lines: schist.", spacing: 0.9, layers: &[one("waves", &[("shape", Text("zigzag"))])] },
    Material { name: "salt", group: Group::Rock, about: "A square crosshatch: rock salt, evaporites.", spacing: 1.6, layers: &[one("hatch", &[("cross", Num(2.0))])] },
    alias("marble", Group::Rock, "Veined marble."),
    // ── Built ──────────────────────────────────────────────────────────────
    Material { name: "brick", group: Group::Built, about: "Brickwork in running bond.", spacing: 1.0, layers: &[one("tile", &[("motif", Text("brick"))])] },
    Material { name: "herringbone", group: Group::Built, about: "Pavers in herringbone.", spacing: 1.2, layers: &[one("tile", &[("motif", Text("herringbone"))])] },
    Material { name: "basket", group: Group::Built, about: "Pavers in basket weave.", spacing: 1.2, layers: &[one("tile", &[("motif", Text("basket"))])] },
    Material { name: "parquet", group: Group::Built, about: "Parquet blocks of boards, alternating.", spacing: 1.2, layers: &[one("tile", &[("motif", Text("parquet"))])] },
    Material { name: "setts", group: Group::Built, about: "Small stone setts in staggered courses: cobbled streets.", spacing: 1.2, layers: &[one("tile", &[("motif", Text("setts"))])] },
    Material { name: "flagstones", group: Group::Built, about: "Irregular rectangular paving.", spacing: 2.0, layers: &[one("tile", &[("motif", Text("flagstones"))])] },
    Material { name: "tiles", group: Group::Built, about: "Square tiles with a joint.", spacing: 1.6, layers: &[one("tile", &[("motif", Text("tiles"))])] },
    Material { name: "hexagons", group: Group::Built, about: "Hexagonal paving.", spacing: 1.6, layers: &[one("tile", &[("motif", Text("hexagons"))])] },
    Material { name: "stairs", group: Group::Built, about: "Steps: treads and risers.", spacing: 1.0, layers: &[one("tile", &[("motif", Text("stairs"))])] },
    Material { name: "grate", group: Group::Built, about: "A grating of bars.", spacing: 1.0, layers: &[one("tile", &[("motif", Text("grate"))])] },
    Material { name: "chess", group: Group::Built, about: "A chessboard, alternate squares hatched.", spacing: 1.2, layers: &[one("tile", &[("motif", Text("chess"))])] },
    Material { name: "cobbles", group: Group::Built, about: "Rounded cobbles laid close.", spacing: 1.4, layers: &[one("cells", &[("round", Num(0.9)), ("gap", Rel(0.12))])] },
    Material { name: "terrazzo", group: Group::Built, about: "Stone chips of every size, some hatched, some inked.", spacing: 2.2, layers: &[one("cells", &[("vary", Num(0.9)), ("inner", Text("mixed")), ("gap", Rel(0.3))])] },
    Material {
        name: "cemetery",
        group: Group::Built,
        about: "The cemetery symbol: crosses in rows.",
        spacing: 2.4,
        layers: &[one("scatter", &[("motif", Text("grave")), ("arrange", Text("rows")), ("rotate", Text("upright")), ("mark", Rel(0.55))])],
    },
    alias("wood", Group::Built, "Wood grain flowing round knots: planks, boards, timber."),
    // ── Sky ────────────────────────────────────────────────────────────────
    alias("clouds", Group::Sky, "Cumulus seen from above, shaded where they turn from the sun."),
    // ── Plain ──────────────────────────────────────────────────────────────
    Material { name: "lines", group: Group::Plain, about: "Parallel lines: the plain hatch.", spacing: 0.6, layers: &[one("hatch", &[])] },
    Material { name: "crosshatch", group: Group::Plain, about: "Two families of lines crossing at right angles.", spacing: 0.8, layers: &[one("hatch", &[("cross", Num(2.0))])] },
    Material { name: "triad", group: Group::Plain, about: "Three families of lines at 60°: a triangular grid.", spacing: 1.0, layers: &[one("hatch", &[("cross", Num(3.0))])] },
    Material {
        name: "serpentine",
        group: Group::Plain,
        about: "Parallel lines joined end to end into one long stroke: the fastest hatch to plot.",
        spacing: 0.6,
        layers: &[one("hatch", &[("join", Flag(true)), ("angle", Num(45.0))])],
    },
    Material { name: "dashes", group: Group::Plain, about: "Rows of even dashes.", spacing: 0.8, layers: &[one("hatch", &[("dash", Rels(1.6, 0.8))])] },
    Material { name: "dots", group: Group::Plain, about: "Dots on a hexagonal lattice.", spacing: 0.6, layers: &[one("scatter", &[("arrange", Text("hex")), ("mark", Rel(0.25))])] },
    Material { name: "stipple", group: Group::Plain, about: "Dots placed evenly but irregularly, like a stipple by hand.", spacing: 0.55, layers: &[one("scatter", &[("mark", Rel(0.3))])] },
    Material { name: "concentric", group: Group::Plain, about: "Rings inward from the outline; holes stop them.", spacing: 0.6, layers: &[one("rings", &[])] },
    Material { name: "offsets", group: Group::Plain, about: "Rings from every edge, holes included, so networks such as streets fill evenly.", spacing: 0.6, layers: &[one("rings", &[("from", Text("edges"))])] },
    Material { name: "spiral", group: Group::Plain, about: "One continuous spiral inward.", spacing: 0.6, layers: &[one("rings", &[("spiral", Flag(true))])] },
    alias("scribble", Group::Plain, "Loose hand scribble-hatching."),
    alias("hilbert", Group::Plain, "A Hilbert curve: one unbroken space-filling line."),
    alias("truchet", Group::Plain, "Truchet arcs: a random maze of curves."),
    alias("honeycomb", Group::Plain, "Hexagonal cells."),
    alias("scales", Group::Plain, "Overlapping scales."),
    alias("sunburst", Group::Plain, "Rays from a centre."),
    alias("isoline-blobs", Group::Plain, "Contoured blobs, like a topographic map of islands."),
    alias("solid", Group::Plain, "Solid ink."),
];

/// A generator listed under its own name in a group (see [`Material::is_alias`]).
const fn alias(name: &'static str, group: Group, about: &'static str) -> Material {
    Material { name, group, about, spacing: 1.0, layers: &[] }
}

