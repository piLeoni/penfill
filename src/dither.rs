//! Pictures as pen work: a greyscale image laid over the region and
//! dithered on a grid of cells, `resolution` mm along a line and
//! `line_spacing` mm between lines. A plotter can't draw grey, so every
//! cell is either inked or not; the dither decides which.
//!
//! Dark cells become marks in one of two ways:
//! - [`Marks::Dots`]: a short tick on each dark cell (a pen needs a
//!   stroke to leave a dot);
//! - [`Marks::Lines`]: dark cells next to each other along a line join
//!   into one stroke, and lines run back and forth — the look of line
//!   engraving, and far fewer pen lifts.
//!
//! The picture is placed like a CSS background ([`Placement`]): by default
//! it covers the region's bounds, centred; it can also be contained,
//! stretched, sized in mm, positioned and repeated. Where no copy of the
//! picture falls there is paper, and everything is cut by the outline.
//! Image row 0 is the top (smallest y, SVG's convention); use
//! [`Greyscale::flipped`] for y-up coordinates.

use crate::geo::{Pt, Region, Strokes};
use crate::placement::Placement;
use crate::random::Rng;
use std::str::FromStr;

/// A greyscale image, 0 = black, 1 = white, rows top to bottom.
#[derive(Debug, Clone)]
pub struct Greyscale {
    width: usize,
    height: usize,
    pixels: Vec<f64>,
}

impl Greyscale {
    /// From brightness values in `[0, 1]`, row by row.
    pub fn new(width: usize, height: usize, pixels: Vec<f64>) -> Result<Greyscale, String> {
        if width == 0 || height == 0 || pixels.len() != width * height {
            return Err(format!("{width} × {height} image needs {} values, got {}", width * height, pixels.len()));
        }
        let pixels = pixels.into_iter().map(|v| if v.is_nan() { 1.0 } else { v.clamp(0.0, 1.0) }).collect();
        Ok(Greyscale { width, height, pixels })
    }

    /// From interleaved samples with 1 (grey), 2 (grey + alpha), 3 (RGB) or
    /// 4 (RGBA) channels, each in `[0, max]` — 255 for 8-bit data. Alpha is
    /// laid over white paper.
    pub fn from_channels(width: usize, height: usize, data: &[f64], max: f64) -> Result<Greyscale, String> {
        let n = width * height;
        if n == 0 || !data.len().is_multiple_of(n) || !(1..=4).contains(&(data.len() / n)) {
            return Err(format!("{width} × {height} image: {} samples is not 1–4 channels per pixel", data.len()));
        }
        let ch = data.len() / n;
        let k = if max > 0.0 { 1.0 / max } else { 1.0 };
        let pixels = data
            .chunks(ch)
            .map(|p| {
                let (grey, alpha) = match ch {
                    1 => (p[0] * k, 1.0),
                    2 => (p[0] * k, p[1] * k),
                    3 => (luma(p[0] * k, p[1] * k, p[2] * k), 1.0),
                    _ => (luma(p[0] * k, p[1] * k, p[2] * k), p[3] * k),
                };
                grey * alpha + (1.0 - alpha)
            })
            .collect();
        Greyscale::new(width, height, pixels)
    }

    /// Decode a PNG, JPEG, GIF, BMP or WebP file's bytes.
    #[cfg(feature = "image")]
    pub fn decode(bytes: &[u8]) -> Result<Greyscale, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_luma_alpha32f();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let data: Vec<f64> = img.into_raw().into_iter().map(f64::from).collect();
        Greyscale::from_channels(w, h, &data, 1.0)
    }

    /// Read and decode an image file.
    #[cfg(feature = "image")]
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Greyscale, String> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| format!("{}: {e}", path.as_ref().display()))?;
        Greyscale::decode(&bytes)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Brightness of pixel `(x, y)`.
    pub fn pixel(&self, x: usize, y: usize) -> f64 {
        self.pixels[y * self.width + x]
    }

    /// Upside down: for drawings whose y axis points up.
    pub fn flipped(&self) -> Greyscale {
        let pixels = self.pixels.chunks(self.width).rev().flatten().copied().collect();
        Greyscale { pixels, ..*self }
    }

    /// Bilinear sample at pixel coordinates (pixel centres at `i + 0.5`),
    /// clamped at the edges.
    fn sample(&self, x: f64, y: f64) -> f64 {
        let fx = (x - 0.5).clamp(0.0, (self.width - 1) as f64);
        let fy = (y - 0.5).clamp(0.0, (self.height - 1) as f64);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let top = self.pixel(x0, y0) * (1.0 - tx) + self.pixel(x1, y0) * tx;
        let bottom = self.pixel(x0, y1) * (1.0 - tx) + self.pixel(x1, y1) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

fn luma(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// How grey becomes ink or paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dither {
    /// Ink where darker than 50 %. Posterised, no texture.
    Threshold,
    /// White-noise threshold: grainy, like film.
    Random,
    /// Ordered dither with an `n × n` Bayer matrix (2, 4, 8 or 16): the
    /// crosshatch texture of early computer graphics.
    Bayer(usize),
    /// Clustered-dot ordered dither in `n × n` cells: dots that grow with
    /// the darkness, like newspaper halftone.
    Halftone(usize),
    /// Error diffusion, the classic.
    FloydSteinberg,
    /// Error diffusion that drops a quarter of the error: crisper,
    /// lighter shadows (the original Macintosh look).
    Atkinson,
    /// Error diffusion over a wider neighbourhood: smoother.
    JarvisJudiceNinke,
    /// Like Jarvis–Judice–Ninke, slightly sharper.
    Stucki,
    /// Sierra (three rows): between Floyd–Steinberg and Jarvis.
    Sierra,
}

impl FromStr for Dither {
    type Err = String;

    /// `threshold`, `random`, `bayer` (4 × 4) or `bayer2` … `bayer16`,
    /// `halftone` (4 × 4) or `halftone3` … `halftone16`,
    /// `floyd-steinberg`, `atkinson`, `jarvis`, `stucki`, `sierra`.
    fn from_str(s: &str) -> Result<Dither, String> {
        let s = s.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        let sized = |prefix: &str| -> Option<Result<usize, String>> {
            let rest = s.strip_prefix(prefix)?;
            if rest.is_empty() {
                return Some(Ok(4));
            }
            Some(rest.trim_start_matches('-').parse::<usize>().map_err(|_| format!("bad size in {s:?}")))
        };
        if let Some(n) = sized("bayer") {
            let n = n?;
            return if [2, 4, 8, 16].contains(&n) { Ok(Dither::Bayer(n)) } else { Err(format!("Bayer size must be 2, 4, 8 or 16, not {n}")) };
        }
        if let Some(n) = sized("halftone") {
            let n = n?;
            return if (2..=16).contains(&n) { Ok(Dither::Halftone(n)) } else { Err(format!("halftone size must be 2–16, not {n}")) };
        }
        match s.as_str() {
            "threshold" => Ok(Dither::Threshold),
            "random" | "noise" => Ok(Dither::Random),
            "floyd-steinberg" | "floyd" | "fs" => Ok(Dither::FloydSteinberg),
            "atkinson" => Ok(Dither::Atkinson),
            "jarvis" | "jarvis-judice-ninke" | "jjn" => Ok(Dither::JarvisJudiceNinke),
            "stucki" => Ok(Dither::Stucki),
            "sierra" => Ok(Dither::Sierra),
            _ => Err(format!(
                "unknown dither {s:?}: use threshold, random, bayer, halftone, floyd-steinberg, atkinson, jarvis, stucki or sierra"
            )),
        }
    }
}

/// What a dark cell turns into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marks {
    /// A short tick per dark cell.
    Dots,
    /// Runs of dark cells along a line, one stroke each.
    Lines,
}

impl FromStr for Marks {
    type Err = String;

    fn from_str(s: &str) -> Result<Marks, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dots" | "dot" => Ok(Marks::Dots),
            "lines" | "line" => Ok(Marks::Lines),
            other => Err(format!("marks must be \"dots\" or \"lines\", not {other:?}")),
        }
    }
}

/// Settings for [`fill_dither`]. [`DitherOptions::new`] gives
/// Floyd–Steinberg lines on square cells.
#[derive(Debug, Clone, Copy)]
pub struct DitherOptions {
    /// Cell length along a line, mm.
    pub resolution: f64,
    /// Distance between lines of cells, mm.
    pub line_spacing: f64,
    pub method: Dither,
    pub marks: Marks,
    /// Turns the grid of cells, degrees. The picture stays put.
    pub angle: f64,
    /// Applied to brightness before dithering: above 1 darkens the
    /// midtones, below 1 lightens them.
    pub gamma: f64,
    /// Ink where the picture is light.
    pub invert: bool,
    /// Tick length for [`Marks::Dots`], mm.
    pub mark: f64,
    /// For [`Dither::Random`].
    pub seed: u64,
    /// Size, position and repeat of the picture, as in CSS.
    pub placement: Placement,
}

impl DitherOptions {
    pub fn new(resolution: f64) -> DitherOptions {
        DitherOptions {
            placement: Placement::default(),
            resolution,
            line_spacing: resolution,
            method: Dither::FloydSteinberg,
            marks: Marks::Lines,
            angle: 0.0,
            gamma: 1.0,
            invert: false,
            mark: (resolution * 0.3).max(0.1),
            seed: 0,
        }
    }
}

/// `image` dithered inside `region`.
pub fn fill_dither(region: &Region, image: &Greyscale, opts: &DitherOptions) -> Strokes {
    let (res, pitch) = (opts.resolution, opts.line_spacing);
    if res.is_nan() || pitch.is_nan() || res <= 0.0 || pitch <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let c = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
    let (sin, cos) = opts.angle.to_radians().sin_cos();
    let (u, v) = ((cos, sin), (-sin, cos));

    // Grid over the bounds in the turned frame.
    let corners = [(minx, miny), (maxx, miny), (maxx, maxy), (minx, maxy)];
    let along = |p: Pt, d: Pt| (p.0 - c.0) * d.0 + (p.1 - c.1) * d.1;
    let span = |d: Pt| {
        corners.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &p| (lo.min(along(p, d)), hi.max(along(p, d))))
    };
    let ((u0, u1), (v0, v1)) = (span(u), span(v));
    let nx = ((u1 - u0) / res).ceil().max(1.0) as usize;
    let ny = ((v1 - v0) / pitch).ceil().max(1.0) as usize;
    if nx.saturating_mul(ny) > 50_000_000 {
        return vec![];
    }
    let (u0, v0) = ((u0 + u1 - nx as f64 * res) / 2.0, (v0 + v1 - ny as f64 * pitch) / 2.0);
    let world = |lu: f64, lv: f64| (c.0 + lu * u.0 + lv * v.0, c.1 + lu * u.1 + lv * v.1);

    let (iw, ih) = (image.width as f64, image.height as f64);
    let tile = opts.placement.tile((iw, ih), (minx, miny, maxx, maxy));
    // mm per pixel; a few samples per cell when cells are bigger than pixels.
    let k = (tile.w / iw).min(tile.h / ih).max(1e-9);
    let su = (res / k).ceil().clamp(1.0, 4.0) as usize;
    let sv = (pitch / k).ceil().clamp(1.0, 4.0) as usize;
    let gamma = if opts.gamma > 0.0 { opts.gamma } else { 1.0 };
    let mut ink: Vec<f64> = Vec::with_capacity(nx * ny);
    for j in 0..ny {
        for i in 0..nx {
            let mut sum = 0.0;
            for b in 0..sv {
                for a in 0..su {
                    let lu = u0 + (i as f64 + (a as f64 + 0.5) / su as f64) * res;
                    let lv = v0 + (j as f64 + (b as f64 + 0.5) / sv as f64) * pitch;
                    if let Some((x, y)) = opts.placement.locate(tile, world(lu, lv)) {
                        let bright = image.sample(x * iw, y * ih).powf(gamma);
                        sum += if opts.invert { bright } else { 1.0 - bright };
                    }
                }
            }
            ink.push(sum / (su * sv) as f64);
        }
    }

    let on = dither(&ink, nx, ny, opts.method, opts.seed);

    let mut out: Strokes = Vec::new();
    for j in 0..ny {
        let lv = v0 + (j as f64 + 0.5) * pitch;
        let row = &on[j * nx..(j + 1) * nx];
        let mut pieces: Strokes = Vec::new();
        match opts.marks {
            Marks::Lines => {
                let mut i = 0;
                while i < nx {
                    if !row[i] {
                        i += 1;
                        continue;
                    }
                    let start = i;
                    while i < nx && row[i] {
                        i += 1;
                    }
                    let a = world(u0 + start as f64 * res, lv);
                    let b = world(u0 + i as f64 * res, lv);
                    pieces.extend(region.clip_segment(a, b));
                }
            }
            Marks::Dots => {
                let half = opts.mark.max(0.01) / 2.0;
                for (i, _) in row.iter().enumerate().filter(|(_, &d)| d) {
                    let lu = u0 + (i as f64 + 0.5) * res;
                    pieces.extend(region.clip_segment(world(lu - half, lv), world(lu + half, lv)));
                }
            }
        }
        // Back and forth, so the pen never crosses the page between lines.
        if j % 2 == 1 {
            pieces.reverse();
            pieces.iter_mut().for_each(|s| s.reverse());
        }
        out.extend(pieces);
    }
    out
}

/// Which cells get ink. `ink` is 0 (paper) to 1 (black), row-major.
fn dither(ink: &[f64], nx: usize, ny: usize, method: Dither, seed: u64) -> Vec<bool> {
    let ordered = |m: Vec<f64>, n: usize| -> Vec<bool> {
        (0..ny).flat_map(|j| (0..nx).map(move |i| (i, j))).map(|(i, j)| ink[j * nx + i] > m[(j % n) * n + i % n]).collect()
    };
    match method {
        Dither::Threshold => ink.iter().map(|&d| d > 0.5).collect(),
        Dither::Random => {
            let mut rng = Rng::new(seed);
            ink.iter().map(|&d| d > rng.unit()).collect()
        }
        Dither::Bayer(n) => ordered(bayer(n), n),
        Dither::Halftone(n) => ordered(halftone(n), n),
        Dither::FloydSteinberg => diffuse(ink, nx, ny, &[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)], 16.0),
        Dither::Atkinson => diffuse(ink, nx, ny, &[(1, 0, 1.0), (2, 0, 1.0), (-1, 1, 1.0), (0, 1, 1.0), (1, 1, 1.0), (0, 2, 1.0)], 8.0),
        Dither::JarvisJudiceNinke => diffuse(
            ink,
            nx,
            ny,
            &[
                (1, 0, 7.0), (2, 0, 5.0),
                (-2, 1, 3.0), (-1, 1, 5.0), (0, 1, 7.0), (1, 1, 5.0), (2, 1, 3.0),
                (-2, 2, 1.0), (-1, 2, 3.0), (0, 2, 5.0), (1, 2, 3.0), (2, 2, 1.0),
            ],
            48.0,
        ),
        Dither::Stucki => diffuse(
            ink,
            nx,
            ny,
            &[
                (1, 0, 8.0), (2, 0, 4.0),
                (-2, 1, 2.0), (-1, 1, 4.0), (0, 1, 8.0), (1, 1, 4.0), (2, 1, 2.0),
                (-2, 2, 1.0), (-1, 2, 2.0), (0, 2, 4.0), (1, 2, 2.0), (2, 2, 1.0),
            ],
            42.0,
        ),
        Dither::Sierra => diffuse(
            ink,
            nx,
            ny,
            &[
                (1, 0, 5.0), (2, 0, 3.0),
                (-2, 1, 2.0), (-1, 1, 4.0), (0, 1, 5.0), (1, 1, 4.0), (2, 1, 2.0),
                (-1, 2, 2.0), (0, 2, 3.0), (1, 2, 2.0),
            ],
            32.0,
        ),
    }
}

/// Error diffusion, serpentine (every other row runs right to left with
/// the kernel mirrored) so the error doesn't drift to one side.
fn diffuse(ink: &[f64], nx: usize, ny: usize, kernel: &[(i64, i64, f64)], divisor: f64) -> Vec<bool> {
    let mut buf = ink.to_vec();
    let mut on = vec![false; nx * ny];
    for j in 0..ny {
        let flip = j % 2 == 1;
        for step in 0..nx {
            let i = if flip { nx - 1 - step } else { step };
            let idx = j * nx + i;
            let value = buf[idx];
            let inked = value > 0.5;
            on[idx] = inked;
            let err = value - if inked { 1.0 } else { 0.0 };
            for &(dx, dy, w) in kernel {
                let x = i as i64 + if flip { -dx } else { dx };
                let y = (j as i64) + dy;
                if x >= 0 && (x as usize) < nx && (y as usize) < ny {
                    buf[y as usize * nx + x as usize] += err * w / divisor;
                }
            }
        }
    }
    on
}

/// Bayer thresholds for an `n × n` matrix, n a power of two, in `(0, 1)`.
fn bayer(n: usize) -> Vec<f64> {
    let mut m = vec![0usize];
    let mut size = 1;
    while size < n {
        let mut next = vec![0usize; 4 * size * size];
        for y in 0..size {
            for x in 0..size {
                let v = 4 * m[y * size + x];
                let w = 2 * size;
                next[y * w + x] = v;
                next[y * w + x + size] = v + 2;
                next[(y + size) * w + x] = v + 3;
                next[(y + size) * w + x + size] = v + 1;
            }
        }
        m = next;
        size *= 2;
    }
    let cells = (n * n) as f64;
    m.into_iter().map(|r| (r as f64 + 0.5) / cells).collect()
}

/// Clustered-dot thresholds: cells ranked by distance from the middle of
/// the `n × n` tile, so ink grows as one round dot.
fn halftone(n: usize) -> Vec<f64> {
    let mid = (n as f64 - 1.0) / 2.0;
    let mut order: Vec<usize> = (0..n * n).collect();
    let key = |k: usize| {
        let (x, y) = ((k % n) as f64 - mid, (k / n) as f64 - mid);
        (x * x + y * y, y.atan2(x))
    };
    order.sort_by(|&a, &b| key(a).partial_cmp(&key(b)).unwrap());
    let mut m = vec![0.0; n * n];
    let cells = (n * n) as f64;
    for (rank, k) in order.into_iter().enumerate() {
        m[k] = (rank as f64 + 0.5) / cells;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(side: f64) -> Region {
        Region::from_ring(&[(0.0, 0.0), (side, 0.0), (side, side), (0.0, side)])
    }

    /// Left to right from black to white.
    fn ramp(w: usize, h: usize) -> Greyscale {
        let px = (0..h).flat_map(|_| (0..w).map(move |x| x as f64 / (w - 1) as f64)).collect();
        Greyscale::new(w, h, px).unwrap()
    }

    fn ink_len(s: &Strokes, pred: impl Fn(Pt) -> bool) -> f64 {
        s.iter().filter(|l| pred(l[0])).map(|l| crate::geo::polyline_length(l)).sum()
    }

    #[test]
    fn every_method_follows_the_tone() {
        let sq = square(40.0);
        let img = ramp(64, 64);
        for name in ["threshold", "random", "bayer", "bayer8", "halftone", "floyd-steinberg", "atkinson", "jarvis", "stucki", "sierra"] {
            let mut o = DitherOptions::new(0.5);
            o.method = name.parse().unwrap();
            let s = fill_dither(&sq, &img, &o);
            let dark = ink_len(&s, |p| p.0 < 12.0);
            let light = ink_len(&s, |p| p.0 > 28.0);
            assert!(dark > light * 3.0, "{name}: dark side {dark:.0} mm vs light side {light:.0} mm");
            for p in s.iter().flatten() {
                assert!(sq.covers_point(*p), "{name}: {p:?} outside");
            }
        }
    }

    #[test]
    fn error_diffusion_keeps_the_average() {
        let grey = Greyscale::new(1, 1, vec![0.75]).unwrap();
        let sq = square(20.0);
        let mut o = DitherOptions::new(0.5);
        o.marks = Marks::Dots;
        let cells = (20.0 / 0.5_f64).powi(2);
        let n = fill_dither(&sq, &grey, &o).len() as f64;
        assert!((n / cells - 0.25).abs() < 0.03, "{:.3} of cells inked, want 0.25", n / cells);
    }

    #[test]
    fn lines_join_runs_and_turn_back() {
        let black = Greyscale::new(1, 1, vec![0.0]).unwrap();
        let sq = square(10.0);
        let o = DitherOptions { line_spacing: 1.0, ..DitherOptions::new(0.5) };
        let s = fill_dither(&sq, &black, &o);
        assert_eq!(s.len(), 10, "one stroke per line");
        assert!(s[0][0].0 < s[0][1].0 && s[1][0].0 > s[1][1].0, "second line runs back");
    }

    #[test]
    fn ordered_matrices_cover_every_level_once() {
        for m in [bayer(8), halftone(6)] {
            let mut v = m.clone();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let n = v.len() as f64;
            assert!(v.iter().enumerate().all(|(r, &t)| (t - (r as f64 + 0.5) / n).abs() < 1e-12));
        }
    }

    #[test]
    fn channels_and_flip() {
        let rgba = [255.0, 0.0, 0.0, 255.0, 0.0, 0.0, 0.0, 0.0];
        let g = Greyscale::from_channels(2, 1, &rgba, 255.0).unwrap();
        assert!((g.pixel(0, 0) - 0.2126).abs() < 1e-9, "red is dark-ish");
        assert_eq!(g.pixel(1, 0), 1.0, "transparent is paper");
        let tall = Greyscale::new(1, 2, vec![0.0, 1.0]).unwrap().flipped();
        assert_eq!((tall.pixel(0, 0), tall.pixel(0, 1)), (1.0, 0.0));
        assert!(Greyscale::new(2, 2, vec![0.0; 3]).is_err());
        assert!("bayer3".parse::<Dither>().is_err());
        assert_eq!("Floyd Steinberg".parse::<Dither>(), Ok(Dither::FloydSteinberg));
    }

    #[test]
    fn placement_repeats_and_leaves_paper() {
        let sq = square(40.0);
        let black = Greyscale::new(1, 1, vec![0.0]).unwrap();
        let place = |size: &str, pos: &str, repeat: &str| DitherOptions {
            placement: Placement::parse(size, pos, repeat).unwrap(),
            ..DitherOptions::new(0.5)
        };
        let inked = |o: &DitherOptions| -> f64 { fill_dither(&sq, &black, o).iter().map(|s| crate::geo::polyline_length(s)).sum() };
        let full = inked(&place("cover", "center", "no-repeat"));
        let corner = inked(&place("20", "top left", "no-repeat"));
        let strip = inked(&place("10", "top left", "repeat-x"));
        assert!((full - 40.0 * 80.0).abs() < 1.0, "cover inks every line: {full}");
        assert!((corner / full - 0.25).abs() < 0.02, "a 20 mm copy in the corner: {corner}");
        assert!((strip / full - 0.25).abs() < 0.02, "one 10 mm row repeated across: {strip}");
        let s = fill_dither(&sq, &black, &place("20", "top left", "no-repeat"));
        assert!(s.iter().flatten().all(|p| p.0 <= 20.0 + 1e-6 && p.1 <= 20.0 + 1e-6), "ink only under the picture");

        // A ramp repeated three times across: dark, light, dark, light…
        let ramp = ramp(32, 32);
        let o = DitherOptions { marks: Marks::Dots, ..place("33.3%", "top left", "repeat") };
        let s = fill_dither(&sq, &ramp, &o);
        let at = |x0: f64, x1: f64| ink_len(&s, |p| p.0 >= x0 && p.0 < x1);
        assert!(at(14.0, 16.0) > 1.0 && at(14.0, 16.0) > 3.0 * at(24.0, 26.0), "second copy starts dark again");
    }

    #[cfg(feature = "image")]
    #[test]
    fn decodes_png() {
        let mut png = Vec::new();
        let img = image::GrayImage::from_fn(4, 2, |x, _| image::Luma([(x * 85) as u8]));
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let g = Greyscale::decode(&png).unwrap();
        assert_eq!((g.width(), g.height()), (4, 2));
        assert!((g.pixel(3, 1) - 1.0).abs() < 1e-6 && g.pixel(0, 0) < 1e-6);
    }
}
