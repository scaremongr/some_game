# Night city behind the apartment windows: a procedural New York-like view,
# rendered by Cycles into two layers for parallax in the game.
#   blender -b --factory-startup --python tools/backdrop/city.py -- [--out assets/backdrop] [--samples 256] [--width 2560]
# (then tools/backdrop/shrink.py runs on its own to palette the near layer).
# far.jpg  - sky, moon, skyline across the river, bridge, water (opaque)
# near.png - rooftops with water towers across the street (transparent above)
# Everything is generated here (our own geometry and shaders), so the images
# are free to ship. The camera matches src/game/backdrop.rs: it looks level,
# horizontal field of view BACKDROP_TAN_H, vertical BACKDROP_TAN_V.
import math
import os
import random
import sys

import bpy
from mathutils import Vector

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


def arg(name, default=None):
    return ARGS[ARGS.index(name) + 1] if name in ARGS else default


ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.abspath(os.path.join(ROOT, arg("--out", "assets/backdrop")))
SAMPLES = int(arg("--samples", "256"))
WIDTH = int(arg("--width", "2560"))
# Must match backdrop.rs: tan of half the horizontal / vertical view.
TAN_H = 1.0
TAN_V = 0.25
HEIGHT = int(round(WIDTH * TAN_V / TAN_H / 8)) * 8
EYE = 42.0  # metres above the street: a 13th floor
rng = random.Random(7)

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
near_objects, far_objects = [], []


# ---------------------------------------------------------------- materials
def node(nt, kind, **inputs):
    n = nt.nodes.new(kind)
    for k, v in inputs.items():
        n.inputs[k].default_value = v
    return n


def math_node(nt, op, a=None, b=None, value=None):
    n = nt.nodes.new("ShaderNodeMath")
    n.operation = op
    for i, x in enumerate((a, b)):
        if x is None:
            continue
        if isinstance(x, (int, float)):
            n.inputs[i].default_value = x
        else:
            nt.links.new(x, n.inputs[i])
    return n.outputs[0]


def haze(nt, shader, color, strength, reach):
    """Atmospheric perspective: far surfaces fade into glowing city haze."""
    cam = nt.nodes.new("ShaderNodeCameraData")
    k = math_node(nt, "DIVIDE", cam.outputs["View Distance"], reach)
    k = math_node(nt, "MULTIPLY", k, -1.0)
    k = math_node(nt, "EXPONENT", k)
    k = math_node(nt, "SUBTRACT", 1.0, k)
    fog = node(nt, "ShaderNodeEmission", Color=(*color, 1), Strength=strength)
    mix = nt.nodes.new("ShaderNodeMixShader")
    nt.links.new(k, mix.inputs[0])
    nt.links.new(shader, mix.inputs[1])
    nt.links.new(fog.outputs[0], mix.inputs[2])
    return mix.outputs[0]


def facade(name, base, glass, spacing, floor, lit, ribbon=False, reach=2600.0, warm=0.75, bright=4.0):
    """Building skin with a grid of windows; a random share of them is lit,
    mostly warm, some cool (screens), at varied brightness."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    info = nt.nodes.new("ShaderNodeObjectInfo")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(geo.outputs["Position"], sep.inputs[0])
    along = math_node(nt, "ADD", sep.outputs["X"], sep.outputs["Y"])
    a = math_node(nt, "DIVIDE", along, spacing)
    b = math_node(nt, "DIVIDE", sep.outputs["Z"], floor)
    ca, cb = math_node(nt, "FLOOR", a), math_node(nt, "FLOOR", b)
    fa, fb = math_node(nt, "FRACT", a), math_node(nt, "FRACT", b)
    # Window opening inside the cell.
    if ribbon:
        mask_a = math_node(nt, "GREATER_THAN", fa, -1.0)
    else:
        mask_a = math_node(nt, "MULTIPLY", math_node(nt, "GREATER_THAN", fa, 0.18), math_node(nt, "LESS_THAN", fa, 0.82))
    mask_b = math_node(nt, "MULTIPLY", math_node(nt, "GREATER_THAN", fb, 0.22), math_node(nt, "LESS_THAN", fb, 0.78))
    mask = math_node(nt, "MULTIPLY", mask_a, mask_b)
    # A random value per window (per floor stretch for ribbons).
    cell = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(math_node(nt, "FLOOR", math_node(nt, "DIVIDE", ca, 2.0)) if ribbon else ca, cell.inputs[0])
    nt.links.new(cb, cell.inputs[1])
    nt.links.new(math_node(nt, "MULTIPLY", info.outputs["Random"], 997.0), cell.inputs[2])
    noise = nt.nodes.new("ShaderNodeTexWhiteNoise")
    noise.noise_dimensions = "3D"
    nt.links.new(cell.outputs[0], noise.inputs["Vector"])
    # Whole floors and blocks go dark together: offices empty at night.
    block = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(math_node(nt, "FLOOR", math_node(nt, "DIVIDE", ca, 6.0)), block.inputs[0])
    nt.links.new(math_node(nt, "FLOOR", math_node(nt, "DIVIDE", cb, 3.0)), block.inputs[1])
    nt.links.new(math_node(nt, "MULTIPLY", info.outputs["Random"], 571.0), block.inputs[2])
    blocks = nt.nodes.new("ShaderNodeTexWhiteNoise")
    blocks.noise_dimensions = "3D"
    nt.links.new(block.outputs[0], blocks.inputs["Vector"])
    chance = math_node(nt, "MULTIPLY", blocks.outputs["Value"], lit * 2.0)
    on = math_node(nt, "LESS_THAN", noise.outputs["Value"], chance)
    on = math_node(nt, "MULTIPLY", on, mask)
    sep_c = nt.nodes.new("ShaderNodeSeparateColor")
    nt.links.new(noise.outputs["Color"], sep_c.inputs[0])
    tone = nt.nodes.new("ShaderNodeMix")
    tone.data_type = "RGBA"
    nt.links.new(math_node(nt, "GREATER_THAN", sep_c.outputs["Red"], warm), tone.inputs["Factor"])
    tone.inputs["A"].default_value = (1.0, 0.66, 0.36, 1)
    tone.inputs["B"].default_value = (0.78, 0.86, 1.0, 1)
    level = math_node(nt, "MULTIPLY", math_node(nt, "ADD", math_node(nt, "MULTIPLY", math_node(nt, "POWER", sep_c.outputs["Green"], 2.0), 1.3), 0.12), bright)
    glow = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(tone.outputs["Result"], glow.inputs["Color"])
    nt.links.new(math_node(nt, "MULTIPLY", on, level), glow.inputs["Strength"])
    wall = node(nt, "ShaderNodeBsdfPrincipled", **{"Base Color": (*base, 1), "Roughness": 0.75})
    pane = node(nt, "ShaderNodeBsdfPrincipled", **{"Base Color": (*glass, 1), "Roughness": 0.12, "Metallic": 0.6})
    skin = nt.nodes.new("ShaderNodeMixShader")
    nt.links.new(mask, skin.inputs[0])
    nt.links.new(wall.outputs[0], skin.inputs[1])
    nt.links.new(pane.outputs[0], skin.inputs[2])
    both = nt.nodes.new("ShaderNodeAddShader")
    nt.links.new(skin.outputs[0], both.inputs[0])
    nt.links.new(glow.outputs[0], both.inputs[1])
    nt.links.new(haze(nt, both.outputs[0], (0.30, 0.22, 0.26), 0.55, reach), out.inputs["Surface"])
    return m


def flat(name, color, rough=0.6, emission=None, strength=0.0, reach=2600.0, metal=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    bsdf = node(nt, "ShaderNodeBsdfPrincipled", **{"Base Color": (*color, 1), "Roughness": rough, "Metallic": metal})
    if emission:
        bsdf.inputs["Emission Color"].default_value = (*emission, 1)
        bsdf.inputs["Emission Strength"].default_value = strength
    nt.links.new(haze(nt, bsdf.outputs[0], (0.30, 0.22, 0.26), 0.55, reach), out.inputs["Surface"])
    return m


def light_mat(name, color, strength):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    e = node(nt, "ShaderNodeEmission", Color=(*color, 1), Strength=strength)
    nt.links.new(e.outputs[0], out.inputs["Surface"])
    return m


# ---------------------------------------------------------------- geometry
def box(x, y, w, d, z0, z1, mat, group, rot=0.0):
    bpy.ops.mesh.primitive_cube_add(size=1, location=(x, y, (z0 + z1) / 2), rotation=(0, 0, rot))
    o = bpy.context.object
    o.scale = (w, d, z1 - z0)
    o.data.materials.append(mat)
    group.append(o)
    return o


def cyl(x, y, r, z0, z1, mat, group, verts=16, r2=None, rot=0.0):
    bpy.ops.mesh.primitive_cone_add(vertices=verts, radius1=r, radius2=r if r2 is None else r2, depth=z1 - z0,
                                    location=(x, y, (z0 + z1) / 2), rotation=(0, 0, rot))
    o = bpy.context.object
    o.data.materials.append(mat)
    group.append(o)
    return o


def ball(x, y, z, r, mat, group):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=8, ring_count=6, radius=r, location=(x, y, z))
    o = bpy.context.object
    o.data.materials.append(mat)
    group.append(o)
    return o


red = light_mat("aviation", (1.0, 0.08, 0.04), 60.0)
white = light_mat("white_light", (1.0, 0.9, 0.75), 40.0)
skins = [
    facade("office_warm", (0.035, 0.035, 0.04), (0.02, 0.025, 0.035), 3.2, 3.9, 0.22, warm=0.6, bright=3.2),
    facade("office_cool", (0.05, 0.05, 0.055), (0.03, 0.04, 0.05), 2.6, 3.8, 0.2, warm=0.35, bright=3.2),
    facade("glass_tower", (0.02, 0.03, 0.045), (0.015, 0.03, 0.05), 2.0, 4.0, 0.16, ribbon=True, warm=0.3, bright=2.6),
    facade("stone_deco", (0.09, 0.075, 0.065), (0.02, 0.02, 0.025), 2.4, 3.6, 0.26, warm=0.75, bright=3.2),
]


def tower(x, y, w, d, h, skin, setbacks=0, crown=None, spire=0.0, beacon=True):
    """A tower with optional setbacks (stepped, the deco way), a lit crown and a spire."""
    z = 0.0
    steps = [1.0] + [0.78 ** (i + 1) for i in range(setbacks)]
    heights = [h * (0.55 if setbacks else 1.0)] + [h * 0.45 / max(1, setbacks)] * setbacks
    for k, hh in zip(steps, heights):
        box(x, y, w * k, d * k, z, z + hh, skin, far_objects)
        z += hh
    top_w, top_d = w * steps[-1], d * steps[-1]
    if crown:
        color, strength, tiers = crown
        glow = light_mat(f"crown_{x:.0f}", color, strength)
        for i in range(tiers):
            k = 0.85 - i * 0.6 / tiers
            hh = h * 0.035
            box(x, y, top_w * k, top_d * k, z, z + hh * 0.6, glow, far_objects)
            box(x, y, top_w * k * 0.97, top_d * k * 0.97, z + hh * 0.6, z + hh, skin, far_objects)
            z += hh
    if spire > 0:
        cyl(x, y, max(1.0, top_w * 0.06), z, z + spire, skins[3], far_objects, verts=8, r2=0.3)
        z += spire
    if beacon:
        ball(x, y, z + 1.5, 2.2, red, far_objects)
    return z


# Manhattan across the river: a dense band of towers, tallest midtown and downtown.
for i in range(170):
    x = rng.uniform(-2300, 2300)
    y = rng.uniform(1250, 2900)
    centre = math.exp(-((x + 250) / 900) ** 2) + 0.8 * math.exp(-((x - 1100) / 500) ** 2)
    h = rng.uniform(45, 110) + centre * rng.uniform(40, 230)
    w, d = rng.uniform(22, 55), rng.uniform(22, 50)
    skin = rng.choice(skins)
    tower(x, y, w, d, h, skin, setbacks=rng.choice([0, 0, 1, 2]) if h > 120 else 0,
          beacon=h > 170 and rng.random() < 0.6)
# Low buildings along the waterfront.
for i in range(140):
    x = rng.uniform(-2300, 2300)
    y = rng.uniform(1120, 1300)
    box(x, y, rng.uniform(25, 60), rng.uniform(20, 40), 0, rng.uniform(15, 40), rng.choice(skins[:2] + skins[3:]), far_objects)
# The icons, evoked, not copied: a stepped deco tower with a lit crown and
# mast, a slender crown of arches, a tapered glass tower downtown.
top = tower(-260, 1700, 62, 44, 300, skins[3], setbacks=3, crown=((0.75, 0.55, 1.0), 25.0, 4), spire=70)
tower(380, 1550, 34, 34, 240, skins[3], setbacks=1, crown=((1.0, 0.95, 0.85), 30.0, 6), spire=45)
cyl(1150, 2350, 48, 0, 380, skins[2], far_objects, verts=4, r2=26, rot=math.radians(45))
cyl(1150, 2350, 1.6, 380, 500, skins[3], far_objects, verts=8, r2=0.4)
ball(1150, 2350, 502, 2.5, white, far_objects)

# The river and its far bank.
water = bpy.data.materials.new("river")
water.use_nodes = True
nt = water.node_tree
bsdf = nt.nodes["Principled BSDF"]
bsdf.inputs["Base Color"].default_value = (0.004, 0.006, 0.01, 1)
bsdf.inputs["Roughness"].default_value = 0.06
coord = nt.nodes.new("ShaderNodeTexCoord")
mapping = node(nt, "ShaderNodeMapping")
mapping.inputs["Scale"].default_value = (0.04, 1.6, 1.0)
nt.links.new(coord.outputs["Object"], mapping.inputs["Vector"])
waves = node(nt, "ShaderNodeTexNoise", Scale=6.0, Detail=6.0, Roughness=0.6)
nt.links.new(mapping.outputs["Vector"], waves.inputs["Vector"])
bump = node(nt, "ShaderNodeBump", Strength=0.6, Distance=0.6)
nt.links.new(waves.outputs["Fac"], bump.inputs["Height"])
nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
bpy.ops.mesh.primitive_plane_add(size=1, location=(0, 700, 0.5))
river = bpy.context.object
river.scale = (8000, 1000, 1)
river.data.materials.append(water)
far_objects.append(river)
box(0, 1160, 8000, 60, 0, 6, flat("embankment", (0.05, 0.045, 0.04)), far_objects)
# Lamps along the far embankment: a necklace that the water stretches.
lamp = light_mat("lamp", (1.0, 0.6, 0.28), 70.0)
for i in range(120):
    ball(-2400 + i * 40 + rng.uniform(-5, 5), 1132, 9, 0.9, lamp, far_objects)
# The near bank below the window.
box(0, 205, 8000, 30, 0, 4, flat("quay", (0.015, 0.014, 0.013)), far_objects)

# A suspension bridge on the left: stone towers, deck lights, cable lights.
stone = flat("bridge_stone", (0.12, 0.10, 0.085))
bridge_lights = light_mat("bridge_lights", (1.0, 0.86, 0.6), 55.0)
a, b = Vector((-1400, 260, 0)), Vector((-520, 1150, 0))
span = b - a
along = span.normalized()
angle = math.atan2(along.y, along.x)
for t in (0.18, 0.82):
    p = a + span * t
    for side in (-1, 1):
        q = p + Vector((-along.y, along.x, 0)) * 12 * side
        box(q.x, q.y, 10, 10, 0, 95, stone, far_objects, rot=angle)
    q = p
    box(q.x, q.y, 34, 10, 82, 92, stone, far_objects, rot=angle)
deck = a + span * 0.5
box(deck.x, deck.y, span.length * 1.1, 26, 38, 41, stone, far_objects, rot=angle)
for i in range(90):
    t = -0.05 + i / 89 * 1.1
    p = a + span * t
    for side in (-1, 1):
        q = p + Vector((-along.y, along.x, 0)) * 13 * side
        ball(q.x, q.y, 42.5, 0.7, bridge_lights, far_objects)
        # Main cables: catenary between the towers, sagging to the deck.
        if 0.18 <= t <= 0.82:
            u = (t - 0.18) / 0.64
            z = 44 + 50 * (2 * u - 1) ** 2
        elif t < 0.18:
            z = 44 + 50 * (t / 0.18)
        else:
            z = 44 + 50 * ((1.1 - t) / 0.28)
        ball(q.x, q.y, z, 0.55, bridge_lights, far_objects)

# Rooftops across the street (the near layer): brick and tar roofs, water
# towers on stilts, bulkheads, chimneys, a few lit windows below the parapets.
brick = facade("brick", (0.045, 0.028, 0.022), (0.012, 0.012, 0.014), 2.8, 3.4, 0.17, warm=0.85, bright=1.8, reach=900)
tar = flat("tar_roof", (0.03, 0.03, 0.032), rough=0.9, reach=900)
wood = flat("tank_wood", (0.09, 0.06, 0.04), rough=0.85, reach=900)
steel = flat("steel", (0.05, 0.05, 0.055), rough=0.5, metal=0.8, reach=900)
# Two rows: the second, farther and a little taller, closes the gaps.
for row in (0, 1):
    x = -700.0 - row * 20
    while x < 700:
        w = rng.uniform(18, 34)
        y = rng.uniform(90, 170) + abs(x) * 0.12 + row * 70
        roof = rng.uniform(24, 34) + row * 8
        box(x + w / 2, y, w - 1.5, rng.uniform(25, 40), 0, roof, brick, near_objects)
        box(x + w / 2, y, w - 1.5, rng.uniform(25, 40), roof, roof + 0.4, tar, near_objects)
        if rng.random() < 0.55:
            # Water tower: barrel on a steel frame, conical cap.
            tx, ty = x + rng.uniform(5, w - 5), y + rng.uniform(-6, 6)
            r = rng.uniform(2.6, 3.6)
            for dx, dy in ((-1, -1), (1, -1), (-1, 1), (1, 1)):
                box(tx + dx * r * 0.6, ty + dy * r * 0.6, 0.35, 0.35, roof, roof + 4.2, steel, near_objects)
            cyl(tx, ty, r, roof + 4.2, roof + 9.5, wood, near_objects, verts=20)
            cyl(tx, ty, r * 1.05, roof + 9.5, roof + 11.6, tar, near_objects, verts=20, r2=0.3)
        if rng.random() < 0.6:
            box(x + rng.uniform(3, w - 3), y, rng.uniform(3, 6), rng.uniform(3, 5), roof, roof + rng.uniform(2.5, 4), brick, near_objects)
        if rng.random() < 0.4:
            cyl(x + rng.uniform(2, w - 2), y + 6, 0.5, roof, roof + rng.uniform(2, 5), steel, near_objects, verts=8)
        x += w

# Sky: deep blue overhead, a sodium-pink glow over the city, faint stars,
# low clouds lit from below and a moon with a halo.
world = bpy.data.worlds.new("night")
scene.world = world
world.use_nodes = True
nt = world.node_tree
nt.nodes.clear()
out = nt.nodes.new("ShaderNodeOutputWorld")
coord = nt.nodes.new("ShaderNodeTexCoord")
sep = nt.nodes.new("ShaderNodeSeparateXYZ")
nt.links.new(coord.outputs["Generated"], sep.inputs[0])
elev = math_node(nt, "MAXIMUM", sep.outputs["Z"], 0.0)
ramp = nt.nodes.new("ShaderNodeValToRGB")
nt.links.new(math_node(nt, "POWER", elev, 0.55), ramp.inputs[0])
els = ramp.color_ramp.elements
els[0].position, els[0].color = 0.0, (0.20, 0.11, 0.10, 1)
els[1].position, els[1].color = 0.55, (0.006, 0.009, 0.025, 1)
mid = els.new(0.2)
mid.color = (0.045, 0.04, 0.075, 1)
# Clouds: stretched noise, brighter near the horizon (lit by the city).
cmap = node(nt, "ShaderNodeMapping")
cmap.inputs["Scale"].default_value = (1.2, 1.2, 9.0)
nt.links.new(coord.outputs["Generated"], cmap.inputs["Vector"])
cloud = node(nt, "ShaderNodeTexNoise", Scale=2.5, Detail=8.0, Roughness=0.62)
nt.links.new(cmap.outputs["Vector"], cloud.inputs["Vector"])
cramp = nt.nodes.new("ShaderNodeValToRGB")
nt.links.new(cloud.outputs["Fac"], cramp.inputs[0])
cramp.color_ramp.elements[0].position = 0.52
cramp.color_ramp.elements[1].position = 0.78
cmask = math_node(nt, "MULTIPLY", cramp.outputs["Color"], math_node(nt, "SUBTRACT", 1.0, math_node(nt, "MULTIPLY", elev, 2.2)))
cmask = math_node(nt, "MAXIMUM", cmask, 0.0)
sky = nt.nodes.new("ShaderNodeMix")
sky.data_type = "RGBA"
nt.links.new(math_node(nt, "MULTIPLY", cmask, 0.75), sky.inputs["Factor"])
nt.links.new(ramp.outputs["Color"], sky.inputs["A"])
sky.inputs["B"].default_value = (0.13, 0.08, 0.09, 1)
# Stars: sparse bright cells, fewer near the hazy horizon.
star = nt.nodes.new("ShaderNodeTexVoronoi")
star.inputs["Scale"].default_value = 420.0
nt.links.new(coord.outputs["Generated"], star.inputs["Vector"])
sparkle = math_node(nt, "LESS_THAN", star.outputs["Distance"], 0.045)
sparkle = math_node(nt, "MULTIPLY", sparkle, math_node(nt, "GREATER_THAN", star.outputs["Color"], 0.0))
sn = nt.nodes.new("ShaderNodeTexWhiteNoise")
nt.links.new(star.outputs["Position"], sn.inputs["Vector"])
sparkle = math_node(nt, "MULTIPLY", sparkle, math_node(nt, "GREATER_THAN", sn.outputs["Value"], 0.8))
sparkle = math_node(nt, "MULTIPLY", sparkle, math_node(nt, "MULTIPLY", math_node(nt, "SUBTRACT", 1.0, cmask), math_node(nt, "MINIMUM", math_node(nt, "MULTIPLY", elev, 8.0), 1.0)))
stars = nt.nodes.new("ShaderNodeMix")
stars.data_type = "RGBA"
nt.links.new(math_node(nt, "MULTIPLY", sparkle, 0.9), stars.inputs["Factor"])
nt.links.new(sky.outputs["Result"], stars.inputs["A"])
stars.inputs["B"].default_value = (0.8, 0.85, 1.0, 1)
# Moon halo around the moon direction (upper right).
moon_dir = Vector((0.45, 1.0, 0.16)).normalized()
dot = nt.nodes.new("ShaderNodeVectorMath")
dot.operation = "DOT_PRODUCT"
nt.links.new(coord.outputs["Generated"], dot.inputs[0])
dot.inputs[1].default_value = moon_dir
halo = math_node(nt, "POWER", math_node(nt, "MAXIMUM", dot.outputs["Value"], 0.0), 1400.0)
glow = nt.nodes.new("ShaderNodeMix")
glow.data_type = "RGBA"
glow.blend_type = "ADD"
nt.links.new(math_node(nt, "MULTIPLY", halo, 0.22), glow.inputs["Factor"])
nt.links.new(stars.outputs["Result"], glow.inputs["A"])
glow.inputs["B"].default_value = (0.55, 0.62, 0.8, 1)
bg = nt.nodes.new("ShaderNodeBackground")
bg.inputs["Strength"].default_value = 1.0
nt.links.new(glow.outputs["Result"], bg.inputs["Color"])
nt.links.new(bg.outputs[0], out.inputs["Surface"])
moon = ball(0, 0, 0, 1, light_mat("moon", (0.95, 0.93, 0.85), 3.0), far_objects)
moon.location = moon_dir * 9000 + Vector((0, 0, EYE))
moon.scale = (75, 75, 75)
# Moonlight and the city's glow from below on the near roofs.
sun = bpy.data.lights.new("moonlight", "SUN")
sun.energy, sun.color, sun.angle = 0.06, (0.6, 0.7, 1.0), math.radians(1)
o = bpy.data.objects.new("moonlight", sun)
scene.collection.objects.link(o)
o.rotation_euler = (-moon_dir).to_track_quat("-Z", "Y").to_euler()
street = bpy.data.lights.new("street_glow", "AREA")
street.energy, street.color, street.size = 3.5e5, (1.0, 0.55, 0.28), 600
o = bpy.data.objects.new("street_glow", street)
scene.collection.objects.link(o)
o.location = (0, 120, 1)
o.rotation_euler = (math.radians(180), 0, 0)

# ---------------------------------------------------------------- render
cam = bpy.data.cameras.new("view")
cam.sensor_fit = "HORIZONTAL"
cam.angle_x = 2 * math.atan(TAN_H)
cam.clip_end = 20000
co = bpy.data.objects.new("view", cam)
scene.collection.objects.link(co)
co.location = (0, 0, EYE)
co.rotation_euler = (math.radians(90), 0, 0)
scene.camera = co
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
scene.cycles.samples = SAMPLES
scene.cycles.use_denoising = True
scene.cycles.max_bounces = 6
scene.render.resolution_x, scene.render.resolution_y = WIDTH, HEIGHT
scene.view_settings.view_transform = "AgX"
scene.view_settings.look = "AgX - Medium High Contrast"
scene.view_settings.exposure = 0.0
os.makedirs(OUT, exist_ok=True)


def glare(on):
    """Bloom around the lights (compositor), for the opaque layer only."""
    scene.use_nodes = on
    if not on:
        return
    tree = scene.compositing_node_group if hasattr(scene, "compositing_node_group") else scene.node_tree
    if tree is None:
        tree = bpy.data.node_groups.new("glow", "CompositorNodeTree")
        scene.compositing_node_group = tree
    tree.nodes.clear()
    rl = tree.nodes.new("CompositorNodeRLayers")
    g = tree.nodes.new("CompositorNodeGlare")
    for key, value in (("glare_type", "FOG_GLOW"), ("quality", "HIGH")):
        if hasattr(g, key):
            setattr(g, key, value)
    for name, value in (("Type", "Fog Glow"), ("Quality", "High"), ("Threshold", 0.6), ("Size", 0.6), ("Strength", 0.55)):
        if name in g.inputs:
            try:
                g.inputs[name].default_value = value
            except (TypeError, ValueError):
                pass
    tree.links.new(rl.outputs["Image"], g.inputs["Image"])
    if hasattr(bpy.types, "NodeGroupOutput") and not any(n.type == "COMPOSITE" for n in tree.nodes):
        try:
            comp = tree.nodes.new("CompositorNodeComposite")
        except RuntimeError:
            comp = tree.nodes.new("NodeGroupOutput")
            tree.interface.new_socket("Image", in_out="OUTPUT", socket_type="NodeSocketColor")
        tree.links.new(g.outputs["Image"], comp.inputs[0])


def render(name, show, hide, transparent, fmt):
    for o in show:
        o.visible_camera = True
    for o in hide:
        o.visible_camera = False
    scene.render.film_transparent = transparent
    scene.render.image_settings.file_format = fmt
    if fmt == "JPEG":
        scene.render.image_settings.color_mode = "RGB"
        scene.render.image_settings.quality = 88
    else:
        scene.render.image_settings.color_mode = "RGBA"
        scene.render.image_settings.compression = 100
    glare(not transparent)
    scene.render.filepath = os.path.join(OUT, name)
    bpy.ops.render.render(write_still=True)
    print("rendered", scene.render.filepath)


render("far.jpg", far_objects, near_objects, False, "JPEG")
render("near.png", near_objects, far_objects, True, "PNG")
# Blender's Python has no Pillow: the palette step runs in the system Python.
try:
    import subprocess
    subprocess.run(["python", os.path.join(os.path.dirname(__file__), "shrink.py"), os.path.join(OUT, "near.png")], check=True)
except Exception as e:
    print("run tools/backdrop/shrink.py to shrink near.png:", e)
