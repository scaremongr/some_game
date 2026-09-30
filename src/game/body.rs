//! Pose solver for the fighter rig.
//!
//! Animations are authored as body-part targets in character space: pelvis,
//! torso and head angles, wrist and ball-of-foot positions, elbow/knee pole
//! directions, palm and foot orientation. The solver turns them into bone
//! rotations with two-bone IK that fixes the *whole* bone frame, not only its
//! direction: the elbow and knee always bend in their anatomical plane and the
//! forearm twist follows the requested palm direction. The old retargeter
//! turned each bone by the shortest arc, which let limbs spin around their own
//! axis and made elbows fold sideways.
//!
//! Character space: +Y up, the fighter faces +Z, its left side is +X, the
//! floor is y = 0. Canonical poses keep the lead limbs on the left; the right
//! side of the screen plays them mirrored.
use super::character::bone;
use crate::engine::{
    math3::*,
    skeleton::{Pose, Skeleton},
};

pub const LEAD: usize = 0;
pub const REAR: usize = 1;

/// Euler angles are stored as (yaw, pitch, roll): yaw turns about +Y,
/// positive pitch leans forward, positive roll tips the top toward -X.
pub fn euler(e: Vec3) -> Quat {
    Quat::from_euler(e.x, e.y, e.z)
}

#[derive(Clone, Copy, Debug)]
pub struct BodyPose {
    /// Pelvis joint position.
    pub hips: Vec3,
    pub pelvis: Vec3,
    /// Relative to the pelvis, spread over the three spine bones.
    pub torso: Vec3,
    /// Relative to the chest, added to the automatic look-at.
    pub head: Vec3,
    /// 0 — head follows the chest, 1 — head tracks the opponent.
    pub look: f32,
    /// Wrist targets, [left, right].
    pub hand: [Vec3; 2],
    /// Where the elbow points.
    pub elbow: [Vec3; 2],
    /// Palm normal; drives forearm twist.
    pub palm: [Vec3; 2],
    /// Wrist flexion toward the palm, radians.
    pub wrist: [f32; 2],
    /// 1 — tight fist, 0 — open hand.
    pub fist: [f32; 2],
    /// Clavicle (raise, forward, 0), radians.
    pub clav: [Vec3; 2],
    /// Ball-of-foot targets.
    pub foot: [Vec3; 2],
    /// Foot (yaw, pitch, roll); positive pitch lifts the heel / points toes.
    pub foot_rot: [Vec3; 2],
    /// Where the knee points.
    pub knee: [Vec3; 2],
    /// Whole-body rotation about the hips (flips, falls, lying).
    pub spin: Vec3,
    /// Translation applied after `spin`.
    pub shift: Vec3,
}

/// A value that can be blended linearly. Poses are blended per channel.
pub trait Channel: Copy {
    fn scale(self, w: f32) -> Self;
    fn plus(self, o: Self) -> Self;
}
impl Channel for f32 {
    fn scale(self, w: f32) -> f32 {
        self * w
    }
    fn plus(self, o: f32) -> f32 {
        self + o
    }
}
impl Channel for Vec3 {
    fn scale(self, w: f32) -> Vec3 {
        self * w
    }
    fn plus(self, o: Vec3) -> Vec3 {
        self + o
    }
}
impl<T: Channel> Channel for [T; 2] {
    fn scale(self, w: f32) -> [T; 2] {
        [self[0].scale(w), self[1].scale(w)]
    }
    fn plus(self, o: [T; 2]) -> [T; 2] {
        [self[0].plus(o[0]), self[1].plus(o[1])]
    }
}

macro_rules! weighted_pose {
    ($items:expr; $($field:ident),*) => {{
        let items = $items;
        let (first, w0) = items[0];
        BodyPose {
            $($field: items[1..].iter().fold(first.$field.scale(w0), |acc, (p, w)| acc.plus(p.$field.scale(*w))),)*
        }
    }};
}

impl BodyPose {
    /// Linear combination of poses. Weights need not sum to one (Hermite).
    pub fn weighted(items: &[(&BodyPose, f32)]) -> BodyPose {
        weighted_pose!(items; hips, pelvis, torso, head, look, hand, elbow, palm, wrist, fist,
            clav, foot, foot_rot, knee, spin, shift)
    }
    pub fn lerp(&self, o: &BodyPose, t: f32) -> BodyPose {
        BodyPose::weighted(&[(self, 1.0 - t), (o, t)])
    }
    pub fn with(mut self, f: impl FnOnce(&mut BodyPose)) -> BodyPose {
        f(&mut self);
        self
    }
    /// Mirror across the character's sagittal plane: lead limbs switch sides.
    pub fn mirrored(&self) -> BodyPose {
        let m = |v: Vec3| vec3(-v.x, v.y, v.z);
        let e = |v: Vec3| vec3(-v.x, v.y, -v.z);
        let sw = |a: [Vec3; 2], f: &dyn Fn(Vec3) -> Vec3| [f(a[1]), f(a[0])];
        BodyPose {
            hips: m(self.hips),
            pelvis: e(self.pelvis),
            torso: e(self.torso),
            head: e(self.head),
            look: self.look,
            hand: sw(self.hand, &m),
            elbow: sw(self.elbow, &m),
            palm: sw(self.palm, &m),
            wrist: [self.wrist[1], self.wrist[0]],
            fist: [self.fist[1], self.fist[0]],
            clav: [self.clav[1], self.clav[0]],
            foot: sw(self.foot, &m),
            foot_rot: sw(self.foot_rot, &e),
            knee: sw(self.knee, &m),
            spin: e(self.spin),
            shift: m(self.shift),
        }
    }
    /// Moves the whole figure; used by lunges and knock-back.
    pub fn translated(mut self, d: Vec3) -> BodyPose {
        self.hips += d;
        for i in 0..2 {
            self.hand[i] += d;
            self.foot[i] += d;
        }
        self
    }
}

/// Bone indices and rest data in character space.
pub struct BodyRig {
    parent: Vec<Option<usize>>,
    rest_locals: Vec<Transform>,
    rest_rot: Vec<Quat>,
    rest_pos: Vec<Vec3>,
    to_c: Mat4,
    from_c: Mat4,
    rt: Quat,
    scale: f32,
    hips: usize,
    spine: [usize; 3],
    neck: usize,
    head: usize,
    clav: [usize; 2],
    arm: [usize; 2],
    fore: [usize; 2],
    hand: [usize; 2],
    thigh: [usize; 2],
    shin: [usize; 2],
    foot: [usize; 2],
    toe: [usize; 2],
    fingers: [Vec<usize>; 2],
    thumbs: [Vec<usize>; 2],
    /// Local rotation of the first thumb bone that tucks it into a fist.
    thumb_fold: [Quat; 2],
    upper_len: [f32; 2],
    fore_len: [f32; 2],
    thigh_len: [f32; 2],
    shin_len: [f32; 2],
    /// Rest ankle relative to the ball of the foot.
    ankle_from_ball: [Vec3; 2],
    pub rest_hips: Vec3,
}

/// Rotation taking the orthonormal frame (a0, b0) onto (a1, b1).
fn frame_rotation(a0: Vec3, b0: Vec3, a1: Vec3, b1: Vec3) -> Quat {
    let c0 = a0.cross(b0);
    let c1 = a1.cross(b1);
    let mut m = [0.0f32; 16];
    let (a0, b0, c0) = (a0.to_array(), b0.to_array(), c0.to_array());
    let (a1, b1, c1) = (a1.to_array(), b1.to_array(), c1.to_array());
    for row in 0..3 {
        for col in 0..3 {
            m[col * 4 + row] = a1[row] * a0[col] + b1[row] * b0[col] + c1[row] * c0[col];
        }
    }
    m[15] = 1.0;
    Quat::from_matrix(Mat4(m))
}
fn perpendicular(v: Vec3, axis: Vec3) -> Vec3 {
    v - axis * v.dot(axis)
}
fn any_perpendicular(v: Vec3) -> Vec3 {
    let far = if v.x.abs() < 0.8 { Vec3::X } else { Vec3::Z };
    v.cross(far).normalize()
}
fn orthonormal(v: Vec3, axis: Vec3, fallback: Vec3) -> Vec3 {
    let p = perpendicular(v, axis);
    if p.length() > 1e-4 {
        p.normalize()
    } else {
        let f = perpendicular(fallback, axis);
        if f.length() > 1e-4 {
            f.normalize()
        } else {
            any_perpendicular(axis)
        }
    }
}
fn slerp_identity(q: Quat, t: f32) -> Quat {
    Quat::IDENTITY.slerp(q, t)
}

/// Two-bone IK: joint position and reachable end for root `s`.
fn two_bone(s: Vec3, target: Vec3, pole: Vec3, l1: f32, l2: f32) -> (Vec3, Vec3) {
    let d = target - s;
    let len = d.length().max(1e-4);
    let dir = d * (1.0 / len);
    let reach = len.clamp((l1 - l2).abs() + 1e-3, (l1 + l2) * 0.9995);
    let cos_a = ((l1 * l1 + reach * reach - l2 * l2) / (2.0 * l1 * reach)).clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
    let side = orthonormal(pole, dir, any_perpendicular(dir));
    let joint = s + dir * (l1 * cos_a) + side * (l1 * sin_a);
    (joint, s + dir * reach)
}

impl BodyRig {
    pub fn new(skeleton: &Skeleton, to_c: Mat4) -> Result<BodyRig, String> {
        let find = |names: &[&str], what: &str| {
            skeleton
                .find_like(names)
                .ok_or_else(|| format!("fighter rig has no {what} bone"))
        };
        let exact = |name: &str| {
            skeleton
                .bones
                .iter()
                .position(|b| b.name.ends_with(name))
                .ok_or_else(|| format!("fighter rig has no {name} bone"))
        };
        let rest = skeleton.rest_pose();
        let n = skeleton.len();
        let rt = Quat::from_matrix(to_c);
        let scale = to_c.transform_direction(Vec3::X).length();
        let mut rest_rot = vec![Quat::IDENTITY; n];
        let mut rest_pos = vec![Vec3::ZERO; n];
        for (i, b) in skeleton.bones.iter().enumerate() {
            let l = rest.locals[i];
            match b.parent {
                None => {
                    rest_rot[i] = (rt * l.rotation).normalize();
                    rest_pos[i] = to_c.transform_point(l.translation);
                }
                Some(p) => {
                    rest_rot[i] = (rest_rot[p] * l.rotation).normalize();
                    rest_pos[i] = rest_pos[p] + rest_rot[p].rotate(l.translation) * scale;
                }
            }
        }
        let side = |left: &str, right: &str| -> Result<[usize; 2], String> {
            Ok([exact(left)?, exact(right)?])
        };
        let fingers = |hand: &str| -> [Vec<usize>; 2] {
            let pick = |thumb: bool| {
                skeleton
                    .bones
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| {
                        b.name.contains(hand)
                            && !b.name.ends_with('4')
                            && if thumb {
                                b.name.contains("Thumb")
                            } else {
                                ["Index", "Middle", "Ring", "Pinky"]
                                    .iter()
                                    .any(|f| b.name.contains(f))
                            }
                    })
                    .map(|(i, _)| i)
                    .collect()
            };
            [pick(false), pick(true)]
        };
        let [lf, lt] = fingers("LeftHand");
        let [rf, rt_] = fingers("RightHand");
        let hips = find(bone::HIPS, "hips")?;
        let arm = side("LeftArm", "RightArm")?;
        let fore = side("LeftForeArm", "RightForeArm")?;
        let hand = side("LeftHand", "RightHand")?;
        let thigh = side("LeftUpLeg", "RightUpLeg")?;
        let shin = side("LeftLeg", "RightLeg")?;
        let foot = side("LeftFoot", "RightFoot")?;
        let toe = side("LeftToeBase", "RightToeBase")?;
        let dist = |a: usize, b: usize| (rest_pos[a] - rest_pos[b]).length();
        let thumb_fold = |thumbs: &Vec<usize>, index: usize| -> Quat {
            let (Some(&t1), Some(&t2)) = (thumbs.first(), thumbs.get(1)) else {
                return Quat::IDENTITY;
            };
            // Across the front of the curled fingers, just below the knuckles.
            let along = rest_pos[index].x.signum();
            let across = rest_pos[index] + vec3(0.004 * along, -0.035, -0.012);
            let from = rest_pos[t2] - rest_pos[t1];
            let to = across - rest_pos[t1];
            let world = Quat::from_rotation_arc(from, to);
            (rest_rot[t1].conjugate() * world * rest_rot[t1]).normalize()
        };
        let index_of = |name: &str| skeleton.bones.iter().position(|b| b.name.ends_with(name));
        let thumb_fold = [
            index_of("LeftHandIndex1").map_or(Quat::IDENTITY, |i| thumb_fold(&lt, i)),
            index_of("RightHandIndex1").map_or(Quat::IDENTITY, |i| thumb_fold(&rt_, i)),
        ];
        let rig = BodyRig {
            parent: skeleton.bones.iter().map(|b| b.parent).collect(),
            rest_locals: rest.locals.clone(),
            to_c,
            from_c: to_c.invert(),
            rt,
            scale,
            hips,
            spine: [exact("Spine")?, exact("Spine1")?, exact("Spine2")?],
            neck: exact("Neck")?,
            head: find(bone::HEAD, "head")?,
            clav: side("LeftShoulder", "RightShoulder")?,
            arm,
            fore,
            hand,
            thigh,
            shin,
            foot,
            toe,
            fingers: [lf, rf],
            thumbs: [lt, rt_],
            thumb_fold,
            upper_len: [dist(arm[0], fore[0]), dist(arm[1], fore[1])],
            fore_len: [dist(fore[0], hand[0]), dist(fore[1], hand[1])],
            thigh_len: [dist(thigh[0], shin[0]), dist(thigh[1], shin[1])],
            shin_len: [dist(shin[0], foot[0]), dist(shin[1], foot[1])],
            ankle_from_ball: [
                rest_pos[foot[0]] - rest_pos[toe[0]],
                rest_pos[foot[1]] - rest_pos[toe[1]],
            ],
            rest_hips: rest_pos[hips],
            rest_rot,
            rest_pos,
        };
        Ok(rig)
    }

    /// Closes the fingers of hand `s` into a fist (1) or leaves them open (0).
    pub fn curl_fingers(&self, out: &mut Pose, s: usize, fist: f32) {
        let curl = fist.clamp(0.0, 1.0);
        for &b in &self.fingers[s] {
            out.locals[b].rotation =
                (self.rest_locals[b].rotation * Quat::from_axis_angle(Vec3::X, 1.45 * curl)).normalize();
        }
        // The thumb folds across the curled index and middle fingers.
        for (k, &b) in self.thumbs[s].iter().enumerate() {
            let fold = if k == 0 {
                slerp_identity(self.thumb_fold[s], curl)
            } else {
                Quat::from_axis_angle(Vec3::X, 0.35 * curl)
            };
            out.locals[b].rotation = (self.rest_locals[b].rotation * fold).normalize();
        }
    }

    /// Bone of a limb end: 0/1 left/right hand, 2/3 left/right foot.
    pub fn limb_bone(&self, limb: usize) -> Option<usize> {
        match limb {
            0 | 1 => Some(self.hand[limb]),
            2 | 3 => Some(self.foot[limb - 2]),
            _ => None,
        }
    }

    fn fk(&self, locals: &[Transform], p: &mut [Vec3], q: &mut [Quat]) {
        for i in 0..locals.len() {
            let l = locals[i];
            match self.parent[i] {
                None => {
                    q[i] = (self.rt * l.rotation).normalize();
                    p[i] = self.to_c.transform_point(l.translation);
                }
                Some(pa) => {
                    q[i] = (q[pa] * l.rotation).normalize();
                    p[i] = p[pa] + q[pa].rotate(l.translation) * self.scale;
                }
            }
        }
    }
    fn set_global(&self, out: &mut Pose, q: &mut [Quat], bone: usize, global: Quat) {
        let parent = match self.parent[bone] {
            Some(p) => q[p],
            None => self.rt,
        };
        out.locals[bone].rotation = (parent.conjugate() * global).normalize();
        q[bone] = global;
    }

    /// World-space ball-of-foot position for a pose (for ground checks).
    pub fn solve(&self, pose: &BodyPose, look_target: Vec3, out: &mut Pose) {
        let n = self.parent.len();
        out.locals.clear();
        out.locals.extend_from_slice(&self.rest_locals);
        let mut p = vec![Vec3::ZERO; n];
        let mut q = vec![Quat::IDENTITY; n];
        let spin = euler(pose.spin);
        let pivot = pose.hips;
        let place = |v: Vec3| pivot + spin.rotate(v - pivot) + pose.shift;
        let turn = |v: Vec3| spin.rotate(v);

        // Feet targets first: they decide how low the pelvis must sit.
        let mut foot_rot = [Quat::IDENTITY; 2];
        let mut ball = [Vec3::ZERO; 2];
        let mut ankle = [Vec3::ZERO; 2];
        for s in 0..2 {
            foot_rot[s] = (spin * euler(pose.foot_rot[s])).normalize();
            ball[s] = place(pose.foot[s]);
            ankle[s] = ball[s] + foot_rot[s].rotate(self.ankle_from_ball[s]);
        }
        let pelvis = (spin * euler(pose.pelvis)).normalize();
        let hips_rot = (pelvis * self.rest_rot[self.hips]).normalize();
        // Grounded feet out of reach: first the pelvis sinks (bent knees), at
        // most MAX_SINK, then the foot is dragged toward the body — the way a
        // back foot slides after a long lunge instead of the fighter folding.
        const MAX_SINK: f32 = 0.14;
        let top = pivot.y + pose.shift.y;
        let mut hips_pos = pivot + pose.shift;
        let hip_of = |hips: Vec3, s: usize| {
            hips + pelvis.rotate(self.rest_pos[self.thigh[s]] - self.rest_pos[self.hips])
        };
        for _ in 0..3 {
            let mut lower: f32 = 0.0;
            for s in 0..2 {
                if ball[s].y > 0.09 {
                    continue;
                }
                let d = ankle[s] - hip_of(hips_pos, s);
                let max = (self.thigh_len[s] + self.shin_len[s]) * 0.985;
                let excess = d.length() - max;
                if excess > 0.0 {
                    let vertical = (-d.y / d.length()).max(0.3);
                    lower = lower.max(excess / vertical);
                }
            }
            lower = lower.min(MAX_SINK - (top - hips_pos.y)).max(0.0);
            if lower <= 1e-4 {
                break;
            }
            hips_pos.y -= lower;
        }
        for s in 0..2 {
            if ball[s].y > 0.09 {
                continue;
            }
            let hip = hip_of(hips_pos, s);
            let d = ankle[s] - hip;
            let max = (self.thigh_len[s] + self.shin_len[s]) * 0.98;
            if d.length() > max && max > d.y.abs() {
                let flat = vec3(d.x, 0.0, d.z);
                let allowed = (max * max - d.y * d.y).sqrt();
                if flat.length() > allowed {
                    let pull = flat * (allowed / flat.length()) - flat;
                    ankle[s] += pull;
                    ball[s] += pull;
                }
            }
        }

        // The upper body sinks with the pelvis, so hands keep their place on it.
        let sink = (pivot.y + pose.shift.y - hips_pos.y).max(0.0);
        out.locals[self.hips].translation = self.from_c.transform_point(hips_pos);
        out.locals[self.hips].rotation = (self.rt.conjugate() * hips_rot).normalize();
        q[self.hips] = hips_rot;

        let torso = euler(pose.torso);
        for (k, c) in [0.34, 0.68, 1.0].iter().enumerate() {
            let bone = self.spine[k];
            let g = (pelvis * slerp_identity(torso, *c) * self.rest_rot[bone]).normalize();
            self.set_global(out, &mut q, bone, g);
        }
        let chest = (pelvis * torso).normalize();
        self.fk(&out.locals, &mut p, &mut q);
        // Head: authored angle relative to the chest, blended toward the opponent.
        let head_local = euler(pose.head);
        let neck_pos = p[self.neck];
        let look_dir = (look_target - neck_pos).normalize();
        let look_frame = frame_rotation(
            Vec3::Z,
            Vec3::Y,
            look_dir,
            orthonormal(Vec3::Y, look_dir, Vec3::Y),
        );
        let authored = (chest * head_local).normalize();
        let look = pose.look.clamp(0.0, 1.0);
        let head_world = authored.slerp((look_frame * head_local).normalize(), look);
        let neck_world = chest.slerp(head_world, 0.45);
        self.set_global(out, &mut q, self.neck, (neck_world * self.rest_rot[self.neck]).normalize());
        self.set_global(out, &mut q, self.head, (head_world * self.rest_rot[self.head]).normalize());

        // Clavicles: authored shrug/protraction plus a partial reach toward the hand.
        let mut hand_target = [Vec3::ZERO; 2];
        for s in 0..2 {
            hand_target[s] = place(pose.hand[s]) - Vec3::Y * sink;
            let sign = if s == 0 { 1.0 } else { -1.0 };
            let c = pose.clav[s];
            let authored = Quat::from_axis_angle(Vec3::Z, sign * c.x)
                * Quat::from_axis_angle(Vec3::Y, -sign * c.y);
            let base = (chest * authored).normalize();
            let joint = p[self.clav[s]];
            let tip = joint + base.rotate(self.rest_pos[self.arm[s]] - self.rest_pos[self.clav[s]]);
            let reach = (hand_target[s] - tip).length();
            let extra = ((reach - 0.30) / 0.14).clamp(0.0, 1.0) * 0.30;
            let toward = Quat::from_rotation_arc(tip - joint, hand_target[s] - joint);
            let g = (slerp_identity(toward, extra) * base * self.rest_rot[self.clav[s]]).normalize();
            self.set_global(out, &mut q, self.clav[s], g);
        }
        self.fk(&out.locals, &mut p, &mut q);

        // Arms.
        for s in 0..2 {
            let (arm, fore, hand) = (self.arm[s], self.fore[s], self.hand[s]);
            let shoulder = p[arm];
            let pole = turn(pose.elbow[s]);
            let (elbow, wrist) =
                two_bone(shoulder, hand_target[s], pole, self.upper_len[s], self.fore_len[s]);
            let u = (elbow - shoulder).normalize();
            let f = (wrist - elbow).normalize();
            let flex = orthonormal(f, u, -pole);
            let u0 = (self.rest_pos[fore] - self.rest_pos[arm]).normalize();
            let flex0 = orthonormal(Vec3::Z, u0, Vec3::Z);
            let upper = frame_rotation(u0, flex0, u, flex);
            self.set_global(out, &mut q, arm, (upper * self.rest_rot[arm]).normalize());
            let hinge = u.cross(flex).normalize();
            let flex_f = hinge.cross(f).normalize();
            let f0 = (self.rest_pos[hand] - self.rest_pos[fore]).normalize();
            let lower = frame_rotation(f0, orthonormal(Vec3::Z, f0, Vec3::Z), f, flex_f);
            // Forearm twist: rotate the palm toward the requested normal.
            let palm0 = -Vec3::Y;
            let current = orthonormal(lower.rotate(palm0), f, -Vec3::Y);
            let wanted = orthonormal(turn(pose.palm[s]), f, current);
            let twist = current
                .cross(wanted)
                .dot(f)
                .atan2(current.dot(wanted))
                .clamp(-2.1, 2.1);
            let half = Quat::from_axis_angle(f, twist * 0.55);
            let full = Quat::from_axis_angle(f, twist);
            self.set_global(out, &mut q, fore, (half * lower * self.rest_rot[fore]).normalize());
            let wrist_axis = (full * lower).rotate(f0.cross(palm0).normalize());
            let bend = Quat::from_axis_angle(wrist_axis, pose.wrist[s]);
            self.set_global(
                out,
                &mut q,
                hand,
                (bend * full * lower * self.rest_rot[hand]).normalize(),
            );
            self.curl_fingers(out, s, pose.fist[s]);
        }

        // Legs.
        for s in 0..2 {
            let (thigh, shin, foot, toe) = (self.thigh[s], self.shin[s], self.foot[s], self.toe[s]);
            let hip = p[thigh];
            let pole = turn(pose.knee[s]);
            let (knee, reached) =
                two_bone(hip, ankle[s], pole, self.thigh_len[s], self.shin_len[s]);
            let u = (knee - hip).normalize();
            let f = (reached - knee).normalize();
            let flex = orthonormal(f, u, pole * -1.0);
            let u0 = (self.rest_pos[shin] - self.rest_pos[thigh]).normalize();
            let flex0 = orthonormal(-Vec3::Z, u0, -Vec3::Z);
            let upper = frame_rotation(u0, flex0, u, flex);
            self.set_global(out, &mut q, thigh, (upper * self.rest_rot[thigh]).normalize());
            let hinge = u.cross(flex).normalize();
            let flex_s = hinge.cross(f).normalize();
            let f0 = (self.rest_pos[foot] - self.rest_pos[shin]).normalize();
            let lower = frame_rotation(f0, orthonormal(-Vec3::Z, f0, -Vec3::Z), f, flex_s);
            self.set_global(out, &mut q, shin, (lower * self.rest_rot[shin]).normalize());
            self.set_global(out, &mut q, foot, (foot_rot[s] * self.rest_rot[foot]).normalize());
            // A raised heel keeps the toes flat on the floor.
            let grounded = (1.0 - (ball[s].y - 0.03) / 0.05).clamp(0.0, 1.0);
            let pitch = pose.foot_rot[s].y;
            let flat = (spin * euler(vec3(pose.foot_rot[s].x, 0.0, pose.foot_rot[s].z))).normalize();
            let toe_rot = if pitch > 0.0 {
                foot_rot[s].slerp(flat, grounded)
            } else {
                foot_rot[s]
            };
            self.set_global(out, &mut q, toe, (toe_rot * self.rest_rot[toe]).normalize());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::gltf;
    use crate::game::dancer::Character;

    fn load() -> (Character, BodyRig) {
        let bytes = std::fs::read("assets/character.glb").unwrap();
        let c = Character::from_model(gltf::load_glb(&bytes).unwrap());
        let rig = BodyRig::new(&c.skeleton, c.transform).unwrap();
        (c, rig)
    }
    fn solved(c: &Character, rig: &BodyRig, pose: &BodyPose) -> (Vec<Vec3>, Vec<Quat>) {
        let mut out = c.skeleton.rest_pose();
        rig.solve(pose, vec3(0.0, 1.4, 3.0), &mut out);
        let n = c.skeleton.len();
        let mut p = vec![Vec3::ZERO; n];
        let mut q = vec![Quat::IDENTITY; n];
        rig.fk(&out.locals, &mut p, &mut q);
        (p, q)
    }

    #[test]
    fn limbs_reach_targets_and_bend_toward_poles() {
        let (c, rig) = load();
        let pose = crate::game::anims::stance(0.0);
        let (p, _) = solved(&c, &rig, &pose);
        for s in 0..2 {
            let hand = p[rig.hand[s]];
            assert!((hand - pose.hand[s]).length() < 0.01, "hand {s}: {hand:?}");
            let elbow = p[rig.fore[s]];
            let shoulder = p[rig.arm[s]];
            let mid = (shoulder + hand) * 0.5;
            // The elbow sits on the pole side of the shoulder-wrist line.
            assert!((elbow - mid).dot(pose.elbow[s]) > 0.0, "elbow {s} bends away from pole");
            let knee = p[rig.shin[s]];
            let hip = p[rig.thigh[s]];
            let ankle = p[rig.foot[s]];
            assert!((knee - (hip + ankle) * 0.5).dot(pose.knee[s]) > 0.0, "knee {s}");
            let ball = p[rig.toe[s]];
            assert!((ball - pose.foot[s]).length() < 0.02, "foot {s}: {ball:?}");
        }
    }

    #[test]
    fn mirrored_pose_is_symmetric() {
        let (c, rig) = load();
        let pose = crate::game::anims::stance(0.0);
        let (a, _) = solved(&c, &rig, &pose);
        let (b, _) = solved(&c, &rig, &pose.mirrored());
        for s in 0..2 {
            let x = a[rig.hand[s]];
            let y = b[rig.hand[1 - s]];
            assert!((x - vec3(-y.x, y.y, y.z)).length() < 0.02, "{x:?} vs {y:?}");
        }
    }

    #[test]
    fn neutral_pose_keeps_bone_frames() {
        // Arms hanging with palms in and legs straight must not twist any limb:
        // each bone's side axis stays close to its rest orientation.
        let (c, rig) = load();
        let pose = crate::game::anims::stance(0.0);
        let (_, q) = solved(&c, &rig, &pose);
        for s in 0..2 {
            let thigh = q[rig.thigh[s]];
            // Knee pole points forward: the thigh's Z axis (forward at rest) stays forward-ish.
            let z = thigh.rotate(rig.rest_rot[rig.thigh[s]].conjugate().rotate(Vec3::Z));
            assert!(z.dot(pose.knee[s].normalize()) > 0.3, "thigh {s} twisted: {z:?}");
        }
    }
}
