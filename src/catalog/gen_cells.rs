//! `cells`: placeholder until the generator lands.

use super::schema::Args;
use super::Generator;
use crate::geo::Strokes;
use crate::pattern::{Canvas, Tools};

pub static CELLS: Generator = Generator {
    name: "cells",
    about: "(not written yet)",
    spacing: 1.0,
    angle: 0.0,
    options: &[],
    own_mask: false,
    draw,
};

fn draw(_c: &Canvas, _t: &Tools, _a: &Args) -> Strokes {
    Strokes::new()
}
