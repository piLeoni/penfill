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
    Plain,
}

impl Group {
    pub const ALL: [Group; 6] = [Group::Water, Group::Vegetation, Group::Ground, Group::Rock, Group::Built, Group::Plain];

    pub fn id(self) -> &'static str {
        match self {
            Group::Water => "water",
            Group::Vegetation => "vegetation",
            Group::Ground => "ground",
            Group::Rock => "rock",
            Group::Built => "built",
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

/// A named recipe.
pub struct Material {
    pub name: &'static str,
    pub group: Group,
    /// One sentence: what it looks like and what it is for.
    pub about: &'static str,
    /// Default spacing in mm, chosen to read at a 0.2–0.3 mm pen.
    pub spacing: f64,
    pub layers: &'static [Layer],
}

const fn one(generator: &'static str, set: &'static [(&'static str, M)]) -> Layer {
    Layer { generator, set }
}

use M::{Flag, Num, Rel, Rels, Text};

/// The catalogue's materials, group by group.
pub static MATERIALS: &[Material] = &[
    // ── Plain ──────────────────────────────────────────────────────────────
    Material { name: "lines", group: Group::Plain, about: "Parallel lines: the plain hatch.", spacing: 0.6, layers: &[one("hatch", &[])] },
    Material {
        name: "crosshatch",
        group: Group::Plain,
        about: "Two families of lines crossing at right angles.",
        spacing: 0.8,
        layers: &[one("hatch", &[("cross", Num(2.0))])],
    },
    Material {
        name: "serpentine",
        group: Group::Plain,
        about: "Parallel lines joined end to end into one long stroke: the fastest hatch to plot.",
        spacing: 0.6,
        layers: &[one("hatch", &[("join", Flag(true)), ("angle", Num(45.0))])],
    },
    Material {
        name: "dashes",
        group: Group::Plain,
        about: "Rows of even dashes.",
        spacing: 0.8,
        layers: &[one("hatch", &[("dash", Rels(1.6, 0.8))])],
    },
];

#[allow(dead_code)]
const _USE: (M, M, M) = (Rel(0.0), Text(""), Flag(false));
