import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from PIL import Image


class AtlasEvidence(unittest.TestCase):
    def test_real_sprite_and_blank_negative_share_geometry(self):
        repo = Path(__file__).resolve().parents[3]
        script = Path(__file__).with_name('viewer-startup-atlas-oracle.py')
        atlas = repo / 'doc/world-simulator/viewer/assets/ui-ux/pixel-world/agent.png'
        sprite = Image.open(atlas).convert('RGBA').resize((32, 32), Image.Resampling.NEAREST)
        row = {'canvas': {'width': 1280, 'height': 720, 'cssWidth': 1280, 'cssHeight': 720},
               'viewport': {'width': 1280, 'height': 720}, 'target': {'x': 640, 'y': 360}}
        with tempfile.TemporaryDirectory(prefix='startup-atlas-') as directory:
            blank = Image.new('RGB', (1280, 720), (0, 0, 0))
            positive = blank.copy()
            positive.paste(sprite, (624, 330), sprite)
            ratios = []
            for name, image in [('blank', blank), ('positive', positive)]:
                path = Path(directory) / (name + '.png')
                image.save(path)
                ratios.append(json.loads(subprocess.check_output(['python3', str(script), str(path), str(atlas), json.dumps(row)]))['ratio'])
            self.assertLess(ratios[0], .1)
            self.assertEqual(ratios[1], 1)


if __name__ == '__main__':
    unittest.main()
