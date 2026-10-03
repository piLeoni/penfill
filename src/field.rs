//! Fills from your own maths.
//!
//! - [`fill_isolines`] draws the contour lines of any function `f(x, y)` —
//!   like the height lines of a map. Marble, wood grain and cracked ground
//!   are all built on it (see `textures`).
//! - [`fill_profile`] draws parallel lines bent by a function `y = g(x)` —
//!   waves, zigzags, sawtooth, anything periodic or not.
//!
//! Both clip to the region like every other fill, holes included.

use crate::anchor::{centre, indices, Anchor, Frame};
use crate::geo::{Pt, Region, Strokes};
use std::collections::HashMap;

/// Contour lines of `f` at the given `levels`, sampled on a grid of
/// `cell` mm (smaller = smoother curves, slower). Lines are chained into
/// long polylines; closed contours come back closed.
pub fn fill_isolines(region: &Region, cell: f64, levels: &[f64], f: impl Fn(Pt) -> f64) -> Strokes {
    if cell <= 0.0 || region.is_empty() || levels.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let (x0, y0) = (minx - cell, miny - cell);
    let nx = ((maxx - minx) / cell).ceil() as usize + 3;
    let ny = ((maxy - miny) / cell).ceil() as usize + 3;
    let at = |i: usize, j: usize| (x0 + i as f64 * cell, y0 + j as f64 * cell);
    let v: Vec<f64> = (0..ny).flat_map(|j| (0..nx).map(move |i| (i, j))).map(|(i, j)| f(at(i, j))).collect();
    let val = |i: usize, j: usize| v[j * nx + i];

    let mut sorted: Vec<f64> = levels.iter().copied().filter(|l| l.is_finite()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted.dedup();
    // Crossing points keyed by grid edge: (i, j, 0) = edge to the right
    // of node (i, j), (i, j, 1) = edge above it. One list per level; each
    // cell only visits the levels between its lowest and highest corner.
    let mut per_level: Vec<Vec<[(usize, usize, u8); 2]>> = vec![Vec::new(); sorted.len()];
    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            let (a, b, c, d) = (val(i, j), val(i + 1, j), val(i + 1, j + 1), val(i, j + 1));
            let lo = a.min(b).min(c).min(d);
            let hi = a.max(b).max(c).max(d);
            let first = sorted.partition_point(|&l| l < lo);
            for (li, &level) in sorted.iter().enumerate().skip(first) {
                if level >= hi {
                    break;
                }
                let segs = &mut per_level[li];
                let case = (a > level) as u8 | ((b > level) as u8) << 1 | ((c > level) as u8) << 2 | ((d > level) as u8) << 3;
                let (bottom, right, top, left) = ((i, j, 0), (i + 1, j, 1), (i, j + 1, 0), (i, j, 1));
                let centre_up = (a + b + c + d) / 4.0 > level;
                let mut push = |p, q| segs.push([p, q]);
                match case {
                    0 | 15 => {}
                    1 | 14 => push(left, bottom),
                    2 | 13 => push(bottom, right),
                    3 | 12 => push(left, right),
                    4 | 11 => push(right, top),
                    6 | 9 => push(bottom, top),
                    7 | 8 => push(left, top),
                    5 if centre_up => {
                        push(left, top);
                        push(bottom, right);
                    }
                    5 => {
                        push(left, bottom);
                        push(right, top);
                    }
                    10 if centre_up => {
                        push(left, bottom);
                        push(right, top);
                    }
                    _ => {
                        push(left, top);
                        push(bottom, right);
                    }
                }
            }
        }
    }
    let mut out: Strokes = Vec::new();
    for (segs, &level) in per_level.iter().zip(&sorted) {
        let point = |(i, j, dir): (usize, usize, u8)| {
            let (p, q) = if dir == 0 { ((i, j), (i + 1, j)) } else { ((i, j), (i, j + 1)) };
            let (va, vb) = (val(p.0, p.1), val(q.0, q.1));
            let t = if (vb - va).abs() < 1e-12 { 0.5 } else { ((level - va) / (vb - va)).clamp(0.0, 1.0) };
            let (a, b) = (at(p.0, p.1), at(q.0, q.1));
            (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
        };
        for chain in chain_segments(segs) {
            let pts: Vec<Pt> = chain.into_iter().map(point).collect();
            let pts = simplify(&pts, cell * 0.05);
            out.extend(region.clip_polyline(&pts));
        }
    }
    out
}

/// `levels` from `lo` to `hi` every `step`.
pub fn levels_every(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    if step <= 0.0 || hi < lo {
        return vec![];
    }
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    (first..=last).map(|k| k as f64 * step).collect()
}

/// Parallel lines `spacing` mm apart at `angle`, each one bent by
/// `profile(x)` (mm across the line for x mm along it). `profile = |_| 0`
/// is plain hatching; `|x| (x * 2.0).sin() * 0.5` is waves.
pub fn fill_profile(
    region: &Region,
    spacing: f64,
    angle: f64,
    anchor: Anchor,
    profile: impl Fn(f64) -> f64,
) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let step = (spacing / 8.0).clamp(0.05, 0.5);
    let (u0, u1, v0, v1) = frame.span(region, spacing);
    let samples = indices(u0, u1, step);
    let mut out: Strokes = Vec::new();
    for k in indices(v0, v1, spacing) {
        let y = k as f64 * spacing;
        let pts: Vec<Pt> = samples
            .clone()
            .map(|s| {
                let x = s as f64 * step;
                frame.world((x, y + profile(x)))
            })
            .collect();
        out.extend(region.clip_polyline(&simplify(&pts, 0.01)));
    }
    out
}

/// Join segments sharing grid edges into polylines (each grid edge is
/// shared by at most two segments of one level).
fn chain_segments<K: Copy + Eq + std::hash::Hash>(segs: &[[K; 2]]) -> Vec<Vec<K>> {
    let mut by_key: HashMap<K, Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        by_key.entry(s[0]).or_default().push(i);
        by_key.entry(s[1]).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut chain = vec![segs[start][0], segs[start][1]];
        for _ in 0..2 {
            loop {
                let tail = *chain.last().unwrap();
                let Some(k) = by_key[&tail].iter().copied().find(|&k| !used[k]) else { break };
                used[k] = true;
                chain.push(if segs[k][0] == tail { segs[k][1] } else { segs[k][0] });
            }
            chain.reverse();
        }
        out.push(chain);
    }
    out
}

/// Ramer–Douglas–Peucker: drop points closer than `tol` to the line
/// through their neighbours. Keeps files small without visible change.
pub(crate) fn simplify(pts: &[Pt], tol: f64) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (pts[a], pts[b]);
        let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
        let len = dx.hypot(dy);
        let mut best = (0.0, a);
        for (i, p) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = if len < 1e-12 {
                (p.0 - pa.0).hypot(p.1 - pa.1)
            } else {
                ((p.0 - pa.0) * dy - (p.1 - pa.1) * dx).abs() / len
            };
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tol {
            keep[best.1] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)])
    }

    #[test]
    fn circles_come_back_as_closed_rings() {
        let f = |p: Pt| (p.0 - 15.0).hypot(p.1 - 15.0);
        let rings = fill_isolines(&square(), 0.25, &[4.0, 8.0, 12.0], f);
        assert_eq!(rings.len(), 3, "one ring per level");
        for r in &rings {
            assert_eq!(r.first(), r.last(), "ring not closed");
            let rad = f(r[0]);
            for p in r {
                assert!((f(*p) - rad).abs() < 0.05, "ring wobbles: {} vs {rad}", f(*p));
            }
        }
    }

    #[test]
    fn isolines_are_clipped_to_the_region() {
        let f = |p: Pt| p.0 + p.1;
        let lines = fill_isolines(&square(), 0.5, &levels_every(0.0, 60.0, 3.0), f);
        assert!(lines.len() >= 18);
        for p in lines.iter().flatten() {
            assert!(square().covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn profile_zero_is_plain_hatching() {
        let lines = fill_profile(&square(), 2.0, 0.0, Anchor::LOCAL, |_| 0.0);
        assert!(lines.len() >= 14);
        for l in &lines {
            assert!(l.iter().all(|p| (p.1 - l[0].1).abs() < 1e-9), "line not straight");
        }
    }
}
