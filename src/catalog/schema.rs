//! How generators describe their options, and the resolved option set a
//! generator reads ([`Args`]).
//!
//! One declaration per option serves three purposes: it validates what a host
//! sends (a typo is an error naming the valid keys, not a silent default), it
//! supplies the default, and it documents the option (hosts build their
//! reference pages from [`super::describe`]).

use std::collections::BTreeMap;

use super::value::Value;
use super::CatalogError;
use crate::geo::Strokes;

/// What kind of value an option takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A length in mm.
    Length,
    /// A plain number in `min..=max`.
    Number { min: f64, max: f64 },
    /// A whole number in `min..=max`.
    Count { min: i64, max: i64 },
    Flag,
    /// One of these words.
    Choice(&'static [&'static str]),
    /// Two lengths in mm (`[on, off]`, `[width, height]`); one number means both.
    Lengths,
    /// A symbol by name (one of these), or polylines in mm around the origin.
    Motif(&'static [&'static str]),
    /// Free text (a CAD `.pat` definition).
    Text,
    /// A number, or one of these words (`rotate: 30` or `rotate: "random"`).
    NumberOr(&'static [&'static str]),
}

/// An option's default. `Rel` lengths are multiples of the spacing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Default {
    None,
    Num(f64),
    Rel(f64),
    Flag(bool),
    Text(&'static str),
    Rels(f64, f64),
}

/// One option a generator reads.
#[derive(Debug, PartialEq)]
pub struct Opt {
    pub key: &'static str,
    pub kind: Kind,
    pub default: Default,
    pub doc: &'static str,
}

// ---------------------------------------------------------------------------
// The shared vocabulary: every generator that has the idea uses these.
// ---------------------------------------------------------------------------

pub const MARK: Opt = Opt {
    key: "mark",
    kind: Kind::Length,
    default: Default::Rel(0.35),
    doc: "The size of one element: a dot's length, a symbol's height, in mm.",
};
pub const JITTER: Opt = Opt {
    key: "jitter",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::Num(0.0),
    doc: "How far elements wander from their place, 0 to 1.",
};
pub const VARY: Opt = Opt {
    key: "vary",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::Num(0.0),
    doc: "How much element sizes differ, 0 (all alike) to 1.",
};
pub const DENSITY: Opt = Opt {
    key: "density",
    kind: Kind::Choice(&["even", "edge", "centre", "noise", "gradient"]),
    default: Default::Text("even"),
    doc: "The tone across the area: even, denser toward the edge, toward the centre, in noisy patches, or along a gradient toward `toward` degrees.",
};
pub const CONTRAST: Opt = Opt {
    key: "contrast",
    kind: Kind::Number { min: 0.0, max: 1.0 },
    default: Default::Num(0.8),
    doc: "How far the tone swings, 0 (none) to 1 (from full to nothing).",
};
pub const FEATURE: Opt = Opt {
    key: "feature",
    kind: Kind::Length,
    default: Default::Rel(8.0),
    doc: "The scale of the tone: the width of the edge band, the size of noise patches, in mm.",
};
pub const TOWARD: Opt = Opt {
    key: "toward",
    kind: Kind::Number { min: -360.0, max: 360.0 },
    default: Default::Num(90.0),
    doc: "With `density: \"gradient\"`: the direction the tone gets denser, in page degrees (90: down the page).",
};

/// The tone options, for generators that follow the density field.
pub const TONE: [&Opt; 4] = [&DENSITY, &CONTRAST, &FEATURE, &TOWARD];

/// The motifs a motif option may name, or `polylines` for your own.
pub enum Motif<'a> {
    Named(&'a str),
    Lines(&'a Strokes),
}

/// A generator's options, resolved: defaults applied, `Rel` lengths turned
/// into mm, and every value checked against its kind.
#[derive(Debug)]
pub struct Args {
    pub spacing: f64,
    values: BTreeMap<&'static str, Value>,
}

impl Args {
    /// Defaults for `opts` at `spacing`, then `given` on top (each checked).
    pub(crate) fn resolve(
        pattern: &str,
        opts: &[&'static Opt],
        spacing: f64,
        given: &[(&str, &Value)],
    ) -> Result<Args, CatalogError> {
        let mut values = BTreeMap::new();
        for o in opts {
            let v = match o.default {
                Default::None => continue,
                Default::Num(n) => Value::Num(n),
                Default::Rel(k) => Value::Num(k * spacing),
                Default::Flag(b) => Value::Flag(b),
                Default::Text(t) => Value::Text(t.to_string()),
                Default::Rels(a, b) => Value::Nums(vec![a * spacing, b * spacing]),
            };
            values.insert(o.key, v);
        }
        for (key, value) in given {
            let o = opts.iter().find(|o| o.key == *key).ok_or_else(|| CatalogError::UnknownOption {
                pattern: pattern.to_string(),
                key: key.to_string(),
                valid: opts.iter().map(|o| o.key.to_string()).collect(),
            })?;
            values.insert(o.key, check(pattern, o, value)?);
        }
        Ok(Args { spacing, values })
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// A length or number option. Panics if the generator reads a key it did
    /// not declare (a bug in the generator, caught by its tests).
    pub fn num(&self, key: &str) -> f64 {
        match self.get(key) {
            Some(Value::Num(n)) => *n,
            Some(Value::Flag(b)) => f64::from(u8::from(*b)),
            Some(Value::Nums(v)) if !v.is_empty() => v[0],
            _ => panic!("generator read undeclared or unset option {key:?}"),
        }
    }

    pub fn opt_num(&self, key: &str) -> Option<f64> {
        match self.get(key) {
            Some(Value::Num(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn count(&self, key: &str) -> i64 {
        self.num(key).round() as i64
    }

    pub fn flag(&self, key: &str) -> bool {
        matches!(self.get(key), Some(Value::Flag(true))) || matches!(self.get(key), Some(Value::Num(n)) if *n != 0.0)
    }

    pub fn word(&self, key: &str) -> &str {
        match self.get(key) {
            Some(Value::Text(t)) => t,
            _ => "",
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// A pair of lengths (`[a, b]`; one number means both).
    pub fn pair(&self, key: &str) -> Option<(f64, f64)> {
        match self.get(key) {
            Some(Value::Nums(v)) if v.len() >= 2 => Some((v[0], v[1])),
            Some(Value::Nums(v)) if v.len() == 1 => Some((v[0], v[0])),
            Some(Value::Num(n)) => Some((*n, *n)),
            _ => None,
        }
    }

    /// A number-or-word option: `Ok(number)` or `Err(word)`.
    pub fn number_or(&self, key: &str) -> Result<f64, &str> {
        match self.get(key) {
            Some(Value::Num(n)) => Ok(*n),
            Some(Value::Text(t)) => Err(t),
            _ => Err(""),
        }
    }

    pub fn motif(&self, key: &str) -> Option<Motif<'_>> {
        match self.get(key) {
            Some(Value::Text(t)) => Some(Motif::Named(t)),
            Some(Value::Lines(l)) => Some(Motif::Lines(l)),
            _ => None,
        }
    }
}

/// `value` as `o` takes it, or an error that says what it should be.
fn check(pattern: &str, o: &Opt, value: &Value) -> Result<Value, CatalogError> {
    let bad = |expected: String| CatalogError::BadValue {
        pattern: pattern.to_string(),
        key: o.key.to_string(),
        expected,
        got: value.kind().to_string(),
    };
    match (o.kind, value) {
        (Kind::Length, Value::Num(n)) if n.is_finite() => Ok(Value::Num(*n)),
        (Kind::Length, _) => Err(bad("a length in mm".into())),
        (Kind::Number { min, max }, Value::Num(n)) if n.is_finite() => {
            if *n < min || *n > max {
                Err(CatalogError::OutOfRange { pattern: pattern.to_string(), key: o.key.to_string(), min, max, got: *n })
            } else {
                Ok(Value::Num(*n))
            }
        }
        (Kind::Number { min, max }, _) => Err(bad(format!("a number from {min} to {max}"))),
        (Kind::Count { min, max }, Value::Num(n)) if n.fract() == 0.0 => {
            if (*n as i64) < min || (*n as i64) > max {
                Err(CatalogError::OutOfRange { pattern: pattern.to_string(), key: o.key.to_string(), min: min as f64, max: max as f64, got: *n })
            } else {
                Ok(Value::Num(*n))
            }
        }
        (Kind::Count { min, max }, _) => Err(bad(format!("a whole number from {min} to {max}"))),
        (Kind::Flag, Value::Flag(b)) => Ok(Value::Flag(*b)),
        (Kind::Flag, _) => Err(bad("true or false".into())),
        (Kind::Choice(words), Value::Text(t)) if words.contains(&t.as_str()) => Ok(value.clone()),
        (Kind::Choice(words), _) => Err(bad(format!("one of {}", list(words)))),
        (Kind::Lengths, Value::Num(n)) => Ok(Value::Nums(vec![*n, *n])),
        (Kind::Lengths, Value::Nums(v)) if (1..=2).contains(&v.len()) && v.iter().all(|x| x.is_finite()) => Ok(value.clone()),
        (Kind::Lengths, _) => Err(bad("a length, or two as [a, b]".into())),
        (Kind::Motif(names), Value::Text(t)) if names.contains(&t.as_str()) => Ok(value.clone()),
        (Kind::Motif(_), Value::Lines(l)) if !l.is_empty() => Ok(value.clone()),
        (Kind::Motif(names), _) => Err(bad(format!("one of {}, or polylines in mm around the origin", list(names)))),
        (Kind::Text, Value::Text(_)) => Ok(value.clone()),
        (Kind::Text, _) => Err(bad("text".into())),
        (Kind::NumberOr(_), Value::Num(n)) if n.is_finite() => Ok(Value::Num(*n)),
        (Kind::NumberOr(words), Value::Text(t)) if words.contains(&t.as_str()) => Ok(value.clone()),
        (Kind::NumberOr(words), _) => Err(bad(format!("a number or one of {}", list(words)))),
    }
}

fn list(words: &[&str]) -> String {
    words.iter().map(|w| format!("{w:?}")).collect::<Vec<_>>().join(", ")
}
