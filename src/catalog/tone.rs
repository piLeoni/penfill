//! The tone field: how dense a pattern is at each point, from 1 (full) down
//! to `1 - contrast`. A pen cannot draw grey, so tone is drawn by thinning:
//! scatter keeps fewer marks, hatch breaks lines and drops cross layers.

use super::schema::Args;
use crate::geo::{Pt, Region};
use crate::noise::Perlin;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Even,
    Edge,
    Centre,
    Noise,
    Gradient,
}

/// The density at each point of one fill.
pub struct Tone<'a> {
    shape: Shape,
    contrast: f64,
    feature: f64,
    region: &'a Region,
    noise: Perlin,
    dir: (f64, f64),
    span: (f64, f64),
}

impl<'a> Tone<'a> {
    /// The tone `args` ask for over `region`. Generators without the tone
    /// options get an even tone.
    pub fn new(args: &Args, region: &'a Region, seed: u64) -> Tone<'a> {
        let shape = match args.has("density").then(|| args.word("density")).unwrap_or("even") {
            "edge" => Shape::Edge,
            "centre" => Shape::Centre,
            "noise" => Shape::Noise,
            "gradient" => Shape::Gradient,
            _ => Shape::Even,
        };
        let contrast = if args.has("contrast") { args.num("contrast") } else { 0.0 };
        let feature = if args.has("feature") { args.num("feature").max(0.1) } else { args.spacing * 8.0 };
        let toward = if args.has("toward") { args.num("toward") } else { 90.0 };
        let dir = toward.to_radians().sin_cos();
        let dir = (dir.1, dir.0);
        let (x0, y0, x1, y1) = region.bounds_mm();
        let proj = |p: Pt| p.0 * dir.0 + p.1 * dir.1;
        let corners = [proj((x0, y0)), proj((x1, y0)), proj((x0, y1)), proj((x1, y1))];
        let lo = corners.iter().copied().fold(f64::MAX, f64::min);
        let hi = corners.iter().copied().fold(f64::MIN, f64::max);
        Tone { shape, contrast, feature, region, noise: Perlin::new(seed ^ 0x70e5), dir, span: (lo, hi) }
    }

    /// Whether the tone is the same everywhere (generators skip the work).
    pub fn is_even(&self) -> bool {
        self.shape == Shape::Even || self.contrast <= 0.0
    }

    /// The density at `p`, in `1 - contrast ..= 1`.
    pub fn at(&self, p: Pt) -> f64 {
        let full = match self.shape {
            Shape::Even => return 1.0,
            Shape::Edge => 1.0 - smoothstep(self.region.dist_to_boundary_within(p, self.feature) / self.feature),
            Shape::Centre => smoothstep(self.region.dist_to_boundary_within(p, self.feature) / self.feature),
            Shape::Noise => {
                let n = self.noise.fbm(p.0 / self.feature, p.1 / self.feature, 3);
                (0.5 + 0.85 * n).clamp(0.0, 1.0)
            }
            Shape::Gradient => {
                let s = p.0 * self.dir.0 + p.1 * self.dir.1;
                let w = (self.span.1 - self.span.0).max(1e-9);
                ((s - self.span.0) / w).clamp(0.0, 1.0)
            }
        };
        1.0 - self.contrast * (1.0 - full)
    }
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A well-spread threshold in `0..1` for lattice cell `(i, j)`: consecutive
/// cells get thresholds far apart (the R2 sequence), so thinning by tone
/// leaves an even scatter rather than holes and clumps.
pub fn rank(i: i64, j: i64, salt: u64) -> f64 {
    const A1: f64 = 0.754_877_666_246_692_7;
    const A2: f64 = 0.569_840_290_998_053_3;
    let h = crate::anchor::cell_seed(salt, i, j);
    let wobble = (h >> 11) as f64 / (1u64 << 53) as f64;
    (i as f64 * A1 + j as f64 * A2 + 0.2 * wobble).rem_euclid(1.0)
}
