"""The plot-sheet tool: a config in, one SVG with a labelled grid out."""

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools"))

import plot_sheet  # noqa: E402


def write(tmp_path, data):
    p = tmp_path / "sheet.json"
    p.write_text(json.dumps(data))
    return p


def run(tmp_path, data, *args):
    cfg_path = write(tmp_path, data)
    out = tmp_path / "sheet.svg"
    assert plot_sheet.main([str(cfg_path), "-o", str(out), *args]) == 0
    return out.read_text()


BASE = {
    "page": {"width": 100.0, "height": 80.0},
    "patterns": [{"name": "zigzag", "spacing": 1.0, "angle": 45}],
}


def test_page_size_is_honoured(tmp_path):
    svg = run(tmp_path, BASE)
    assert "width='100.000mm'" in svg and "height='80.000mm'" in svg
    assert "viewBox='0 0 100.000 80.000'" in svg


def test_every_pattern_gets_a_cell_and_a_label(tmp_path):
    svg = run(tmp_path, {
        "page": {"width": 100.0, "height": 80.0},
        "patterns": [
            {"name": "zigzag", "spacing": 1.0},
            {"name": "dots", "spacing": 2.0, "angle": 0},
            {"name": "honeycomb", "spacing": 3.0},
        ],
    })
    # one rectangle per cell and nothing else: a plotter would trace a page
    # background as a border around the sheet
    assert svg.count("<rect ") == 3
    # one layer per cell, in order, each named after its label
    assert svg.count("inkscape:groupmode='layer'") == 3
    assert svg.index("inkscape:label='1 zigzag  1 mm'") < svg.index("inkscape:label='2 dots  2 mm'")
    assert "inkscape:label='3 honeycomb  3 mm'" in svg


def test_labels_are_strokes_not_text(tmp_path):
    with_label = run(tmp_path, BASE)
    without = run(tmp_path, {**BASE, "label": "none"})
    assert "<text" not in with_label
    assert with_label.count("<path ") > without.count("<path ")


def test_each_layer_holds_its_outline_fill_and_label(tmp_path):
    import xml.etree.ElementTree as ET

    svg = run(tmp_path, {
        "page": {"width": 100.0, "height": 80.0},
        "patterns": [{"name": "zigzag", "spacing": 1.0}, {"name": "dots", "spacing": 2.0}],
    })
    root = ET.fromstring(svg)
    ns = "{http://www.w3.org/2000/svg}"
    layers = root.findall(f"{ns}g")
    assert len(layers) == 2
    for layer in layers:
        assert layer.find(f"{ns}rect") is not None
        fill, label = layer.findall(f"{ns}g")
        assert fill.findall(f"{ns}path") and label.findall(f"{ns}path")


def test_label_can_be_overridden_and_suppressed(tmp_path):
    svg = run(tmp_path, {
        "patterns": [{"name": "zigzag", "spacing": 1.0, "label": "mine"}],
        "label": "name",
    })
    assert "data-label='mine'" in svg
    svg = run(tmp_path, {"patterns": [{"name": "zigzag", "spacing": 1.0}], "label": "none"})
    assert "data-label=''" in svg


def test_default_label_puts_the_name_over_the_settings():
    assert plot_sheet.label_lines("name  spacing", {"name": "zigzag", "spacing": 0.5, "angle": 45}) == [
        "zigzag", "0.5 mm  45°"]
    assert plot_sheet.label_lines("name  spacing", {"name": "chess"}) == ["chess"]
    assert plot_sheet.label_lines("name", {"name": "a", "label": "one\ntwo"}) == ["one", "two"]


def test_dots_hex_is_shorthand_for_the_hex_lattice(tmp_path):
    svg = run(tmp_path, {"patterns": [{"name": "dots hex", "spacing": 1.0}]})
    assert "inkscape:label='1 dots hex  1 mm'" in svg
    assert svg != run(tmp_path, {"patterns": [{"name": "dots", "spacing": 1.0}]})


def test_angle_shows_in_the_default_label(tmp_path):
    svg = run(tmp_path, BASE)
    assert "45°" in svg


def test_a_long_label_is_shrunk_to_its_cell(tmp_path):
    import plotfont

    svg = run(tmp_path, {
        "page": {"width": 60.0, "height": 40.0},
        "columns": 1,
        "patterns": [{"name": "zigzag", "spacing": 1.0, "label": "a very long label " * 4}],
    })
    xs = [float(p.split(",")[0]) for p in svg.split("<g fill='none' stroke='#6b645c'>")[1].split("</g>")[0]
          .replace("<path d='M", " ").replace("'/>", " ").replace("L", " ").split()]
    assert min(xs) >= 6.0 - 1e-6 and max(xs) <= 54.0 + 1e-6
    assert plotfont.text_width("a very long label " * 4, 1.8) > 48.0


def test_inset_keeps_the_fill_off_the_wall(tmp_path):
    small = run(tmp_path, {
        "page": {"width": 100.0, "height": 80.0},
        "inset": 0.5,
        "patterns": [{"name": "lines", "spacing": 2.0}],
    })
    big = run(tmp_path, {
        "page": {"width": 100.0, "height": 80.0},
        "inset": 12.0,
        "patterns": [{"name": "lines", "spacing": 2.0}],
    })
    # a thicker inset leaves less wall to hatch, so less ink
    assert big.count("<path ") < small.count("<path ")


def test_pat_takes_its_name_from_the_pat_key(tmp_path):
    svg = run(tmp_path, {"patterns": [{"name": "pat", "pat": "BRICK", "scale": 2.0}]})
    assert "<path " in svg


def test_unknown_pattern_is_reported(tmp_path):
    cfg = write(tmp_path, {"patterns": [{"name": "not-a-fill"}]})
    with pytest.raises(SystemExit) as e:
        plot_sheet.main([str(cfg)])
    assert "unknown pattern" in str(e.value)


def test_unknown_key_is_reported(tmp_path):
    cfg = write(tmp_path, {"patterns": [{"name": "zigzag", "speed": 3}]})
    with pytest.raises(SystemExit) as e:
        plot_sheet.main([str(cfg)])
    assert "unsupported key" in str(e.value)


def test_a_missing_pattern_list_is_reported(tmp_path):
    cfg = write(tmp_path, {"page": {"width": 100.0, "height": 80.0}})
    with pytest.raises(SystemExit) as e:
        plot_sheet.main([str(cfg)])
    assert "patterns" in str(e.value)


def test_too_many_patterns_for_the_page_is_reported(tmp_path):
    data = {"page": {"width": 30.0, "height": 20.0},
            "patterns": [{"name": "zigzag", "spacing": 1.0} for _ in range(40)]}
    cfg = write(tmp_path, data)
    with pytest.raises(SystemExit) as e:
        plot_sheet.main([str(cfg)])
    assert "do not fit" in str(e.value)


def test_bad_json_is_reported(tmp_path):
    cfg = tmp_path / "bad.json"
    cfg.write_text("{not json")
    with pytest.raises(SystemExit) as e:
        plot_sheet.main([str(cfg)])
    assert "not valid JSON" in str(e.value)


def test_one_bad_entry_does_not_lose_the_whole_sheet(tmp_path, capsys):
    svg = run(tmp_path, {
        "page": {"width": 100.0, "height": 80.0},
        "patterns": [
            {"name": "zigzag", "spacing": 1.0},
            {"name": "dots", "spacing": 2.0, "mark": "wide"},  # not a number
        ],
    })
    assert "<path " in svg  # the good cell is still drawn
    assert "failed" in capsys.readouterr().err
