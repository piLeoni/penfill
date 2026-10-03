//! Where a picture or a drawing sits inside the region, the way CSS places
//! a background image: a `size`, a `position` and a `repeat`, measured
//! against the region's bounding box. Whatever comes out is cut by the
//! region's outline.
//!
//! Strings use the CSS syntax, with lengths in millimetres:
//! - size: `cover`, `contain`, `fill` (stretch to the box), `40` (40 mm
//!   wide), `40 30`, `auto 30`, `50%`;
//! - position: `center`, `top left`, `right 10`, `25% 75%`;
//! - repeat: `no-repeat`, `repeat`, `repeat-x`, `repeat-y`.
//!
//! "Top" is the smallest y, as in SVG.

use crate::geo::Pt;
use std::str::FromStr;

/// A length along one side of the box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Mm(f64),
    /// Fraction of the box side (CSS `50%` is `Fraction(0.5)`).
    Fraction(f64),
}

impl Length {
    fn of(self, side: f64) -> f64 {
        match self {
            Length::Mm(v) => v,
            Length::Fraction(f) => f * side,
        }
    }
}

/// How big one copy is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    /// As small as possible while covering the whole box.
    Cover,
    /// As big as possible while fitting inside the box.
    Contain,
    /// Stretched to the box, proportions lost (CSS `100% 100%`).
    Stretch,
    /// This wide, height in proportion.
    Width(Length),
    /// This tall, width in proportion.
    Height(Length),
    /// Exactly this wide and tall.
    Exact(Length, Length),
}

/// Whether copies repeat to fill the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    NoRepeat,
    Repeat,
    RepeatX,
    RepeatY,
}

impl Repeat {
    fn axes(self) -> (bool, bool) {
        match self {
            Repeat::NoRepeat => (false, false),
            Repeat::Repeat => (true, true),
            Repeat::RepeatX => (true, false),
            Repeat::RepeatY => (false, true),
        }
    }
}

/// CSS-style placement. The default is `cover`, centred, no repeat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub size: Size,
    /// Where the first copy sits, per axis. A fraction places the copy's
    /// point at that fraction on the box's point at that fraction (CSS
    /// percentages: 0 = left/top, 0.5 = centre, 1 = right/bottom); mm is an
    /// offset from the left/top.
    pub position: (Length, Length),
    pub repeat: Repeat,
}

impl Default for Placement {
    fn default() -> Placement {
        Placement { size: Size::Cover, position: (Length::Fraction(0.5), Length::Fraction(0.5)), repeat: Repeat::NoRepeat }
    }
}

/// One copy: corner and size, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// More copies than this and the placement gives up (a size of a few µm
/// repeated over a sheet is a mistake, not a pattern).
const MAX_TILES: usize = 200_000;

impl Placement {
    /// From CSS strings; see the module docs.
    pub fn parse(size: &str, position: &str, repeat: &str) -> Result<Placement, String> {
        Ok(Placement { size: size.parse()?, position: parse_position(position)?, repeat: repeat.parse()? })
    }

    /// The copy that the position places, for content `natural` =
    /// (width, height) in any unit, in the box `bounds` = (min x, min y,
    /// max x, max y).
    pub fn tile(&self, natural: (f64, f64), bounds: (f64, f64, f64, f64)) -> Tile {
        let (bx, by, bw, bh) = (bounds.0, bounds.1, bounds.2 - bounds.0, bounds.3 - bounds.1);
        let (nw, nh) = (natural.0.max(1e-12), natural.1.max(1e-12));
        let (w, h) = match self.size {
            Size::Cover => {
                let s = (bw / nw).max(bh / nh);
                (nw * s, nh * s)
            }
            Size::Contain => {
                let s = (bw / nw).min(bh / nh);
                (nw * s, nh * s)
            }
            Size::Stretch => (bw, bh),
            Size::Width(l) => (l.of(bw), l.of(bw) * nh / nw),
            Size::Height(l) => (l.of(bh) * nw / nh, l.of(bh)),
            Size::Exact(a, b) => (a.of(bw), b.of(bh)),
        };
        let at = |l: Length, start: f64, room: f64, own: f64| match l {
            Length::Mm(v) => start + v,
            Length::Fraction(f) => start + (room - own) * f,
        };
        Tile { x: at(self.position.0, bx, bw, w), y: at(self.position.1, by, bh, h), w, h }
    }

    /// Where `p` lands in the content, as `(u, v)` in `[0, 1)` from its
    /// top-left corner, or `None` where no copy covers it.
    pub fn locate(&self, tile: Tile, p: Pt) -> Option<(f64, f64)> {
        if !(tile.w > 0.0 && tile.h > 0.0) {
            return None;
        }
        let (rx, ry) = self.repeat.axes();
        let (u, v) = ((p.0 - tile.x) / tile.w, (p.1 - tile.y) / tile.h);
        let fold = |t: f64, repeats: bool| {
            if repeats {
                Some(t.rem_euclid(1.0))
            } else if (0.0..1.0).contains(&t) {
                Some(t)
            } else {
                None
            }
        };
        Some((fold(u, rx)?, fold(v, ry)?))
    }

    /// Every copy that touches the box.
    pub fn tiles(&self, natural: (f64, f64), bounds: (f64, f64, f64, f64)) -> Vec<Tile> {
        let t = self.tile(natural, bounds);
        if !(t.w > 0.0 && t.h > 0.0) {
            return vec![];
        }
        let (rx, ry) = self.repeat.axes();
        let span = |repeats: bool, start: f64, size: f64, lo: f64, hi: f64| -> (i64, i64) {
            if repeats {
                (((lo - start) / size).floor() as i64, ((hi - start) / size).ceil() as i64 - 1)
            } else {
                (0, 0)
            }
        };
        let (i0, i1) = span(rx, t.x, t.w, bounds.0, bounds.2);
        let (j0, j1) = span(ry, t.y, t.h, bounds.1, bounds.3);
        let count = (i1 - i0 + 1).max(0) as u128 * (j1 - j0 + 1).max(0) as u128;
        if count > MAX_TILES as u128 {
            return vec![];
        }
        (j0..=j1)
            .flat_map(|j| (i0..=i1).map(move |i| Tile { x: t.x + i as f64 * t.w, y: t.y + j as f64 * t.h, w: t.w, h: t.h }))
            .collect()
    }
}

fn parse_length(s: &str) -> Result<Length, String> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        return p.trim().parse::<f64>().map(|v| Length::Fraction(v / 100.0)).map_err(|_| format!("bad length {s:?}"));
    }
    s.trim_end_matches("mm").trim().parse::<f64>().map(Length::Mm).map_err(|_| format!("bad length {s:?}"))
}

impl FromStr for Size {
    type Err = String;

    fn from_str(s: &str) -> Result<Size, String> {
        let words: Vec<&str> = s.split_whitespace().collect();
        let size = match words.as_slice() {
            ["cover"] => Ok(Size::Cover),
            ["contain"] => Ok(Size::Contain),
            ["fill"] | ["stretch"] | ["100%", "100%"] => Ok(Size::Stretch),
            [w] | [w, "auto"] if *w != "auto" => parse_length(w).map(Size::Width),
            ["auto", h] if *h != "auto" => parse_length(h).map(Size::Height),
            [w, h] if *w != "auto" => parse_length(w).and_then(|w| Ok(Size::Exact(w, parse_length(h)?))),
            _ => Err(String::new()),
        };
        size.map_err(|_| format!("bad size {s:?}: use cover, contain, fill, a width (\"40\", \"50%\"), or width and height"))
    }
}

impl FromStr for Repeat {
    type Err = String;

    fn from_str(s: &str) -> Result<Repeat, String> {
        match s.trim() {
            "no-repeat" | "none" => Ok(Repeat::NoRepeat),
            "repeat" => Ok(Repeat::Repeat),
            "repeat-x" => Ok(Repeat::RepeatX),
            "repeat-y" => Ok(Repeat::RepeatY),
            other => Err(format!("repeat must be no-repeat, repeat, repeat-x or repeat-y, not {other:?}")),
        }
    }
}

/// CSS `background-position` with one or two values.
pub fn parse_position(s: &str) -> Result<(Length, Length), String> {
    let centre = Length::Fraction(0.5);
    let keyword = |w: &str| match w {
        "left" | "top" => Some(Length::Fraction(0.0)),
        "center" | "centre" => Some(centre),
        "right" | "bottom" => Some(Length::Fraction(1.0)),
        _ => None,
    };
    let value = |w: &str| keyword(w).map(Ok).unwrap_or_else(|| parse_length(w));
    let vertical = |w: &str| matches!(w, "top" | "bottom");
    let horizontal = |w: &str| matches!(w, "left" | "right");
    let words: Vec<&str> = s.split_whitespace().collect();
    match words.as_slice() {
        [] => Ok((centre, centre)),
        [w] if vertical(w) => Ok((centre, value(w)?)),
        [w] => Ok((value(w)?, centre)),
        [a, b] if vertical(a) || horizontal(b) => Ok((value(b)?, value(a)?)),
        [a, b] => Ok((value(a)?, value(b)?)),
        _ => Err(format!("bad position {s:?}: use one or two values, e.g. \"center\", \"top left\", \"25% 75%\"")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: (f64, f64, f64, f64) = (0.0, 0.0, 40.0, 20.0);

    #[test]
    fn sizes_like_css() {
        let tile = |size: &str| Placement::parse(size, "center", "no-repeat").unwrap().tile((10.0, 10.0), BOX);
        let t = tile("cover");
        assert_eq!((t.w, t.h, t.x, t.y), (40.0, 40.0, 0.0, -10.0));
        let t = tile("contain");
        assert_eq!((t.w, t.h, t.x, t.y), (20.0, 20.0, 10.0, 0.0));
        let t = tile("fill");
        assert_eq!((t.w, t.h), (40.0, 20.0));
        let t = tile("25%");
        assert_eq!((t.w, t.h), (10.0, 10.0));
        let t = tile("auto 5mm");
        assert_eq!((t.w, t.h), (5.0, 5.0));
        let t = tile("8 4");
        assert_eq!((t.w, t.h), (8.0, 4.0));
    }

    #[test]
    fn positions_like_css() {
        let at = |pos: &str| {
            let t = Placement::parse("10", pos, "no-repeat").unwrap().tile((1.0, 1.0), BOX);
            (t.x, t.y)
        };
        assert_eq!(at("top left"), (0.0, 0.0));
        assert_eq!(at("left top"), (0.0, 0.0));
        assert_eq!(at("bottom"), (15.0, 10.0));
        assert_eq!(at("right 3"), (30.0, 3.0));
        assert_eq!(at("50% 100%"), (15.0, 10.0));
        assert!(parse_position("a b c").is_err());
    }

    #[test]
    fn repeat_folds_and_no_repeat_leaves_gaps() {
        let p = Placement::parse("10", "top left", "repeat-x").unwrap();
        let t = p.tile((1.0, 1.0), BOX);
        assert_eq!(p.locate(t, (25.0, 5.0)), Some((0.5, 0.5)));
        assert_eq!(p.locate(t, (25.0, 15.0)), None, "repeat-x doesn't go down");
        let tiles = p.tiles((1.0, 1.0), BOX);
        assert_eq!(tiles.len(), 4);
        let all = Placement::parse("10", "center", "repeat").unwrap().tiles((1.0, 1.0), BOX);
        assert_eq!(all.len(), 5 * 3, "centred tiles overhang both sides");
        assert!(Placement::parse("0.000001", "center", "repeat").unwrap().tiles((1.0, 1.0), BOX).is_empty());
        assert!("tile".parse::<Repeat>().is_err());
    }
}
