#!/usr/bin/env python3
"""Lay a sheet of penfill patterns out as one SVG, ready for a pen plotter.

Read a small JSON config (page size, grid, margins, and the list of patterns
to draw), fill a rectangle per pattern, and write a single SVG.

Each cell is its own top-level `<g>` layer, in reading order, holding three
things: the cell outline, the fill, and the label. The label is written with
`tools/plotfont.py`, a single-stroke font, so every glyph is a `<path>` a pen
can draw - there are no `<text>` elements. Layers are in the order a pen should
plot them (one cell fully, then the next), which also makes the sheet easy to
reorder or export cell-by-cell.

    python tools/plot_sheet.py config.json -o sheet.svg

Everything is in millimetres. A config looks like:    {
      "page": { "width": 152.4, "height": 101.6 },
      "columns": 6,
      "inset": 1.5,
      "label": "name  spacing",
      "patterns": [
        { "name": "zigzag", "spacing": 1.0, "angle": 45 },
        { "name": "dots",   "spacing": 2.0 }
      ]
    }

Only `patterns` is required. Each entry names a fill (`zigzag`, `dots`,
`honeycomb`, ...) and passes whatever that fill needs: `spacing`, `angle`,
`seed`, `cell`, `mark`, `lattice`, `pat`, `scale`, `line_width`, `size`,
`hatch`, and so on. A `label` overrides the text drawn under the cell.

The cells are all rectangles of the same size: the grid is fitted to the page
(minus `margin`), the label strip is taken off the bottom of each cell, and the
fill is inset from the rectangle's wall by `inset` so the pen does not run over
the outline.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

import penfill
import plotfont


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("config", type=Path, help="JSON sheet description (see tools/plot_sheet.example.json)")
    p.add_argument("-o", "--out", type=Path, help="SVG to write (default: the config's name with .svg)")
    p.add_argument("--columns", type=int, help="override the config's column count")
    p.add_argument("--rows", type=int, help="pin the row count, wrapping into more columns instead")
    p.add_argument("--page-width", type=float, help="override the page width, mm")
    p.add_argument("--page-height", type=float, help="override the page height, mm")
    p.add_argument("--cell-size", type=float, nargs="+", metavar="MM",
                   help="fixed cell size, mm: one value for squares, or width height")
    p.add_argument("--inset", type=float, help="override the inset from the wall, mm")
    p.add_argument("--stroke-width", type=float, help="override the stroke width, mm")
    return p.parse_args(argv)


# --- config ---------------------------------------------------------------

DEFAULTS = {
    "margin": 6.0,
    "gap": 2.0,
    "columns": 6,
    "inset": 1.5,
    "label_height": 4.5,
    "stroke_width": 0.3,
    "outline": "#b7aea3",
    "ink": "#2a2622",
    "text": "#6b645c",
    "font_size": 1.8,
    "label": "name  spacing",
    "page": {"width": 152.4, "height": 101.6},  # A5 landscape, mm
}

# Argument name -> how to call the penfill fill. Only these keys are read
# from a pattern entry, so a typo is reported instead of ignored.
FILL_KEYS = {
    "spacing",
    "angle",
    "seed",
    "mark",
    "lattice",
    "cell",
    "pat",
    "scale",
    "line_width",
    "size",
    "hatch",
    "inset",
    "centre",
}


class ConfigError(SystemExit):
    pass


def load_config(path: Path) -> dict:
    try:
        raw = json.loads(path.read_text())
    except FileNotFoundError:
        raise ConfigError(f"config not found: {path}")
    except json.JSONDecodeError as e:
        raise ConfigError(f"{path}: not valid JSON ({e})")
    if not isinstance(raw, dict):
        raise ConfigError(f"{path}: the top level must be an object")
    cfg = dict(DEFAULTS)
    cfg["page"] = dict(DEFAULTS["page"])
    cfg.update({k: v for k, v in raw.items() if k not in ("page", "patterns")})
    if "page" in raw:
        cfg["page"].update(raw["page"])
    patterns = raw.get("patterns")
    if not patterns:
        raise ConfigError(f"{path}: 'patterns' must list at least one pattern")
    if not isinstance(patterns, list):
        raise ConfigError(f"{path}: 'patterns' must be a list")
    cfg["patterns"] = patterns
    return cfg


# --- fills ----------------------------------------------------------------

def fill_for(entry: dict) -> "penfill.Strokes":
    """Run one pattern entry against its rectangle of the sheet."""
    if not isinstance(entry, dict):
        raise ConfigError(f"a pattern entry must be an object, got {entry!r}")
    name = entry.get("name")
    if not name:
        raise ConfigError(f"a pattern entry has no 'name': {entry!r}")
    # `dots hex` is shorthand for `dots` with `"lattice": "hex"`.
    base, _, variant = str(name).partition(" ")
    if base == "dots" and variant:
        entry = {"lattice": variant.strip(), **entry, "name": base}
        name = base
    fill = getattr(penfill, name, None)
    if fill is None or not callable(fill):
        known = sorted(n for n in dir(penfill) if not n.startswith("_"))
        raise ConfigError(f"unknown pattern {name!r}. Known: {', '.join(known)}")

    unknown = set(entry) - FILL_KEYS - {"name", "label", "rect"}
    if unknown:
        raise ConfigError(f"{name!r}: unsupported key(s) {', '.join(sorted(unknown))}. Allowed: {', '.join(sorted(FILL_KEYS))}")

    kwargs = {k: entry[k] for k in FILL_KEYS if k in entry}
    if name == "pat" and "pat" in entry:
        # `pat(region, name, ...)`: the .pat name is `pat` in the config so
        # it does not clash with the fill's own `name`.
        kwargs["name"] = kwargs.pop("pat")
    if name == "tile" and "strokes" not in entry:
        raise ConfigError("'tile' needs a 'strokes' list, which no config key carries; use svg_tile instead")
    return fill(REGION, **kwargs)


# The rectangle currently being filled. Set by the layout, so `fill_for`
# stays a plain function of the entry.
REGION: "penfill.Region" = None  # type: ignore[assignment]


def draw(entry: dict, rect: "penfill.Region") -> "penfill.Strokes":
    global REGION
    REGION = rect
    try:
        return fill_for(entry)
    finally:
        REGION = None  # type: ignore[assignment]


# --- layout ---------------------------------------------------------------

def label_lines(template: str, entry: dict) -> list[str]:
    """The label under a cell, one string per line.

    `name  spacing` gives the pattern on the first line and its settings on
    the second: `zigzag` / `1 mm  45°`. A `label` on the entry overrides it;
    in a label or a template, `\\n` starts a new line.
    """
    if "label" in entry:
        text = str(entry["label"])
    elif template == "none":
        return []
    elif template == "name":
        text = str(entry["name"])
    elif template == "name  spacing":
        title = str(entry["name"])
        lattice = entry.get("lattice")
        if lattice not in (None, "square") and lattice not in title.split():
            title += f" {lattice}"
        specs = []
        for key in ("spacing", "size", "cell"):
            if key in entry:
                specs.append(f"{entry[key]:g} mm")
                break
        if entry.get("angle"):
            specs.append(f"{entry['angle']:g}°")
        return [title, "  ".join(specs)] if specs else [title]
    else:
        try:
            text = template.format(**entry)
        except (KeyError, IndexError) as e:
            raise ConfigError(f"label template {template!r} needs {e}")
    return [line for line in text.split("\n") if line.strip()]


def grid_shape(count: int, columns: int, rows: int | None, page_w: float, page_h: float,
               margin: float, gap: float, label_h: float) -> tuple[int, int]:
    """Pick `(cols, rows)` so the cells are as square as the page allows.

    `columns` is a cap, not a promise: with square cells a fixed column count
    would leave the grid short on one axis whenever the page is not shaped for
    it. `rows`, when given, wins outright.
    """
    if rows is not None:
        return max(1, math.ceil(count / rows)), rows

    cell_aspect = (page_w - 2 * margin) / max(page_h - 2 * margin, 1e-9)
    best = (1, count)
    best_score = -1.0
    for cols in range(1, min(columns, count) + 1):
        rows = math.ceil(count / cols)
        usable_w = page_w - 2 * margin - (cols - 1) * gap
        usable_h = page_h - 2 * margin - (rows - 1) * gap - rows * label_h
        cw = usable_w / cols
        ch = usable_h / rows
        if cw <= 0 or ch <= 0:
            continue
        # Score: how square the cell is, weighted by how much of the page it uses.
        squareness = min(cw, ch) / max(cw, ch)
        fill = (cw * ch * count) / max((page_w - 2 * margin) * (page_h - 2 * margin), 1e-9)
        score = squareness * 0.7 + fill * 0.3
        if score > best_score:
            best_score = score
            best = (cols, rows)
    return best


def build_svg(cfg: dict) -> str:
    page = cfg["page"]
    page_w = float(page["width"])
    page_h = float(page["height"])
    margin = float(cfg["margin"])
    gap = float(cfg["gap"])
    inset = float(cfg["inset"])
    patterns = cfg["patterns"]
    count = len(patterns)
    font = float(cfg["font_size"])
    labels = [label_lines(cfg["label"], entry) for entry in patterns]
    n_lines = max((len(lines) for lines in labels), default=0)
    # The strip under the cells grows to hold every line at full size: the
    # text is never shrunk to make room vertically.
    label_h = max(float(cfg["label_height"]), _label_height(n_lines, font)) if n_lines else 0.0

    fixed = _cell_size(cfg.get("cell_size"))
    if fixed:
        # Fixed cells: pack as many per row as the page (and `columns`) allow,
        # from the top-left, and leave the rest of the page blank.
        cell_w, cell_h = fixed
        fit = int((page_w - 2 * margin + gap + 1e-9) // (cell_w + gap))
        cols = max(1, min(fit, int(cfg["columns"]), count))
        rows = math.ceil(count / cols)
        if fit < 1 or margin + rows * (cell_h + label_h + gap) - gap > page_h - margin + 1e-9:
            raise ConfigError(
                f"{count} cells of {cell_w:g}×{cell_h:g} mm do not fit a {page_w:g}×{page_h:g} mm page "
                f"({cols}×{rows} grid). Try smaller cells, fewer patterns, or a bigger page."
            )
    else:
        cols, rows = grid_shape(count, int(cfg["columns"]), cfg.get("rows"), page_w, page_h, margin, gap, label_h)
        usable_w = page_w - 2 * margin - (cols - 1) * gap
        usable_h = page_h - 2 * margin - (rows - 1) * gap - rows * label_h
        cell_w = usable_w / cols
        cell_h = usable_h / rows
    if cell_w <= 0 or cell_h <= 0:
        raise ConfigError(
            f"{count} patterns do not fit {cols}×{rows} on a {page_w:g}×{page_h:g} mm page "
            f"(cells would be {cell_w:.1f}×{cell_h:.1f} mm). Try fewer patterns, smaller margins, or a bigger page."
        )

    out: list[str] = []
    out.append(
        f"<svg xmlns='http://www.w3.org/2000/svg' "
        f"xmlns:inkscape='http://www.inkscape.org/namespaces/inkscape' "
        f"width='{page_w:.3f}mm' height='{page_h:.3f}mm' "
        f"viewBox='0 0 {page_w:.3f} {page_h:.3f}'>"
    )
    frames: list[tuple[float, float, float, float]] = []
    for i, entry in enumerate(patterns):
        c = i % cols
        r = i // cols
        x0 = margin + c * (cell_w + gap)
        y0 = margin + r * (cell_h + label_h + gap)
        frames.append((x0, y0, cell_w, cell_h))

    # One Inkscape layer per cell, in reading order, so a plotter that draws
    # layers in document order fills each cell and labels it before moving on.
    # The layer name starts with its number, which AxiDraw reads as the layer
    # to plot. Each layer holds its outline, its fill, then its label.
    for i, entry in enumerate(patterns):
        x0, y0, w, h = frames[i]
        name = entry.get("name", f"cell-{i + 1}")
        text = "  ".join(labels[i])
        layer_name = f"{i + 1} {text or name}"
        out.append(
            f"<g id='{_layer_id(i, name)}' inkscape:groupmode='layer' "
            f"inkscape:label='{_esc(layer_name)}' data-label='{_esc(text)}' "
            f"stroke-linecap='round' stroke-linejoin='round' stroke-width='{cfg['stroke_width']}'>"
        )
        # outline
        out.append(f"<rect x='{x0:.3f}' y='{y0:.3f}' width='{w:.3f}' height='{h:.3f}' fill='none' stroke='{cfg['outline']}'/>")
        # fill
        # y grows down in the SVG; penfill works the same way as the rectangle
        # coordinates above, so the region needs no flip.
        cell = penfill.Region(
            [(x0 + inset, y0 + inset), (x0 + w - inset, y0 + inset),
             (x0 + w - inset, y0 + h - inset), (x0 + inset, y0 + h - inset)]
        )
        out.append(f"<g fill='none' stroke='{cfg['ink']}'>")
        try:
            strokes = draw(entry, cell)
        except Exception as e:  # keep going: one bad entry shouldn't lose the sheet
            print(f"warning: {name!r} failed ({e}); leaving an empty cell", file=sys.stderr)
            strokes = []
        for s in strokes:
            if len(s) < 2:
                continue
            out.append(f"<path d='{_path(s)}'/>")
        out.append("</g>")
        # label, drawn as strokes so a plotter can write it
        if labels[i]:
            out.append(f"<g fill='none' stroke='{cfg['text']}'>")
            baseline = y0 + h + font * (LABEL_PAD + 1.0)
            for line in labels[i]:
                for s in _label_strokes(font, line, x0 + w / 2.0, baseline, w):
                    out.append(f"<path d='{_path(s)}'/>")
                baseline += font * LINE_PITCH
            out.append("</g>")
        out.append("</g>")

    out.append("</svg>")
    return "\n".join(out) + "\n"


def _cell_size(value) -> tuple[float, float] | None:
    """`cell_size` as (width, height) mm: a number for squares, or {width, height}."""
    if value is None:
        return None
    try:
        if isinstance(value, (int, float)):
            w = h = float(value)
        elif isinstance(value, dict):
            w, h = float(value["width"]), float(value["height"])
        else:
            w, h = (float(v) for v in value)
    except (KeyError, TypeError, ValueError):
        raise ConfigError(f"cell_size must be a number or {{\"width\": ..., \"height\": ...}}, got {value!r}")
    if w <= 0 or h <= 0:
        raise ConfigError(f"cell_size must be positive, got {w:g}×{h:g}")
    return w, h


def _path(s) -> str:
    return "M" + "L".join(f"{p[0]:.3f},{p[1]:.3f}" for p in s)


def _layer_id(i: int, name: str) -> str:
    slug = "".join(c if c.isalnum() else "-" for c in str(name).lower()).strip("-") or "cell"
    return f"{i + 1:02d}-{slug}"


# Label layout, in multiples of the cap height: the space between the cell
# and the first line (and under the last), and baseline to baseline.
LABEL_PAD = 0.7
LINE_PITCH = 1.6


def _label_height(n_lines: int, font: float) -> float:
    return font * (2 * LABEL_PAD + 1.0 + (n_lines - 1) * LINE_PITCH)


def _label_strokes(size: float, text: str, cx: float, baseline_y: float, max_width: float):
    """One label line as pen strokes, centred on `cx`, sitting on `baseline_y`.

    `size` is the cap height in mm; a line wider than its cell is shrunk to
    fit rather than running into the neighbour.
    """
    width = plotfont.text_width(text, size)
    if width > max_width:
        size *= max_width / width
    return plotfont.text_strokes(text, cx, baseline_y, size, align="center")


def _esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("'", "&apos;")


# --- cli ------------------------------------------------------------------

def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    cfg = load_config(args.config)
    if args.columns is not None:
        cfg["columns"] = args.columns
    if args.inset is not None:
        cfg["inset"] = args.inset
    if args.stroke_width is not None:
        cfg["stroke_width"] = args.stroke_width
    if args.page_width is not None:
        cfg["page"]["width"] = args.page_width
    if args.page_height is not None:
        cfg["page"]["height"] = args.page_height
    if args.rows is not None:
        cfg["rows"] = args.rows
    if args.cell_size is not None:
        cfg["cell_size"] = args.cell_size[0] if len(args.cell_size) == 1 else args.cell_size[:2]

    svg = build_svg(cfg)
    out = args.out or args.config.with_suffix(".svg")
    out.write_text(svg)
    n = len(cfg["patterns"])
    print(f"wrote {out} · {n} pattern{'s' if n != 1 else ''} · "
          f"{cfg['page']['width']:g}×{cfg['page']['height']:g} mm")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
