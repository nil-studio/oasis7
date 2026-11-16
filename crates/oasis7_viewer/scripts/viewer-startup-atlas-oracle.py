"""Actual shared opaque atlas pixels at a DOM target's final projected foot."""
import json
import math
import sys
from PIL import Image

image = Image.open(sys.argv[1]).convert('RGB')
sprite = Image.open(sys.argv[2]).convert('RGBA')
row = json.loads(sys.argv[3])
assert sprite.size == (16, 16)
canvas, target = row['canvas'], row['target']
step = max(1, min(canvas['width'] // 640, canvas['height'] // 360))
ux = step * canvas['cssWidth'] / canvas['width']
uy = step * canvas['cssHeight'] / canvas['height']
scale = image.width / row['viewport']['width']
assert image.height / row['viewport']['height'] == scale
matched = total = 0
for y in range(16):
    for x in range(16):
        rgba = sprite.getpixel((x, y))
        if rgba[3] != 255:
            continue
        total += 1
        px = math.floor((target['x'] + (-8 + x + .5) * ux) * scale)
        py = math.floor((target['y'] + (-15 + y + .5) * uy) * scale)
        if 0 <= px < image.width and 0 <= py < image.height:
            matched += all(abs(image.getpixel((px, py))[c] - rgba[c]) <= 3 for c in range(3))
print(json.dumps({'matched': matched, 'total': total, 'ratio': matched / total,
                  'semantics': 'opaque_atlas_pixels_not_first_physical_frame'}))
