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


def runtime(root, profile, metadata, material):
    a = json.loads(metadata)
    c = a["canvas"]
    step = max(1, min(c["width"] // 640, c["height"] // 360))
    ux, uy = step * c["cssWidth"] / c["width"], step * c["cssHeight"] / c["height"]
    cx, cy = c["cssWidth"] / 2, c["cssHeight"] / 2
    shots = {phase: Image.open(root / f"{profile}-runtime-{material}-{phase}.png").convert("RGB")
             for phase in ["initial", "absent", "restored", "deleted", "recreated"]}
    scale = shots["initial"].width / a["viewport"]["width"]
    assert shots["initial"].height / a["viewport"]["height"] == scale
    source = Image.open(root / f"{material}-source.png").convert("RGBA")
    if material == "depot":
        phases = {}
        for phase, shot in shots.items():
            matched = total = 0
            for y in range(32):
                for x in range(32):
                    rgba = source.getpixel((x, y))
                    if rgba[3] != 255:
                        continue
                    px = math.floor((cx + (x - 16 + 0.5) * ux) * scale)
                    py = math.floor((cy + (y - 16 + 0.5) * uy) * scale)
                    total += 1
                    matched += all(abs(shot.getpixel((px, py))[ch] - rgba[ch]) <= 3 for ch in range(3))
            ratio = matched / max(total, 1)
            phases[phase] = {"matched": matched, "total": total, "ratio": ratio,
                             "passed": ratio <= 0.05 if phase in ["absent", "deleted"] else ratio >= 0.8}
        return {"passed": all(p["passed"] for p in phases.values()), "source": "depot.png",
                "unit": [ux, uy], "phases": phases}

    def linear(v):
        v /= 255
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4

    def srgb(v):
        return 255 * (12.92 * v if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055)

    # Same base color/geometry/LOD and same inset parity; only the shared texture
    # shell differs. Predict the actual tinted alpha blend from source PNG and
    # the negative-control framebuffer, excluding pixels occluded by old details.
    alpha = round((0.38 + 0.28) * 255) / 255
    initial, absent = shots["initial"], shots["absent"]
    best = None
    for phase_x in [-0.5, 0, 0.5]:
        for phase_y in [-0.5, 0, 0.5]:
            matched = total = transparent = unchanged = changed = 0
            for py in range(math.ceil((cy - 20) * scale), math.floor((cy + 20) * scale)):
                for px in range(math.ceil((cx - 20) * scale), math.floor((cx + 20) * scale)):
                    sample_x = (math.floor((px + 0.5) / scale * c["width"] / c["cssWidth"] / step) + 0.5 + phase_x) * ux
                    sample_y = (math.floor((py + 0.5) / scale * c["height"] / c["cssHeight"] / step) + 0.5 + phase_y) * uy
                    tx = min(15, max(0, math.floor((sample_x - cx + 21) / 42 * 16)))
                    ty = min(15, max(0, math.floor((sample_y - cy + 21) / 42 * 16)))
                    rgba = source.getpixel((tx, ty))
                    known, base = initial.getpixel((px, py)), absent.getpixel((px, py))
                    changed += known != base
                    if rgba[3] == 0:
                        transparent += 1
                        unchanged += max(abs(known[ch] - base[ch]) for ch in range(3)) <= 1
                    else:
                        total += 1
                        expected = [srgb(alpha * linear(rgba[ch]) + (1 - alpha) * linear(base[ch])) for ch in range(3)]
                        matched += max(abs(known[ch] - expected[ch]) for ch in range(3)) <= 5
            result = {"matched": matched, "total": total, "ratio": matched / max(total, 1),
                      "transparent": transparent, "transparentUnchanged": unchanged,
                      "changedPixels": changed, "rasterPhaseLogicalPixels": [phase_x, phase_y]}
            if best is None or result["ratio"] > best["ratio"]:
                best = result
    roi = (math.floor((cx - 20) * scale), math.floor((cy - 20) * scale),
           math.ceil((cx + 20) * scale), math.ceil((cy + 20) * scale))
    restoration = {phase: list(shots[phase].crop(roi).getdata()) == list(initial.crop(roi).getdata())
                   for phase in ["restored", "recreated"]}
    deletion_changed = sum(a != b for a, b in zip(initial.crop(roi).getdata(), shots["deleted"].crop(roi).getdata()))
    passed = (deletion_changed > 100 and best["total"] >= 100 and best["ratio"] >= 0.75 and best["changedPixels"] > 100
              and best["transparent"] > 0 and best["transparentUnchanged"] / best["transparent"] >= 0.95
              and all(restoration.values()))
    return {"passed": passed, "source": "ore.png", "alpha": alpha, "blendModel": "sRGB decoded linear alpha over same-color base negative control",
            "sourcePixelMatch": best, "deletionChangedPixels": deletion_changed, "roi": roi, "exactRestoration": restoration}


if __name__ == "__main__":
    mode, directory, profile, *extra = sys.argv[1:]
    root = Path(directory)
    if mode.startswith("runtime-"):
        result = runtime(root, profile, *extra, mode.removeprefix("runtime-"))
    else:
        result = factory(root, profile, *extra) if mode == "factory" else terrain(root, profile)
    print(json.dumps(result))
