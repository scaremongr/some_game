"""Five open rooms, authored in game metres. CC0 model IDs are in fetch_polyhaven.py.

The front 2 m form one continuous fighting lane. Dividers only occupy the
back of the apartment; crossing a doorway never requires destroying a prop.
"""
import math
import bpy
from mathutils import Vector


def build_apartment(r):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    r._materials.clear()
    box, surface, game = r.box, r.surface, r.game
    plaster = surface("warm_plaster", "plastered_wall_02", (0.76, 0.71, 0.61), 2.2, 0.8, 0.22)
    teal = surface("deep_teal", "painted_plaster_wall", (0.10, 0.26, 0.24), 2.0, 0.75, 0.18)
    sage = surface("sage", "painted_plaster_wall", (0.36, 0.43, 0.30), 2.0, 0.8, 0.18)
    indigo = surface("indigo", "painted_plaster_wall", (0.16, 0.23, 0.32), 2.2, 0.85, 0.16)
    clay = surface("terracotta", "painted_plaster_wall", (0.55, 0.30, 0.20), 2.2, 0.8, 0.2)
    oak = surface("walnut", "dark_wood", (0.75, 0.64, 0.48), 1.2, 0.44, 0.15)
    floor = surface("parquet", "herringbone_parquet", (0.74, 0.65, 0.48), 2.0, 0.42, 0.14)
    stone = surface("limestone", "floor_tiles_06", (0.75, 0.73, 0.62), 2.0, 0.55, 0.18)
    marble = surface("marble", "marble_01", (0.84, 0.83, 0.78), 1.0, 0.26, 0.08)
    linen = surface("linen", "rough_linen", (0.66, 0.52, 0.35), 0.65, 0.95, 0.22)
    trim = surface("ivory_trim", color=(0.69, 0.65, 0.54), rough=0.5)
    brass = surface("brushed_brass", color=(0.38, 0.25, 0.10), rough=0.3)
    dark = surface("black_metal", color=(0.025, 0.03, 0.025), rough=0.35)
    ceramic = surface("ceramic", color=(0.65, 0.69, 0.60), rough=0.25)
    glow = r.emissive("warm_glass", (1.0, 0.65, 0.3), 3.0)
    zones = [(-12, -8, "garden", sage), (-8, -3, "kitchen", clay),
             (-3, 3, "living", teal), (3, 8, "study", indigo), (8, 12, "bedroom", sage)]

    def solid(name, pos, size, mat, zone, category="shell", bevel=0.012, owner=-1):
        o = box(name, owner, pos, size, mat, bevel)
        o["bake_group"] = f"{zone}_{category}"
        return o

    def asset(name, x, z, fit, zone, owner=-1, y=0, rot=0, tris=3000, category="furniture", height=None, **kw):
        holder = r.place(name, owner, x, z, rot, fit, y=y, tris=tris, nometal=True, **kw)
        if height is not None:
            bpy.context.view_layer.update()
            points = [o.matrix_world @ Vector(c) for o in holder.children_recursive if o.type == "MESH" for c in o.bound_box]
            holder.scale.z *= height / (max(p.z for p in points) - min(p.z for p in points))
            bpy.context.view_layer.update()
        for o in holder.children_recursive:
            if o.type == "MESH":
                o["bake_group"] = f"{zone}_{category}"
        return holder

    def light(name, pos, target, energy, color, size=1.0):
        data = bpy.data.lights.new(name, "AREA")
        data.energy, data.color, data.shape, data.size = energy, color, "DISK", size
        obj = bpy.data.objects.new(name, data)
        bpy.context.collection.objects.link(obj)
        obj.location = game(*pos)
        obj.rotation_euler = (game(*target) - obj.location).to_track_quat("-Z", "Y").to_euler()

    def rug(x, width, depth, zone, color):
        # Woven fibre, contrasting nested borders and small diamonds.
        m = surface("rug_" + zone, "rough_linen", color, 0.45, 1.0, 0.25)
        nt, bsdf = m.node_tree, m.node_tree.nodes["Principled BSDF"]
        uv = nt.nodes.new("ShaderNodeTexCoord")
        sep = nt.nodes.new("ShaderNodeSeparateXYZ")
        nt.links.new(uv.outputs["UV"], sep.inputs[0])
        edges = []
        for axis in ("X", "Y"):
            sub = nt.nodes.new("ShaderNodeMath"); sub.operation = "SUBTRACT"; sub.inputs[0].default_value = 0.5
            nt.links.new(sep.outputs[axis], sub.inputs[1])
            ab = nt.nodes.new("ShaderNodeMath"); ab.operation = "ABSOLUTE"
            nt.links.new(sub.outputs[0], ab.inputs[0]); edges.append(ab)
        mx = nt.nodes.new("ShaderNodeMath"); mx.operation = "MAXIMUM"
        for i in range(2): nt.links.new(edges[i].outputs[0], mx.inputs[i])
        ramp = nt.nodes.new("ShaderNodeValToRGB")
        ramp.color_ramp.interpolation = "CONSTANT"
        ramp.color_ramp.elements.remove(ramp.color_ramp.elements[1])
        for i, (at, col) in enumerate([(0, color), (0.34, (0.12, 0.17, 0.16)),
                                      (0.37, (0.55, 0.38, 0.18)), (0.39, color),
                                      (0.44, (0.12, 0.17, 0.16)), (0.47, (0.55, 0.38, 0.18))]):
            el = ramp.color_ramp.elements[0] if i == 0 else ramp.color_ramp.elements.new(at)
            el.position, el.color = at, (*col, 1)
        nt.links.new(mx.outputs[0], ramp.inputs[0]); nt.links.new(ramp.outputs[0], bsdf.inputs["Base Color"])
        o = solid("rug", (x, 0.014, -0.2), (width, 0.022, depth), m, zone, "floor", 0.003)
        for loop in o.data.loops:
            v = o.data.vertices[loop.vertex_index].co
            o.data.uv_layers.active.data[loop.index].uv = (v.x / width + 0.5, v.y / depth + 0.5)

    def window(x, width, zone, owner=-1):
        # An open light aperture; thin glass is only used for the two owned panes.
        for dx in (-width / 2, 0, width / 2):
            solid("window_frame", (x + dx, 2.05, -3.30), (0.065, 2.35, 0.12), dark, zone)
        for y in (0.87, 1.72, 3.23):
            solid("window_frame", (x, y, -3.30), (width + 0.12, 0.065, 0.12), dark, zone)
        solid("window_sill", (x, 0.83, -3.19), (width + 0.3, 0.07, 0.38), marble, zone)
        if owner >= 0:
            for dx in (-width / 4, width / 4):
                solid("pane", (x + dx, 2.04, -3.34), (width / 2 - 0.07, 2.24, 0.008), r.glass(), zone, owner=owner, bevel=0)
        light("window_" + str(x), (x, 2.5, -3.6), (x - 0.7, 0.1, 0.8), 210, (0.68, 0.79, 1.0), 2.0)

    # Architecture: broad, seamless wall strips instead of a grid of bricks.
    for a, b, zone, paint in zones:
        cx = (a + b) / 2
        solid("foundation", (cx, -0.12, 0.5), (b - a, 0.20, 8.0), oak, zone, "floor", 0)
        solid("floor", (cx, -0.008, 0.5), (b - a, 0.025, 8.0), stone if zone in ("garden", "kitchen") else floor, zone, "floor", 0)
        if zone in ("garden", "living"):
            # A band of tall windows behind the furnishings.
            solid("backwall_low", (cx, 0.40, -3.42), (b - a, 0.8, 0.20), paint, zone, bevel=0)
            solid("backwall_top", (cx, 3.46, -3.42), (b - a, 0.5, 0.20), plaster, zone, bevel=0)
            for px in (a + 0.18, b - 0.18):
                solid("window_pier", (px, 1.98, -3.42), (0.36, 2.38, 0.20), paint, zone, bevel=0)
            if zone == "living":
                solid("window_pier", (0, 1.98, -3.42), (0.70, 2.38, 0.20), paint, zone, bevel=0)
                window(-1.57, 2.38, zone, 5); window(1.57, 2.38, zone, 6)
            else:
                window(cx, b - a - 0.76, zone)
        else:
            solid("backwall", (cx, 1.78, -3.42), (b - a, 3.56, 0.20), paint, zone, bevel=0)
        # Low panelling with inset frames and a continuous chair rail.
        solid("wainscot", (cx, 0.44, -3.285), (b - a - 0.08, 0.88, 0.045), oak if zone == "study" else paint, zone)
        for y in (0.09, 0.88, 3.48):
            solid("moulding", (cx, y, -3.24), (b - a, 0.075, 0.075), trim, zone)
        for px in [a + 0.4 + j * 0.65 for j in range(int((b - a - 0.5) / 0.65))]:
            solid("panel_stile", (px, 0.47, -3.24), (0.035, 0.68, 0.025), trim, zone, bevel=0.003)
        solid("ceiling_back", (cx, 3.64, -2.1), (b - a, 0.12, 2.9), plaster, zone, "ceiling", 0)
        solid("ceiling_beam", (cx, 3.51, -1.25), (b - a, 0.16, 0.18), oak, zone, "ceiling")
        light(zone + "_bounce", (cx, 3.15, 2.8), (cx, 0.8, -1.5), 150, (1.0, 0.87, 0.71), 4.0)
    for x in (-8, -3, 3, 8):
        zone = "kitchen" if x < 0 else "study"
        # These returns define rooms, with a clear opening at the fighting lane.
        solid("room_return", (x, 1.77, -2.45), (0.18, 3.54, 1.9), plaster, zone)
        solid("portal_lintel", (x, 3.35, -0.1), (0.23, 0.35, 3.0), oak, zone, "ceiling")
        solid("portal_trim", (x, 1.70, -1.47), (0.27, 3.38, 0.10), trim, zone)
        solid("threshold", (x, 0.009, 0), (0.08, 0.01, 3.0), brass, zone, "floor", 0)
    for x in (-12.1, 12.1):
        solid("exterior_side", (x, 1.8, -1.0), (0.2, 3.6, 5.0), plaster, "garden" if x < 0 else "bedroom")

    # LIVING ROOM: leather, walnut, books, ceramics and practical warm lights.
    rug(0, 5.15, 3.2, "living", (0.34, 0.24, 0.16))
    asset("sofa_02", 0, -2.02, ("w", 2.65), "living", tris=6000, category="sofa")
    asset("mid_century_lounge_chair", 2.1, -1.75, ("h", 1.02), "living", rot=-22, tris=4500)
    asset("modern_coffee_table_01", -0.5, -0.95, ("d", 1.45), "living", owner=7, tris=3200, height=0.45, rot=90)
    asset("ceramic_vase_03", -0.82, -1.0, ("h", 0.26), "living", owner=7, y=0.45, tris=1400)
    asset("book_encyclopedia_set_01", 0.0, -1.02, ("w", 0.30), "living", owner=7, y=0.45, tris=900)
    asset("side_table_01", 2.4, -0.9, ("h", 0.58), "living", owner=8, tris=2000)
    asset("brass_candleholders", 2.4, -0.9, ("h", 0.34), "living", owner=8, y=0.58, tris=1500)
    asset("potted_plant_02", -2.25, -1.20, ("h", 1.52), "living", owner=15, tris=5500)
    solid("media_console", (-2.10, 0.40, -2.73), (1.25, 0.8, 0.56), oak, "living", "furniture")
    asset("Television_01", -2.05, -2.70, ("w", 0.82), "living", y=0.80, tris=4000)
    asset("Chandelier_01", 0, -0.8, ("w", 1.20), "living", y=2.50, tris=4500)
    for x, owner in ((-1.8, 13), (1.8, 14)):
        solid("brass_sconce", (x, 2.85, -2.94), (0.13, 0.35, 0.22), brass, "living", owner=owner)
        solid("sconce_glass", (x, 2.9, -2.78), (0.18, 0.22, 0.15), glow, "living", owner=owner)
        light("living_sconce" + str(x), (x, 2.82, -2.60), (x, 0.3, -1.0), 65, (1.0, 0.61, 0.31), 0.35)
    light("living_pendant", (0, 2.72, -0.85), (0, 0, -0.8), 140, (1.0, 0.69, 0.40), 0.8)
    # Pleated curtains have actual depth and catch grazing light.
    for x in (-2.65, 2.65):
        for j in range(6):
            solid("curtain_fold", (x + (j - 2.5) * 0.075, 2.04, -3.04 + (j % 2) * 0.07), (0.10, 2.70, 0.12), linen, "living", "textile", 0.035)

    # KITCHEN: inset cabinet doors, worktop appliances, cookware, dining table.
    for x in (-7.45, -6.70, -5.95):
        solid("cabinet", (x, 0.43, -2.94), (0.73, 0.86, 0.65), teal, "kitchen", "fixtures")
        solid("cabinet_inset", (x, 0.44, -2.595), (0.60, 0.68, 0.025), sage, "kitchen", "fixtures")
        solid("cabinet_handle", (x + 0.23, 0.65, -2.55), (0.035, 0.18, 0.04), brass, "kitchen", "fixtures")
    solid("worktop", (-6.70, 0.90, -2.93), (2.30, 0.075, 0.73), marble, "kitchen", "fixtures")
    for row in range(4):
        for col in range(15):
            solid("backsplash_tile", (-7.7 + col * 0.19, 1.04 + row * 0.17, -3.28), (0.184, 0.164, 0.022), ceramic, "kitchen", "fixtures", 0.002)
    asset("electric_stove", -5.0, -2.85, ("h", 0.93), "kitchen", tris=4500)
    asset("vintage_microwave", -7.1, -2.91, ("w", 0.65), "kitchen", y=0.94, tris=2800)
    asset("vintage_electric_kettle", -6.25, -2.82, ("h", 0.32), "kitchen", y=0.94, tris=2400)
    asset("modern_wooden_cabinet", -3.6, -2.8, ("w", 1.15), "kitchen", owner=11, tris=3500)
    asset("round_wooden_table_01", -4.9, -1.15, ("h", 0.75), "kitchen", tris=2800)
    asset("wooden_bowl_01", -5.0, -1.15, ("w", 0.32), "kitchen", y=0.76, tris=1000)
    asset("jug_01", -4.7, -1.2, ("h", 0.28), "kitchen", y=0.76, tris=1600)
    asset("painted_wooden_chair_02", -5.85, -1.18, ("h", 0.93), "kitchen", owner=9, rot=32, tris=2000)
    asset("hanging_industrial_lamp", -5.5, -1.45, ("h", 0.75), "kitchen", y=2.63, tris=1800)
    solid("open_shelf", (-6.8, 2.11, -3.07), (2.12, 0.07, 0.44), oak, "kitchen", "fixtures")
    for j in range(7):
        solid("canister", (-7.55 + j * 0.22, 2.29, -3.03), (0.15, 0.27 + (j % 2) * 0.10, 0.15), ceramic if j % 2 else clay, "kitchen", "fixtures", 0.045)
    light("kitchen_pendant", (-5.5, 2.6, -1.5), (-5.5, 0, -1.4), 230, (1.0, 0.73, 0.47), 0.65)
    light("worktop_strip", (-6.8, 2.08, -2.85), (-6.8, 0.9, -2.8), 65, (1.0, 0.82, 0.57), 1.5)

    # GARDEN: sunlit foliage, chess table and mismatched chairs.
    asset("anthurium_botany_01", -11.25, -2.55, ("h", 1.70), "garden", tris=6500)
    asset("potted_plant_04", -8.75, -2.2, ("h", 1.45), "garden", tris=4000)
    asset("side_table_tall_01", -10.0, -1.4, ("h", 0.72), "garden", owner=16, tris=2000)
    asset("chess_set", -10.0, -1.4, ("w", 0.53), "garden", owner=16, y=0.73, tris=3500)
    asset("painted_wooden_chair_01", -10.95, -1.25, ("h", 0.94), "garden", rot=25, tris=2000)
    asset("wooden_bookshelf_worn", -8.6, -2.8, ("h", 1.9), "garden", owner=18, tris=2000)
    asset("ceramic_vase_03", -8.6, -2.68, ("h", 0.3), "garden", owner=18, y=0.90, tris=1000)
    rug(-10, 3.4, 2.3, "garden", (0.37, 0.40, 0.28))

    # STUDY: book-lined wall, laptop, desk lamp and music equipment.
    for x in (3.65, 4.7, 5.75, 6.8):
        solid("bookcase_back", (x, 1.48, -3.22), (1.0, 2.85, 0.06), oak, "study", "shelves")
        for dx in (-0.48, 0.48):
            solid("bookcase_upright", (x + dx, 1.45, -3.02), (0.065, 2.9, 0.42), oak, "study", "shelves")
        for y in (0.12, 0.74, 1.35, 1.96, 2.58):
            solid("bookcase_shelf", (x, y, -3.01), (1.0, 0.06, 0.45), oak, "study", "shelves")
            for j in range(8):
                bookmat = (teal, clay, trim, indigo, sage)[(j + int(x * 7 + y * 5)) % 5]
                solid("book_spine", (x - 0.39 + j * 0.105, y + 0.23, -2.97), (0.080, 0.32 + (j % 3) * 0.04, 0.25), bookmat, "study", "shelves", 0.003)
                solid("book_gilt", (x - 0.39 + j * 0.105, y + 0.27, -2.838), (0.060, 0.018, 0.005), brass, "study", "shelves", 0)
    asset("WoodenTable_02", 5.1, -1.35, ("w", 1.7), "study", owner=12, tris=2300, height=0.78)
    asset("classic_laptop", 5.05, -1.35, ("w", 0.43), "study", owner=12, y=0.79, tris=2200)
    asset("desk_lamp_arm_01", 4.55, -1.4, ("h", 0.57), "study", owner=12, y=0.79, tris=2200)
    asset("mid_century_lounge_chair", 6.6, -1.30, ("h", 0.96), "study", owner=10, rot=-20, tris=3800)
    asset("boombox", 6.8, -2.98, ("w", 0.56), "study", y=1.39, tris=2200)
    asset("vintage_grandfather_clock_01", 7.50, -2.70, ("h", 2.45), "study", tris=3000)
    light("desk_light", (4.58, 1.34, -1.35), (5.05, 0.79, -1.3), 32, (1.0, 0.67, 0.3), 0.24)
    light("study_ceiling", (5.4, 3.2, -1.5), (5.4, 0.6, -2), 260, (1.0, 0.77, 0.50), 1.6)
    rug(5.5, 4.0, 2.5, "study", (0.25, 0.20, 0.16))

    # BEDROOM: upholstered day bed, nightstand, luggage and framed art.
    asset("vintage_day_bed", 10.05, -2.0, ("w", 2.6), "bedroom", tris=6200)
    asset("ClassicNightstand_01", 8.72, -1.4, ("h", 0.62), "bedroom", owner=17, tris=2600)
    asset("brass_candleholders", 8.72, -1.4, ("h", 0.34), "bedroom", owner=17, y=0.63, tris=1600)
    asset("modern_wooden_cabinet", 11.25, -2.6, ("w", 1.30), "bedroom", owner=19, tris=3500)
    asset("vintage_suitcase", 11.0, -1.15, ("w", 0.66), "bedroom", tris=1800)
    asset("fancy_picture_frame_01", 9.65, -3.29, ("w", 1.15), "bedroom", y=1.75, tris=2200)
    asset("hanging_picture_frame_03", 10.9, -3.29, ("w", 0.54), "bedroom", y=2.05, tris=1600)
    light("bedroom_lamp", (9.0, 2.6, -1.6), (10.0, 0.7, -2.0), 200, (1.0, 0.64, 0.42), 1.0)
    rug(10, 3.3, 2.4, "bedroom", (0.38, 0.31, 0.25))

    # City beyond the tall windows gives real depth and broken-up evening light.
    facade = surface("city_brick", "brick_wall_02", (0.38, 0.40, 0.43), 2.8, 0.9, 0.2)
    cityglass = surface("city_glass", color=(0.075, 0.115, 0.15), rough=0.2)
    for j in range(9):
        x = -18 + j * 4.5
        z = -13 - (j % 3) * 2.5
        height = 7 + (j % 4) * 2
        solid("facade", (x, height / 2 - 3, z), (4.3, height, 0.5), facade, "city", "outside", 0)
        for col in range(3):
            for row in range(4):
                solid("city_window", (x - 1.35 + col * 1.30, -1 + row * 1.75, z + 0.28), (0.85, 1.15, 0.03), glow if (j + row + col) % 5 == 0 else cityglass, "city", "outside", 0)
    world = bpy.data.worlds.new("evening_sky"); bpy.context.scene.world = world
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.42, 0.54, 0.75, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.32
    sun = bpy.data.lights.new("late_sun", "SUN"); sun.energy = 3.0; sun.color = (1.0, 0.70, 0.43); sun.angle = math.radians(7)
    obj = bpy.data.objects.new("late_sun", sun); bpy.context.collection.objects.link(obj)
    obj.rotation_euler = game(-0.55, -0.65, 0.7).normalized().to_track_quat("-Z", "Y").to_euler()
