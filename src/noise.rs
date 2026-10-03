//! Perlin gradient noise: a smooth random field for fills whose density
//! should drift across the region (clouds of stipple, patchy shading).

use crate::random::Rng;

#[derive(Clone)]
pub struct Perlin {
    perm: [u8; 512],
}

impl Perlin {
    pub fn new(seed: u64) -> Perlin {
        let mut p: Vec<u8> = (0..=255).collect();
        let mut rng = Rng::new(seed.wrapping_add(11));
        for i in (1..256).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            p.swap(i, j);
        }
        let mut perm = [0u8; 512];
        for i in 0..512 {
            perm[i] = p[i % 256];
        }
        Perlin { perm }
    }

    /// Noise at `(x, y)`, roughly in `[-1, 1]`, one bump per unit.
    pub fn noise(&self, x: f64, y: f64) -> f64 {
        let (xf, yf) = (x.floor(), y.floor());
        let (xi, yi) = ((xf as i64 & 255) as usize, (yf as i64 & 255) as usize);
        let (dx, dy) = (x - xf, y - yf);
        let h = |i: usize, j: usize| self.perm[self.perm[xi + i] as usize + yi + j];
        let g = |hash: u8, x: f64, y: f64| {
            let a = (hash & 7) as f64 * std::f64::consts::FRAC_PI_4;
            a.cos() * x + a.sin() * y
        };
        let fade = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let (u, v) = (fade(dx), fade(dy));
        let lerp = |a: f64, b: f64, t: f64| a + t * (b - a);
        let n00 = g(h(0, 0), dx, dy);
        let n10 = g(h(1, 0), dx - 1.0, dy);
        let n01 = g(h(0, 1), dx, dy - 1.0);
        let n11 = g(h(1, 1), dx - 1.0, dy - 1.0);
        std::f64::consts::SQRT_2 * lerp(lerp(n00, n10, u), lerp(n01, n11, u), v)
    }

    /// `octaves` layers of noise, each twice as fine and half as strong,
    /// normalised back to roughly `[-1, 1]`.
    pub fn fbm(&self, x: f64, y: f64, octaves: usize) -> f64 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for o in 0..octaves {
            // Offset each octave so their lattices don't line up.
            let off = o as f64 * 17.3;
            sum += amp * self.noise(x * freq + off, y * freq - off);
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_bounded_and_seeded() {
        let a = Perlin::new(1);
        let b = Perlin::new(2);
        let mut differs = false;
        let mut prev = a.noise(0.0, 0.3);
        for i in 1..2000 {
            let x = i as f64 * 0.01;
            let n = a.noise(x, 0.3);
            assert!(n.abs() <= 1.0 + 1e-9, "noise {n} out of range");
            assert!((n - prev).abs() < 0.05, "jump at x={x}");
            prev = n;
            differs |= (n - b.noise(x, 0.3)).abs() > 1e-3;
        }
        assert!(differs, "seed has no effect");
        assert_eq!(a.noise(0.0, 0.0), 0.0, "zero on the lattice");
    }
}
