//! Planted feet. While a fighter stands, guards, crouches or walks, each
//! foot stays where it was put on the floor; when the body has moved too far
//! from it, the foot takes a quick arcing step to where it belongs (a little
//! ahead when moving), one foot at a time, the leading one first. The legs
//! reach their feet with two-bone IK, the feet keep the animated angle. No
//! foot slides, whatever the walking speed. Cosmetic: the simulation owns
//! the body's position.
use super::ragdoll::turn_bone;
use crate::engine::{math3::*, skeleton::{Pose, Skeleton}};

#[derive(Clone, Copy)]
struct Swing {
    from: Vec3,
    t: f32,
    len: f32,
}

#[derive(Clone, Default)]
pub struct Feet {
    /// 0 when the animation owns the legs, 1 when the feet are planted.
    weight: f32,
    /// Where each foot (ankle) stands, in the room.
    planted: [Vec3; 2],
    swing: [Option<Swing>; 2],
    /// Seconds a standing foot has been off its place.
    restless: f32,
    root: Option<Vec3>,
    /// Smoothed root velocity (m/s), to land steps ahead of a moving body.
    velocity: Vec3,
    /// How far the hips sink so both legs reach their feet (m).
    sink: f32,
}

impl Feet {
    pub fn reset(&mut self) {
        *self = Feet::default();
    }

    /// Plants the feet of `pose` (hip, knee, ankle bones per leg; `hips` the
    /// root of the body) when `standing`, otherwise lets go within a tenth of
    /// a second. `body` maps model space into body space, `place` body space
    /// into the room.
    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        &mut self,
        hips: usize,
        legs: [[usize; 3]; 2],
        skeleton: &Skeleton,
        pose: &mut Pose,
        globals: &mut Vec<Mat4>,
        body: Mat4,
        place: Mat4,
        standing: bool,
        dt: f32,
    ) {
        let root = place.transform_point(Vec3::ZERO);
        if let Some(last) = self.root {
            if (root - last).length() > 0.8 {
                // A teleport (a new round): start over.
                self.reset();
            } else if dt > 0.0 {
                let measured = (root - last) * (1.0 / dt);
                self.velocity = self.velocity + (measured - self.velocity) * (1.0 - (-dt / 0.08).exp());
            }
        }
        self.root = Some(root);
        skeleton.global_matrices(pose, globals);
        let world = place * body;
        let ankle = |globals: &Vec<Mat4>, leg: usize| world.transform_point(globals[legs[leg][2]].transform_point(Vec3::ZERO));
        let home = [ankle(globals, 0), ankle(globals, 1)];
        if !standing {
            self.weight = (self.weight - dt * 10.0).max(0.0);
            self.swing = [None, None];
            if self.weight <= 0.0 {
                return;
            }
        } else {
            if self.weight <= 0.0 {
                self.planted = home;
                self.swing = [None, None];
                self.restless = 0.0;
            }
            self.weight = (self.weight + dt * 10.0).min(1.0);
            self.step(home, dt);
        }

        // Where each foot is now: planted, or along its step.
        let speed = self.velocity.x.abs();
        let lift = 0.05 + 0.04 * (speed / 1.6).min(1.0);
        let mut feet = self.planted;
        for i in 0..2 {
            if let Some(s) = self.swing[i] {
                let u = (s.t / s.len).clamp(0.0, 1.0);
                let to = self.landing(home[i], s);
                let e = u * u * (3.0 - 2.0 * u);
                feet[i] = s.from.lerp(to, e) + vec3(0.0, lift * (u * std::f32::consts::PI).sin(), 0.0);
            }
        }
        // The legs reach for them (in body space); the hips sink just
        // enough for both legs to get there.
        let to_model = body.invert();
        let to_body = place.invert();
        let targets = [0, 1].map(|i| to_body.transform_point(home[i].lerp(feet[i], self.weight)));
        let at = |globals: &Vec<Mat4>, bone: usize| body.transform_point(globals[bone].transform_point(Vec3::ZERO));
        let mut sink: f32 = 0.0;
        for i in 0..2 {
            let [up, knee, foot] = legs[i];
            let hip = at(globals, up);
            let length = ((at(globals, knee) - hip).length() + (at(globals, foot) - at(globals, knee)).length()) * 0.97;
            let d = targets[i] - hip;
            let across = (d.x * d.x + d.z * d.z).sqrt().min(length);
            let fits = (length * length - across * across).sqrt();
            sink = sink.max(-d.y - fits);
        }
        let sink = sink.clamp(0.0, 0.15) * self.weight;
        // Sinks at once when a leg needs it (a foot never slides for want of
        // reach), rises back gently.
        self.sink = if sink > self.sink { sink } else { self.sink + (sink - self.sink) * (1.0 - (-dt * 12.0).exp()) };
        if self.sink > 1e-4 {
            let down = to_model.transform_direction(vec3(0.0, -self.sink, 0.0));
            let parent = skeleton.bones[hips].parent.map_or(Mat4::IDENTITY, |b| globals[b]);
            pose.locals[hips].translation += parent.invert().transform_direction(down);
            skeleton.global_matrices(pose, globals);
        }
        for i in 0..2 {
            reach(skeleton, pose, globals, body, to_model, legs[i], targets[i]);
        }
    }

    /// Where a step lands: the foot's place, ahead of a moving body by the
    /// rest of the step and half the time it will stand there, so it stands
    /// centred under its place.
    fn landing(&self, home: Vec3, s: Swing) -> Vec3 {
        let ahead = self.velocity * ((s.len - s.t).max(0.0) + s.len * 0.5);
        vec3(home.x + ahead.x, home.y, home.z)
    }

    fn step(&mut self, home: [Vec3; 2], dt: f32) {
        let speed = self.velocity.x.abs();
        // Short quick steps keep the stance: a fighter shuffles, never
        // brings the feet together.
        let len = (0.22 - 0.03 * speed).clamp(0.16, 0.22);
        // Steps in flight move on; a finished one plants the foot.
        for i in 0..2 {
            if let Some(mut s) = self.swing[i] {
                s.t += dt;
                if s.t >= s.len {
                    self.planted[i] = self.landing(home[i], s);
                    self.swing[i] = None;
                } else {
                    self.swing[i] = Some(s);
                }
            }
        }
        let off = |i: usize| {
            let d = home[i] - self.planted[i];
            (d.x * d.x + d.z * d.z).sqrt()
        };
        let moving = speed > 0.2;
        let worst = off(0).max(off(1));
        self.restless = if worst > 0.05 { self.restless + dt } else { 0.0 };
        let threshold = if moving { 0.03 + 0.5 * speed * len } else { 0.05 };
        let swinging = self.swing.iter().filter(|s| s.is_some()).count();
        // One foot at a time, unless the body is running away from both.
        let allowed = swinging == 0 || (swinging == 1 && worst > 0.5);
        if !allowed || worst < threshold || (!moving && self.restless < 0.15) {
            return;
        }
        // The foot that leads the way goes first; standing, the worst placed.
        let lead = |i: usize| home[i].x * self.velocity.x.signum();
        let pick = (0..2)
            .filter(|&i| self.swing[i].is_none() && off(i) >= threshold * 0.8)
            .max_by(|&a, &b| {
                let score = |i: usize| if moving { lead(i) + off(i) * 0.5 } else { off(i) };
                score(a).total_cmp(&score(b))
            });
        if let Some(i) = pick {
            self.swing[i] = Some(Swing { from: self.planted[i], t: 0.0, len });
        }
    }
}

/// Two-bone IK: turns the hip and knee of `leg` so the ankle reaches
/// `target` (body space), keeping the knee in its animated plane and the
/// foot at its animated angle.
fn reach(skeleton: &Skeleton, pose: &mut Pose, globals: &mut Vec<Mat4>, body: Mat4, to_model: Mat4, leg: [usize; 3], target: Vec3) {
    let [up, knee, foot] = leg;
    let at = |globals: &Vec<Mat4>, bone: usize| body.transform_point(globals[bone].transform_point(Vec3::ZERO));
    let foot_angle = Quat::from_matrix(globals[foot]);
    let (hip, k, a) = (at(globals, up), at(globals, knee), at(globals, foot));
    let (upper, lower) = ((k - hip).length(), (a - k).length());
    let to = target - hip;
    if to.length() < 1e-4 || upper < 1e-4 || lower < 1e-4 {
        return;
    }
    let dir = to.normalize();
    let reach = to.length().clamp((upper - lower).abs() + 1e-3, upper + lower - 1e-3);
    // The knee bends where the animation bends it.
    let bend = k - hip;
    let mut pole = bend - dir * bend.dot(dir);
    if pole.length() < 1e-4 {
        pole = vec3(0.0, 0.0, 1.0);
    }
    let pole = pole.normalize();
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
    fn walking_feet_never_slide_and_the_legs_reach_them() {
        let c = Character::from_model(gltf::load_glb(&std::fs::read("assets/character.glb").unwrap()).unwrap());
        let rig = RagdollRig::new(&c.skeleton).unwrap();
        let legs = [rig.leg(0), rig.leg(1)];
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
        // Walk forward at the game's speed (26 mm/tick) for two seconds.
        for frame in 0..120 {
            let x = frame as f32 * 0.026;
            let place = Mat4::translation(vec3(x, 0.0, 0.0));
            let mut pose = rest.clone();
            let hips = c.skeleton.bones.iter().position(|b| b.name.ends_with("Hips")).unwrap();
            feet.apply(hips, legs, &c.skeleton, &mut pose, &mut globals, c.transform, place, true, 1.0 / 60.0);
            let now = [ankle(&pose, &mut globals, place, 0), ankle(&pose, &mut globals, place, 1)];
            for i in 0..2 {
                let planted = feet.swing[i].is_none();
                if planted && !planted_before[i] {
                    steps += 1;
                }
                // After the first tenth of a second the feet own the legs.
                if let (true, true, Some(p), true) = (planted, planted_before[i], previous, frame > 6) {
                    let slide = (now[i] - p[i]).length();
                    assert!(slide < 0.004, "foot {i} slid {slide} m at frame {frame}");
                }
                planted_before[i] = planted;
            }
            previous = Some(now);
        }
        assert!(steps >= 6, "only {steps} steps in two seconds of walking");
    }
}
