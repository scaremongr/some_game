# Builds the apartment arena in Blender from CC0 assets (see fetch_polyhaven.py).
#   blender -b --factory-startup --python tools/room/build_room.py -- [--preview out.png] [--save room.blend]
#
# Coordinates follow the game: x to the right, y up, z towards the camera;
# fighters walk the lane z = 0 between the arena walls at x = +-3.34. Blender
# is z-up, so a game point (x, y, z) sits at Blender (x, -z, y).
#
# Every mesh carries an "owner" property: the combat room object it belongs to
# (combat/src/room.rs LAYOUT index 0..19, 20/21 the arena walls) or -1 for the
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


def texture_path(tex, kind):
    folder = os.path.join(ROOT, "textures", tex)
    for f in os.listdir(folder):
        if f"_{kind}_" in f.lower():
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
    """Imports a Poly Haven model, scales it to `fit` (height 'h', width 'w'
    or depth 'd' in metres), stands it on y and turns it `rot` degrees
    (0 = facing the camera)."""
    before = set(bpy.data.objects)
    folder = os.path.join(ROOT, "models", asset)
    gltf = next(f for f in os.listdir(folder) if f.endswith(".gltf"))
    bpy.ops.import_scene.gltf(filepath=os.path.join(folder, gltf))
    new = [o for o in bpy.data.objects if o not in before]
    meshes = [o for o in new if o.type == "MESH"]
    roots = [o for o in new if o.parent is None]
    for o in meshes:
        if o.data.users > 1:
            o.data = o.data.copy()
        decimate(o, max(400, tris // max(1, len(meshes))))
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
    bpy.ops.wm.read_factory_settings(use_empty=True)
    lavender = surface("wall_lavender", "painted_plaster_wall", (0.50, 0.46, 0.78), 1.6, 0.8, 0.35)
    mint = surface("wall_mint", "painted_plaster_wall", (0.42, 0.62, 0.52), 1.6, 0.7, 0.3)
    brick = surface("brick", "brick_wall_02", (1.0, 0.92, 0.86), 1.5, 0.85, 0.6)
    parquet = surface("parquet", "herringbone_parquet", (1.0, 0.92, 0.82), 1.8, 0.45, 0.15)
    oak = surface("oak", "wood_floor", (0.85, 0.62, 0.42), 1.2, 0.5, 0.1)
    beam = surface("beam", "dark_wood", (1.0, 0.85, 0.7), 1.0, 0.6, 0.2)
    blue = surface("kitchen_blue", "blue_painted_planks", (0.85, 1.0, 1.0), 0.9, 0.55, 0.3)
    counter = surface("counter", "kitchen_wood", (1, 1, 1), 0.8, 0.35, 0.1)
    tiles = surface("kitchen_tiles", "floor_tiles_06", (1, 1, 1), 1.2, 0.3, 0.2)
    trim = surface("trim", color=(0.92, 0.9, 0.86), rough=0.4)
    door = surface("door", "white_planks_clean", (0.45, 0.40, 0.75), 1.0, 0.5, 0.2)
    subfloor = surface("subfloor", color=(0.32, 0.26, 0.2), rough=0.9)
    ceiling = surface("ceiling", "plastered_wall_02", (0.95, 0.92, 0.88), 2.0, 0.9, 0.2)
    fridge = surface("fridge", color=(0.93, 0.92, 0.86), rough=0.25)
    chrome = surface("chrome", color=(0.75, 0.75, 0.78), rough=0.15)
    facade = surface("facade", "brick_wall_02", (0.75, 0.62, 0.55), 1.2, 0.9, 0.4)
    curtain = surface("curtain", "rough_linen", (0.95, 0.62, 0.38), 0.6, 0.9, 0.2)
    linen = surface("linen", "rough_linen", (0.95, 0.9, 0.78), 0.5, 0.9, 0.2)

    # Floor: fixed base, then the two breakable finish sections (tiles of the
    # old layout: 6 x 6 boards around x = +-2.1), with the rug baked into them.
    box("floor", -1, (0, -0.06, 0.6), (15.0, 0.1, 6.2), subfloor, 0)
    for side, owner in ((-1, 16), (1, 17)):
        for row in range(6):
            for col in range(6):
                cx = side * 2.1 + (col - 2.5) * 0.70
                cz = (row - 2.5) * 0.66
                box("floorboard", owner, (cx, -0.005, cz), (0.70, 0.02, 0.66), parquet, 0.0)
    for x0, x1 in ((-7.0, -4.2), (4.2, 7.0)):
        box("floor_side", -1, ((x0 + x1) / 2, -0.005, 0), (x1 - x0, 0.02, 3.96), parquet, 0)
    box("floor_front", -1, (0, -0.005, 3.2), (14.0, 0.02, 2.44), parquet, 0)
    box("floor_back", -1, (0, -0.005, -2.0), (14.0, 0.02, 0.08), parquet, 0)
    # The rug is cut along the floorboards so it cracks with them.
    rug_tiles(rug_material(), (-1.15, 3.05), (-1.5, 1.1))

    # Back wall (z = -1.8): permanent exterior shell, kitchen brick on
    # the left, windows at x = +-2.6, the apartment door in the middle.
    window_holes = [(-3.15, -2.05, 1.05, 2.45), (1.95, 3.25, 0.75, 2.75)]
    door_hole = (-0.5, 0.5, 0.0, 2.15)
    for i, cx in enumerate((-3.6, -1.8, 0.0, 1.8, 3.6)):
        mat = brick if cx < -2.7 else lavender
        holes = [h for h in window_holes + [door_hole] if h[0] < cx + 0.9 and h[1] > cx - 0.9]
        grid_wall("backwall", -1, cx - 0.9, cx + 0.9, 0.0, 3.3, -1.8, 0.2, mat, 3, 4, holes)
    # The entrance and exterior trim stay attached to the shell.
    box("door", -1, (0.0, 1.06, -1.83), (0.96, 2.1, 0.05), door)
    for dx in (-0.53, 0.53):
        box("doorframe", -1, (dx, 1.1, -1.7), (0.08, 2.2, 0.06), trim)
    box("doorframe", -1, (0, 2.22, -1.7), (1.14, 0.1, 0.06), trim)
    box("doorknob", -1, (0.36, 1.0, -1.79), (0.05, 0.05, 0.05), chrome)
    # Wainscot and baseboard.
    for i, cx in enumerate((-1.8, 1.8, 3.6)):
        box("baseboard", -1, (cx, 0.07, -1.69), (1.8, 0.14, 0.03), trim, 0.002)
    box("wainscot", -1, (3.6, 0.5, -1.69), (1.8, 1.0, 0.02), mint, 0.0)
    box("chair_rail", -1, (3.6, 1.02, -1.68), (1.8, 0.05, 0.04), trim, 0.002)

    # Windows (glass objects 5 and 6): frames, mullions and panes.
    glass_m = glass()
    for owner, (x0, x1, y0, y1) in ((5, window_holes[0]), (6, window_holes[1])):
        cx, cw, ch = (x0 + x1) / 2, x1 - x0, y1 - y0
        cols, rows = (2, 2) if owner == 5 else (3, 3)
        for c in range(cols):
            for r in range(rows):
                px = x0 + (c + 0.5) * cw / cols
                py = y0 + (r + 0.5) * ch / rows
                box("pane", owner, (px, py, -1.8), (cw / cols - 0.04, ch / rows - 0.04, 0.01), glass_m, 0)
        for c in range(cols + 1):
            box("mullion", owner, (x0 + c * cw / cols, (y0 + y1) / 2, -1.79), (0.045, ch, 0.06), trim)
        for r in range(rows + 1):
            box("transom", owner, (cx, y0 + r * ch / rows, -1.79), (cw, 0.045, 0.06), trim)
        box("sill", owner, (cx, y0 - 0.03, -1.66), (cw + 0.16, 0.05, 0.22), trim)

    # Side partitions (objects 18 / 19) and the arena walls (owners 20 / 21).
    side_wall("sidewall", 18, -4.65, -1.83, 1.23, 0.0, 3.3, 0.2, brick, 4, 4)
    side_wall("sidewall", 19, 4.65, -1.83, 1.23, 0.0, 3.3, 0.2, lavender, 4, 4)
    # Left arena wall: kitchen counter island, doors facing the lane.
    for k, cz in enumerate((-0.45, 0.0, 0.45)):
        box("island_base", 20, (-3.34, 0.44, cz), (0.62, 0.86, 0.44), blue)
        box("island_door", 20, (-3.02, 0.46, cz), (0.02, 0.62, 0.36), blue, 0.003)
        box("island_knob", 20, (-3.0, 0.7, cz + 0.12), (0.03, 0.03, 0.03), chrome, 0)
    box("island_top", 20, (-3.34, 0.9, 0.0), (0.72, 0.05, 1.42), counter)
    # Right arena wall: an open wooden divider shelf with books.
    for dz in (-0.66, 0.0, 0.66):
        box("divider_post", 21, (3.34, 1.0, dz), (0.36, 2.0, 0.05), oak)
    for y in (0.06, 0.55, 1.05, 1.55, 1.98):
        box("divider_shelf", 21, (3.34, y, 0.0), (0.36, 0.04, 1.36), oak)

    # Ceiling with beams.
    box("ceiling", -1, (0, 3.35, -0.2), (14.0, 0.1, 4.0), ceiling, 0)
    for x in (-3.6, -1.2, 1.2, 3.6):
        box("beam", -1, (x, 3.18, -0.2), (0.2, 0.26, 4.0), beam)
    box("beam", -1, (0, 3.18, -1.6), (9.4, 0.24, 0.24), beam)
    box("crown", -1, (0, 3.26, -1.66), (9.3, 0.08, 0.1), trim)

    # Kitchen along the back wall on the left (fixed).
    box("kitchen_base", -1, (-3.5, 0.44, -1.45), (1.9, 0.88, 0.6), blue)
    box("kitchen_top", -1, (-3.5, 0.9, -1.45), (1.96, 0.05, 0.64), counter)
    box("sink", -1, (-2.6, 0.9, -1.45), (0.55, 0.04, 0.4), chrome, 0.01)
    box("backsplash", -1, (-3.5, 1.15, -1.69), (1.9, 0.45, 0.02), tiles, 0)
    box("fridge", -1, (-4.25, 0.9, -1.35), (0.72, 1.8, 0.66), fridge, 0.05)
    box("fridge_handle", -1, (-3.95, 1.25, -1.0), (0.03, 0.4, 0.04), chrome, 0.01)
    box("shelf", -1, (-3.6, 2.35, -1.62), (1.2, 0.04, 0.3), oak)
    box("valance", -1, (-2.6, 2.55, -1.68), (1.3, 0.22, 0.05), curtain, 0.01)
    box("kitchen_floor", -1, (-5.1, 0.0, -0.6), (1.8, 0.012, 2.4), tiles, 0)

    # Drapes on the big window.
    for dx in (-0.78, 0.78):
        box("drape", -1, (2.6 + dx, 1.7, -1.6), (0.36, 2.9, 0.08), curtain, 0.02)
    box("blind", -1, (2.6, 2.62, -1.72), (1.4, 0.3, 0.04), linen, 0.01)

    # Outside: the building across the street, lit by the afternoon sun.
    box("facade", -1, (0, 1.5, -13.0), (40.0, 9.0, 0.3), facade, 0)
    dark_glass = surface("dark_glass", color=(0.05, 0.07, 0.09), rough=0.1)
    for fx in range(-9, 10):
        for fy in (0.2, 2.6, 5.0):
            box("facade_window", -1, (fx * 2.1, fy, -12.8), (1.0, 1.5, 0.05), dark_glass, 0)
            box("facade_sill", -1, (fx * 2.1, fy - 0.8, -12.75), (1.2, 0.08, 0.12), trim, 0)
    box("street", -1, (0, -3.0, -8.0), (40.0, 0.1, 12.0), subfloor, 0)
    # Two furnished side rooms open up when the interior partitions break.
    annex_rug = rug_material()
    for s in (-1, 1):
        box("hall_floor", -1, (s * 6.0, -0.005, -0.3), (2.6, 0.02, 3.2), oak, 0)
        box("hall_wall", -1, (s * 7.3, 1.65, -0.3), (0.2, 3.3, 3.4), lavender if s > 0 else brick, 0)
        box("hall_ceiling", -1, (s * 6.0, 3.35, -0.3), (2.8, 0.1, 3.4), ceiling, 0)
        box("annex_back", -1, (s * 5.9, 1.65, -1.87), (2.8, 3.3, 0.18), mint if s > 0 else brick, 0)
        box("annex_skirt", -1, (s * 5.9, 0.09, -1.75), (2.8, 0.18, 0.05), trim)
        box("annex_rail", -1, (s * 5.9, 2.88, -1.75), (2.8, 0.12, 0.08), trim)
        for x in (s * 4.85, s * 6.95):
            box("annex_pilaster", -1, (x, 1.65, -1.75), (0.12, 3.1, 0.12), trim)
        box("annex_rug", -1, (s * 5.8, 0.012, 0.05), (2.15, 0.018, 1.7), annex_rug, 0.005)
        box("annex_sconce", -1, (s * 5.8, 2.38, -1.65), (0.46, 0.14, 0.18), chrome)
        box("annex_glow", -1, (s * 5.8, 2.30, -1.59), (0.34, 0.05, 0.11),
            emissive(f"annex_lamp_{s}", (1.0, 0.72, 0.42), 5.0))

    # Left room: reading corner and shelves along the back wall.
    place("wooden_bookshelf_worn", -1, -6.65, -1.35, 0, ("h", 2.05), tris=4500)
    place("side_table_01", -1, -5.08, -1.27, 0, ("h", 0.62), tris=3000)
    place("book_encyclopedia_set_01", -1, -5.08, -1.27, 0, ("w", 0.45), y=0.64, tris=1600)
    place("painted_wooden_chair_02", -1, -6.12, -0.95, -35, ("h", 0.9), tris=3500)
    place("hanging_picture_frame_02", -1, -5.20, -1.74, 0, ("w", 0.72), y=1.78, tris=1600)
    # Right room: a sitting area with plants and a cabinet.
    place("ArmChair_01", -1, 5.08, -1.06, 25, ("h", 0.94), tris=4500)
    place("side_table_tall_01", -1, 6.02, -1.32, 0, ("h", 0.68), tris=2500)
    place("brass_vase_02", -1, 6.02, -1.32, 0, ("h", 0.24), y=0.68, tris=1200)
    place("potted_plant_04", -1, 6.85, -1.30, 0, ("h", 1.38), tris=4500)
    place("hanging_picture_frame_03", -1, 5.72, -1.74, 0, ("w", 0.92), y=1.76, tris=1600)

    # Furniture. Owners follow room.rs; -1 stays put.
    place("round_wooden_table_01", 7, -1.45, -0.7, 0, ("h", 0.76))
    place("painted_wooden_chair_01", 7, -1.95, -0.85, 70, ("h", 0.95))
    place("painted_wooden_chair_02", 7, -1.0, -1.1, -130, ("h", 0.95))
    place("ceramic_vase_01", 7, -1.4, -0.72, 0, ("h", 0.26), y=0.76)
    place("Ottoman_01", 9, -0.6, -0.65, 15, ("h", 0.42), recolor=((0.2, 0.55, 0.42), 2.4))
    place("WoodenTable_02", 10, 0.65, -0.65, 0, ("h", 0.5))
    place("brass_vase_01", 10, 0.65, -0.65, 0, ("h", 0.28), y=0.5)
    place("potted_plant_02", 15, 0.0, -1.25, 20, ("h", 1.1), tris=6000)
    place("painted_wooden_cabinet_02", 11, -3.75, -0.95, 55, ("h", 1.9))
    place("vintage_cabinet_01", 12, 3.95, -1.05, -60, ("h", 1.85), tris=7000)
    place("mantel_clock_01", 12, 3.95, -1.05, -60, ("h", 0.25), y=1.0)
    place("CoffeeTable_01", 8, 1.45, -0.7, 0, ("h", 0.45), nometal=True)
    place("wicker_basket_01", 8, 1.2, -0.7, 10, ("h", 0.14), y=0.45)
    lamp_glow = {"globe": ((1.0, 0.82, 0.6), 6.0)}
    place("modern_ceiling_lamp_01", 13, -1.6, -0.45, 0, ("h", 0.9), y=2.3, glow=lamp_glow)
    place("modern_ceiling_lamp_01", 14, 1.6, -0.45, 0, ("h", 0.9), y=2.3, glow=lamp_glow)
    place("Sofa_01", -1, 1.35, -1.28, 0, ("w", 2.3), tint=(1.12, 1.08, 1.0), tris=9000)
    place("throw_pillows_01", -1, 1.35, -1.22, 0, ("w", 1.4), y=0.42)
    place("ArmChair_01", -1, 2.75, -1.0, -25, ("h", 1.0), tint=(1.12, 1.08, 1.0), tris=6000)
    place("side_table_tall_01", -1, 0.1, -1.55, 0, ("h", 0.7))
    place("fancy_picture_frame_01", -1, 1.3, -1.69, 0, ("w", 0.9), y=1.45)
    place("hanging_picture_frame_03", -1, -1.6, -1.69, 0, ("h", 0.55), y=1.35)
    place("fancy_picture_frame_02", -1, 3.7, -1.69, 0, ("h", 0.6), y=1.35)
    place("wall_clock", -1, -3.9, -1.69, 0, ("h", 0.32), y=2.0)
    place("brass_vase_02", -1, -3.9, -1.45, 0, ("h", 0.3), y=0.92)
    # Books on the divider shelves.
    for y in (0.59, 1.09, 1.59):
        place("book_encyclopedia_set_01", 21, 3.34, 0.0, 90, ("w", 1.1), y=y, tris=2500)

    # Light: afternoon sun through the windows, warm lamps inside, and the
    # open fourth wall lets the sky fill the room.
    world = bpy.data.worlds.new("sky")
    bpy.context.scene.world = world
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.55, 0.62, 0.75, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.5
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 9.0
    sun.color = (1.0, 0.86, 0.68)
    sun.angle = math.radians(2.5)
    so = bpy.data.objects.new("sun", sun)
    bpy.context.collection.objects.link(so)
    # Afternoon sun from behind the building, through the back windows
    # onto the floor in front of the sofa.
    so.rotation_euler = game(-0.30, -0.74, 0.60).normalized().to_track_quat("-Z", "Y").to_euler()
    for x in (-1.6, 1.6):
        p = bpy.data.lights.new(f"lamp{x}", "POINT")
        p.energy = 120
        p.color = (1.0, 0.72, 0.45)
        p.shadow_soft_size = 0.12
        po = bpy.data.objects.new(f"lamp{x}", p)
        bpy.context.collection.objects.link(po)
        po.location = game(x, 2.22, -0.45)
    for x in (-5.8, 5.8):
        p = bpy.data.lights.new(f"annex_lamp{x}", "POINT")
        p.energy = 95
        p.color = (1.0, 0.70, 0.44) if x < 0 else (0.74, 0.82, 1.0)
        p.shadow_soft_size = 0.25
        po = bpy.data.objects.new(f"annex_lamp{x}", p)
        bpy.context.collection.objects.link(po)
        po.location = game(x, 2.3, -1.45)
    fill = bpy.data.lights.new("fill", "AREA")
    fill.energy = 90
    fill.size = 6
    fill.color = (1.0, 0.9, 0.8)
    fo = bpy.data.objects.new("fill", fill)
    bpy.context.collection.objects.link(fo)
    fo.location = game(0, 3.0, 3.5)
    fo.rotation_euler = (math.radians(62), 0, 0)


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
        x = {"left": -5.8, "right": 5.8}.get(view, 0.0)
        camera((x, 1.25, 5.0), (x, 1.05, 0.0), 0.62)
        scene = bpy.context.scene
        scene.render.resolution_x, scene.render.resolution_y = 1280, 720
        scene.render.filepath = os.path.abspath(preview)
        bpy.ops.render.render(write_still=True)
        print("preview", preview)
    save = arg("--save")
    if save:
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(save))
        print("saved", save)
