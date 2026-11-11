//! # penfill
//!
//! Infill / traversal pattern generation for pen plotters, CNC toolpaths and
//! laser engravers — the algorithms a 3D-print slicer uses for infill, applied
//! to ink.
//!
//! Units are **millimetres** everywhere. A fill takes a [`Region`] (mm) and a
//! spacing (mm) and returns open polylines in mm, in draw order.

pub mod anchor;
pub mod catalog;
pub mod chess;
pub mod clouds;
pub mod concentric;
pub mod dither;
pub mod doodle;
pub mod dotswaves;
pub mod field;
pub mod fray;
pub mod geo;
pub mod growth;
pub mod hand;
pub mod hilbert;
pub mod isoline_blobs;
pub mod isoline_stack;
pub mod gravel;
pub mod grid;
pub mod honeycomb;
pub mod nature;
pub mod noise;
pub mod pat;
pub mod pattern;
pub mod placement;
pub mod radial;
pub mod random;
pub mod rectilinear;
pub mod scales;
pub mod scribble;
pub mod solid;
pub mod spiral;
pub mod stipple;
pub mod stream;
pub mod svg;
pub mod sweep;
pub mod terrazzo;
pub mod textures;
pub mod tile;
pub mod truchet;
pub mod waves_curl;
pub mod waves_flow;
pub mod waves_scallop;
pub mod zigzag;

pub use anchor::{Anchor, Frame, Origin};
pub use geo::{polyline_length, Region, Strokes};
pub use hand::{hand, HandParams};
pub use isoline_blobs::{
    fill_isoline_blobs, fill_isoline_blobs_with_coverage, FieldSite, FieldSource, FillWithCoverage, IsolineFieldParams,
};
pub use pattern::{
    all_fill_names, fill_by_name, fill_output_by_name, fill_pattern, fill_pattern_output,
    fill_pattern_with, Canvas, CustomParams, EdgeMode, FillOutput, FillParams, Tools,
};
pub use solid::fill_solid;
pub use stream::{Clipped, Frayed, Meter, Sink};
