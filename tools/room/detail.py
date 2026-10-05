"""Detail atlas for the baked room: the fine grain the light maps are too
coarse to hold (parquet grain, tile edges, rug pile, plaster), as the ratio
of each texture to its own blur. 0.5 is neutral; the renderer multiplies the
baked colour by 2x the sample (see BAKED_FRAGMENT_SHADER in render3d.rs).
    python tools/room/detail.py   ->  assets/room_detail.jpg
Quadrants (2048 px): parquet | stone tiles  /  rug pile | plaster.
Sources are the same CC0 Poly Haven textures the bake used (assets-src/room/textures),
so the detail lines up with the pattern already in the light maps. Needs Pillow."""
import os

from PIL import Image, ImageChops, ImageFilter, ImageOps

ROOT = os.path.join(os.path.dirname(__file__), "..", "..")
SRC = os.path.join(ROOT, "assets-src", "room", "textures")
TILE = 1024
# Texture, blur radius in texels (about one light-map texel), contrast.
LAYERS = [
    ("herringbone_parquet", 3.0, 1.3),
    ("floor_tiles_06", 3.0, 1.2),
    ("dirty_carpet", 8.0, 1.1),
    ("painted_plaster_wall", 6.0, 2.2),
]


def detail(name, radius, contrast):
    folder = os.path.join(SRC, name)
    path = os.path.join(folder, next(f for f in sorted(os.listdir(folder)) if "_diff_" in f))
    lum = ImageOps.grayscale(Image.open(path)).resize((TILE, TILE), Image.LANCZOS)
    blur = lum.filter(ImageFilter.GaussianBlur(radius))
    # ratio = lum / blur, stored as 0.5 * ratio, contrast around neutral.
    px, pb = lum.load(), blur.load()
    out = Image.new("L", (TILE, TILE))
    po = out.load()
    for y in range(TILE):
        for x in range(TILE):
            r = (px[x, y] + 4) / (pb[x, y] + 4)
            po[x, y] = max(0, min(255, round(128 * (1 + (r - 1) * contrast))))
    return out


atlas = Image.new("L", (TILE * 2, TILE * 2), 128)
for i, (name, radius, contrast) in enumerate(LAYERS):
    atlas.paste(detail(name, radius, contrast), ((i % 2) * TILE, (i // 2) * TILE))
out = os.path.join(ROOT, "assets", "room_detail.jpg")
atlas.convert("RGB").save(out, quality=90)
print(out, os.path.getsize(out) // 1024, "KB")
