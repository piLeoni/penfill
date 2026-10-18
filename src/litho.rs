//! Geological map patterns as a small pattern language.
//!
//! One module-level kit — lattices, marks, modifiers — and then thin public
//! entries named after the lithology of the foglio: `granite`, `gneiss`,
//! `diagonal_hatch`, `checker`, and so on. Every entry clips to the region.

use crate::concentric::signed_area;
use crate::geo::{Pt, Region, Strokes};
use crate::random::{poisson_disk, Rng};

/// One seed for both the shapes and the index tie-breaks.
fn rand(seed: u64, salt: u64) -> u64 {
    seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

// ---------------------------------------------------------------------------
// lattice
// ---------------------------------------------------------------------------

/// Regular grid, columns at `dx`, rows at `dy`, `offset` shifts odd rows.
fn stagger_grid(
    region: &Region,
    dx: f64,
    dy: f64,
    offset: f64,
    pad: f64,
    mut f: impl FnMut(i64, i64, Pt),
) {
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let j0 = ((miny - pad) / dy).floor() as i64;
    let j1 = ((maxy + pad) / dy).ceil() as i64;
    let i0 = ((minx - pad) / dx).floor() as i64;
    let i1 = ((maxx + pad) / dx).ceil() as i64;
    for j in j0..=j1 {
        let shift = if j.rem_euclid(2) == 1 { offset } else { 0.0 };
        for i in i0..=i1 {
            f(i, j, (i as f64 * dx + shift, j as f64 * dy));
        }
    }
}

fn grid(region: &Region, dx: f64, dy: f64, pad: f64, f: impl FnMut(i64, i64, Pt)) {
    stagger_grid(region, dx, dy, 0.0, pad, f)
}

/// `f` for each lattice point inside the region, with an optional jitter.
/// Each cell `(i, j)` draws from its own [`crate::anchor::cell_seed`] stream so
/// neighbouring regions share a continuous lattice.
fn inside_grid_seeded(
    region: &Region,
    dx: f64,
    dy: f64,
    offset: f64,
    jitter: f64,
    seed: u64,
    mut f: impl FnMut(&mut Rng, i64, i64, Pt),
) {
    stagger_grid(region, dx, dy, offset, dx.max(dy) + jitter.abs(), |i, j, (x, y)| {
        let mut rng = Rng::new(crate::anchor::cell_seed(seed, i, j));
        let c = (x + rng.range(-jitter, jitter), y + rng.range(-jitter, jitter));
        if region.covers_point(c) {
            f(&mut rng, i, j, c);
        }
    });
}

// ---------------------------------------------------------------------------
// marks
// ---------------------------------------------------------------------------

/// Everything in the mark builders below goes through these two, so clipping
/// is the region's job and never the caller's.
fn stroke(region: &Region, out: &mut Strokes, pts: &[Pt]) {
    out.extend(region.clip_polyline(pts));
}

fn seg(region: &Region, out: &mut Strokes, a: Pt, b: Pt) {
    out.extend(region.clip_segment(a, b));
}

/// Straight tick through `c` at `a` radians, half length `half`.
fn tick(region: &Region, out: &mut Strokes, c: Pt, a: f64, half: f64) {
    let (ca, sa) = a.sin_cos();
    seg(region, out, (c.0 - ca * half, c.1 - sa * half), (c.0 + ca * half, c.1 + sa * half));
}

/// `+` at `c`: vertical then horizontal, arms of half length `half`.
fn cross(region: &Region, out: &mut Strokes, c: Pt, half: f64, twist: f64) {
    tick(region, out, c, std::f64::consts::FRAC_PI_2 + twist, half);
    tick(region, out, c, twist, half);
}

/// `×` at `c`, the diagonal cross.
fn x_cross(region: &Region, out: &mut Strokes, c: Pt, half: f64, twist: f64) {
    tick(region, out, c, std::f64::consts::FRAC_PI_4 + twist, half);
    tick(region, out, c, -std::f64::consts::FRAC_PI_4 + twist, half);
}

/// `T` at `c`, stem along `a`, cap `cap` long, stem `stem` long.
fn tee(region: &Region, out: &mut Strokes, c: Pt, a: f64, stem: f64, cap: f64) {
    let (ca, sa) = a.sin_cos();
    let base = (c.0 - ca * stem * 0.4, c.1 - sa * stem * 0.4);
    let top = (c.0 + ca * stem * 0.6, c.1 + sa * stem * 0.6);
    seg(region, out, base, top);
    let (cb, sb) = (a + std::f64::consts::FRAC_PI_2).sin_cos();
    seg(region, out, (top.0 - cb * cap, top.1 - sb * cap), (top.0 + cb * cap, top.1 + sb * cap));
}

/// `V` at `c`, opening up by `spread` either side of `a`.
fn vee(region: &Region, out: &mut Strokes, c: Pt, a: f64, arm: f64, spread: f64) {
    let top = (c.0 + a.cos() * arm * 0.4, c.1 + a.sin() * arm * 0.4);
    let base = (top.0 - a.cos() * arm * 0.6, top.1 - a.sin() * arm * 0.6);
    for side in [-1.0, 1.0] {
        let b = a + std::f64::consts::FRAC_PI_2 + side * spread;
        let tip = (base.0 + b.cos() * arm, base.1 + b.sin() * arm);
        seg(region, out, top, tip);
    }
}

/// Orthogonal bracket at `c`: `└`, `┘`, `┐` or `┌`.
fn bracket(region: &Region, out: &mut Strokes, c: Pt, arm: f64, corner: usize) {
    let signs = match corner % 4 {
        0 => (1.0, 1.0),
        1 => (-1.0, 1.0),
        2 => (-1.0, -1.0),
        _ => (1.0, -1.0),
    };
    seg(region, out, (c.0 - signs.0 * arm * 0.5, c.1), (c.0 + signs.0 * arm * 0.5, c.1));
    seg(region, out, c, (c.0, c.1 + signs.1 * arm * 0.5));
}

/// A small blob: circle whose radius wobbles with two harmonics.
fn blob(c: Pt, r: f64, phase: f64, rough: f64, steps: usize) -> Vec<Pt> {
    let mut pts = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = std::f64::consts::TAU * k as f64 / steps as f64;
        let w = (3.0 * t + phase).sin() * 0.7 + (5.0 * t - phase).sin() * 0.3;
        let rr = r * (1.0 + rough * w);
        pts.push((c.0 + rr * t.cos(), c.1 + rr * t.sin()));
    }
    pts
}

/// Round pebble, optionally squashed into an ellipse along `angle`.
fn pebble(c: Pt, r: f64, squash: f64, angle: f64, steps: usize) -> Vec<Pt> {
    let (sa, ca) = angle.sin_cos();
    let mut pts = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = std::f64::consts::TAU * k as f64 / steps as f64;
        let (x, y) = (r * t.cos(), r * squash * t.sin());
        pts.push((c.0 + x * ca - y * sa, c.1 + x * sa + y * ca));
    }
    pts
}

/// Irregular fragment — polygon of 4..7 vertices, radius `r * (0.6..1.15)`.
fn fragment(c: Pt, r: f64, rng: &mut Rng) -> Vec<Pt> {
    let n = 4 + (rng.next_u64() % 4) as usize;
    let phase = rng.range(0.0, std::f64::consts::TAU);
    let steps = (n * 3).min(16);
    let mut pts = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = std::f64::consts::TAU * k as f64 / steps as f64;
        let w = (n as f64 * t + phase).sin() * 0.25;
        let rr = r * (0.85 + w) * rng.range(0.9, 1.1);
        pts.push((c.0 + rr * t.cos(), c.1 + rr * t.sin()));
    }
    pts
}

// ---------------------------------------------------------------------------
// fields
// ---------------------------------------------------------------------------

fn crosses(region: &Region, spacing: f64, seed: u64, diag: bool, jitter: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let half = spacing * 0.34;
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, i: i64, j: i64, c: Pt| {
        let twist = rng.range(-0.25, 0.25);
        if diag {
            x_cross(region, &mut out, c, half * rng.range(0.8, 1.2), twist);
        } else {
            cross(region, &mut out, c, half * rng.range(0.8, 1.2), twist);
        }
        let _ = (i, j);
    };
    if diag {
        inside_grid_seeded(region, spacing, spacing, spacing * 0.5, jitter, rand(seed, 1), &mut f);
    } else {
        inside_grid_seeded(region, spacing, spacing * 0.88, spacing * 0.5, jitter, rand(seed, 3), &mut f);
    }
    out
}

fn tees(region: &Region, spacing: f64, seed: u64, jitter: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let stem = spacing * 0.45;
    let cap = spacing * 0.3;
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let a = std::f64::consts::FRAC_PI_2 + rng.range(-0.35, 0.35);
        tee(region, &mut out, c, a, stem, cap);
    };
    inside_grid_seeded(region, spacing, spacing * 0.9, spacing * 0.5, jitter, rand(seed, 2), &mut f);
    out
}

fn vees(region: &Region, spacing: f64, seed: u64, jitter: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let mut out: Strokes = Vec::new();
    let mut f = |_: &mut Rng, i: i64, j: i64, c: Pt| {
        let flip = (i + j).rem_euclid(2) == 0;
        let a = if flip { std::f64::consts::FRAC_PI_2 } else { -std::f64::consts::FRAC_PI_2 };
        vee(region, &mut out, c, a, spacing * 0.5, 0.55);
    };
    inside_grid_seeded(region, spacing, spacing * 0.9, spacing * 0.5, jitter, rand(seed, 4), &mut f);
    out
}

fn brackets(region: &Region, spacing: f64, seed: u64, jitter: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let mut out: Strokes = Vec::new();
    let mut f = |_: &mut Rng, i: i64, j: i64, c: Pt| bracket(region, &mut out, c, spacing * 0.55, (i + j).rem_euclid(4) as usize);
    inside_grid_seeded(region, spacing, spacing * 0.9, spacing * 0.5, jitter, rand(seed, 5), &mut f);
    out
}

/// Short strokes at `angle`, jittered — the workhorse for crystalline marks.
fn dashes(region: &Region, spacing: f64, angle: f64, seed: u64, len_frac: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let base = angle.to_radians();
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let a = base + rng.range(-0.28, 0.28);
        let half = spacing * len_frac * rng.range(0.4, 0.6);
        tick(region, &mut out, c, a, half);
    };
    inside_grid_seeded(region, spacing, spacing * 0.9, spacing * 0.5, spacing * 0.12, rand(seed, 6), &mut f);
    out
}

/// Rows of short dashes, each row offset half a step — mulch, straw litter.
fn mulch(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(rand(seed, 18));
    let strike = angle.to_radians();
    let (su, cu) = strike.sin_cos();
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let half = (maxx - minx).hypot(maxy - miny) / 2.0 + spacing * 2.0;
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let mut out: Strokes = Vec::new();
    // Rows sit closer than the dash pitch so the litter reads as a mat.
    let step = spacing * 0.62;
    let rows = (half / step).ceil() as i64 + 1;
    for k in -rows..=rows {
        let off = k as f64 * step;
        let stagger = if k.rem_euclid(2) == 0 { 0.0 } else { spacing * 0.5 };
        let base = (cx - cu * half + su * off, cy - su * half - cu * off);
        // Walk in whole cells so the stagger stays aligned along the row.
        let len = spacing * rng.range(0.5, 0.85);
        let mut t = -half + stagger;
        while t < half * 2.0 + spacing {
            let a = (base.0 + cu * t, base.1 + su * t);
            let b = (a.0 + cu * len, a.1 + su * len);
            if region.covers_point(a) || region.covers_point(b) {
                seg(region, &mut out, a, b);
            }
            t += spacing;
        }
    }
    out
}

fn dot_field(region: &Region, spacing: f64, seed: u64, radius: f64, jitter: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let steps = 8;
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let r = radius * rng.range(0.8, 1.2);
        let mut ring = Vec::with_capacity(steps + 1);
        for k in 0..=steps {
            let t = std::f64::consts::TAU * k as f64 / steps as f64;
            ring.push((c.0 + r * t.cos(), c.1 + r * t.sin()));
        }
        stroke(region, &mut out, &ring);
    };
    inside_grid_seeded(region, spacing, spacing * 0.9, spacing * 0.5, jitter, rand(seed, 7), &mut f);
    out
}

fn pebble_field(
    region: &Region,
    spacing: f64,
    seed: u64,
    squash: f64,
    tilt: bool,
    jitter: f64,
) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let steps = 16;
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let r = spacing * rng.range(0.28, 0.46);
        let a = if tilt { rng.range(-0.35, 0.35) } else { 0.0 };
        let ring = pebble(c, r, squash, a, steps);
        stroke(region, &mut out, &ring);
    };
    inside_grid_seeded(region, spacing, spacing * 0.9, spacing * 0.5, jitter, rand(seed, 8), &mut f);
    out
}

fn fragments(region: &Region, spacing: f64, seed: u64, angular: bool) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(rand(seed, 8));
    let field = region.outset(spacing);
    let mut out: Strokes = Vec::new();
    for c in poisson_disk(&field, spacing, seed.wrapping_add(8)) {
        if !region.covers_point(c) {
            continue;
        }
        let r = spacing * rng.range(0.3, 0.48);
        let ring = if angular {
            fragment(c, r, &mut rng)
        } else {
            blob(c, r, rng.range(0.0, std::f64::consts::TAU), 0.22, 14)
        };
        let mut closed = ring;
        if let Some(&first) = closed.first() {
            closed.push(first);
        }
        stroke(region, &mut out, &closed);
    }
    out
}

/// Beds: long wavy horizontals `spacing` apart, each with its own phase.
fn beds(region: &Region, spacing: f64, angle: f64, seed: u64, amp_frac: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(rand(seed, 9));
    let strike = angle.to_radians();
    let (su, cu) = strike.sin_cos();
    let amp = spacing * amp_frac;
    let wave = spacing * rng.range(2.4, 3.6);
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let diag = (maxx - minx).hypot(maxy - miny) + spacing * 6.0;
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let mut out: Strokes = Vec::new();
    let n = ((diag * 2.0) / spacing).ceil() as i64;
    let steps = 24;
    for k in -n..=n {
        let off = k as f64 * spacing;
        let phase = rng.range(0.0, std::f64::consts::TAU);
        let mut pts = Vec::with_capacity(steps + 1);
        for s in 0..=steps {
            let t = -diag + 2.0 * diag * s as f64 / steps as f64;
            let along = (cx + cu * t, cy + su * t);
            let d = off + amp * (t / wave * std::f64::consts::TAU + phase).sin();
            pts.push((along.0 - su * d, along.1 + cu * d));
        }
        stroke(region, &mut out, &pts);
    }
    out
}

/// Dashed bedding — `dash` long pieces separated by near-`dash` gaps.
fn bedding_dashes(region: &Region, spacing: f64, angle: f64, seed: u64, dash: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(rand(seed, 10));
    let strike = angle.to_radians();
    let (su, cu) = strike.sin_cos();
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    // Half-diagonal of the bounding box: enough to cross it along any strike.
    let half = (maxx - minx).hypot(maxy - miny) / 2.0 + spacing * 2.0;
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let mut out: Strokes = Vec::new();
    // Each row starts at the box centre pushed back by `half`, then walks
    // `t ∈ [-half, +half]` along the strike. That only spans `2 * half`
    // *total*, i.e. the segment from `c - half` to `c` — the far half of
    // every row was never emitted, which is why vertical `bentonite` columns
    // filled the bottom of the recipe and horizontal `coal` seams filled
    // only its left side. The walk has to run out to `+2 * half` instead.
    let n = (half / spacing).ceil() as i64 + 1;
    for k in -n..=n {
        let off = k as f64 * spacing;
        let base = (cx - cu * half + su * off, cy - su * half - cu * off);
        let mut t = -half;
        while t < half * 2.0 + spacing {
            t += dash * rng.range(0.4, 1.3);
            let a = (base.0 + cu * t, base.1 + su * t);
            let b = (a.0 + cu * dash, a.1 + su * dash);
            if region.covers_point(a) || region.covers_point(b) {
                seg(region, &mut out, a, b);
            }
            t += dash;
        }
    }
    out
}

/// Varying-width vertical bars: `band` is the widest bar, `gap` the narrowest.
fn bars(region: &Region, band: f64, seed: u64, vertical: bool) -> Strokes {
    if band <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(rand(seed, 11));
    let mut out: Strokes = Vec::new();
    let steps = 10;
    if vertical {
        let (minx, _, maxx, _) = region.bounds_mm();
        let n = ((maxx - minx) / band).ceil() as i64 + 1;
        for i in -1..=n {
            let x = minx + i as f64 * band + rng.range(-0.15, 0.15) * band;
            let (_, miny, _, maxy) = region.bounds_mm();
            let y0 = miny - band;
            let y1 = maxy + band;
            let mut pts = Vec::with_capacity(steps + 1);
            for s in 0..=steps {
                let t = y0 + (y1 - y0) * s as f64 / steps as f64;
                pts.push((x + rng.range(-0.04, 0.04) * band, t));
            }
            stroke(region, &mut out, &pts);
        }
    } else {
        beds(region, band, 0.0, seed.wrapping_add(1), 0.0);
        return out;
    }
    out
}

/// Dense parallel hatch at `angle`, `spacing` apart.
fn hatch(region: &Region, spacing: f64, angle: f64, seed: u64, jitter: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(rand(seed, 12));
    let strike = angle.to_radians();
    let (su, cu) = strike.sin_cos();
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let half = (maxx - minx).hypot(maxy - miny) / 2.0 + spacing * 2.0;
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let mut out: Strokes = Vec::new();
    let n = (2.0 * half / spacing).ceil() as i64 + 1;
    for k in -n..=n {
        let off = k as f64 * spacing + rng.range(-jitter, jitter);
        let a = (cx - cu * half + su * off, cy - su * half - cu * off);
        let b = (cx + cu * half + su * off, cy + su * half - cu * off);
        seg(region, &mut out, a, b);
    }
    out
}

fn checker(region: &Region, cell: f64, _seed: u64) -> Strokes {
    if cell <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut out: Strokes = Vec::new();
    grid(region, cell, cell, cell, |_, _, (x, y)| {
        let ring = [(x, y), (x + cell, y), (x + cell, y + cell), (x, y + cell), (x, y)];
        stroke(region, &mut out, &ring);
    });
    out
}

/// Running-bond masonry: bricks `2 × 1` units, courses offset half a brick.
fn brick(region: &Region, cell: f64, seed: u64) -> Strokes {
    if cell <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (bw, bh) = (cell * 2.0, cell);
    let mut rng = Rng::new(rand(seed, 13));
    let mut out: Strokes = Vec::new();
    stagger_grid(region, bw, bh, bw * 0.5, cell, |_, _, (x, y)| {
        let w = bw * rng.range(0.94, 1.06);
        let h = bh * rng.range(0.94, 1.06);
        let ring = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)];
        stroke(region, &mut out, &ring);
    });
    out
}

/// Concentric ring motifs — diatom cells, `rings` rings plus a centre dot.
fn concentric(region: &Region, spacing: f64, seed: u64, rings: usize) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let steps = 14;
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let r = spacing * rng.range(0.3, 0.42);
        for k in 0..=rings {
            let rr = r * (1.0 - 0.32 * k as f64);
            let mut ring = Vec::with_capacity(steps + 1);
            for s in 0..=steps {
                let t = std::f64::consts::TAU * s as f64 / steps as f64;
                ring.push((c.0 + rr * t.cos(), c.1 + rr * t.sin()));
            }
            stroke(region, &mut out, &ring);
        }
    };
    inside_grid_seeded(region, spacing, spacing * 0.9, spacing * 0.5, spacing * 0.08, rand(seed, 9), &mut f);
    out
}

/// Shell spirals in rows — coquina.
fn spirals(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let steps = 14;
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let a0 = rng.range(0.0, std::f64::consts::TAU);
        let r = spacing * rng.range(0.28, 0.4);
        let mut pts = Vec::with_capacity(steps + 1);
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let rr = r * t;
            let a = a0 + t * std::f64::consts::TAU * 1.4;
            pts.push((c.0 + rr * a.cos(), c.1 + rr * a.sin()));
        }
        stroke(region, &mut out, &pts);
    };
    inside_grid_seeded(region, spacing, spacing * 0.9, spacing * 0.5, spacing * 0.08, rand(seed, 10), &mut f);
    out
}

fn zigzag_field(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let diag = (maxx - minx).hypot(maxy - miny) + spacing * 4.0;
    let mut out: Strokes = Vec::new();
    let a = angle.to_radians();
    let (su, cu) = a.sin_cos();
    let (pa, pb) = (-su, cu);
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let period = spacing * 0.4;
    let amp = spacing * 0.22;
    // Long, thin teeth: many periods along the row. A coarse tooth count per
    // row (as in a sash) reads as a staircase, not a sawtooth.
    let teeth = if period > 0.0 { ((diag * 2.0) / period) as usize } else { 0 };
    if teeth == 0 {
        return out;
    }
    let rows = (diag * 2.0 / spacing).ceil() as i64 + 1;
    for k in -rows..=rows {
        let off = k as f64 * spacing;
        let start = (cx - cu * diag + su * off, cy - su * diag - cu * off);
        let mut pts = Vec::with_capacity(teeth + 2);
        pts.push(start);
        for t in 0..teeth {
            let along = t as f64 * period;
            let side = if t % 2 == 0 { amp } else { -amp };
            pts.push((start.0 + cu * along + pa * side, start.1 + su * along + pb * side));
            pts.push((start.0 + cu * (along + period * 0.5), start.1 + su * (along + period * 0.5)));
        }
        stroke(region, &mut out, &pts);
    }
    let _ = seed;
    out
}

// ---------------------------------------------------------------------------
// public entries — one per lithology we actually use
// ---------------------------------------------------------------------------

/// Igneous `+` on a staggered grid.
pub fn fill_granite(region: &Region, spacing: f64, seed: u64) -> Strokes {
    crosses(region, spacing, seed, false, spacing * 0.08)
}

/// Igneous diagonal `×`.
pub fn fill_granite2(region: &Region, spacing: f64, seed: u64) -> Strokes {
    crosses(region, spacing, seed, true, spacing * 0.08)
}

/// Igneous `T` marks.
pub fn fill_diorite(region: &Region, spacing: f64, seed: u64) -> Strokes {
    tees(region, spacing, seed, spacing * 0.08)
}

/// Igneous `V`s, alternating up and down.
pub fn fill_gabbro(region: &Region, spacing: f64, seed: u64) -> Strokes {
    vees(region, spacing, seed, spacing * 0.08)
}

/// Orthogonal `└`/`┘` brackets.
pub fn fill_brackets(region: &Region, spacing: f64, seed: u64) -> Strokes {
    brackets(region, spacing, seed, spacing * 0.08)
}

/// Diagonal crystal dashes at `angle`.
pub fn fill_dacite(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    dashes(region, spacing, angle, seed, 1.0)
}

/// Dense short crystal strokes, all roughly `angle`.
pub fn fill_feldspar(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    dashes(region, spacing * 0.7, angle, seed, 0.9)
}

/// Fine dot stipple.
pub fn fill_arena(region: &Region, spacing: f64, seed: u64) -> Strokes {
    dot_field(region, spacing, seed, spacing * 0.09, spacing * 0.12)
}

/// Sparse larger dots.
pub fn fill_conglom5(region: &Region, spacing: f64, seed: u64) -> Strokes {
    dot_field(region, spacing, seed, spacing * 0.2, spacing * 0.2)
}

/// Rounded clasts.
pub fn fill_conglomerate(region: &Region, spacing: f64, seed: u64) -> Strokes {
    pebble_field(region, spacing, seed, 1.0, false, spacing * 0.12)
}

/// Horizontally squashed ellipses.
pub fn fill_conglomerate2(region: &Region, spacing: f64, seed: u64) -> Strokes {
    pebble_field(region, spacing, seed, 0.6, true, spacing * 0.15)
}

/// Coarse stone: a few large rounded block outlines — no grain overlay.
pub fn fill_coarse_stone(region: &Region, spacing: f64, seed: u64) -> Strokes {
    voronoi_graded(
        region,
        spacing * 1.9,
        seed,
        spacing * 0.14,
        0.42,
        spacing * 0.1,
    )
}

/// Mixed rocks: Voronoi cells spanning a wide size range, no grain.
pub fn fill_mixed_rocks(region: &Region, spacing: f64, seed: u64) -> Strokes {
    voronoi_graded(region, spacing, seed, spacing * 0.05, 0.55, 0.06)
}

/// Cracked earth: tight angular cells over a fine granular dot field.
pub fn fill_cracked_earth(region: &Region, spacing: f64, seed: u64) -> Strokes {
    let mut out = dot_field(region, spacing * 0.38, seed, spacing * 0.042, spacing * 0.22);
    out.extend(voronoi(region, spacing * 1.05, seed.wrapping_add(1), spacing * 0.035, 0.0));
    out
}

/// Chalky soil: pale granular field with a very light, open cell net.
pub fn fill_chalky_soil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    let mut out = dot_field(region, spacing * 0.42, seed, spacing * 0.038, spacing * 0.28);
    out.extend(voronoi(region, spacing * 1.75, seed.wrapping_add(2), spacing * 0.025, 0.0));
    out
}

/// Loose soil: open scatter of small rings and specks, mostly paper.
pub fn fill_loose_soil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        if rng.unit() > 0.45 {
            return;
        }
        let r = spacing * rng.range(0.06, 0.15);
        let ring = pebble(c, r, rng.range(0.7, 1.0), 0.0, 6);
        let mut closed = ring;
        if let Some(&first) = closed.first() {
            closed.push(first);
        }
        stroke(region, &mut out, &closed);
    };
    inside_grid_seeded(region, spacing * 0.85, spacing * 0.8, spacing * 0.42, spacing * 0.5, rand(seed, 31), &mut f);
    out
}

/// Compacted soil: tight, thin parallel waves pressed together.
pub fn fill_compacted_soil(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    beds(region, spacing * 0.34, angle, seed, 0.3)
}

/// Sandy soil: fine dense dot grain — tighter and smaller than gravelly soil.
pub fn fill_sandy_soil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    dot_field(region, spacing * 0.5, seed, spacing * 0.045, spacing * 0.35)
}

/// Mulched soil: rows of short dashes; denser and finer than mulch.
pub fn fill_mulched_soil(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    mulch(region, spacing * 0.55, angle, seed.wrapping_add(5))
}

/// Debris soil: tiny chips and specks from crushed rock.
pub fn fill_debris_soil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        if rng.unit() > 0.5 {
            return;
        }
        let r = spacing * rng.range(0.05, 0.12);
        let ring = pebble(c, r, rng.range(0.6, 1.0), rng.range(0.0, std::f64::consts::TAU), 5);
        let mut closed = ring;
        if let Some(&first) = closed.first() {
            closed.push(first);
        }
        stroke(region, &mut out, &closed);
    };
    inside_grid_seeded(region, spacing * 0.8, spacing * 0.75, spacing * 0.4, spacing * 0.48, rand(seed, 32), &mut f);
    out
}

/// Sandstone: a dot grain with short cross-beds running through it.
pub fn fill_sandstone(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    let mut out = dot_field(region, spacing * 0.55, seed, spacing * 0.05, spacing * 0.35);
    out.extend(beds(region, spacing * 1.6, angle, seed.wrapping_add(7), 0.5));
    out
}

/// Grainy soil: dense fine dot field, slightly irregular.
pub fn fill_grainy_soil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    dot_field(region, spacing, seed, spacing * 0.07, spacing * 0.32)
}

/// Organic topsoil: small rings and dots scattered densely, mixed sizes.
///
/// The reference reads as crumbs — mostly tiny open circles with a few
/// larger ones — so the field mixes ring motifs with plain dots rather
/// than drawing uniform circles.
pub fn fill_topsoil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let pick = rng.unit();
        // Two thirds small rings, the rest plain dots, sizes spread.
        let r = spacing * if pick < 0.55 {
            rng.range(0.045, 0.075)
        } else if pick < 0.85 {
            rng.range(0.075, 0.11)
        } else {
            rng.range(0.11, 0.16)
        };
        let steps = 8;
        let mut ring = Vec::with_capacity(steps + 1);
        for k in 0..=steps {
            let t = std::f64::consts::TAU * k as f64 / steps as f64;
            ring.push((c.0 + r * t.cos(), c.1 + r * t.sin()));
        }
        stroke(region, &mut out, &ring);
    };
    inside_grid_seeded(region, spacing * 0.62, spacing * 0.58, spacing * 0.31, spacing * 0.5, rand(seed, 29), &mut f);
    out
}

/// A rock splinter: a long, thin angular sliver at `a` radians, `len` long.
///
/// The reference shows crushed rock as elongated shards with corners, not
/// as compact polygons — an equant `fragment` reads as a dot at this size,
/// so rock debris needs the length to show.
fn splinter(c: Pt, len: f64, wide: f64, a: f64, rng: &mut Rng) -> Vec<Pt> {
    let (sa, ca) = a.sin_cos();
    // Five stations along the shard, each with its own half-width.
    let mut pts = Vec::with_capacity(7);
    let steps = 6;
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let along = (t - 0.5) * len;
        // Taper to a point at both ends, wobbling a little in between.
        let taper = (t * std::f64::consts::PI).sin().max(0.08);
        let w = wide * taper * rng.range(0.7, 1.3);
        let px = along;
        let py = w;
        pts.push((c.0 + px * ca - py * sa, c.1 + px * sa + py * ca));
    }
    // Mirror back along the other side to close the shard.
    for k in (0..=steps).rev() {
        let t = k as f64 / steps as f64;
        let along = (t - 0.5) * len;
        let taper = (t * std::f64::consts::PI).sin().max(0.08);
        let w = -wide * taper * rng.range(0.7, 1.3);
        pts.push((c.0 + along * ca - w * sa, c.1 + along * sa + w * ca));
    }
    pts
}

/// Dry cracked soil: wide mud-crack polygons (rounded gaps), no grain.
pub fn fill_dry_cracked(region: &Region, spacing: f64, seed: u64) -> Strokes {
    crate::textures::fill_cracks(region, spacing, seed)
}

/// Erosion lines: many low-amplitude parallel waves.
pub fn fill_erosion(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    beds(region, spacing, angle, seed, 0.3)
}

/// Dense stone cluster: tight, varied pebbles with almost no gaps.
pub fn fill_stone_cluster(region: &Region, spacing: f64, seed: u64) -> Strokes {
    pebble_field(region, spacing * 0.72, seed, 1.0, true, spacing * 0.16)
}

/// Squashed pebbles tilted off horizontal.
pub fn fill_drift(region: &Region, spacing: f64, seed: u64) -> Strokes {
    pebble_field(region, spacing * 1.4, seed, 0.75, true, spacing * 0.3)
}

/// Angular Voronoi fragments with narrow gaps.
pub fn fill_breccia(region: &Region, spacing: f64, seed: u64) -> Strokes {
    voronoi(region, spacing, seed, spacing * 0.07, 0.0)
}

/// Rounded low-frequency blobs.
pub fn fill_breccia2(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fragments(region, spacing, seed, false)
}

/// Small angular shards, using the elongated rock-splinter shape.
pub fn fill_shards(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(rand(seed, 27));
    let field = region.outset(spacing);
    let mut out: Strokes = Vec::new();
    for c in poisson_disk(&field, spacing * 0.66, seed.wrapping_add(27)) {
        if !region.covers_point(c) {
            continue;
        }
        let len = spacing * rng.range(0.6, 1.2);
        let wide = spacing * rng.range(0.09, 0.17);
        let a = rng.range(0.0, std::f64::consts::TAU);
        let mut ring = splinter(c, len, wide, a, &mut rng);
        if let Some(&first) = ring.first() {
            ring.push(first);
        }
        stroke(region, &mut out, &ring);
    }
    out
}

/// Bedded: continuous wavy horizons.
pub fn fill_beds(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    beds(region, spacing, angle, seed, 0.22)
}

/// Fine wavy laminations, low amplitude.
pub fn fill_laminae(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    beds(region, spacing * 0.5, angle, seed, 0.35)
}

/// Dashed bedding / coal seams.
pub fn fill_coal(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    bedding_dashes(region, spacing, angle, seed, spacing * 0.6)
}

/// Gneiss foliation: dash rows along the strike, longer pieces.
pub fn fill_gneiss(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    bedding_dashes(region, spacing * 0.55, angle, seed, spacing * 1.1)
}

/// Vertical dashed columns.
pub fn fill_benton(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    bedding_dashes(region, spacing, angle, seed, spacing * 0.7)
}

/// Running-bond brick.
pub fn fill_masonry(region: &Region, spacing: f64, seed: u64) -> Strokes {
    brick(region, spacing, seed)
}

/// Checkerboard cells.
pub fn fill_ferrous(region: &Region, spacing: f64, seed: u64) -> Strokes {
    checker(region, spacing, seed)
}

/// Continuous vertical zigzag — amphibolite.
pub fn fill_amphibolite(region: &Region, spacing: f64, seed: u64) -> Strokes {
    zigzag_field(region, spacing, std::f64::consts::FRAC_PI_2, seed)
}

/// Concentric micro-motifs — diatom.
pub fn fill_diatom(region: &Region, spacing: f64, seed: u64) -> Strokes {
    concentric(region, spacing, seed, 1)
}

/// Denser concentric motifs.
pub fn fill_diatom2(region: &Region, spacing: f64, seed: u64) -> Strokes {
    concentric(region, spacing * 0.8, seed, 2)
}

/// Shell spirals in rows.
pub fn fill_coquina(region: &Region, spacing: f64, seed: u64) -> Strokes {
    spirals(region, spacing, seed)
}

/// Dense diagonal hatch.
pub fn fill_diabase(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    hatch(region, spacing, angle, seed, spacing * 0.05)
}

/// Ophitic diabase: dense plagioclase laths crossed by pyroxene grain.
///
/// The bare hatch reads as nothing but parallel lines; real diabase is a
/// mesh, so this adds short transverse crystallites across the laths and a
/// scatter of granular dots in the interstices.
pub fn fill_ophitic(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    let mut out = hatch(region, spacing, angle, seed, spacing * 0.06);
    // Transverse crystallites, sparse enough that the laths stay readable.
    let base = (angle + 90.0).to_radians();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        if rng.unit() > 0.25 {
            return;
        }
        let half = spacing * rng.range(0.7, 1.4);
        tick(region, &mut out, c, base + rng.range(-0.35, 0.35), half);
    };
    inside_grid_seeded(region, spacing * 2.4, spacing * 2.0, spacing * 1.0, spacing * 0.8, rand(seed, 15), &mut f);
    // Granular pyroxene between the laths.
    out.extend(dot_field(region, spacing * 1.8, seed.wrapping_add(2), spacing * 0.05, spacing * 0.42));
    out
}

/// Gravelly soil: dense short dashes at scattered angles.
pub fn fill_gravelly_soil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let a = rng.range(0.0, std::f64::consts::TAU);
        let half = spacing * rng.range(0.2, 0.4);
        tick(region, &mut out, c, a, half);
    };
    inside_grid_seeded(region, spacing * 0.6, spacing * 0.55, spacing * 0.3, spacing * 0.4, rand(seed, 20), &mut f);
    out
}

/// Mulched soil: dense rows of short dashes with a blown offset.
pub fn fill_mulch(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    mulch(region, spacing, angle, seed)
}

/// Layered soil: tightly stacked horizontal bands.
pub fn fill_layered_soil(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    // The reference stacks many thin horizons rather than a few broad ones,
    // so the beds sit at a fraction of the nominal spacing.
    let mut out = beds(region, spacing * 0.3, angle, seed, 0.1);
    // A few straighter partings break the even stack so it reads as layering
    // rather than as a single sheet of ripples.
    out.extend(hatch(region, spacing * 0.9, angle, seed.wrapping_add(3), 0.0));
    out
}

/// Pebbled soil: sparse small squares and diamonds.
pub fn fill_pebbled_soil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        if rng.unit() > 0.6 {
            return;
        }
        let r = spacing * rng.range(0.16, 0.3);
        let a = std::f64::consts::FRAC_PI_4 * if rng.coin() { 1.0 } else { 0.0 };
        let ring = pebble(c, r, rng.range(0.75, 1.0), a, 4);
        let mut closed = ring;
        if let Some(&first) = closed.first() {
            closed.push(first);
        }
        stroke(region, &mut out, &closed);
    };
    inside_grid_seeded(region, spacing * 0.72, spacing * 0.68, spacing * 0.36, spacing * 0.42, rand(seed, 25), &mut f);
    out
}
/// Clay soil: broad waves with a wider calmer profile than `laminae`.
pub fn fill_clay_soil(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    beds(region, spacing, angle, seed, 0.55)
}

/// Silty soil: small tight polygonal cells, no grain.
pub fn fill_silty_soil(region: &Region, spacing: f64, seed: u64) -> Strokes {
    voronoi_graded(region, spacing * 0.5, seed, spacing * 0.04, 0.25, 0.0)
}

/// Fine gravel: sparse dots and small rings on a loose grid.
pub fn fill_fine_gravel(region: &Region, spacing: f64, seed: u64) -> Strokes {
    let mut out = dot_field(region, spacing, seed, spacing * 0.035, spacing * 0.42);
    out.extend(dot_field(region, spacing * 1.6, seed.wrapping_add(1), spacing * 0.075, spacing * 0.45));
    out
}

/// Pebble scatter: well-spaced, lightly tilted ovals.
pub fn fill_pebble_scatter(region: &Region, spacing: f64, seed: u64) -> Strokes {
    pebble_field(region, spacing, seed, 0.82, true, spacing * 0.32)
}

/// Cross hatch, woven.
pub fn fill_fill(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    let mut out = hatch(region, spacing, angle, seed, spacing * 0.04);
    out.extend(hatch(region, spacing, angle + 90.0, seed.wrapping_add(1), spacing * 0.04));
    out
}

/// Vertical bars of slightly varying width.
pub fn fill_lava(region: &Region, spacing: f64, seed: u64) -> Strokes {
    bars(region, spacing, seed, true)
}

/// Vertical narrow capsules on a grid.
pub fn fill_hornblende(region: &Region, spacing: f64, angle: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
        let base = angle.to_radians();
    let mut out: Strokes = Vec::new();
    let mut f = |rng: &mut Rng, _: i64, _: i64, c: Pt| {
        let a = base + rng.range(-0.2, 0.2);
        let len = spacing * rng.range(0.5, 0.8);
        let half_w = spacing * 0.09;
        let (ca, sa) = a.sin_cos();
        let (pa, pb) = (-sa, ca);
        let ring = [
            (c.0 - ca * len + pa * half_w, c.1 - sa * len + pb * half_w),
            (c.0 + ca * len + pa * half_w, c.1 + sa * len + pb * half_w),
            (c.0 + ca * len - pa * half_w, c.1 + sa * len - pb * half_w),
            (c.0 - ca * len - pa * half_w, c.1 - sa * len - pb * half_w),
            (c.0 - ca * len + pa * half_w, c.1 - sa * len + pb * half_w),
        ];
        stroke(region, &mut out, &ring);
    };
    inside_grid_seeded(region, spacing, spacing * 0.85, spacing * 0.5, spacing * 0.1, rand(seed, 3), &mut f);
    out
}

fn voronoi(region: &Region, spacing: f64, seed: u64, gap: f64, corner: f64) -> Strokes {
    voronoi_graded(region, spacing, seed, gap, 0.0, corner)
}

/// Voronoi cells whose centres are jittered by `vary` so cell sizes differ.
///
/// `vary` 0 is the even Poisson field; around 0.3 the cells read as a natural
/// stone pavement, and past 0.5 the small cells crowd out the large ones.
fn voronoi_graded(region: &Region, spacing: f64, seed: u64, gap: f64, vary: f64, corner: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let tools = crate::pattern::Tools::new(region, seed).with_spacing(spacing);
    let field = region.outset(spacing);
    let mut centres = poisson_disk(&field, spacing * 0.9, seed);
    if vary > 0.0 {
        let mut rng = Rng::new(rand(seed, 14));
        for c in centres.iter_mut() {
            let d = spacing * vary;
            *c = (c.0 + rng.range(-d, d), c.1 + rng.range(-d, d));
        }
    }
    let min_area = 0.05 * spacing * spacing;
    let mut out: Strokes = Vec::new();
    for (_, _, cell_ring) in tools.voronoi_cells(&centres, spacing * 2.2) {
        let cell = Region::from_ring(&cell_ring).intersection(region);
        let area: f64 = cell.all_rings_mm().iter().map(|r| signed_area(r)).sum();
        if area < min_area {
            continue;
        }
        let piece = if corner > 0.0 {
            cell.inset(gap + corner).outset(corner)
        } else {
            cell.inset(gap)
        };
        for ring in piece.all_rings_mm() {
            if ring.len() < 3 || signed_area(&ring).abs() < min_area {
                continue;
            }
            let mut closed = ring;
            if closed.first() != closed.last() {
                closed.push(closed[0]);
            }
            stroke(region, &mut out, &closed);
        }
    }
    out
}

/// Names of all 46 lithology / ground fills in this module.
pub const BUILTIN: &[&str] = &[
    "granite",
    "granite2",
    "diorite",
    "gabbro",
    "brackets",
    "dacite",
    "feldspar",
    "arena",
    "conglom5",
    "conglomerate",
    "conglomerate2",
    "coarse_stone",
    "mixed_rocks",
    "cracked_earth",
    "chalky_soil",
    "loose_soil",
    "compacted_soil",
    "sandy_soil",
    "mulched_soil",
    "debris_soil",
    "sandstone",
    "grainy_soil",
    "topsoil",
    "dry_cracked",
    "erosion",
    "stone_cluster",
    "drift",
    "breccia",
    "breccia2",
    "shards",
    "beds",
    "laminae",
    "coal",
    "gneiss",
    "benton",
    "masonry",
    "ferrous",
    "amphibolite",
    "diatom",
    "diatom2",
    "coquina",
    "diabase",
    "ophitic",
    "gravelly_soil",
    "mulch",
    "layered_soil",
    "pebbled_soil",
    "clay_soil",
    "silty_soil",
    "fine_gravel",
    "pebble_scatter",
    "fill",
    "lava",
    "hornblende",
];

/// Dispatch a lithology / ground fill by its canonical name.
pub fn fill_by_name(
    name: &str,
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
) -> Option<Strokes> {
    Some(match name {
        "granite" => fill_granite(region, spacing, seed),
        "granite2" => fill_granite2(region, spacing, seed),
        "diorite" => fill_diorite(region, spacing, seed),
        "gabbro" => fill_gabbro(region, spacing, seed),
        "brackets" => fill_brackets(region, spacing, seed),
        "dacite" => fill_dacite(region, spacing, angle, seed),
        "feldspar" => fill_feldspar(region, spacing, angle, seed),
        "arena" => fill_arena(region, spacing, seed),
        "conglom5" => fill_conglom5(region, spacing, seed),
        "conglomerate" => fill_conglomerate(region, spacing, seed),
        "conglomerate2" => fill_conglomerate2(region, spacing, seed),
        "coarse_stone" => fill_coarse_stone(region, spacing, seed),
        "mixed_rocks" => fill_mixed_rocks(region, spacing, seed),
        "cracked_earth" => fill_cracked_earth(region, spacing, seed),
        "chalky_soil" => fill_chalky_soil(region, spacing, seed),
        "loose_soil" => fill_loose_soil(region, spacing, seed),
        "compacted_soil" => fill_compacted_soil(region, spacing, angle, seed),
        "sandy_soil" => fill_sandy_soil(region, spacing, seed),
        "mulched_soil" => fill_mulched_soil(region, spacing, angle, seed),
        "debris_soil" => fill_debris_soil(region, spacing, seed),
        "sandstone" => fill_sandstone(region, spacing, angle, seed),
        "grainy_soil" => fill_grainy_soil(region, spacing, seed),
        "topsoil" => fill_topsoil(region, spacing, seed),
        "dry_cracked" => fill_dry_cracked(region, spacing, seed),
        "erosion" => fill_erosion(region, spacing, angle, seed),
        "stone_cluster" => fill_stone_cluster(region, spacing, seed),
        "drift" => fill_drift(region, spacing, seed),
        "breccia" => fill_breccia(region, spacing, seed),
        "breccia2" => fill_breccia2(region, spacing, seed),
        "shards" => fill_shards(region, spacing, seed),
        "beds" => fill_beds(region, spacing, angle, seed),
        "laminae" => fill_laminae(region, spacing, angle, seed),
        "coal" => fill_coal(region, spacing, angle, seed),
        "gneiss" => fill_gneiss(region, spacing, angle, seed),
        "benton" => fill_benton(region, spacing, angle, seed),
        "masonry" => fill_masonry(region, spacing, seed),
        "ferrous" => fill_ferrous(region, spacing, seed),
        "amphibolite" => fill_amphibolite(region, spacing, seed),
        "diatom" => fill_diatom(region, spacing, seed),
        "diatom2" => fill_diatom2(region, spacing, seed),
        "coquina" => fill_coquina(region, spacing, seed),
        "diabase" => fill_diabase(region, spacing, angle, seed),
        "ophitic" => fill_ophitic(region, spacing, angle, seed),
        "gravelly_soil" => fill_gravelly_soil(region, spacing, seed),
        "mulch" => fill_mulch(region, spacing, angle, seed),
        "layered_soil" => fill_layered_soil(region, spacing, angle, seed),
        "pebbled_soil" => fill_pebbled_soil(region, spacing, seed),
        "clay_soil" => fill_clay_soil(region, spacing, angle, seed),
        "silty_soil" => fill_silty_soil(region, spacing, seed),
        "fine_gravel" => fill_fine_gravel(region, spacing, seed),
        "pebble_scatter" => fill_pebble_scatter(region, spacing, seed),
        "fill" => fill_fill(region, spacing, angle, seed),
        "lava" => fill_lava(region, spacing, seed),
        "hornblende" => fill_hornblende(region, spacing, angle, seed),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)])
    }

    fn inside(s: &Strokes) {
        for p in s.iter().flatten() {
            assert!(square().covers_point(*p), "{p:?} outside");
        }
    }

    #[test]
    fn mark_grids() {
        for s in [
            fill_granite(&square(), 3.0, 1),
            fill_granite2(&square(), 3.0, 1),
            fill_diorite(&square(), 3.0, 1),
            fill_gabbro(&square(), 3.0, 1),
            fill_brackets(&square(), 3.0, 1),
        ] {
            assert!(s.len() >= 20, "only {} strokes", s.len());
            inside(&s);
        }
    }

    #[test]
    fn crystalline_and_dots() {
        let d = fill_dacite(&square(), 3.0, 35.0, 1);
        assert!(d.len() >= 40);
        inside(&d);
        let a = fill_arena(&square(), 2.0, 1);
        assert!(a.len() >= 80);
        inside(&a);
    }

    #[test]
    fn clastic() {
        for s in [
            fill_conglomerate(&square(), 3.0, 1),
            fill_conglomerate2(&square(), 3.0, 1),
            fill_drift(&square(), 4.0, 1),
            fill_breccia(&square(), 3.0, 1),
            fill_breccia2(&square(), 3.0, 1),
            fill_shards(&square(), 3.0, 1),
        ] {
            assert!(s.len() >= 15, "only {} strokes", s.len());
            inside(&s);
        }
    }

    #[test]
    fn beds_and_bars() {
        for s in [
            fill_beds(&square(), 2.5, 0.0, 1),
            fill_laminae(&square(), 2.5, 15.0, 1),
            fill_coal(&square(), 2.0, 0.0, 1),
            fill_gneiss(&square(), 3.0, 30.0, 1),
            fill_benton(&square(), 2.5, 90.0, 1),
            fill_masonry(&square(), 2.0, 1),
            fill_ferrous(&square(), 2.0, 1),
            fill_amphibolite(&square(), 2.5, 1),
            fill_lava(&square(), 2.5, 1),
        ] {
            assert!(s.len() >= 10, "only {} strokes", s.len());
            inside(&s);
        }
    }

    #[test]
    fn motifs_and_hatches() {
        for s in [
            fill_diatom(&square(), 4.0, 1),
            fill_diatom2(&square(), 4.0, 1),
            fill_coquina(&square(), 4.0, 1),
            fill_diabase(&square(), 1.5, -45.0, 1),
            fill_ophitic(&square(), 2.5, -45.0, 1),
            fill_fill(&square(), 2.0, 45.0, 1),
            fill_hornblende(&square(), 3.0, 90.0, 1),
        ] {
            assert!(s.len() >= 15, "only {} strokes", s.len());
            inside(&s);
        }
    }

    /// Extent of a stroke set along both axes.
    fn extent(s: &Strokes) -> (f64, f64, f64, f64) {
        let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in s.iter().flatten() {
            b.0 = b.0.min(p.0);
            b.1 = b.1.min(p.1);
            b.2 = b.2.max(p.0);
            b.3 = b.3.max(p.1);
        }
        b
    }

    /// Rows used to walk only `[c - half, c]` along their own direction, so
    /// half of every recipe came out blank. Each fill must reach both walls
    /// of the square, whichever way it is oriented. The slack is one spacing:
    /// a dash row ends wherever its random gap falls.
    #[test]
    fn oriented_fills_reach_the_far_wall() {
        let s = square();
        for (name, spacing, strokes) in [
            ("coal", 2.0, fill_coal(&s, 2.0, 0.0, 1)),
            ("benton", 2.5, fill_benton(&s, 2.5, 90.0, 1)),
            ("gneiss", 3.0, fill_gneiss(&s, 3.0, 30.0, 1)),
            ("coarse", 4.0, fill_coarse_stone(&s, 4.0, 1)),
        ] {
            let (x0, y0, x1, y1) = extent(&strokes);
            let slack = spacing * 1.5;
            assert!(x0 <= slack && y0 <= slack, "{name} starts late at ({x0}, {y0})");
            assert!(x1 >= 40.0 - slack && y1 >= 40.0 - slack, "{name} stops early at ({x1}, {y1})");
        }
    }

    #[test]
    fn ground_and_stone_fills() {
        for s in [
            fill_fine_gravel(&square(), 3.0, 1),
            fill_coarse_stone(&square(), 4.0, 1),
            fill_mixed_rocks(&square(), 4.0, 1),
            fill_pebble_scatter(&square(), 4.0, 1),
            fill_cracked_earth(&square(), 4.0, 1),
            fill_grainy_soil(&square(), 1.5, 1),
            fill_erosion(&square(), 1.2, 0.0, 1),
            fill_stone_cluster(&square(), 3.0, 1),
        ] {
            assert!(s.len() >= 15, "only {} strokes", s.len());
            inside(&s);
        }
    }

    #[test]
    fn soil_ground_fills() {
        for s in [
    fill_gravelly_soil(&square(), 2.5, 1),
    fill_mulch(&square(), 2.0, 0.0, 1),
    fill_layered_soil(&square(), 1.5, 0.0, 1),
    fill_pebbled_soil(&square(), 4.0, 1),
    fill_topsoil(&square(), 3.0, 1),
    fill_dry_cracked(&square(), 2.5, 1),
    fill_chalky_soil(&square(), 3.0, 1),
    fill_loose_soil(&square(), 3.0, 1),
    fill_sandy_soil(&square(), 3.0, 1),
    fill_debris_soil(&square(), 3.0, 1),
    fill_compacted_soil(&square(), 3.0, 0.0, 1),
    fill_mulched_soil(&square(), 3.0, 0.0, 1),
    fill_sandstone(&square(), 3.0, 0.0, 1),
            fill_clay_soil(&square(), 2.0, 0.0, 1),
            fill_silty_soil(&square(), 4.0, 1),
        ] {
            assert!(s.len() >= 15, "only {} strokes", s.len());
            inside(&s);
        }
    }

    /// Topsoil must mix visibly different dot sizes, not draw one radius.
    #[test]
    fn topsoil_mixes_dot_sizes() {
        let s = fill_topsoil(&square(), 3.0, 1);
        let mut radii: Vec<f64> = s
            .iter()
            .filter(|r| r.len() >= 3)
            .map(|r| {
                let cx = r.iter().map(|p| p.0).sum::<f64>() / r.len() as f64;
                let cy = r.iter().map(|p| p.1).sum::<f64>() / r.len() as f64;
                r.iter().map(|p| (p.0 - cx).hypot(p.1 - cy)).sum::<f64>() / r.len() as f64
            })
            .collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(radii.len() >= 20, "only {} rings", radii.len());
        let small = radii[radii.len() / 10];
        let large = radii[radii.len() * 9 / 10];
        assert!(large / small > 1.8, "dot sizes barely differ: {small} vs {large}");
    }

    /// Dry cracked soil uses wide mud-crack polygons; cracked earth is a
    /// finer cell net over dots.
    #[test]
    fn dry_cracked_cells_are_large() {
        let dry = fill_dry_cracked(&square(), 4.0, 1);
        let cracked = fill_cracked_earth(&square(), 4.0, 1);
        assert!(
            dry.len() * 2 < cracked.len(),
            "dry-cracked {} strokes should be coarser than cracked-earth {}",
            dry.len(),
            cracked.len()
        );
    }

    /// The mulch rows are staggered, so a naive walk would leave the far
    /// half-row bare; check both walls are reached at 0° and 90°.
    #[test]
    fn mulch_covers_the_whole_recipe() {
        let s = square();
        for angle in [0.0, 90.0] {
            let (x0, y0, x1, y1) = extent(&mulch(&s, 2.0, angle, 1));
            let slack = 3.0;
            assert!(
                x0 <= slack && y0 <= slack && x1 >= 40.0 - slack && y1 >= 40.0 - slack,
                "{angle}° covers ({x0}, {y0})..({x1}, {y1})"
            );
        }
    }
}
