//! Hit, hurt and push boxes in the fighting plane: x along the lane, y up,
//! absolute millimetres. A blow connects when its hitbox overlaps one of the
//! defender's hurtboxes — the body (shorter while crouching, sweeping or
//! rising into the uppercut; lifted with a jump) or a limb the defender has
//! stretched out to strike, which can be hit on its own. Which guard stops a
//! blow is still its height class (`moves::Height`). The renderer draws the
//! same boxes in training.
use crate::moves::{Height, Move};
use crate::Fighter;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: i32,
    pub x1: i32,
    pub y0: i32,
    pub y1: i32,
}
impl Rect {
    fn around(x: i32, half: i32, y0: i32, y1: i32) -> Rect {
        Rect { x0: x - half, x1: x + half, y0, y1 }
    }
    /// From `near` to `far` millimetres in front of `x`.
    fn ahead(x: i32, facing: i32, near: i32, far: i32, y0: i32, y1: i32) -> Rect {
        let (a, b) = (x + facing * near, x + facing * far);
        Rect { x0: a.min(b), x1: a.max(b), y0, y1 }
    }
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }
}

/// Half width of a body; a blow's `reach` is measured centre to centre
/// against it, so a standing body is hit within `reach`.
pub const BODY_HALF: i32 = 250;
/// Half width of the pushbox: two bodies never come closer than twice this.
pub const PUSH_HALF: i32 = 300;
/// Height of a standing body and of a low one (crouching, sweeping).
pub const STAND: i32 = 1750;
pub const LOW: i32 = 1100;
/// A stretched limb stops this short of the blow's reach: at the very tip
/// the striker wins the trade.
const LIMB_SHORT: i32 = 80;

/// Height band of a blow above the striker's feet.
pub fn band(action: u32, height: Height) -> (i32, i32) {
    match action {
        // Jab: at the head — a crouching body ducks under it.
        1 => (1250, 1650),
        // Cross and hook: head to chest, a crouching head too.
        11 | 17 => (950, 1550),
        // Front and side kick: the body.
        8 | 18 => (800, 1300),
        12 => (950, 1700),
        // Overhead: from above, onto a crouching head.
        2 => (1050, 1700),
        // Air kick: down and forward from the feet.
        13 => (-150, 700),
        // Rising uppercut: tall, it meets jumps.
        10 => (800, 2400),
        14 => (850, 1500),
        // Grab: arms at chest height.
        4 => (700, 1600),
        // Stomp (room smash).
        19 => (0, 1100),
        // Low kick: at the knee, standing.
        16 => (100, 500),
        // Jump knee: forward at the body; bicycle kick: down and forward.
        20 => (150, 1100),
        21 => (-250, 650),
        // Lunging hook: head and chest; rear uppercut: tall, it meets jumps.
        22 => (1000, 1650),
        23 => (900, 2300),
        // Advancing roundhouse: chest to head; thrust kick: the body.
        24 => (900, 1650),
        25 => (700, 1250),
        // Flying knee: from above, onto a crouching head too.
        26 => (950, 1700),
        // Hurricane kick: the whole body height.
        27 => (500, 1700),
        _ if height == Height::Low => (0, 280),
        _ => (900, 1500),
    }
}

/// The blow's box while `f` is in the active frames of `m`.
pub fn hitbox(f: &Fighter, m: &Move) -> Option<Rect> {
    if f.frame < m.startup || f.frame >= m.startup + m.active {
        return None;
    }
    let (y0, y1) = band(f.action, m.height);
    Some(Rect::ahead(f.x, f.facing, 100, m.reach - BODY_HALF, f.y + y0, f.y + y1))
}

/// Where `f` can be hit: its body, and the limb it is striking with from
/// just before contact until halfway through the recovery, drawing back
/// over that time. `m` is the move `f`
/// is performing, if any. A body knocked down has none.
pub fn hurtboxes(f: &Fighter, m: Option<&Move>) -> [Option<Rect>; 2] {
    if f.down > 0 {
        return [None, None];
    }
    let rising = f.action == 10 && m.is_some_and(|m| f.frame < m.startup);
    // Crouching, sweeping and rising into the uppercut keep the head low.
    let low = (f.crouch && f.y == 0) || f.action == 9 || rising;
    let body = Rect::around(f.x, BODY_HALF, f.y, f.y + if low { LOW } else { STAND });
    let limb = m.and_then(|m| {
        // The rising uppercut keeps its arm in until it strikes.
        let out = if f.action == 10 { m.startup } else { m.startup.saturating_sub(2) };
        let back = m.startup + m.active + (m.total.saturating_sub(m.startup + m.active) + 1) / 2;
        let far = m.reach - BODY_HALF - LIMB_SHORT;
        if f.frame < out || f.frame >= back || far <= BODY_HALF {
            return None;
        }
        let done = m.startup + m.active;
        let far = if f.frame >= done {
            let u = (f.frame - done) as f32 / (back - done).max(1) as f32;
            far - ((far - BODY_HALF) as f32 * u) as i32
        } else {
            far
        };
        let (y0, y1) = band(f.action, m.height);
        Some(Rect::ahead(f.x, f.facing, BODY_HALF, far, f.y + y0, f.y + y1))
    });
    [Some(body), limb]
}

/// The box that keeps bodies apart.
pub fn pushbox(f: &Fighter) -> Rect {
    Rect::around(f.x, PUSH_HALF, f.y, f.y + STAND)
}
