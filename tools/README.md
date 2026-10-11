# Plot-sheet tool

Draw a page of penfill patterns side by side and write it as one SVG, ready
for a pen plotter. Each pattern gets a rectangle, filled and labelled with its
name and settings.

![A sheet of pattern swatches](https://raw.githubusercontent.com/pileoni/penfill/master/examples/gallery/plot-sheet.svg)

## Use

```sh
pip install penfill
python tools/plot_sheet.py tools/plot_sheet.example.json -o sheet.svg
```

Everything is in millimetres. The page above is 152.4 × 101.6 mm — 6 × 4 in,
A5 landscape.

## Config

A JSON file. Only `patterns` is required; everything else has a default.

```json
{
  "page": { "width": 152.4, "height": 101.6 },
  "margin": 6.0,
  "gap": 2.0,
  "columns": 6,
  "inset": 1.5,
  "label": "name  spacing",
  "patterns": [
    { "name": "zigzag", "spacing": 1.0 },
    { "name": "lines",  "spacing": 1.2, "angle": 45 },
    { "name": "dots",   "spacing": 2.0 },
    { "name": "pat",    "pat": "BRICK", "scale": 2.0 }
  ]
}
```

| key | default | what it does |
| --- | --- | --- |
| `page.width`, `page.height` | 152.4 × 101.6 | the SVG's size, mm |
| `margin` | 6.0 | paper border around the grid, mm |
| `gap` | 2.0 | space between cells, mm |
| `columns` | 6 | the widest grid allowed; the tool squares up the cells inside it |
| `cell_size` | none | fixed cell size, mm: `15` for squares or `{"width": 20, "height": 12}`; cells pack from the top-left instead of filling the page |
| `inset` | 1.5 | how far the fill stays off the cell wall, mm |
| `label_height` | 4.5 | minimum strip under each cell for its label, mm; it grows to fit every line |
| `label` | `"name  spacing"` | label text: `"none"`, `"name"`, or a template like `"{name} {angle}°"`; the default puts the name on one line and spacing/angle on the next, and `\n` starts a new line |
| `font_size` | 1.8 | label cap height, mm; a label wider than its cell is shrunk to fit |
| `stroke_width`, `outline`, `ink`, `text` | 0.3, colours as above | how the sheet is drawn |

Labels are drawn with a single-stroke font ([`plotfont.py`](plotfont.py)), so
they are plain paths a pen can draw, not `<text>`. Every cell is its own
Inkscape layer, in reading order, holding its outline, its fill and its
label. Layers are named `1 zigzag  1 mm`, `2 …`, so AxiDraw (or any tool that
plots layer by layer) finishes one cell before starting the next.

Each entry in `patterns` names a fill and passes it the arguments it takes:
`spacing`, `angle`, `seed`, `mark`, `lattice`, `cell`, `pat`, `scale`,
`line_width`, `size`, `hatch`, `centre`. A `label` on the entry overrides the
label for that cell. Any other key is an error, so a typo is caught instead of
silently ignored.

## Command line

Overrides the config, when you want a quick tweak:

```sh
python tools/plot_sheet.py sheet.json --page-width 210 --page-height 297 --columns 8
```

`-o` sets the output path (default: the config's name with `.svg`).

The 6 × 4 in example above, in inches and millimetres, is
[`plot_sheet.example.json`](plot_sheet.example.json).
