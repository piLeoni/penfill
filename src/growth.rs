//! Space colonization: branching networks that grow to fill a shape from a
//! handful of roots — river deltas, veins, cracks, roots, lichen, mould.
//! Not a hatch and not an isoline fill: each stroke is one step of a branch
//! reaching toward open space, so the network thins out and leaves the
//! irregular gaps a hatch never would.
//!
//! Every root competes for the same field of attractor points (Bridson
//! Poisson, so they're evenly spread): each step, every surviving attractor
//! pulls the nearest branch tip within `reach` toward it; tips advance one
//! `step` in the averaged pull direction, and an attractor is consumed once
//! some tip gets within `kill` of it. Two tips pulled toward different
//! attractor clusters is how a branch forks — no explicit branching rule,
//! same as the real algorithm (Runions et al., *Modeling Trees with a Space
//! Colonization Algorithm*, 2007) this is a straight port of.

use crate::geo::{Pt, Region, Strokes};
use crate::random::{poisson_disk, Rng};
use std::collections::HashMap;

/// Where the branches start.
#[derive(Clone, Debug)]
pub enum RootSource {
    /// This many roots, spread out inside the region.
    Generate(usize),
    /// Exactly these positions (e.g. real trunk/stem locations).
    At(Vec<Pt>),
}

#[derive(Clone, Debug)]
pub struct GrowthParams {
    pub seed: u64,
    pub roots: RootSource,
    /// mm between attractor points: lower is a finer, more branched network.
    pub spacing: f64,
    /// mm a branch advances per step.
    pub step: f64,
    /// mm: an attractor beyond this from every tip pulls nothing — growth
    /// cannot reach across empty space on its own.
    pub reach: f64,
    /// mm: an attractor this close to a tip is consumed.
    pub kill: f64,
    /// 0 – 1ish: wobble added to each step's direction.
    pub jitter: f64,
    /// A hard cap on growth steps, so a pathological region cannot hang.
    pub max_steps: usize,
}

impl Default for GrowthParams {
    fn default() -> Self {
        GrowthParams {
            seed: 0,
            roots: RootSource::Generate(5),
            spacing: 3.0,
            step: 1.2,
            reach: 6.0,
            kill: 1.0,
            jitter: 0.15,
            max_steps: 20_000,
        }
    }
}

struct Node {
    at: Pt,
    parent: Option<usize>,
}

/// Points bucketed by `cell`-sized grid cells, queried by their 3×3
/// neighbourhood — exact whenever the query radius does not exceed `cell`.
struct Grid {
    cell: f64,
    buckets: HashMap<(i64, i64), Vec<usize>>,
}

impl Grid {
    fn new(cell: f64) -> Grid {
        Grid { cell: cell.max(1e-6), buckets: HashMap::new() }
    }

    fn key(&self, p: Pt) -> (i64, i64) {
        ((p.0 / self.cell).floor() as i64, (p.1 / self.cell).floor() as i64)
    }

    fn insert(&mut self, idx: usize, p: Pt) {
        self.buckets.entry(self.key(p)).or_default().push(idx);
    }

    /// The nearest `idx` to `p` within `radius` (must be `<= cell`), passing
    /// `alive(idx)` and at least `min_dist` from `p`; `at(idx)` its position.
    fn nearest(&self, p: Pt, radius: f64, at: impl Fn(usize) -> Pt, alive: impl Fn(usize) -> bool) -> Option<(usize, f64)> {
        let (ci, cj) = self.key(p);
        let r2 = radius * radius;
        let mut best: Option<(usize, f64)> = None;
        for i in ci - 1..=ci + 1 {
            for j in cj - 1..=cj + 1 {
                let Some(idx) = self.buckets.get(&(i, j)) else { continue };
                for &k in idx {
                    if !alive(k) {
                        continue;
                    }
                    let q = at(k);
                    let d2 = (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2);
                    if d2 <= r2 && best.is_none_or(|(_, bd2)| d2 < bd2) {
                        best = Some((k, d2));
                    }
                }
            }
        }
        best.map(|(k, d2)| (k, d2.sqrt()))
    }

    /// Every `idx` within `radius` of `p` (same `<= cell` caveat).
    fn within(&self, p: Pt, radius: f64, at: impl Fn(usize) -> Pt, alive: impl Fn(usize) -> bool) -> Vec<usize> {
        let (ci, cj) = self.key(p);
        let r2 = radius * radius;
        let mut out = Vec::new();
        for i in ci - 1..=ci + 1 {
            for j in cj - 1..=cj + 1 {
                let Some(idx) = self.buckets.get(&(i, j)) else { continue };
                for &k in idx {
                    if alive(k) {
                        let q = at(k);
                        if (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2) <= r2 {
                            out.push(k);
                        }
                    }
                }
            }
        }
        out
    }
}

/// Root positions spread inside `region`: Poisson spacing scaled so roughly
/// `n` survive, then capped to exactly `n` (fewer, if the region is small).
fn generate_roots(region: &Region, n: usize, seed: u64) -> Vec<Pt> {
    if n == 0 || region.is_empty() {
        return vec![];
    }
    let (x0, y0, x1, y1) = region.bounds_mm();
    let diag = (x1 - x0).hypot(y1 - y0).max(1e-6);
    let min_dist = diag / (n as f64).sqrt().max(1.0) * 0.7;
    let mut pts = poisson_disk(region, min_dist, seed);
    pts.truncate(n);
    pts
}

pub fn fill_growth(region: &Region, p: &GrowthParams) -> Strokes {
    if region.is_empty() || p.spacing <= 0.0 || p.step <= 0.0 || p.reach <= 0.0 || p.kill <= 0.0 {
        return vec![];
    }
    let roots = match &p.roots {
        RootSource::At(pts) => pts.clone(),
        RootSource::Generate(n) => generate_roots(region, *n, p.seed),
    };
    if roots.is_empty() {
        return vec![];
    }

    let mut rng = Rng::new(p.seed.wrapping_add(31));
    let mut nodes: Vec<Node> = roots.iter().map(|&at| Node { at, parent: None }).collect();
    let mut node_grid = Grid::new(p.reach);
    for (i, n) in nodes.iter().enumerate() {
        node_grid.insert(i, n.at);
    }

    let attractors = poisson_disk(region, p.spacing, p.seed.wrapping_add(7));
    let mut alive = vec![true; attractors.len()];
    let attractor_grid = {
        let mut g = Grid::new(p.reach.max(p.kill));
        for (i, &a) in attractors.iter().enumerate() {
            g.insert(i, a);
        }
        g
    };

    // A handful of attractors can end up in a stable orbit around a tip
    // (pulled every step, never quite landing within `kill`) instead of
    // being consumed or falling out of `reach` — a rare geometric fixed
    // point, not a growth error. Stop once the live count stalls for a
    // while rather than spin for the rest of `max_steps`.
    const STALL_LIMIT: u32 = 64;
    let mut stalled_for = 0u32;
    let mut last_alive_count = usize::MAX;

    for _ in 0..p.max_steps {
        let alive_count = alive.iter().filter(|&&a| a).count();
        if alive_count == 0 {
            break;
        }
        if alive_count == last_alive_count {
            stalled_for += 1;
            if stalled_for > STALL_LIMIT {
                break;
            }
        } else {
            stalled_for = 0;
            last_alive_count = alive_count;
        }
        // 1. Every live attractor pulls its nearest tip within reach.
        let mut pull: HashMap<usize, (f64, f64, u32)> = HashMap::new();
        for (i, &a) in attractors.iter().enumerate() {
            if !alive[i] {
                continue;
            }
            if let Some((node_idx, dist)) = node_grid.nearest(a, p.reach, |k| nodes[k].at, |_| true) {
                if dist > 1e-9 {
                    let at = nodes[node_idx].at;
                    let e = pull.entry(node_idx).or_insert((0.0, 0.0, 0));
                    e.0 += (a.0 - at.0) / dist;
                    e.1 += (a.1 - at.1) / dist;
                    e.2 += 1;
                }
            }
        }
        if pull.is_empty() {
            break;
        }
        // 2. Each pulled tip advances one step toward the average direction.
        let mut grown = Vec::new();
        for (&node_idx, &(sx, sy, _)) in &pull {
            let len = sx.hypot(sy);
            if len < 1e-9 {
                continue;
            }
            let (mut dx, mut dy) = (sx / len, sy / len);
            if p.jitter > 0.0 {
                let wobble = rng.range(-p.jitter, p.jitter) * std::f64::consts::FRAC_PI_4;
                let (s, c) = wobble.sin_cos();
                (dx, dy) = (dx * c - dy * s, dx * s + dy * c);
            }
            let from = nodes[node_idx].at;
            let at = (from.0 + p.step * dx, from.1 + p.step * dy);
            grown.push(Node { at, parent: Some(node_idx) });
        }
        if grown.is_empty() {
            break;
        }
        for n in grown {
            let at = n.at;
            nodes.push(n);
            node_grid.insert(nodes.len() - 1, at);
            // 3. Attractors a fresh tip reached are consumed on the spot.
            for i in attractor_grid.within(at, p.kill, |k| attractors[k], |k| alive[k]) {
                alive[i] = false;
            }
        }
    }

    let strokes: Strokes = nodes
        .iter()
        .filter_map(|n| n.parent.map(|p| vec![nodes[p].at, n.at]))
        .collect();
    region.clip_strokes(&strokes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc() -> Region {
        let n = 48;
        let ring: Vec<Pt> = (0..n)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / n as f64;
                (60.0 + 55.0 * a.cos(), 60.0 + 55.0 * a.sin())
            })
            .collect();
        Region::from_ring(&ring)
    }

    #[test]
    fn grows_a_branching_network_inside_the_region() {
        let host = disc();
        let p = GrowthParams { seed: 5, ..Default::default() };
        let out = fill_growth(&host, &p);
        assert!(out.len() > 200, "only {} segments", out.len());
        for seg in &out {
            for pt in seg {
                assert!(host.covers_point(*pt), "{pt:?} outside");
            }
        }
    }

    #[test]
    fn more_roots_grow_faster_within_a_step_budget() {
        // Within the same short budget, ten parallel fronts out-pace one.
        let host = disc();
        let budget = 12;
        let few = fill_growth(&host, &GrowthParams { seed: 9, roots: RootSource::Generate(1), max_steps: budget, ..Default::default() });
        let many = fill_growth(&host, &GrowthParams { seed: 9, roots: RootSource::Generate(10), max_steps: budget, ..Default::default() });
        assert!(many.len() > few.len(), "{} roots vs {} roots", many.len(), few.len());
    }

    #[test]
    fn explicit_roots_are_used_as_given() {
        let host = disc();
        let at = vec![(40.0, 40.0), (80.0, 80.0)];
        let out = fill_growth(&host, &GrowthParams { seed: 1, roots: RootSource::At(at.clone()), ..Default::default() });
        assert!(!out.is_empty());
        // Every root that grew left a segment starting exactly at it.
        for r in &at {
            assert!(out.iter().any(|s| dist(s[0], *r) < 1e-9), "root {r:?} never grew");
        }
    }

    #[test]
    fn empty_region_draws_nothing() {
        assert!(fill_growth(&Region::default(), &GrowthParams::default()).is_empty());
    }

    #[test]
    fn fewer_steps_grow_less() {
        let host = disc();
        let capped = fill_growth(&host, &GrowthParams { seed: 4, max_steps: 5, ..Default::default() });
        let full = fill_growth(&host, &GrowthParams { seed: 4, ..Default::default() });
        assert!(full.len() > capped.len(), "full {} vs capped {}", full.len(), capped.len());
    }

    fn dist(a: Pt, b: Pt) -> f64 {
        (a.0 - b.0).hypot(a.1 - b.1)
    }
}
