# Batch FBX -> GLB conversion in Blender, for Mixamo animation exports.
#   blender -b --factory-startup --python tools/fbx-to-glb.py -- <input dir or .fbx> <output dir>
# Every FBX becomes one GLB holding the armature and its animation (no mesh
# needed: clips are matched to the fighter's rig by bone name).
import os
import sys

import bpy

args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
if len(args) < 2:
    print("usage: blender -b --python fbx-to-glb.py -- <in> <out-dir>")
    sys.exit(1)
source, target = args[0], args[1]
os.makedirs(target, exist_ok=True)
files = [source] if source.lower().endswith(".fbx") else sorted(
    os.path.join(source, f) for f in os.listdir(source) if f.lower().endswith(".fbx")
)
done = failed = 0
for path in files:
    name = os.path.splitext(os.path.basename(path))[0]
    out = os.path.join(target, name + ".glb")
    try:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.fbx(filepath=path, automatic_bone_orientation=False)
        bpy.ops.export_scene.gltf(
            filepath=out,
            export_format="GLB",
            export_animations=True,
            export_apply=False,
            export_yup=True,
        )
        done += 1
        print(f"OK {name}")
    except Exception as error:  # keep going: one broken file must not stop the batch
        failed += 1
        print(f"FAIL {name}: {error}")
print(f"converted {done}, failed {failed}")
