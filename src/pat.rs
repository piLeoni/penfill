//! CAD hatch patterns in the `.pat` format (AutoCAD, LibreCAD, QCAD…).
//!
//! A pattern is a list of line families. Each family line reads
//!
//! ```text
//! angle, x0, y0, dx, dy [, dash1, dash2, …]
//! ```
//!
//! — lines at `angle` degrees through `(x0, y0)`, repeated every `dy`
//! across the line direction and shifted `dx` along it at each repeat.
//! Dashes alternate pen down (positive), pen up (negative) and dots (0);
//! no dashes means a solid line. A header line `*NAME, description`
//! starts a pattern; `;` starts a comment.
//!
//! Any `.pat` file can be parsed with [`parse_pat`]. A few patterns are
//! built in ([`builtin`]); their definitions are derived here from the
//! brick geometry, not copied from a vendor file.

use crate::anchor::Anchor;
use crate::geo::{Pt, Region, Strokes};

/// One line family of a pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct PatLine {
    pub angle: f64,
    pub origin: Pt,
    pub delta: (f64, f64),
    pub dashes: Vec<f64>,
}

/// A named hatch pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub name: String,
    pub lines: Vec<PatLine>,
}

/// Running bond: bricks 2 × 1 units, every other course shifted half a brick.
const BRICK: &str = "*BRICK, running bond, bricks 2 x 1
0, 0,0, 0,1
90, 0,0, 1,1, 1,-1
";

/// Herringbone with 2 × 1 bricks, laid square to the page. Turn it 45°
/// for the classic parquet look. Each brick edge line is three units of
/// ink and one of gap, stepping one unit along at every repeat.
const HERRINGBONE: &str = "*HERRINGBONE, bricks 2 x 1 in a staircase
0, -1,0, 1,1, 3,-1
90, 0,0, 1,-1, 3,-1
";

/// Basket weave: 3 × 3 squares of three lines, horizontal and vertical in
/// a checkerboard.
const BASKET: &str = "*BASKET, basket weave, blocks of three lines
0, 0,0.5, 3,3, 3,-3
0, 0,1.5, 3,3, 3,-3
0, 0,2.5, 3,3, 3,-3
90, 0.5,3, 3,-3, 3,-3
90, 1.5,3, 3,-3, 3,-3
90, 2.5,3, 3,-3, 3,-3
";

/// Parquet: basket weave with blocks of five boards.
const PARQUET: &str = "*PARQUET, blocks of five boards, alternating
0, 0,0.5, 5,5, 5,-5
0, 0,1.5, 5,5, 5,-5
0, 0,2.5, 5,5, 5,-5
0, 0,3.5, 5,5, 5,-5
0, 0,4.5, 5,5, 5,-5
90, 0.5,5, 5,-5, 5,-5
90, 1.5,5, 5,-5, 5,-5
90, 2.5,5, 5,-5, 5,-5
90, 3.5,5, 5,-5, 5,-5
90, 4.5,5, 5,-5, 5,-5
";

/// Drafting section hatches, in the spirit of the ANSI families: plain
/// 45° lines (general / cast iron), close pairs (steel), solid and dashed
/// alternating (bronze), crossed (lead, rubber).
const SECTIONS: &str = "*ANSI31, 45 degree section lines
45, 0,0, 0,1
*ANSI32, steel: close pairs of 45 degree lines
45, 0,0, 0,2
45, -0.353553,0.353553, 0,2
*ANSI33, bronze: solid and dashed 45 degree lines
45, 0,0, 0,2
45, -0.707107,0.707107, 0,2, 1,-0.5
*ANSI37, crossed 45 degree lines
45, 0,0, 0,1
135, 0,0, 0,1
";

/// Site and detail symbols.
const SYMBOLS: &str = "*EARTH, earth: groups of three lines, flat and upright in a checkerboard
0, 0,0.25, 1,1, 1,-1
0, 0,0.5, 1,1, 1,-1
0, 0,0.75, 1,1, 1,-1
90, 1.25,0, 1,1, 1,-1
90, 1.5,0, 1,1, 1,-1
90, 1.75,0, 1,1, 1,-1
*CROSS, small plus signs on a square grid
0, 0,0, 0,2, 1,-1
90, 0.5,-0.5, 0,2, 1,-1
*DASH, staggered dashes
0, 0,0, 1,1, 1,-1
*STAIRS, a staircase of steps
0, 0,0, 1,1, 1,-1
90, 1,0, 1,1, 1,-1
*GRATE, grating: dense bars crossed by sparse ones
0, 0,0, 0,0.5
90, 0,0, 0,3
*ANGLE, steel angles: L shapes on a grid
0, 0,0, 0,1.5, 1,-0.5
90, 0,0, 0,1.5, 1,-0.5
";

/// Names of the built-in patterns.
pub const BUILTIN: [&str; 14] = [
    "BRICK", "HERRINGBONE", "BASKET", "PARQUET", "ANSI31", "ANSI32", "ANSI33", "ANSI37", "EARTH", "CROSS",
    "DASH", "STAIRS", "GRATE", "ANGLE",
];

/// A built-in pattern by name (case-insensitive).
pub fn builtin(name: &str) -> Option<Pattern> {
    let name = name.to_ascii_uppercase();
    [BRICK, HERRINGBONE, BASKET, PARQUET, SECTIONS, SYMBOLS]
        .iter()
        .flat_map(|t| parse_pat(t))
        .find(|p| p.name == name)
}

/// Every pattern in a `.pat` text. Malformed lines are skipped.
pub fn parse_pat(text: &str) -> Vec<Pattern> {
    let mut out: Vec<Pattern> = Vec::new();
    for raw in text.lines() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(head) = line.strip_prefix('*') {
            let name = head.split(',').next().unwrap_or("").trim().to_string();
            out.push(Pattern { name, lines: vec![] });
            continue;
        }
        let Some(pattern) = out.last_mut() else { continue };
        let nums: Vec<f64> = line
            .split(',')
            .map(|s| s.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .unwrap_or_default();
        if nums.len() < 5 {
            continue;
        }
        pattern.lines.push(PatLine {
            angle: nums[0],
            origin: (nums[1], nums[2]),
            delta: (nums[3], nums[4]),
            dashes: nums[5..].to_vec(),
        });
    }
    out
}

/// Hatch `region` with `pattern`, scaled by `scale` (pattern units → mm)
/// and turned by `angle` degrees. A `.pat` hatch is drawn from the page
/// origin, so its local placement is already the page one; `shift` moves it.
pub fn fill_pat(region: &Region, pattern: &Pattern, scale: f64, angle: f64, anchor: Anchor) -> Strokes {
    if scale <= 0.0 || region.is_empty() {
        return vec![];
    }
    let rings = region.all_rings_mm();
    let (sin_r, cos_r) = angle.to_radians().sin_cos();
    let at = anchor.resolve((0.0, 0.0));
    let turn = |p: Pt| (at.0 + p.0 * cos_r - p.1 * sin_r, at.1 + p.0 * sin_r + p.1 * cos_r);
    let mut out: Strokes = Vec::new();
    for fam in &pattern.lines {
        if fam.delta.1.abs() < 1e-9 {
            continue;
        }
        let a = (fam.angle + angle).to_radians();
        let (u, v) = ((a.cos(), a.sin()), (-a.sin(), a.cos()));
        let o = turn((fam.origin.0 * scale, fam.origin.1 * scale));
        let (dx, dy) = (fam.delta.0 * scale, fam.delta.1 * scale);
        let dashes: Vec<f64> = fam.dashes.iter().map(|d| d * scale).collect();

        // Region in the family frame: s along the lines, t across.
        let frame: Vec<Vec<Pt>> = rings
            .iter()
            .map(|r| {
                r.iter()
                    .map(|p| {
                        let q = (p.0 - o.0, p.1 - o.1);
                        (q.0 * u.0 + q.1 * u.1, q.0 * v.0 + q.1 * v.1)
                    })
                    .collect()
            })
            .collect();
        let (tmin, tmax) = frame
            .iter()
            .flatten()
            .fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
        let (k0, k1) = {
            let (a, b) = (tmin / dy, tmax / dy);
            (a.min(b).floor() as i64, a.max(b).ceil() as i64)
        };
        for k in k0..=k1 {
            let t = k as f64 * dy;
            let phase = k as f64 * dx;
            for (s0, s1) in crossings(&frame, t) {
                for (a, b) in dash_runs(s0, s1, phase, &dashes) {
                    let p = (o.0 + u.0 * a + v.0 * t, o.1 + u.1 * a + v.1 * t);
                    let q = (o.0 + u.0 * b + v.0 * t, o.1 + u.1 * b + v.1 * t);
                    out.push(vec![p, q]);
                }
            }
        }
    }
    out
}

/// Inside intervals of the line `t = const` (even-odd), in `s`.
fn crossings(rings: &[Vec<Pt>], t: f64) -> Vec<(f64, f64)> {
    let mut xs: Vec<f64> = Vec::new();
    for r in rings {
        let n = r.len();
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            if (a.1 <= t && b.1 > t) || (b.1 <= t && a.1 > t) {
                xs.push(a.0 + (t - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.chunks(2)
        .filter(|c| c.len() == 2 && c[1] - c[0] > 1e-9)
        .map(|c| (c[0], c[1]))
        .collect()
}

/// Pen-down runs of a dash sequence that starts at `phase`, cut to
/// `[s0, s1]`. A zero dash is a dot, drawn as a 0.1 mm tick.
fn dash_runs(s0: f64, s1: f64, phase: f64, dashes: &[f64]) -> Vec<(f64, f64)> {
    if dashes.is_empty() {
        return vec![(s0, s1)];
    }
    let period: f64 = dashes.iter().map(|d| d.abs()).sum();
    if period < 1e-9 {
        return vec![(s0, s1)];
    }
    let mut out = Vec::new();
    let mut s = phase + ((s0 - phase) / period).floor() * period;
    while s < s1 {
        for &d in dashes {
            let len = d.abs();
            if d >= 0.0 {
                let end = if d == 0.0 { s + 0.1 } else { s + len };
                let (a, b) = (s.max(s0), end.min(s1));
                if b - a > 1e-6 {
                    out.push((a, b));
                }
            }
            s += len;
            if s >= s1 {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_families_and_dashes() {
        let p = builtin("brick").unwrap();
        assert_eq!(p.name, "BRICK");
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.lines[1].dashes, vec![1.0, -1.0]);
        assert_eq!(parse_pat("; only a comment\n").len(), 0);
    }

    #[test]
    fn brick_joints_alternate_between_courses() {
        let sq = Region::from_ring(&[(0.1, 0.1), (8.1, 0.1), (8.1, 4.1), (0.1, 4.1)]);
        let s = fill_pat(&sq, &builtin("BRICK").unwrap(), 1.0, 0.0, Anchor::LOCAL);
        let joints: Vec<(f64, f64)> = s
            .iter()
            .filter(|l| (l[0].0 - l[1].0).abs() < 1e-6)
            .map(|l| (l[0].0, (l[0].1 + l[1].1) / 2.0))
            .collect();
        // Course 0 (y 0..1) and course 1 (y 1..2) joints are one unit apart.
        let row = |lo: f64| {
            let mut xs: Vec<f64> = joints.iter().filter(|j| j.1 > lo && j.1 < lo + 1.0).map(|j| j.0).collect();
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            xs
        };
        let (a, b) = (row(1.0), row(2.0));
        assert!(!a.is_empty() && !b.is_empty());
        for x in &a {
            assert!(b.iter().all(|y| ((x - y).abs() - 1.0).abs() < 1e-6 || (x - y).abs() > 1.5));
            assert!(b.iter().any(|y| ((x - y).abs() - 1.0).abs() < 1e-6));
        }
    }

    #[test]
    fn strokes_stay_inside() {
        let sq = Region::from_ring(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        for name in BUILTIN {
            let s = fill_pat(&sq, &builtin(name).unwrap(), 1.3, 30.0, Anchor::LOCAL);
            assert!(!s.is_empty(), "{name} drew nothing");
            for l in &s {
                for p in l {
                    assert!(p.0 > -1e-6 && p.0 < 20.0 + 1e-6 && p.1 > -1e-6 && p.1 < 20.0 + 1e-6, "{name}: {p:?}");
                }
            }
        }
    }
}
