//! Option values a host sends with a fill: numbers, flags, words, pairs and
//! polylines. Hosts that speak JSON (draw.land, the wasm and Node bindings)
//! map their values onto [`Value`] once; the catalogue checks them against
//! each generator's schema.

use std::collections::BTreeMap;

use crate::geo::Strokes;

/// One option's value, as a host sends it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Flag(bool),
    Text(String),
    /// A list of numbers: a pair of lengths (`dash: [0.8, 0.4]`), a size.
    Nums(Vec<f64>),
    /// Polylines in mm: a custom motif or tile.
    Lines(Strokes),
}

impl Value {
    /// What the value is, in a word, for error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Num(_) => "a number",
            Value::Flag(_) => "true or false",
            Value::Text(_) => "a word",
            Value::Nums(_) => "a list of numbers",
            Value::Lines(_) => "polylines",
        }
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Value {
        Value::Num(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Flag(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Value {
        Value::Text(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Value {
        Value::Text(v)
    }
}

impl From<Strokes> for Value {
    fn from(v: Strokes) -> Value {
        Value::Lines(v)
    }
}

/// A fill's options by name, beyond the standard [`crate::pattern::FillParams`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options(BTreeMap<String, Value>);

impl Options {
    pub fn new() -> Options {
        Options::default()
    }

    /// `self` with `key` set.
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Options {
        self.set(key, value);
        self
    }

    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.0.insert(key.to_string(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.0.remove(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<K: Into<String>, V: Into<Value>> FromIterator<(K, V)> for Options {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Options {
        Options(iter.into_iter().map(|(k, v)| (k.into(), v.into())).collect())
    }
}
