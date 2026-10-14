//! Natural textures drawn as contour lines of a noise-driven field
//! (`field::fill_isolines`): marble veins, wood grain, cracked ground.

use crate::field::{fill_isolines, levels_every};
use crate::anchor::{centre, Anchor, Frame};
use crate::fray::{fray_strokes, whole_at};
use crate::geo::{Pt, Region, Strokes};
use crate::noise::Perlin;
use crate::random::{poisson_disk, Rng};

/// Marble: veins that drift across the slab at `angle`, about `spacing` mm
/// apart, warped by turbulence. Each vein is drawn as its two edges, so it
/// pinches and swells like a real one.
pub fn fill_marble(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let noise = Perlin::new(seed);
    let thick = Perlin::new(seed.wrapping_add(1));
    let (sin, cos) = angle.to_radians().sin_cos();
    let warp = spacing * 1.2;
    // The veins are a field over the page; the anchor moves the field.
    let at = anchor.resolve((0.0, 0.0));
    let f = |p: Pt| {
        let p = (p.0 - at.0, p.1 - at.1);
        let across = -p.0 * sin + p.1 * cos;
        let (u, v) = (p.0 / (spacing * 4.0), p.1 / (spacing * 4.0));
        // Domain warp twice: slow drift, then a folded ripple on top.
        let w1 = noise.fbm(u, v, 3);
        let w2 = noise.fbm(u * 2.0 + 5.2 * w1, v * 2.0 - 3.1 * w1, 2);
        let g = across + warp * (w1 + 0.35 * w2);
        // Veins swell and thin along their length, never vanishing.
        let t = 0.5 + 0.5 * thick.noise(u * 0.8 + 9.0, v * 0.8);
        (std::f64::consts::PI * g / spacing).sin().abs() - (0.1 + 0.14 * t)
    };
    // Sampled finer than the thinnest vein, or it breaks into dashes.
    fill_isolines(region, (spacing / 25.0).clamp(0.05, 0.3), &[0.0], f)
}

/// Wood grain: long lines `spacing` mm apart running along `angle`,
/// wandering slowly, with a few knots the grain flows around.
/// The grain is a field over the page, moved by the anchor; the knots are
/// placed inside each board.
pub fn fill_wood(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let noise = Perlin::new(seed);
    let (minx, miny, maxx, maxy) = region.bounds_mm();
    let area = (maxx - minx) * (maxy - miny);
    let mut rng = Rng::new(seed.wrapping_add(9));
    // About one knot per 900 mm², at least one.
    let n_knots = ((area / 900.0).round() as usize).max(1);
    // Knots sit inside the board: one just outside the wall only crowds
    // the grain into a dark wedge along the edge.
    let knots: Vec<(Pt, f64)> = (0..n_knots)
        .filter_map(|_| {
            // Knot size is about the board, not the line spacing: at fine
            // spacing a spacing-sized knot is a black blot.
            let r = spacing + rng.range(2.5, 5.0);
            let inner = region.inset(r);
            (0..50)
                .map(|_| (rng.range(minx, maxx), rng.range(miny, maxy)))
                .find(|&c| inner.covers_point(c))
                .map(|c| (c, r))
        })
        .collect();
    let (sin, cos) = angle.to_radians().sin_cos();
    let at = anchor.resolve((0.0, 0.0));
    let f = |p: Pt| {
        let (along, across) = {
            let q = (p.0 - at.0, p.1 - at.1);
            (q.0 * cos + q.1 * sin, -q.0 * sin + q.1 * cos)
        };
        let mut g = across + spacing * 2.0 * noise.fbm(along / (spacing * 25.0), across / (spacing * 6.0), 3);
        for &(k, r) in &knots {
            let d2 = ((p.0 - k.0).powi(2) + (p.1 - k.1).powi(2)) / (r * r);
            // Just past the height that closes rings around the core;
            // higher, the grain on the knot's flanks crowds into a blot.
            g += 1.3 * r * (-d2).exp();
        }
        g
    };
    let reach = (maxx - minx).hypot(maxy - miny) + spacing * 20.0;
    fill_isolines(region, (spacing / 8.0).clamp(0.08, 0.4), &levels_every(-reach, reach, spacing), f)
}

/// Parched ground / mud cracks: rounded Voronoi cells with wide gaps between
/// them — a cellular tile like dry desert earth. `spacing` is roughly the
/// cell size in mm.
pub fn fill_cracks(region: &Region, spacing: f64, seed: u64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    use crate::concentric::signed_area;
    let field = region.outset(spacing);
    let centres = poisson_disk(&field, spacing * 0.92, seed);
    let mut rng = Rng::new(seed.wrapping_add(4));
    let reach2 = (spacing * 2.5).powi(2);
    let gap = spacing * rng.range(0.14, 0.20);
    let corner = spacing * rng.range(0.22, 0.34);
    let min_area = 0.08 * spacing * spacing;
    let mut out: Strokes = Vec::new();
    for (i, &c) in centres.iter().enumerate() {
        let r = spacing * 2.0;
        let mut cell = vec![(c.0 - r, c.1 - r), (c.0 + r, c.1 - r), (c.0 + r, c.1 + r), (c.0 - r, c.1 + r)];
        for (j, &o) in centres.iter().enumerate() {
            if i == j || (o.0 - c.0).powi(2) + (o.1 - c.1).powi(2) > reach2 {
                continue;
            }
            cell = clip_half_plane(&cell, c, o);
            if cell.len() < 3 {
                break;
            }
        }
        if cell.len() < 3 {
            continue;
        }
        let cell = Region::from_ring(&cell).intersection(region);
        let area: f64 = cell.all_rings_mm().iter().map(|r| signed_area(r)).sum();
        if area < min_area {
            continue;
        }
        let piece = cell.inset(gap + corner).outset(corner);
        for ring in piece.all_rings_mm() {
            if ring.len() < 3 || signed_area(&ring).abs() < min_area {
                continue;
            }
            let mut closed = ring;
            if closed.first() != closed.last() {
                closed.push(closed[0]);
            }
            out.extend(region.clip_polyline(&closed));
        }
    }
    out
}

/// Keep the part of `poly` closer to `c` than to `o`.
fn clip_half_plane(poly: &[Pt], c: Pt, o: Pt) -> Vec<Pt> {
    let m = ((c.0 + o.0) / 2.0, (c.1 + o.1) / 2.0);
    let n = (o.0 - c.0, o.1 - c.1);
    let side = |p: Pt| (p.0 - m.0) * n.0 + (p.1 - m.1) * n.1;
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
            let t = sa / (sa - sb);
            out.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
        }
    }
    out
}

/// Keep the part of `poly` closer to `c` than to `o`, recording which
/// neighbour (`Some(j)`) created each edge so shared Voronoi walls can be
/// emitted once with organic kinks while keeping 3-way junctions exact.
fn clip_half_plane_tagged(poly: &[(Pt, Option<usize>)], c: Pt, o: Pt, tag: usize) -> Vec<(Pt, Option<usize>)> {
    let m = ((c.0 + o.0) / 2.0, (c.1 + o.1) / 2.0);
    let n = (o.0 - c.0, o.1 - c.1);
    let side = |p: Pt| (p.0 - m.0) * n.0 + (p.1 - m.1) * n.1;
    let mut out = Vec::new();
    let len = poly.len();
    for i in 0..len {
        let (a, tag_a) = poly[i];
        let (b, _) = poly[(i + 1) % len];
        let (sa, sb) = (side(a), side(b));
        if sa <= 0.0 {
            if sb <= 0.0 {
                out.push((a, tag_a));
            } else {
                out.push((a, tag_a));
                let t = sa / (sa - sb);
                out.push(((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)), Some(tag)));
            }
        } else if sb <= 0.0 {
            let t = sa / (sa - sb);
            out.push(((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)), tag_a));
        }
    }
    out
}

/// Single-line Voronoi / polygonal crack network: adjacent cells share a single
/// wall (no double lines), 3-way junctions meet cleanly, and each shared edge
/// has 1–2 subtle angular kinks so cells look like hand-drawn irregular stone
/// or parched-earth polygons.
pub fn fill_cell_net(region: &Region, spacing: f64, seed: u64) -> Strokes {
    fill_cell_net_frayed(region, spacing, seed, 0.0)
}

/// [`fill_cell_net`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_cell_net_frayed(region: &Region, spacing: f64, seed: u64, fray: f64) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let field = region.outset(spacing * 1.6);
    let mut rng = Rng::new(seed.wrapping_add(17));
    let centres: Vec<Pt> = poisson_disk(&field, spacing * 0.88, seed)
        .into_iter()
        .map(|c| {
            (
                c.0 + rng.range(-0.14, 0.14) * spacing,
                c.1 + rng.range(-0.14, 0.14) * spacing,
            )
        })
        .collect();
    let reach2 = (spacing * 2.6).powi(2);
    let mut raw: Strokes = Vec::new();
    for (i, &c) in centres.iter().enumerate() {
        let r = spacing * 2.2;
        let mut cell: Vec<(Pt, Option<usize>)> = vec![
            ((c.0 - r, c.1 - r), None),
            ((c.0 + r, c.1 - r), None),
            ((c.0 + r, c.1 + r), None),
            ((c.0 - r, c.1 + r), None),
        ];
        for (j, &o) in centres.iter().enumerate() {
            if i == j || (o.0 - c.0).powi(2) + (o.1 - c.1).powi(2) > reach2 {
                continue;
            }
            cell = clip_half_plane_tagged(&cell, c, o, j);
            if cell.len() < 3 {
                break;
            }
        }
        if cell.len() < 3 {
            continue;
        }
        let n = cell.len();
        for k in 0..n {
            let (a, tag) = cell[k];
            let (b, _) = cell[(k + 1) % n];
            let Some(j) = tag else { continue };
            if i >= j {
                continue;
            }
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy);
            if len < 1e-4 {
                continue;
            }
            let mut erng = Rng::new(
                seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    ^ (j as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9),
            );
            let (nx, ny) = (-dy / len, dx / len);
            let poly = if len >= spacing * 0.42 && erng.unit() < 0.65 {
                let t1 = 0.34 + erng.range(-0.07, 0.07);
                let t2 = 0.68 + erng.range(-0.07, 0.07);
                let d1 = erng.range(-0.11, 0.11) * len;
                let d2 = erng.range(-0.11, 0.11) * len;
                vec![
                    a,
                    (a.0 + t1 * dx + d1 * nx, a.1 + t1 * dy + d1 * ny),
                    (a.0 + t2 * dx + d2 * nx, a.1 + t2 * dy + d2 * ny),
                    b,
                ]
            } else if len >= spacing * 0.20 {
                let t = 0.5 + erng.range(-0.12, 0.12);
                let d = erng.range(-0.12, 0.12) * len;
                vec![a, (a.0 + t * dx + d * nx, a.1 + t * dy + d * ny), b]
            } else {
                vec![a, b]
            };
            raw.push(poly);
        }
    }
    if fray > 0.0 {
        fray_strokes(region, raw, spacing, fray, seed)
    } else {
        region.clip_strokes(&raw)
    }
}

/// Flowing wave streamlines integrated through a smooth 2D vector field of
/// undulating currents and spiral eddies, seeded evenly using Jobard-Lefer
/// streamline placement so the flow is a true continuous vector field with no
/// empty gaps around vortices.
pub fn fill_eddies(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    fill_eddies_frayed(region, spacing, angle, seed, anchor, 0.0)
}

/// [`fill_eddies`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_eddies_frayed(
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: Anchor,
    fray: f64,
) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let noise = Perlin::new(seed);
    let fray_noise = Perlin::new(seed.wrapping_add(313));
    let break_noise = Perlin::new(seed.wrapping_add(101));
    let mut rng = Rng::new(seed.wrapping_add(29));

    let eddy_r = spacing * 3.2;
    let inner = region.inset(eddy_r * 0.85);
    let mut raw_centres = if !inner.is_empty() {
        poisson_disk(&inner, spacing * 13.5, seed.wrapping_add(3))
    } else {
        vec![]
    };
    if raw_centres.is_empty() {
        let fallback_inner = region.inset(spacing * 1.6);
        if !fallback_inner.is_empty() {
            let candidates = poisson_disk(&fallback_inner, spacing * 4.0, seed.wrapping_add(7));
            if let Some(best) = candidates.into_iter().max_by(|&a, &b| {
                region
                    .dist_to_boundary(a)
                    .partial_cmp(&region.dist_to_boundary(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                raw_centres.push(best);
            }
        }
    }

    struct Eddy {
        uc: f64,
        vc: f64,
        r: f64,
        spin: f64,
    }
    let mut eddies: Vec<Eddy> = Vec::new();
    for c in raw_centres {
        let (uc, vc) = frame.to_frame(c);
        if eddies.iter().any(|e| (e.uc - uc).hypot(e.vc - vc) < spacing * 10.5) {
            continue;
        }
        let d_edge = region.dist_to_boundary(c);
        let r = eddy_r.min((d_edge * 0.78).max(spacing * 2.3));
        let spin = if rng.unit() < 0.75 { 1.0 } else { -1.0 };
        eddies.push(Eddy { uc, vc, r, spin });
    }

    let pad = eddy_r + spacing * 3.0;
    let (u_min, u_max, v_min, v_max) = frame.span(region, pad);
    let step = (spacing * 0.16).clamp(0.08, 0.32);
    let d_sep = spacing * 0.88;
    let d_test = spacing * 0.56;

    // Spatial occupancy grid in (u, v) frame coordinates to maintain even streamline density.
    let cell = spacing;
    let gw = (((u_max - u_min) / cell).ceil() as usize).max(1) + 2;
    let gh = (((v_max - v_min) / cell).ceil() as usize).max(1) + 2;
    // Store (u, v, streamline_id, arc_s)
    let mut grid: Vec<Vec<(f64, f64, usize, f64)>> = vec![Vec::new(); gw * gh];

    let grid_coords = |u: f64, v: f64| -> Option<(usize, usize)> {
        if u < u_min || u > u_max || v < v_min || v > v_max {
            return None;
        }
        let gx = (((u - u_min) / cell).floor() as isize).clamp(0, gw as isize - 1) as usize;
        let gy = (((v - v_min) / cell).floor() as isize).clamp(0, gh as isize - 1) as usize;
        Some((gx, gy))
    };

    let too_close = |u: f64,
                     v: f64,
                     min_dist: f64,
                     own_id: usize,
                     own_s: f64,
                     grid: &[Vec<(f64, f64, usize, f64)>]|
     -> bool {
        let Some((gx, gy)) = grid_coords(u, v) else {
            return true;
        };
        let r2 = min_dist * min_dist;
        let x0 = gx.saturating_sub(1);
        let x1 = (gx + 1).min(gw - 1);
        let y0 = gy.saturating_sub(1);
        let y1 = (gy + 1).min(gh - 1);
        for yy in y0..=y1 {
            let row_off = yy * gw;
            for xx in x0..=x1 {
                for &(pu, pv, sid, ps) in &grid[row_off + xx] {
                    if sid == own_id && (own_s - ps).abs() < spacing * 1.65 {
                        continue;
                    }
                    if (pu - u).powi(2) + (pv - v).powi(2) < r2 {
                        return true;
                    }
                }
            }
        }
        false
    };

    let insert_pt = |u: f64,
                     v: f64,
                     sid: usize,
                     s: f64,
                     grid: &mut [Vec<(f64, f64, usize, f64)>]| {
        if let Some((gx, gy)) = grid_coords(u, v) {
            grid[gy * gw + gx].push((u, v, sid, s));
        }
    };

    // Pure unified 2D vector field in (u, v):
    // - Inside each eddy (`r <= e.r`), the field is an exact Archimedean spiral tangent
    //   (`v_theta + (spacing / 2pi r) v_r`) so integrating from the eddy centre traces a tight
    //   spiral with constant radial turn spacing `spacing` and zero gap!
    // - Outside (`r > e.r`), the velocity is the exact tangent of a divergence-free streamfunction
    //   ψ(u, v) combining a smooth undulating wave current with a Rankine-oval cylinder deflection
    //   around each eddy core, so outer streamlines hug the spiral boundary smoothly on every side.
    let psi_outer = |u: f64, v: f64| -> f64 {
        let p_world = frame.world((u, v));
        let edge_damp = if fray > 0.0 {
            0.35 + 0.65 * whole_at(region, p_world, spacing, fray, &fray_noise)
        } else {
            1.0
        };
        let mut shield = 1.0_f64;
        for e in &eddies {
            let d2 = ((u - e.uc).powi(2) + (v - e.vc).powi(2)) / ((e.r * 1.4).powi(2));
            if d2 < 4.0 {
                shield *= 1.0 - (-d2 * 1.2).exp();
            }
        }
        let w1 = noise.fbm(u / (spacing * 9.5), v / (spacing * 6.5), 2);
        let w2 = (std::f64::consts::TAU * u / (spacing * 11.5)
            + 1.8 * noise.noise(u / (spacing * 9.0), v / (spacing * 7.5)))
        .sin();
        let mut val = v + spacing * 1.85 * edge_damp * shield * (0.70 * w1 + 0.35 * w2);

        for e in &eddies {
            let du = u - e.uc;
            let dv = v - e.vc;
            let r2 = (du * du + dv * dv).max(e.r * e.r * 0.75);
            // Cylinder dipole deflection + gentle circulation so streamlines wrap tightly around `r = e.r`
            val += -dv * (e.r * e.r * 0.92) / r2
                + e.spin * 0.42 * e.r * (-(r2 / (e.r * e.r * 2.0))).exp();
        }
        val
    };

    let velocity = |u: f64, v: f64| -> (f64, f64) {
        let eps = spacing * 0.06;
        let dpsi_du = (psi_outer(u + eps, v) - psi_outer(u - eps, v)) / (2.0 * eps);
        let dpsi_dv = (psi_outer(u, v + eps) - psi_outer(u, v - eps)) / (2.0 * eps);
        let (mut vx, mut vy) = (dpsi_dv, -dpsi_du);
        let n0 = vx.hypot(vy).max(1e-6);
        vx /= n0;
        vy /= n0;

        for e in &eddies {
            let du = u - e.uc;
            let dv = v - e.vc;
            let r = du.hypot(dv).max(1e-4);
            let q = r / e.r;
            if q < 1.25 {
                let tx = -e.spin * dv / r;
                let ty = e.spin * du / r;
                let rx = du / r;
                let ry = dv / r;
                let pitch = spacing / (std::f64::consts::TAU * r.max(spacing * 0.28));
                let sp_x = tx + pitch * rx;
                let sp_y = ty + pitch * ry;
                let sp_n = sp_x.hypot(sp_y);
                let (sp_x, sp_y) = (sp_x / sp_n, sp_y / sp_n);

                let blend = if q <= 0.92 {
                    1.0
                } else {
                    let t = ((q - 0.92) / 0.33).clamp(0.0, 1.0);
                    1.0 - t * t * (3.0 - 2.0 * t)
                };
                vx = (1.0 - blend) * vx + blend * sp_x;
                vy = (1.0 - blend) * vy + blend * sp_y;
            }
        }

        let norm = vx.hypot(vy).max(1e-6);
        (vx / norm, vy / norm)
    };

    let mut streamlines: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut seeds: std::collections::VecDeque<(f64, f64)> = std::collections::VecDeque::new();

    // 1. Seed each eddy near its centre and integrate forward through the unified vector field:
    //    it coils outward for ~2.5 turns and flows directly into the main stream!
    for e in &eddies {
        let sid = streamlines.len();
        let (mut u, mut v) = (e.uc + spacing * 0.28, e.vc);
        let mut pts_uv = vec![(u, v)];
        let mut s_acc = 0.0;
        insert_pt(u, v, sid, s_acc, &mut grid);

        for _ in 0..2000 {
            let (vx1, vy1) = velocity(u, v);
            let (vx2, vy2) = velocity(u + 0.5 * step * vx1, v + 0.5 * step * vy1);
            u += step * vx2;
            v += step * vy2;
            s_acc += step;
            if u < u_min || u > u_max || v < v_min || v > v_max {
                break;
            }
            if too_close(u, v, d_test, sid, s_acc, &grid) {
                break;
            }
            pts_uv.push((u, v));
            insert_pt(u, v, sid, s_acc, &mut grid);
        }
        for pair in pts_uv.windows(2).step_by(3) {
            let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            let len = dx.hypot(dy);
            if len > 1e-5 {
                let (nx, ny) = (-dy / len, dx / len);
                seeds.push_back((pair[0].0 + nx * spacing, pair[0].1 + ny * spacing));
                seeds.push_back((pair[0].0 - nx * spacing, pair[0].1 - ny * spacing));
            }
        }
        streamlines.push(pts_uv);
    }

    // 2. Add a dense fallback grid of seeds across the whole domain so no area can be missed.
    let k0 = (v_min / (spacing * 0.5)).floor() as i64;
    let k1 = (v_max / (spacing * 0.5)).ceil() as i64;
    let m0 = (u_min / (spacing * 0.5)).floor() as i64;
    let m1 = (u_max / (spacing * 0.5)).ceil() as i64;
    for k in k0..=k1 {
        for m in m0..=m1 {
            seeds.push_back((m as f64 * spacing * 0.5, k as f64 * spacing * 0.5));
        }
    }

    // 3. Jobard-Lefer streamline integration (both backward and forward through the vector field)
    while let Some((u0, v0)) = seeds.pop_front() {
        if u0 < u_min || u0 > u_max || v0 < v_min || v0 > v_max {
            continue;
        }
        if too_close(u0, v0, d_sep, usize::MAX, 0.0, &grid) {
            continue;
        }
        let sid = streamlines.len();
        insert_pt(u0, v0, sid, 0.0, &mut grid);

        // Integrate backward
        let mut back: Vec<((f64, f64), f64)> = Vec::new();
        let (mut u, mut v) = (u0, v0);
        let mut s_back = 0.0;
        for _ in 0..1400 {
            let (vx1, vy1) = velocity(u, v);
            let (vx2, vy2) = velocity(u - 0.5 * step * vx1, v - 0.5 * step * vy1);
            u -= step * vx2;
            v -= step * vy2;
            s_back -= step;
            if u < u_min || u > u_max || v < v_min || v > v_max {
                break;
            }
            if too_close(u, v, d_test, sid, s_back, &grid) {
                break;
            }
            insert_pt(u, v, sid, s_back, &mut grid);
            back.push(((u, v), s_back));
        }
        back.reverse();

        // Integrate forward
        let mut pts_uv: Vec<(f64, f64)> = back.into_iter().map(|(p, _)| p).collect();
        pts_uv.push((u0, v0));
        let (mut u, mut v) = (u0, v0);
        let mut s_fwd = 0.0;
        for _ in 0..1400 {
            let (vx1, vy1) = velocity(u, v);
            let (vx2, vy2) = velocity(u + 0.5 * step * vx1, v + 0.5 * step * vy1);
            u += step * vx2;
            v += step * vy2;
            s_fwd += step;
            if u < u_min || u > u_max || v < v_min || v > v_max {
                break;
            }
            if too_close(u, v, d_test, sid, s_fwd, &grid) {
                break;
            }
            insert_pt(u, v, sid, s_fwd, &mut grid);
            pts_uv.push((u, v));
        }

        if pts_uv.len() < 4 {
            continue;
        }
        for pair in pts_uv.windows(2).step_by(4) {
            let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            let len = dx.hypot(dy);
            if len > 1e-5 {
                let (nx, ny) = (-dy / len, dx / len);
                seeds.push_front((pair[0].0 - nx * spacing, pair[0].1 - ny * spacing));
                seeds.push_front((pair[0].0 + nx * spacing, pair[0].1 + ny * spacing));
            }
        }
        streamlines.push(pts_uv);
    }

    // Convert to world coordinates with occasional inline breaks like Image 3.
    let mut raw: Strokes = Vec::new();
    for (sid, pts_uv) in streamlines.into_iter().enumerate() {
        let mut cur: Vec<Pt> = Vec::new();
        let mut dist = 0.0_f64;
        let mut in_gap = false;
        let mut gap_end = 0.0_f64;
        for i in 0..pts_uv.len() {
            let p_world = frame.world(pts_uv[i]);
            if i > 0 {
                let prev = frame.world(pts_uv[i - 1]);
                dist += (p_world.0 - prev.0).hypot(p_world.1 - prev.1);
            }
            if in_gap {
                if dist >= gap_end {
                    in_gap = false;
                    cur.push(p_world);
                }
                continue;
            }
            let near_eddy = eddies
                .iter()
                .any(|e| (pts_uv[i].0 - e.uc).hypot(pts_uv[i].1 - e.vc) < e.r * 1.25);
            let bn = break_noise.noise(dist / (spacing * 4.2), sid as f64 * 0.73 + 3.1);
            if !near_eddy && cur.len() > 14 && bn > 0.56 {
                if cur.len() >= 2 {
                    raw.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
                in_gap = true;
                gap_end = dist + spacing * 0.65;
            } else {
                cur.push(p_world);
            }
        }
        if cur.len() >= 2 {
            raw.push(cur);
        }
    }

    if fray > 0.0 {
        fray_strokes(region, raw, spacing, fray, seed)
    } else {
        region.clip_strokes(&raw)
    }
}

/// Overlapping wave swells / rolling dunes (`seigaiha`-like organic wave hills):
/// layered undulating crest-lines arranged from back to front so each swell
/// occludes the ones behind it and is filled with internal flow lines that fan
/// and follow the slope of the wave.
pub fn fill_swells(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    fill_swells_frayed(region, spacing, angle, seed, anchor, 0.0)
}

/// [`fill_swells`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_swells_frayed(
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: Anchor,
    fray: f64,
) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let noise = Perlin::new(seed);
    let mut rng = Rng::new(seed.wrapping_add(53));

    let swell_w = spacing * 18.0;
    let swell_h = spacing * 8.0;
    let pad = swell_w * 0.9;
    let (u_min, u_max, v_min, v_max) = frame.span(region, pad);

    let step = (spacing * 0.25).clamp(0.12, 0.5);
    let n_samples = (((u_max - u_min) / step).ceil() as usize).max(2) + 1;

    // Horizon buffer: `horizon[idx]` tracks the lowest (most foreground) `v`
    // drawn so far at sample `idx`. We march from foreground (`v_max`) toward
    // background (`v_min`), drawing each swell only where `v < horizon[idx]`.
    let mut horizon = vec![f64::INFINITY; n_samples];

    let row_pitch = swell_h * 0.62;
    let col_pitch = swell_w * 0.78;
    let j0 = ((v_min - swell_h) / row_pitch).floor() as i64;
    let j1 = ((v_max + swell_h) / row_pitch).ceil() as i64;
    let i0 = ((u_min - swell_w) / col_pitch).floor() as i64;
    let i1 = ((u_max + col_pitch) / col_pitch).ceil() as i64;

    let mut raw: Strokes = Vec::new();

    // Emit a polyline clipped against the current horizon buffer.
    let emit_masked = |profile: &[f64], horizon: &[f64], out: &mut Strokes| {
        let mut cur: Vec<Pt> = Vec::new();
        for idx in 0..n_samples {
            let u = u_min + idx as f64 * step;
            let v = profile[idx];
            let h = horizon[idx];
            if v < h - 1e-4 {
                if cur.is_empty() && idx > 0 {
                    // Interpolate exact intersection with the foreground crest!
                    let v_prev = profile[idx - 1];
                    let h_prev = horizon[idx - 1];
                    let d0 = v_prev - h_prev;
                    let d1 = v - h;
                    if (d0 - d1).abs() > 1e-9 {
                        let t = (d0 / (d0 - d1)).clamp(0.0, 1.0);
                        let u_hit = u - step + t * step;
                        let v_hit = v_prev + t * (v - v_prev);
                        cur.push(frame.world((u_hit, v_hit)));
                    }
                }
                cur.push(frame.world((u, v)));
            } else if !cur.is_empty() {
                if idx > 0 {
                    let v_prev = profile[idx - 1];
                    let h_prev = horizon[idx - 1];
                    let d0 = v_prev - h_prev;
                    let d1 = v - h;
                    if (d0 - d1).abs() > 1e-9 {
                        let t = (d0 / (d0 - d1)).clamp(0.0, 1.0);
                        let u_hit = u - step + t * step;
                        let v_hit = v_prev + t * (v - v_prev);
                        cur.push(frame.world((u_hit, v_hit)));
                    }
                }
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
            }
        }
        if cur.len() >= 2 {
            out.push(cur);
        }
    };

    // Process rows from foreground (largest v) to background (smallest v).
    for j in (j0..=j1).rev() {
        let stagger = if j.rem_euclid(2) == 1 { col_pitch * 0.5 } else { 0.0 };
        // Randomize left-to-right order within the row slightly so adjacent swells overlap naturally.
        let mut cols: Vec<i64> = (i0..=i1).collect();
        if j.rem_euclid(2) == 0 {
            cols.reverse();
        }
        for i in cols {
            let mut srng = Rng::new(
                seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    ^ (j as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9),
            );
            let uc = i as f64 * col_pitch + stagger + srng.range(-0.22, 0.22) * col_pitch;
            let v_base = j as f64 * row_pitch + srng.range(-0.15, 0.15) * row_pitch;
            let w_l = swell_w * srng.range(0.55, 0.95);
            let w_r = swell_w * srng.range(0.55, 0.95);
            let amp = swell_h * srng.range(0.75, 1.35);
            let skew = srng.range(-0.35, 0.35);
            let fan = srng.range(-0.42, 0.42);
            let n_phase = srng.range(0.0, 100.0);

            // Compute crest profile v_crest(u) and local vertical rib spacing s_rib(u).
            let mut crest = vec![0.0; n_samples];
            let mut rib_step = vec![spacing; n_samples];
            let mut visible_any = false;
            for idx in 0..n_samples {
                let u = u_min + idx as f64 * step;
                let du = u - uc;
                let w = if du < 0.0 { w_l } else { w_r };
                let x = du / w;
                if x.abs() > 2.4 {
                    crest[idx] = v_base + swell_h * 2.0;
                    continue;
                }
                // Skewed bell-like wave crest + smooth Perlin undulation
                let bell = (-x * x * (1.0 + skew * x.clamp(-1.0, 1.0))).exp();
                let undul = 0.16 * swell_h * noise.fbm(u / (swell_w * 0.7) + n_phase, j as f64 * 0.8, 2);
                let vc = v_base + swell_h * 0.7 - amp * bell + undul;
                crest[idx] = vc;
                if vc < horizon[idx] {
                    visible_any = true;
                }
                // Rib spacing fans smoothly from one flank of the swell to the other
                // and compresses on steeper slopes, just like the reference image!
                let factor = (1.0 + fan * x.tanh()).clamp(0.52, 1.55);
                rib_step[idx] = spacing * factor;
            }
            if !visible_any {
                continue;
            }

            // Draw the upper crest line first
            emit_masked(&crest, &horizon, &mut raw);

            // Draw internal contour ribs flowing inside the swell body (`m = 1..n_ribs`)
            let n_ribs = ((swell_h * 2.2 / spacing).ceil() as usize).clamp(6, 40);
            let mut rib = vec![0.0; n_samples];
            for m in 1..=n_ribs {
                let mf = m as f64;
                for idx in 0..n_samples {
                    let u = u_min + idx as f64 * step;
                    let drift = 0.08
                        * spacing
                        * mf.min(6.0)
                        * noise.noise(u / (swell_w * 0.5) + n_phase, mf * 0.25);
                    rib[idx] = crest[idx] + mf * rib_step[idx] + drift;
                }
                emit_masked(&rib, &horizon, &mut raw);
            }

            // Raise the horizon to this swell's crest so background swells behind it are occluded!
            for idx in 0..n_samples {
                if crest[idx] < horizon[idx] {
                    horizon[idx] = crest[idx];
                }
            }
        }
    }

    let _ = rng.next_u64();
    if fray > 0.0 {
        fray_strokes(region, raw, spacing, fray, seed)
    } else {
        region.clip_strokes(&raw)
    }
}

/// Broken undulating wave swells: rows of medium-length wavy strokes whose
/// crests and troughs bend together in a smooth 2D swell field, with staggered
/// breaks between strokes and optional perimeter-aware edge fraying.
pub fn fill_ripples(region: &Region, spacing: f64, angle: f64, seed: u64, anchor: Anchor) -> Strokes {
    fill_ripples_frayed(region, spacing, angle, seed, anchor, 0.0)
}

/// [`fill_ripples`] with optional perimeter-aware edge fraying (`fray` in mm).
pub fn fill_ripples_frayed(
    region: &Region,
    spacing: f64,
    angle: f64,
    seed: u64,
    anchor: Anchor,
    fray: f64,
) -> Strokes {
    if spacing <= 0.0 || region.is_empty() {
        return vec![];
    }
    let frame = Frame::anchored(anchor, centre(region), angle);
    let swell = Perlin::new(seed);
    let ripple = Perlin::new(seed.wrapping_add(19));
    let fray_noise = Perlin::new(seed.wrapping_add(409));

    let pad = spacing * 3.0;
    let (u_min, u_max, v_min, v_max) = frame.span(region, pad);
    let row_h = spacing;
    let k0 = (v_min / row_h).floor() as i64;
    let k1 = (v_max / row_h).ceil() as i64;
    let step = (spacing * 0.25).clamp(0.12, 0.5);

    let mut raw: Strokes = Vec::new();
    for k in k0..=k1 {
        let v0 = k as f64 * row_h;
        let mut rrng = Rng::new(seed ^ (k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut u = u_min - rrng.range(0.0, spacing * 6.0);
        while u < u_max {
            let seg_len = spacing * rrng.range(4.2, 9.8);
            let gap_len = spacing * rrng.range(0.55, 1.45);
            let u_end = (u + seg_len).min(u_max);
            let mut pts = Vec::new();
            let mut x = u;
            while x <= u_end {
                let p_base = frame.world((x, v0));
                let edge_scale = if fray > 0.0 {
                    0.3 + 0.7 * whole_at(region, p_base, spacing, fray, &fray_noise)
                } else {
                    1.0
                };
                // Smooth 2D swell shared across neighbouring rows + rhythmic wave undulation
                let phase_warp = swell.fbm(x / (spacing * 10.0), v0 / (spacing * 5.5), 2) * 3.2;
                let wave = (std::f64::consts::TAU * x / (spacing * 6.2) + v0 / (spacing * 2.8) + phase_warp).sin();
                let envelope = 0.55 + 0.55 * ripple.noise(x / (spacing * 7.5) + 4.7, v0 / (spacing * 4.2) - 2.3);
                let slow_drift = swell.noise(x / (spacing * 12.0) - 1.9, v0 / (spacing * 7.0) + 3.1);
                let dv = spacing * edge_scale * (0.62 * wave * envelope + 0.85 * slow_drift);
                pts.push(frame.world((x, v0 + dv)));
                x += step;
            }
            if pts.len() >= 2 {
                raw.push(pts);
            }
            u = u_end + gap_len;
        }
    }

    if fray > 0.0 {
        fray_strokes(region, raw, spacing, fray, seed)
    } else {
        region.clip_strokes(&raw)
    }
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
    fn marble_has_veins_inside() {
        let m = fill_marble(&square(), 4.0, 30.0, 1, Anchor::LOCAL);
        assert!(m.len() >= 6, "only {} vein edges", m.len());
        inside(&m);
    }

    #[test]
    fn wood_grain_runs_along_the_angle() {
        let w = fill_wood(&square(), 2.0, 0.0, 1, Anchor::LOCAL);
        assert!(w.len() >= 15, "only {} grain lines", w.len());
        inside(&w);
        // Mostly horizontal: total x travel beats y travel.
        let (mut dx, mut dy) = (0.0, 0.0);
        for s in &w {
            for p in s.windows(2) {
                dx += (p[1].0 - p[0].0).abs();
                dy += (p[1].1 - p[0].1).abs();
            }
        }
        assert!(dx > 2.0 * dy, "grain not along 0°: dx {dx}, dy {dy}");
    }

    #[test]
    fn cracks_draw_a_net() {
        let w = fill_cracks(&square(), 5.0, 1);
        assert!(w.len() >= 20, "only {} crack segments", w.len());
        inside(&w);
    }
}
