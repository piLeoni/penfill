//! The pattern catalogue: **generators** (the algorithms, each describing
//! its own options) and **materials** (named recipes over them, grouped by
//! what they are for). See `docs/architecture/catalog.md`.
//!
//! ```
//! use penfill::catalog::{self, Spec};
//! use penfill::Region;
//!
//! let square = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
//! // A material, at its own spacing.
//! let gravel = catalog::fill("gravel", &square, &Spec::default()).unwrap();
//! // A generator, every option open.
//! let marsh = catalog::fill(
//!     "scatter",
//!     &square,
//!     &Spec::default().spacing(1.5).option("motif", "tuft").option("density", "noise"),
//! ).unwrap();
//! assert!(!gravel.strokes.is_empty() && !marsh.strokes.is_empty());
//! ```

mod gen_cells;
mod gen_hatch;
mod gen_scatter;
mod gen_special;
mod gen_tile;
mod gen_waves;
pub mod marks;
mod materials;
pub mod points;
pub mod schema;
pub mod tone;
pub mod value;

use std::fmt;

use crate::anchor::Anchor;
use crate::geo::{Region, Strokes};
use crate::pattern::{fill_pattern_output, fill_pattern_with, Canvas, EdgeMode, FillOutput, FillParams, Tools};

pub use materials::{Group, Material, MATERIALS};
pub use schema::{Args, Kind, Opt};
pub use value::{Options, Value};

/// One generator: an algorithm and the options it reads.
pub struct Generator {
    pub name: &'static str,
    /// One sentence: what it draws.
    pub about: &'static str,
    /// Spacing in mm when the generator is used directly, without a material.
    pub spacing: f64,
    /// Angle in degrees when the generator is used directly.
    pub angle: f64,
    /// Every option it reads, the shared ones included.
    pub options: &'static [&'static Opt],
    /// Whether it registers its own mask (shapes it covers), so hosts can
    /// mask what lies under its marks rather than under the whole area.
    pub own_mask: bool,
    pub draw: fn(&Canvas, &Tools, &Args) -> Strokes,
}

/// Every generator, in the order the documentation lists them.
pub fn generators() -> &'static [&'static Generator] {
    GENERATORS
}

static GENERATORS: &[&Generator] = &[
    &gen_hatch::HATCH,
    &gen_scatter::SCATTER,
    &gen_cells::CELLS,
    &gen_tile::TILE,
    &gen_waves::WAVES,
    &gen_special::RINGS,
    &gen_special::SCRIBBLE,
    &gen_special::HILBERT,
    &gen_special::TRUCHET,
    &gen_special::HONEYCOMB,
    &gen_special::SCALES,
    &gen_special::SUNBURST,
    &gen_special::WOOD,
    &gen_special::MARBLE,
    &gen_special::SWELLS,
    &gen_special::EDDIES,
    &gen_special::RIPPLES,
    &gen_special::CANOPY,
    &gen_special::TWIGS,
    &gen_special::DOODLE_TREES,
    &gen_special::GROWTH,
    &gen_special::ISOLINE_STACK,
    &gen_special::ISOLINE_BLOBS,
    &gen_special::CLOUDS,
    &gen_special::SOLID,
];

pub fn generator(name: &str) -> Option<&'static Generator> {
    GENERATORS.iter().copied().find(|g| g.name == name)
}

pub fn material(name: &str) -> Option<&'static Material> {
    MATERIALS.iter().find(|m| m.name == name)
}

/// Every name a fill can be asked for: materials first, then generators.
pub fn names() -> Vec<&'static str> {
    MATERIALS.iter().map(|m| m.name).chain(GENERATORS.iter().map(|g| g.name)).collect()
}

/// What a host asks for, beyond the pattern's name. Unset standard fields
/// take the material's (or generator's) own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spec {
    pub spacing: Option<f64>,
    pub angle: Option<f64>,
    pub seed: u64,
    pub anchor: Anchor,
    pub inset: Option<f64>,
    pub edge: Option<EdgeMode>,
    pub fray: f64,
    pub options: Options,
}

impl Spec {
    pub fn spacing(mut self, mm: f64) -> Spec {
        self.spacing = Some(mm);
        self
    }
    pub fn angle(mut self, deg: f64) -> Spec {
        self.angle = Some(deg);
        self
    }
    pub fn seed(mut self, seed: u64) -> Spec {
        self.seed = seed;
        self
    }
    pub fn anchor(mut self, anchor: Anchor) -> Spec {
        self.anchor = anchor;
        self
    }
    pub fn inset(mut self, mm: f64) -> Spec {
        self.inset = Some(mm);
        self
    }
    pub fn edge(mut self, edge: EdgeMode) -> Spec {
        self.edge = Some(edge);
        self
    }
    pub fn fray(mut self, mm: f64) -> Spec {
        self.fray = mm;
        self
    }
    pub fn option(mut self, key: &str, value: impl Into<Value>) -> Spec {
        self.options.set(key, value);
        self
    }
}

/// Why a fill could not be made. Every message says what to write instead.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogError {
    UnknownPattern { name: String, near: Vec<String> },
    UnknownOption { pattern: String, key: String, valid: Vec<String> },
    BadValue { pattern: String, key: String, expected: String, got: String },
    OutOfRange { pattern: String, key: String, min: f64, max: f64, got: f64 },
    BadSpacing { pattern: String, got: f64 },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogError::UnknownPattern { name, near } if near.is_empty() => write!(f, "unknown pattern {name:?}"),
            CatalogError::UnknownPattern { name, near } => {
                write!(f, "unknown pattern {name:?}; did you mean {}?", near.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>().join(" or "))
            }
            CatalogError::UnknownOption { pattern, key, valid } => {
                write!(f, "{pattern} has no option {key:?}; it takes {}", valid.join(", "))
            }
            CatalogError::BadValue { pattern, key, expected, got } => write!(f, "{pattern}: {key} is {expected}, got {got}"),
            CatalogError::OutOfRange { pattern, key, min, max, got } => write!(f, "{pattern}: {key} goes from {min} to {max}, got {got}"),
            CatalogError::BadSpacing { pattern, got } => write!(f, "{pattern}: spacing must be more than 0, got {got}"),
        }
    }
}

impl std::error::Error for CatalogError {}

/// The strokes of pattern `name` (a material or a generator) in `region`.
pub fn fill(name: &str, region: &Region, spec: &Spec) -> Result<FillOutput, CatalogError> {
    run(name, region, spec, false)
}

/// The same, also returning the mask: the area the pattern really covers.
pub fn fill_output(name: &str, region: &Region, spec: &Spec) -> Result<FillOutput, CatalogError> {
    run(name, region, spec, true)
}

/// Check `spec` against `name` without drawing: what a host runs when a
/// profile is read, so a mistake is reported before any map data arrives.
pub fn validate(name: &str, spec: &Spec) -> Result<(), CatalogError> {
    let layers = plan(name, spec)?;
    for l in &layers {
        Args::resolve(name, l.generator.options, l.spacing, &l.given())?;
    }
    Ok(())
}

/// One layer to draw: a generator, its spacing and angle, and its options.
struct Planned<'a> {
    generator: &'static Generator,
    spacing: f64,
    angle: f64,
    salt: u64,
    edge: Option<EdgeMode>,
    values: Vec<(&'a str, Value)>,
}

impl Planned<'_> {
    fn given(&self) -> Vec<(&str, &Value)> {
        self.values.iter().map(|(k, v)| (*k, v)).collect()
    }
}

/// The layers `name` draws with `spec`: a material's recipe, scaled and with
/// the user's options routed to the layers that take them; or a generator.
fn plan<'a>(name: &str, spec: &'a Spec) -> Result<Vec<Planned<'a>>, CatalogError> {
    if let Some(s) = spec.spacing {
        if !(s > 0.0 && s.is_finite()) {
            return Err(CatalogError::BadSpacing { pattern: name.to_string(), got: s });
        }
    }
    if let Some(g) = generator(name) {
        let values = spec.options.iter().map(|(k, v)| (k, v.clone())).collect();
        return Ok(vec![Planned {
            generator: g,
            spacing: spec.spacing.unwrap_or(g.spacing),
            angle: spec.angle.unwrap_or(g.angle),
            salt: 0,
            edge: spec.edge,
            values,
        }]);
    }
    let Some(m) = material(name) else {
        return Err(CatalogError::UnknownPattern { name: name.to_string(), near: near(name) });
    };
    // Every user option must be taken by at least one layer.
    for (key, _) in spec.options.iter() {
        let taken = m.layers.iter().any(|l| generator(l.generator).is_some_and(|g| g.options.iter().any(|o| o.key == key)));
        if !taken {
            let mut valid: Vec<String> = m
                .layers
                .iter()
                .filter_map(|l| generator(l.generator))
                .flat_map(|g| g.options.iter().map(|o| o.key.to_string()))
                .collect();
            valid.sort();
            valid.dedup();
            return Err(CatalogError::UnknownOption { pattern: name.to_string(), key: key.to_string(), valid });
        }
    }
    let scale = spec.spacing.map_or(1.0, |s| s / m.spacing);
    let base = m.spacing * scale;
    let mut out = Vec::new();
    for (i, layer) in m.layers.iter().enumerate() {
        let g = generator(layer.generator).unwrap_or_else(|| panic!("material {} names unknown generator {}", m.name, layer.generator));
        let mut spacing = base;
        let mut angle = spec.angle.unwrap_or(0.0);
        let mut edge = None;
        let mut values: Vec<(&str, Value)> = Vec::new();
        for (key, v) in layer.set {
            match (*key, v) {
                ("spacing", materials::M::Rel(k)) => spacing = base * k,
                ("angle", materials::M::Num(a)) => angle += a,
                ("edge", materials::M::Text("inside")) => edge = Some(EdgeMode::Inside),
                ("edge", materials::M::Text("clip")) => edge = Some(EdgeMode::Clip),
                ("edge", materials::M::Rel(k)) => edge = Some(EdgeMode::Overflow(base * k)),
                _ => values.push((key, v.resolve(base))),
            }
        }
        for (key, v) in spec.options.iter() {
            if g.options.iter().any(|o| o.key == key) {
                values.retain(|(k, _)| *k != key);
                values.push((key, v.clone()));
            }
        }
        out.push(Planned { generator: g, spacing, angle, salt: i as u64, edge: spec.edge.or(edge), values });
    }
    Ok(out)
}

fn run(name: &str, region: &Region, spec: &Spec, mask: bool) -> Result<FillOutput, CatalogError> {
    let layers = plan(name, spec)?;
    let mut out = FillOutput::default();
    for l in &layers {
        let args = Args::resolve(name, l.generator.options, l.spacing, &l.given())?;
        let mut params = FillParams::new(l.spacing)
            .angle(l.angle)
            .seed(spec.seed.wrapping_add(l.salt.wrapping_mul(0x9E37_79B9)))
            .anchor(spec.anchor)
            .inset_opt(spec.inset)
            .fray(spec.fray);
        if let Some(e) = l.edge {
            params = params.edge(e);
        }
        let draw = l.generator.draw;
        if mask {
            let o = fill_pattern_output(region, &params, |c, t| draw(c, t, &args));
            out.strokes.extend(o.strokes);
            out.mask = if out.mask.is_empty() { o.mask } else { out.mask.union(&o.mask) };
        } else {
            out.strokes.extend(fill_pattern_with(region, &params, |c, t| draw(c, t, &args)));
        }
    }
    Ok(out)
}

/// Names close to `name`, for "did you mean".
fn near(name: &str) -> Vec<String> {
    let mut scored: Vec<(usize, &str)> = names().into_iter().map(|n| (distance(name, n), n)).filter(|(d, _)| *d <= 3).collect();
    scored.sort();
    scored.into_iter().take(2).map(|(_, n)| n.to_string()).collect()
}

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

// ---------------------------------------------------------------------------
// Self-description
// ---------------------------------------------------------------------------

/// The catalogue as JSON: every generator with its options, every material
/// with its group and recipe. Hosts generate their reference from it.
pub fn describe() -> String {
    let mut s = String::from("{\n  \"generators\": [\n");
    for (gi, g) in GENERATORS.iter().enumerate() {
        s += &format!(
            "    {{ \"name\": {}, \"about\": {}, \"spacing\": {}, \"angle\": {}, \"ownMask\": {}, \"options\": [\n",
            json(g.name),
            json(g.about),
            num(g.spacing),
            num(g.angle),
            g.own_mask
        );
        for (oi, o) in g.options.iter().enumerate() {
            s += &format!(
                "      {{ \"key\": {}, \"kind\": {}, \"default\": {}, \"doc\": {} }}{}\n",
                json(o.key),
                kind_json(&o.kind),
                default_json(&o.default),
                json(o.doc),
                if oi + 1 < g.options.len() { "," } else { "" }
            );
        }
        s += &format!("    ] }}{}\n", if gi + 1 < GENERATORS.len() { "," } else { "" });
    }
    s += "  ],\n  \"groups\": [";
    s += &Group::ALL.iter().map(|g| format!("{{ \"id\": {}, \"title\": {}, \"about\": {} }}", json(g.id()), json(g.title()), json(g.about()))).collect::<Vec<_>>().join(", ");
    s += "],\n  \"materials\": [\n";
    for (mi, m) in MATERIALS.iter().enumerate() {
        let layers = m
            .layers
            .iter()
            .map(|l| {
                let set = l.set.iter().map(|(k, v)| format!("{}: {}", json(k), v.json())).collect::<Vec<_>>().join(", ");
                format!("{{ \"generator\": {}, \"set\": {{ {set} }} }}", json(l.generator))
            })
            .collect::<Vec<_>>()
            .join(", ");
        s += &format!(
            "    {{ \"name\": {}, \"group\": {}, \"about\": {}, \"spacing\": {}, \"layers\": [{layers}] }}{}\n",
            json(m.name),
            json(m.group.id()),
            json(m.about),
            num(m.spacing),
            if mi + 1 < MATERIALS.len() { "," } else { "" }
        );
    }
    s += "  ]\n}\n";
    s
}

fn json(t: &str) -> String {
    let mut s = String::with_capacity(t.len() + 2);
    s.push('"');
    for c in t.chars() {
        match c {
            '"' => s += "\\\"",
            '\\' => s += "\\\\",
            '\n' => s += "\\n",
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{}", (v * 1e6).round() / 1e6)
    }
}

fn words(ws: &[&str]) -> String {
    format!("[{}]", ws.iter().map(|w| json(w)).collect::<Vec<_>>().join(", "))
}

fn kind_json(k: &Kind) -> String {
    match k {
        Kind::Length => "{ \"type\": \"length\" }".into(),
        Kind::Number { min, max } => format!("{{ \"type\": \"number\", \"min\": {}, \"max\": {} }}", num(*min), num(*max)),
        Kind::Count { min, max } => format!("{{ \"type\": \"count\", \"min\": {min}, \"max\": {max} }}"),
        Kind::Flag => "{ \"type\": \"flag\" }".into(),
        Kind::Choice(ws) => format!("{{ \"type\": \"choice\", \"values\": {} }}", words(ws)),
        Kind::Lengths => "{ \"type\": \"lengths\" }".into(),
        Kind::Motif(ws) => format!("{{ \"type\": \"motif\", \"values\": {} }}", words(ws)),
        Kind::Text => "{ \"type\": \"text\" }".into(),
        Kind::NumberOr(ws) => format!("{{ \"type\": \"numberOr\", \"values\": {} }}", words(ws)),
    }
}

fn default_json(d: &schema::Default) -> String {
    use schema::Default as D;
    match d {
        D::None => "null".into(),
        D::Num(n) => num(*n),
        D::Rel(k) => format!("{{ \"timesSpacing\": {} }}", num(*k)),
        D::Flag(b) => b.to_string(),
        D::Text(t) => json(t),
        D::Rels(a, b) => format!("{{ \"timesSpacing\": [{}, {}] }}", num(*a), num(*b)),
    }
}
