"""Paired native-label pixels, not a DOM/capacity proxy (requires Pillow)."""
import json
import math
import sys
from pathlib import Path
from PIL import Image


def inspect(directory):
    root = Path(directory)
    states = json.loads((root / "label-captures.json").read_text())
    positive, negative, restored = [Image.open(root / f"labels-{p}.png").convert("RGB")
                                    for p in ["positive", "negative", "restored"]]
    first = states["positive"]
    rows = first["metrics"]["label_layouts"]
    assert len(rows) == 200 and len({r["frame_entity"] for r in rows}) == 200
    assert first["metrics"]["laid_out_in_view_labels"] == 200
    assert positive.size == negative.size == restored.size
    scale = positive.width / first["viewport"]["width"]
    canvas = first["canvas"]
    measurements = []
    for row in rows:
        r = row["css_rect"]
        assert row["glyph_count"] > 0 and r["right"] > r["left"] and r["bottom"] > r["top"]
        # Observer coordinates are canvas-local CSS, screenshots are full-page.
        box = (math.floor((canvas["left"] + first["scrollX"] + r["left"]) * scale),
               math.floor((canvas["top"] + first["scrollY"] + r["top"]) * scale),
               math.ceil((canvas["left"] + first["scrollX"] + r["right"]) * scale),
               math.ceil((canvas["top"] + first["scrollY"] + r["bottom"]) * scale))
        assert 0 <= box[0] < box[2] <= positive.width and 0 <= box[1] < box[3] <= positive.height
        a, b, c = [list(im.crop(box).getdata()) for im in [positive, negative, restored]]
        changes = sum(max(abs(x[k] - y[k]) for k in range(3)) > 8 for x, y in zip(a, b))
        equal = sum(max(abs(x[k] - y[k]) for k in range(3)) <= 3 for x, y in zip(a, c))
        measurements.append({"frame_entity": row["frame_entity"], "box_physical": box,
                             "source_glyphs": row["glyph_count"], "changed_label_pixels": changes,
                             "restoration_ratio": equal / len(a),
                             "passed": changes > 0 and equal / len(a) >= .98})
    return {"scope": "200 post-layout/in-view native Text2d boxes, paired normal/zero-width-name/normal source reads; not universal readability or GPU fence",
            "passed": all(r["passed"] for r in measurements), "labels": len(measurements),
            "all_boxes_show_label_only_change": all(r["changed_label_pixels"] > 0 for r in measurements),
            "rows": measurements}


if __name__ == "__main__":
    print(json.dumps(inspect(sys.argv[1])))
