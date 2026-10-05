"""Shrinks the near city layer to a 256-colour PNG with alpha (~10x smaller;
flat roofs and lit windows survive the palette). Run after city.py:
    python tools/backdrop/shrink.py [assets/backdrop/near.png]
Needs Pillow."""
import os
import sys

from PIL import Image

path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "..", "assets", "backdrop", "near.png")
before = os.path.getsize(path)
image = Image.open(path).convert("RGBA")
image.quantize(256, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE).save(path, optimize=True)
print(f"{path}: {before // 1024} KB -> {os.path.getsize(path) // 1024} KB")
