//! Anchored placement: the points a scattering generator puts marks at.
//!
//! Every arrangement starts from a lattice placed by the anchor, and every
//! random choice comes from the lattice cell's own seed. So with a global
//! anchor two areas that share an edge get one continuous scatter, and a card
//! moved over the map keeps its marks where they were on the ground — which a
//! Poisson disk seeded per region cannot do.

use crate::geo::{Pt, Region};
use crate::pattern::Tools;
use crate::random::Rng;

use super::tone::rank;

/// How the points are arranged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arrange {
    /// Even but irregular, like a stipple by hand: a hexagonal lattice, each
    /// point moved within its cell.
    Blue,
    Hex,
    Square,
    /// Rows along the pattern's angle, points wandering along their row only
    /// (an orchard, a vineyard, bedding).
    Rows,
    /// Small groups round scattered centres.
    Clusters,
}

impl Arrange {
    pub fn parse(word: &str) -> Arrange {
        match word {
            "hex" => Arrange::Hex,
            "square" => Arrange::Square,
            "rows" => Arrange::Rows,
            "clusters" => Arrange::Clusters,
            _ => Arrange::Blue,
        }
    }
}

/// One place for a mark.
pub struct Site {
    pub at: Pt,
    /// A threshold in `0..1`, well spread over neighbouring sites: a site is
    /// kept where the tone is at least its rank.
    pub rank: f64,
    /// This site's own randomness (size, turn, variant).
    pub rng: Rng,
}

/// Sites `pitch` apart over `within` (usually the tools' placement field),
/// arranged as asked, turned by `angle` degrees.
pub fn sites(tools: &Tools, seed: u64, within: &Region, pitch: f64, arrange: Arrange, jitter: f64, angle: f64) -> Vec<Site> {
    let mut out = Vec::new();
    if pitch <= 0.0 || within.is_empty() {
        return out;
    }
    let hex_dy = pitch * 3f64.sqrt() / 2.0;
    let (dx, dy, shift) = match arrange {
        Arrange::Blue | Arrange::Hex => (pitch, hex_dy, pitch / 2.0),
        Arrange::Square | Arrange::Rows => (pitch, pitch, 0.0),
        Arrange::Clusters => (pitch * 3.2, pitch * 3.2 * 3f64.sqrt() / 2.0, pitch * 1.6),
    };
    let salt = seed ^ 0x5173;
    let cover = within.cutter();
    tools.grid_cells(dx, dy, angle, shift, pitch * 2.0, |frame, rng, i, j, (u, v)| {
        let push = |out: &mut Vec<Site>, uv: Pt, r: f64, site_rng: Rng| {
            let p = frame.world(uv);
            if cover.inside(p) {
                out.push(Site { at: p, rank: r, rng: site_rng });
            }
        };
        match arrange {
            Arrange::Blue => {
                // 0.42 of a cell keeps neighbours apart (no clumps) and breaks
                // the lattice for the eye; jitter loosens it further.
                let w = 0.42 + 0.4 * jitter;
                let (ju, jv) = (rng.range(-w, w) * dx, rng.range(-w, w) * dy);
                let r = rank(i, j, salt);
                push(&mut out, (u + ju, v + jv), r, Rng::new(rng.next_u64()));
            }
            Arrange::Hex | Arrange::Square => {
                let w = 0.5 * jitter;
                let (ju, jv) = (rng.range(-w, w) * dx, rng.range(-w, w) * dy);
                let r = rank(i, j, salt);
                push(&mut out, (u + ju, v + jv), r, Rng::new(rng.next_u64()));
            }
            Arrange::Rows => {
                let ju = rng.range(-0.5, 0.5) * jitter * dx;
                let jv = rng.range(-0.08, 0.08) * jitter * dy;
                let r = rank(i, j, salt);
                push(&mut out, (u + ju, v + jv), r, Rng::new(rng.next_u64()));
            }
            Arrange::Clusters => {
                // Not every cell holds a cluster; the ones that do hold 3–7
                // points round a centre moved within the cell.
                if rng.unit() < 0.25 {
                    return;
                }
                let cu = u + rng.range(-0.3, 0.3) * dx;
                let cv = v + rng.range(-0.3, 0.3) * dy;
                let n = 3 + (rng.unit() * 5.0) as usize;
                let spread = pitch * (0.7 + 0.5 * jitter);
                for k in 0..n {
                    let a = rng.range(0.0, std::f64::consts::TAU);
                    let d = spread * rng.unit().sqrt();
                    let r = rank(i * 8 + k as i64, j, salt);
                    push(&mut out, (cu + d * a.cos(), cv + d * a.sin()), r, Rng::new(rng.next_u64()));
                }
            }
        }
    });
    out
}
