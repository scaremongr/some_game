//! Cutaway room: every visible furnishing and architectural panel has a server ID.
//! Render in batches of 64 rigid parts; broken parts reuse the same GPU mesh.
//!
//! The room is the baked apartment (assets/room.glb, tools/room/bake_room.py)
//! once it has loaded: fixed geometry plus breakable pieces named after their
//! room object. Until then (or without the file) a box room stands in.
use crate::engine::{
    gltf::{self, MaterialData, TextureData},
    graphics::Graphics,
    math3::*,
    mesh3::{MeshData, Vertex3},
    physics::Fragment,
    render3d::{Camera, Lighting, SkinnedMesh},
};
use arena_combat::{room::LAYOUT, Match};
use std::collections::BTreeMap;
const BATCH: usize = 64;
/// Scale of the baked light (the textures already carry the tone).
const EXPOSURE: f32 = 1.0;
/// Opacity of window glass.
const GLASS: f32 = 0.12;
struct Part {
    owner: usize,
    position: Vec3,
    size: Vec3,
    color: Vec3,
    fragment: Option<Fragment>,
    /// Knocked about by a fighter after settling; simulated on its own.
    kicked: bool,
    /// A piece of the baked room, drawn from its own mesh at scale 1
    /// (box parts are unit cubes scaled to `size`).
    piece: bool,
}
/// Room geometry on the CPU, waiting for the GPU.
struct RoomData {
    textures: Vec<TextureData>,
    materials: Vec<MaterialData>,
    fixed: Vec<(MeshData, Vec3)>,
    /// Pieces grouped by owner: mesh, part indices (joint order), glass.
    batches: Vec<(MeshData, Vec<usize>, bool)>,
}
struct Room {
    data: Option<RoomData>,
    fixed: Vec<(SkinnedMesh, Vec3)>,
    batches: Vec<(SkinnedMesh, Vec<usize>, bool)>,
}
pub struct ArenaProps {
    meshes: Vec<SkinnedMesh>,
    room: Option<Room>,
    parts: Vec<Part>,
    broken: [u32; 22],
    age: [u32; 22],
    /// Side-wall wobble after a body slams into it (1 → 0).
    shake: [f32; 2],
    wall_impacts: [u32; 2],
    /// Newly broken objects: centre and layout kind (10 for side walls).
    breaks: Vec<(Vec3, u32)>,
}
impl ArenaProps {
    pub fn new() -> Self {
        let mut this = Self {
            meshes: vec![],
            room: None,
            parts: vec![],
            broken: [0; 22],
            age: [0; 22],
            shake: [0.0; 2],
            wall_impacts: [0; 2],
            breaks: Vec::new(),
        };
        let stone = vec3(0.24, 0.29, 0.32);
        let metal = vec3(0.08, 0.11, 0.14);
        let wood = vec3(0.34, 0.17, 0.095);
        let trim = vec3(0.59, 0.37, 0.18);
        for (id, d) in LAYOUT.iter().enumerate() {
            let x = d.x as f32 / 1000.0;
            let z = d.z as f32 / 1000.0;
            match d.kind {
                0 => {
                    // Masonry, pilasters, ceiling cross-beams and cornice share a panel.
                    for row in 0..4 {
                        for col in 0..3 {
                            this.add(
                                id,
                                vec3(x + (col as f32 - 1.0) * 0.58, 0.40 + row as f32 * 0.76, z),
                                vec3(0.565, 0.73, 0.18),
                                stone * (0.82 + row as f32 * 0.07),
                            );
                        }
                    }
                    this.add(
                        id,
                        vec3(x - 0.84, 1.65, z + 0.13),
                        vec3(0.075, 3.3, 0.19),
                        metal,
                    );
                    this.add(id, vec3(x, 3.25, z + 0.20), vec3(1.8, 0.18, 0.42), trim);
                    this.add(id, vec3(x, 3.32, -0.8), vec3(0.12, 0.13, 2.0), metal);
                }
                1 => {
                    for row in 0..3 {
                        for col in 0..3 {
                            this.add(
                                id,
                                vec3(
                                    x + (col as f32 - 1.0) * 0.38,
                                    1.6 + row as f32 * 0.40,
                                    z + 0.03,
                                ),
                                vec3(0.35, 0.37, 0.035),
                                vec3(0.16 + col as f32 * 0.055, 0.43, 0.52),
                            );
                        }
                    }
                    for dx in [-0.60, 0.60] {
                        this.add(
                            id,
                            vec3(x + dx, 2.0, z + 0.07),
                            vec3(0.045, 1.3, 0.07),
                            trim,
                        );
                    }
                    for y in [1.36, 2.64] {
                        this.add(id, vec3(x, y, z + 0.07), vec3(1.22, 0.045, 0.07), trim);
                    }
                }
                2 => {
                    for i in 0..3 {
                        this.add(
                            id,
                            vec3(x + (i as f32 - 1.0) * 0.39, 0.82, z),
                            vec3(0.38, 0.12, 0.70),
                            wood * (0.9 + i as f32 * 0.08),
                        );
                    }
                    for dx in [-0.46, 0.46] {
                        for dz in [-0.25, 0.25] {
                            this.add(
                                id,
                                vec3(x + dx, 0.38, z + dz),
                                vec3(0.075, 0.76, 0.075),
                                metal,
                            );
                        }
                    }
                    this.add(
                        id,
                        vec3(x - 0.25, 0.96, z),
                        vec3(0.09, 0.17, 0.09),
                        vec3(0.30, 0.55, 0.45),
                    );
                    this.add(
                        id,
                        vec3(x + 0.15, 0.92, z - 0.1),
                        vec3(0.16, 0.07, 0.12),
                        trim,
                    );
                }
                3 => {
                    this.add(id, vec3(x, 0.48, z), vec3(0.38, 0.11, 0.38), wood);
                    for dx in [-0.14, 0.14] {
                        for dz in [-0.14, 0.14] {
                            this.add(
                                id,
                                vec3(x + dx, 0.23, z + dz),
                                vec3(0.05, 0.46, 0.05),
                                metal,
                            );
                        }
                    }
                }
                4 => {
                    for dx in [-0.42, 0.42] {
                        this.add(id, vec3(x + dx, 1.0, z), vec3(0.10, 2.0, 0.45), metal);
                    }
                    for y in [0.08, 0.65, 1.25, 1.95] {
                        this.add(id, vec3(x, y, z), vec3(0.84, 0.09, 0.45), wood);
                    }
                    for j in 0..6 {
                        this.add(
                            id,
                            vec3(
                                x + (j % 3) as f32 * 0.23 - 0.23,
                                0.82 + (j / 3) as f32 * 0.60,
                                z,
                            ),
                            vec3(0.17, 0.28, 0.22),
                            vec3(0.40, 0.33, 0.21),
                        );
                    }
                }
                5 => {
                    this.add(id, vec3(x, 3.05, z), vec3(0.025, 0.8, 0.025), metal);
                    this.add(id, vec3(x, 2.64, z), vec3(0.62, 0.10, 0.35), metal);
                    this.add(
                        id,
                        vec3(x, 2.57, z),
                        vec3(0.48, 0.035, 0.26),
                        vec3(1.0, 0.69, 0.32),
                    );
                }
                6 => {
                    this.add(
                        id,
                        vec3(x, 0.25, z),
                        vec3(0.48, 0.50, 0.44),
                        vec3(0.46, 0.26, 0.18),
                    );
                    for i in 0..5 {
                        this.add(
                            id,
                            vec3(
                                x + (i as f32 - 2.0) * 0.09,
                                0.65 + (i % 2) as f32 * 0.16,
                                z + (i % 2) as f32 * 0.10,
                            ),
                            vec3(0.13, 0.60, 0.15),
                            vec3(0.12, 0.29, 0.19),
                        );
                    }
                }
                7 => {
                    // Breakable floor finish; foundation below keeps the arena playable.
                    for row in 0..6 {
                        for col in 0..6 {
                            this.add(
                                id,
                                vec3(
                                    x + (col as f32 - 2.5) * 0.70,
                                    -0.018,
                                    (row as f32 - 2.5) * 0.66,
                                ),
                                vec3(0.685, 0.04, 0.645),
                                if (row + col) % 2 == 0 {
                                    vec3(0.23, 0.26, 0.27)
                                } else {
                                    vec3(0.17, 0.20, 0.22)
                                },
                            );
                        }
                    }
                }
                8 => {
                    for row in 0..4 {
                        for col in 0..4 {
                            this.add(
                                id,
                                vec3(x, 0.40 + row as f32 * 0.77, z + (col as f32 - 1.5) * 0.77),
                                vec3(0.18, 0.74, 0.74),
                                stone,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        for side in 0..2 {
            for i in 0..12 {
                this.add(
                    20 + side,
                    vec3(
                        if side == 0 { -3.34 } else { 3.34 },
                        0.24 + (i / 3) as f32 * 0.48,
                        (i % 3) as f32 * 0.45 - 0.45,
                    ),
                    vec3(0.16, 0.45, 0.42),
                    wood * (0.8 + (i % 3) as f32 * 0.12),
                );
            }
        }
        // Fixed shell around the room (owner 22, never breaks): broken walls
        // open onto a loading bay instead of empty space.
        let shell = vec3(0.13, 0.155, 0.18);
        let slab = vec3(0.15, 0.17, 0.19);
        let lamp = vec3(1.0, 0.78, 0.45);
        for side in [-1.0f32, 1.0] {
            for k in 0..3 {
                let x = side * (4.55 + k as f32 * 1.5);
                this.add(22, vec3(x, -0.04, -0.2), vec3(1.48, 0.06, 3.9), slab * (0.9 + k as f32 * 0.05));
            }
            this.add(22, vec3(side * 7.0, 1.9, -0.4), vec3(0.3, 3.9, 3.6), shell * 0.9);
            this.add(22, vec3(side * 6.0, 3.55, -0.4), vec3(2.2, 0.16, 3.6), shell * 0.7);
        }
        for k in 0..3 {
            let x = -6.2 + k as f32 * 6.2;
            this.add(22, vec3(x, -0.05, 3.9), vec3(6.18, 0.06, 4.0), slab * (0.82 + k as f32 * 0.04));
        }
        for k in 0..8 {
            let x = -7.0 + k as f32 * 2.0;
            this.add(22, vec3(x, 1.9, -2.45), vec3(1.98, 3.9, 0.3), shell * (0.85 + (k % 2) as f32 * 0.08));
            if k % 2 == 1 {
                this.add(22, vec3(x, 2.35, -2.28), vec3(1.1, 0.5, 0.05), lamp * 0.55);
            }
        }
        this
    }
    fn add(&mut self, owner: usize, position: Vec3, size: Vec3, color: Vec3) {
        self.parts.push(Part {
            owner: if owner < 5 { 22 } else { owner },
            position,
            size,
            color,
            fragment: None,
            kicked: false,
            piece: false,
        });
    }
    /// Replaces the box room with the baked apartment. Nodes named
    /// `oNN_kkk` / `glassNN_kkk` are pieces of room object NN (20 and 21 are
    /// the arena walls); everything else is fixed.
    pub fn set_room(&mut self, bytes: &[u8]) -> Result<usize, String> {
        let scene = gltf::load_scene(bytes)?;
        let mut parts = Vec::new();
        let mut fixed = Vec::new();
        let mut groups: BTreeMap<(usize, bool), Vec<(usize, MeshData)>> = BTreeMap::new();
        for node in scene.nodes {
            let (glass, rest) = match (node.name.strip_prefix("glass"), node.name.strip_prefix('o')) {
                (Some(rest), _) => (true, rest),
                (None, Some(rest)) => (false, rest),
                _ => (false, ""),
            };
            let owner = rest.get(0..2).and_then(|s| s.parse::<usize>().ok()).filter(|&o| o < 22);
            let Some(owner) = owner.filter(|&o| o >= 5) else {
                fixed.push((node.mesh, node.position));
                continue;
            };
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &node.mesh.verts {
                for k in 0..3 {
                    lo[k] = lo[k].min(v.pos[k]);
                    hi[k] = hi[k].max(v.pos[k]);
                }
            }
            let extent = |k: usize| (hi[k] - lo[k]).max(0.01);
            // Size is only used once broken: panes break into shards.
            let shard = if glass { 0.3 } else { 1.0 };
            parts.push(Part {
                owner,
                position: node.position,
                size: vec3(extent(0), extent(1), extent(2)) * shard,
                color: Vec3::ZERO,
                fragment: None,
                kicked: false,
                piece: true,
            });
            groups.entry((owner, glass)).or_default().push((parts.len() - 1, node.mesh));
        }
        let mut batches = Vec::new();
        for ((_, glass), list) in groups {
            for chunk in list.chunks(BATCH) {
                let mut mesh = MeshData::default();
                let mut ids = Vec::new();
                for (joint, (index, piece)) in chunk.iter().enumerate() {
                    append_piece(&mut mesh, piece, joint);
                    ids.push(*index);
                }
                batches.push((mesh, ids, glass));
            }
        }
        let count = parts.len();
        self.parts = parts;
        self.meshes.clear();
        self.reset();
        self.room = Some(Room {
            data: Some(RoomData {
                textures: scene.textures,
                materials: scene.materials,
                fixed,
                batches,
            }),
            fixed: Vec::new(),
            batches: Vec::new(),
        });
        Ok(count)
    }
    pub fn has_room(&self) -> bool {
        self.room.is_some()
    }
    fn part_matrix(&self, p: &Part, glass: bool) -> Mat4 {
        match (&p.fragment, p.piece) {
            // Broken panes fly as small shards and are swept away once settled.
            (Some(f), true) if glass => {
                let k = if f.age >= 360 { 0.0 } else { 0.3 };
                Mat4::from_trs(f.position, f.rotation, Vec3::ONE * k)
            }
            (Some(f), true) => Mat4::from_trs(f.position, f.rotation, Vec3::ONE),
            (Some(f), false) => f.matrix(),
            (None, _) => {
                let wobble = match p.owner {
                    20 | 21 => {
                        let k = self.shake[p.owner - 20];
                        let lean = p.position.y * 0.035;
                        vec3((k * 38.0).sin() * k * (0.03 + lean), 0.0, 0.0)
                    }
                    _ => Vec3::ZERO,
                };
                let size = if p.piece { Vec3::ONE } else { p.size };
                Mat4::from_trs(p.position + wobble, Quat::IDENTITY, size)
            }
        }
    }
    pub fn reset(&mut self) {
        for p in &mut self.parts {
            p.fragment = None;
            p.kicked = false;
        }
        self.broken = [0; 22];
        self.age = [0; 22];
        self.shake = [0.0; 2];
        self.wall_impacts = [0; 2];
    }
    pub fn take_breaks(&mut self) -> Vec<(Vec3, u32)> {
        std::mem::take(&mut self.breaks)
    }
    /// Advances debris to `state.tick`. `fighters` are body x positions: loose
    /// chunks in the fighting lane get kicked out of the way.
    pub fn update(&mut self, state: &Match, fighters: [f32; 2]) {
        for wall in 0..2 {
            let w = &state.walls[wall];
            if w.impacts > self.wall_impacts[wall] && w.hp > 0 {
                self.shake[wall] = 1.0;
            }
            self.wall_impacts[wall] = w.impacts;
            self.shake[wall] = (self.shake[wall] - 1.0 / 26.0).max(0.0);
        }
        for owner in 0..22 {
            let (hp, tick, impulse) = if owner < 20 {
                let s = &state.objects[owner];
                (s.hp, s.broken_tick, s.impulse)
            } else {
                let s = &state.walls[owner - 20];
                (
                    s.hp,
                    s.broken_tick,
                    s.impulse * if owner == 20 { -1 } else { 1 },
                )
            };
            if hp > 0 {
                continue;
            }
            if tick != self.broken[owner] {
                self.broken[owner] = tick;
                self.age[owner] = 0;
                let (sum, count) = self
                    .parts
                    .iter()
                    .filter(|p| p.owner == owner)
                    .fold((Vec3::ZERO, 0.0), |(s, n), p| (s + p.position, n + 1.0));
                if count > 0.0 {
                    let kind = if owner < 20 { LAYOUT[owner].kind } else { 10 };
                    self.breaks.push((sum * (1.0 / count), kind));
                }
                for (i, p) in self
                    .parts
                    .iter_mut()
                    .enumerate()
                    .filter(|(_, p)| p.owner == owner)
                {
                    let hash = tick
                        .wrapping_mul(1664525)
                        .wrapping_add((i as u32).wrapping_mul(1013904223))
                        .rotate_left(i as u32);
                    let r = (hash % 1000) as f32 / 1000.0;
                    let floor = owner == 16 || owner == 17;
                    // Big pieces of the baked room are heavy: they topple and
                    // slide instead of flying across the room.
                    let heft = if p.piece { heft(p.size) } else { 1.0 };
                    // Floor tiles crack in place; everything else breaks into
                    // chunks small enough not to bury the fighting lane.
                    p.fragment = Some(if floor {
                        Fragment {
                            position: p.position,
                            velocity: vec3(impulse as f32 / 900.0 * r, 0.15 + r * 0.25, 0.0),
                            rotation: Quat::IDENTITY,
                            spin: vec3(r * 0.8 - 0.4, 0.0, 0.5 - r),
                            size: p.size * 0.94,
                            age: 0,
                        }
                    } else {
                        Fragment {
                            position: p.position,
                            velocity: vec3(
                                impulse as f32 / 90.0 * (0.3 + r),
                                0.8 + r * 1.6,
                                (r - 0.5) * 1.2,
                            ) * heft,
                            rotation: Quat::IDENTITY,
                            spin: vec3(r * 4.0, 1.0 - r * 3.0, 2.0) * heft.sqrt(),
                            size: if p.piece {
                                p.size
                            } else {
                                vec3(p.size.x.min(0.4), p.size.y.min(0.4), p.size.z.min(0.4)) * 0.6
                            },
                            age: 0,
                        }
                    });
                }
            }
            let target = state.tick.saturating_sub(tick).min(360);
            let target = if state.phase >= 2 {
                target.max((self.age[owner] + 1).min(360))
            } else {
                target
            };
            while self.age[owner] < target {
                for p in self.parts.iter_mut().filter(|p| p.owner == owner && !p.kicked) {
                    if let Some(f) = &mut p.fragment {
                        f.step();
                        let big = p.piece && heft(p.size) < 0.5;
                        match owner {
                            // Big pieces stay against the back wall, out of the lane.
                            0..=15 if big => f.position.z = f.position.z.min(-1.0),
                            0..=15 => f.position.z = f.position.z.min(-0.55),
                            // Side partitions fall outward into the hallway.
                            18 | 19 if p.piece => {
                                let side = if owner == 18 { -1.0 } else { 1.0 };
                                f.position.x = side * (f.position.x * side).max(4.45);
                                f.position.z = f.position.z.min(0.8);
                            }
                            20 | 21 => f.position.z = f.position.z.min(0.8),
                            _ => {}
                        }
                    }
                }
                self.age[owner] += 1;
            }
        }
        // Chunks lying in the lane are kicked aside instead of clipping feet.
        for (i, p) in self.parts.iter_mut().enumerate() {
            if p.owner == 16 || p.owner == 17 {
                continue;
            }
            let Some(f) = &mut p.fragment else { continue };
            for &x in &fighters {
                let dx = f.position.x - x;
                if dx.abs() < 0.3 && f.position.z > -0.42 && f.position.y < 0.7 {
                    let side = if dx >= 0.0 { 1.0 } else { -1.0 };
                    let r = ((i as u32).wrapping_mul(2654435761) % 1000) as f32 / 1000.0;
                    let heft = if p.piece { heft(p.size) } else { 1.0 };
                    f.position.x = x + side * 0.3;
                    f.velocity = vec3(side * (1.4 + r), 0.9 + r * 0.6, (r - 0.5) * 0.8) * heft;
                    f.spin = vec3(r * 5.0, 2.0 - r * 3.0, 3.0) * heft.sqrt();
                    f.age = f.age.min(290);
                    p.kicked = true;
                }
            }
            if p.kicked {
                f.step();
                if f.age >= 360 {
                    p.kicked = false;
                }
            }
        }
    }
    pub fn draw(&mut self, g: &mut Graphics, _state: &Match, camera: &Camera, lighting: &Lighting) {
        if let Some(room) = &mut self.room {
            if let Some(data) = room.data.take() {
                let textures = g.upload_textures(&data.textures);
                room.fixed = data
                    .fixed
                    .iter()
                    .map(|(mesh, at)| (g.upload_shared(mesh, &data.materials, &textures), *at))
                    .collect();
                room.batches = data
                    .batches
                    .iter()
                    .map(|(mesh, ids, glass)| (g.upload_shared(mesh, &data.materials, &textures), ids.clone(), *glass))
                    .collect();
            }
            let room = self.room.as_ref().unwrap();
            for (mesh, at) in &room.fixed {
                g.draw_baked(mesh, &[Mat4::translation(*at)], camera, EXPOSURE, None, 0.0);
            }
            // Opaque pieces first, glass blended over them.
            for pass_glass in [false, true] {
                for (mesh, ids, glass) in &room.batches {
                    if *glass != pass_glass {
                        continue;
                    }
                    let matrices: Vec<Mat4> = ids.iter().map(|&i| self.part_matrix(&self.parts[i], *glass)).collect();
                    // Broken pieces turn faces to the light that were baked in
                    // shadow (cabinet backs, undersides): lift their blacks.
                    let broken = ids.first().is_some_and(|&i| self.parts[i].fragment.is_some());
                    g.draw_baked(mesh, &matrices, camera, EXPOSURE, glass.then_some(GLASS), if broken { 1.0 } else { 0.0 });
                }
            }
            return;
        }
        if self.meshes.is_empty() {
            for batch in self.parts.chunks(BATCH) {
                let mut mesh = MeshData::default();
                for (i, p) in batch.iter().enumerate() {
                    cube(&mut mesh, i, p.color);
                }
                self.meshes.push(g.upload_skinned(&mesh, &[], &[]));
            }
        }
        for (batch, mesh) in self.parts.chunks(BATCH).zip(&self.meshes) {
            let shake = self.shake;
            let matrices: Vec<_> = batch
                .iter()
                .map(|p| {
                    p.fragment.as_ref().map_or_else(
                        || {
                            let wobble = match p.owner {
                                20 | 21 => {
                                    let k = shake[p.owner - 20];
                                    let lean = p.position.y * 0.035;
                                    vec3((k * 38.0).sin() * k * (0.03 + lean), 0.0, 0.0)
                                }
                                _ => Vec3::ZERO,
                            };
                            Mat4::from_trs(p.position + wobble, Quat::IDENTITY, p.size)
                        },
                        |f| f.matrix(),
                    )
                })
                .collect();
            g.draw_skinned(mesh, &matrices, Mat4::IDENTITY, camera, lighting, 0.08);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconnect_reconstructs_the_same_room_debris() {
        let mut state = Match::new(4);
        state.phase = 1;
        for object in &mut state.objects {
            object.hp = 0;
            object.broken_tick = 10;
            object.impulse = 100;
        }
        let mut live = ArenaProps::new();
        for tick in 10..130 {
            state.tick = tick;
            live.update(&state, [-20.0, 20.0]);
        }
        let mut resumed = ArenaProps::new();
        resumed.update(&state, [-20.0, 20.0]);
        assert_eq!(live.parts.len(), resumed.parts.len());
        for (a, b) in live.parts.iter().zip(&resumed.parts) {
            if (5..20).contains(&a.owner) {
                assert_eq!(
                    a.fragment.as_ref().unwrap().matrix().0,
                    b.fragment.as_ref().unwrap().matrix().0
                );
            }
        }
    }
    #[test]
    fn baked_room_has_pieces_for_every_breakable_object() {
        let Ok(bytes) = std::fs::read("assets/room.glb") else { return };
        let mut room = ArenaProps::new();
        let pieces = room.set_room(&bytes).unwrap();
        assert!(pieces > 200, "{pieces} pieces");
        for id in 5..22 {
            assert!(room.parts.iter().any(|p| p.owner == id && p.piece), "owner {id}");
        }
        // Break everything: pieces fall, stay finite and out of the lane.
        let mut state = Match::new(4);
        state.phase = 1;
        for object in &mut state.objects {
            object.hp = 0;
            object.broken_tick = 10;
            object.impulse = 120;
        }
        for wall in &mut state.walls {
            wall.hp = 0;
            wall.broken_tick = 10;
            wall.impulse = 120;
        }
        state.tick = 400;
        room.update(&state, [-20.0, 20.0]);
        for p in &room.parts {
            let f = p.fragment.as_ref().unwrap();
            assert!(f.position.x.is_finite() && f.position.y >= -0.01, "{:?}", f.position);
            if p.owner < 16 && heft(p.size) < 0.5 {
                assert!(f.position.z <= -1.0 + 1e-4);
            }
        }
    }

    #[test]
    fn every_visible_part_has_a_durable_owner_and_reset_restores_it() {
        let mut room = ArenaProps::new();
        for id in 5..22 {
            assert!(room.parts.iter().any(|p| p.owner == id));
        }
        assert!(room.parts.len() < 400);
        let mut state = Match::default();
        state.tick = 50;
        state.objects[7].hp = 0;
        state.objects[7].broken_tick = 10;
        room.update(&state, [-20.0, 20.0]);
        assert!(room.parts.iter().any(|p| p.fragment.is_some()));
        room.reset();
        assert!(room.parts.iter().all(|p| p.fragment.is_none()));
    }
}
/// Velocity scale for a broken piece of this size: fist-sized bits fly,
/// a cabinet side only tips over.
fn heft(size: Vec3) -> f32 {
    let longest = size.x.max(size.y).max(size.z);
    (0.45 / longest.max(0.01)).clamp(0.12, 1.0)
}

/// Appends a room piece to a batch, bound to `joint`.
fn append_piece(mesh: &mut MeshData, piece: &MeshData, joint: usize) {
    for sub in &piece.submeshes {
        let base = mesh.verts.len() as u32;
        let first = sub.first_index as usize;
        let range = &piece.indices[first..first + sub.index_count as usize];
        let lo = range.iter().copied().min().unwrap_or(0);
        let hi = range.iter().copied().max().map_or(0, |i| i + 1);
        for v in &piece.verts[lo as usize..hi as usize] {
            mesh.verts.push(Vertex3 {
                joints: [joint as f32, 0.0, 0.0, 0.0],
                weights: [1.0, 0.0, 0.0, 0.0],
                ..*v
            });
        }
        mesh.indices.extend(range.iter().map(|&i| i - lo + base));
        mesh.end_submesh(sub.material);
    }
}
fn cube(mesh: &mut MeshData, joint: usize, color: Vec3) {
    for (normal, u, v) in [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, Vec3::Y, Vec3::X),
    ] {
        let base = mesh.verts.len() as u32;
        for (x, y) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            mesh.verts.push(Vertex3 {
                pos: (normal * 0.5 + u * x + v * y).to_array(),
                normal: normal.to_array(),
                uv: [0.0, 0.0],
                color: color.to_array(),
                joints: [joint as f32, 0.0, 0.0, 0.0],
                weights: [1.0, 0.0, 0.0, 0.0],
            });
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
