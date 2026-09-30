# Bakes the apartment's lighting into textures and exports the game room.
#   blender -b --factory-startup --python tools/room/bake_room.py -- --out assets/room.glb [--samples 384] [--density 150]
#
# Builds the scene (build_room.py), then per group of meshes (each breakable
# room object, plus a few groups of fixed furniture and architecture):
#   1. joins the group into one mesh and unwraps a second UV set for baking,
#   2. bakes Cycles global illumination (direct + bounced light, AO, colour)
#      into one texture, saved through the scene's AgX view transform,
#   3. swaps the materials for that texture and splits the group back into
#      its pieces (bricks, boards, legs, cushions) so breaks can scatter them.
# Glass panes are not baked; they keep a flat tinted colour.
# Node names in the GLB: "o{owner:02}_{piece:03}" for breakable pieces,
# "glass{owner:02}_{piece:03}" for panes, "s_{group}" for fixed geometry.
import math
import os
import sys

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import build_room as room  # noqa: E402

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


def arg(name, default=None):
    return ARGS[ARGS.index(name) + 1] if name in ARGS else default


OUT = os.path.abspath(arg("--out", "assets/room.glb"))
SAMPLES = int(arg("--samples", "384"))
DENSITY = float(arg("--density", "150"))  # texels per metre
WORK = os.path.join(os.path.dirname(OUT), "room_bake")


def static_group(o):
    n = o.name.lower()
    if n.startswith(("facade", "street")):
        return "outside"
    if n.startswith(("hall", "annex")):
        return "annex"
    if n.startswith(("backwall", "door", "baseboard", "wainscot", "chair_rail")):
        return "shell"
    if n.startswith(("floor", "rug", "kitchen_floor")):
        return "floor"
    if o.get("asset"):
        x = (o.matrix_world @ Vector((0, 0, 0))).x
        return "furniture_west" if x < -4.5 else "furniture_east" if x > 4.5 else "furniture_center"
    if n.startswith(("ceiling", "beam", "crown")):
        return "ceiling"
    return "fixtures"


def is_glass(o):
    return any(s.material and s.material.get("glass") for s in o.material_slots)


def world_mesh(o):
    """Detaches a mesh from its parents with every transform applied."""
    bpy.ops.object.select_all(action="DESELECT")
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.parent_clear(type="CLEAR_KEEP_TRANSFORM")
    if o.data.users > 1:
        o.data = o.data.copy()
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


# Breakable models split into their loose parts (doors, shelves, books,
# legs), grouped by position into cells of this size, so a cabinet comes
# apart instead of flying off whole.
PIECE_CELL = 0.35


def loose_pieces(o):
    """Piece id per face: connected parts clustered by grid cell."""
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bm.faces.ensure_lookup_table()
    comp = [-1] * len(bm.faces)
    centers = []
    for f in bm.faces:
        if comp[f.index] >= 0:
            continue
        label = len(centers)
        comp[f.index] = label
        stack, total, count = [f], Vector(), 0
        while stack:
            g = stack.pop()
            total += g.calc_center_median()
            count += 1
            for e in g.edges:
                for h in e.link_faces:
                    if comp[h.index] < 0:
                        comp[h.index] = label
                        stack.append(h)
        centers.append(total / count)
    bm.free()
    cells = {}
    ids = [cells.setdefault(tuple(int(math.floor(c[k] / PIECE_CELL)) for k in range(3)), len(cells)) for c in centers]
    return [ids[c] for c in comp], len(cells)


def join(objects, name, split_models=False):
    """Joins objects into one, remembering which face came from which piece."""
    base = 0
    for o in objects:
        world_mesh(o)
        attr = o.data.attributes.new("piece", "INT", "FACE")
        if split_models and o.get("asset"):
            ids, count = loose_pieces(o)
        else:
            ids, count = [0] * len(o.data.polygons), 1
        for f, piece in enumerate(ids):
            attr.data[f].value = base + piece
        base += count
        # Models and boxes use different UV names; unify before joining.
        if not o.data.uv_layers:
            o.data.uv_layers.new(name="UVMap")
        o.data.uv_layers[0].name = "UVMap"
        while len(o.data.uv_layers) > 1:
            o.data.uv_layers.remove(o.data.uv_layers[1])
    bpy.ops.object.select_all(action="DESELECT")
    for o in objects:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    if len(objects) > 1:
        bpy.ops.object.join()
    o = bpy.context.active_object
    o.name = name
    return o


def unwrap(o):
    uv = o.data.uv_layers.new(name="bake")
    o.data.uv_layers.active = uv
    for layer in o.data.uv_layers:
        layer.active_render = layer.name == "UVMap"
    bpy.ops.object.select_all(action="DESELECT")
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=0.01, area_weight=0.0)
    bpy.ops.object.mode_set(mode="OBJECT")


# Texel budget per group: what the camera sees up close gets more.
DETAIL = {"s_outside": 0.35, "s_ceiling": 0.4, "s_annex": 0.8,
          "s_shell": 0.9, "o21": 0.6, "o12": 0.7}
LIMIT = {"s_floor": 2048, "s_annex": 2048, "s_shell": 2048,
         "o16": 2048, "o17": 2048}


def texture_size(o):
    area = sum(p.area for p in o.data.polygons)
    side = math.sqrt(area) * DENSITY * DETAIL.get(o.name, 1.0)
    size = 128
    while size < side and size < LIMIT.get(o.name, 1024):
        size *= 2
    if o.name.startswith("s_furniture"):
        size = max(size, 2048)
    return size


def bake(o, name):
    size = texture_size(o)
    image = bpy.data.images.new(name, size, size, float_buffer=True)
    for slot in o.material_slots:
        m = slot.material
        nt = m.node_tree
        node = nt.nodes.get("bake_target") or nt.nodes.new("ShaderNodeTexImage")
        node.name = "bake_target"
        node.image = image
        nt.nodes.active = node
    bpy.ops.object.select_all(action="DESELECT")
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.bake(
        type="COMBINED",
        pass_filter={"DIRECT", "INDIRECT", "DIFFUSE", "GLOSSY", "TRANSMISSION", "EMIT"},
        margin=16,
        margin_type="EXTEND",
        use_clear=True,
        target="IMAGE_TEXTURES",
    )
    path = os.path.join(WORK, name + ".jpg")
    scene = bpy.context.scene
    scene.render.image_settings.file_format = "JPEG"
    scene.render.image_settings.quality = 88
    image.save_render(path, scene=scene)
    print(f"baked {name}: {size}px", flush=True)
    return path


def baked_material(name, path):
    m = bpy.data.materials.new("baked_" + name)
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    img = nt.nodes.new("ShaderNodeTexImage")
    img.image = bpy.data.images.load(path)
    uvmap = nt.nodes.new("ShaderNodeUVMap")
    uvmap.uv_map = "bake"
    nt.links.new(uvmap.outputs["UV"], img.inputs["Vector"])
    nt.links.new(img.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 1.0
    return m


def finish(o, material):
    """Keeps only the baked UVs and material."""
    o.data.materials.clear()
    o.data.materials.append(material)
    for p in o.data.polygons:
        p.material_index = 0
    for layer in [l for l in o.data.uv_layers if l.name != "bake"]:
        o.data.uv_layers.remove(layer)


def split_pieces(o, prefix):
    """Separates a baked group back into its pieces; origins at each piece's
    centre so it can tumble about it."""
    attr = o.data.attributes["piece"]
    ids = sorted({attr.data[i].value for i in range(len(o.data.polygons))})
    pieces = []
    for k, piece in enumerate(ids):
        bm = bmesh.new()
        bm.from_mesh(o.data)
        layer = bm.faces.layers.int.get("piece")
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if f[layer] != piece], context="FACES")
        if not bm.faces:
            bm.free()
            continue
        mesh = bpy.data.meshes.new(f"{prefix}_{k:03d}")
        bm.to_mesh(mesh)
        bm.free()
        for m in o.data.materials:
            mesh.materials.append(m)
        part = bpy.data.objects.new(f"{prefix}_{k:03d}", mesh)
        bpy.context.collection.objects.link(part)
        pieces.append(part)
    bpy.data.objects.remove(o)
    for part in pieces:
        bpy.ops.object.select_all(action="DESELECT")
        part.select_set(True)
        bpy.context.view_layer.objects.active = part
        bpy.ops.object.origin_set(type="ORIGIN_GEOMETRY", center="BOUNDS")
    return pieces


def glass_piece(o, owner, k, material):
    world_mesh(o)
    o.data.materials.clear()
    o.data.materials.append(material)
    o.name = f"glass{owner:02d}_{k:03d}"
    bpy.ops.object.select_all(action="DESELECT")
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.origin_set(type="ORIGIN_GEOMETRY", center="BOUNDS")


def main():
    os.makedirs(WORK, exist_ok=True)
    room.build()
    room.setup_cycles(SAMPLES)
    scene = bpy.context.scene
    scene.render.bake.use_pass_direct = True
    groups = {}
    panes = {}
    for o in list(bpy.data.objects):
        if o.type != "MESH":
            continue
        owner = int(o.get("owner", -1))
        if owner >= 0 and is_glass(o):
            panes.setdefault(owner, []).append(o)
            continue
        key = f"o{owner:02d}" if owner >= 0 else f"s_{static_group(o)}"
        groups.setdefault(key, []).append(o)
    # Panes stay in the scene during the bake (their light passes through).
    joined = {key: join(objs, key, key.startswith("o")) for key, objs in sorted(groups.items())}
    for key, o in joined.items():
        unwrap(o)
    exported = []
    for key, o in joined.items():
        path = bake(o, key)
        finish(o, baked_material(key, path))
        if key.startswith("o"):
            exported += split_pieces(o, key)
        else:
            exported.append(o)
    glass_m = bpy.data.materials.new("glass_pane")
    glass_m.use_nodes = True
    bsdf = glass_m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (0.78, 0.88, 0.93, 1)
    bsdf.inputs["Alpha"].default_value = 0.22
    for owner, objs in panes.items():
        for k, o in enumerate(objs):
            glass_piece(o, owner, k, glass_m)
            exported.append(o)
    # Export only the baked room.
    for o in list(bpy.data.objects):
        if o not in exported:
            bpy.data.objects.remove(o)
    bpy.ops.export_scene.gltf(
        filepath=OUT,
        export_format="GLB",
        export_image_format="JPEG",
        export_jpeg_quality=85,
        export_normals=False,
        export_texcoords=True,
        export_materials="EXPORT",
        export_yup=True,
        export_apply=True,
        export_animations=False,
        export_cameras=False,
        export_lights=False,
    )
    print(f"exported {len(exported)} nodes, {os.path.getsize(OUT) // 1024} KB -> {OUT}", flush=True)


main()
