//! Powered ragdoll: physical secondary motion on top of the animated pose.
//!
//! Particles sit on the main joints. The hips are pinned to the animation
//! (the authoritative position never moves); every other particle is a
//! damped spring around the place the animation puts its joint, so with
//! nothing happening the pose is exactly the animated one, strikes included.
//! Hits push the struck particles and loosen them for a moment, the root's
//! acceleration (a launch, a knockback, a landing) drags them by inertia,
//! loose parts sag under gravity, bones keep their lengths, joints stay in
//! their range (knees and elbows are hinges that bend one way; the spine,
//! neck, shoulders and hips turn within cones around the animated pose) and
//! nothing goes through the floor. The bones are then turned to follow the
//! particles.
//! Cosmetic and client side only, like the debris.
use crate::engine::{math3::*, skeleton::{Pose, Skeleton}};

/// The simulation's gravity (5 mm/tick²): a body in free fall is weightless
/// relative to its root, as it should be.
const GRAVITY: f32 = 18.0;

const HIPS: usize = 0;
const SPINE: usize = 1;
const CHEST: usize = 2;
const NECK: usize = 3;
const HEAD: usize = 4;
/// The top of the head (not a bone: a point fixed above the head joint).
const TIP: usize = 5;
const L_ARM: usize = 6;
const L_FORE: usize = 7;
const L_HAND: usize = 8;
const R_ARM: usize = 9;
const R_FORE: usize = 10;
const R_HAND: usize = 11;
const L_UPLEG: usize = 12;
const L_LEG: usize = 13;
const L_FOOT: usize = 14;
const R_UPLEG: usize = 15;
const R_LEG: usize = 16;
const R_FOOT: usize = 17;
const N: usize = 18;

/// Spring rate (1/s) of each particle when the body is in control.
const RATE: [f32; N] = [
    0.0, 45.0, 35.0, 30.0, 25.0, 25.0, 35.0, 28.0, 24.0, 35.0, 28.0, 24.0, 60.0, 50.0, 60.0, 60.0, 50.0, 60.0,
];
/// Collision radius above the floor.
const RADIUS: [f32; N] = [
    0.12, 0.12, 0.12, 0.08, 0.10, 0.10, 0.06, 0.05, 0.04, 0.06, 0.05, 0.04, 0.09, 0.06, 0.03, 0.09, 0.06, 0.03,
];
/// Inverse mass in the length constraints (the pinned pelvis takes nothing).
const GIVE: [f32; N] = [
    0.0, 0.5, 0.5, 0.8, 1.0, 1.0, 0.8, 1.0, 1.0, 0.8, 1.0, 1.0, 0.0, 0.9, 1.0, 0.0, 0.9, 1.0,
];
/// The pelvis is rigid and follows the animation: the hips and hip joints.
fn pinned(i: usize) -> bool {
    matches!(i, HIPS | L_UPLEG | R_UPLEG)
}
/// Lengths kept by the solver: the bones, then braces that keep the torso
/// and pelvis rigid.
const LINKS: [(usize, usize); 30] = [
    (HIPS, SPINE),
    (SPINE, CHEST),
    (CHEST, NECK),
    (NECK, HEAD),
    (HEAD, TIP),
    (CHEST, L_ARM),
    (L_ARM, L_FORE),
    (L_FORE, L_HAND),
    (CHEST, R_ARM),
    (R_ARM, R_FORE),
    (R_FORE, R_HAND),
    (HIPS, L_UPLEG),
    (L_UPLEG, L_LEG),
    (L_LEG, L_FOOT),
    (HIPS, R_UPLEG),
    (R_UPLEG, R_LEG),
    (R_LEG, R_FOOT),
    (L_ARM, R_ARM),
    (L_UPLEG, R_UPLEG),
    (L_ARM, SPINE),
    (R_ARM, SPINE),
    (L_UPLEG, SPINE),
    (R_UPLEG, SPINE),
    (HIPS, CHEST),
    (NECK, L_ARM),
    (NECK, R_ARM),
    (HIPS, NECK),
    (L_ARM, L_UPLEG),
    (R_ARM, R_UPLEG),
    (SPINE, NECK),
];

/// A joint's range. `Cone`: the segment stays within this angle (radians)
/// of where the animation puts it relative to its parent segment. `Hinge`:
/// it bends only about the animated hinge axis, from straight to this angle.
#[derive(Clone, Copy)]
enum Range {
    Cone(f32),
    Hinge(usize, f32),
}

/// Segment (joint, child), its parent segment (None: the pinned hips, as
/// animated), its range and the particles that move with the child.
const JOINTS: [(usize, usize, Option<(usize, usize)>, Range, &[usize]); 13] = [
    (HIPS, SPINE, None, Range::Cone(0.35), &[SPINE, CHEST, NECK, HEAD, TIP, L_ARM, L_FORE, L_HAND, R_ARM, R_FORE, R_HAND]),
    (SPINE, CHEST, Some((HIPS, SPINE)), Range::Cone(0.35), &[CHEST, NECK, HEAD, TIP, L_ARM, L_FORE, L_HAND, R_ARM, R_FORE, R_HAND]),
    (CHEST, NECK, Some((SPINE, CHEST)), Range::Cone(0.35), &[NECK, HEAD, TIP]),
    (NECK, HEAD, Some((CHEST, NECK)), Range::Cone(0.6), &[HEAD, TIP]),
    (HEAD, TIP, Some((NECK, HEAD)), Range::Cone(0.5), &[TIP]),
    (L_ARM, L_FORE, Some((CHEST, NECK)), Range::Cone(1.2), &[L_FORE, L_HAND]),
    (L_FORE, L_HAND, Some((L_ARM, L_FORE)), Range::Hinge(0, 2.6), &[L_HAND]),
    (R_ARM, R_FORE, Some((CHEST, NECK)), Range::Cone(1.2), &[R_FORE, R_HAND]),
    (R_FORE, R_HAND, Some((R_ARM, R_FORE)), Range::Hinge(1, 2.6), &[R_HAND]),
    (L_UPLEG, L_LEG, None, Range::Cone(0.95), &[L_LEG, L_FOOT]),
    (L_LEG, L_FOOT, Some((L_UPLEG, L_LEG)), Range::Hinge(2, 2.6), &[L_FOOT]),
    (R_UPLEG, R_LEG, None, Range::Cone(0.95), &[R_LEG, R_FOOT]),
    (R_LEG, R_FOOT, Some((R_UPLEG, R_LEG)), Range::Hinge(3, 2.6), &[R_FOOT]),
];

/// Which part of the body a blow (or a landing) pushes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Part {
    Head,
    Chest,
    Gut,
    Legs,
    /// The forearms of a raised guard.
    Guard,
    Body,
}
impl Part {
    fn weights(self) -> &'static [(usize, f32)] {
        match self {
            Part::Head => &[(TIP, 1.0), (HEAD, 1.0), (NECK, 0.6), (CHEST, 0.25), (L_HAND, 0.3), (R_HAND, 0.3)],
            Part::Chest => &[(CHEST, 1.0), (NECK, 0.7), (SPINE, 0.6), (L_ARM, 0.6), (R_ARM, 0.6), (HEAD, 0.5), (TIP, 0.5), (L_HAND, 0.4), (R_HAND, 0.4)],
            Part::Gut => &[(SPINE, 1.0), (CHEST, 0.5), (HEAD, 0.4), (TIP, 0.4), (L_HAND, 0.5), (R_HAND, 0.5), (L_FORE, 0.4), (R_FORE, 0.4)],
            Part::Legs => &[(L_LEG, 1.0), (L_FOOT, 1.0), (R_LEG, 0.7), (R_FOOT, 0.7), (SPINE, 0.2)],
            Part::Guard => &[(L_HAND, 1.0), (R_HAND, 1.0), (L_FORE, 0.8), (R_FORE, 0.8), (HEAD, 0.3), (TIP, 0.3), (CHEST, 0.2)],
            Part::Body => &[(SPINE, 0.6), (CHEST, 0.6), (NECK, 0.7), (HEAD, 0.8), (TIP, 0.8), (L_HAND, 1.0), (R_HAND, 1.0), (L_FORE, 0.9), (R_FORE, 0.9), (L_FOOT, 0.8), (R_FOOT, 0.8), (L_LEG, 0.6), (R_LEG, 0.6)],
        }
    }
}

/// How much the body is in control of each region (multiplies `RATE`), and
/// how much of the root's change of speed it feels (its own jumps and steps
/// barely shake it; a launched or knocked out body feels all of it).
#[derive(Clone, Copy, Debug)]
pub struct Drive {
    pub torso: f32,
    pub head: f32,
    pub arms: f32,
    pub legs: f32,
    pub inertia: f32,
}
impl Drive {
    pub const FULL: Drive = Drive { torso: 1.0, head: 1.0, arms: 1.0, legs: 1.0, inertia: 0.25 };
    fn of(self, i: usize) -> f32 {
        match i {
            SPINE | CHEST => self.torso,
            NECK | HEAD | TIP => self.head,
            L_ARM..=R_HAND => self.arms,
            _ => self.legs,
        }
    }
    pub fn lerp(self, o: Drive, t: f32) -> Drive {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Drive {
            torso: l(self.torso, o.torso),
            head: l(self.head, o.head),
            arms: l(self.arms, o.arms),
            legs: l(self.legs, o.legs),
            inertia: l(self.inertia, o.inertia),
        }
    }
}

/// The bones a rig gives each particle (`TIP` uses the head bone).
pub struct RagdollRig {
    bones: [usize; N],
}
impl RagdollRig {
    /// Hip, knee and ankle bones of the left (0) or right (1) leg.
    pub fn leg(&self, side: usize) -> [usize; 3] {
        if side == 0 {
            [self.bones[L_UPLEG], self.bones[L_LEG], self.bones[L_FOOT]]
        } else {
            [self.bones[R_UPLEG], self.bones[R_LEG], self.bones[R_FOOT]]
        }
    }
    pub fn new(skeleton: &Skeleton) -> Option<RagdollRig> {
        let find = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| skeleton.bones.iter().position(|b| b.name.ends_with(n)))
        };
        let hips = find(&["Hips"])?;
        let spine = find(&["Spine1", "Spine"])?;
        let chest = find(&["Spine2", "Spine1", "Spine"])?;
        let neck = find(&["Neck"]).unwrap_or(chest);
        let head = find(&["Head"])?;
        Some(RagdollRig {
            bones: [
                hips,
                spine,
                chest,
                neck,
                head,
                head,
                find(&["LeftArm"])?,
                find(&["LeftForeArm"])?,
                find(&["LeftHand"])?,
                find(&["RightArm"])?,
                find(&["RightForeArm"])?,
                find(&["RightHand"])?,
                find(&["LeftUpLeg"])?,
                find(&["LeftLeg"])?,
                find(&["LeftFoot"])?,
                find(&["RightUpLeg"])?,
                find(&["RightLeg"])?,
                find(&["RightFoot"])?,
            ],
        })
    }
}

/// The physical state of one body, in body space: metres, the fighter's
/// canonical facing, before the placement in the room.
#[derive(Clone)]
pub struct Ragdoll {
    ready: bool,
    /// Offset of each particle from its animated place, and its velocity.
    offset: [Vec3; N],
    velocity: [Vec3; N],
    /// Loosened by a recent blow (1 = limp), recovering over a third of a second.
    shock: [f32; N],
    root: Vec3,
    root_velocity: Vec3,
    /// Hinge axes of the elbows and knees (body space) from the last frame
    /// the animation bent them; zero until then.
    hinge: [Vec3; 4],
}
impl Default for Ragdoll {
    fn default() -> Self {
        Ragdoll {
            ready: false,
            offset: [Vec3::ZERO; N],
            velocity: [Vec3::ZERO; N],
            shock: [0.0; N],
            root: Vec3::ZERO,
            root_velocity: Vec3::ZERO,
            hinge: [Vec3::ZERO; 4],
        }
    }
}

impl Ragdoll {
    pub fn reset(&mut self) {
        *self = Ragdoll::default();
    }

    /// A blow: `velocity` (m/s, body space) on a part of the body, which
    /// goes loose for a moment.
    pub fn push(&mut self, part: Part, velocity: Vec3) {
        if !self.ready {
            return;
        }
        for &(i, w) in part.weights() {
            self.velocity[i] += velocity * w;
            self.shock[i] = self.shock[i].max(w.min(1.0));
        }
    }

    /// Advances the body by `dt` and turns the bones of `pose` to follow it.
    /// `root` is the placement origin in the room and `root_velocity` its
    /// authoritative velocity (m/s, from the simulation, not from rendered
    /// positions: uneven frames would shake the body); a change of it is felt
    /// as inertia. `to_body` turns room directions into body space and `body`
    /// maps the model space of `skeleton` into body space.
    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        &mut self,
        rig: &RagdollRig,
        skeleton: &Skeleton,
        pose: &mut Pose,
        globals: &mut Vec<Mat4>,
        body: Mat4,
        to_body: Mat4,
        root: Vec3,
        root_velocity: Vec3,
        drive: Drive,
        dt: f32,
    ) {
        let dt = dt.clamp(0.0, 1.0 / 20.0);
        skeleton.global_matrices(pose, globals);
        let joint = |globals: &Vec<Mat4>, bone: usize| body.transform_point(globals[bone].transform_point(Vec3::ZERO));
        let mut target = [Vec3::ZERO; N];
        for i in 0..N {
            target[i] = joint(globals, rig.bones[i]);
        }
        // The top of the head, carried by the head bone.
        let up = (target[HEAD] - target[NECK]).normalize();
        target[TIP] = target[HEAD] + up * 0.18;
        let head_inverse = (body * globals[rig.bones[HEAD]]).invert();
        let tip_local = head_inverse.transform_point(target[TIP]);

        // A teleport (a new round) starts over; otherwise a change of the
        // root's speed is felt as a kick the other way.
        if !self.ready || (root - self.root).length() > 0.8 {
            *self = Ragdoll { ready: true, root, root_velocity, ..Ragdoll::default() };
            return;
        }
        // Snapshots change the velocity in steps (20 Hz online): a 40 ms
        // filter turns them into ramps, a blow still lands as a jolt.
        let previous = self.root_velocity;
        self.root_velocity = previous + (root_velocity - previous) * (1.0 - (-dt / 0.04).exp());
        let kick = to_body.transform_direction(previous - self.root_velocity) * drive.inertia;
        self.root = root;
        for v in &mut self.velocity[1..] {
            *v += kick;
        }
        let floor = -root.y;

        // Hinge axes: from the animated bend, or kept from the last frame
        // it bent (kept square to the animated upper segment).
        for &(a, b, parent, range, _) in &JOINTS {
            let (Range::Hinge(k, _), Some((pa, _))) = (range, parent) else { continue };
            let (upper, lower) = ((target[a] - target[pa]).normalize(), (target[b] - target[a]).normalize());
            let bend = upper.cross(lower);
            if bend.length() > 0.09 {
                self.hinge[k] = bend.normalize();
            } else if self.hinge[k].length() > 0.5 {
                let kept = self.hinge[k] - upper * self.hinge[k].dot(upper);
                if kept.length() > 1e-3 {
                    self.hinge[k] = kept.normalize();
                }
            }
        }
        let mut rest = [0.0; LINKS.len()];
        for (k, &(a, b)) in LINKS.iter().enumerate() {
            rest[k] = (target[a] - target[b]).length();
        }
        let steps = if dt > 0.0 { (dt * 120.0).ceil().clamp(1.0, 6.0) as usize } else { 0 };
        let h = dt / steps.max(1) as f32;
        for _ in 0..steps {
            let mut p = [Vec3::ZERO; N];
            for i in 0..N {
                if pinned(i) {
                    p[i] = target[i];
                    self.offset[i] = Vec3::ZERO;
                    self.velocity[i] = Vec3::ZERO;
                    continue;
                }
                // Control: 1 when the body holds this part, less when it is
                // loose (by state or a recent blow). Only loose parts sag.
                let control = (drive.of(i) * (1.0 - 0.85 * self.shock[i])).clamp(0.0, 1.0);
                let w = (RATE[i] * control).max(0.5);
                let sag = 1.0 - control;
                let force = self.offset[i] * (-w * w) - self.velocity[i] * (1.6 * w) + vec3(0.0, -GRAVITY * sag, 0.0);
                self.velocity[i] += force * h;
                p[i] = target[i] + self.offset[i] + self.velocity[i] * h;
            }
            for _ in 0..4 {
                for (k, &(a, b)) in LINKS.iter().enumerate() {
                    let d = p[b] - p[a];
                    let len = d.length();
                    let give = GIVE[a] + GIVE[b];
                    if len < 1e-6 || give <= 0.0 {
                        continue;
                    }
                    let fix = d * ((len - rest[k]) / (len * give));
                    p[a] += fix * GIVE[a];
                    p[b] = p[b] - fix * GIVE[b];
                }
                limit_joints(&mut p, &target, &self.hinge);
                for i in (0..N).filter(|&i| !pinned(i)) {
                    let low = floor + RADIUS[i];
                    if p[i].y < low {
                        p[i].y = low;
                    }
                }
            }
            // The joints have the last word (the floor only nudges).
            limit_joints(&mut p, &target, &self.hinge);
            for i in (0..N).filter(|&i| !pinned(i)) {
                let offset = p[i] - target[i];
                let mut v = (offset - self.offset[i]) * (1.0 / h);
                // The floor stops a body dead (no bounce) and drags it.
                if p[i].y <= floor + RADIUS[i] + 1e-3 {
                    v.y = v.y.min(0.0);
                    v.x *= 0.85;
                    v.z *= 0.85;
                }
                self.velocity[i] = v;
                self.offset[i] = offset;
            }
        }
        for s in &mut self.shock {
            *s = (*s - dt * 3.0).max(0.0);
        }
        // Sub-millimetre: the animation as is (no visible switch).
        if self.offset.iter().all(|o| o.length() < 0.0005) {
            return;
        }

        // Turn the bones to follow the particles, root to tips.
        let p: [Vec3; N] = std::array::from_fn(|i| target[i] + self.offset[i]);
        let to_model = body.invert();
        let turn = |bone: usize, rotation: Quat, at: Vec3, pose: &mut Pose, globals: &mut Vec<Mat4>| {
            turn_bone(skeleton, pose, globals, body, to_model, bone, rotation, at);
        };
        // The spine: towards the neck, shoulders level with the particles.
        {
            let at = joint(globals, rig.bones[SPINE]);
            let up_now = joint(globals, rig.bones[NECK]) - at;
            let side_now = joint(globals, rig.bones[R_ARM]) - joint(globals, rig.bones[L_ARM]);
            let rotation = align(up_now, side_now, p[NECK] - at, p[R_ARM] - p[L_ARM]);
            turn(rig.bones[SPINE], rotation, at, pose, globals);
        }
        let segments: [(usize, usize); 9] = [
            (NECK, HEAD),
            (L_ARM, L_FORE),
            (L_FORE, L_HAND),
            (R_ARM, R_FORE),
            (R_FORE, R_HAND),
            (L_UPLEG, L_LEG),
            (L_LEG, L_FOOT),
            (R_UPLEG, R_LEG),
            (R_LEG, R_FOOT),
        ];
        for (a, b) in segments {
            if rig.bones[a] == rig.bones[b] {
                continue;
            }
            let at = joint(globals, rig.bones[a]);
            let now = joint(globals, rig.bones[b]) - at;
            turn(rig.bones[a], Quat::from_rotation_arc(now, p[b] - at), at, pose, globals);
        }
        // The head nods towards its tip.
        let at = joint(globals, rig.bones[HEAD]);
        let tip = (body * globals[rig.bones[HEAD]]).transform_point(tip_local);
        turn(rig.bones[HEAD], Quat::from_rotation_arc(tip - at, p[TIP] - at), at, pose, globals);
    }
}

/// Keeps every joint of `p` in its range (`JOINTS`), root to tips; a moved
/// segment carries the particles beyond it.
fn limit_joints(p: &mut [Vec3; N], target: &[Vec3; N], hinge: &[Vec3; 4]) {
    for &(a, b, parent, range, moves) in &JOINTS {
        let d = p[b] - p[a];
        let len = d.length();
        if len < 1e-5 {
            continue;
        }
        let dir = d * (1.0 / len);
        // The parent's turn away from the animation, applied to the
        // animated segment: where the joint, as animated, would put it.
        let animated = (target[b] - target[a]).normalize();
        let turn = match parent {
            Some((pa, pb)) => Quat::from_rotation_arc((target[pb] - target[pa]).normalize(), (p[pb] - p[pa]).normalize()),
            None => Quat::IDENTITY,
        };
        let wanted = match range {
            Range::Cone(max) => {
                let expected = turn.rotate(animated);
                if dir.dot(expected) >= max.cos() {
                    continue;
                }
                let mut axis = expected.cross(dir);
                if axis.length() < 1e-6 {
                    axis = expected.cross(if expected.x.abs() < 0.9 { Vec3::X } else { Vec3::Y });
                }
                Quat::from_axis_angle(axis.normalize(), max).rotate(expected)
            }
            Range::Hinge(k, max) => {
                let Some((pa, _)) = parent else { continue };
                if hinge[k].length() < 0.5 {
                    continue;
                }
                let upper = (p[a] - p[pa]).normalize();
                let axis = turn.rotate(hinge[k]);
                let axis = (axis - upper * axis.dot(upper)).normalize();
                // Bend only about the axis, from straight to `max`.
                let flat = dir - axis * dir.dot(axis);
                let angle = if flat.length() < 1e-5 { 0.0 } else { upper.cross(flat).dot(axis).atan2(upper.dot(flat)) };
                let angle = angle.clamp(0.0, max);
                let bent = Quat::from_axis_angle(axis, angle).rotate(upper);
                if (bent - dir).length() < 1e-4 {
                    continue;
                }
                bent
            }
        };
        let shift = p[a] + wanted * len - p[b];
        for &i in moves {
            p[i] += shift;
        }
    }
}

/// Turns `bone` by `rotation` about the point `at` (both in body space;
/// `body` maps model space there, `to_model` back) and refreshes the global
/// matrices.
#[allow(clippy::too_many_arguments)]
pub(crate) fn turn_bone(
    skeleton: &Skeleton,
    pose: &mut Pose,
    globals: &mut Vec<Mat4>,
    body: Mat4,
    to_model: Mat4,
    bone: usize,
    rotation: Quat,
    at: Vec3,
) {
    let delta = to_model
        * Mat4::translation(at)
        * Mat4::from_trs(Vec3::ZERO, rotation, vec3(1.0, 1.0, 1.0))
        * Mat4::translation(at * -1.0)
        * body;
    let global = delta * globals[bone];
    let parent = skeleton.bones[bone].parent.map_or(Mat4::IDENTITY, |b| globals[b]);
    pose.locals[bone].rotation = Quat::from_matrix(parent.invert() * global);
    skeleton.global_matrices(pose, globals);
}

/// The rotation taking direction `a` (with `a_side` across it) onto `b`
/// (with `b_side`).
fn align(a: Vec3, a_side: Vec3, b: Vec3, b_side: Vec3) -> Quat {
    let first = Quat::from_rotation_arc(a, b);
    let axis = b.normalize();
    let flat = |v: Vec3| v - axis * v.dot(axis);
    let (from, to) = (flat(first.rotate(a_side)), flat(b_side));
    if from.length() < 1e-4 || to.length() < 1e-4 {
        return first;
    }
    (Quat::from_rotation_arc(from, to) * first).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::gltf;
    use crate::game::dancer::Character;

    fn load() -> (Character, RagdollRig) {
        let character = Character::from_model(gltf::load_glb(&std::fs::read("assets/character.glb").unwrap()).unwrap());
        let rig = RagdollRig::new(&character.skeleton).expect("ragdoll rig");
        (character, rig)
    }

    #[test]
    fn at_rest_the_pose_is_exactly_the_animation() {
        let (c, rig) = load();
        let mut pose = c.skeleton.rest_pose();
        let mut globals = Vec::new();
        let mut doll = Ragdoll::default();
        for _ in 0..30 {
            doll.apply(&rig, &c.skeleton, &mut pose, &mut globals, c.transform, Mat4::IDENTITY, Vec3::ZERO, Vec3::ZERO, Drive::FULL, 1.0 / 60.0);
        }
        let rest = c.skeleton.rest_pose();
        for (a, b) in pose.locals.iter().zip(&rest.locals) {
            assert!(a.rotation.dot(b.rotation).abs() > 0.9999);
        }
    }

    #[test]
    fn a_blow_moves_the_head_then_it_recovers_and_limbs_keep_their_length() {
        let (c, rig) = load();
        let rest = c.skeleton.rest_pose();
        let mut globals = Vec::new();
        let mut doll = Ragdoll::default();
        let head = |pose: &Pose, globals: &mut Vec<Mat4>| {
            c.skeleton.global_matrices(pose, globals);
            c.transform.transform_point(globals[rig.bones[HEAD]].transform_point(Vec3::ZERO))
        };
        let mut pose = rest.clone();
        doll.apply(&rig, &c.skeleton, &mut pose, &mut globals, c.transform, Mat4::IDENTITY, Vec3::ZERO, Vec3::ZERO, Drive::FULL, 1.0 / 60.0);
        let still = head(&rest, &mut globals);
        doll.push(Part::Head, vec3(0.0, 0.0, -3.0));
        let mut furthest: f32 = 0.0;
        for frame in 0..90 {
            let mut pose = rest.clone();
            doll.apply(&rig, &c.skeleton, &mut pose, &mut globals, c.transform, Mat4::IDENTITY, Vec3::ZERO, Vec3::ZERO, Drive::FULL, 1.0 / 60.0);
            let moved = (head(&pose, &mut globals) - still).length();
            furthest = furthest.max(moved);
            assert!(pose.locals.iter().all(|l| l.rotation.x.is_finite()), "frame {frame}");
        }
        assert!(furthest > 0.05, "the head barely moved: {furthest}");
        let mut pose = rest.clone();
        doll.apply(&rig, &c.skeleton, &mut pose, &mut globals, c.transform, Mat4::IDENTITY, Vec3::ZERO, Vec3::ZERO, Drive::FULL, 1.0 / 60.0);
        assert!((head(&pose, &mut globals) - still).length() < 0.01, "the head came back");
    }

    /// Flexion of the joint at `b` (between segments a-b and b-c) about
    /// the animated hinge axis turned with the upper segment, as the solver
    /// sees it.
    fn flexion(p: &[Vec3; N], t: &[Vec3; N], a: usize, b: usize, c: usize, axis: Vec3) -> f32 {
        let (u, v) = ((p[b] - p[a]).normalize(), (p[c] - p[b]).normalize());
        let axis = Quat::from_rotation_arc((t[b] - t[a]).normalize(), u).rotate(axis);
        u.cross(v).dot(axis).atan2(u.dot(v))
    }

    #[test]
    fn a_limp_tumbling_body_keeps_its_joints_in_range() {
        let (c, rig) = load();
        let rest = c.skeleton.rest_pose();
        let mut globals = Vec::new();
        let mut doll = Ragdoll::default();
        // A bent stance teaches the hinge axes, as a fight does.
        let mut stance = rest.clone();
        for (leg, bend) in [(rig.leg(0), 0.5f32), (rig.leg(1), 0.5)] {
            stance.locals[leg[1]].rotation = (Quat::from_axis_angle(Vec3::X, bend) * stance.locals[leg[1]].rotation).normalize();
        }
        let limp = Drive { torso: 0.2, head: 0.1, arms: 0.05, legs: 0.05, inertia: 1.0 };
        let joints = |pose: &Pose, globals: &mut Vec<Mat4>| -> [Vec3; N] {
            c.skeleton.global_matrices(pose, globals);
            std::array::from_fn(|i| c.transform.transform_point(globals[rig.bones[i]].transform_point(Vec3::ZERO)))
        };
        let mut pose = stance.clone();
        doll.apply(&rig, &c.skeleton, &mut pose, &mut globals, c.transform, Mat4::IDENTITY, Vec3::ZERO, Vec3::ZERO, Drive::FULL, 1.0 / 60.0);
        let animated = joints(&stance, &mut globals);
        let (mut worst_knee, mut worst_elbow): (f32, f32) = (0.0, 0.0);
        for frame in 0..300 {
            if frame % 40 == 0 {
                // Kicked about from every side.
                let a = frame as f32 * 0.7;
                doll.push(Part::Body, vec3(a.cos(), 0.6, a.sin()) * 6.0);
                doll.push(Part::Legs, vec3(-a.sin(), -0.4, a.cos()) * 6.0);
            }
            let mut pose = stance.clone();
            doll.apply(&rig, &c.skeleton, &mut pose, &mut globals, c.transform, Mat4::IDENTITY, Vec3::ZERO, Vec3::ZERO, limp, 1.0 / 60.0);
            let j = joints(&pose, &mut globals);
            for (k, (a, b, cc)) in [(2, (L_UPLEG, L_LEG, L_FOOT)), (3, (R_UPLEG, R_LEG, R_FOOT)), (0, (L_ARM, L_FORE, L_HAND)), (1, (R_ARM, R_FORE, R_HAND))] {
                if doll.hinge[k].length() < 0.5 {
                    continue;
                }
                let f = flexion(&j, &animated, a, b, cc, doll.hinge[k]);
                let out = if f < 0.0 { -f } else { (f - 2.6).max(0.0) };
                if k >= 2 { worst_knee = worst_knee.max(out) } else { worst_elbow = worst_elbow.max(out) }
            }
        }
        // Small residue: the bones follow the particles after the floor.
        assert!(worst_knee < 0.15, "a knee bent {worst_knee} rad the wrong way");
        assert!(worst_elbow < 0.15 || doll.hinge[0].length() < 0.5, "an elbow bent {worst_elbow} rad the wrong way");
        assert!(doll.hinge[2].length() > 0.5, "knee axes learned");
    }

    #[test]
    fn a_limp_body_stays_above_the_floor() {
        let (c, rig) = load();
        let rest = c.skeleton.rest_pose();
        let mut globals = Vec::new();
        let mut doll = Ragdoll::default();
        let limp = Drive { torso: 0.2, head: 0.1, arms: 0.05, legs: 0.05, inertia: 1.0 };
        // The body space floor sits 0.9 m below the root.
        let root = vec3(0.0, 0.0, 0.0);
        for _ in 0..240 {
            let mut pose = rest.clone();
            doll.apply(&rig, &c.skeleton, &mut pose, &mut globals, c.transform, Mat4::IDENTITY, root, Vec3::ZERO, limp, 1.0 / 60.0);
            c.skeleton.global_matrices(&pose, &mut globals);
            for &bone in &rig.bones[1..] {
                let y = c.transform.transform_point(globals[bone].transform_point(Vec3::ZERO)).y;
                assert!(y > -0.02, "bone {bone} under the floor at {y}");
            }
        }
    }
}
