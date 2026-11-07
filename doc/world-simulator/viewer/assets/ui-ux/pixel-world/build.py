#!/usr/bin/env python3
"""Export the design pixel sources and refresh only generated world layers.

Run --sprites-only with Python 3 and Pillow. Full scene/page exports also
require Inkscape on PATH. This does
not build or change the Bevy renderer. Atlas symbols are the drawing source;
scene.svg owns their illustrative placement, not authoritative world positions.
"""

import argparse
import copy
import json
import math
import re
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent
UI = ROOT.parent
NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", NS)


def tag(name):
    return f"{{{NS}}}{name}"


def xml(element):
    text = ET.tostring(element, encoding="unicode")
    return text.replace(f' xmlns="{NS}"', "").rstrip()


def validate(root, metadata):
    allowed = {"svg", "title", "desc", "metadata", "defs", "symbol", "g", "rect", "use"}
    palette = set(metadata["palette"].values())
    for element in root.iter():
        kind = element.tag.rsplit("}", 1)[-1]
        assert kind in allowed, f"Unsupported world geometry: {kind}"
        for key in ("x", "y", "width", "height"):
            if key in element.attrib:
                assert re.fullmatch(r"-?\d+", element.get(key)), element.attrib
        assert not {"opacity", "fill-opacity", "stroke", "filter", "transform", "rx", "ry"} & element.attrib.keys(), element.attrib
        if "fill" in element.attrib:
            assert element.get("fill") in palette, element.attrib
        if kind == "use":
            asset = element.get("href").removeprefix("#")
            assert [int(element.get("width")), int(element.get("height"))] == metadata["asset_sizes"][asset]


def camera(region):
    x, y, width, height = region
    scale = max(1, math.ceil(max(width / 320, height / 180)))
    ox = min(0, max(width - 320 * scale, math.floor(width / 2 - 168 * scale)))
    oy = min(0, max(height - 180 * scale, math.floor(height / 2 - 88 * scale)))
    return x + ox, y + oy, scale


def world_layer(scene, region, element_id):
    x, y, width, height = region
    ox, oy, scale = camera(region)
    root = ET.Element(tag("g"), {"id": element_id, "data-source": "pixel-world/scene.svg", "data-scale": str(scale)})
    definitions = ET.SubElement(root, tag("defs"))
    clip = ET.SubElement(definitions, tag("clipPath"), {"id": "pw-clip"})
    ET.SubElement(clip, tag("rect"), dict(zip(("x", "y", "width", "height"), map(str, region))))
    crop = ET.SubElement(root, tag("g"), {"clip-path": "url(#pw-clip)"})
    pixels = ET.SubElement(crop, tag("g"), {"transform": f"translate({ox} {oy}) scale({scale})", "shape-rendering": "crispEdges"})
    for child in scene:
        if child.tag not in (tag("defs"), tag("g")):
            continue
        cloned = copy.deepcopy(child)
        for element in cloned.iter():
            if "id" in element.attrib:
                element.set("id", "pw-" + element.get("id"))
            if "href" in element.attrib:
                element.set("href", element.get("href").replace("#", "#pw-", 1))
        pixels.append(cloned)
    return xml(root)


def agent_layer(region, foot):
    ox, oy, scale = camera(region)
    x, y = ox + (foot[0] - 8) * scale, oy + (foot[1] - 15) * scale
    return f'<g id="pixel-agent" data-asset="atlas.svg#agent" data-foot="{foot[0]} {foot[1]}" transform="translate({x} {y}) scale({scale})" shape-rendering="crispEdges"><use href="#pw-agent" width="16" height="16"/></g>'


def replace_block(path, name, content):
    source = path.read_text()
    pattern = rf"<!-- pixel-world:{name}:begin -->.*?<!-- pixel-world:{name}:end -->"
    replacement = f"<!-- pixel-world:{name}:begin -->\n{content}\n<!-- pixel-world:{name}:end -->"
    updated, count = re.subn(pattern, lambda _: replacement, source, flags=re.S)
    assert count == 1, f"Missing or repeated generated block {name}: {path}"
    path.write_text(updated)


def export(source, output, palette, transparent=False):
    subprocess.run(["inkscape", str(source), "--export-type=png", "--export-area-page", f"--export-filename={output}"], check=True, capture_output=True)
    image = Image.open(output).convert("RGBA")
    colors = image.getcolors(image.width * image.height)
    assert colors is not None
    allowed = {tuple(bytes.fromhex(color[1:])) for color in palette}
    for _, (r, g, b, a) in colors:
        assert a in ((0, 255) if transparent else (255,)), (r, g, b, a)
        assert a == 0 or (r, g, b) in allowed, (r, g, b, a)
    # Strip renderer metadata and stabilize the small checked-in PNG exports.
    image.save(output, optimize=True)
    print(f"{output.name}: {image.width}x{image.height}, {len(colors)} RGBA colors")


def export_sprite(atlas, metadata, name):
    # Validated symbols contain only opaque integer rectangles. Half-open pixel
    # bounds preserve SVG coverage without antialiasing or another art source.
    width, height = metadata["asset_sizes"][name]
    symbol = atlas.find(f".//{tag('symbol')}[@id='{name}']")
    assert symbol is not None
    assert symbol.get("viewBox") == f"0 0 {width} {height}"
    image = Image.new("RGBA", (width, height), (255, 255, 255, 0))
    for rectangle in symbol:
        if rectangle.tag in (tag("title"), tag("desc")):
            continue
        assert rectangle.tag == tag("rect"), f"Unsupported sprite geometry: {name}"
        x, y, w, h = (int(rectangle.get(key, "0")) for key in ("x", "y", "width", "height"))
        assert w > 0 and h > 0
        image.paste(rectangle.get("fill"), (x, y, x + w, y + h))
    output = ROOT / f"{name}.png"
    image.save(output, optimize=True)
    print(f"{output.name}: {width}x{height}, source atlas.svg#{name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sprites-only", action="store_true", help="export validated atlas symbols with Pillow; no Inkscape needed")
    args = parser.parse_args()
    atlas = ET.parse(ROOT / "atlas.svg").getroot()
    metadata = json.loads(atlas.find(tag("metadata")).text)
    validate(atlas, metadata)
    for name in ("agent", "ore", "smelter", "depot"):
        export_sprite(atlas, metadata, name)
    if args.sprites_only:
        return
    scene_path = ROOT / "scene.svg"
    scene_text = scene_path.read_text()
    # The scene placement is authored; its embedded symbols/metadata are synced.
    scene_text = re.sub(r"<defs>.*?</defs>", lambda _: xml(atlas.find(tag("defs"))), scene_text, flags=re.S)
    scene_text = re.sub(r'<metadata id="pixel-world-metadata">.*?</metadata>', lambda _: xml(atlas.find(tag("metadata"))), scene_text, flags=re.S)
    scene_path.write_text(scene_text)
    scene = ET.fromstring(scene_text)
    validate(scene, metadata)
    palette = metadata["palette"].values()
    export(scene_path, ROOT / "scene.png", palette)
    frames = [
        ("16-low-height-command-expanded.svg", (0, 0, 484, 390), "world-stage", (136, 77)),
        ("17-dense-and-unknown-targets.svg", (0, 0, 1048, 900), "world-stage", None),
        ("full-function/gameplay/frames/industrial-native-plan.svg", (0, 56, 944, 844), "terrain", (216, 131)),
        ("full-function/gameplay/frames/industrial-native-result.svg", (0, 56, 944, 844), "terrain", (216, 131)),
    ]
    for name, region, element_id, foot in frames:
        path = UI / name
        replace_block(path, "scene", world_layer(scene, region, element_id))
        if foot is not None:
            replace_block(path, "agent", agent_layer(region, foot))
        print(f"{name}: integer camera {camera(region)}")


if __name__ == "__main__":
    main()
