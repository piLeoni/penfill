//! Clouds seen from above: a sheet of cumulus the way it lies under a plane
//! — masses of cloud and clear sky between, ragged at their edges, drawn
//! out along the wind.
//!
//! The sky is the usual cloud field: fractal Perlin noise (fbm), its domain
//! warped twice by more of it (Quilez's domain warping) so the masses curl
//! and drift instead of sitting as blobs, stretched along the wind. Where it
//! passes a threshold (`coverage` of the sky) there is cloud, and how far
//! past is how thick. The cloud is heaped from round puffs, big and high in
//! the thick cores, small and low at the fringes. Every puff shows only the
//! arc of its rim that no higher puff covers, on its side away from the sun
//! — the lumpy, cauliflower look of cumulus — and is hatched in a crescent
//! where it turns from the sun.
//!
//! [`fill_clouds_with_coverage`] also returns what the clouds cover, so a
//! map can clear what lies under them and, moved down-sun, cast their shadow.

use crate::geo::{Pt, Region, Strokes};
use crate::isoline_blobs::FillWithCoverage;
use crate::noise::Perlin;
use crate::random::{poisson_disk, Rng};
use std::f64::consts::{PI, TAU};

/// How the clouds look; every length in mm.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudParams {
    pub seed: u64,
    /// A typical puff's width: the scale of a cloud's lumps.
    pub size: f64,
    /// The scale of the cloud masses and the clear gaps between them. 0: eight puffs.
    pub cell: f64,
    /// 0 – 1: the share of the sky that is cloud.
    pub coverage: f64,
    /// 0 – 1: how much the masses curl and drift (domain warping); 0, plain fbm blobs.
    pub warp: f64,
    /// How closely puffs crowd, about 0.6 (few, round) to 2 (many, lumpy).
    pub puffs: f64,
    /// 0 – 1: how drawn out each puff is, along the run of the cloud round it
    /// (most at the ragged fringes); 0, round puffs.
    pub elongate: f64,
    /// 0 – 1: how far the field is drawn out along the wind (1: cloud streets).
    pub stretch: f64,
    /// The wind's direction on the page, degrees (0: left to right).
    pub wind_deg: f64,
    /// Where the sun is, degrees on the page (0: from the right, 90: from below).
    pub light_deg: f64,
    /// How high the sun stands, degrees: lower, longer shadows across the cloud.
    pub light_elevation: f64,
    /// How tall the cloud heaps up, against its width: 1 a cumulus, 0.3 a flat sheet.
    pub height: f64,
    /// 0 – 1: how much of the cloud is shaded — the flanks turned from the
    /// sun and what higher cloud shadows; 0, no shading.
    pub shade: f64,
    /// Spacing of the shading at its deepest: between dots, or between lines.
    pub hatch: f64,
    /// Shade in dots (stippled, thicker where deeper) rather than hatched lines.
    pub dots: bool,
    /// Each dot's length, mm: a pen's dot is a short stroke.
    pub dot: f64,
}

impl Default for CloudParams {
    fn default() -> Self {
        CloudParams {
            seed: 1,
            size: 6.0,
            cell: 0.0,
            coverage: 0.5,
            warp: 0.25,
            puffs: 1.0,
            elongate: 0.35,
            stretch: 0.25,
            wind_deg: 15.0,
            light_deg: 225.0,
            light_elevation: 30.0,
            height: 1.0,
            shade: 0.45,
            hatch: 0.45,
            dots: true,
            dot: 0.12,
        }
    }
}

/// One puff: a disc, and how high it rises (higher puffs are drawn over lower).
#[derive(Debug, Clone, Copy)]
struct Puff {
    c: Pt,
    r: f64,
    h: f64,
    /// Where on the noise its rim's wobble is read.
    w: f64,
    /// Long over short axis (1: round), and which way the long one points.
    a: f64,
    rot: f64,
    /// Its base, sitting on the cloud's body below it; `h` is its top.
    z: f64,
}

impl Puff {
    /// Its half long axis: as far as its rim reaches from its centre.
    fn reach(&self) -> f64 {
        self.r * self.a.sqrt() * 1.12
    }
}

/// The clouds over `region`, and what they cover.
pub fn fill_clouds_with_coverage(region: &Region, p: &CloudParams) -> FillWithCoverage {
    if region.is_empty() || !(p.size > 0.0) || p.coverage <= 0.0 {
        return FillWithCoverage { strokes: vec![], coverage: Region::default() };
    }
    let (puffs, ground) = place_puffs(region, p);
    let (strokes, body) = draw_puffs(region, &puffs, &ground, p);
    FillWithCoverage { strokes: region.clip_strokes(&strokes), coverage: body.intersection(region) }
}

/// The sky's thickness at a point: higher is cloudier.
struct Sky {
    base: Perlin,
    qx: Perlin,
    qy: Perlin,
    rx: Perlin,
    ry: Perlin,
    cell: f64,
    warp: f64,
    /// Along the wind.
    along: Pt,
    stretch: f64,
}

impl Sky {
    fn new(p: &CloudParams) -> Sky {
        let cell = if p.cell > 0.0 { p.cell } else { p.size * 8.0 };
        let w = p.wind_deg.to_radians();
        let at = |k: u64| Perlin::new(p.seed.wrapping_mul(31).wrapping_add(k));
        Sky {
            base: at(11),
            qx: at(13),
            qy: at(17),
            rx: at(19),
            ry: at(23),
            cell,
            warp: p.warp.clamp(0.0, 1.0),
            along: (w.cos(), w.sin()),
            stretch: p.stretch.clamp(0.0, 0.95),
        }
    }

    fn at(&self, q: Pt) -> f64 {
        // Into the wind's frame, drawn out along it.
        let u = q.0 * self.along.0 + q.1 * self.along.1;
        let v = -q.0 * self.along.1 + q.1 * self.along.0;
        let (x, y) = (u / (self.cell * (1.0 + 2.0 * self.stretch)), v / (self.cell * (1.0 - 0.3 * self.stretch)));
        // fbm warped by fbm warped by fbm.
        let k = 3.0 * self.warp;
        let (q1, q2) = (self.qx.fbm(x, y, 4), self.qy.fbm(x + 5.2, y + 1.3, 4));
        let (r1, r2) = (self.rx.fbm(x + k * q1 + 1.7, y + k * q2 + 9.2, 4), self.ry.fbm(x + k * q1 + 8.3, y + k * q2 + 2.8, 4));
        self.base.fbm(x + k * r1, y + k * r2, 6)
    }
}

/// Puffs over the sky, where it is cloud: spread evenly, big and high where
/// the field is thick, small and low near its edge.
/// The cloud's body: the large-scale heap the puffs sit on. Its height is
/// how far the sky's field passes the threshold, domed.
struct Ground {
    sky: Sky,
    threshold: f64,
    top: f64,
    /// The height of the thickest cloud, mm.
    tall: f64,
}

impl Ground {
    /// 0 – 1: how far into the cloud `q` is (0 at its edge or clear sky).
    fn depth(&self, q: Pt) -> f64 {
        ((self.sky.at(q) - self.threshold) / (self.top - self.threshold)).clamp(0.0, 1.0)
    }

    fn height(&self, q: Pt) -> f64 {
        self.tall * self.depth(q).sqrt()
    }
}

fn place_puffs(region: &Region, p: &CloudParams) -> (Vec<Puff>, Ground) {
    let (x0, y0, x1, y1) = region.bounds_mm();
    let pad = p.size;
    let frame = Region::from_ring(&[(x0 - pad, y0 - pad), (x1 + pad, y0 - pad), (x1 + pad, y1 + pad), (x0 - pad, y1 + pad)]);
    let sky = Sky::new(p);
    // The threshold that leaves `coverage` of the sky cloud, from a sample.
    let mut sample: Vec<f64> = Vec::new();
    let step = (p.size * 0.5).max((x1 - x0 + y1 - y0) / 400.0);
    let mut y = y0;
    while y <= y1 {
        let mut x = x0;
        while x <= x1 {
            sample.push(sky.at((x, y)));
            x += step;
        }
        y += step;
    }
    sample.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| sample[((sample.len() - 1) as f64 * q).round() as usize];
    let threshold = at((1.0 - p.coverage).clamp(0.0, 1.0));
    let top = at(0.995).max(threshold + 1e-6);
    let gap = p.size * 0.42 / p.puffs.clamp(0.3, 3.0).sqrt();
    let tall = sky.cell * 0.35 * p.height.max(0.0);
    let mut rng = Rng::new(p.seed.wrapping_mul(0x9e37_79b9).wrapping_add(5));
    let puffs: Vec<Puff> = poisson_disk(&frame, gap, p.seed)
        .into_iter()
        .filter_map(|c| {
            let v = sky.at(c);
            (v > threshold).then(|| {
                let t = ((v - threshold) / (top - threshold)).clamp(0.0, 1.0);
                // Thin at the fringe, full in the cores.
                let r = p.size * 0.5 * (0.6 + 0.55 * t.sqrt()) * rng.range(0.85, 1.15);
                // Drawn out along the cloud's run (across the field's slope),
                // the more at its ragged edge.
                // Read across the masses, not the grain, so neighbours run together.
                let e = sky.cell * 0.2;
                let (gx, gy) = (sky.at((c.0 + e, c.1)) - sky.at((c.0 - e, c.1)), sky.at((c.0, c.1 + e)) - sky.at((c.0, c.1 - e)));
                let run = if gx.hypot(gy) > 1e-9 { gy.atan2(gx) + PI / 2.0 } else { p.wind_deg.to_radians() };
                let a = 1.0 + p.elongate.max(0.0) * (0.6 + 0.8 * (1.0 - t)) * rng.range(0.75, 1.2);
                // On the body, a little sunk into it, so the domes merge.
                let z = tall * t.sqrt() - r * 0.35;
                Puff { c, r, h: z + r * 0.9 + rng.range(-0.05, 0.05) * r, w: rng.range(0.0, 1000.0), a, rot: run + rng.range(-0.15, 0.15), z }
            })
        })
        .collect();
    // A puff with fewer than two others overlapping it would float alone
    // like a bubble: cloud is never that thin.
    let cell = 2.0 * puffs.iter().map(Puff::reach).fold(p.size * 0.5, f64::max);
    let key = |c: Pt| ((c.0 / cell).floor() as i64, (c.1 / cell).floor() as i64);
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    for (i, q) in puffs.iter().enumerate() {
        grid.entry(key(q.c)).or_default().push(i);
    }
    let kept: Vec<Puff> = puffs
        .iter()
        .enumerate()
        .filter(|&(i, q)| {
            let (kx, ky) = key(q.c);
            let near = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (kx + dx, ky + dy)))
                .filter_map(|k| grid.get(&k))
                .flatten()
                .filter(|&&j| j != i && (puffs[j].c.0 - q.c.0).hypot(puffs[j].c.1 - q.c.1) < (puffs[j].reach() + q.reach()) * 0.8)
                .count();
            near >= 2
        })
        .map(|(_, q)| *q)
        .collect();
    (kept, Ground { sky, threshold, top, tall })
}

/// The puff's rim between angles `from` and `to` (radians): an ellipse
/// drawn out by `a` along `rot`, its radius wobbling, so no puff is a clean
/// circle or a clean oval.
fn rim(p: &Puff, wobble: &Perlin, from: f64, to: f64) -> Vec<Pt> {
    let n = (((p.r * p.a.sqrt() * 10.0).ceil() as usize).clamp(16, 140) as f64 * (to - from) / TAU).ceil().max(4.0) as usize;
    let (rx, ry) = (p.r * p.a.sqrt(), p.r / p.a.sqrt());
    let (cr, sr) = (p.rot.cos(), p.rot.sin());
    (0..=n)
        .map(|i| {
            let t = from + (to - from) * i as f64 / n as f64;
            let k = 1.0 + 0.1 * wobble.noise(p.w + t.cos() * 1.3, p.w * 0.7 + t.sin() * 1.3) + 0.04 * wobble.noise(p.w + t.cos() * 4.0, t.sin() * 4.0);
            let (x, y) = (rx * k * t.cos(), ry * k * t.sin());
            (p.c.0 + x * cr - y * sr, p.c.1 + x * sr + y * cr)
        })
        .collect()
}

/// The part of the puff's rim facing away from the light `(lx, ly)`.
fn back_rim(p: &Puff, wobble: &Perlin, (lx, ly): Pt) -> Vec<Pt> {
    let mut ring = rim(p, wobble, 0.0, TAU);
    ring.pop();
    let facing = |q: &Pt| (q.0 - p.c.0) * lx + (q.1 - p.c.1) * ly;
    // Start where it faces the light most; the far side is then one run.
    let start = (0..ring.len()).max_by(|&i, &j| facing(&ring[i]).total_cmp(&facing(&ring[j]))).unwrap_or(0);
    ring.rotate_left(start);
    let back: Vec<usize> = (0..ring.len()).filter(|&i| facing(&ring[i]) < 0.0).collect();
    match (back.first(), back.last()) {
        (Some(&a), Some(&b)) => ring[a.saturating_sub(1)..=(b + 1).min(ring.len() - 1)].to_vec(),
        _ => vec![],
    }
}

fn disc(p: &Puff, wobble: &Perlin) -> Vec<Pt> {
    let mut ring = rim(p, wobble, 0.0, TAU);
    ring.pop();
    ring
}

fn polyline_len(pts: &[Pt]) -> f64 {
    pts.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum()
}

/// The cloud as a height field on a grid of `g` mm: the body, and every
/// puff a dome on it (the higher wins). Outside the cloud, 0.
struct Heights {
    x0: f64,
    y0: f64,
    g: f64,
    nx: usize,
    ny: usize,
    v: Vec<f64>,
    /// A typical puff's reach, mm: the scale the shade is read at.
    reach: f64,
}

impl Heights {
    fn build(bounds: (f64, f64, f64, f64), g: f64, puffs: &[Puff], ground: &Ground) -> Heights {
        let (x0, y0, x1, y1) = bounds;
        let nx = ((x1 - x0) / g).ceil() as usize + 1;
        let ny = ((y1 - y0) / g).ceil() as usize + 1;
        let mut v = vec![f64::NEG_INFINITY; nx * ny];
        for q in puffs {
            let (rx, ry) = (q.r * q.a.sqrt(), q.r / q.a.sqrt());
            let (cr, sr) = (q.rot.cos(), q.rot.sin());
            let reach = rx.max(ry);
            let (i0, i1) = (((q.c.0 - reach - x0) / g).floor().max(0.0) as usize, (((q.c.0 + reach - x0) / g).ceil() as usize).min(nx - 1));
            let (j0, j1) = (((q.c.1 - reach - y0) / g).floor().max(0.0) as usize, (((q.c.1 + reach - y0) / g).ceil() as usize).min(ny - 1));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let (dx, dy) = (x0 + i as f64 * g - q.c.0, y0 + j as f64 * g - q.c.1);
                    let (u, w) = ((dx * cr + dy * sr) / rx, (-dx * sr + dy * cr) / ry);
                    let d2 = u * u + w * w;
                    if d2 < 1.0 {
                        let z = q.z + (q.h - q.z) * (1.0 - d2).sqrt();
                        let cell = &mut v[j * nx + i];
                        if z > *cell {
                            *cell = z;
                        }
                    }
                }
            }
        }
        // Under a puff the body rises too: the domes ride on it.
        for j in 0..ny {
            for i in 0..nx {
                let k = j * nx + i;
                if v[k].is_finite() {
                    v[k] = v[k].max(ground.height((x0 + i as f64 * g, y0 + j as f64 * g)));
                }
            }
        }
        let reach = puffs.iter().map(|q| q.r).sum::<f64>() / puffs.len().max(1) as f64;
        Heights { x0, y0, g, nx, ny, v, reach }
    }

    /// The heights averaged over a box `k` cells across each way (cloud
    /// only: clear sky counts as height 0 at the edge).
    fn smoothed(&self, k: usize) -> Vec<f64> {
        let (nx, ny) = (self.nx, self.ny);
        let val = |x: f64| if x.is_finite() { x.max(0.0) } else { 0.0 };
        // Separable box blur, twice, through running sums.
        let mut a: Vec<f64> = self.v.iter().map(|&x| val(x)).collect();
        for _ in 0..2 {
            let mut b = vec![0.0; a.len()];
            for j in 0..ny {
                let mut sum = 0.0;
                for i in 0..nx + k {
                    if i < nx {
                        sum += a[j * nx + i];
                    }
                    if i >= 2 * k + 1 {
                        sum -= a[j * nx + i - 2 * k - 1];
                    }
                    if i >= k && i - k < nx {
                        b[j * nx + i - k] = sum / (2 * k + 1) as f64;
                    }
                }
            }
            for i in 0..nx {
                let mut sum = 0.0;
                for j in 0..ny + k {
                    if j < ny {
                        sum += b[j * nx + i];
                    }
                    if j >= 2 * k + 1 {
                        sum -= b[(j - 2 * k - 1) * nx + i];
                    }
                    if j >= k && j - k < ny {
                        a[(j - k) * nx + i] = sum / (2 * k + 1) as f64;
                    }
                }
            }
        }
        a.iter().zip(&self.v).map(|(&s, &z)| if z.is_finite() { s } else { f64::NEG_INFINITY }).collect()
    }

    fn get(&self, i: isize, j: isize) -> f64 {
        if i < 0 || j < 0 || i as usize >= self.nx || j as usize >= self.ny {
            return f64::NEG_INFINITY;
        }
        self.v[j as usize * self.nx + i as usize]
    }

    /// How dark each cell is, 0 (in the sun) to 1 (deep shade): its slope
    /// turned from the sun, or higher cloud between it and the sun.
    fn shade(&self, (lx, ly): Pt, elevation: f64) -> Vec<f64> {
        let (ce, se) = (elevation.cos(), elevation.sin());
        let rise = elevation.tan().max(0.05);
        let top = self.v.iter().copied().filter(|z| z.is_finite()).fold(0.0, f64::max);
        let steps = ((top / rise) / self.g).ceil() as usize + 1;
        // The slope is read off the heights smoothed over a few puffs, so
        // the shade follows the heap's flanks, not every dome.
        let smooth = self.smoothed((self.reach / self.g).ceil().max(1.0) as usize);
        let sm = |i: isize, j: isize, z: f64| {
            if i < 0 || j < 0 || i as usize >= self.nx || j as usize >= self.ny {
                return z;
            }
            let n = smooth[j as usize * self.nx + i as usize];
            if n.is_finite() { n } else { z }
        };
        let mut out = vec![0.0; self.v.len()];
        for j in 0..self.ny as isize {
            for i in 0..self.nx as isize {
                let z = self.get(i, j);
                if !z.is_finite() {
                    continue;
                }
                let zs = smooth[j as usize * self.nx + i as usize];
                let (hx, hy) = ((sm(i + 1, j, zs) - sm(i - 1, j, zs)) / (2.0 * self.g), (sm(i, j + 1, zs) - sm(i, j - 1, zs)) / (2.0 * self.g));
                let norm = (hx * hx + hy * hy + 1.0).sqrt();
                let lambert = (-hx * lx * ce - hy * ly * ce + se) / norm;
                let turned = ((0.55 - lambert) / 0.55).clamp(0.0, 1.0);
                // Cast: walk toward the sun; any cloud above the ray hides it.
                let mut cast = 0.0;
                for k in 1..steps {
                    let d = k as f64 * self.g;
                    let (si, sj) = ((i as f64 + lx * k as f64).round() as isize, (j as f64 + ly * k as f64).round() as isize);
                    if self.get(si, sj) > z + d * rise + self.g * 0.5 {
                        cast = 1.0;
                        break;
                    }
                }
                out[j as usize * self.nx + i as usize] = turned.max(cast);
            }
        }
        // Soften, so the shade's edges run smooth over the puffs' seams.
        for _ in 0..5 {
            let src = out.clone();
            for j in 1..self.ny - 1 {
                for i in 1..self.nx - 1 {
                    let mut sum = 0.0;
                    for (di, dj) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                        sum += src[(j as isize + dj) as usize * self.nx + (i as isize + di) as usize];
                    }
                    out[j * self.nx + i] = sum / 5.0;
                }
            }
        }
        out
    }

    /// Where `field` passes `level`, as a region (cells outside the grid count as below).
    fn region_above(&self, field: &[f64], level: f64) -> Region {
        let (x0, y0, g, nx, ny) = (self.x0, self.y0, self.g, self.nx, self.ny);
        let bounds = Region::from_ring(&[(x0 - g, y0 - g), (x0 + nx as f64 * g, y0 - g), (x0 + nx as f64 * g, y0 + ny as f64 * g), (x0 - g, y0 + ny as f64 * g)]);
        let f = |q: Pt| {
            let (fi, fj) = ((q.0 - x0) / g, (q.1 - y0) / g);
            if fi < 0.0 || fj < 0.0 || fi > (nx - 1) as f64 || fj > (ny - 1) as f64 {
                return -1.0;
            }
            field[fj.round() as usize * nx + fi.round() as usize]
        };
        let loops: Vec<Vec<Pt>> = crate::field::fill_isolines(&bounds, g, &[level], f).into_iter().filter(|r| r.len() >= 4 && r.first() == r.last()).collect();
        Region::from_rings_even_odd(&loops)
    }
}

/// The clouds' lines and footprint. The outline is the edge of all the puffs
/// together. Inside it, each puff shows the part of its rim that no higher
/// neighbour covers, on its side away from the sun; and the shade of the
/// whole heap — flanks turned from the sun, and what higher cloud shadows —
/// is hatched, twice as close where it is deepest.
fn draw_puffs(region: &Region, puffs: &[Puff], ground: &Ground, p: &CloudParams) -> (Strokes, Region) {
    let wobble = Perlin::new(p.seed.wrapping_add(29));
    let discs: Vec<Vec<Pt>> = puffs.iter().map(|q| disc(q, &wobble)).collect();
    let body = Region::from_polygons(&discs.iter().map(|d| vec![d.clone()]).collect::<Vec<_>>());
    // The outline; a pinhole between puffs is not worth a pen's ring.
    let mut strokes: Strokes = body
        .all_rings_mm()
        .into_iter()
        .filter(|r| polyline_len(r) > p.size * 0.8)
        .map(|mut r| {
            if let Some(&first) = r.first() {
                r.push(first);
            }
            r
        })
        .collect();
    let inside = body.inset(p.hatch.max(0.2) * 0.6);
    // Neighbours by grid, so each puff is checked against the few round it.
    let rmax = puffs.iter().map(Puff::reach).fold(0.0, f64::max);
    let cell = (2.0 * rmax).max(1e-6);
    let key = |c: Pt| ((c.0 / cell).floor() as i64, (c.1 / cell).floor() as i64);
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    for (i, q) in puffs.iter().enumerate() {
        grid.entry(key(q.c)).or_default().push(i);
    }
    let light = p.light_deg.to_radians();
    let (lx, ly) = (light.cos(), light.sin());
    for (i, puff) in puffs.iter().enumerate() {
        let (kx, ky) = key(puff.c);
        let higher: Vec<Vec<Pt>> = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (kx + dx, ky + dy)))
            .filter_map(|k| grid.get(&k))
            .flatten()
            .filter(|&&j| j != i && (puffs[j].h > puff.h || (puffs[j].h == puff.h && j > i)))
            .filter(|&&j| (puffs[j].c.0 - puff.c.0).hypot(puffs[j].c.1 - puff.c.1) < puffs[j].reach() + puff.reach())
            .map(|&j| discs[j].clone())
            .collect();
        // The rim facing away from the sun, inside the cloud, under nothing higher.
        let back = back_rim(puff, &wobble, (lx, ly));
        let pieces = if higher.is_empty() { vec![back] } else { Region::from_polygons(&higher.into_iter().map(|d| vec![d]).collect::<Vec<_>>()).clip_polyline_mode(&back, true) };
        for piece in pieces {
            for arc in inside.clip_polyline(&piece) {
                if polyline_len(&arc) > puff.r * 0.5 {
                    strokes.push(arc);
                }
            }
        }
    }
    // The heap's shade, from its heights.
    let shade = p.shade.clamp(0.0, 1.0);
    if shade > 0.0 && p.hatch > 0.0 && !puffs.is_empty() {
        let (x0, y0, x1, y1) = region.bounds_mm();
        let pad = rmax;
        let g = (p.hatch * 0.6).max((x1 - x0 + y1 - y0) / 1600.0);
        let heights = Heights::build((x0 - pad, y0 - pad, x1 + pad, y1 + pad), g, puffs, ground);
        let dark = heights.shade((lx, ly), p.light_elevation.clamp(5.0, 85.0).to_radians());
        // More shade asked, lower the bar for each tone.
        let (half, deep) = (0.75 - 0.45 * shade, 0.95 - 0.35 * shade);
        let angle = p.light_deg + 90.0;
        let spacing = p.hatch * 2.0;
        // Each tone as patches big enough to hatch: specks of shade are dropped.
        let least = (p.size * 0.6).powi(2);
        let toned = |level: f64| {
            let tone = heights.region_above(&dark, level).intersection(&inside);
            let parts: Vec<Region> = tone
                .parts()
                .into_iter()
                .filter(|part| {
                    let (a, b, c, d) = part.bounds_mm();
                    (c - a) * (d - b) > least
                })
                .collect();
            parts.iter().fold(Region::default(), |acc, part| acc.union(part))
        };
        if p.dots {
            // Dots, from sparse where the shade begins to dense where it is
            // deepest: one tone that thickens, not two.
            let begin = half;
            let tone = toned(begin);
            if !tone.is_empty() {
                let (x0, y0, g, nx, ny) = (heights.x0, heights.y0, heights.g, heights.nx, heights.ny);
                let depth = |q: Pt| {
                    let (fi, fj) = (((q.0 - x0) / g).round(), ((q.1 - y0) / g).round());
                    if fi < 0.0 || fj < 0.0 || fi as usize >= nx || fj as usize >= ny {
                        return 1.0;
                    }
                    let d = dark[fj as usize * nx + fi as usize];
                    // 0 at the deepest (dense), 1 where the shade begins (sparse).
                    1.0 - ((d - begin) / (1.0 - begin)).clamp(0.0, 1.0)
                };
                strokes.extend(crate::stipple::fill_stipple_field(&tone, p.hatch, p.hatch * 3.2, p.dot, p.seed.wrapping_add(41), depth));
            }
            return (strokes, body);
        }
        let half_tone = toned(half);
        if !half_tone.is_empty() {
            strokes.extend(crate::sweep::sweep_lines_through(&half_tone, spacing, angle, (0.0, 0.0)).into_iter().filter(|l| polyline_len(l) > p.hatch * 4.0));
        }
        let deep_tone = toned(deep);
        if !deep_tone.is_empty() {
            // The lines between the half tone's, for twice the density.
            let (nx, ny) = ((angle + 90.0).to_radians().cos(), (angle + 90.0).to_radians().sin());
            strokes.extend(crate::sweep::sweep_lines_through(&deep_tone, spacing, angle, (nx * p.hatch, ny * p.hatch)).into_iter().filter(|l| polyline_len(l) > p.hatch * 4.0));
        }
    }
    (strokes, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(w: f64) -> Region {
        Region::from_ring(&[(0.0, 0.0), (w, 0.0), (w, w), (0.0, w)])
    }

    fn area(r: &Region) -> f64 {
        r.all_rings_mm()
            .iter()
            .map(|ring| ring.windows(2).map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1).sum::<f64>() / 2.0)
            .sum::<f64>()
            .abs()
    }

    #[test]
    fn clouds_stay_inside_and_cover_about_what_was_asked() {
        let sky = square(120.0);
        for coverage in [0.3, 0.6] {
            let out = fill_clouds_with_coverage(&sky, &CloudParams { coverage, ..Default::default() });
            assert!(!out.strokes.is_empty());
            assert!(out.strokes.iter().flatten().all(|q| q.0 >= -1e-6 && q.0 <= 120.0 + 1e-6 && q.1 >= -1e-6 && q.1 <= 120.0 + 1e-6));
            let share = area(&out.coverage) / (120.0 * 120.0);
            assert!((coverage - 0.2..coverage + 0.25).contains(&share), "asked {coverage}, got {share}");
        }
    }

    #[test]
    fn no_coverage_no_clouds_and_the_seed_repeats() {
        let sky = square(60.0);
        assert!(fill_clouds_with_coverage(&sky, &CloudParams { coverage: 0.0, ..Default::default() }).strokes.is_empty());
        let p = CloudParams::default();
        assert_eq!(fill_clouds_with_coverage(&sky, &p).strokes, fill_clouds_with_coverage(&sky, &p).strokes);
    }
}
