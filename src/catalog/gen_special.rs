//! The specialists: generators with a look of their own (placeholders until wrapped).

use super::schema::Args;
use super::Generator;
use crate::geo::Strokes;
use crate::pattern::{Canvas, Tools};

fn nothing(_c: &Canvas, _t: &Tools, _a: &Args) -> Strokes {
    Strokes::new()
}

pub static RINGS: Generator = Generator { name: "rings", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static SCRIBBLE: Generator = Generator { name: "scribble", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static HILBERT: Generator = Generator { name: "hilbert", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static TRUCHET: Generator = Generator { name: "truchet", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static HONEYCOMB: Generator = Generator { name: "honeycomb", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static SCALES: Generator = Generator { name: "scales", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static SUNBURST: Generator = Generator { name: "sunburst", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static WOOD: Generator = Generator { name: "wood", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static MARBLE: Generator = Generator { name: "marble", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static SWELLS: Generator = Generator { name: "swells", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static EDDIES: Generator = Generator { name: "eddies", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static RIPPLES: Generator = Generator { name: "ripples", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static CANOPY: Generator = Generator { name: "canopy", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static TWIGS: Generator = Generator { name: "twigs", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static DOODLE_TREES: Generator = Generator { name: "doodle-trees", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static GROWTH: Generator = Generator { name: "growth", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static ISOLINE_STACK: Generator = Generator { name: "isoline-stack", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static ISOLINE_BLOBS: Generator = Generator { name: "isoline-blobs", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static CLOUDS: Generator = Generator { name: "clouds", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };

pub static SOLID: Generator = Generator { name: "solid", about: "(not written yet)", spacing: 1.0, angle: 0.0, options: &[], own_mask: false, draw: nothing };
