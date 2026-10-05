//! Authored fighter animation: key poses on the combat core's 60 Hz frames.
//!
//! Every attack follows the same beat as hand-keyed fighting-game moves:
//! a quick set into the anticipation pose, a held coil, an explosive strike
//! that *accelerates* into contact exactly on the simulation's startup frame,
//! a one-frame whip past the target, a held impact pose through the active
//! frames and hit-stop, then a snappy retract and a slower settle.
//! On top of that, `Lead` offsets the body parts in time — the pelvis starts
//! first, the chest follows, the limbs whip last and the head trails — which
//! is what makes a strike read as a kinetic chain rather than a rigid rotation.
//!
//! Poses are canonical: the fighter faces +Z with the lead (left) side on +X.
//! Heights are metres for the 1.65 m model; the wrist sits ~0.45 m from the
//! shoulder and the ball of the foot ~0.98 m below the hip joint.
use super::body::{BodyPose, LEAD as L, REAR as R};
use crate::engine::math3::*;
use arena_combat::moves;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Copy, PartialEq)]
pub enum Ease {
    /// Cubic Hermite through neighbouring keys.
    Smooth,
    /// Fast start, decelerates into the key (sets, retracts).
    Out,
    /// Accelerates into the key (falls).
    In,
    /// Strong acceleration with a dead stop on the key (strikes).
    Snap,
    Linear,
}

#[derive(Clone)]
pub struct Key {
    pub t: f32,
    pub pose: BodyPose,
    pub ease: Ease,
}
pub fn key(t: f32, pose: BodyPose) -> Key {
    Key {
        t,
        pose,
        ease: Ease::Smooth,
    }
}
impl Key {
    pub fn out(mut self) -> Key {
        self.ease = Ease::Out;
        self
    }
    pub fn ease_in(mut self) -> Key {
        self.ease = Ease::In;
        self
    }
    pub fn snap(mut self) -> Key {
        self.ease = Ease::Snap;
        self
    }
    pub fn linear(mut self) -> Key {
        self.ease = Ease::Linear;
        self
    }
}

/// Time offsets (frames) per body region: positive samples ahead (leads).
#[derive(Clone, Copy, Default)]
pub struct Lead {
    pub core: f32,
    pub chest: f32,
    pub head: f32,
}
/// Attacks: hips drive, chest follows, head trails the blow.
pub const STRIKE: Lead = Lead {
    core: 2.0,
    chest: 1.0,
    head: -1.5,
};
/// Blow to the head: head snaps first, the body is dragged after it.
pub const WHIPLASH: Lead = Lead {
    core: -1.5,
    chest: -0.5,
    head: 1.0,
};
/// Blow to the body: chest folds first, hips and head follow.
pub const FOLD: Lead = Lead {
    core: -1.0,
    chest: 1.0,
    head: -1.5,
};

pub struct Anim {
    pub keys: Vec<Key>,
    pub lead: Lead,
}
impl Anim {
    pub fn new(keys: Vec<Key>) -> Anim {
        debug_assert!(keys.windows(2).all(|w| w[0].t < w[1].t));
        Anim {
            keys,
            lead: Lead::default(),
        }
    }
    pub fn with_lead(mut self, lead: Lead) -> Anim {
        self.lead = lead;
        self
    }
    pub fn length(&self) -> f32 {
        self.keys.last().map_or(0.0, |k| k.t)
    }
    fn tangent_free(&self, i: usize) -> bool {
        let k = &self.keys;
        i > 0 && i + 1 < k.len() && k[i].ease == Ease::Smooth && k[i + 1].ease == Ease::Smooth
    }
    /// The pose at `t` with body regions offset in time (overlapping action).
    pub fn pose(&self, t: f32) -> BodyPose {
        let limbs = self.sample(t);
        let Lead { core, chest, head } = self.lead;
        if core == 0.0 && chest == 0.0 && head == 0.0 {
            return limbs;
        }
        let c = self.sample(t + core);
        let ch = self.sample(t + chest);
        let h = self.sample(t + head);
        BodyPose {
            hips: c.hips,
            pelvis: c.pelvis,
            torso: ch.torso,
            clav: ch.clav,
            head: h.head,
            ..limbs
        }
    }
    pub fn sample(&self, t: f32) -> BodyPose {
        let k = &self.keys;
        if t <= k[0].t || k.len() == 1 {
            return k[0].pose;
        }
        let last = k.len() - 1;
        if t >= k[last].t {
            return k[last].pose;
        }
        let i = k.iter().rposition(|key| key.t <= t).unwrap_or(0).min(last - 1);
        let (a, b) = (&k[i], &k[i + 1]);
        let dt = b.t - a.t;
        let u = ((t - a.t) / dt).clamp(0.0, 1.0);
        match b.ease {
            Ease::Out => a.pose.lerp(&b.pose, 1.0 - (1.0 - u).powi(3)),
            Ease::In => a.pose.lerp(&b.pose, u * u),
            Ease::Snap => a.pose.lerp(&b.pose, u * u * u),
            Ease::Linear => a.pose.lerp(&b.pose, u),
            Ease::Smooth => {
                let (u2, u3) = (u * u, u * u * u);
                let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
                let h10 = u3 - 2.0 * u2 + u;
                let h01 = -2.0 * u3 + 3.0 * u2;
                let h11 = u3 - u2;
                // Catmull-Rom tangents in time; zero next to eased segments.
                let mut items: Vec<(&BodyPose, f32)> = vec![(&a.pose, h00), (&b.pose, h01)];
                if self.tangent_free(i) {
                    let w = h10 * dt / (k[i + 1].t - k[i - 1].t);
                    items.push((&k[i + 1].pose, w));
                    items.push((&k[i - 1].pose, -w));
                }
                if self.tangent_free(i + 1) {
                    let w = h11 * dt / (k[i + 2].t - k[i].t);
                    items.push((&k[i + 2].pose, w));
                    items.push((&k[i].pose, -w));
                }
                BodyPose::weighted(&items)
            }
        }
    }
}

pub const fn v(x: f32, y: f32, z: f32) -> Vec3 {
    vec3(x, y, z)
}

/// Orthodox fighting stance: low and wide, lead (left) foot and hand forward,
/// hips bladed, chest turned back toward the opponent, chin behind the guard.
pub fn base() -> BodyPose {
    BodyPose {
        hips: v(0.0, 0.885, -0.02),
        pelvis: v(-0.55, 0.10, 0.0),
        torso: v(0.22, 0.16, 0.03),
        head: v(0.0, 0.10, 0.0),
        look: 1.0,
        hand: [v(0.09, 1.30, 0.30), v(-0.10, 1.26, 0.16)],
        elbow: [v(0.4, -1.0, -0.15), v(-0.35, -1.0, 0.0)],
        palm: [v(-0.8, -0.3, -0.3), v(0.8, -0.2, -0.4)],
        wrist: [0.0, 0.0],
        fist: [1.0, 1.0],
        clav: [v(0.0, 0.10, 0.0), v(0.0, 0.0, 0.0)],
        foot: [v(0.12, 0.004, 0.30), v(-0.19, 0.004, -0.31)],
        foot_rot: [v(-0.2, 0.0, 0.0), v(-0.85, 0.30, 0.0)],
        knee: [v(0.35, 0.0, 1.0), v(-0.8, 0.0, 0.6)],
        spin: Vec3::ZERO,
        shift: Vec3::ZERO,
    }
}

/// Rhythmic fighting-game bounce layered on a pose. `beat` in cycles; the
/// knees give, the chest dips over them and the guard trails the body.
fn bounce(mut p: BodyPose, beat: f32, amount: f32) -> BodyPose {
    let b = beat.rem_euclid(1.0);
    let down = (1.0 - (b * TAU).cos()) * 0.5;
    let late = (1.0 - ((b - 0.1) * TAU).cos()) * 0.5;
    p.hips.y -= 0.034 * down * amount;
    p.hips.z += 0.010 * down * amount;
    p.torso.y += 0.06 * down * amount;
    p.head.y -= 0.05 * down * amount;
    p.pelvis.x += (b * TAU).sin() * 0.03 * amount;
    for h in 0..2 {
        p.hand[h].y -= 0.026 * late * amount;
        p.hand[h].z += 0.012 * late * amount;
    }
    p.hand[L].x += (b * TAU).sin() * 0.012 * amount;
    p
}

pub fn stance(beat: f32) -> BodyPose {
    bounce(base(), beat, 1.0)
}

/// Forearms stacked in front of the face, weight back.
pub fn guard(beat: f32) -> BodyPose {
    let p = base().with(|p| {
        p.hips = v(0.0, 0.865, -0.05);
        p.torso = v(0.28, 0.24, 0.0);
        p.head = v(0.0, 0.28, 0.0);
        p.hand = [v(0.06, 1.38, 0.25), v(-0.07, 1.36, 0.21)];
        p.elbow = [v(0.2, -1.0, 0.35), v(-0.2, -1.0, 0.35)];
        p.palm = [v(-0.3, 0.0, -1.0), v(0.3, 0.0, -1.0)];
        p.clav = [v(0.10, 0.12, 0.0), v(0.10, 0.12, 0.0)];
    });
    bounce(p, beat, 0.4)
}

pub fn crouch(beat: f32) -> BodyPose {
    let p = base().with(|p| {
        p.hips = v(0.0, 0.58, -0.04);
        p.pelvis = v(-0.45, 0.28, 0.0);
        p.torso = v(0.20, 0.28, 0.0);
        p.head = v(0.0, -0.12, 0.0);
        p.hand = [v(0.08, 1.02, 0.34), v(-0.09, 0.97, 0.22)];
        p.foot = [v(0.16, 0.004, 0.24), v(-0.21, 0.004, -0.25)];
        p.foot_rot = [v(-0.25, 0.0, 0.0), v(-0.85, 0.65, 0.0)];
        p.knee = [v(0.55, 0.2, 1.0), v(-0.8, 0.0, 0.6)];
    });
    bounce(p, beat, 0.45)
}

pub fn crouch_guard(beat: f32) -> BodyPose {
    let p = crouch(0.0).with(|p| {
        p.torso.y = 0.38;
        p.head.y = 0.15;
        p.hand = [v(0.05, 1.06, 0.28), v(-0.06, 1.03, 0.25)];
        p.elbow = [v(0.2, -1.0, 0.4), v(-0.2, -1.0, 0.4)];
        p.palm = [v(-0.3, 0.0, -1.0), v(0.3, 0.0, -1.0)];
    });
    bounce(p, beat, 0.3)
}

/// Walking in stance: feet shuffle without crossing. `phase` advances by the
/// distance travelled, so planted feet never slide on the floor.
pub fn walk_layer(pose: &mut BodyPose, phase: f32, weight: f32, stride: f32) {
    if weight <= 0.0 {
        return;
    }
    let phase = phase.rem_euclid(1.0);
    for (s, start) in [(L, 0.0f32), (R, 0.5f32)] {
        // Local position of the foot: triangle wave, swing half then planted half.
        let local = (phase - start).rem_euclid(1.0);
        let (offset, lift) = if local < 0.5 {
            let u = local / 0.5;
            let e = u * u * (3.0 - 2.0 * u);
            (-0.25 + 0.5 * e, (u * PI).sin())
        } else {
            (0.25 - (local - 0.5) / 0.5 * 0.5, 0.0)
        };
        pose.foot[s].z += offset * stride * weight;
        pose.foot[s].y += lift * 0.06 * weight;
        pose.foot_rot[s].y += lift * 0.4 * weight;
    }
    // The body rises over each passing foot and drops as it plants.
    let dip = (phase * TAU * 2.0).cos();
    pose.hips.y -= (0.016 + dip * 0.014) * weight;
    pose.torso.y += (0.04 - dip * 0.02) * weight;
    pose.torso.x += (phase * TAU).sin() * 0.06 * weight;
    for s in 0..2 {
        pose.hand[s].y += dip * 0.008 * weight;
    }
}

/// Airborne pose from vertical speed (mm/tick, +112 at takeoff).
pub fn jump(vy: f32, forward: f32) -> BodyPose {
    let base = base();
    let rise = base.with(|p| {
        p.hips = v(0.0, 1.0, 0.0);
        p.pelvis = v(-0.35, 0.0, 0.0);
        p.torso = v(0.15, 0.02, 0.0);
        p.hand = [v(0.12, 1.55, 0.28), v(-0.10, 1.50, 0.16)];
        p.foot = [v(0.08, 0.04, 0.10), v(-0.10, 0.08, -0.18)];
        p.foot_rot = [v(-0.15, 1.0, 0.0), v(-0.5, 1.1, 0.0)];
        p.knee = [v(0.2, 0.0, 1.0), v(-0.4, 0.0, 1.0)];
    });
    let tuck = base.with(|p| {
        p.hips = v(0.0, 0.98, 0.0);
        p.pelvis = v(-0.35, -0.12, 0.0);
        p.torso = v(0.15, 0.38, 0.0);
        p.head = v(0.0, 0.10, 0.0);
        p.hand = [v(0.10, 1.28, 0.36), v(-0.10, 1.24, 0.28)];
        p.foot = [v(0.10, 0.60, 0.26), v(-0.10, 0.52, 0.10)];
        p.foot_rot = [v(-0.1, 0.6, 0.0), v(-0.3, 0.7, 0.0)];
        p.knee = [v(0.2, 0.6, 1.0), v(-0.2, 0.6, 1.0)];
    });
    let fall = base.with(|p| {
        p.hips = v(0.0, 0.97, 0.0);
        p.torso = v(0.18, 0.12, 0.0);
        p.hand = [v(0.12, 1.42, 0.32), v(-0.10, 1.38, 0.20)];
        p.foot = [v(0.12, 0.10, 0.22), v(-0.14, 0.08, -0.20)];
        p.foot_rot = [v(-0.15, 0.45, 0.0), v(-0.7, 0.55, 0.0)];
    });
    let a = (vy / 112.0).clamp(-1.0, 1.0);
    let mut pose = if a > 0.0 {
        tuck.lerp(&rise, (a * 1.4 - 0.2).clamp(0.0, 1.0))
    } else {
        tuck.lerp(&fall, (-a * 1.5).clamp(0.0, 1.0))
    };
    if forward.abs() > 0.1 {
        // Directional jumps somersault: tucked and done well before landing.
        let progress = ((112.0 - vy) / 185.0).clamp(0.0, 1.0);
        let e = progress * progress * (3.0 - 2.0 * progress);
        let tucked = (progress * PI).sin().powf(0.6);
        pose = pose.lerp(&tuck, tucked);
        pose.spin.y += forward.signum() * 2.0 * PI * e;
        pose.look *= 1.0 - tucked;
    }
    pose
}

/// Launched: a whiplash arch on impact, then a backward tumble that is flat
/// on the back by the time the body falls to the floor.
pub fn air_hit(vy: f32, frame: f32) -> BodyPose {
    let t = ((70.0 - vy) / 140.0).clamp(0.0, 1.0);
    let flail = (frame * 0.4).sin();
    let tumble = base().with(|p| {
        p.hips = v(0.0, 0.95, 0.0);
        p.pelvis = v(-0.3, -0.25, 0.0);
        p.torso = v(0.15, -0.30, 0.1);
        p.head = v(0.25, -0.45, 0.0);
        p.look = 0.0;
        p.hand = [
            v(0.34, 1.26 + flail * 0.06, 0.34),
            v(-0.30, 1.36 - flail * 0.06, 0.26),
        ];
        p.elbow = [v(0.6, -0.4, -0.5), v(-0.6, -0.4, -0.5)];
        p.palm = [v(0.0, -1.0, 0.0), v(0.0, -1.0, 0.0)];
        p.fist = [0.3, 0.3];
        p.foot = [v(0.12, 0.34, 0.46), v(-0.12, 0.14, 0.20)];
        p.foot_rot = [v(0.0, 0.7, 0.0), v(-0.3, 0.9, 0.0)];
        p.knee = [v(0.3, 0.5, 1.0), v(-0.3, 0.3, 1.0)];
        p.spin = v(0.0, -0.35 - 1.15 * t, 0.0);
    });
    let arch = tumble.with(|p| {
        p.torso = v(0.2, -0.60, 0.15);
        p.head = v(0.3, -0.75, 0.1);
        p.hips.z = 0.08;
        p.hand = [v(0.36, 1.08, 0.40), v(-0.34, 1.12, 0.34)];
        p.foot = [v(0.12, 0.18, 0.30), v(-0.12, 0.05, 0.12)];
        p.spin = v(0.0, -0.25, 0.0);
    });
    let whip = (1.0 - frame / 9.0).clamp(0.0, 1.0);
    tumble.lerp(&arch, whip * whip * (3.0 - 2.0 * whip))
}

/// Flat on the back, limbs loose; `breath` in metres.
pub fn lying(breath: f32) -> BodyPose {
    BodyPose {
        hips: v(0.0, 0.95, 0.0),
        pelvis: v(0.0, 0.0, 0.0),
        torso: v(0.0, 0.05 - breath, 0.0),
        head: v(0.35, -0.10, 0.0),
        look: 0.0,
        hand: [v(0.42, 1.12, -0.08), v(-0.38, 0.92, 0.04)],
        elbow: [v(0.3, 0.0, -1.0), v(-0.3, 0.0, -1.0)],
        palm: [v(0.0, 0.0, -1.0), v(0.0, 0.0, -1.0)],
        wrist: [0.2, 0.2],
        fist: [0.35, 0.3],
        clav: [v(0.1, 0.0, 0.0), v(0.1, 0.0, 0.0)],
        foot: [v(0.20, 0.06, 0.18), v(-0.16, 0.05, -0.02)],
        foot_rot: [v(0.5, 0.35, 0.3), v(-0.4, 0.3, -0.3)],
        knee: [v(0.5, 0.0, 1.0), v(-0.5, 0.0, 1.0)],
        spin: v(0.0, -FRAC_PI_2, 0.0),
        shift: v(0.0, -0.80, 0.0),
    }
}

/// Knockdown and quick rise over `total` frames. From the air the body lands
/// already flat; from the feet the legs are swept first.
pub fn knockdown(from_air: bool, total: f32) -> Anim {
    let flat = lying(0.0);
    let bounce = flat.with(|p| p.shift.y += 0.07);
    let sit = flat.with(|p| {
        p.spin = v(0.0, -0.75, 0.0);
        p.shift = v(0.0, -0.62, -0.08);
        p.torso = v(0.0, 0.35, 0.0);
        p.head = v(0.0, 0.25, 0.0);
        p.look = 0.4;
        p.hand = [v(0.25, 0.80, -0.15), v(-0.25, 0.78, -0.12)];
        p.elbow = [v(0.4, 0.0, -1.0), v(-0.4, 0.0, -1.0)];
        p.palm = [v(0.0, 0.8, -0.2), v(0.0, 0.8, -0.2)];
        p.wrist = [-0.8, -0.8];
        p.fist = [0.1, 0.1];
        p.foot = [v(0.12, 0.30, 0.45), v(-0.14, 0.45, 0.25)];
        p.foot_rot = [v(0.0, -0.9, 0.0), v(-0.3, -1.2, 0.0)];
        p.knee = [v(0.3, 1.0, 0.3), v(-0.3, 1.0, 0.3)];
    });
    let kneel = crouch(0.0).with(|p| {
        p.hips = v(0.0, 0.52, -0.05);
        p.torso.y = 0.55;
        p.hand[L] = v(0.20, 0.62, 0.22);
        p.hand[R] = v(-0.12, 0.35, 0.30);
        p.fist[R] = 0.2;
        p.foot[L] = v(0.14, 0.004, 0.24);
        p.foot[R] = v(-0.18, 0.004, -0.10);
        p.foot_rot[R] = v(-0.6, 1.1, 0.0);
    });
    let s = total / 48.0;
    let mut keys = Vec::new();
    if from_air {
        keys.push(key(0.0, flat));
        keys.push(key(3.0 * s, bounce).out());
        keys.push(key(7.0 * s, flat).ease_in());
    } else {
        let swept = base().with(|p| {
            p.hips = v(0.0, 0.80, -0.10);
            p.torso = v(0.1, -0.35, 0.0);
            p.head = v(0.0, -0.3, 0.0);
            p.look = 0.3;
            p.hand = [v(0.35, 1.25, 0.20), v(-0.30, 1.30, 0.05)];
            p.fist = [0.4, 0.4];
            p.foot = [v(0.10, 0.38, 0.52), v(-0.10, 0.22, 0.32)];
            p.foot_rot = [v(0.0, 0.6, 0.0), v(-0.3, 0.6, 0.0)];
            p.spin = v(0.0, -0.6, 0.0);
            p.shift = v(0.0, -0.18, 0.0);
        });
        keys.push(key(0.0, base()));
        keys.push(key(4.0 * s, swept).out());
        keys.push(key(9.0 * s, flat).ease_in());
        keys.push(key(12.0 * s, bounce).out());
        keys.push(key(15.0 * s, flat).ease_in());
    }
    keys.push(key(26.0 * s, flat));
    keys.push(key(34.0 * s, sit));
    keys.push(key(41.0 * s, kneel));
    keys.push(key(total, base()));
    Anim::new(keys)
}

pub struct Library {
    attacks: Vec<Option<Anim>>,
    /// Nominal forward reach of the striking limb at contact (canonical +Z).
    pub reach: Vec<f32>,
    pub dash_forward: Anim,
    pub dash_back: Anim,
    pub reactions: Vec<Anim>,
    pub knockdown_air: Anim,
    pub knockdown_ground: Anim,
    pub ko: Anim,
    pub victory: Anim,
    pub defeat: Anim,
}

/// Hit-reaction variants; indices into `Library::reactions`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Reaction {
    #[default]
    Head,
    Gut,
    Low,
    Parried,
    GuardBreak,
    Pushed,
    Wall,
    /// A jab: the head snaps back a little.
    HeadLight,
    /// A hook: the head is whipped to the side.
    HeadSide,
    /// A counter hit: the body is spun round.
    Spin,
    /// A side kick: doubled over.
    GutBig,
}
impl Reaction {
    pub fn from_attack(action: u32) -> Reaction {
        match action {
            1 => Reaction::HeadLight,
            17 => Reaction::HeadSide,
            18 => Reaction::GutBig,
            8 | 14 | 19 => Reaction::Gut,
            9 | 16 => Reaction::Low,
            _ => Reaction::Head,
        }
    }
    /// Frames of the authored reaction; the tail is stretched to the stun.
    pub fn impact_frames(self) -> f32 {
        match self {
            Reaction::GuardBreak => 10.0,
            _ => 8.0,
        }
    }
}

/// Which limb lands the blow: 0/1 hands, 2/3 feet, 4 both hands.
pub fn striker(action: u32) -> usize {
    match action {
        1 | 17 => 0,
        2 | 10 | 11 => 1,
        4 | 14 | 19 => 4,
        8 | 13 | 18 => 2,
        9 | 12 | 16 => 3,
        _ => 0,
    }
}

impl Library {
    pub fn attack(&self, action: u32) -> Option<&Anim> {
        self.attacks.get(action as usize).and_then(|a| a.as_ref())
    }
    pub fn reaction(&self, r: Reaction) -> &Anim {
        &self.reactions[r as usize]
    }
}

pub fn library() -> Library {
    let mut attacks: Vec<Option<Anim>> = (0..20).map(|_| None).collect();
    attacks[1] = Some(jab());
    attacks[11] = Some(cross());
    attacks[2] = Some(overhead());
    attacks[4] = Some(throw());
    attacks[8] = Some(front_kick());
    attacks[9] = Some(sweep());
    attacks[10] = Some(uppercut());
    attacks[12] = Some(roundhouse());
    attacks[13] = Some(air_kick());
    attacks[14] = Some(impulse());
    attacks[19] = Some(ground_pound());
    attacks[16] = Some(low_kick());
    attacks[17] = Some(hook());
    attacks[18] = Some(side_kick());
    let reach = (0..20u32)
        .map(|action| {
            let (Some(anim), Some(m)) = (&attacks[action as usize], moves::attack(action)) else {
                return 0.0;
            };
            let p = anim.sample(m.startup as f32);
            match striker(action) {
                0 => p.hand[L].z,
                1 => p.hand[R].z,
                2 => p.foot[L].z,
                3 => p.foot[R].z,
                _ => (p.hand[L].z + p.hand[R].z) * 0.5,
            }
        })
        .collect();
    let knock = arena_combat::KNOCKDOWN as f32;
    Library {
        attacks,
        reach,
        dash_forward: dash_forward(),
        dash_back: dash_back(),
        reactions: vec![
            react_head(),
            react_gut(),
            react_low(),
            react_parried(),
            react_guard_break(),
            react_parried(),
            react_wall(),
            // Captured clips tell these apart; the authored poses share.
            react_head(),
            react_head(),
            react_head(),
            react_gut(),
        ],
        knockdown_air: knockdown(true, knock),
        knockdown_ground: knockdown(false, knock),
        ko: knockout(),
        victory: victory(),
        defeat: defeat(),
    }
}

fn jab() -> Anim {
    let s = base();
    // Tiny set: the fist tucks and the knees give before the snap.
    let set = s.with(|p| {
        p.hips.y -= 0.014;
        p.pelvis.x = -0.60;
        p.torso.y += 0.04;
        p.hand[L] = v(0.08, 1.26, 0.25);
    });
    let hit = s.with(|p| {
        p.hips = v(0.0, 0.872, 0.07);
        p.pelvis = v(-0.72, 0.12, 0.0);
        p.torso = v(0.02, 0.27, 0.05);
        p.head = v(0.12, 0.14, 0.0);
        p.hand[L] = v(0.03, 1.33, 0.62);
        p.elbow[L] = v(0.6, -1.0, 0.1);
        p.palm[L] = v(-0.25, -1.0, 0.0);
        p.clav[L] = v(0.05, 0.38, 0.0);
        p.hand[R] = v(-0.08, 1.25, 0.18);
        p.foot_rot[R] = v(-0.85, 0.45, 0.0);
    });
    let whip = hit.with(|p| {
        p.hand[L].z += 0.03;
        p.clav[L].y = 0.44;
    });
    let back = s.with(|p| {
        p.hips.z += 0.03;
        p.hand[L] = v(0.09, 1.31, 0.35);
    });
    Anim::new(vec![
        key(0.0, s),
        key(2.0, set).out(),
        key(4.0, set.with(|p| p.hips.y -= 0.004)),
        key(7.0, hit).snap(),
        key(8.0, whip).out(),
        key(10.0, hit),
        key(14.0, back).out(),
        key(20.0, s),
    ])
    .with_lead(STRIKE)
}

fn cross() -> Anim {
    let s = base();
    let load = s.with(|p| {
        p.pelvis = v(-0.74, 0.10, 0.0);
        p.torso = v(0.16, 0.19, 0.04);
        p.hips = v(0.0, 0.872, -0.04);
        p.hand[R] = v(-0.12, 1.21, 0.11);
    });
    let hit = s.with(|p| {
        p.hips = v(0.0, 0.868, 0.09);
        p.pelvis = v(0.12, 0.14, 0.0);
        p.torso = v(0.40, 0.28, -0.06);
        p.head = v(-0.1, 0.14, 0.0);
        p.hand[R] = v(0.00, 1.33, 0.63);
        p.elbow[R] = v(-0.65, -1.0, 0.1);
        p.palm[R] = v(0.25, -1.0, 0.0);
        p.clav[R] = v(0.05, 0.42, 0.0);
        p.hand[L] = v(0.12, 1.28, 0.20);
        p.foot[R] = v(-0.17, 0.004, -0.29);
        p.foot_rot[R] = v(-0.25, 0.85, 0.0);
        p.knee[R] = v(-0.1, -0.1, 1.0);
        p.knee[L] = v(0.3, 0.0, 1.0);
    });
    let whip = hit.with(|p| {
        p.hand[R].z += 0.03;
        p.clav[R].y = 0.48;
    });
    let back = s.with(|p| {
        p.hand[R] = v(-0.10, 1.28, 0.26);
        p.pelvis.x = -0.35;
        p.hips.z += 0.03;
    });
    Anim::new(vec![
        key(0.0, s),
        key(2.0, load).out(),
        key(3.0, load),
        key(6.0, hit).snap(),
        key(7.0, whip).out(),
        key(9.0, hit),
        key(15.0, back).out(),
        key(24.0, s),
    ])
    .with_lead(STRIKE)
}

fn overhead() -> Anim {
    let s = base();
    // Coil: weight over the back leg, shoulders turned away, fist cocked
    // behind the head, lead hand measuring the distance.
    let coil = s.with(|p| {
        p.hips = v(0.02, 0.89, -0.08);
        p.pelvis = v(-0.95, 0.02, 0.05);
        p.torso = v(-0.35, -0.02, 0.14);
        p.head = v(0.4, 0.05, -0.1);
        p.hand[R] = v(-0.27, 1.34, -0.20);
        p.elbow[R] = v(-0.5, 0.5, -0.7);
        p.palm[R] = v(0.5, 0.0, 0.5);
        p.clav[R] = v(0.20, -0.15, 0.0);
        p.hand[L] = v(0.20, 1.25, 0.36);
        p.elbow[L] = v(0.6, -1.0, 0.0);
        p.fist[L] = 0.5;
        p.foot[L] = v(0.12, 0.02, 0.28);
        p.foot_rot[L] = v(-0.2, 0.25, 0.0);
        p.foot_rot[R] = v(-0.85, 0.10, 0.0);
    });
    let deep = coil.with(|p| {
        p.hand[R] = v(-0.30, 1.38, -0.24);
        p.torso.z = 0.18;
        p.hips.y = 0.875;
    });
    // The fist comes over the top as the lead foot steps in.
    let over = s.with(|p| {
        p.hips = v(0.0, 0.86, 0.10);
        p.pelvis = v(-0.30, 0.10, 0.0);
        p.torso = v(0.0, 0.20, 0.0);
        p.hand[R] = v(-0.12, 1.50, 0.16);
        p.elbow[R] = v(-0.4, 0.9, -0.2);
        p.palm[R] = v(0.3, -0.5, 0.5);
        p.clav[R] = v(0.25, 0.15, 0.0);
        p.hand[L] = v(0.18, 1.12, 0.20);
        p.foot[L] = v(0.12, 0.07, 0.44);
        p.foot_rot[L] = v(-0.15, 0.2, 0.0);
    });
    let hit = s.with(|p| {
        p.hips = v(0.0, 0.80, 0.18);
        p.pelvis = v(0.28, 0.18, 0.0);
        p.torso = v(0.35, 0.36, -0.14);
        p.head = v(-0.15, -0.05, 0.0);
        p.hand[R] = v(0.02, 1.26, 0.64);
        p.elbow[R] = v(-0.7, 0.2, -0.3);
        p.palm[R] = v(0.3, -1.0, 0.2);
        p.clav[R] = v(0.0, 0.42, 0.0);
        p.hand[L] = v(0.20, 1.00, 0.10);
        p.foot[L] = v(0.12, 0.004, 0.50);
        p.foot_rot[L] = v(-0.1, 0.0, 0.0);
        p.foot_rot[R] = v(-0.3, 0.95, 0.0);
        p.knee[R] = v(-0.2, -0.1, 1.0);
    });
    let follow = hit.with(|p| {
        p.hand[R] = v(0.05, 1.10, 0.60);
        p.torso.y = 0.42;
        p.hips.y = 0.79;
    });
    let through = hit.with(|p| {
        p.hand[R] = v(0.06, 0.98, 0.52);
        p.torso.y = 0.44;
        p.hips.y = 0.79;
    });
    let recover = s.with(|p| {
        p.hips = v(0.0, 0.86, 0.12);
        p.foot[L] = v(0.12, 0.05, 0.40);
        p.hand[R] = v(-0.10, 1.18, 0.24);
    });
    Anim::new(vec![
        key(0.0, s),
        key(7.0, coil).out(),
        key(15.0, deep),
        key(18.0, over).ease_in(),
        key(21.0, hit).snap(),
        key(23.0, follow).out(),
        key(27.0, through),
        key(36.0, recover),
        key(47.0, s),
    ])
    .with_lead(STRIKE)
}

fn throw() -> Anim {
    let s = base();
    let reach = s.with(|p| {
        p.hips = v(0.0, 0.87, 0.10);
        p.pelvis.x = -0.30;
        p.torso = v(0.15, 0.28, 0.0);
        p.hand = [v(0.11, 1.30, 0.50), v(-0.07, 1.28, 0.48)];
        p.elbow = [v(0.8, -0.6, 0.0), v(-0.8, -0.6, 0.0)];
        p.palm = [v(-0.4, -0.2, 1.0), v(0.4, -0.2, 1.0)];
        p.fist = [0.15, 0.15];
    });
    let grab = reach.with(|p| {
        p.hand = [v(0.07, 1.32, 0.54), v(-0.06, 1.31, 0.52)];
        p.fist = [1.0, 1.0];
    });
    let pull = s.with(|p| {
        p.hips = v(0.0, 0.83, 0.0);
        p.pelvis = v(0.35, 0.18, 0.0);
        p.torso = v(0.45, 0.25, -0.12);
        p.hand = [v(-0.02, 1.12, 0.36), v(-0.26, 1.06, 0.14)];
        p.elbow = [v(0.5, -1.0, 0.0), v(-0.5, -1.0, -0.2)];
        p.foot_rot[R] = v(-0.5, 0.7, 0.0);
    });
    let heave = s.with(|p| {
        p.hips = v(0.0, 0.90, 0.12);
        p.pelvis = v(-0.40, 0.02, 0.0);
        p.torso = v(-0.10, 0.02, 0.12);
        p.hand = [v(0.18, 1.48, 0.56), v(0.02, 1.46, 0.58)];
        p.palm = [v(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0)];
        p.fist = [0.2, 0.2];
    });
    Anim::new(vec![
        key(0.0, s),
        key(6.0, reach).out(),
        key(11.0, reach.with(|p| {
            for h in 0..2 {
                p.hand[h].z += 0.03;
            }
        })),
        key(13.0, grab).snap(),
        key(26.0, pull),
        key(42.0, heave).snap(),
        key(48.0, s.with(|p| p.hips.z += 0.05)),
        key(56.0, s),
    ])
    .with_lead(STRIKE)
}

fn front_kick() -> Anim {
    let s = base();
    let chamber = s.with(|p| {
        p.hips = v(0.0, 0.89, -0.07);
        p.pelvis = v(-0.30, -0.08, 0.0);
        p.torso = v(0.08, 0.08, 0.0);
        p.foot[L] = v(0.08, 0.50, 0.26);
        p.foot_rot[L] = v(0.0, 0.7, 0.0);
        p.knee[L] = v(0.1, 0.4, 1.0);
        p.foot_rot[R] = v(-1.0, 0.0, 0.0);
        p.hand[L] = v(0.14, 1.24, 0.24);
        p.hand[R] = v(-0.08, 1.24, 0.18);
    });
    let high = chamber.with(|p| {
        p.foot[L] = v(0.07, 0.64, 0.32);
        p.hips.z = -0.08;
        p.torso.y = 0.0;
    });
    let hit = s.with(|p| {
        p.hips = v(0.0, 0.93, 0.14);
        p.pelvis = v(-0.28, -0.28, 0.0);
        p.torso = v(0.10, -0.20, 0.0);
        p.head = v(0.0, 0.28, 0.0);
        p.foot[L] = v(0.03, 1.00, 0.98);
        p.foot_rot[L] = v(0.0, -1.3, 0.0);
        p.knee[L] = v(0.1, 1.0, 0.1);
        p.foot[R] = v(-0.17, 0.004, -0.28);
        p.foot_rot[R] = v(-1.05, 0.0, 0.0);
        p.hand[L] = v(0.26, 1.06, 0.02);
        p.fist[L] = 0.8;
        p.hand[R] = v(-0.08, 1.26, 0.18);
    });
    Anim::new(vec![
        key(0.0, s),
        key(4.0, chamber).out(),
        key(8.0, high),
        key(11.0, hit).snap(),
        key(12.0, hit.with(|p| p.foot[L].z += 0.04)).out(),
        key(15.0, hit),
        key(20.0, chamber.with(|p| p.foot[L] = v(0.08, 0.50, 0.34))).out(),
        key(25.0, s.with(|p| p.foot[L] = v(0.12, 0.03, 0.32))),
        key(30.0, s),
    ])
    .with_lead(STRIKE)
}

fn sweep() -> Anim {
    let s = base();
    let drop = s.with(|p| {
        p.hips = v(0.02, 0.46, 0.0);
        p.pelvis = v(-0.75, 0.32, 0.0);
        p.torso = v(0.25, 0.45, 0.0);
        p.head = v(-0.1, -0.25, 0.0);
        p.hand[L] = v(0.26, 0.07, 0.30);
        p.palm[L] = v(0.0, -1.0, 0.0);
        p.wrist[L] = -0.9;
        p.fist[L] = 0.1;
        p.elbow[L] = v(0.5, 0.0, -1.0);
        p.hand[R] = v(-0.02, 0.90, 0.28);
        p.foot[L] = v(0.22, 0.004, 0.18);
        p.foot_rot[L] = v(-0.3, 0.3, 0.0);
        p.knee[L] = v(0.65, 0.1, 1.0);
        p.foot[R] = v(-0.42, 0.02, -0.32);
        p.foot_rot[R] = v(-0.9, 0.9, 0.0);
        p.knee[R] = v(-0.7, 0.0, 0.3);
    });
    let wind = drop.with(|p| {
        p.pelvis.x = -0.40;
        p.foot[R] = v(-0.55, 0.03, -0.15);
    });
    let side = drop.with(|p| {
        p.pelvis.x = 0.05;
        p.torso.x = 0.05;
        p.foot[R] = v(-0.62, 0.03, 0.18);
        p.foot_rot[R] = v(-1.1, 0.3, 1.0);
        p.knee[R] = v(-0.3, 1.0, 0.2);
    });
    let hit = drop.with(|p| {
        p.pelvis.x = 0.60;
        p.torso.x = -0.25;
        p.hips = v(0.0, 0.44, 0.03);
        p.foot[R] = v(-0.10, 0.04, 0.86);
        p.foot_rot[R] = v(0.9, 0.0, 1.2);
        p.knee[R] = v(-0.6, 1.0, 0.0);
    });
    let through = hit.with(|p| {
        p.pelvis.x = 0.82;
        p.foot[R] = v(0.16, 0.04, 0.78);
    });
    let retract = drop.with(|p| {
        p.hips.y = 0.58;
        p.foot[R] = v(-0.22, 0.02, -0.26);
        p.foot_rot[R] = v(-0.9, 0.7, 0.0);
        p.hand[L] = v(0.18, 0.55, 0.30);
        p.fist[L] = 0.7;
        p.wrist[L] = 0.0;
    });
    Anim::new(vec![
        key(0.0, s),
        key(4.0, drop).out(),
        key(8.0, wind),
        key(11.0, side).ease_in(),
        key(14.0, hit).snap(),
        key(17.0, through).out(),
        key(29.0, retract),
        key(42.0, s),
    ])
    .with_lead(STRIKE)
}

fn uppercut() -> Anim {
    let s = base();
    let coil = s.with(|p| {
        p.hips = v(0.0, 0.66, 0.02);
        p.pelvis = v(-0.80, 0.32, 0.0);
        p.torso = v(-0.20, 0.45, 0.08);
        p.head = v(0.1, -0.25, 0.0);
        p.hand[R] = v(-0.14, 0.86, 0.16);
        p.elbow[R] = v(-0.5, -1.0, -0.3);
        p.palm[R] = v(0.4, 0.0, -1.0);
        p.hand[L] = v(0.12, 1.00, 0.34);
        p.foot[L] = v(0.13, 0.004, 0.28);
        p.foot_rot[L] = v(-0.25, 0.0, 0.0);
        p.foot_rot[R] = v(-0.85, 0.6, 0.0);
        p.knee = [v(0.6, 0.1, 1.0), v(-0.85, 0.0, 0.5)];
    });
    let hit = s.with(|p| {
        p.hips = v(0.0, 1.03, 0.14);
        p.pelvis = v(0.18, -0.08, 0.0);
        p.torso = v(0.30, -0.14, -0.12);
        p.head = v(-0.1, -0.25, 0.0);
        p.hand[R] = v(0.00, 1.84, 0.34);
        p.elbow[R] = v(-0.3, -1.0, 0.6);
        p.palm[R] = v(0.2, 0.0, -1.0);
        p.clav[R] = v(0.40, 0.25, 0.0);
        p.hand[L] = v(0.22, 1.08, -0.02);
        p.fist[L] = 0.8;
        p.foot_rot[L] = v(-0.15, 0.7, 0.0);
        p.foot_rot[R] = v(-0.35, 1.0, 0.0);
        p.knee[R] = v(-0.3, 0.0, 1.0);
    });
    let over = hit.with(|p| {
        p.hand[R].y = 1.92;
        p.hips.y = 1.05;
    });
    Anim::new(vec![
        key(0.0, s),
        key(5.0, coil).out(),
        key(8.0, coil.with(|p| {
            p.hips.y = 0.64;
            p.hand[R] = v(-0.14, 0.82, 0.12);
        })),
        key(12.0, hit).snap(),
        key(14.0, over).out(),
        key(17.0, over),
        key(28.0, s.with(|p| {
            p.hips.y = 0.84;
            p.hand[R] = v(-0.08, 1.35, 0.26);
        })),
        key(46.0, s),
    ])
    .with_lead(STRIKE)
}

fn roundhouse() -> Anim {
    let s = base();
    let pivot = s.with(|p| {
        p.hips = v(0.04, 0.89, 0.04);
        p.pelvis = v(0.05, 0.0, 0.0);
        p.torso = v(-0.20, 0.08, -0.12);
        p.foot_rot[L] = v(-1.15, 0.2, 0.0);
        p.foot[R] = v(-0.18, 0.10, -0.14);
        p.foot_rot[R] = v(-0.6, 0.9, 0.0);
        p.knee[R] = v(-0.2, 0.2, 1.0);
        p.hand[R] = v(-0.24, 1.10, 0.02);
        p.fist[R] = 0.8;
    });
    let chamber = s.with(|p| {
        p.hips = v(0.06, 0.90, 0.02);
        p.pelvis = v(0.70, -0.06, -0.22);
        p.torso = v(-0.55, 0.0, -0.28);
        p.head = v(0.0, 0.05, 0.1);
        p.foot[R] = v(-0.36, 0.86, 0.22);
        p.foot_rot[R] = v(0.9, 0.8, -1.3);
        p.knee[R] = v(0.2, 0.6, 1.0);
        p.foot_rot[L] = v(-1.8, 0.25, 0.0);
        p.knee[L] = v(-0.4, 0.0, 1.0);
        p.hand[R] = v(-0.32, 1.00, -0.10);
        p.fist[R] = 0.8;
        p.hand[L] = v(0.06, 1.34, 0.26);
    });
    let hit = chamber.with(|p| {
        p.pelvis = v(1.25, -0.12, -0.32);
        p.torso = v(-0.85, -0.02, -0.34);
        p.foot[R] = v(0.12, 1.48, 0.88);
        p.foot_rot[R] = v(1.4, 1.0, -1.4);
        p.knee[R] = v(0.5, 1.0, -0.1);
        p.foot_rot[L] = v(-2.15, 0.25, 0.0);
        p.hand[R] = v(-0.40, 0.92, -0.26);
        p.hand[L] = v(0.04, 1.34, 0.24);
    });
    let over = hit.with(|p| {
        p.pelvis.x = 1.35;
        p.foot[R] = v(0.24, 1.46, 0.82);
    });
    let through = hit.with(|p| {
        p.pelvis.x = 1.5;
        p.foot[R] = v(0.40, 1.28, 0.58);
    });
    let recoil = chamber.with(|p| {
        p.foot[R] = v(-0.18, 0.55, 0.20);
        p.pelvis.x = 0.5;
    });
    Anim::new(vec![
        key(0.0, s),
        key(3.0, pivot).out(),
        key(7.0, chamber),
        key(12.0, hit).snap(),
        key(13.0, over).out(),
        key(17.0, through),
        key(24.0, recoil),
        key(31.0, s.with(|p| p.foot[R] = v(-0.19, 0.05, -0.24))),
        key(38.0, s),
    ])
    .with_lead(STRIKE)
}
fn hook() -> Anim {
    let s = base();
    // The lead hand loads wide, then swings round with the hips into the
    // side of the head, elbow up.
    let load = s.with(|p| {
        p.pelvis = v(-0.85, 0.10, 0.0);
        p.torso = v(-0.15, 0.20, 0.06);
        p.hips.z -= 0.02;
        p.hand[L] = v(0.30, 1.30, 0.20);
        p.elbow[L] = v(1.0, 0.2, -0.3);
    });
    let hit = s.with(|p| {
        p.hips = v(0.0, 0.86, 0.08);
        p.pelvis = v(0.25, 0.14, 0.0);
        p.torso = v(0.45, 0.24, -0.08);
        p.hand[L] = v(-0.05, 1.36, 0.52);
        p.elbow[L] = v(1.0, 0.6, 0.0);
        p.palm[L] = v(0.0, -1.0, 0.0);
        p.clav[L] = v(0.10, 0.35, 0.0);
        p.hand[R] = v(-0.06, 1.28, 0.18);
    });
    let through = hit.with(|p| {
        p.hand[L] = v(-0.18, 1.34, 0.46);
        p.torso.x = 0.55;
    });
    Anim::new(vec![
        key(0.0, s),
        key(3.0, load).out(),
        key(5.0, load),
        key(8.0, hit).snap(),
        key(9.0, through).out(),
        key(12.0, through),
        key(18.0, s.with(|p| p.hand[L] = v(0.12, 1.30, 0.32))).out(),
        key(28.0, s),
    ])
    .with_lead(STRIKE)
}

fn side_kick() -> Anim {
    let s = base();
    // Knee up across the body, then the lead foot drives out flat at the
    // stomach while the torso leans away.
    let chamber = s.with(|p| {
        p.hips = v(0.0, 0.88, -0.10);
        p.pelvis = v(-1.2, 0.0, 0.0);
        p.torso = v(-0.30, -0.10, 0.10);
        p.foot[L] = v(0.06, 0.55, 0.20);
        p.knee[L] = v(0.3, 0.6, 1.0);
        p.foot_rot[L] = v(1.2, 0.3, 0.0);
        p.foot_rot[R] = v(-1.4, 0.0, 0.0);
        p.hand[L] = v(0.20, 1.22, 0.10);
        p.hand[R] = v(-0.10, 1.20, 0.05);
    });
    let hit = chamber.with(|p| {
        p.hips = v(0.0, 0.90, 0.04);
        p.torso = v(-0.45, -0.30, 0.12);
        p.foot[L] = v(0.04, 0.95, 1.00);
        p.foot_rot[L] = v(1.5, -0.2, 0.0);
        p.knee[L] = v(0.2, 1.0, 0.2);
    });
    Anim::new(vec![
        key(0.0, s),
        key(5.0, chamber).out(),
        key(9.0, chamber.with(|p| p.foot[L].z = 0.26)),
        key(12.0, hit).snap(),
        key(13.0, hit.with(|p| p.foot[L].z += 0.04)).out(),
        key(16.0, hit),
        key(24.0, chamber),
        key(34.0, s),
    ])
    .with_lead(STRIKE)
}

fn low_kick() -> Anim {
    let s = base();
    // Out of the crouch the weight settles on the lead leg and the rear foot
    // snaps out low, at the shin.
    let set = s.with(|p| {
        p.hips = v(0.0, 0.70, -0.02);
        p.pelvis = v(-0.60, 0.22, 0.0);
        p.torso = v(0.12, 0.32, 0.02);
        p.head = v(-0.05, -0.12, 0.0);
        p.hand[L] = v(0.14, 1.06, 0.28);
        p.hand[R] = v(-0.08, 1.08, 0.16);
        p.foot[R] = v(-0.22, 0.03, -0.26);
        p.foot_rot[R] = v(-0.9, 0.5, 0.0);
        p.knee = [v(0.6, 0.1, 1.0), v(-0.7, 0.0, 0.5)];
    });
    let hit = s.with(|p| {
        p.hips = v(0.0, 0.68, 0.08);
        p.pelvis = v(0.30, 0.12, 0.0);
        p.torso = v(-0.12, 0.28, -0.06);
        p.head = v(0.05, -0.10, 0.0);
        p.hand[L] = v(0.18, 1.08, 0.30);
        p.hand[R] = v(-0.10, 1.10, 0.12);
        p.foot[L] = v(0.12, 0.004, 0.24);
        p.knee[L] = v(0.6, 0.1, 1.0);
        p.foot[R] = v(-0.04, 0.12, 0.90);
        p.foot_rot[R] = v(0.0, -0.4, 0.0);
        p.knee[R] = v(0.0, 1.0, 0.25);
    });
    Anim::new(vec![
        key(0.0, s),
        key(4.0, set).out(),
        key(8.0, hit).snap(),
        key(9.0, hit.with(|p| p.foot[R].z += 0.04)).out(),
        key(11.0, hit),
        key(17.0, set),
        key(24.0, s),
    ])
    .with_lead(STRIKE)
}


fn air_kick() -> Anim {
    let air = jump(0.0, 0.0);
    let chamber = air.with(|p| {
        p.foot[L] = v(0.08, 0.64, 0.34);
        p.knee[L] = v(0.1, 0.5, 1.0);
        p.foot[R] = v(-0.12, 0.42, -0.14);
        p.torso.y = 0.18;
    });
    let hit = air.with(|p| {
        p.hips = v(0.0, 1.0, 0.0);
        p.pelvis = v(-0.50, -0.35, 0.0);
        p.torso = v(0.20, -0.10, 0.0);
        p.head = v(0.0, 0.38, 0.0);
        p.foot[L] = v(0.05, 0.22, 0.86);
        p.foot_rot[L] = v(-0.2, 1.3, 0.0);
        p.knee[L] = v(0.2, 1.0, 0.3);
        p.foot[R] = v(-0.10, 0.54, 0.0);
        p.foot_rot[R] = v(-0.3, 0.9, 0.0);
        p.knee[R] = v(-0.2, 0.4, 1.0);
        p.hand[L] = v(0.32, 1.30, 0.0);
        p.fist[L] = 0.6;
        p.hand[R] = v(-0.08, 1.42, 0.22);
    });
    Anim::new(vec![
        key(0.0, air),
        key(3.0, chamber).out(),
        key(7.0, hit).snap(),
        key(8.0, hit.with(|p| p.foot[L].z += 0.04)).out(),
        key(13.0, hit),
        key(28.0, hit),
    ])
    .with_lead(STRIKE)
}

fn impulse() -> Anim {
    let s = base();
    let gather = s.with(|p| {
        p.hips = v(0.0, 0.85, -0.04);
        p.pelvis = v(-0.95, 0.05, 0.0);
        p.torso = v(-0.30, 0.10, 0.0);
        p.head = v(0.2, 0.05, 0.0);
        p.hand = [v(-0.10, 1.02, 0.06), v(-0.18, 0.93, -0.02)];
        p.elbow = [v(0.3, -1.0, 0.2), v(-0.6, -1.0, -0.4)];
        p.palm = [v(-0.2, -1.0, 0.0), v(0.2, 1.0, 0.0)];
        p.fist = [0.1, 0.1];
        p.foot_rot[R] = v(-1.0, 0.2, 0.0);
    });
    let charge = gather.with(|p| {
        p.hand = [v(-0.12, 1.03, 0.02), v(-0.20, 0.94, -0.05)];
        p.hips.y -= 0.025;
    });
    let hit = s.with(|p| {
        p.hips = v(0.0, 0.84, 0.13);
        p.pelvis = v(-0.05, 0.14, 0.0);
        p.torso = v(0.25, 0.18, 0.0);
        p.head = v(0.0, 0.05, 0.0);
        p.hand = [v(0.06, 1.26, 0.62), v(-0.06, 1.15, 0.60)];
        p.elbow = [v(0.7, -1.0, 0.0), v(-0.7, -1.0, 0.0)];
        p.palm = [v(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0)];
        p.wrist = [-1.05, -1.05];
        p.fist = [0.1, 0.1];
        p.clav = [v(0.05, 0.34, 0.0), v(0.05, 0.34, 0.0)];
        p.foot[L] = v(0.13, 0.004, 0.42);
        p.foot_rot[R] = v(-0.5, 0.75, 0.0);
        p.knee[R] = v(-0.3, 0.0, 1.0);
    });
    Anim::new(vec![
        key(0.0, s),
        key(8.0, gather).out(),
        key(15.0, charge),
        key(18.0, hit).snap(),
        key(19.0, hit.with(|p| {
            for h in 0..2 {
                p.hand[h].z += 0.03;
            }
        }))
        .out(),
        key(24.0, hit),
        key(34.0, s.with(|p| {
            p.hips.z = 0.06;
            p.foot[L] = v(0.12, 0.02, 0.36);
        })),
        key(46.0, s),
    ])
    .with_lead(STRIKE)
}

fn ground_pound() -> Anim {
    let s = base();
    let raise = s.with(|p| {
        p.hips = v(0.0, 0.99, 0.0);
        p.pelvis = v(-0.25, -0.05, 0.0);
        p.torso = v(0.10, -0.22, 0.0);
        p.head = v(0.0, -0.1, 0.0);
        p.hand = [v(0.10, 1.92, 0.12), v(-0.08, 1.92, 0.06)];
        p.elbow = [v(0.6, 0.2, -0.5), v(-0.6, 0.2, -0.5)];
        p.palm = [v(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0)];
        p.foot_rot = [v(-0.15, 0.5, 0.0), v(-0.6, 0.6, 0.0)];
    });
    let hop = raise.with(|p| {
        p.shift = v(0.0, 0.14, 0.06);
        p.foot = [v(0.10, 0.10, 0.28), v(-0.15, 0.12, -0.18)];
        p.hand = [v(0.10, 1.98, 0.16), v(-0.08, 1.98, 0.10)];
    });
    let slam = s.with(|p| {
        p.hips = v(0.0, 0.50, 0.02);
        p.pelvis = v(-0.25, 0.55, 0.0);
        p.torso = v(0.10, 0.78, 0.0);
        p.head = v(0.0, -0.4, 0.0);
        p.hand = [v(0.10, 0.06, 0.48), v(-0.10, 0.06, 0.46)];
        p.elbow = [v(0.6, 0.0, -1.0), v(-0.6, 0.0, -1.0)];
        p.palm = [v(0.0, -0.3, 1.0), v(0.0, -0.3, 1.0)];
        p.foot = [v(0.18, 0.004, 0.22), v(-0.20, 0.004, -0.22)];
        p.foot_rot = [v(-0.3, 0.0, 0.0), v(-0.8, 0.5, 0.0)];
        p.knee = [v(0.7, 0.0, 1.0), v(-0.8, 0.0, 0.6)];
    });
    Anim::new(vec![
        key(0.0, s),
        key(7.0, raise).out(),
        key(13.0, hop),
        key(18.0, slam).snap(),
        key(21.0, slam.with(|p| p.hips.y -= 0.04)).out(),
        key(26.0, slam),
        key(35.0, crouch(0.0)),
        key(44.0, s),
    ])
    .with_lead(STRIKE)
}

fn dash_forward() -> Anim {
    let s = base();
    let dip = s.with(|p| {
        p.hips.y -= 0.05;
        p.torso.y = 0.28;
    });
    let glide = s.with(|p| {
        p.hips = v(0.0, 0.84, 0.06);
        p.torso = v(0.25, 0.40, 0.0);
        p.head = v(0.0, -0.1, 0.0);
        p.foot[L] = v(0.10, 0.07, 0.36);
        p.foot_rot[L] = v(-0.1, 0.35, 0.0);
        p.foot[R] = v(-0.14, 0.16, -0.44);
        p.foot_rot[R] = v(-0.5, 1.1, 0.0);
        p.knee[R] = v(-0.4, -0.3, 1.0);
        p.hand = [v(0.08, 1.20, 0.36), v(-0.10, 1.16, 0.22)];
    });
    let land = s.with(|p| {
        p.hips.y -= 0.05;
        p.torso.y = 0.24;
        p.foot[R] = v(-0.15, 0.05, -0.18);
    });
    Anim::new(vec![
        key(0.0, s),
        key(2.0, dip).out(),
        key(5.0, glide),
        key(9.0, glide.with(|p| p.foot[L].y = 0.02)),
        key(13.0, land).ease_in(),
        key(24.0, s),
    ])
}

fn dash_back() -> Anim {
    let s = base();
    let dip = s.with(|p| p.hips.y -= 0.05);
    let hop = s.with(|p| {
        p.hips = v(0.0, 0.94, -0.06);
        p.torso = v(0.25, -0.05, 0.0);
        p.foot[L] = v(0.10, 0.14, 0.20);
        p.foot_rot[L] = v(-0.2, 0.7, 0.0);
        p.foot[R] = v(-0.16, 0.07, -0.32);
        p.foot_rot[R] = v(-0.8, 0.6, 0.0);
        p.hand = [v(0.06, 1.38, 0.26), v(-0.09, 1.34, 0.18)];
    });
    Anim::new(vec![
        key(0.0, s),
        key(2.0, dip).out(),
        key(6.0, hop),
        key(10.0, hop.with(|p| p.foot[R].y = 0.004)),
        key(14.0, s.with(|p| p.hips.y -= 0.05)).ease_in(),
        key(24.0, s),
    ])
}

fn react_head() -> Anim {
    let s = base();
    let snap = s.with(|p| {
        p.hips = v(0.0, 0.875, -0.12);
        p.pelvis = v(-0.45, -0.12, 0.02);
        p.torso = v(0.42, -0.32, 0.10);
        p.head = v(0.45, -0.60, 0.18);
        p.look = 0.0;
        p.hand = [v(0.24, 1.18, 0.18), v(-0.18, 1.20, 0.05)];
        p.elbow = [v(0.8, -0.6, 0.0), v(-0.8, -0.6, 0.0)];
        p.fist = [0.45, 0.45];
        p.foot[L] = v(0.12, 0.035, 0.26);
        p.foot_rot[L] = v(-0.2, 0.35, 0.0);
    });
    let hang = snap.with(|p| {
        p.head = v(0.25, -0.30, 0.1);
        p.torso.y = -0.22;
        p.look = 0.3;
        p.foot[L] = v(0.12, 0.004, 0.24);
        p.foot_rot[L] = v(-0.2, 0.0, 0.0);
    });
    Anim::new(vec![
        key(0.0, s),
        key(2.0, snap).out(),
        key(9.0, hang),
        key(18.0, s.with(|p| {
            p.hips.z -= 0.05;
            p.hand[L].y -= 0.05;
        })),
        key(30.0, s),
    ])
    .with_lead(WHIPLASH)
}

fn react_gut() -> Anim {
    let s = base();
    let fold = s.with(|p| {
        p.hips = v(0.0, 0.82, -0.15);
        p.pelvis = v(-0.4, -0.18, 0.0);
        p.torso = v(0.20, 0.72, 0.0);
        p.head = v(0.0, -0.30, 0.0);
        p.look = 0.2;
        p.hand = [v(0.07, 0.97, 0.22), v(-0.07, 0.94, 0.19)];
        p.elbow = [v(0.8, -0.6, 0.0), v(-0.8, -0.6, 0.0)];
        p.palm = [v(0.0, 0.0, -1.0), v(0.0, 0.0, -1.0)];
        p.fist = [0.4, 0.4];
        p.foot_rot[L] = v(-0.15, 0.3, 0.0);
        p.knee = [v(0.45, 0.0, 1.0), v(-0.6, 0.0, 0.8)];
    });
    Anim::new(vec![
        key(0.0, s),
        key(3.0, fold).out(),
        key(12.0, fold.with(|p| p.torso.y = 0.60)),
        key(22.0, s.with(|p| p.hips.z -= 0.04)),
        key(30.0, s),
    ])
    .with_lead(FOLD)
}

fn react_low() -> Anim {
    let s = base();
    let buckle = s.with(|p| {
        p.hips = v(0.0, 0.78, -0.06);
        p.torso = v(0.25, 0.30, 0.1);
        p.head = v(0.0, 0.3, 0.0);
        p.foot[L] = v(0.10, 0.12, 0.34);
        p.hand = [v(0.20, 1.18, 0.28), v(-0.18, 1.15, 0.14)];
        p.fist = [0.5, 0.5];
    });
    Anim::new(vec![key(0.0, s), key(4.0, buckle).out(), key(30.0, s)])
}

fn react_parried() -> Anim {
    let s = base();
    let reel = s.with(|p| {
        p.hips = v(0.0, 0.92, -0.14);
        p.pelvis = v(-0.20, -0.18, 0.0);
        p.torso = v(0.45, -0.38, 0.20);
        p.head = v(0.2, -0.32, 0.1);
        p.look = 0.5;
        p.hand = [v(0.40, 1.50, 0.02), v(-0.36, 1.28, -0.12)];
        p.elbow = [v(0.2, -1.0, -0.4), v(-0.2, -1.0, -0.4)];
        p.palm = [v(0.0, 0.0, 1.0), v(0.0, -1.0, 0.0)];
        p.fist = [0.2, 0.3];
        p.foot[R] = v(-0.18, 0.004, -0.42);
        p.foot[L] = v(0.10, 0.07, 0.22);
    });
    Anim::new(vec![
        key(0.0, s),
        key(4.0, reel).out(),
        key(14.0, reel.with(|p| {
            p.torso.y = -0.25;
            p.foot[L].y = 0.004;
        })),
        key(30.0, s),
    ])
    .with_lead(WHIPLASH)
}

fn react_guard_break() -> Anim {
    let s = base();
    let burst = s.with(|p| {
        p.hips = v(0.0, 0.90, -0.12);
        p.torso = v(0.2, -0.32, 0.0);
        p.head = v(0.0, -0.38, 0.0);
        p.look = 0.3;
        p.hand = [v(0.42, 1.54, 0.10), v(-0.40, 1.52, 0.02)];
        p.elbow = [v(0.3, -1.0, -0.3), v(-0.3, -1.0, -0.3)];
        p.palm = [v(0.0, 0.0, 1.0), v(0.0, 0.0, 1.0)];
        p.fist = [0.1, 0.1];
    });
    let dizzy = s.with(|p| {
        p.hips = v(0.02, 0.86, -0.05);
        p.torso = v(0.15, 0.25, -0.15);
        p.head = v(-0.2, 0.35, 0.25);
        p.look = 0.0;
        p.hand = [v(0.24, 0.98, 0.16), v(-0.22, 0.96, 0.08)];
        p.fist = [0.3, 0.3];
    });
    let sway = dizzy.with(|p| {
        p.hips.x = -0.03;
        p.torso.z = 0.15;
        p.head.z = -0.25;
    });
    Anim::new(vec![
        key(0.0, s),
        key(5.0, burst).out(),
        key(10.0, burst.with(|p| p.torso.y = -0.2)),
        key(18.0, dizzy),
        key(27.0, sway),
        key(35.0, dizzy),
        key(42.0, s),
    ])
}

fn react_wall() -> Anim {
    let s = base();
    let slam = s.with(|p| {
        p.hips = v(0.0, 0.92, -0.16);
        p.pelvis = v(-0.2, -0.20, 0.0);
        p.torso = v(0.2, -0.42, 0.0);
        p.head = v(0.0, -0.48, 0.0);
        p.look = 0.0;
        p.hand = [v(0.45, 1.32, -0.12), v(-0.42, 1.28, -0.16)];
        p.elbow = [v(0.2, -1.0, 0.0), v(-0.2, -1.0, 0.0)];
        p.palm = [v(0.0, 0.0, -1.0), v(0.0, 0.0, -1.0)];
        p.fist = [0.1, 0.1];
        p.foot[L] = v(0.10, 0.004, 0.12);
    });
    let rebound = s.with(|p| {
        p.hips = v(0.0, 0.82, 0.04);
        p.torso = v(0.2, 0.48, 0.0);
        p.head = v(0.0, 0.2, 0.0);
        p.look = 0.5;
        p.hand = [v(0.20, 1.02, 0.20), v(-0.18, 1.00, 0.12)];
        p.fist = [0.5, 0.5];
    });
    Anim::new(vec![
        key(0.0, s),
        key(3.0, slam).out(),
        key(9.0, slam.with(|p| p.torso.y = -0.36)),
        key(17.0, rebound),
        key(30.0, s),
    ])
    .with_lead(WHIPLASH)
}

fn knockout() -> Anim {
    let s = base();
    let flung = s.with(|p| {
        p.hips = v(0.0, 0.90, -0.05);
        p.pelvis = v(-0.3, -0.25, 0.0);
        p.torso = v(0.2, -0.40, 0.1);
        p.head = v(0.3, -0.55, 0.2);
        p.look = 0.0;
        p.hand = [v(0.35, 1.45, 0.30), v(-0.32, 1.52, 0.18)];
        p.elbow = [v(0.8, -0.3, -0.4), v(-0.8, -0.3, -0.4)];
        p.palm = [v(0.0, -1.0, 0.0), v(0.0, -1.0, 0.0)];
        p.fist = [0.2, 0.2];
        p.foot = [v(0.12, 0.20, 0.40), v(-0.14, 0.05, 0.05)];
        p.foot_rot = [v(0.0, 0.5, 0.0), v(-0.5, 0.6, 0.0)];
        p.spin = v(0.0, -0.45, 0.0);
    });
    let falling = flung.with(|p| {
        p.spin = v(0.15, -1.20, 0.1);
        p.shift = v(0.0, -0.45, 0.0);
        p.foot = [v(0.10, 0.40, 0.45), v(-0.14, 0.28, 0.30)];
        p.hand = [v(0.42, 1.30, 0.20), v(-0.40, 1.40, 0.10)];
    });
    let splat = lying(0.0).with(|p| {
        p.hand = [v(0.50, 1.20, -0.02), v(-0.48, 1.10, 0.08)];
        p.head = v(0.45, 0.0, 0.0);
        p.fist = [0.2, 0.15];
        p.spin.x = 0.15;
    });
    Anim::new(vec![
        key(0.0, s),
        key(9.0, flung).out(),
        key(20.0, falling),
        key(25.0, splat).ease_in(),
        key(29.0, splat.with(|p| p.shift.y += 0.08)).out(),
        key(33.0, splat).ease_in(),
        key(40.0, splat.with(|p| {
            p.foot[L] = v(0.24, 0.02, 0.26);
            p.head.x = 0.55;
        })),
    ])
}

fn victory() -> Anim {
    let s = base();
    let tall = s.with(|p| {
        p.hips = v(0.0, 0.96, 0.0);
        p.pelvis = v(-0.35, 0.0, 0.0);
        p.torso = v(0.15, -0.05, 0.0);
        p.head = v(0.0, -0.15, 0.0);
        p.hand[R] = v(-0.12, 1.05, 0.10);
        p.foot[R] = v(-0.15, 0.004, -0.18);
        p.foot_rot[R] = v(-0.8, 0.0, 0.0);
    });
    let raise = tall.with(|p| {
        p.hand[L] = v(0.14, 1.98, 0.14);
        p.elbow[L] = v(1.0, 0.0, -0.3);
        p.palm[L] = v(0.0, 0.0, 1.0);
        p.clav[L] = v(0.35, 0.0, 0.0);
        p.hand[R] = v(-0.20, 1.00, 0.02);
        p.head = v(0.0, -0.3, 0.0);
        p.look = 0.3;
    });
    Anim::new(vec![
        key(0.0, s),
        key(14.0, tall),
        key(26.0, raise).snap(),
        key(29.0, raise.with(|p| p.hand[L].y += 0.03)).out(),
        key(40.0, raise.with(|p| p.hand[L].y -= 0.02)),
    ])
}

fn defeat() -> Anim {
    let s = base();
    let tired = s.with(|p| {
        p.hips = v(0.0, 0.84, -0.12);
        p.pelvis = v(-0.25, 0.30, 0.0);
        p.torso = v(0.10, 0.55, 0.0);
        p.head = v(0.0, -0.40, 0.0);
        p.look = 0.4;
        p.hand = [v(0.14, 0.66, 0.16), v(-0.14, 0.64, 0.12)];
        p.elbow = [v(1.0, 0.0, -0.3), v(-1.0, 0.0, -0.3)];
        p.palm = [v(0.0, -1.0, 0.0), v(0.0, -1.0, 0.0)];
        p.fist = [0.2, 0.2];
        p.foot = [v(0.13, 0.004, 0.14), v(-0.14, 0.004, -0.10)];
        p.foot_rot = [v(-0.1, 0.0, 0.0), v(-0.3, 0.0, 0.0)];
        p.knee = [v(0.3, 0.0, 1.0), v(-0.3, 0.0, 1.0)];
    });
    Anim::new(vec![key(0.0, s), key(24.0, tired), key(40.0, tired)])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_attack_is_authored_to_its_frame_data() {
        let lib = library();
        for action in 0..20u32 {
            if let Some(m) = moves::attack(action) {
                let anim = lib.attack(action).expect("attack without animation");
                assert!(
                    (anim.length() - m.total as f32).abs() < 0.5,
                    "action {action}: anim {} vs total {}",
                    anim.length(),
                    m.total
                );
                assert!(lib.reach[action as usize] > 0.3, "action {action} reach");
                // The strike accelerates into contact: the last frames before
                // the startup frame cover more distance than the ones before.
                let limb = |t: f32| {
                    let p = anim.sample(t);
                    match striker(action) {
                        0 => p.hand[L],
                        1 | 4 => p.hand[R],
                        2 => p.foot[L],
                        _ => p.foot[R],
                    }
                };
                let hit = m.startup as f32;
                let late = (limb(hit) - limb(hit - 1.0)).length();
                let early = (limb(hit - 1.0) - limb(hit - 2.0)).length();
                assert!(late >= early, "action {action} decelerates into contact");
            }
        }
    }
    #[test]
    fn sampling_is_continuous_and_finite() {
        let lib = library();
        for anim in lib.attacks.iter().flatten().chain(lib.reactions.iter()) {
            let mut previous = anim.pose(0.0);
            let mut t = 0.0;
            while t <= anim.length() {
                let p = anim.pose(t);
                assert!(p.hand[0].x.is_finite() && p.hips.y.is_finite());
                let jump = (p.hand[0] - previous.hand[0]).length();
                assert!(jump < 0.35, "jump {jump} at t={t} len {}", anim.length());
                previous = p;
                t += 0.25;
            }
        }
    }
    #[test]
    fn knockdown_ends_standing() {
        let lib = library();
        let end = lib.knockdown_air.sample(1000.0);
        assert!((end.hips - base().hips).length() < 1e-4);
    }
    #[test]
    fn idle_bounce_is_a_closed_loop() {
        let a = stance(0.0);
        let b = stance(1.0);
        assert!((a.hips - b.hips).length() < 1e-5);
        assert!((stance(0.5).hips.y - a.hips.y) < -0.02);
    }
}
