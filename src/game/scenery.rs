//! Pictures from the page placed in the room: the night city behind the
//! windows (two layers, parallax) and the players' photos on the TV, the
//! laptop and the bedroom canvases. The page decodes and composes them
//! (web/scenery.js) and hands over RGBA pixels; here they become textures on
//! flat quads. Purely cosmetic, like the rest of the room.
use super::arena_props::ArenaProps;
use crate::engine::{graphics::Graphics, math3::*, render3d::Camera};
use miniquad::TextureId;

#[cfg(target_arch = "wasm32")]
extern "C" {
    /// [version, width, height] of a page picture; 0 when the slot is empty.
    fn fight_image_info(slot: i32, out: *mut i32) -> i32;
    /// Copies the slot's RGBA pixels; returns the byte count (0: no room).
    fn fight_image_copy(slot: i32, ptr: *mut u8, capacity: usize) -> usize;
}

/// Page picture slots (web/scenery.js uses the same numbers).
pub const CITY_FAR: usize = 0;
pub const CITY_NEAR: usize = 1;
const SLOTS: usize = 6;

/// The city render's view (tools/backdrop/city.py): level, tan of half the
/// horizontal and vertical field, seen from the usual fight camera.
const BACKDROP_TAN_H: f32 = 1.0;
const BACKDROP_TAN_V: f32 = 0.25;
const EYE: Vec3 = Vec3 { x: 0.0, y: 1.62, z: 6.0 };
/// How far behind the back wall each layer stands: farther moves less.
const LAYER_DEPTH: [f32; 2] = [160.0, 40.0];
/// The back wall's outside face; the mask stands just behind it.
const MASK_Z: f32 = -3.6;
/// Window openings in the back wall (x0, x1, y0, y1), a little oversized:
/// the wall around them hides the edges.
const OPENINGS: [[f32; 4]; 2] = [[-11.95, -8.05, 0.7, 3.3], [-2.95, 2.95, 0.7, 3.3]];
const VOID: [f32; 4] = [0.031, 0.055, 0.094, 1.0];

/// A picture in the room: slot, lower-left corner, width and height vectors,
/// colour multiplier, and the breakable object that carries it (if any).
struct Frame {
    slot: usize,
    origin: Vec3,
    right: Vec3,
    up: Vec3,
    tint: [f32; 4],
    owner: Option<usize>,
}

/// Measured on the baked room (Blender, see ARCHITECTURE.md §4): screens
/// glow, canvases take the room's light.
fn frames() -> [Frame; 4] {
    let flat = |slot, x0: f32, x1: f32, y0: f32, y1: f32, z: f32, tint, owner| Frame {
        slot,
        origin: vec3(x0, y0, z),
        right: vec3(x1 - x0, 0.0, 0.0),
        up: vec3(0.0, y1 - y0, 0.0),
        tint,
        owner,
    };
    [
        // Living room TV (curved glass: the picture sits on its front).
        flat(2, -2.38, -1.898, 0.972, 1.337, -2.398, [1.0, 1.0, 1.0, 1.0], None),
        // Study laptop on the desk (room object 12).
        flat(3, 4.906, 5.194, 0.891, 1.125, -1.483, [1.0, 1.0, 1.0, 1.0], Some(12)),
        // Bedroom: the large canvas and the small framed picture.
        flat(4, 9.135, 10.164, 1.809, 2.572, -3.296, [0.66, 0.6, 0.53, 1.0], None),
        flat(5, 10.745, 11.055, 2.175, 2.627, -3.235, [0.62, 0.56, 0.5, 1.0], None),
    ]
}

#[derive(Default)]
struct Slot {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    version: i32,
    texture: Option<TextureId>,
}

pub struct Scenery {
    slots: Vec<Slot>,
    white: Option<TextureId>,
}

impl Scenery {
    pub fn new() -> Self {
        Self { slots: (0..SLOTS).map(|_| Slot::default()).collect(), white: None }
    }

    /// Takes new pictures from the page (each slot carries a version).
    pub fn update(&mut self, g: &mut Graphics) {
        if self.white.is_none() {
            self.white = Some(g.upload_rgba(1, 1, &[255; 4]));
        }
        #[cfg(target_arch = "wasm32")]
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let mut info = [0i32; 3];
            if unsafe { fight_image_info(index as i32, info.as_mut_ptr()) } != 1 || info[0] == slot.version {
                continue;
            }
            let (w, h) = (info[1].max(0) as usize, info[2].max(0) as usize);
            if w == 0 || h == 0 || w * h > 4096 * 2048 {
                continue;
            }
            let mut rgba = vec![0u8; w * h * 4];
            if unsafe { fight_image_copy(index as i32, rgba.as_mut_ptr(), rgba.len()) } != rgba.len() {
                continue;
            }
            if let Some(old) = slot.texture.take() {
                g.delete_texture(old);
            }
            slot.texture = Some(g.upload_rgba(w as u32, h as u32, &rgba));
            slot.version = info[0];
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = g;
    }

    /// The city has arrived: the baked stand-in buildings can go.
    pub fn has_city(&self) -> bool {
        self.slots[CITY_FAR].texture.is_some()
    }

    /// Before the room: the dark void around the apartment (the back wall
    /// continued, with holes at the windows), then the city layers, farthest
    /// first. The room drawn afterwards covers them except in the windows.
    pub fn draw_backdrop(&self, g: &mut Graphics, camera: &Camera) {
        let (Some(white), true) = (self.white, self.has_city()) else { return };
        let big = 400.0;
        let [a, b] = OPENINGS;
        let (y0, y1) = (a[2], a[3]);
        let mask = [
            [-big, big, y1, big],
            [-big, big, -big, y0],
            [-big, a[0], y0, y1],
            [a[1], b[0], y0, y1],
            [b[1], big, y0, y1],
        ];
        for [x0, x1, b0, b1] in mask {
            let m = Mat4::from_trs(vec3(x0, b0, MASK_Z), Quat::IDENTITY, vec3(x1 - x0, b1 - b0, 1.0));
            g.draw_sprite(white, m, camera, VOID, 0.0);
        }
        // Depth at the far end: the near layer a step in front (16-bit safe).
        for (slot, depth, far) in [(CITY_FAR, LAYER_DEPTH[0], 0.9999), (CITY_NEAR, LAYER_DEPTH[1], 0.9997)] {
            let Some(texture) = self.slots[slot].texture else { continue };
            let reach = EYE.z + depth;
            let (w, h) = (2.0 * reach * BACKDROP_TAN_H, 2.0 * reach * BACKDROP_TAN_V);
            let m = Mat4::from_trs(vec3(EYE.x - w / 2.0, EYE.y - h / 2.0, -depth), Quat::IDENTITY, vec3(w, h, 1.0));
            g.draw_sprite(texture, m, camera, [1.0; 4], far);
        }
    }

    /// Photos on the screens and canvases; a picture on a breakable object
    /// flies with the piece it lies on.
    pub fn draw_screens(&self, g: &mut Graphics, camera: &Camera, props: &ArenaProps) {
        for f in frames() {
            let Some(texture) = self.slots[f.slot].texture else { continue };
            let centre = f.origin + (f.right + f.up) * 0.5;
            let carry = f.owner.and_then(|owner| props.piece_motion(owner, centre)).unwrap_or(Mat4::IDENTITY);
            let n = f.right.cross(f.up).normalize();
            let (r, u, o) = (f.right, f.up, f.origin);
            let place = Mat4([r.x, r.y, r.z, 0.0, u.x, u.y, u.z, 0.0, n.x, n.y, n.z, 0.0, o.x, o.y, o.z, 1.0]);
            g.draw_sprite(texture, carry * place, camera, f.tint, 0.0);
        }
    }
}
