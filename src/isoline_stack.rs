//! Stacked bubble slices: vegetation or cloud masses seen from above.
//!
//! Crowns are clusters of spheres at different heights (metaballs, so
//! touching spheres melt into one bubbly surface). The volume is cut into
//! horizontal slices and each slice's outline is drawn, top slice first;
//! every slice hides what lies under the slices above it. A bulge that is
//! wider than the layer below it therefore overhangs and hides it — unlike
//! the nested rings of a plain height field ([`crate::isoline_blobs`]),
//! where every level sits inside the one below.
//!
//! [`Light`] rakes every crown from one direction: bubbles swell and climb
//! on the far side, thin and flatten on the near side, so the stack reads
//! as lit without changing ink weight or colour.
//!
//! Visual target: `drawland/docs/architecture/img/isoline-blob-fills/`.

use crate::geo::{Pt, Region, Strokes};
use crate::isoline_blobs::FillWithCoverage;
use crate::noise::Perlin;
use crate::random::{poisson_disk, Rng};
use std::collections::HashMap;

/// Field level the outlines are drawn at.
const ISO: f64 = 0.25;
/// Where an isolated sphere's outline sits, as a fraction of its support
/// radius: `(1 - q)³ = ISO` at `q = VISIBLE²`.
const VISIBLE: f64 = 0.608;

/// How the masses meet the region's boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeMode {
    /// Ignore the boundary: masses run past it and are cut there.
    Clip,
    /// Every mass closes before the boundary; nothing is cut.
    Inside,
    /// Bubbles line the boundary and bulge past it by up to this many mm.
    Overflow(f64),
}

/// A crown at a known position (a mapped tree): `radius` across in mm,
/// `height` how tall it stands among its neighbours, in mm.
#[derive(Clone, Copy, Debug)]
pub struct Crown {
    pub at: Pt,
    pub radius: f64,
    pub height: f64,
}

/// A sun raking across every crown from one direction: bubbles lean away
/// from it, so the far side of each bump stands taller and wider — more
/// overhang, more nested rings — while the near side stays low and tight.
/// Reads as shading without touching ink colour or width.
///
/// With `hatch > 0` the stack is also shaded like an engraving: the top
/// surface is lit from `from_deg` at `elevation_deg` (cast shadows
/// included) and its dark parts hatched, more lines the darker, crossed in
/// the deepest shadow; outlines break where the light hits them full on.
#[derive(Clone, Copy, Debug)]
pub struct Light {
    /// Degrees, 0 = +x, 90 = +y: the direction the light comes *from*.
    pub from_deg: f64,
    /// 0 – 1: how strong the shading is.
    pub strength: f64,
    /// Degrees above the horizon; lower is longer shadows.
    pub elevation_deg: f64,
    /// mm between shading hatch lines; 0 draws no hatching.
    pub hatch: f64,
}

impl Default for Light {
    fn default() -> Self {
        Light { from_deg: 135.0, strength: 0.8, elevation_deg: 35.0, hatch: 0.4 }
    }
}

#[derive(Clone, Debug)]
pub enum CrownSource {
    /// Scattered over the region, patchy by `coverage`.
    Generate,
    /// Exactly these crowns; `coverage` is ignored.
    Crowns(Vec<Crown>),
}

#[derive(Clone, Debug)]
pub struct IsolineStackParams {
    pub seed: u64,
    pub crowns: CrownSource,
    /// Typical crown radius in mm.
    pub crown: f64,
    /// Typical bubble radius in mm: smaller is lumpier.
    pub bubble: f64,
    /// 0 – 1: share of the region the generated crowns cover.
    pub coverage: f64,
    /// Horizontal cuts through the whole stack: more is more rings.
    pub slices: usize,
    /// 0 – 1-ish: wobble of every outline.
    pub jitter: f64,
    pub edge: EdgeMode,
    /// A sun from one direction, shading every crown the same way. `None`:
    /// every crown is lit evenly, as before.
    pub light: Option<Light>,
}

impl Default for IsolineStackParams {
    fn default() -> Self {
        IsolineStackParams {
            seed: 0,
            crowns: CrownSource::Generate,
            crown: 6.0,
            bubble: 1.8,
            coverage: 0.6,
            slices: 14,
            jitter: 0.8,
            edge: EdgeMode::Clip,
            light: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Ball {
    x: f64,
    y: f64,
    z: f64,
    /// Support radius: the field is zero past it.
    r: f64,
}

pub fn fill_isoline_stack(region: &Region, p: &IsolineStackParams) -> Strokes {
    fill_isoline_stack_with_coverage(region, p).strokes
}

pub fn fill_isoline_stack_with_coverage(region: &Region, p: &IsolineStackParams) -> FillWithCoverage {
    if region.is_empty() || p.crown <= 0.0 || p.bubble <= 0.0 || p.slices == 0 {
        return FillWithCoverage::default();
    }
    let bubble = p.bubble.min(p.crown);
    let warp = p.jitter.max(0.0) * bubble * 0.3;
    let mut rng = Rng::new(p.seed.wrapping_add(71));

    let crowns = match &p.crowns {
        CrownSource::Crowns(c) => c.clone(),
        CrownSource::Generate => generate_crowns(region, p, &mut rng),
    };
    let limit = match p.edge {
        EdgeMode::Clip => None,
        EdgeMode::Inside => Some(0.0),
        EdgeMode::Overflow(m) => Some(m.max(0.0)),
    };
    let light = p.light.map(|l| {
        let r = l.from_deg.to_radians();
        ((r.cos(), r.sin()), l.strength.clamp(0.0, 1.0))
    });
    let balls: Vec<Ball> = crowns.iter().flat_map(|c| crown_balls(c, bubble, &mut rng, light)).collect();
    let balls = match limit {
        Some(l) => fit_to_limit(region, balls, l),
        None => balls,
    };
    if balls.is_empty() {
        return FillWithCoverage::default();
    }
    let cell = (bubble * 0.1).clamp(0.05, 0.3);
    let frame = frame_around(&balls, warp + 2.0 * cell);
    let fade = limit.map(|l| Fade::new(region, &frame, l, bubble));
    let jitter_noise = Perlin::new(p.seed.wrapping_add(5));
    let warp_x = Perlin::new(p.seed.wrapping_add(6));
    let warp_y = Perlin::new(p.seed.wrapping_add(7));
    let wobble = bubble * 0.7;

    let z_lo = balls.iter().map(|b| b.z - b.r).fold(f64::MAX, f64::min);
    let z_hi = balls.iter().map(|b| b.z + b.r).fold(f64::MIN, f64::max);
    let dz = (z_hi - z_lo) / p.slices as f64;

    let mut shade = p.light.filter(|l| l.hatch > 0.0 && l.strength > 0.0).map(|_| Surface::new(&frame, bubble));
    let mut strokes: Strokes = Vec::new();
    let mut above = Region::default();
    for k in (0..p.slices).rev() {
        let z = z_lo + (k as f64 + 0.5) * dz;
        let cut: Vec<(f64, f64, f64)> = balls
            .iter()
            .filter(|b| (z - b.z).abs() < b.r)
            .map(|b| (b.x, b.y, b.r * b.r - (z - b.z).powi(2)))
            .collect();
        if cut.is_empty() {
            continue;
        }
        let grid = Grid::new(&cut);
        let f = |q: Pt| {
            let (x, y) = if warp > 0.0 {
                let (u, v) = (q.0 / wobble, q.1 / wobble);
                (q.0 + warp * warp_x.fbm(u + z * 0.31, v, 2), q.1 + warp * warp_y.fbm(u, v - z * 0.27, 2))
            } else {
                q
            };
            let mut h = grid.field(x, y);
            if h <= 0.0 {
                return 0.0;
            }
            // Faded at the unwarped sample, so the wobble can't carry ink past the limit.
            if let Some(fade) = &fade {
                h *= fade.at(q);
            }
            h * (1.0 + p.jitter * 0.8 * jitter_noise.fbm(x / wobble + z * 0.43, y / wobble - z * 0.37, 2))
        };
        let loops = crate::field::fill_isolines(&frame, cell, &[ISO], &f);
        if let Some(s) = &mut shade {
            let reach = cut.iter().map(|d| d.2.sqrt()).fold(0.0, f64::max) + warp;
            let (x0, y0, x1, y1) = cut.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, d| {
                (b.0.min(d.0), b.1.min(d.1), b.2.max(d.0), b.3.max(d.1))
            });
            s.mark(z, (x0 - reach, y0 - reach, x1 + reach, y1 + reach), &f);
        }
        let slice = Region::from_rings_even_odd(&loops);
        if above.is_empty() {
            strokes.extend(loops);
        } else {
            strokes.extend(frame.difference(&above).clip_strokes(&loops));
        }
        above = above.union(&slice);
    }

    let keep = match p.edge {
        EdgeMode::Overflow(m) => region.outset(m.max(0.0)),
        _ => region.clone(),
    };
    // Bottom slice first reads as drawing the ground up.
    strokes.reverse();
    if let (Some(mut s), Some(l)) = (shade, p.light) {
        s.light(z_lo - dz, l, bubble);
        strokes = s.break_highlights(&strokes, l.strength);
        strokes.extend(s.hatching(&keep, l));
    }
    FillWithCoverage { strokes: keep.clip_strokes(&strokes), coverage: above.intersection(&keep) }
}

/// The stack's top surface on a grid, lit: what the shading hatch follows.
struct Surface {
    x0: f64,
    y0: f64,
    step: f64,
    nx: usize,
    ny: usize,
    /// Height of the topmost slice over each cell; NaN where none is.
    h: Vec<f64>,
    /// 0 (lit) – 1 (black) once lit.
    dark: Vec<f64>,
    /// Light on the surface relative to a flat top: above 1 faces the sun.
    glare: Vec<f64>,
}

impl Surface {
    fn new(frame: &Region, bubble: f64) -> Surface {
        let (x0, y0, x1, y1) = frame.bounds_mm();
        let step = (bubble * 0.2).clamp(0.12, 0.5);
        let nx = ((x1 - x0) / step).ceil() as usize + 1;
        let ny = ((y1 - y0) / step).ceil() as usize + 1;
        Surface { x0, y0, step, nx, ny, h: vec![f64::NAN; nx * ny], dark: Vec::new(), glare: Vec::new() }
    }

    fn at(&self, i: usize, j: usize) -> Pt {
        (self.x0 + i as f64 * self.step, self.y0 + j as f64 * self.step)
    }

    /// Slices come top first: a cell the slice at `z` covers, and no slice
    /// above did, stands at `z`.
    fn mark(&mut self, z: f64, (x0, y0, x1, y1): (f64, f64, f64, f64), f: impl Fn(Pt) -> f64) {
        let lo = |v: f64, o: f64, n: usize| (((v - o) / self.step).floor().max(0.0) as usize).min(n - 1);
        let (i0, i1) = (lo(x0, self.x0, self.nx), lo(x1, self.x0, self.nx) + 1);
        let (j0, j1) = (lo(y0, self.y0, self.ny), lo(y1, self.y0, self.ny) + 1);
        for j in j0..j1.min(self.ny) {
            for i in i0..i1.min(self.nx) {
                let k = j * self.nx + i;
                if self.h[k].is_nan() && f(self.at(i, j)) >= ISO {
                    self.h[k] = z;
                }
            }
        }
    }

    /// Smooth the terraces into a surface, then light it: Lambert against a
    /// sun at `elevation_deg`, and cast shadows marched towards the sun.
    fn light(&mut self, ground: f64, l: Light, bubble: f64) {
        for v in &mut self.h {
            if v.is_nan() {
                *v = ground;
            }
        }
        let r = ((bubble * 0.35) / self.step).ceil().max(1.0) as usize;
        for _ in 0..2 {
            self.h = blur(&self.h, self.nx, self.ny, r, true);
            self.h = blur(&self.h, self.nx, self.ny, r, false);
        }
        let (az, el) = (l.from_deg.to_radians(), l.elevation_deg.clamp(5.0, 85.0).to_radians());
        let (lx, ly) = (az.cos(), az.sin());
        let (lh, lz, rise) = (el.cos(), el.sin(), el.tan());
        let strength = l.strength.clamp(0.0, 1.0);
        let top = self.h.iter().copied().fold(f64::MIN, f64::max);
        let reach = bubble * 3.0;
        let (nx, ny) = (self.nx, self.ny);
        let mut dark = vec![0.0; nx * ny];
        let mut glare = vec![1.0; nx * ny];
        for j in 1..ny - 1 {
            for i in 1..nx - 1 {
                let k = j * nx + i;
                let hx = (self.h[k + 1] - self.h[k - 1]) / (2.0 * self.step);
                let hy = (self.h[k + nx] - self.h[k - nx]) / (2.0 * self.step);
                let norm = (hx * hx + hy * hy + 1.0).sqrt();
                let lit = ((-hx * lx * lh - hy * ly * lh + lz) / norm).max(0.0);
                let rel = lit / lz;
                let mut d = (1.0 - rel).clamp(0.0, 1.0);
                // March towards the sun until the ray clears the tallest crown;
                // shadows stay short and half-tone so gaps don't go black.
                let mut t = self.step;
                while self.h[k] + t * rise < top && t < reach {
                    let (u, v) = (i as f64 + t * lx / self.step, j as f64 + t * ly / self.step);
                    if u < 0.0 || v < 0.0 || u >= (nx - 1) as f64 || v >= (ny - 1) as f64 {
                        break;
                    }
                    if self.sample(&self.h, u, v) > self.h[k] + t * rise + self.step * 0.5 {
                        d = d.max(0.5 * (1.0 - t / reach) + 0.25);
                        break;
                    }
                    t += self.step;
                }
                dark[k] = d * strength;
                glare[k] = rel;
            }
        }
        self.dark = dark;
        self.glare = glare;
    }

    fn sample(&self, grid: &[f64], u: f64, v: f64) -> f64 {
        let (i, j) = ((u.floor() as usize).min(self.nx - 2), (v.floor() as usize).min(self.ny - 2));
        let (fu, fv) = (u - i as f64, v - j as f64);
        let g = |a: usize, b: usize| grid[b * self.nx + a];
        (g(i, j) * (1.0 - fu) + g(i + 1, j) * fu) * (1.0 - fv) + (g(i, j + 1) * (1.0 - fu) + g(i + 1, j + 1) * fu) * fv
    }

    fn lookup(&self, grid: &[f64], p: Pt) -> f64 {
        let u = ((p.0 - self.x0) / self.step).clamp(0.0, (self.nx - 1) as f64);
        let v = ((p.1 - self.y0) / self.step).clamp(0.0, (self.ny - 1) as f64);
        self.sample(grid, u, v)
    }

    /// Outlines broken where the sun hits the surface full on.
    fn break_highlights(&self, strokes: &Strokes, strength: f64) -> Strokes {
        let limit = 1.5 - 0.3 * strength.clamp(0.0, 1.0);
        let mut out = Vec::new();
        for s in strokes {
            let mut run: Vec<Pt> = Vec::new();
            for &q in s {
                if self.lookup(&self.glare, q) > limit {
                    if run.len() >= 2 {
                        out.push(std::mem::take(&mut run));
                    }
                    run.clear();
                } else {
                    run.push(q);
                }
            }
            if run.len() >= 2 {
                out.push(run);
            }
        }
        out
    }

    /// Hatch lines where the surface is dark: each line of the family has
    /// its own threshold (ordered dither), so the darker the shade the more
    /// lines reach into it; a crossing family joins in the deepest shadow.
    fn hatching(&self, area: &Region, l: Light) -> Strokes {
        const ORDER: [f64; 4] = [0.0, 0.5, 0.25, 0.75];
        let angle = l.from_deg - 45.0;
        let mut out = Vec::new();
        for (family, base, span) in [(angle, 0.2, 0.45), (angle + 90.0, 0.62, 0.3)] {
            let (s, c) = family.to_radians().sin_cos();
            for line in crate::sweep::sweep_lines_through(area, l.hatch, family, (0.0, 0.0)) {
                let (a, b) = (line[0], line[line.len() - 1]);
                let row = ((-(a.0 + b.0) * 0.5 * s + (a.1 + b.1) * 0.5 * c) / l.hatch).round() as i64;
                let threshold = base + span * ORDER[row.rem_euclid(4) as usize];
                let len = (b.0 - a.0).hypot(b.1 - a.1);
                let n = (len / (self.step * 0.5)).ceil().max(1.0) as usize;
                let mut run: Vec<Pt> = Vec::new();
                for k in 0..=n {
                    let t = k as f64 / n as f64;
                    let q = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
                    if self.lookup(&self.dark, q) > threshold {
                        run.push(q);
                    } else {
                        flush(&mut run, &mut out, self.step * 2.0);
                    }
                }
                flush(&mut run, &mut out, self.step * 2.0);
            }
        }
        out
    }
}

/// Keep a hatch run as a two-point stroke if it is long enough to read.
fn flush(run: &mut Vec<Pt>, out: &mut Strokes, min: f64) {
    if run.len() >= 2 {
        let (a, b) = (run[0], run[run.len() - 1]);
        if (b.0 - a.0).hypot(b.1 - a.1) >= min {
            out.push(vec![a, b]);
        }
    }
    run.clear();
}

/// One box-blur pass of radius `r` cells along x (`along_x`) or y.
fn blur(src: &[f64], nx: usize, ny: usize, r: usize, along_x: bool) -> Vec<f64> {
    let mut out = vec![0.0; src.len()];
    let (len, lines) = if along_x { (nx, ny) } else { (ny, nx) };
    let idx = |line: usize, k: usize| if along_x { line * nx + k } else { k * nx + line };
    for line in 0..lines {
        let mut sum = 0.0;
        let mut count = 0usize;
        for k in 0..=r.min(len - 1) {
            sum += src[idx(line, k)];
            count += 1;
        }
        for k in 0..len {
            out[idx(line, k)] = sum / count as f64;
            if k + r + 1 < len {
                sum += src[idx(line, k + r + 1)];
                count += 1;
            }
            if k >= r {
                sum -= src[idx(line, k - r)];
                count -= 1;
            }
        }
    }
    out
}

/// Crown sites scattered over the region, thinned into patches by low
/// noise so `coverage` reads as woods and clearings, not an even sprinkle.
/// With `Overflow`, a row of crowns also stands on every boundary ring,
/// whatever the coverage: the canopy starts at the border.
fn generate_crowns(region: &Region, p: &IsolineStackParams, rng: &mut Rng) -> Vec<Crown> {
    let spread = match p.edge {
        EdgeMode::Clip => region.outset(p.crown),
        _ => region.clone(),
    };
    let coverage = p.coverage.clamp(0.0, 1.0);
    let patches = Perlin::new(p.seed.wrapping_add(3));
    let scale = p.crown * 5.0;
    let threshold = 0.6 - 1.2 * coverage;
    let spacing = p.crown * (1.6 - 0.4 * coverage);
    let patch = |c: Pt| coverage >= 1.0 || patches.fbm(c.0 / scale, c.1 / scale, 3) > threshold;
    let mut sites: Vec<Pt> = poisson_disk(&spread, spacing, p.seed).into_iter().filter(|c| patch(*c)).collect();
    match p.edge {
        EdgeMode::Overflow(_) => {
            sites.retain(|c| region.dist_to_boundary(*c) > spacing * 0.6);
            for ring in region.all_rings_mm() {
                sites.extend(along(&ring, spacing * 0.8).into_iter().filter(|c| patch(*c)));
            }
        }
        // A row just inside the border for narrow woods, thinned by the same
        // patch noise as the interior so low `coverage` is not a solid rim.
        EdgeMode::Inside => {
            let inset = p.crown * 0.55;
            sites.retain(|c| region.dist_to_boundary(*c) > inset + spacing * 0.6);
            if coverage >= 0.35 {
                let lining = region.inset(inset);
                let step = spacing * 0.75 / coverage.sqrt();
                for ring in lining.all_rings_mm() {
                    sites.extend(along(&ring, step).into_iter().filter(|c| patch(*c)));
                }
            }
        }
        EdgeMode::Clip => {}
    }
    sites
        .into_iter()
        .map(|at| {
            let radius = p.crown * rng.range(0.7, 1.25);
            Crown { at, radius, height: radius * rng.range(0.6, 1.3) }
        })
        .collect()
}

/// Points every `step` mm around a closed ring.
fn along(ring: &[Pt], step: f64) -> Vec<Pt> {
    let n = ring.len();
    let mut out = Vec::new();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        while s < len {
            let t = s / len;
            out.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
            s += step;
        }
        s -= len;
    }
    out
}

/// A crown as a dome of bubbles: higher and larger towards the middle, so
/// its top layers overhang its own flanks. `light`: a direction and
/// strength bubbles lean away from — their radius and height grow on the
/// far side, shrink on the near side, so rings read dense in the shadow
/// and sparse where lit.
fn crown_balls(c: &Crown, bubble: f64, rng: &mut Rng, light: Option<((f64, f64), f64)>) -> Vec<Ball> {
    let radius = c.radius.max(bubble);
    let n = ((radius / bubble).powi(2) * 1.4).round().clamp(1.0, 90.0) as usize;
    let mut out = Vec::with_capacity(n + 1);
    out.push(Ball { x: c.at.0, y: c.at.1, z: c.height * 0.35, r: radius * 0.6 / VISIBLE });
    for _ in 0..n {
        let t = rng.range(0.0, 1.0).sqrt() * 0.85;
        let a = rng.range(0.0, std::f64::consts::TAU);
        let (sa, ca) = a.sin_cos();
        // `shade` is +1 toward the light, -1 away from it: the far side
        // (shade < 0) is in shadow and swells; the near side thins.
        let shade = light.map_or(0.0, |((lx, ly), strength)| strength * (ca * lx + sa * ly));
        let rb = bubble * rng.range(0.7, 1.3) * (1.15 - 0.4 * t) * (1.0 - shade * 0.65);
        let z = c.height * (1.0 - t * t).sqrt() * rng.range(0.7, 1.0) * (1.0 - shade * 0.5);
        out.push(Ball { x: c.at.0 + radius * t * ca, y: c.at.1 + radius * t * sa, z, r: rb / VISIBLE });
    }
    out
}

/// Shrink (or drop) bubbles whose outline would come near the limit line,
/// `limit` mm outside the boundary, so crowns round off on their own and
/// the fade only catches what merging pushes further: an edge pressed flat
/// by the fade reads as a cut.
fn fit_to_limit(region: &Region, balls: Vec<Ball>, limit: f64) -> Vec<Ball> {
    balls
        .into_iter()
        .filter_map(|mut b| {
            let room = signed_inside(region, (b.x, b.y)) + limit;
            let fits = room / 0.95;
            if fits < b.r * 0.35 {
                return None;
            }
            b.r = b.r.min(fits);
            Some(b)
        })
        .collect()
}

/// Distance to the boundary, negative outside.
fn signed_inside(region: &Region, p: Pt) -> f64 {
    let d = region.dist_to_boundary(p);
    if region.covers_point(p) { d } else { -d }
}

/// A multiplier on the field falling to zero at the limit line, from a
/// coarse grid of distances: outlines close just inside the line instead
/// of being cut by it.
struct Fade {
    x0: f64,
    y0: f64,
    step: f64,
    nx: usize,
    ny: usize,
    room: Vec<f64>,
    band: f64,
}

impl Fade {
    fn new(region: &Region, frame: &Region, limit: f64, bubble: f64) -> Fade {
        let (x0, y0, x1, y1) = frame.bounds_mm();
        let step = (bubble * 0.4).clamp(0.2, 1.5);
        let nx = ((x1 - x0) / step).ceil() as usize + 2;
        let ny = ((y1 - y0) / step).ceil() as usize + 2;
        let mut room = Vec::with_capacity(nx * ny);
        for j in 0..ny {
            for i in 0..nx {
                room.push(signed_inside(region, (x0 + i as f64 * step, y0 + j as f64 * step)) + limit);
            }
        }
        Fade { x0, y0, step, nx, ny, room, band: bubble * 0.35 }
    }

    fn at(&self, p: Pt) -> f64 {
        let (u, v) = ((p.0 - self.x0) / self.step, (p.1 - self.y0) / self.step);
        let (i, j) = (u.floor().max(0.0) as usize, v.floor().max(0.0) as usize);
        let (i, j) = (i.min(self.nx - 2), j.min(self.ny - 2));
        let (fu, fv) = ((u - i as f64).clamp(0.0, 1.0), (v - j as f64).clamp(0.0, 1.0));
        let r = |a: usize, b: usize| self.room[b * self.nx + a];
        let d = (r(i, j) * (1.0 - fu) + r(i + 1, j) * fu) * (1.0 - fv) + (r(i, j + 1) * (1.0 - fu) + r(i + 1, j + 1) * fu) * fv;
        let t = (d / self.band).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

fn frame_around(balls: &[Ball], pad: f64) -> Region {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for b in balls {
        x0 = x0.min(b.x - b.r);
        y0 = y0.min(b.y - b.r);
        x1 = x1.max(b.x + b.r);
        y1 = y1.max(b.y + b.r);
    }
    let (x0, y0, x1, y1) = (x0 - pad, y0 - pad, x1 + pad, y1 + pad);
    Region::from_ring(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

/// The spheres cut by one slice, as discs `(x, y, r²)`, bucketed so a
/// sample only visits its neighbours.
struct Grid<'a> {
    discs: &'a [(f64, f64, f64)],
    size: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

impl<'a> Grid<'a> {
    fn new(discs: &'a [(f64, f64, f64)]) -> Grid<'a> {
        let size = discs.iter().map(|d| d.2.sqrt()).fold(1e-6, f64::max);
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, d) in discs.iter().enumerate() {
            cells.entry(((d.0 / size).floor() as i64, (d.1 / size).floor() as i64)).or_default().push(i);
        }
        Grid { discs, size, cells }
    }

    /// Sum of `(1 - d²/r²)³` over the discs covering `(x, y)`.
    fn field(&self, x: f64, y: f64) -> f64 {
        let (ci, cj) = ((x / self.size).floor() as i64, (y / self.size).floor() as i64);
        let mut h = 0.0;
        for i in ci - 1..=ci + 1 {
            for j in cj - 1..=cj + 1 {
                let Some(idx) = self.cells.get(&(i, j)) else { continue };
                for &k in idx {
                    let (dx, dy, r2) = (x - self.discs[k].0, y - self.discs[k].1, self.discs[k].2);
                    let q = (dx * dx + dy * dy) / r2;
                    if q < 1.0 {
                        h += (1.0 - q).powi(3);
                    }
                }
            }
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (60.0, 0.0), (60.0, 60.0), (0.0, 60.0)])
    }

    fn params(edge: EdgeMode) -> IsolineStackParams {
        IsolineStackParams { seed: 3, crown: 6.0, bubble: 2.0, coverage: 0.8, slices: 8, edge, ..Default::default() }
    }

    #[test]
    fn draws_inside_the_region() {
        let host = square();
        let out = fill_isoline_stack_with_coverage(&host, &params(EdgeMode::Clip));
        assert!(out.strokes.len() > 20, "only {} strokes", out.strokes.len());
        for p in out.strokes.iter().flatten() {
            assert!(host.covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn inside_mode_never_touches_the_boundary() {
        let host = square();
        let out = fill_isoline_stack_with_coverage(&host, &params(EdgeMode::Inside));
        assert!(!out.strokes.is_empty());
        for p in out.strokes.iter().flatten() {
            assert!(host.dist_to_boundary(*p) > 0.05, "{p:?} reaches the boundary");
        }
    }

    #[test]
    fn overflow_mode_crosses_the_boundary_but_not_far() {
        let host = square();
        let m = 1.5;
        let out = fill_isoline_stack_with_coverage(&host, &params(EdgeMode::Overflow(m)));
        let outside: Vec<&Pt> = out.strokes.iter().flatten().filter(|p| !host.covers_point(**p)).collect();
        assert!(!outside.is_empty(), "nothing bulges past the border");
        for p in outside {
            assert!(host.dist_to_boundary(*p) <= m + 1e-6, "{p:?} too far out");
        }
    }

    #[test]
    fn light_makes_the_shadow_side_denser_than_the_lit_side() {
        // Light from +x (0°): the shadow is on -x, so that half should hold
        // more ink (rings pack tighter, bulges swell) than the +x half. A
        // dead zone around the crown's own centre keeps a bulge that swells
        // past the midline from being counted on the wrong side.
        let crowns = vec![Crown { at: (30.0, 30.0), radius: 8.0, height: 6.0 }];
        let lit = IsolineStackParams {
            crowns: CrownSource::Crowns(crowns),
            light: Some(Light { from_deg: 0.0, strength: 0.9, hatch: 0.0, ..Light::default() }),
            jitter: 0.0,
            ..params(EdgeMode::Clip)
        };
        let out = fill_isoline_stack_with_coverage(&square(), &lit);
        let ink_past = |x_min: f64, x_max: f64| -> f64 {
            out.strokes
                .iter()
                .flat_map(|s| s.windows(2))
                .filter(|w| w[0].0 >= x_min && w[0].0 < x_max && w[1].0 >= x_min && w[1].0 < x_max)
                .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
                .sum()
        };
        let shadow = ink_past(0.0, 24.0);
        let sunlit = ink_past(36.0, 60.0);
        assert!(shadow > sunlit * 1.3, "shadow {shadow:.1} should clearly outweigh the lit side {sunlit:.1}");
    }

    #[test]
    fn shading_hatches_the_side_away_from_the_light() {
        // Light from +x: the hatch belongs on the crown's -x flank.
        let crowns = vec![Crown { at: (30.0, 30.0), radius: 10.0, height: 8.0 }];
        let base = IsolineStackParams { crowns: CrownSource::Crowns(crowns), jitter: 0.0, ..params(EdgeMode::Clip) };
        let plain = fill_isoline_stack_with_coverage(&square(), &base).strokes;
        let lit = IsolineStackParams { light: Some(Light { from_deg: 0.0, ..Light::default() }), ..base };
        let shaded = fill_isoline_stack_with_coverage(&square(), &lit).strokes;
        let hatch: Vec<&Vec<Pt>> = shaded.iter().filter(|s| s.len() == 2).collect();
        assert!(hatch.len() > plain.iter().filter(|s| s.len() == 2).count() + 10, "no shading hatch");
        let (west, east) = hatch.iter().fold((0, 0), |(w, e), s| {
            let x = (s[0].0 + s[1].0) * 0.5;
            if x < 27.0 { (w + 1, e) } else if x > 33.0 { (w, e + 1) } else { (w, e) }
        });
        assert!(west > east * 2, "{west} hatch lines in shadow vs {east} lit");
    }

    #[test]
    fn a_higher_wider_bubble_hides_the_rings_under_it() {
        // A low wide crown, and a smaller one standing higher over its side:
        // none of the low crown's rings may show inside the high one's top.
        let crowns = vec![
            Crown { at: (25.0, 30.0), radius: 10.0, height: 3.0 },
            Crown { at: (35.0, 30.0), radius: 6.0, height: 14.0 },
        ];
        let p = IsolineStackParams { crowns: CrownSource::Crowns(crowns), jitter: 0.0, ..params(EdgeMode::Clip) };
        let out = fill_isoline_stack_with_coverage(&square(), &p);
        let core = Region::from_ring(&[(34.0, 29.0), (36.0, 29.0), (36.0, 31.0), (34.0, 31.0)]);
        let inside_core = out.strokes.iter().flatten().filter(|q| core.covers_point(**q)).count();
        assert!(out.strokes.len() > 5);
        assert!(inside_core < 40, "{inside_core} points under the top bubble");
    }
}
