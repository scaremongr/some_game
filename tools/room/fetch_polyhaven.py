"""Downloads the CC0 models and textures used by the apartment arena.

    python tools/room/fetch_polyhaven.py

Everything comes from Poly Haven (https://polyhaven.com, CC0: free for any
use). Models land in assets-src/room/models/<id>/ as glTF with 1k textures,
textures in assets-src/room/textures/<id>/ (colour, OpenGL normal, roughness).
Files already present are skipped.
"""
import json
import pathlib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2] / "assets-src" / "room"
MODELS = [
    "Sofa_01", "ArmChair_01", "Ottoman_01", "CoffeeTable_01", "round_wooden_table_01",
    "painted_wooden_chair_01", "painted_wooden_chair_02", "painted_wooden_cabinet_02",
    "vintage_cabinet_01", "wooden_bookshelf_worn", "potted_plant_02", "potted_plant_04",
    "modern_ceiling_lamp_01", "hanging_industrial_lamp", "fancy_picture_frame_01",
    "fancy_picture_frame_02", "hanging_picture_frame_03", "hanging_picture_frame_02",
    "wall_clock", "throw_pillows_01", "wicker_basket_01", "ceramic_vase_01", "brass_vase_01",
    "brass_vase_02", "mantel_clock_01", "decorative_book_set_01", "book_encyclopedia_set_01",
    "side_table_tall_01", "side_table_01", "WoodenTable_02",
]
TEXTURES = [
    "painted_plaster_wall", "plastered_wall_02", "herringbone_parquet", "wood_floor",
    "brick_wall_02", "blue_painted_planks", "kitchen_wood", "floor_tiles_06", "dark_wood",
    "rough_linen", "fabric_pattern_05", "white_planks_clean", "marble_01",
]


HEADERS = {"User-Agent": "pulse-arena-asset-fetch/1.0"}


def fetch(url, timeout=120):
    return urllib.request.urlopen(urllib.request.Request(url, headers=HEADERS), timeout=timeout)


def get(url, path):
    if path.exists() and path.stat().st_size > 0:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    with fetch(url) as r, open(path, "wb") as f:
        f.write(r.read())


def files(asset):
    with fetch(f"https://api.polyhaven.com/files/{asset}", 60) as r:
        return json.load(r)


for asset in MODELS:
    formats = files(asset)
    if "gltf" not in formats:
        print("skip (no glTF)", asset)
        continue
    gltf = formats["gltf"]["1k"]["gltf"]
    folder = ROOT / "models" / asset
    get(gltf["url"], folder / pathlib.Path(gltf["url"]).name)
    for rel, item in gltf.get("include", {}).items():
        get(item["url"], folder / rel)
    print("model", asset)

for asset in TEXTURES:
    data = files(asset)
    folder = ROOT / "textures" / asset
    for kind in ("Diffuse", "nor_gl", "Rough"):
        maps = data.get(kind, {}).get("1k", {})
        entry = maps.get("jpg") or maps.get("png")
        if entry:
            get(entry["url"], folder / pathlib.Path(entry["url"]).name)
    print("texture", asset)
