//! `scatter`: one mark at each of many places — a stipple, sand, gravel,
//! grass tufts, a marsh, woodland, an orchard in rows, granite crosses, a
//! cemetery — or any symbol of your own.
//!
//! The places come from [`points::sites`]: an anchored lattice, every random
//! choice taken from the lattice cell's own seed. Everything this generator
//! decides per mark (its size, its turn, its shape, whether the tone keeps
//! it) comes from that site's own randomness too, so with a global anchor two
//! areas side by side draw one scatter, mark for mark.
//!
//! Marks are whole things, so the boundary is met mark by mark: `edge:
//! inside` keeps only the marks that fit, `clip` lets the runner cut them at
//! the wall, `overflow` lets them stand over it by up to its margin, and a
//! `fray` thins and shrinks whole marks toward the edge rather than chopping
//! them. Each mark's footprint (a disc of its radius plus half a pen) goes
//! into the mask, so a host can mask exactly what the marks cover.

use std::collections::HashMap;
use std::f64::consts::TAU;

use super::marks;
use super::points::{self, Arrange};
use super::schema::{Args, Default, Kind, Motif, Opt, CONTRAST, DENSITY, FEATURE, JITTER, MARK, TOWARD, VARY};
use super::tone::Tone;
use super::Generator;
use crate::geo::{Pt, Strokes};
use crate::pattern::{Canvas, EdgeMode, Tools};

const ARRANGE: Opt = Opt {
    key: "arrange",
    kind: Kind::Choice(&["blue", "hex", "square", "rows", "clusters"]),
    default: Default::Text("blue"),
    doc: "Where the marks go: blue (even but irregular, like a stipple by hand), hex or square (a lattice; add jitter to loosen it), rows (along the angle, wandering only along their row: an orchard, a vineyard), clusters (small groups).",
};
const MOTIF: Opt = Opt {
    key: "motif",
    kind: Kind::Motif(marks::NAMES),
    default: Default::Text("dot"),
    doc: "The mark: a symbol by name, or your own polylines in mm around the origin (up is negative y), drawn at their own size.",
};
const OVERLAP: Opt = Opt {
    key: "overlap",
    kind: Kind::Flag,
    default: Default::Flag(false),
    doc: "Let marks overlap their neighbours. Off, a mark that would touch a neighbour gives way to it, as an engraver leaves symbols apart.",
};
const ROTATE: Opt = Opt {
    key: "rotate",
    kind: Kind::NumberOr(&["random", "aligned", "upright"]),
    default: Default::Text("random"),
    doc: "How marks turn: random (every mark its own way), aligned (with the pattern's angle), upright (standing up on the page whatever the angle: trees, reeds, crosses), or a number of degrees added to the pattern's angle.",
};

pub static SCATTER: Generator = Generator {
    name: "scatter",
    about: "One mark at each of many places: dots, strokes, stones, plants or a symbol of your own, evenly scattered, on a lattice, in rows or in clusters, thinned by tone.",
    spacing: 1.0,
    angle: 0.0,
    options: &[&ARRANGE, &MOTIF, &MARK, &ROTATE, &VARY, &JITTER, &OVERLAP, &DENSITY, &CONTRAST, &FEATURE, &TOWARD],
    own_mask: true,
    inset: 0.0,
    draw,
};

/// Half the pen the marks are drawn for: a mark's footprint reaches this far
/// past its centre lines.
const HALF_PEN: f64 = 0.125;

/// What is placed at each site.
enum Shape<'a> {
    Named(&'a str),
    /// Your own polylines and the farthest they reach from the origin.
    Lines(&'a Strokes, f64),
}

fn draw(c: &Canvas, t: &Tools, a: &Args) -> Strokes {
    let size = a.num("mark").max(0.0);
    let vary = a.num("vary").clamp(0.0, 1.0);
    let shape = match a.motif("motif") {
        Some(Motif::Lines(lines)) => Shape::Lines(lines, marks::radius(lines)),
        Some(Motif::Named(name)) => Shape::Named(name),
        None => Shape::Named("dot"),
    };
    // The farthest any mark can reach from its centre: a named mark keeps to
    // its size box.
    let reach = match shape {
        Shape::Named(_) => size * std::f64::consts::FRAC_1_SQRT_2,
        Shape::Lines(_, r) => r,
    } * (1.0 + 0.5 * vary);
    if !(reach > 0.0 && reach.is_finite()) {
        return Strokes::new();
    }
    let fixed = match a.number_or("rotate") {
        Ok(deg) => Some((c.angle + deg).to_radians()),
        Err("aligned") => Some(c.angle.to_radians()),
        Err("upright") => Some(0.0),
        Err(_) => None,
    };
    let tone = Tone::new(a, c.region, c.seed);
    let fray = c.fray > 0.0;
    if fray {
        t.claim_fray();
    }

    let overlap = a.flag("overlap");

    // Centres: anywhere a mark could still show (clip), inside the region
    // (inside, overflow: each mark is then checked against the wall). When
    // marks give way to each other, clip looks farther out, so a mark near
    // the edge sees every neighbour it could touch whichever area draws it.
    let pad = match (c.edge, overlap) {
        (EdgeMode::Clip, true) => reach,
        (EdgeMode::Clip, false) => 3.0 * reach,
        _ => 0.0,
    };
    let field = t.placement_field(pad);
    let sites = points::sites(t, c.seed, &field, c.spacing, Arrange::parse(a.word("arrange")), a.num("jitter"), c.angle);

    let mut placed: Vec<Placed> = Vec::with_capacity(sites.len());
    for site in sites {
        let mut rng = site.rng;
        let p = site.at;
        // Tone and fray both thin by the site's rank, so the marks that stay
        // stay well spread: a gradient, not holes.
        let mut keep = if tone.is_even() { 1.0 } else { tone.at(p) };
        let mut scale = 1.0 + vary * rng.range(-0.5, 0.5);
        if fray {
            let w = t.whole_at(p);
            keep *= w;
            scale *= 0.7 + 0.3 * w;
        }
        if keep < site.rank {
            continue;
        }
        let turn = fixed.unwrap_or_else(|| rng.range(0.0, TAU));
        let local = match shape {
            Shape::Named(name) => marks::draw(name, size * scale, &mut rng),
            Shape::Lines(lines, _) => lines.iter().map(|l| l.iter().map(|q| (q.0 * scale, q.1 * scale)).collect()).collect(),
        };
        let r = match shape {
            Shape::Named(_) => marks::radius(&local),
            Shape::Lines(_, r) => r * scale,
        };
        if local.is_empty() || !fits(c, p, r) {
            continue;
        }
        let (sin, cos) = turn.sin_cos();
        let strokes: Strokes = local.into_iter().map(|l| l.into_iter().map(|q| (p.0 + q.0 * cos - q.1 * sin, p.1 + q.0 * sin + q.1 * cos)).collect()).collect();
        let half = strokes.iter().flatten().fold((0.0f64, 0.0f64), |h, q| (h.0.max((q.0 - p.0).abs()), h.1.max((q.1 - p.1).abs())));
        placed.push(Placed { at: p, r, half, rank: site.rank, strokes });
    }
    if !overlap {
        give_way(&mut placed);
    }

    if t.wants_mask() {
        t.add_mask_rings(&placed.iter().map(|m| footprint(m.at, m.r + HALF_PEN)).collect::<Vec<_>>());
    }
    placed.into_iter().flat_map(|m| m.strokes).collect()
}

/// One mark, drawn at its place.
struct Placed {
    at: Pt,
    /// How far it reaches from `at`, and how far across and down: its
    /// footprint is where this disc and this box meet.
    r: f64,
    half: (f64, f64),
    rank: f64,
    strokes: Strokes,
}

/// How close two marks may come, as a share of their two radii (plus a pen
/// width, the ink either side), before the one of higher rank gives way. A
/// little under 1: a radius is the mark's farthest point (a tree's is its
/// trunk), so circles that just meet still leave the marks apart.
const TOUCH: f64 = 0.9;

/// Whether the footprints of `a` and `b` come within a pen width.
fn touch(a: &Placed, b: &Placed) -> bool {
    let (dx, dy) = ((a.at.0 - b.at.0).abs(), (a.at.1 - b.at.1).abs());
    let pen = 2.0 * HALF_PEN;
    let near = TOUCH * (a.r + b.r) + pen;
    dx < a.half.0 + b.half.0 + pen && dy < a.half.1 + b.half.1 + pen && dx * dx + dy * dy < near * near
}

/// Drop every mark that would touch a neighbour of lower rank. The rule
/// looks only at the two marks, never at the order they were made in, so a
/// mark is kept or dropped the same whichever area draws it; and ranks are
/// the tone's thresholds, so the marks that stay are the ones the tone keeps
/// longest.
fn give_way(placed: &mut Vec<Placed>) {
    let reach = placed.iter().map(|m| m.r).fold(0.0, f64::max);
    if placed.len() < 2 || reach <= 0.0 {
        return;
    }
    let cell = 2.0 * TOUCH * reach;
    let key = |p: Pt| ((p.0 / cell).floor() as i64, (p.1 / cell).floor() as i64);
    let mut grid: HashMap<(i64, i64), Vec<u32>> = HashMap::with_capacity(placed.len());
    for (k, m) in placed.iter().enumerate() {
        grid.entry(key(m.at)).or_default().push(k as u32);
    }
    let first = |a: &Placed, b: &Placed| (a.rank, a.at.0, a.at.1).partial_cmp(&(b.rank, b.at.0, b.at.1)) == Some(std::cmp::Ordering::Less);
    let keep: Vec<bool> = placed
        .iter()
        .map(|m| {
            let (i, j) = key(m.at);
            !(-1..=1).any(|di| {
                (-1..=1).any(|dj| {
                    grid.get(&(i + di, j + dj)).is_some_and(|ks| {
                        ks.iter().any(|&k| {
                            let o = &placed[k as usize];
                            first(o, m) && touch(m, o)
                        })
                    })
                })
            })
        })
        .collect();
    let mut k = 0;
    placed.retain(|_| {
        k += 1;
        keep[k - 1]
    });
}

/// Whether a mark of radius `r` at `p` may be drawn under the edge mode.
/// The same rule as [`Tools::element_fits`], but asking the distance only
/// as far as `r`, which stays cheap deep inside a big region.
fn fits(c: &Canvas, p: Pt, r: f64) -> bool {
    match c.edge {
        EdgeMode::Clip => true,
        EdgeMode::Inside => c.region.dist_to_boundary_within(p, r) >= r,
        EdgeMode::Overflow(m) => m <= 0.0 || c.limit.dist_to_boundary_within(p, r) >= r,
    }
}

/// A disc of radius `r` round `p`, as a ring: a mark's footprint.
fn footprint(p: Pt, r: f64) -> Vec<Pt> {
    const N: usize = 10;
    let mut ring: Vec<Pt> = (0..N)
        .map(|k| {
            let a = TAU * k as f64 / N as f64;
            (p.0 + r * a.cos(), p.1 + r * a.sin())
        })
        .collect();
    ring.push(ring[0]);
    ring
}

#[cfg(test)]
mod tests {
    use crate::catalog::{self, Spec, Value};
    use crate::geo::{polyline_length, Region, Strokes};
    use crate::pattern::EdgeMode;
    use crate::Anchor;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Region {
        Region::from_ring(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
    }

    fn length(s: &Strokes) -> f64 {
        s.iter().map(|l| polyline_length(l)).sum()
    }

    fn scatter(region: &Region, spec: Spec) -> Strokes {
        catalog::fill("scatter", region, &spec).unwrap().strokes
    }

    #[test]
    fn every_motif_draws_in_a_small_square() {
        let square = rect(0.0, 0.0, 20.0, 20.0);
        for &name in super::marks::NAMES {
            let s = scatter(&square, Spec::default().spacing(2.5).option("motif", name).option("mark", 1.8));
            assert!(s.len() > 20, "{name} drew {} strokes", s.len());
        }
        let s = scatter(&square, Spec::default());
        assert!(s.len() > 300, "the default stipple drew {} dots", s.len());
    }

    #[test]
    fn same_seed_same_strokes() {
        let square = rect(0.0, 0.0, 20.0, 20.0);
        let spec = Spec::default().spacing(2.0).seed(7).option("motif", "tuft").option("mark", 1.5).option("vary", 0.4).option("jitter", 0.3);
        assert_eq!(scatter(&square, spec.clone()), scatter(&square, spec.clone()));
        assert_ne!(scatter(&square, spec.clone()), scatter(&square, spec.seed(8)));
    }

    #[test]
    fn global_anchor_is_seamless() {
        for (motif, arrange) in [("tuft", "blue"), ("cross", "hex"), ("tree", "rows"), ("pebble", "clusters"), ("dot", "square")] {
            let spec = Spec::default()
                .spacing(1.6)
                .anchor(Anchor::GLOBAL)
                .edge(EdgeMode::Clip)
                .option("motif", motif)
                .option("arrange", arrange)
                .option("mark", 1.4)
                .option("vary", 0.3)
                .option("jitter", 0.4);
            let whole = length(&scatter(&rect(0.0, 0.0, 40.0, 20.0), spec.clone()));
            let halves = length(&scatter(&rect(0.0, 0.0, 20.0, 20.0), spec.clone())) + length(&scatter(&rect(20.0, 0.0, 20.0, 20.0), spec));
            let err = (halves - whole).abs() / whole;
            assert!(err < 0.01, "{motif}/{arrange}: halves {halves:.2} vs whole {whole:.2} ({:.2}%)", err * 100.0);
        }
    }

    #[test]
    fn inside_keeps_every_mark_whole() {
        // A lobed island with a lake: corners, bays and a hole.
        let ring = |r: f64, n: usize| -> Vec<(f64, f64)> {
            (0..n)
                .map(|k| {
                    let a = std::f64::consts::TAU * k as f64 / n as f64;
                    let rr = r * (1.0 + 0.1 * (3.0 * a).sin());
                    (20.0 + rr * a.cos(), 20.0 + rr * a.sin())
                })
                .collect()
        };
        let island = Region::from_polygons(&[vec![ring(18.0, 90), ring(5.0, 40)]]);
        for motif in ["tree", "ring", "tuft", "rock"] {
            let spec = Spec::default().spacing(2.0).edge(EdgeMode::Inside).option("motif", motif).option("mark", 2.0).option("vary", 0.5);
            let s = scatter(&island, spec);
            assert!(s.len() > 50, "{motif}: {} strokes", s.len());
            for p in s.iter().flatten() {
                // A mark cut at the wall would end on it.
                assert!(island.covers_point(*p) && island.dist_to_boundary(*p) > 1e-6, "{motif}: a point on or past the wall at {p:?}");
            }
            // Clip, by contrast, does cut marks at the wall.
            let clipped = scatter(&island, Spec::default().spacing(2.0).option("motif", motif).option("mark", 2.0));
            assert!(clipped.iter().flatten().any(|p| island.dist_to_boundary(*p) < 1e-6), "{motif}: clip cut nothing");
        }
    }

    #[test]
    fn tone_thins_the_marks() {
        let square = rect(0.0, 0.0, 40.0, 40.0);
        let spec = Spec::default().spacing(1.0).option("density", "edge").option("contrast", 1.0).option("feature", 10.0);
        let dots = scatter(&square, spec);
        let (mut middle, mut rim) = (0usize, 0usize);
        for d in &dots {
            let p = d[0];
            if (12.0..28.0).contains(&p.0) && (12.0..28.0).contains(&p.1) {
                middle += 1;
            } else if !(5.0..35.0).contains(&p.0) || !(5.0..35.0).contains(&p.1) {
                rim += 1;
            }
        }
        // Per square millimetre: the middle 16×16 has none at full contrast,
        // the rim (5 mm wide, 700 mm²) is dense.
        let (dm, dr) = (middle as f64 / 256.0, rim as f64 / 700.0);
        assert!(dr > 0.6, "rim density {dr:.2}");
        assert!(dm < 0.1 * dr, "middle {dm:.3} vs rim {dr:.3}");
        // A gentler contrast thins without emptying.
        let soft = scatter(&square, Spec::default().spacing(1.0).option("density", "edge").option("contrast", 0.5).option("feature", 10.0));
        let n = soft.iter().filter(|d| (12.0..28.0).contains(&d[0].0) && (12.0..28.0).contains(&d[0].1)).count() as f64 / 256.0;
        assert!(n > 0.3 * dr && n < 0.8 * dr, "soft middle {n:.2} vs rim {dr:.2}");
    }

    #[test]
    fn fray_drops_whole_marks() {
        let square = rect(0.0, 0.0, 30.0, 30.0);
        let spec = Spec::default().spacing(2.0).fray(6.0).option("motif", "plus").option("mark", 1.5).option("rotate", "aligned");
        let s = scatter(&square, spec);
        // Whole marks only: every plus is two strokes, never chopped bits.
        assert_eq!(s.len() % 2, 0);
        let near = s.iter().filter(|l| l[0].0 < 2.0).count();
        let deep = s.iter().filter(|l| (14.0..16.0).contains(&l[0].0)).count();
        assert!(near < deep / 2, "fray band {near} vs middle {deep}");
    }

    #[test]
    fn custom_motif_is_drawn_at_its_own_size() {
        let square = rect(0.0, 0.0, 20.0, 20.0);
        let arrow: Strokes = vec![vec![(0.0, 0.6), (0.0, -0.6)], vec![(-0.3, -0.3), (0.0, -0.6), (0.3, -0.3)]];
        let spec = Spec::default().spacing(2.0).edge(EdgeMode::Inside).option("motif", Value::Lines(arrow)).option("rotate", "upright").option("arrange", "square");
        let s = scatter(&square, spec);
        assert!(s.len() > 100);
        // Upright: every arrow's shaft is vertical and 1.2 mm long.
        assert!(s.iter().step_by(2).all(|l| l.len() == 2 && (l[0].0 - l[1].0).abs() < 1e-9 && ((l[0].1 - l[1].1).abs() - 1.2).abs() < 1e-9));
        assert!(catalog::fill("scatter", &square, &Spec::default().option("motif", Value::Lines(vec![]))).is_err());
        assert!(catalog::fill("scatter", &square, &Spec::default().option("motif", "triffid")).is_err());
    }

    #[test]
    fn mask_is_the_marks_footprint() {
        let square = rect(0.0, 0.0, 20.0, 20.0);
        let sparse = catalog::fill_output("scatter", &square, &Spec::default().spacing(4.0).option("motif", "ring").option("mark", 1.0)).unwrap();
        let area = |r: &Region| -> f64 {
            r.all_rings_mm()
                .iter()
                .map(|ring| ring.iter().zip(ring.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum::<f64>() / 2.0)
                .sum::<f64>()
                .abs()
        };
        let a = area(&sparse.mask);
        assert!(a > 5.0 && a < 120.0, "mask area {a:.1} of 400");
    }

    #[test]
    fn marks_give_way_unless_overlap() {
        let square = rect(0.0, 0.0, 30.0, 30.0);
        let rings = |overlap: bool| {
            let spec = Spec::default().spacing(1.6).option("motif", "ring").option("mark", 1.4).option("vary", 0.4).option("overlap", overlap);
            // A ring is one closed stroke; its centre is the mean of its points.
            scatter(&square, spec.edge(EdgeMode::Inside))
                .iter()
                .map(|l| {
                    let n = (l.len() - 1) as f64;
                    let c = l[..l.len() - 1].iter().fold((0.0, 0.0), |c, p| (c.0 + p.0 / n, c.1 + p.1 / n));
                    let r = l.iter().map(|p| (p.0 - c.0).hypot(p.1 - c.1)).fold(0.0, f64::max);
                    (c, r)
                })
                .collect::<Vec<_>>()
        };
        let touching = |rs: &[((f64, f64), f64)]| {
            let mut n = 0;
            for (i, (a, ra)) in rs.iter().enumerate() {
                for (b, rb) in &rs[i + 1..] {
                    if (a.0 - b.0).hypot(a.1 - b.1) < ra + rb {
                        n += 1;
                    }
                }
            }
            n
        };
        let apart = rings(false);
        let crowded = rings(true);
        assert!(apart.len() > 150, "{} rings", apart.len());
        assert_eq!(touching(&apart), 0);
        assert!(touching(&crowded) > 20 && crowded.len() > apart.len());
    }

    #[test]
    fn rotate_takes_words_and_numbers() {
        let square = rect(0.0, 0.0, 10.0, 10.0);
        for r in [Value::from("random"), Value::from("aligned"), Value::from("upright"), Value::from(30.0)] {
            assert!(!scatter(&square, Spec::default().spacing(2.0).option("motif", "tick").option("mark", 1.0).option("rotate", r)).is_empty());
        }
        // Aligned ticks turn with the angle; upright ones do not.
        let ticks = |rot: &str| scatter(&square, Spec::default().spacing(2.0).angle(90.0).option("motif", "tick").option("mark", 1.0).option("rotate", rot));
        let slope = |s: &Strokes| s.iter().map(|l| (l[1].0 - l[0].0).abs() / ((l[1].1 - l[0].1).abs() + 1e-9)).sum::<f64>() / s.len() as f64;
        assert!(slope(&ticks("upright")) < 0.2);
        assert!(slope(&ticks("aligned")) > 5.0);
    }
}
