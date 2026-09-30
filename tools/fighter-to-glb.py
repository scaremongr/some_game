# Mixamo character (FBX, T-pose with skin; or a GLB) -> game-ready GLB, in Blender.
#   blender -b --factory-startup --python tools/fighter-to-glb.py -- <in.fbx> <out.glb> [max texture px]
# Keeps the skinned mesh and the armature, drops animation, shrinks textures
# (1024 px by default) and stores them as JPEG so a fighter weighs a few MB.
import os
import sys

import bpy

args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
if len(args) < 2:
    print("usage: blender -b --python fighter-to-glb.py -- <in.fbx> <out.glb> [max px]")
    sys.exit(1)
source, target = args[0], args[1]
limit = int(args[2]) if len(args) > 2 else 1024
os.makedirs(os.path.dirname(os.path.abspath(target)), exist_ok=True)

bpy.ops.wm.read_factory_settings(use_empty=True)
if source.lower().endswith((".glb", ".gltf")):
    bpy.ops.import_scene.gltf(filepath=source)
else:
    bpy.ops.import_scene.fbx(filepath=source, automatic_bone_orientation=False)
for action in list(bpy.data.actions):
    bpy.data.actions.remove(action)
for image in bpy.data.images:
    if image.size[0] == 0:
        continue
    w, h = image.size
    if max(w, h) > limit:
        k = limit / max(w, h)
        image.scale(max(1, round(w * k)), max(1, round(h * k)))
        # Scaled pixels live in memory only; pack them so the export sees them.
        image.pack()
bpy.ops.export_scene.gltf(
    filepath=target,
    export_format="GLB",
    export_animations=False,
    export_apply=False,
    export_yup=True,
    export_image_format="JPEG",
    export_jpeg_quality=82,
)
meshes = sum(1 for o in bpy.data.objects if o.type == "MESH")
bones = sum(len(o.data.bones) for o in bpy.data.objects if o.type == "ARMATURE")
print(f"OK {os.path.basename(target)}: {meshes} meshes, {bones} bones, {os.path.getsize(target) // 1024} KB")
