# Builds the apartment arena in Blender from CC0 assets (see fetch_polyhaven.py).
#   blender -b --factory-startup --python tools/room/build_room.py -- [--preview out.png] [--save room.blend]
#
# Coordinates follow the game: x to the right, y up, z towards the camera;
# fighters walk the continuous lane z = 0 through five open rooms. Exterior
# bounds are x = +-11.5. Blender is z-up: game (x, y, z) = Blender (x, -z, y).
#
# Every mesh carries an "owner" property: the combat room object it belongs to
# (combat/src/room.rs LAYOUT index 5..19 for furnishings) or -1 for the
# fixed apartment. Owned meshes are built from separate pieces (boards, bricks,
# legs, cushions) so they can later break apart along those seams.
import math
import os
import sys

import bpy
from mathutils import Vector

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "assets-src", "room"))


def arg(name, default=None):
    return ARGS[ARGS.index(name) + 1] if name in ARGS else default


def game(x, y, z):
    return Vector((x, -z, y))


# ---------------------------------------------------------------- materials
_materials = {}


# File name markers per map: Poly Haven (`_diff_1k.jpg`) and ambientCG
# (`_Color.jpg`) naming both work, so either library can be dropped in.
TEXTURE_MARKERS = {
    "diff": ("_diff_", "_color."),
    "rough": ("_rough_", "_roughness."),
    "nor_gl": ("_nor_gl_", "_normalgl."),
}


def texture_path(tex, kind):
    folder = os.path.join(ROOT, "textures", tex)
    for f in sorted(os.listdir(folder)):
        if any(m in f.lower() for m in TEXTURE_MARKERS.get(kind, (f"_{kind}_",))):
            return os.path.join(folder, f)
    return None


def surface(name, tex=None, tint=(1, 1, 1), scale=1.0, rough=0.6, bump=0.25, color=None):
    """Principled material; image textures are projected in world space so
    neighbouring pieces of a wall continue the same pattern."""
    if name in _materials:
        return _materials[name]
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = rough
    if tex:
        geo = nt.nodes.new("ShaderNodeNewGeometry")
        mapping = nt.nodes.new("ShaderNodeMapping")
        mapping.inputs["Scale"].default_value = (1 / scale, 1 / scale, 1 / scale)
        nt.links.new(geo.outputs["Position"], mapping.inputs["Vector"])
        img = nt.nodes.new("ShaderNodeTexImage")
        img.image = bpy.data.images.load(texture_path(tex, "diff"), check_existing=True)
        img.projection = "BOX"
        img.projection_blend = 0.25
        nt.links.new(mapping.outputs["Vector"], img.inputs["Vector"])
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.blend_type = "MULTIPLY"
        mix.inputs["Factor"].default_value = 1.0
        nt.links.new(img.outputs["Color"], mix.inputs["A"])
        mix.inputs["B"].default_value = (*tint, 1)
        nt.links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
        rpath = texture_path(tex, "rough")
        if rpath:
            r = nt.nodes.new("ShaderNodeTexImage")
            r.image = bpy.data.images.load(rpath, check_existing=True)
            r.image.colorspace_settings.name = "Non-Color"
            r.projection = "BOX"
            r.projection_blend = 0.25
            nt.links.new(mapping.outputs["Vector"], r.inputs["Vector"])
            nt.links.new(r.outputs["Color"], bsdf.inputs["Roughness"])
        if bump > 0:
            b = nt.nodes.new("ShaderNodeBump")
            b.inputs["Strength"].default_value = bump
            b.inputs["Distance"].default_value = 0.004
            nt.links.new(img.outputs["Color"], b.inputs["Height"])
            nt.links.new(b.outputs["Normal"], bsdf.inputs["Normal"])
    else:
        bsdf.inputs["Base Color"].default_value = (*(color or tint), 1)
    _materials[name] = m
    return m


def emissive(name, color, strength):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1)
    bsdf.inputs["Emission Color"].default_value = (*color, 1)
    bsdf.inputs["Emission Strength"].default_value = strength
    return m


def glass():
    m = bpy.data.materials.new("glass")
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (0.8, 0.9, 0.95, 1)
    bsdf.inputs["Roughness"].default_value = 0.05
    # Thin window glass: mostly see-through, so sunlight reaches the floor
    # (Cycles blocks direct light behind refractive glass).
    bsdf.inputs["Alpha"].default_value = 0.12
    m["glass"] = True
    return m


def rug_image(width_m=4.2, depth_m=2.6, px=1024):
    """Paints a floral rug (navy border, blossom band, cream field and a
    central wreath) into an sRGB image."""
    import numpy as np
    w, h = px, int(px * depth_m / width_m)
    X, Y = np.meshgrid(np.linspace(0, width_m, w), np.linspace(0, depth_m, h))
    d = np.minimum(np.minimum(X, width_m - X), np.minimum(Y, depth_m - Y))
    rng = np.random.default_rng(7)
    noise = rng.normal(0, 1, (h // 8 + 1, w // 8 + 1))
    noise = np.kron(noise, np.ones((8, 8)))[:h, :w]
    img = np.zeros((h, w, 3))
    img[:] = (0.86, 0.81, 0.68)
    img += noise[..., None] * 0.012

    def flowers(mask, cx, cy, size, palette):
        dx, dy = X - cx, Y - cy
        r = np.hypot(dx, dy)
        a = np.arctan2(dy, dx)
        petals = r < size * (0.55 + 0.45 * np.abs(np.cos(2.5 * a)))
        heart = r < size * 0.28
        leaf_a = np.abs(np.sin(a * 2 + 0.6)) > 0.93
        leaves = (r > size * 0.9) & (r < size * 1.6) & leaf_a
        img[mask & leaves] = (0.40, 0.50, 0.36)
        img[mask & petals] = palette
        img[mask & heart] = (0.88, 0.68, 0.36)

    band = (d > 0.14) & (d < 0.46)
    step = 0.32
    for i, palette in enumerate([(0.72, 0.36, 0.40), (0.52, 0.60, 0.76), (0.80, 0.55, 0.50)]):
        ox = (i * step / 3)
        cx = np.round((X - ox) / step) * step + ox
        cy = np.round((Y - ox) / step) * step + ox
        flowers(band, cx, cy, 0.075, palette)
    img[(d > 0.46) & (d < 0.475)] = (0.55, 0.45, 0.30)
    # Central wreath.
    ex = (X - width_m / 2) / 0.9
    ey = (Y - depth_m / 2) / 0.55
    er = np.hypot(ex, ey)
    ring = (er > 0.8) & (er < 1.15)
    ang = np.arctan2(ey, ex)
    k = np.round(ang / (2 * np.pi / 14)) * (2 * np.pi / 14)
    cx = width_m / 2 + np.cos(k) * 0.9 * 0.97
    cy = depth_m / 2 + np.sin(k) * 0.55 * 0.97
    flowers(ring, cx, cy, 0.085, (0.74, 0.38, 0.42))
    centre = er < 0.35
    flowers(centre, width_m / 2, depth_m / 2, 0.16, (0.56, 0.64, 0.80))
    # Border: navy with a gold line.
    img[d < 0.14] = (0.08, 0.10, 0.17)
    img[(d > 0.035) & (d < 0.05)] = (0.72, 0.58, 0.30)
    img = np.clip(img, 0, 1)
    rgba = np.concatenate([img, np.ones((h, w, 1))], axis=2)
    image = bpy.data.images.new("rug", w, h)
    image.pixels = rgba[::-1].ravel().tolist() if False else rgba.ravel().tolist()
    image.pack()
    return image


def rug_material():
    """The rug: painted image, matte fibre with a little bump."""
    m = bpy.data.materials.new("rug")
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.95
    img = nt.nodes.new("ShaderNodeTexImage")
    img.image = rug_image()
    nt.links.new(img.outputs["Color"], bsdf.inputs["Base Color"])
    bump = nt.nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.3
    nt.links.new(img.outputs["Color"], bump.inputs["Height"])
    nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return m


def rug_material_procedural():
    m = bpy.data.materials.new("rug_nodes")
    m.use_nodes = True
    nt = m.node_tree
    n, l = nt.nodes, nt.links
    bsdf = n["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.95
    uv = n.new("ShaderNodeTexCoord")
    sep = n.new("ShaderNodeSeparateXYZ")
    l.new(uv.outputs["UV"], sep.inputs["Vector"])
    # Distance to the rug edge in UV units (0 at the edge, 0.5 in the middle).
    def edge(axis):
        a = n.new("ShaderNodeMath"); a.operation = "PINGPONG"; a.inputs[1].default_value = 0.5
        l.new(sep.outputs[axis], a.inputs[0])
        return a
    ex, ey = edge("X"), edge("Y")
    mn = n.new("ShaderNodeMath"); mn.operation = "MINIMUM"
    l.new(ex.outputs[0], mn.inputs[0]); l.new(ey.outputs[0], mn.inputs[1])
    border = n.new("ShaderNodeMath"); border.operation = "LESS_THAN"; border.inputs[1].default_value = 0.06
    l.new(mn.outputs[0], border.inputs[0])
    band = n.new("ShaderNodeMath"); band.operation = "LESS_THAN"; band.inputs[1].default_value = 0.2
    l.new(mn.outputs[0], band.inputs[0])
    flowers = n.new("ShaderNodeTexVoronoi"); flowers.inputs["Scale"].default_value = 26
    l.new(uv.outputs["UV"], flowers.inputs["Vector"])
    petal = n.new("ShaderNodeValToRGB")
    petal.color_ramp.elements[0].position = 0.0
    petal.color_ramp.elements[0].color = (0.62, 0.25, 0.28, 1)
    petal.color_ramp.elements[1].position = 0.28
    petal.color_ramp.elements[1].color = (0.86, 0.80, 0.66, 1)
    el = petal.color_ramp.elements.new(0.16); el.color = (0.34, 0.47, 0.34, 1)
    l.new(flowers.outputs["Distance"], petal.inputs["Fac"])
    field = n.new("ShaderNodeTexNoise"); field.inputs["Scale"].default_value = 90
    l.new(uv.outputs["UV"], field.inputs["Vector"])
    cream = n.new("ShaderNodeMix"); cream.data_type = "RGBA"
    cream.inputs["A"].default_value = (0.86, 0.80, 0.66, 1)
    cream.inputs["B"].default_value = (0.80, 0.73, 0.58, 1)
    l.new(field.outputs["Fac"], cream.inputs["Factor"])
    ring = n.new("ShaderNodeMix"); ring.data_type = "RGBA"
    l.new(band.outputs[0], ring.inputs["Factor"])
    l.new(cream.outputs["Result"], ring.inputs["A"])
    l.new(petal.outputs["Color"], ring.inputs["B"])
    edge_col = n.new("ShaderNodeMix"); edge_col.data_type = "RGBA"
    l.new(border.outputs[0], edge_col.inputs["Factor"])
    l.new(ring.outputs["Result"], edge_col.inputs["A"])
    edge_col.inputs["B"].default_value = (0.08, 0.10, 0.16, 1)
    l.new(edge_col.outputs["Result"], bsdf.inputs["Base Color"])
    return m


# ---------------------------------------------------------------- geometry
def box(name, owner, center, size, material, bevel=0.004):
    """Axis-aligned box, game coordinates: centre (x, y, z), size (w, h, d)."""
    bpy.ops.mesh.primitive_cube_add(size=1)
    o = bpy.context.active_object
    o.name = name
    o.location = game(*center)
    o.scale = (size[0], size[2], size[1])
    bpy.ops.object.transform_apply(scale=True)
    if bevel > 0:
        mod = o.modifiers.new("bevel", "BEVEL")
        mod.width = bevel
        mod.segments = 2
        bpy.ops.object.modifier_apply(modifier=mod.name)
    o.data.materials.append(material)
    o["owner"] = owner
    return o


def grid_wall(name, owner, x0, x1, y0, y1, z, depth, material, cols, rows, holes=()):
    """A wall face split into bricks of plaster (its break pattern), with
    rectangular openings (x0, x1, y0, y1) left out."""
    w = (x1 - x0) / cols
    h = (y1 - y0) / rows
    for c in range(cols):
        for r in range(rows):
            cx0, cx1 = x0 + c * w, x0 + (c + 1) * w
            cy0, cy1 = y0 + r * h, y0 + (r + 1) * h
            for hx0, hx1, hy0, hy1 in holes:
                if cx0 < hx1 and cx1 > hx0 and cy0 < hy1 and cy1 > hy0:
                    # Split the brick around the opening.
                    parts = []
                    if cy0 < hy0:
                        parts.append((cx0, cx1, cy0, hy0))
                    if cy1 > hy1:
                        parts.append((cx0, cx1, hy1, cy1))
                    if cx0 < hx0:
                        parts.append((cx0, hx0, max(cy0, hy0), min(cy1, hy1)))
                    if cx1 > hx1:
                        parts.append((hx1, cx1, max(cy0, hy0), min(cy1, hy1)))
                    for (a, b, c0, c1) in parts:
                        if b - a > 0.02 and c1 - c0 > 0.02:
                            box(f"{name}", owner, ((a + b) / 2, (c0 + c1) / 2, z), (b - a, c1 - c0, depth), material, 0.0)
                    break
            else:
                box(f"{name}", owner, ((cx0 + cx1) / 2, (cy0 + cy1) / 2, z), (w, h, depth), material, 0.0)


FLOOR_X = [round(-4.2 + 0.7 * i, 3) for i in range(13)]
FLOOR_Z = [-1.98 + 0.66 * i for i in range(7)]


def rug_tiles(material, xr, zr):
    """Rug pieces on the floorboard grid; each shows its part of the image
    and belongs to the floor section under it (the middle strip is fixed)."""
    xs = sorted(set([xr[0], xr[1]] + [x for x in FLOOR_X if xr[0] < x < xr[1]]))
    zs = sorted(set([zr[0], zr[1]] + [z for z in FLOOR_Z if zr[0] < z < zr[1]]))
    w, d = xr[1] - xr[0], zr[1] - zr[0]
    for x0, x1 in zip(xs, xs[1:]):
        for z0, z1 in zip(zs, zs[1:]):
            cx = (x0 + x1) / 2
            owner = 16 if cx < 0 else 17
            o = box("rug", owner, (cx, 0.004, (z0 + z1) / 2), (x1 - x0, 0.008, z1 - z0), material, 0)
            uv = o.data.uv_layers.active
            for loop in o.data.loops:
                v = o.data.vertices[loop.vertex_index].co
                # Blender y is minus game z.
                uv.data[loop.index].uv = ((v.x - xr[0]) / w, (v.y + zr[1]) / d)


def side_wall(name, owner, x, z0, z1, y0, y1, depth, material, cols, rows):
    w = (z1 - z0) / cols
    h = (y1 - y0) / rows
    for c in range(cols):
        for r in range(rows):
            box(name, owner, (x, y0 + (r + 0.5) * h, z0 + (c + 0.5) * w), (depth, h, w), material, 0.0)


# ---------------------------------------------------------------- models
def decimate(o, budget):
    """Collapses a film-quality model to a phone-friendly triangle budget;
    the baked lighting carries the fine shading instead."""
    tris = sum(len(p.vertices) - 2 for p in o.data.polygons)
    if tris <= budget:
        return
    bpy.ops.object.select_all(action="DESELECT")
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    mod = o.modifiers.new("decimate", "DECIMATE")
    mod.ratio = budget / tris
    mod.use_collapse_triangulate = True
    bpy.ops.object.modifier_apply(modifier=mod.name)


def place(asset, owner, x, z, rot=0.0, fit=("h", 1.0), y=0.0, tint=None, stretch=None,
          recolor=None, nometal=False, glow=None, tris=5000):
    """Imports a model from assets-src/room/models/<asset>/ (a .gltf or .glb:
    Poly Haven, Sketchfab or any other source), scales it to `fit` (height
    'h', width 'w' or depth 'd' in metres), stands it on y and turns it
    `rot` degrees (0 = facing the camera)."""
    before = set(bpy.data.objects)
    folder = os.path.join(ROOT, "models", asset)
    gltf = next(f for f in sorted(os.listdir(folder)) if f.lower().endswith((".gltf", ".glb")))
    bpy.ops.import_scene.gltf(filepath=os.path.join(folder, gltf))
    new = [o for o in bpy.data.objects if o not in before]
    meshes = [o for o in new if o.type == "MESH"]
    roots = [o for o in new if o.parent is None]
    counts = [sum(len(p.vertices) - 2 for p in o.data.polygons) for o in meshes]
    total = max(1, sum(counts))
    for o, count in zip(meshes, counts):
        if o.data.users > 1:
            o.data = o.data.copy()
        decimate(o, max(12, round(tris * count / total)))
    # Measure in world space.
    bpy.context.view_layer.update()
    pts = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    size = hi - lo
    kind, value = fit
    current = {"h": size.z, "w": size.x, "d": size.y}[kind]
    k = value / max(current, 1e-4)
    pivot = Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z))
    for r in roots:
        r.location = (r.location - pivot) * k
        r.scale = r.scale * k
    if stretch:
        for r in roots:
            r.scale = (r.scale[0] * stretch[0], r.scale[1] * stretch[2], r.scale[2] * stretch[1])
    holder = bpy.data.objects.new(asset, None)
    bpy.context.collection.objects.link(holder)
    for r in roots:
        r.parent = holder
    holder.location = game(x, y, z)
    holder.rotation_euler = (0, 0, math.radians(rot))
    bpy.context.view_layer.update()
    for o in meshes:
        o["owner"] = owner
        o["asset"] = asset
        for slot in o.material_slots:
            m = slot.material
            if not (m and m.use_nodes) or m.get("tinted"):
                continue
            drop_vertex_colour(m)
            if tint:
                tint_material(m, tint)
            if recolor:
                recolor_material(m, *recolor)
            bsdf = next((n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
            if bsdf and nometal:
                for l in [l for l in m.node_tree.links if l.to_socket == bsdf.inputs["Metallic"]]:
                    m.node_tree.links.remove(l)
                bsdf.inputs["Metallic"].default_value = 0.0
            if bsdf and glow:
                for key, (color, strength) in glow.items():
                    if key in m.name:
                        bsdf.inputs["Emission Color"].default_value = (*color, 1)
                        bsdf.inputs["Emission Strength"].default_value = strength
            m["tinted"] = True
    return holder


def drop_vertex_colour(m):
    """Poly Haven glTFs carry masks in COLOR_0, which the importer
    multiplies into the base colour; use the texture alone."""
    nt = m.node_tree
    for mix in [n for n in nt.nodes if n.type == "MIX"]:
        sources = {l.to_socket.identifier: l.from_node for l in nt.links if l.to_node == mix}
        if not any(src.type == "VERTEX_COLOR" for src in sources.values()):
            continue
        keep = next((l.from_socket for l in nt.links if l.to_node == mix and l.from_node.type != "VERTEX_COLOR"), None)
        targets = [l.to_socket for l in nt.links if l.from_node == mix]
        for out in [l for l in nt.links if l.from_node == mix]:
            nt.links.remove(out)
        if keep:
            for socket in targets:
                nt.links.new(keep, socket)


def recolor_material(m, color, gain):
    """Takes hue and saturation from `color`, keeps the texture's shading."""
    nt = m.node_tree
    bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
    link = next((l for l in nt.links if bsdf and l.to_socket == bsdf.inputs["Base Color"]), None)
    if not link:
        return
    hue = nt.nodes.new("ShaderNodeMix")
    hue.data_type = "RGBA"
    hue.blend_type = "COLOR"
    hue.inputs["Factor"].default_value = 1.0
    hue.inputs["B"].default_value = (*color, 1)
    nt.links.new(link.from_socket, hue.inputs["A"])
    lift = nt.nodes.new("ShaderNodeMix")
    lift.data_type = "RGBA"
    lift.blend_type = "MULTIPLY"
    lift.inputs["Factor"].default_value = 1.0
    lift.inputs["B"].default_value = (gain, gain, gain, 1)
    nt.links.new(hue.outputs["Result"], lift.inputs["A"])
    nt.links.remove(link)
    nt.links.new(lift.outputs["Result"], bsdf.inputs["Base Color"])


def tint_material(m, tint):
    """Multiplies a model's base colour (keeps its texture detail)."""
    nt = m.node_tree
    bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
    if not bsdf:
        return
    link = next((l for l in nt.links if l.to_socket == bsdf.inputs["Base Color"]), None)
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY" if max(tint) <= 1.0 else "MIX"
    mix.inputs["Factor"].default_value = 1.0
    mix.inputs["B"].default_value = (*tint[:3], 1)
    if link:
        nt.links.new(link.from_socket, mix.inputs["A"])
        nt.links.remove(link)
    else:
        mix.inputs["A"].default_value = bsdf.inputs["Base Color"].default_value
    nt.links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    m["tinted"] = True


# ---------------------------------------------------------------- the room
def build():
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from apartment import build_apartment
    build_apartment(sys.modules[__name__])


def camera(eye, target, fov):
    cam = bpy.data.cameras.new("cam")
    cam.sensor_fit = "VERTICAL"
    cam.angle_y = fov
    o = bpy.data.objects.new("cam", cam)
    bpy.context.collection.objects.link(o)
    o.location = game(*eye)
    direction = game(*target) - o.location
    o.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    bpy.context.scene.camera = o
    return o


def setup_cycles(samples=128):
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    prefs = bpy.context.preferences.addons["cycles"].preferences
    for kind in ("OPTIX", "CUDA"):
        try:
            prefs.compute_device_type = kind
            prefs.get_devices()
            if any(d.type == kind for d in prefs.devices):
                for d in prefs.devices:
                    d.use = d.type == kind
                scene.cycles.device = "GPU"
                break
        except TypeError:
            continue
    scene.cycles.samples = samples
    scene.cycles.use_denoising = True
    scene.view_settings.view_transform = "AgX"
    scene.view_settings.look = "AgX - Medium High Contrast"


if __name__ == "__main__":
    build()
    preview = arg("--preview")
    if preview:
        setup_cycles(int(arg("--samples", "96")))
        view = arg("--view", "centre")
        x = {"garden": -10, "kitchen": -5.5, "left": -5.5, "study": 5.5, "right": 5.5, "bedroom": 10}.get(view, 0.0)
        camera((x, 1.65, 6.0), (x, 1.25, -0.7), 0.65)
        if view == "overview":
            camera((0, 5, 15), (0, 1.0, -1), 0.62)
        scene = bpy.context.scene
        scene.render.resolution_x, scene.render.resolution_y = (1920, 720) if view == "overview" else (1280, 720)
        scene.render.filepath = os.path.abspath(preview)
        bpy.ops.render.render(write_still=True)
        print("preview", preview)
    save = arg("--save")
    if save:
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(save))
        print("saved", save)
