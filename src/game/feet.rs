//! Foot locking. In every state on the ground (stance, guard, strikes,
//! reactions, dances) a foot the animation puts down on the floor, and keeps
//! still under the body, is locked where it touched down; the leg reaches it
//! with two-bone IK while the animation and the simulation move the body
//! over it. A foot the animation lifts or swings fast (a kick, a dance step)
//! follows the animation and locks again where it lands. When the body has
//! moved too far from a locked foot (a shove, a slide), the foot takes a
//! short arcing step back under it. Knees only bend forward. Cosmetic: the
//! simulation owns the body's position.
use super::ragdoll::turn_bone;
use crate::engine::{math3::*, skeleton::{Pose, Skeleton}};

/// A toe this close to the floor (m) is down; above `LIFT` it is lifted.
const DOWN: f32 = 0.035;
const LIFT: f32 = 0.07;
/// A locked foot is let go when the animation lifts it, or, in a sweep, when
/// it drives along the floor faster than `SWEEP` (and then only a foot
/// slower than `STILL`, m/s relative to the body, locks).
const STILL: f32 = 0.9;
const SWEEP: f32 = 2.0;
/// A locked foot this far (m) from where the animation wants it steps.
const DRIFT: f32 = 0.2;
/// Seconds of a recovery step.
const STEP: f32 = 0.16;

#[derive(Clone, Copy)]
struct Swing {
    from: Vec3,
    t: f32,
}

#[derive(Clone, Default)]
pub struct Feet {
    /// Per foot: 0 when the animation owns it, 1 when it is locked.
    pub(crate) weight: [f32; 2],
    /// Where each locked foot's toe stands, in the room (the heel may rise).
    pub(crate) locked: [Option<Vec3>; 2],
    swing: [Option<Swing>; 2],
    /// Last frame's animated ankle in body space, to tell a still foot.
    last: Option<[Vec3; 2]>,
    root: Option<Vec3>,
    /// How far the hips sink so both legs reach their feet (m).
    sink: f32,
    /// Each knee's hinge axis in the knee bone's own frame, learned from the
    /// animation whenever it bends the knee (the way it flexes is positive).
    hinge: [Option<Vec3>; 2],
}

impl Feet {
    pub fn reset(&mut self) {
        *self = Feet::default();
    }

    /// Locks the feet of `pose` (hip, knee, ankle bones per leg, `toes` the
    /// toe bones; `hips` the root of the body) while `grounded`, otherwise
    /// lets go within a tenth of a second; `sweeping`: a foot may drive along
    /// the floor. `body` maps model space into body space, `place` body space
    /// into the room.
    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        &mut self,
        hips: usize,
        legs: [[usize; 3]; 2],
        toes: [usize; 2],
        skeleton: &Skeleton,
        pose: &mut Pose,
        globals: &mut Vec<Mat4>,
        body: Mat4,
        place: Mat4,
        grounded: bool,
        sweeping: bool,
        dt: f32,
    ) {
        let root = place.transform_point(Vec3::ZERO);
        if self.root.is_some_and(|last| (root - last).length() > 0.8) {
            // A teleport (a new round): start over.
            self.reset();
        }
        self.root = Some(root);
        skeleton.global_matrices(pose, globals);
        let in_body = |globals: &Vec<Mat4>, bone: usize| body.transform_point(globals[bone].transform_point(Vec3::ZERO));
        let toes_body = [in_body(globals, toes[0]), in_body(globals, toes[1])];
        let ankles = [in_body(globals, legs[0][2]), in_body(globals, legs[1][2])];
        // The animated toe in the room and the ankle's offset from it.
        let home = [place.transform_point(toes_body[0]), place.transform_point(toes_body[1])];
        let heel = [0, 1].map(|i| place.transform_direction(ankles[i] - toes_body[i]));
        let toe_y = [home[0].y, home[1].y];
        let speed = match (self.last, dt > 0.0) {
            (Some(last), true) => [0, 1].map(|i| {
                let d = toes_body[i] - last[i];
                (d.x * d.x + d.z * d.z).sqrt() / dt
            }),
            _ => [0.0; 2],
        };
        self.last = Some(toes_body);
        // Learn the knee hinges from the animated bends.
        for i in 0..2 {
            let [up, knee, foot] = legs[i];
            let (hip, k, a) = (in_body(globals, up), in_body(globals, knee), in_body(globals, foot));
            let bend = (k - hip).normalize().cross((a - k).normalize());
            if bend.length() > 0.3 {
                let frame = Quat::from_matrix(body * globals[knee]);
                self.hinge[i] = Some(frame.conjugate().rotate(bend.normalize()));
            }
        }
        let fade = (dt / 0.1).min(1.0);
        for i in 0..2 {
            // A foot on the floor locks (one the animation shuffles along it
            // too: it then steps); in a sweep only a still one does.
            let down = toe_y[i] < DOWN && (!sweeping || speed[i] < STILL);
            let lifted = toe_y[i] > LIFT || (sweeping && speed[i] > SWEEP);
            if !grounded || lifted {
                // The animation takes the foot (and lifts it).
                self.swing[i] = None;
                self.weight[i] = (self.weight[i] - fade).max(0.0);
                if self.weight[i] <= 0.0 {
                    self.locked[i] = None;
                }
                continue;
            }
            if self.locked[i].is_none() || self.weight[i] < 1.0 {
                if !down {
                    // Still letting go after a lift.
                    self.weight[i] = (self.weight[i] - fade).max(0.0);
                    if self.weight[i] <= 0.0 {
                        self.locked[i] = None;
                    }
                    continue;
                }
                // Touchdown: lock where the foot is shown now, at once (it is
                // there already, so nothing jumps).
                let shown = match self.locked[i] {
                    Some(at) => home[i].lerp(at, self.weight[i]),
                    None => home[i],
                };
                self.locked[i] = Some(vec3(shown.x, home[i].y, shown.z));
                self.weight[i] = 1.0;
            }
        }
        // A leg that cannot reach its locked foot without straightening
        // fully steps instead of stretching like a stilt.
        let stretched = [0, 1].map(|i| match self.locked[i] {
            Some(at) if self.swing[i].is_none() => {
                let [up, knee, foot] = legs[i];
                let hip = place.transform_point(in_body(globals, up));
                let length = (in_body(globals, knee) - in_body(globals, up)).length() + (in_body(globals, foot) - in_body(globals, knee)).length();
                (at + heel[i] - hip).length() > length * 0.97
            }
            _ => false,
        });
        self.step(home, stretched, dt);
        if self.weight.iter().all(|w| *w <= 0.0) {
            self.sink *= (1.0 - dt * 12.0).max(0.0);
            return;
        }

        // Where each foot is now: locked, or along its step.
        let mut feet = home;
        for i in 0..2 {
            let Some(at) = self.locked[i] else { continue };
            feet[i] = match self.swing[i] {
                Some(s) => {
                    let u = (s.t / STEP).clamp(0.0, 1.0);
                    let e = u * u * (3.0 - 2.0 * u);
                    s.from.lerp(home[i], e) + vec3(0.0, 0.07 * (u * std::f32::consts::PI).sin(), 0.0)
                }
                None => at,
            };
        }
        // The legs reach for them (in body space); the hips sink just
        // enough for both legs to get there.
        let to_model = body.invert();
        let to_body = place.invert();
        let targets = [0, 1].map(|i| to_body.transform_point(home[i].lerp(feet[i], self.weight[i]) + heel[i]));
        let sink = sink_needed(globals, body, legs, targets);
        // Sinks at once when a leg needs it (a foot never slides for want of
        // reach), rises back gently.
        self.sink = if sink > self.sink { sink } else { self.sink + (sink - self.sink) * (1.0 - (-dt * 12.0).exp()) };
        lower_hips(skeleton, pose, globals, to_model, hips, self.sink);
        for i in 0..2 {
            if self.weight[i] > 0.0 {
                reach(skeleton, pose, globals, body, to_model, legs[i], self.hinge[i], targets[i]);
            }
        }
    }

    /// A locked foot left too far behind (or ahead), or out of the leg's
    /// reach (`stretched`), steps back under the body, one foot at a time; a
    /// finished step locks it there.
    fn step(&mut self, home: [Vec3; 2], stretched: [bool; 2], dt: f32) {
        for i in 0..2 {
            if let Some(mut s) = self.swing[i] {
                s.t += dt;
                if s.t >= STEP {
                    self.locked[i] = Some(home[i]);
                    self.swing[i] = None;
                } else {
                    self.swing[i] = Some(s);
                }
            }
        }
        if self.swing.iter().any(|s| s.is_some()) {
            return;
        }
        let off = |i: usize| match self.locked[i] {
            Some(at) if self.weight[i] >= 1.0 => {
                let d = home[i] - at;
                (d.x * d.x + d.z * d.z).sqrt() + if stretched[i] { DRIFT } else { 0.0 }
            }
            _ => 0.0,
        };
        let pick = (0..2).filter(|&i| off(i) > DRIFT).max_by(|&a, &b| off(a).total_cmp(&off(b)));
        if let Some(i) = pick {
            self.swing[i] = Some(Swing { from: self.locked[i].unwrap(), t: 0.0 });
        }
    }
}

/// How far the hips must come down for both ankles to reach their targets.
fn sink_needed(globals: &[Mat4], body: Mat4, legs: [[usize; 3]; 2], targets: [Vec3; 2]) -> f32 {
    let at = |bone: usize| body.transform_point(globals[bone].transform_point(Vec3::ZERO));
    let mut sink: f32 = 0.0;
    for i in 0..2 {
        let [up, knee, foot] = legs[i];
        let hip = at(up);
        let length = ((at(knee) - hip).length() + (at(foot) - at(knee)).length()) * 0.97;
        let d = targets[i] - hip;
        let across = (d.x * d.x + d.z * d.z).sqrt().min(length);
        let fits = (length * length - across * across).sqrt();
        sink = sink.max(-d.y - fits);
    }
    sink.clamp(0.0, 0.15)
}

fn lower_hips(skeleton: &Skeleton, pose: &mut Pose, globals: &mut Vec<Mat4>, to_model: Mat4, hips: usize, sink: f32) {
    if sink <= 1e-4 {
        return;
    }
    let down = to_model.transform_direction(vec3(0.0, -sink, 0.0));
    let parent = skeleton.bones[hips].parent.map_or(Mat4::IDENTITY, |b| globals[b]);
    pose.locals[hips].translation += parent.invert().transform_direction(down);
    skeleton.global_matrices(pose, globals);
}

/// Two-bone IK: turns the hip and knee of `leg` so the ankle reaches
/// `target` (body space), the knee bending about its hinge (`hinge`, in the
/// knee bone's frame) only the way it flexes, the foot at its animated angle.
#[allow(clippy::too_many_arguments)]
fn reach(skeleton: &Skeleton, pose: &mut Pose, globals: &mut Vec<Mat4>, body: Mat4, to_model: Mat4, leg: [usize; 3], hinge: Option<Vec3>, target: Vec3) {
    let [up, knee, foot] = leg;
    let at = |globals: &Vec<Mat4>, bone: usize| body.transform_point(globals[bone].transform_point(Vec3::ZERO));
    let axis = hinge.map(|h| Quat::from_matrix(body * globals[knee]).rotate(h));
    let foot_angle = Quat::from_matrix(globals[foot]);
    let (hip, k, a) = (at(globals, up), at(globals, knee), at(globals, foot));
    let (upper, lower) = ((k - hip).length(), (a - k).length());
    let to = target - hip;
    if to.length() < 1e-4 || upper < 1e-4 || lower < 1e-4 {
        return;
    }
    let dir = to.normalize();
    let reach = to.length().clamp((upper - lower).abs() + 1e-3, upper + lower - 1e-3);
    // The knee bends about its hinge, the way it flexes (a pole of
    // dir x axis gives a positive flexion about the axis); before the hinge
    // is known, where the animation bends it.
    let bend = k - hip;
    let anim = bend - dir * bend.dot(dir);
    let pole = match axis.map(|a| dir.cross(a)).filter(|p| p.length() > 0.1) {
        Some(p) => p.normalize(),
        None if anim.length() > 1e-3 => anim.normalize(),
        None => vec3(0.0, 0.0, 1.0),
    };
    let cos = ((upper * upper + reach * reach - lower * lower) / (2.0 * upper * reach)).clamp(-1.0, 1.0);
    let knee_at = hip + dir * (upper * cos) + pole * (upper * (1.0 - cos * cos).sqrt());
    turn_bone(skeleton, pose, globals, body, to_model, up, Quat::from_rotation_arc(k - hip, knee_at - hip), hip);
    let (k, a) = (at(globals, knee), at(globals, foot));
    turn_bone(skeleton, pose, globals, body, to_model, knee, Quat::from_rotation_arc(a - k, hip + dir * reach - k), k);
    // The foot keeps its animated angle.
    let parent = Quat::from_matrix(globals[knee]);
    pose.locals[foot].rotation = (parent.conjugate() * foot_angle).normalize();
    skeleton.global_matrices(pose, globals);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::gltf;
    use crate::game::{dancer::Character, ragdoll::RagdollRig};

    #[test]
    fn a_body_pushed_over_locked_feet_steps_and_never_slides() {
        let c = Character::from_model(gltf::load_glb(&std::fs::read("assets/character.glb").unwrap()).unwrap());
        let rig = RagdollRig::new(&c.skeleton).unwrap();
        let legs = [rig.leg(0), rig.leg(1)];
        let find = |n: &str| c.skeleton.bones.iter().position(|b| b.name.ends_with(n)).unwrap();
        let (hips, toes) = (find("Hips"), [find("LeftToeBase"), find("RightToeBase")]);
        let rest = c.skeleton.rest_pose();
        let mut globals = Vec::new();
        let mut feet = Feet::default();
        let ankle = |pose: &Pose, globals: &mut Vec<Mat4>, place: Mat4, leg: usize| {
            c.skeleton.global_matrices(pose, globals);
            (place * c.transform).transform_point(globals[legs[leg][2]].transform_point(Vec3::ZERO))
        };
        let mut previous: Option<[Vec3; 2]> = None;
        let mut steps = 0;
        let mut planted_before = [true; 2];
        // Shoved along at 1 m/s for two seconds, the stance as animated.
        for frame in 0..120 {
            let x = frame as f32 * 0.017;
            let place = Mat4::translation(vec3(x, 0.0, 0.0));
            let mut pose = rest.clone();
            feet.apply(hips, legs, toes, &c.skeleton, &mut pose, &mut globals, c.transform, place, true, false, 1.0 / 60.0);
            let now = [ankle(&pose, &mut globals, place, 0), ankle(&pose, &mut globals, place, 1)];
            for i in 0..2 {
                let planted = feet.swing[i].is_none() && feet.weight[i] >= 1.0;
                if planted && !planted_before[i] {
                    steps += 1;
                }
                if let (true, true, Some(p)) = (planted, planted_before[i], previous) {
                    let slide = (now[i] - p[i]).length();
                    assert!(slide < 0.004, "foot {i} slid {slide} m at frame {frame}");
                }
                planted_before[i] = planted;
            }
            previous = Some(now);
        }
        assert!(steps >= 4, "only {steps} recovery steps");
    }
}
