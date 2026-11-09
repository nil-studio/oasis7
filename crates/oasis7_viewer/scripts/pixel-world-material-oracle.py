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
        # The original world status glyph is centered and native stock/throughput
        # readouts sit above it. The source lower quarter lies below their
        # footprints at every pixel unit; include the lower left frame so every
        # opaque source color participates in the spatial pattern.
        regions = [(0, 24, 32, 32), (6, 18, 8, 22)]
        expected_colors = {rgba[:3] for rgba in source.getdata() if rgba[3] == 255}
        for phase, shot in shots.items():
            matched = total = full_matched = full_total = 0
            colors = set()
            for y in range(32):
                for x in range(32):
                    rgba = source.getpixel((x, y))
                    if rgba[3] != 255:
                        continue
                    px = math.floor((cx + (x - 16 + 0.5) * ux) * scale)
                    py = math.floor((cy + (y - 16 + 0.5) * uy) * scale)
                    ok = all(abs(shot.getpixel((px, py))[ch] - rgba[ch]) <= 3 for ch in range(3))
                    full_total += 1
                    full_matched += ok
                    if any(r[0] <= x < r[2] and r[1] <= y < r[3] for r in regions):
                        total += 1
                        matched += ok
                        if ok:
                            colors.add(rgba[:3])
            ratio = matched / max(total, 1)
            phases[phase] = {"matched": matched, "total": total, "ratio": ratio,
                             "matchedSourceColorClasses": len(colors), "matchedSourceColors": [list(c) for c in sorted(colors)],
                             "fullSourceDiagnostic": {"matched": full_matched, "total": full_total, "ratio": full_matched / max(full_total, 1)},
                             "passed": ratio <= 0.05 if phase in ["absent", "deleted"] else ratio >= 0.95 and colors == expected_colors}
        return {"passed": all(p["passed"] for p in phases.values()), "source": "depot.png",
                "sourceRegions": regions, "expectedSourceColors": [list(c) for c in sorted(expected_colors)], "scope": "unoccluded lower-quarter and left-frame patterns; every opaque source color; semantic foregrounds retained",
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
    expected_colors = {rgba[:3] for rgba in source.getdata() if rgba[3] == 255}
    best = None
    for phase_x in [-0.5, 0, 0.5]:
        for phase_y in [-0.5, 0, 0.5]:
            matched = total = transparent = unchanged = changed = 0
            matched_colors = set()
            sampled_colors = set()
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
                        sampled_colors.add(rgba[:3])
                        ok = max(abs(known[ch] - expected[ch]) for ch in range(3)) <= 5
                        matched += ok
                        if ok:
                            matched_colors.add(rgba[:3])
            result = {"matched": matched, "total": total, "ratio": matched / max(total, 1),
                      "transparent": transparent, "transparentUnchanged": unchanged,
                      "changedPixels": changed, "rasterPhaseLogicalPixels": [phase_x, phase_y],
                      "sampledSourceColors": [list(c) for c in sorted(sampled_colors)],
                      "matchedSourceColors": [list(c) for c in sorted(matched_colors)]}
            if best is None or result["ratio"] > best["ratio"]:
                best = result
    roi = (math.floor((cx - 20) * scale), math.floor((cy - 20) * scale),
           math.ceil((cx + 20) * scale), math.ceil((cy + 20) * scale))
    restoration = {phase: list(shots[phase].crop(roi).getdata()) == list(initial.crop(roi).getdata())
                   for phase in ["restored", "recreated"]}
    deletion_changed = sum(a != b for a, b in zip(initial.crop(roi).getdata(), shots["deleted"].crop(roi).getdata()))
    passed = (deletion_changed > 100 and best["total"] >= 100 and best["ratio"] >= 0.75 and best["changedPixels"] > 100
              and best["transparent"] > 0 and best["transparentUnchanged"] / best["transparent"] >= 0.95
              and all(restoration.values()) and best["matchedSourceColors"] == [list(c) for c in sorted(expected_colors)])
    return {"passed": passed, "source": "ore.png", "alpha": alpha, "blendModel": "sRGB decoded linear alpha over same-color base negative control",
            "expectedSourceColors": [list(c) for c in sorted(expected_colors)],
            "sourcePixelMatch": best, "deletionChangedPixels": deletion_changed, "roi": roi, "exactRestoration": restoration}


def recovery(root, profile, metadata):
    row = json.loads(metadata)
    phases = row["phases"]
    shots = {p: Image.open(root / f"{profile}-recovery-{p}.png").convert("RGB") for p in phases}
    first = phases["initial"]
    c = first["canvas"]
    step = max(1, min(c["width"] // 640, c["height"] // 360))
    ux, uy = step*c["cssWidth"]/c["width"], step*c["cssHeight"]/c["height"]
    scale = shots["initial"].width / first["viewport"]["width"]
    results = {}
    for material, key, size in [("smelter", "module:material-smelter", 48), ("depot", "location:loc-recovery-depot", 32)]:
        source = Image.open(root / f"{material}-source.png").convert("RGBA")
        points = [(x,y,source.getpixel((x,y))) for y in range(size) for x in range(size)
                  if source.getpixel((x,y))[3] == 255 and (material == "smelter" or (y >= 24 and x < 24) or (6 <= x < 8 and 18 <= y < 22))]
        expected_colors = {v[:3] for _,_,v in points}
        rows = {}
        # Location is a native production projection of the depot anchor. Its
        # deterministic <=12000cm offset is bounded by 12CSS in this fixture.
        # The existing Location lower-right corner frame overlaps the source
        # right quarter; exclude that geometric region, retain all eight colors.
        for phase, state in phases.items():
            anchor = state["anchors"].get(key) or first["anchors"][key]
            shot = shots[phase]
            best = {"matched": -1}
            offsets = range(-12,13) if material == "depot" else [0]
            for dy in offsets:
                for dx in offsets:
                    colors = set(); matched = 0
                    for x,y,rgba in points:
                        px = math.floor((anchor["x"]+dx+(x-size/2+.5)*ux)*scale)
                        py = math.floor((anchor["y"]+dy+(y-size/2+.5)*uy)*scale)
                        if 0 <= px < shot.width and 0 <= py < shot.height and all(abs(shot.getpixel((px,py))[ch]-rgba[ch]) <= 3 for ch in range(3)):
                            matched += 1; colors.add(rgba[:3])
                    if matched > best["matched"]:
                        best = {"matched":matched,"total":len(points),"ratio":matched/len(points),"offsetCSS":[dx,dy],"matchedColors":[list(v) for v in sorted(colors)]}
            best["passed"] = best["ratio"] <= .05 if phase == "removed" else best["ratio"] >= (.9 if material == "smelter" else .95) and len(best["matchedColors"]) == len(expected_colors)
            rows[phase] = best
        results[material] = {"sourceRegions": [[0,0,48,48]] if material == "smelter" else [[0,24,24,32],[6,18,8,22]], "presentMinimum": .9 if material == "smelter" else .95, "absentMaximum": .05, "anchorSearchCSS": 12 if material == "depot" else 0, "expectedColors":[list(v) for v in sorted(expected_colors)],"phases":rows,"passed":all(v["passed"] for v in rows.values())}
    # Fixed sidebands avoid the one-character location label/central marker.
    # This checks composition restoration, not independent ore source consumption
    # (that remains the separately recorded same-base runtime PNG oracle).
    anchor = first["anchors"]["location:loc-recovery-ore"]
    pixels = [(math.floor((anchor["x"]+x+.5)*scale),math.floor((anchor["y"]+y+.5)*scale))
              for y in range(-18,18) for x in list(range(-18,-12))+list(range(12,18))]
    baseline = [shots["initial"].getpixel(p) for p in pixels]
    ore = {}
    for phase, shot in shots.items():
        equal = sum(all(abs(shot.getpixel(p)[ch]-v[ch]) <= 3 for ch in range(3)) for p,v in zip(pixels,baseline))
        ratio = equal/len(pixels)
        ore[phase] = {"equal":equal,"total":len(pixels),"ratio":ratio,"passed":ratio <= .25 if phase == "removed" else ratio >= .95}
    results["oreComposition"] = {"scope":"sideband composition restoration; independent source PNG proof stays in runtime oracle", "baselineColors":len(set(baseline)),"phases":ore,"passed":len(set(baseline)) >= 5 and all(v["passed"] for v in ore.values())}
    return {"passed":all(v["passed"] for v in results.values()),"unitCSS":[ux,uy],"materials":results}


if __name__ == "__main__":
    mode, directory, profile, *extra = sys.argv[1:]
    root = Path(directory)
    if mode == "recovery":
        result = recovery(root, profile, *extra)
    elif mode.startswith("runtime-"):
        result = runtime(root, profile, *extra, mode.removeprefix("runtime-"))
    else:
        result = factory(root, profile, *extra) if mode == "factory" else terrain(root, profile)
    print(json.dumps(result))
