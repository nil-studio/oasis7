"""Offline PNG oracles for the bounded material_shells browser fixture (Pillow)."""
import json
import math
from pathlib import Path
import sys
from PIL import Image


def factory(root, profile, anchor):
    sprite = Image.open(root / "smelter-source.png").convert("RGBA")
    assert sprite.size == (48, 48)
    a = json.loads(anchor)
    unit = max(1, min(a["canvasWidth"] // 640, a["canvasHeight"] // 360))
    ux = unit * a["cssWidth"] / a["canvasWidth"]
    uy = unit * a["cssHeight"] / a["canvasHeight"]
    phases = {}
    for phase in ["initial", "unknown", "restored", "deleted", "recreated"]:
        shot = Image.open(root / f"{profile}-material-{phase}.png").convert("RGB")
        scale = shot.width / a["width"]
        matched = total = 0
        for y in range(48):
            for x in range(48):
                rgba = sprite.getpixel((x, y))
                if rgba[3] != 255:
                    continue
                px = math.floor((a["x"] + (x - 24 + 0.5) * ux) * scale)
                py = math.floor((a["y"] + a["scroll"] + (y - 24 + 0.5) * uy) * scale)
                total += 1
                if 0 <= px < shot.width and 0 <= py < shot.height:
                    matched += all(abs(shot.getpixel((px, py))[c] - rgba[c]) <= 3 for c in range(3))
        ratio = matched / max(total, 1)
        present = phase in ["initial", "restored", "recreated"]
        phases[phase] = {"matched": matched, "total": total, "ratio": ratio,
                         "passed": ratio >= 0.9 if present else ratio <= 0.05}
    return {"passed": all(p["passed"] for p in phases.values()), "unit": [ux, uy], "phases": phases}


def terrain(root, profile):
    # Fixed controlled loc-0 placement, independent of the material implementation.
    # This deliberately covers the visible ore patch only, not the whole fragment.
    roi = {"desktop": (569, 63, 605, 99), "narrow": (147, 94, 183, 130),
           "dpr2": (1138, 126, 1210, 198)}[profile]
    baseline = Image.open(root / f"{profile}-terrain-initial.png").convert("RGB").crop(roi)
    before = list(baseline.getdata())
    phases = {}
    for phase in ["unknown", "restored", "deleted", "recreated"]:
        shot = Image.open(root / f"{profile}-terrain-{phase}.png").convert("RGB").crop(roi)
        changed = sum(a != b for a, b in zip(before, shot.getdata()))
        restored = phase in ["restored", "recreated"]
        phases[phase] = {"changedPixels": changed, "total": len(before),
                         "passed": changed == 0 if restored else changed / len(before) >= 0.9}
    return {"passed": all(p["passed"] for p in phases.values()), "roi": roi,
            "scope": "controlled visible patch delta and exact restoration", "phases": phases}


if __name__ == "__main__":
    mode, directory, profile, *extra = sys.argv[1:]
    root = Path(directory)
    result = factory(root, profile, *extra) if mode == "factory" else terrain(root, profile)
    print(json.dumps(result))
