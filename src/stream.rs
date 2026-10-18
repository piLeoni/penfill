//! Streaming fills: generate strokes in batches, clip and hand them on
//! without ever holding the whole drawing in memory.
//!
//! Every classic fill builds its strokes all at once and clips once at the
//! end. On a big output — a 50 km strip of dots at 0.5 mm — that is millions
//! of polylines in a single `Vec` before a single one reaches the plotter,
//! and the peak is proportional to the *whole* canvas. In a browser (4 GB
//! wasm heap) that aborts.
//!
//! A [`Sink`] flips the flow: generators push strokes into it as they make
//! them, and the consumer keeps only what it is willing to hold. The unit of
//! work is a **row** (or a batch of rows) of the generator's own lattice, so
//! no geometric grid is imposed and there are no seams to stitch: the
//! strokes are exactly the ones the one-shot version produces, emitted
//! earlier.
//!
//! ```
//! use penfill::{dotswaves, Anchor, Region, Strokes};
//! use penfill::stream::Sink;
//!
//! let region = Region::from_ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
//! let mut sink: Strokes = Vec::new();
//! dotswaves::fill_dots_with_lattice_into(&region, 1.0, 0.0, 0.2, dotswaves::Lattice::Square, Anchor::LOCAL, &mut sink);
//! assert_eq!(sink, dotswaves::fill_dots_with_lattice(&region, 1.0, 0.0, 0.2, dotswaves::Lattice::Square, Anchor::LOCAL));
//! ```

use crate::geo::{Pt, Region, Strokes};

/// Where a fill puts the strokes it makes.
///
/// A generator never keeps more than the batch it is emitting; the sink
/// decides what to do with each stroke (count it, clip it, write it). The
/// callback form takes `F: FnMut(Vec<Pt>)`, so an iterator loop can feed it
/// one stroke at a time.
pub trait Sink {
    /// Take one finished polyline (mm, at least two points).
    fn stroke(&mut self, pts: Vec<Pt>);

    /// Take a run of polylines; the default just loops [`Sink::stroke`], so
    /// a sink that wants to see whole rows overrides this.
    fn batch(&mut self, strokes: Strokes) {
        for s in strokes {
            self.stroke(s);
        }
    }
}

impl Sink for Strokes {
    fn stroke(&mut self, pts: Vec<Pt>) {
        self.push(pts);
    }

    fn batch(&mut self, strokes: Strokes) {
        // Whole rows append without moving stroke by stroke.
        self.extend(strokes);
    }
}

impl<S: Sink + ?Sized> Sink for &mut S {
    fn stroke(&mut self, pts: Vec<Pt>) {
        (**self).stroke(pts);
    }

    fn batch(&mut self, strokes: Strokes) {
        (**self).batch(strokes);
    }
}

/// A sink that clips each stroke to a region as it arrives, so the
/// generator's raw geometry never lingers.
pub struct Clipped<'a, S: Sink> {
    region: &'a Region,
    inner: S,
}

impl<'a, S: Sink> Clipped<'a, S> {
    pub fn new(region: &'a Region, inner: S) -> Clipped<'a, S> {
        Clipped { region, inner }
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: Sink> Sink for Clipped<'_, S> {
    fn stroke(&mut self, pts: Vec<Pt>) {
        for cut in self.region.clip_polyline(&pts) {
            self.inner.stroke(cut);
        }
    }

    fn batch(&mut self, strokes: Strokes) {
        for s in &strokes {
            for cut in self.region.clip_polyline(s) {
                self.inner.stroke(cut);
            }
        }
    }
}

/// A sink that clips each stroke to `region` and applies perimeter edge
/// fraying (`fray` mm) as strokes arrive, without holding the whole drawing.
pub struct Frayed<'a, S: Sink> {
    region: &'a Region,
    spacing: f64,
    fray: f64,
    seed: u64,
    noise: crate::noise::Perlin,
    index: usize,
    inner: S,
}

impl<'a, S: Sink> Frayed<'a, S> {
    pub fn new(region: &'a Region, spacing: f64, fray: f64, seed: u64, inner: S) -> Frayed<'a, S> {
        Frayed {
            region,
            spacing,
            fray,
            seed,
            noise: crate::noise::Perlin::new(seed ^ 0xF4A7_5EA1),
            index: 0,
            inner,
        }
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: Sink> Sink for Frayed<'_, S> {
    fn stroke(&mut self, pts: Vec<Pt>) {
        for cut in self.region.clip_polyline(&pts) {
            if self.fray <= 0.0 || self.spacing <= 0.0 {
                self.inner.stroke(cut);
            } else {
                let idx = self.index;
                self.index += 1;
                crate::fray::fray_clipped_stroke(
                    self.region,
                    cut,
                    idx,
                    self.spacing,
                    self.fray,
                    self.seed,
                    &self.noise,
                    &mut self.inner,
                );
            }
        }
    }
}

/// The cheap probe: count strokes and their points without keeping them.
///
/// Run a fill into a `Meter` to know its size before running it for real.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Meter {
    pub strokes: usize,
    pub points: usize,
}

impl Sink for Meter {
    fn stroke(&mut self, pts: Vec<Pt>) {
        self.strokes += 1;
        self.points += pts.len();
    }
}

/// Estimate a fill's stroke count without keeping the geometry, by
/// generating into a [`Meter`] and clipping as we go.
pub fn meter(region: &Region, fill: impl FnOnce(&mut dyn Sink)) -> Meter {
    let mut m = Meter::default();
    {
        let mut clipped = Clipped::new(region, &mut m);
        fill(&mut clipped);
    }
    m
}

/// Split a large span of rows into batches of at most `batch` rows, so a
/// generator can hand `Sink::batch` whole groups and a consumer can bound
/// how much it holds.
///
/// Returns an iterator of `(start, end)` row-index pairs.
pub fn row_batches(count: usize, batch: usize) -> impl Iterator<Item = (usize, usize)> {
    let batch = batch.max(1);
    (0..count).step_by(batch).map(move |start| (start, (start + batch).min(count)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region::from_ring(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)])
    }

    #[test]
    fn collect_matches_a_plain_vec() {
        let mut s: Strokes = Vec::new();
        s.stroke(vec![(0.0, 0.0), (1.0, 1.0)]);
        s.batch(vec![vec![(2.0, 2.0), (3.0, 3.0)]]);
        assert_eq!(s, vec![vec![(0.0, 0.0), (1.0, 1.0)], vec![(2.0, 2.0), (3.0, 3.0)]]);
    }

    #[test]
    fn clipped_sink_matches_a_clip_over_everything() {
        let region = square();
        let raw: Strokes = vec![vec![(-5.0, 15.0), (35.0, 15.0)], vec![(15.0, -5.0), (15.0, 35.0)]];
        let mut s: Strokes = Vec::new();
        {
            let mut c = Clipped::new(&region, &mut s);
            for line in &raw {
                c.stroke(line.clone());
            }
        }
        assert_eq!(s, region.clip_strokes(&raw));
    }

    #[test]
    fn frayed_sink_matches_fray_strokes() {
        let region = square();
        let raw: Strokes = (0..15).map(|i| vec![(-5.0, i as f64 * 2.0), (35.0, i as f64 * 2.0)]).collect();
        let mut s: Strokes = Vec::new();
        {
            let mut f = Frayed::new(&region, 2.0, 6.0, 11, &mut s);
            for line in &raw {
                f.stroke(line.clone());
            }
        }
        assert_eq!(s, crate::fray::fray_strokes(&region, raw, 2.0, 6.0, 11));
    }

    #[test]
    fn meter_counts_what_collect_keeps() {
        let region = square();
        let mut meter = Meter::default();
        {
            let mut c = Clipped::new(&region, &mut meter);
            c.batch(vec![vec![(-5.0, 15.0), (35.0, 15.0)], vec![(-5.0, 5.0), (35.0, 5.0)]]);
        }
        let kept = region.clip_strokes(&vec![vec![(-5.0, 15.0), (35.0, 15.0)], vec![(-5.0, 5.0), (35.0, 5.0)]]);
        assert_eq!(meter.strokes, kept.len());
        assert_eq!(meter.points, kept.iter().map(|s| s.len()).sum::<usize>());
    }

    #[test]
    fn row_batches_cover_every_row_once() {
        let got: Vec<_> = row_batches(7, 3).collect();
        assert_eq!(got, vec![(0, 3), (3, 6), (6, 7)]);
        assert_eq!(row_batches(0, 3).count(), 0);
    }
}
