//! Space colonization: branching networks that grow to fill a shape from a
//! handful of roots — river deltas, veins, cracks, roots, lichen, mould.
//! Each root grows a basin of fine, evenly spaced twigs that merge into
//! thicker and thicker trunks back towards it; neighbouring basins meet
//! along thin white seams.
//!
//! Every root competes for the same field of attractor points (Bridson
//! Poisson, so they're evenly spread): each step, every surviving attractor
//! pulls its nearest node within `reach`; pulled nodes grow one `step`
//! along the averaged pull, and an attractor is consumed once a node lands
//! within `kill` of it. Two groups of attractors pulling one node apart is
//! how a branch forks — no explicit branching rule (Runions et al.,
//! *Modeling Trees with a Space Colonization Algorithm*, 2007).
//!
//! Output is one polyline per branch (the trunk keeps following its
//! heaviest child), smoothed, and thickened for a pen of one width: a
//! branch carrying many twigs gets extra parallel passes, tapering into
//! the centre line, so trunks read dark and twigs hairline.

use crate::geo::{Pt, Region, Strokes};
use crate::random::{poisson_disk, Rng};
use std::collections::HashMap;

/// Where the branches start.
#[derive(Clone, Debug)]
pub enum RootSource {
    /// This many roots, spread out inside the region.
    Generate(usize),
    /// About one root per this many mm apart (Poisson), and at least one in
    /// every separate part of the region.
    Every(f64),
    /// Exactly these positions (e.g. real trunk/stem locations).
    At(Vec<Pt>),
}

#[derive(Clone, Debug)]
pub struct GrowthParams {
    pub seed: u64,
    pub roots: RootSource,
    /// mm between attractors; neighbouring twigs end up about 2–2.5× this
    /// apart.
    pub spacing: f64,
    /// mm a branch advances per step.
    pub step: f64,
    /// mm: attractors farther than this from every node pull nothing.
    pub reach: f64,
    /// mm: an attractor this close to a node is consumed.
    pub kill: f64,
    /// 0 – 1ish: wobble added to each step's direction.
    pub jitter: f64,
    /// A hard cap on growth steps.
    pub max_steps: usize,
    /// mm between the parallel passes that thicken a trunk; 0 draws every
    /// branch as a single line.
    pub pen: f64,
    /// Most extra passes on each side of the thickest trunk.
    pub max_passes: usize,
    /// Twigs a branch must carry per extra pass: `k` passes a side from
    /// `thicken · k²` twigs.
    pub thicken: f64,
    /// Chaikin rounds on every branch: 0 keeps the raw steps.
    pub smooth: usize,
}

impl Default for GrowthParams {
    fn default() -> Self {
        GrowthParams {
            seed: 0,
            roots: RootSource::Every(30.0),
            spacing: 1.0,
            step: 0.4,
            reach: 8.0,
            kill: 1.0,
            jitter: 0.1,
            max_steps: 20_000,
            pen: 0.15,
            max_passes: 3,
            thicken: 15.0,
            smooth: 2,
        }
    }
}

const NONE: usize = usize::MAX;
/// Most branches leaving one node.
const MAX_KIDS: u8 = 3;

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

    /// Calls `f(idx)` for every point in the 3×3 cells around `p`.
    fn near(&self, p: Pt, mut f: impl FnMut(usize)) {
        let (ci, cj) = self.key(p);
        for i in ci - 1..=ci + 1 {
            for j in cj - 1..=cj + 1 {
                if let Some(idx) = self.buckets.get(&(i, j)) {
                    idx.iter().for_each(|&k| f(k));
                }
            }
        }
    }
}

/// The growing network and the attractors it is growing towards.
struct Tree {
    attractors: Vec<Pt>,
    alive: Vec<bool>,
    /// Each attractor's nearest node so far: nodes are never removed, so a
    /// new node only has to be compared with the attractors around it.
    best: Vec<(usize, f64)>,
    attractor_grid: Grid,
    at: Vec<Pt>,
    parent: Vec<usize>,
    /// Children per node: capped, so a root forks instead of fanning out.
    kids: Vec<u8>,
    node_grid: Grid,
    reach2: f64,
    kill2: f64,
    too_close2: f64,
}

impl Tree {
    fn new(attractors: Vec<Pt>, p: &GrowthParams) -> Tree {
        let mut attractor_grid = Grid::new(p.reach.max(p.kill));
        for (i, &a) in attractors.iter().enumerate() {
            attractor_grid.insert(i, a);
        }
        let too_close = p.step * 0.3;
        Tree {
            alive: vec![true; attractors.len()],
            best: vec![(NONE, f64::MAX); attractors.len()],
            attractors,
            attractor_grid,
            at: Vec::new(),
            parent: Vec::new(),
            kids: Vec::new(),
            node_grid: Grid::new(too_close),
            reach2: p.reach * p.reach,
            kill2: p.kill * p.kill,
            too_close2: too_close * too_close,
        }
    }

    fn add(&mut self, q: Pt, from: usize) {
        let idx = self.at.len();
        self.at.push(q);
        self.parent.push(from);
        self.kids.push(0);
        if from != NONE {
            self.kids[from] += 1;
        }
        self.node_grid.insert(idx, q);
        let Tree { attractors, alive, best, attractor_grid, reach2, kill2, .. } = self;
        attractor_grid.near(q, |k| {
            if !alive[k] {
                return;
            }
            let d = d2(q, attractors[k]);
            if d <= *kill2 {
                alive[k] = false;
            } else if d <= *reach2 && d < best[k].1 {
                best[k] = (idx, d);
            }
        });
    }

    /// Whether `q` lands on top of an existing node.
    fn crowded(&self, q: Pt) -> bool {
        let mut hit = false;
        self.node_grid.near(q, |m| hit |= d2(q, self.at[m]) < self.too_close2);
        hit
    }
}

fn d2(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

fn generate_roots(region: &Region, source: &RootSource, seed: u64) -> Vec<Pt> {
    match source {
        RootSource::At(pts) => pts.iter().copied().filter(|p| region.covers_point(*p)).collect(),
        RootSource::Generate(n) => {
            if *n == 0 {
                return vec![];
            }
            let (x0, y0, x1, y1) = region.bounds_mm();
            let diag = (x1 - x0).hypot(y1 - y0).max(1e-6);
            let mut pts = poisson_disk(region, diag / (*n as f64).sqrt() * 0.7, seed);
            pts.truncate(*n);
            pts
        }
        RootSource::Every(d) => {
            let mut pts = poisson_disk(region, d.max(1e-3), seed);
            // Woods smaller than the root spacing still get one root each.
            for part in region.parts() {
                if !pts.iter().any(|p| part.covers_point(*p)) {
                    let (x0, y0, x1, y1) = part.bounds_mm();
                    let small = ((x1 - x0).min(y1 - y0) * 0.25).max(1e-3);
                    if let Some(p) = poisson_disk(&part, small, seed).into_iter().next() {
                        pts.push(p);
                    }
                }
            }
            pts
        }
    }
}

pub fn fill_growth(region: &Region, p: &GrowthParams) -> Strokes {
    if region.is_empty() || p.spacing <= 0.0 || p.step <= 0.0 || p.reach <= 0.0 || p.kill <= 0.0 {
        return vec![];
    }
    let roots = generate_roots(region, &p.roots, p.seed);
    if roots.is_empty() {
        return vec![];
    }
    let mut tree = Tree::new(poisson_disk(region, p.spacing, p.seed.wrapping_add(7)), p);
    for &r in &roots {
        tree.add(r, NONE);
    }

    let mut rng = Rng::new(p.seed.wrapping_add(31));
    let mut live: Vec<usize> = (0..tree.attractors.len()).collect();
    let mut pull: Vec<(f64, f64)> = Vec::new();
    let mut pulled: Vec<usize> = Vec::new();
    for _ in 0..p.max_steps {
        live.retain(|&k| tree.alive[k]);
        pull.resize(tree.at.len(), (0.0, 0.0));
        pulled.clear();
        for &k in &live {
            let (n, d) = tree.best[k];
            if n == NONE {
                continue;
            }
            let d = d.sqrt();
            let e = &mut pull[n];
            if *e == (0.0, 0.0) {
                pulled.push(n);
            }
            e.0 += (tree.attractors[k].0 - tree.at[n].0) / d;
            e.1 += (tree.attractors[k].1 - tree.at[n].1) / d;
        }
        if pulled.is_empty() {
            break;
        }
        pulled.sort_unstable();
        let mut stuck: Vec<usize> = Vec::new();
        let mut grew = false;
        for &n in &pulled {
            let (sx, sy) = std::mem::take(&mut pull[n]);
            if tree.kids[n] >= MAX_KIDS {
                continue;
            }
            let len = sx.hypot(sy);
            if len < 1e-9 {
                stuck.push(n);
                continue;
            }
            let (mut dx, mut dy) = (sx / len, sy / len);
            if p.jitter > 0.0 {
                let (s, c) = (rng.range(-p.jitter, p.jitter) * std::f64::consts::FRAC_PI_4).sin_cos();
                (dx, dy) = (dx * c - dy * s, dx * s + dy * c);
            }
            let q = (tree.at[n].0 + p.step * dx, tree.at[n].1 + p.step * dy);
            if tree.crowded(q) {
                stuck.push(n);
            } else {
                tree.add(q, n);
                grew = true;
            }
        }
        // Pulled apart evenly, or growing back onto an existing node: the
        // attractors holding it can never be reached, so drop them instead
        // of growing the same twig forever.
        if !stuck.is_empty() {
            stuck.sort_unstable();
            for &k in &live {
                if stuck.binary_search(&tree.best[k].0).is_ok() {
                    tree.alive[k] = false;
                }
            }
        } else if !grew {
            break;
        }
    }

    let Tree { at, parent, .. } = tree;
    let paths = branches(&parent);
    let leaves = leaf_counts(&parent);
    let passes = |n: usize| -> usize {
        if p.pen <= 0.0 || p.thicken <= 0.0 {
            return 0;
        }
        ((leaves[n] as f64 / p.thicken).sqrt().floor() as usize).min(p.max_passes)
    };
    let mut strokes: Strokes = Vec::new();
    for (path, from_junction) in &paths {
        let pts: Vec<Pt> = path.iter().map(|&n| at[n]).collect();
        let mut side: Vec<usize> = path.iter().map(|&n| passes(n)).collect();
        // A side branch starts on its parent's trunk; it is as thick as itself.
        if *from_junction {
            side[0] = side[1];
        }
        strokes.push(chaikin(&pts, p.smooth));
        for j in 1..=p.max_passes {
            let m = side.iter().take_while(|&&s| s >= j).count();
            if m < 3 {
                break;
            }
            for sign in [-1.0, 1.0] {
                let off: Vec<Pt> = (0..m)
                    .map(|k| {
                        let (a, b) = (pts[k.saturating_sub(1)], pts[(k + 1).min(pts.len() - 1)]);
                        let (tx, ty) = (b.0 - a.0, b.1 - a.1);
                        let tl = tx.hypot(ty).max(1e-9);
                        // Fold the pass into the centre over its last few nodes.
                        let taper = ((m - 1 - k) as f64 / 3.0).min(1.0);
                        let o = sign * j as f64 * p.pen * taper;
                        (pts[k].0 - ty / tl * o, pts[k].1 + tx / tl * o)
                    })
                    .collect();
                strokes.push(chaikin(&off, p.smooth));
            }
        }
    }
    region.clip_strokes(&strokes)
}

/// The tree as polylines of node indices: each path follows its heaviest
/// child; every other child starts a path at its parent's node (flagged).
fn branches(parent: &[usize]) -> Vec<(Vec<usize>, bool)> {
    let leaves = leaf_counts(parent);
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); parent.len()];
    for (i, &p) in parent.iter().enumerate() {
        if p != NONE {
            children[p].push(i);
        }
    }
    let mut stack: Vec<usize> = (0..parent.len()).filter(|&i| parent[i] == NONE).collect();
    stack.reverse();
    let mut out = Vec::new();
    while let Some(start) = stack.pop() {
        let mut path = Vec::new();
        if parent[start] != NONE {
            path.push(parent[start]);
        }
        let mut cur = start;
        loop {
            path.push(cur);
            let kids = &children[cur];
            let Some(&heavy) = kids.iter().max_by_key(|&&c| (leaves[c], std::cmp::Reverse(c))) else { break };
            for &c in kids.iter().rev() {
                if c != heavy {
                    stack.push(c);
                }
            }
            cur = heavy;
        }
        // A twig of one or two steps off a passing branch reads as a burr,
        // not a branch.
        let spur = parent[start] != NONE && path.len() <= 3;
        if path.len() >= 2 && !spur {
            out.push((path, parent[start] != NONE));
        }
    }
    out
}

/// Twigs (leaf nodes) under every node, itself included when it is one.
fn leaf_counts(parent: &[usize]) -> Vec<u32> {
    let mut has_child = vec![false; parent.len()];
    for &p in parent {
        if p != NONE {
            has_child[p] = true;
        }
    }
    let mut leaves: Vec<u32> = has_child.iter().map(|&c| u32::from(!c)).collect();
    // Children always come after their parent.
    for i in (0..parent.len()).rev() {
        if parent[i] != NONE {
            leaves[parent[i]] += leaves[i];
        }
    }
    leaves
}

/// Corner cutting, ends kept where they are.
fn chaikin(pts: &[Pt], rounds: usize) -> Vec<Pt> {
    let mut cur = pts.to_vec();
    for _ in 0..rounds {
        if cur.len() < 3 {
            break;
        }
        let mut next = Vec::with_capacity(cur.len() * 2);
        next.push(cur[0]);
        for w in cur.windows(2) {
            let (a, b) = (w[0], w[1]);
            next.push((0.75 * a.0 + 0.25 * b.0, 0.75 * a.1 + 0.25 * b.1));
            next.push((0.25 * a.0 + 0.75 * b.0, 0.25 * a.1 + 0.75 * b.1));
        }
        next.push(*cur.last().unwrap());
        cur = next;
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc() -> Region {
        let n = 64;
        let ring: Vec<Pt> = (0..n)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / n as f64;
                (30.0 + 28.0 * a.cos(), 30.0 + 28.0 * a.sin())
            })
            .collect();
        Region::from_ring(&ring)
    }

    fn ink(s: &Strokes) -> f64 {
        s.iter().flat_map(|l| l.windows(2)).map(|w| d2(w[0], w[1]).sqrt()).sum()
    }

    #[test]
    fn fills_the_region_densely_and_stays_inside() {
        let host = disc();
        let out = fill_growth(&host, &GrowthParams { seed: 5, roots: RootSource::Generate(4), ..Default::default() });
        for pt in out.iter().flatten() {
            assert!(host.covers_point(*pt), "{pt:?} outside");
        }
        // Twigs ~2.5 `spacing` apart, plus trunk passes.
        let area = std::f64::consts::PI * 28.0 * 28.0;
        assert!(ink(&out) > 0.35 * area, "only {:.0} mm of ink over {area:.0} mm²", ink(&out));
    }

    #[test]
    fn branches_are_polylines_not_segments() {
        let out = fill_growth(&disc(), &GrowthParams { seed: 2, roots: RootSource::Generate(3), pen: 0.0, ..Default::default() });
        let points: usize = out.iter().map(|s| s.len()).sum();
        assert!(points > out.len() * 6, "{} strokes for {points} points", out.len());
    }

    #[test]
    fn trunks_get_extra_passes() {
        let thin = fill_growth(&disc(), &GrowthParams { seed: 2, roots: RootSource::Generate(3), pen: 0.0, ..Default::default() });
        let thick = fill_growth(&disc(), &GrowthParams { seed: 2, roots: RootSource::Generate(3), ..Default::default() });
        assert!(thick.len() > thin.len() + 6, "{} vs {}", thick.len(), thin.len());
    }

    #[test]
    fn explicit_roots_are_used_as_given() {
        let at = vec![(20.0, 20.0), (40.0, 40.0)];
        let out = fill_growth(&disc(), &GrowthParams { seed: 1, roots: RootSource::At(at.clone()), pen: 0.0, smooth: 0, ..Default::default() });
        for r in &at {
            assert!(out.iter().any(|s| d2(s[0], *r) < 1e-9), "root {r:?} never grew");
        }
    }

    #[test]
    fn every_part_gets_a_root() {
        let two = Region::from_polygons(&[
            vec![vec![(0.0, 0.0), (6.0, 0.0), (6.0, 6.0), (0.0, 6.0)]],
            vec![vec![(50.0, 0.0), (56.0, 0.0), (56.0, 6.0), (50.0, 6.0)]],
        ]);
        let out = fill_growth(&two, &GrowthParams { seed: 3, roots: RootSource::Every(100.0), ..Default::default() });
        assert!(out.iter().flatten().any(|p| p.0 < 10.0));
        assert!(out.iter().flatten().any(|p| p.0 > 45.0));
    }

    #[test]
    fn empty_region_draws_nothing() {
        assert!(fill_growth(&Region::default(), &GrowthParams::default()).is_empty());
    }
}
