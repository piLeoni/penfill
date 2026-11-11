//! `waves`: wavy parallel lines, one generator for every way a line can
//! wave — a `sine`, `scallop`s dipping between sharp cusps, a `zigzag` of
//! sharp teeth (amphibolite, schist), `curl`ing crests (the stylised
//! breaking wave), and a `flow`ing sea of long swells (the printed-textile
//! look). `cross: 2` doubles the lines into a net.
//!
//! `spacing` is always the distance between neighbouring lines, and
//! `wavelength` and `amplitude` are lengths in mm whatever the shape: the
//! length of one wave along the line, and how far the line swings away from
//! its row. Left unset they take the shape's own proportions, so each shape
//! keeps its look at any spacing.
//!
//! Rows sit at `j · spacing` in the anchored frame and every wave starts at
//! a whole number of wavelengths from the anchor; `flow`'s field and its
//! breaks are functions of the frame alone. So with a global anchor two
//! areas side by side draw one sea.

use super::schema::{Args, Default, Kind, Opt};
use super::Generator;
use crate::anchor::{indices, Frame};
use crate::geo::{Pt, Strokes};
use crate::pattern::{Canvas, Tools};
use crate::waves_flow::{flow_lines, Flow};

const SHAPE: Opt = Opt {
    key: "shape",
    kind: Kind::Choice(&["sine", "scallop", "zigzag", "curl", "flow"]),
    default: Default::Text("sine"),
    doc: "How the lines wave: `sine`, the smooth classic; `scallop`, shallow arcs dipping between sharp cusps; `zigzag`, sharp straight teeth (amphibolite, schist); `curl`, crests that roll over into a spiral hook; `flow`, long swells the lines bunch and spread over, broken into strokes like a printed textile.",
};
const WAVELENGTH: Opt = Opt {
    key: "wavelength",
    kind: Kind::Length,
    default: Default::None,
    doc: "The length of one wave along the line, in mm. Default by shape, × spacing: sine 4 (3.6 with `cross: 2`), scallop 1.35, zigzag 1, curl 1.35, flow 13.",
};
const AMPLITUDE: Opt = Opt {
    key: "amplitude",
    kind: Kind::Length,
    default: Default::None,
    doc: "How far a line swings away from its row, in mm: a sine's or zigzag's peak, a scallop's dip, a curl's crest, a swell's height. Default by shape, × spacing: sine 0.25 (0.5 with `cross: 2`, so the net closes), scallop 0.35, zigzag 0.3, curl 0.27, flow 2.6.",
};
const CROSS: Opt = Opt {
    key: "cross",
    kind: Kind::Count { min: 1, max: 2 },
    default: Default::Num(1.0),
    doc: "1 one family of lines; 2 a net. Sine, zigzag and scallop add every row's mirror image, so crest meets trough and the lines close into cells (the ogee net, diamonds, eyes); curl and flow add a second family at right angles.",
};
const STAGGER: Opt = Opt {
    key: "stagger",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::Num(0.0),
    doc: "Sine, scallop, zigzag and curl: shift every other row along by this share of a wavelength (0.5 sets the cusps of one row over the middles of the next).",
};
const FLIP: Opt = Opt {
    key: "flip",
    kind: Kind::Flag,
    default: Default::Flag(false),
    doc: "Turn the waves over: curls break the other way, scallops bulge up between downward cusps.",
};
const ROUND: Opt = Opt {
    key: "round",
    kind: Kind::Number { min: 0.3, max: 2.0 },
    default: Default::Num(0.8),
    doc: "Scallop: the arc's shape, below 1 fuller and rounder, above 1 drawn to a point.",
};
const CURL: Opt = Opt {
    key: "curl",
    kind: Kind::Number { min: 0.3, max: 1.5 },
    default: Default::Num(1.0),
    doc: "Curl: the size of the hook as a share of the amplitude. 1 rolls down onto the next wave's trough; smaller hooks hang above it.",
};
const TURNS: Opt = Opt {
    key: "turns",
    kind: Kind::Number { min: 0.3, max: 3.0 },
    default: Default::Num(0.95),
    doc: "Curl: how far each hook winds in, in turns (1 is one full turn).",
};
const DRIFT: Opt = Opt {
    key: "drift",
    kind: Kind::Number { min: 0.0, max: 5.0 },
    default: Default::Num(1.0),
    doc: "Flow: how fast the swell's height and phase change from line to line across the sea (0 keeps one swell everywhere).",
};
const BREAKS: Opt = Opt {
    key: "breaks",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::Num(0.5),
    doc: "Flow: how broken the lines are, 0 (unbroken) to 1 (short strokes).",
};

pub static WAVES: Generator = Generator {
    name: "waves",
    about: "Wavy parallel lines: sine, scallops, zigzag teeth, curling crests or a flowing swell; doubled into a net with `cross: 2`.",
    spacing: 1.0,
    angle: 0.0,
    options: &[&SHAPE, &WAVELENGTH, &AMPLITUDE, &CROSS, &STAGGER, &FLIP, &ROUND, &CURL, &TURNS, &DRIFT, &BREAKS],
    own_mask: false,
    draw,
};

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Sine,
    Scallop,
    Zigzag,
    Curl,
    Flow,
}

impl Shape {
    fn parse(word: &str) -> Shape {
        match word {
            "scallop" => Shape::Scallop,
            "zigzag" => Shape::Zigzag,
            "curl" => Shape::Curl,
            "flow" => Shape::Flow,
            _ => Shape::Sine,
        }
    }

    /// `(wavelength, amplitude)` as multiples of the spacing: each shape's
    /// own proportions.
    fn proportions(self, cross: usize) -> (f64, f64) {
        match self {
            // Mirrored sines close into ogee cells when each swings half a row.
            Shape::Sine if cross == 2 => (3.6, 0.5),
            Shape::Sine => (4.0, 0.25),
            Shape::Scallop => (1.35, 0.35),
            Shape::Zigzag => (1.0, 0.3),
            Shape::Curl => (1.35, 0.27),
            Shape::Flow => (13.0, 2.6),
        }
    }
}

fn draw(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    let s = c.spacing;
    let shape = Shape::parse(a.word("shape"));
    let cross = a.count("cross").clamp(1, 2) as usize;
    let (kw, ka) = shape.proportions(cross);
    let wavelength = a.opt_num("wavelength").unwrap_or(kw * s).max(s * 0.05).max(0.02);
    let amplitude = a.opt_num("amplitude").unwrap_or(ka * s).max(0.0);
    let stagger = a.num("stagger");
    let flip = a.flag("flip");

    match shape {
        Shape::Flow => {
            let flow = Flow { wavelength, amplitude, drift: a.num("drift"), breaks: a.num("breaks") };
            (0..cross).flat_map(|k| flow_lines(&c.limit, t.frame(c.angle + 90.0 * k as f64), s, &flow, c.seed ^ k as u64)).collect()
        }
        Shape::Curl => {
            let hook = amplitude * a.num("curl");
            let mut unit = crate::waves_curl::wave_unit(wavelength, amplitude, hook, a.num("turns"));
            if flip {
                for p in unit.iter_mut() {
                    p.0 = wavelength - p.0;
                }
            }
            let reach = amplitude.max(hook * 2.0);
            (0..cross).flat_map(|k| curl_rows(c, t.frame(c.angle + 90.0 * k as f64), &unit, wavelength, reach, stagger)).collect()
        }
        _ => {
            let sign = if flip { -1.0 } else { 1.0 };
            let rows = Rows { c, frame: t.frame(c.angle), shape, wavelength, amplitude, round: a.num("round"), stagger };
            let mut out = rows.draw(sign);
            if cross == 2 {
                out.extend(rows.draw(-sign));
            }
            out
        }
    }
}

/// One family of profile rows (sine, scallop, zigzag) in the anchored frame.
struct Rows<'a> {
    c: &'a Canvas<'a>,
    frame: Frame,
    shape: Shape,
    wavelength: f64,
    amplitude: f64,
    round: f64,
    stagger: f64,
}

impl Rows<'_> {
    /// How far the line is from its row `x` wavelengths along.
    fn profile(&self, x: f64) -> f64 {
        let amp = self.amplitude;
        match self.shape {
            Shape::Zigzag => amp * (1.0 - 4.0 * ((x + 0.25).rem_euclid(1.0) - 0.5).abs()),
            Shape::Scallop => crate::waves_scallop::scallop(x.rem_euclid(1.0), amp, self.round),
            _ => amp * (std::f64::consts::TAU * x).sin(),
        }
    }

    /// Every row, waving by `sign · profile`, clipped to the limit.
    fn draw(&self, sign: f64) -> Strokes {
        let (s, w) = (self.c.spacing, self.wavelength);
        // Where along one wave (0 – 1) to put points: a zigzag needs only
        // its corners, a curve enough points to read smooth at any size.
        let (offset, per) = match self.shape {
            Shape::Zigzag => (0.25, 2usize),
            Shape::Scallop => (0.0, 16),
            _ => (0.0, ((w / 0.08).ceil() as usize).clamp(12, 48)),
        };
        let (u0, u1, v0, v1) = self.frame.span(&self.c.limit, s + self.amplitude + w);
        let mut out = Strokes::new();
        for j in indices(v0, v1, s) {
            let v = j as f64 * s;
            let shift = if j.rem_euclid(2) == 1 { self.stagger * w } else { 0.0 };
            let (n0, n1) = (((u0 - shift) / w).floor() as i64 - 1, ((u1 - shift) / w).ceil() as i64 + 1);
            let mut line: Vec<Pt> = Vec::with_capacity(((n1 - n0) as usize + 1) * per);
            for n in n0..=n1 {
                for k in 0..per {
                    let x = n as f64 + offset + k as f64 / per as f64;
                    line.push(self.frame.world((shift + x * w, v + sign * self.profile(x))));
                }
            }
            let line = if self.shape == Shape::Zigzag { line } else { crate::field::simplify(&line, 0.004) };
            out.extend(self.c.limit.clip_polyline(&line));
        }
        out
    }
}

/// Rows of curling waves, one stroke per wave, `unit` laid end to end.
fn curl_rows(c: &Canvas, frame: Frame, unit: &[Pt], w: f64, reach: f64, stagger: f64) -> Strokes {
    let s = c.spacing;
    let (u0, u1, v0, v1) = frame.span(&c.limit, s + reach + w);
    let mut out = Strokes::new();
    for j in indices(v0, v1, s) {
        let v = j as f64 * s;
        let shift = if j.rem_euclid(2) == 1 { stagger * w } else { 0.0 };
        for i in indices(u0 - shift, u1 - shift, w) {
            let u = i as f64 * w + shift;
            let wave: Vec<Pt> = unit.iter().map(|p| frame.world((u + p.0, v + p.1))).collect();
            out.extend(c.limit.clip_polyline(&wave));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::catalog::{fill, Spec};
    use crate::geo::{polyline_length, Region, Strokes};
    use crate::Anchor;

    const SHAPES: [&str; 5] = ["sine", "scallop", "zigzag", "curl", "flow"];

    fn rect(x0: f64, y0: f64, w: f64, h: f64) -> Region {
        Region::from_ring(&[(x0, y0), (x0 + w, y0), (x0 + w, y0 + h), (x0, y0 + h)])
    }

    fn ink(s: &Strokes) -> f64 {
        s.iter().map(|p| polyline_length(p)).sum()
    }

    fn waves(region: &Region, spec: Spec) -> Strokes {
        fill("waves", region, &spec).unwrap().strokes
    }

    #[test]
    fn every_shape_draws_the_same_twice() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        for shape in SHAPES {
            for cross in [1.0, 2.0] {
                let spec = Spec::default().seed(4).option("shape", shape).option("cross", cross);
                let a = waves(&sq, spec.clone());
                assert!(ink(&a) > 200.0, "{shape} × {cross}: {}", ink(&a));
                assert_eq!(a, waves(&sq, spec), "{shape} is not deterministic");
            }
        }
    }

    #[test]
    fn the_net_adds_a_second_family() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        for shape in SHAPES {
            let one = ink(&waves(&sq, Spec::default().option("shape", shape)));
            let two = ink(&waves(&sq, Spec::default().option("shape", shape).option("cross", 2.0)));
            assert!(two > one * 1.6, "{shape}: {one} → {two}");
        }
    }

    #[test]
    fn a_global_anchor_leaves_no_seam() {
        // Away from the page origin, so nothing is measured from (0, 0) by accident.
        let at = Spec::default().anchor(Anchor::GLOBAL).seed(2).angle(17.0);
        for shape in SHAPES {
            let spec = at.clone().option("shape", shape);
            let whole = ink(&waves(&rect(130.0, 70.0, 40.0, 20.0), spec.clone()));
            let halves = ink(&waves(&rect(130.0, 70.0, 20.0, 20.0), spec.clone())) + ink(&waves(&rect(150.0, 70.0, 20.0, 20.0), spec));
            assert!((whole - halves).abs() < 0.02 * whole, "{shape}: whole {whole} vs halves {halves}");
        }
    }

    #[test]
    fn wavelength_and_amplitude_are_millimetres() {
        // A sine's ink per row grows with its amplitude at a fixed wavelength.
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let flat = ink(&waves(&sq, Spec::default().option("amplitude", 0.0)));
        let tall = ink(&waves(&sq, Spec::default().option("amplitude", 1.0).option("wavelength", 4.0)));
        // 21 rows of 20 mm; a sine 1 mm high every 4 mm is about 1.46× longer.
        assert!((flat / (21.0 * 20.0) - 1.0).abs() < 0.08, "flat {flat}");
        assert!((tall / flat - 1.46).abs() < 0.08, "tall/flat {}", tall / flat);
        // The same wavelength in mm whatever the spacing: rows double, ink per row stays.
        let a = ink(&waves(&sq, Spec::default().spacing(1.0).option("wavelength", 3.0).option("amplitude", 0.4)));
        let b = ink(&waves(&sq, Spec::default().spacing(0.5).option("wavelength", 3.0).option("amplitude", 0.4)));
        assert!((b / a - 2.0).abs() < 0.12, "{a} {b}");
    }

    #[test]
    fn options_are_checked() {
        let sq = rect(0.0, 0.0, 10.0, 10.0);
        assert!(fill("waves", &sq, &Spec::default().option("shape", "spiral")).is_err());
        assert!(fill("waves", &sq, &Spec::default().option("depth", 0.3)).is_err());
        assert!(fill("waves", &sq, &Spec::default().option("cross", 3.0)).is_err());
    }
}
