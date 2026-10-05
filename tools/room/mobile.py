"""Phone version of the baked room: the same GLB with every light map larger
than 1024 px scaled down (phones have little GPU memory and slower networks;
their screens do not need more). Desktops load room-hd.glb, phones room.glb.
    python tools/room/mobile.py [assets/room-hd.glb] [assets/room.glb] [--max 1024]
Needs Pillow."""
import io
import json
import struct
import sys

from PIL import Image

args = [a for a in sys.argv[1:] if not a.startswith("--")]
src = args[0] if args else "assets/room-hd.glb"
dst = args[1] if len(args) > 1 else "assets/room.glb"
limit = int(sys.argv[sys.argv.index("--max") + 1]) if "--max" in sys.argv else 1024

data = open(src, "rb").read()
json_len = struct.unpack("<I", data[12:16])[0]
doc = json.loads(data[20:20 + json_len])
bin_start = 20 + json_len + 8
binary = data[bin_start:bin_start + struct.unpack("<I", data[20 + json_len:24 + json_len])[0]]

# Rebuild the binary chunk view by view, shrinking the images.
images = {im["bufferView"] for im in doc.get("images", []) if "bufferView" in im}
out = bytearray()
for i, view in enumerate(doc["bufferViews"]):
    chunk = binary[view.get("byteOffset", 0):view.get("byteOffset", 0) + view["byteLength"]]
    if i in images:
        img = Image.open(io.BytesIO(chunk))
        if max(img.size) > limit:
            k = limit / max(img.size)
            img = img.convert("RGB").resize((round(img.width * k), round(img.height * k)), Image.LANCZOS)
            buf = io.BytesIO()
            img.save(buf, "JPEG", quality=85)
            chunk = buf.getvalue()
    while len(out) % 4:
        out.append(0)
    view["byteOffset"] = len(out)
    view["byteLength"] = len(chunk)
    out += chunk
while len(out) % 4:
    out.append(0)
doc["buffers"][0]["byteLength"] = len(out)
text = json.dumps(doc, separators=(",", ":")).encode()
text += b" " * (-len(text) % 4)
glb = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(text) + 8 + len(out))
glb += struct.pack("<II", len(text), 0x4E4F534A) + text + struct.pack("<II", len(out), 0x004E4942) + bytes(out)
open(dst, "wb").write(glb)
print(f"{src} {len(data) // 1024} KB -> {dst} {len(glb) // 1024} KB (light maps <= {limit} px)")
