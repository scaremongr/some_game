//! The textured GLB fighter, driven by authored key poses and the body solver.
//!
//! Each frame picks a clip from the authoritative fighter state, samples it on
//! the simulation's frame counter, layers procedural motion on top (walking,
//! landing, hit-stop shake, reach lunge), cross-fades clip changes in pose
//! space and solves the skeleton. The right-hand fighter plays every pose
//! mirrored so both show the same silhouette to the camera.
use super::{
    anims::{self, Library, Reaction},
    body::{BodyPose, BodyRig, LEAD, REAR},
    dancer::Character,
    feet::Feet,
    mocap::{strike_limb, strike_marks, Marks, MocapLib},
    ragdoll::{Drive, Part, Ragdoll, RagdollRig},
    timeline::Body,
};
use std::collections::HashMap;
use crate::engine::{gltf, graphics::Graphics, math3::*, pose::Joint, skeleton::Pose};
use arena_combat::{moves, Fighter};

/// Turn toward the camera so the chest reads in the side view.
const CAMERA_TURN: f32 = 0.10;
/// Share of the recoil springs shown on captured poses, which already carry
/// their own reaction.
const RECOIL: f32 = 0.6;
const GRAVITY: f32 = 5.0 * 60.0 * 60.0 / 1000.0;
/// Frames of a knockdown spent falling and lying; the rest is the get-up.
const DOWN_FALL: f32 = 22.0;
/// Frame of a knockdown at which a captured fall hits the floor.
const DOWN_LAND: f32 = 18.0;
/// The block take: forearms up in front of the face (held while guarding)
/// and the deepest point of the recoil when a blow lands on the guard.
const BLOCK_HOLD: f32 = 0.27;
const BLOCK_PEAK: f32 = 0.47;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Clip {
    Idle,
    Guard,
    Crouch,
    CrouchGuard,
    Attack(u32),
    Dash(bool),
    Air,
    React(Reaction),
    AirHit,
    Down(bool),
    /// In a thrower's grip: lifted, flipped and slammed down.
    Held,
    Knockout,
    Victory,
    Defeat,
    /// The match is won: the winner turns to the camera and dances.
    Dance,
}

/// Dance takes, played one after another while the winner celebrates.
const DANCES: [&str; 5] = ["dance", "dance2", "dance3", "dance4", "dance5"];
/// Seconds of the match end before the winner starts dancing.
const DANCE_AFTER: f32 = 2.6;
/// Body turn while dancing: nearly facing the camera.
const DANCE_YAW: f32 = 0.3;

/// Per-frame facts the scene knows and the fighter state does not.
pub struct View {
    pub dt: f32,
    pub time: f32,
    pub phase: u32,
    pub winner: i32,
    /// Seconds since the current phase began.
    pub phase_time: f32,
    pub freeze: u32,
    pub victim: usize,
    pub reaction: [Reaction; 2],
    /// Arena limits for bodies flying after a knockout.
    pub bounds: [f32; 2],
    /// Screenshot mode: no extrapolation, clips time from the frame counter.
    pub preview: bool,
    /// Picks between equivalent takes (victory and defeat poses).
    pub variant: u32,
}

struct Ko {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

/// Damped spring used for secondary motion (overlap, follow-through, recoil).
#[derive(Clone, Copy, Default)]
struct Spring {
    x: Vec3,
    v: Vec3,
}
impl Spring {
    fn step(&mut self, target: Vec3, omega: f32, zeta: f32, dt: f32) -> Vec3 {
        let steps = (dt * 240.0).ceil().clamp(1.0, 16.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let a = (target - self.x) * (omega * omega) - self.v * (2.0 * zeta * omega);
            self.v += a * h;
            self.x += self.v * h;
        }
        self.x
    }
}

/// Springs on the body channels that should lag and overshoot: the hips lead,
/// the chest follows, the head and free hands trail. Striking limbs stay exact.
#[derive(Clone, Copy, Default)]
struct Secondary {
    ready: bool,
    hips: Spring,
    pelvis: Spring,
    torso: Spring,
    head: Spring,
    hand: [Spring; 2],
}
impl Secondary {
    fn apply(&mut self, pose: &mut BodyPose, dt: f32, attacking: bool, striker: usize) {
        if !self.ready {
            let at = |x: Vec3| Spring { x, v: Vec3::ZERO };
            *self = Secondary {
                ready: true,
                hips: at(pose.hips),
                pelvis: at(pose.pelvis),
                torso: at(pose.torso),
                head: at(pose.head),
                hand: [at(pose.hand[0]), at(pose.hand[1])],
            };
        }
        let dt = dt.min(1.0 / 20.0);
        if attacking {
            // A strike is driven by the authored kinetic chain; springs would
            // only soften it. Keep them in sync so the recovery stays smooth.
            let exact = |s: &mut Spring, x: Vec3, dt: f32| {
                s.v = if dt > 0.0 { (x - s.x) * (1.0 / dt) } else { Vec3::ZERO };
                s.x = x;
            };
            exact(&mut self.hips, pose.hips, dt);
            exact(&mut self.pelvis, pose.pelvis, dt);
            exact(&mut self.torso, pose.torso, dt);
            pose.head = self.head.step(pose.head, 20.0, 0.55, dt);
        } else {
            pose.hips = self.hips.step(pose.hips, 30.0, 0.8, dt);
            pose.pelvis = self.pelvis.step(pose.pelvis, 28.0, 0.75, dt);
            pose.torso = self.torso.step(pose.torso, 20.0, 0.6, dt);
            pose.head = self.head.step(pose.head, 14.0, 0.5, dt);
        }
        for h in 0..2 {
            let exact = attacking && (striker == h || striker == 4);
            if exact {
                self.hand[h] = Spring { x: pose.hand[h], v: Vec3::ZERO };
            } else {
                pose.hand[h] = self.hand[h].step(pose.hand[h], 26.0, 0.65, dt);
            }
        }
    }
    /// Physical kick: hips velocity (m/s) and torso/head pitch velocity (rad/s).
    fn kick(&mut self, recoil: &mut Recoil, hips: Vec3, torso: f32, head: f32) {
        recoil.hips.v += hips;
        recoil.torso.v.y += torso;
        recoil.head.v.y += head;
        self.hips.v += hips;
        self.torso.v.y += torso;
        self.head.v.y += head;
    }
}

/// A captured clip bound to a fight state, with its strike timing.
struct Take {
    clip: usize,
    marks: Marks,
    duration: f32,
    /// Hips in character space at the start mark.
    start_hips: Vec3,
    /// Height of the hips when the clip stands upright.
    stand_y: f32,
    /// For strikes: forward travel of the hips at contact, and how far the
    /// striking limb reaches ahead of the hips there.
    lunge: f32,
    reach: f32,
}

/// Captured clips (Mixamo) that replace authored poses where available.
struct Captured {
    lib: MocapLib,
    takes: HashMap<String, Take>,
}

/// Which part of a take keeps its root travel.
#[derive(Clone, Copy, PartialEq)]
enum Travel {
    /// Loops and steps: travel removed, the simulation moves the body.
    InPlace,
    /// Strikes and reactions: travel kept, then eased back home.
    Return,
    /// Knockouts: the body keeps flying.
    Keep,
    /// Airborne: the simulation owns the height too. The hips are held at
    /// standing height (weight 1) so the root arc is not doubled; lower
    /// weights let the clip's own fall reach the floor.
    Air(f32),
    /// Dances: small sways kept, travel capped at 30 cm.
    Sway,
}

/// Physical recoil layered on captured poses: impulses from hits, blocks and
/// landings push the hips and pitch the chest and head, then spring back.
#[derive(Clone, Copy, Default)]
struct Recoil {
    hips: Spring,
    torso: Spring,
    head: Spring,
}
impl Recoil {
    fn step(&mut self, dt: f32) {
        let dt = dt.min(1.0 / 20.0);
        self.hips.step(Vec3::ZERO, 18.0, 0.55, dt);
        self.torso.step(Vec3::ZERO, 16.0, 0.5, dt);
        self.head.step(Vec3::ZERO, 14.0, 0.45, dt);
    }
}

struct Side {
    clip: Clip,
    clip_time: f32,
    from: BodyPose,
    blend: f32,
    blend_len: f32,
    last: BodyPose,
    secondary: Secondary,
    blockstun: u32,
    last_frame: f32,
    walk_phase: f32,
    walk_weight: f32,
    last_x: f32,
    airborne: bool,
    ko: Option<Ko>,
    root: Vec3,
    /// Bone-space cross-fade shared by captured and authored poses.
    out_locals: Pose,
    from_locals: Pose,
    fade: f32,
    fade_len: f32,
    take: Option<String>,
    recoil: Recoil,
    /// Direction of the current jump (+1 forward, -1 back, 0 straight up).
    jump_dir: f32,
    /// Launched by a throw: the flight and landing use the thrown clips.
    thrown: bool,
    /// Blockstun of the last blocked blow and render ticks since it landed:
    /// the guard recoil runs on its own clock, smooth between snapshots.
    block_total: f32,
    block_clock: f32,
    /// Physical secondary motion over the animated pose.
    ragdoll: Ragdoll,
    /// Feet planted on the floor while standing and walking.
    feet: Feet,
    /// Room directions into this body's space (placement without the origin).
    to_body: Mat4,
    /// Dancing: which take of `DANCES` plays, since when (clip time), and
    /// how far the body has turned to the camera (0..1).
    dance: usize,
    dance_start: f32,
    turn: f32,
}
impl Side {
    fn new() -> Side {
        let s = anims::base();
        Side {
            clip: Clip::Idle,
            clip_time: 0.0,
            from: s,
            blend: 1.0,
            blend_len: 0.1,
            last: s,
            secondary: Secondary::default(),
            blockstun: 0,
            last_frame: 0.0,
            walk_phase: 0.0,
            walk_weight: 0.0,
            last_x: f32::NAN,
            airborne: false,
            ko: None,
            root: Vec3::ZERO,
            out_locals: Pose { locals: Vec::new() },
            from_locals: Pose { locals: Vec::new() },
            fade: 1.0,
            fade_len: 0.1,
            take: None,
            recoil: Recoil::default(),
            jump_dir: 0.0,
            thrown: false,
            block_total: 0.0,
            block_clock: f32::MAX,
            ragdoll: Ragdoll::default(),
            feet: Feet::default(),
            to_body: Mat4::IDENTITY,
            dance: 0,
            dance_start: 0.0,
            turn: 0.0,
        }
    }
}

/// One fighter's body: textured mesh, skeleton, pose solver and the
/// captured clips retargeted onto that skeleton.
pub struct Avatar {
    pub character: Character,
    rig: BodyRig,
    /// Hands, toes, head, chest, lower spine.
    bones: [usize; 7],
    /// The hips bone (root of the body).
    hips: usize,
    /// Bones from the lower spine up (chest, arms, head).
    upper: Vec<bool>,
    captured: Option<Captured>,
    /// Joints for the physical layer (None: the rig lacks a main bone).
    ragdoll: Option<RagdollRig>,
}

/// Both fighters: picks and blends clips from the fight state and poses
/// each side's avatar (the two sides may share one).
pub struct FighterModel {
    avatars: Vec<Avatar>,
    /// Avatar shown on each side.
    which: [usize; 2],
    lib: Library,
    sides: [Side; 2],
    pose: Pose,
    pub skin: [Vec<Mat4>; 2],
    globals: [Vec<Mat4>; 2],
    pub placement: [Mat4; 2],
    /// Body-on-floor impacts since the last `take_impacts` (position, strength).
    impacts: Vec<(Vec3, f32)>,
    scratch: Pose,
    /// Clip review (screenshots): pack clip index and time for the left fighter.
    pub review: Option<(usize, f32)>,
}

impl Avatar {
    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        let character = Character::from_model(gltf::load_glb(bytes)?);
        let rig = BodyRig::new(&character.skeleton, character.transform)?;
        let find = |name: &str| {
            character
                .skeleton
                .bones
                .iter()
                .position(|b| b.name.ends_with(name))
                .ok_or_else(|| format!("fighter rig has no {name}"))
        };
        let bones = [
            find("LeftHand")?,
            find("RightHand")?,
            find("LeftToeBase")?,
            find("RightToeBase")?,
            find("Head")?,
            find("Spine2")?,
            find("Spine")?,
        ];
        let skeleton = &character.skeleton;
        let mut upper = vec![false; skeleton.len()];
        for b in 0..skeleton.len() {
            let mut at = Some(b);
            while let Some(i) = at {
                if i == bones[6] {
                    upper[b] = true;
                    break;
                }
                at = skeleton.bones[i].parent;
            }
        }
        let hips = skeleton.find_like(super::character::bone::HIPS).ok_or("fighter rig has no hips")?;
        let ragdoll = RagdollRig::new(&character.skeleton);
        Ok(Self { character, rig, bones, hips, upper, captured: None, ragdoll })
    }

    /// Loads a fight pack. Every clip becomes a take named by its pack key;
    /// strikes get their timing from the striking limb's track.
    pub fn set_captured(&mut self, bytes: &[u8]) -> Result<usize, String> {
        let lib = MocapLib::parse(bytes, &self.character.skeleton)?;
        let skeleton = &self.character.skeleton;
        let hips = skeleton
            .find_like(super::character::bone::HIPS)
            .ok_or("no hips")?;
        let tracked = [hips, self.bones[0], self.bones[1], self.bones[2], self.bones[3]];
        let mut takes = HashMap::new();
        for (i, clip) in lib.clips.iter().enumerate() {
            let tracks = lib.positions(i, skeleton, self.character.transform, &tracked);
            // Tracks: 0 hips, 1/2 hands, 3/4 feet, then two synthetic limbs:
            // 5 both hands (the one further back), 6 whichever foot leads.
            let limb_at = |f: usize, k: usize| {
                let p = &tracks[f];
                match k {
                    5 if p[1].z < p[2].z => p[1],
                    5 => p[2],
                    6 if p[3].z > p[4].z => p[3],
                    6 => p[4],
                    _ => p[k],
                }
            };
            let column = |k: usize| (0..tracks.len()).map(|f| limb_at(f, k)).collect::<Vec<_>>();
            let hip = column(0);
            let duration = clip.duration();
            let name = clip.name.as_str();
            let limb = strike_limb(name);
            let n = hip.len().max(1);
            // Standing height: falls start upright, get-ups end upright.
            let standing = match (hip.first(), hip.last()) {
                (Some(a), Some(b)) => a.y.max(b.y),
                _ => 1.0,
            };
            let lowest = hip.iter().map(|h| h.y).fold(f32::MAX, f32::min);
            let secs = |f: usize| f as f32 / clip.fps;
            let toe_high = |f: usize| tracks[f][3].y.max(tracks[f][4].y);
            let toe_low = |f: usize| tracks[f][3].y.min(tracks[f][4].y);
            let floor = (0..n).map(toe_low).fold(f32::MAX, f32::min);
            // Captured falls often shuffle or sag first: the fall proper
            // begins when the body leaves its stance (hips up, both feet
            // down). Knockdowns on the ground start at most 0.4 s before the
            // body lands; throws and juggles keep their whole flight.
            let landed = (0..n).find(|&f| hip[f].y <= lowest + 0.12).unwrap_or(0);
            let mut fall_onset = (0..landed)
                .rev()
                .find(|&f| hip[f].y >= standing - 0.08 && toe_high(f) <= floor + 0.1)
                .unwrap_or(0)
                .saturating_sub(2);
            if matches!(name, "ko" | "swept") {
                fall_onset = fall_onset.max(landed.saturating_sub((0.4 * clip.fps) as usize));
            }
            // Get-ups leave the floor and end once the body is upright again.
            let floor_last = (0..n).rev().find(|&f| hip[f].y <= lowest + 0.04).unwrap_or(0);
            let upright = (floor_last..n).find(|&f| hip[f].y >= standing - 0.05).map_or(n - 1, |f| (f + 6).min(n - 1));
            // Jumps: last foot leaves the floor, apex of the hips, first touch.
            let apex = (0..n).max_by(|&a, &b| hip[a].y.total_cmp(&hip[b].y)).unwrap_or(0);
            let grounded = |f: usize| toe_low(f) <= floor + 0.05;
            let takeoff = (0..apex).rev().find(|&f| grounded(f)).unwrap_or(0);
            let touchdown = (apex..n).find(|&f| grounded(f)).unwrap_or(n - 1);
            let marks = match limb {
                // Falls: contact is the moment the body hits the floor.
                _ if matches!(name, "ko" | "swept" | "air_hit" | "thrown") => Marks {
                    start: secs(fall_onset),
                    contact: secs(landed),
                    end: duration,
                },
                // Lying after a fall from the air: play on from the landing.
                _ if matches!(name, "air_down" | "thrown_down") => Marks {
                    start: secs(landed.saturating_sub(1)),
                    contact: secs(landed),
                    end: duration,
                },
                _ if name == "getup" => Marks {
                    start: secs(floor_last.saturating_sub(3)),
                    contact: secs(floor_last),
                    end: secs(upright),
                },
                _ if name.starts_with("jump") => Marks {
                    start: secs(takeoff),
                    contact: secs(apex.max(takeoff)),
                    end: secs(touchdown.max(apex)),
                },
                // A stomp lands when the raised foot comes back to the floor.
                _ if name == "smash" => {
                    let (foot, peak) = [3usize, 4]
                        .iter()
                        .map(|&k| (k, (0..n).max_by(|&a, &b| tracks[a][k].y.total_cmp(&tracks[b][k].y)).unwrap_or(0)))
                        .max_by(|a, b| tracks[a.1][a.0].y.total_cmp(&tracks[b.1][b.0].y))
                        .unwrap_or((3, 0));
                    let on_floor = |f: usize| tracks[f][foot].y <= floor + 0.04;
                    let lifted = (0..peak).rev().find(|&f| on_floor(f)).unwrap_or(0);
                    let stamped = (peak..n).find(|&f| on_floor(f)).unwrap_or(n - 1);
                    let marks = Marks { start: secs(lifted.saturating_sub(2)), contact: secs(stamped), end: duration };
                    match moves::attack(19) {
                        Some(m) => marks.fit(m.startup as f32 / 60.0, (m.total - m.startup) as f32 / 60.0),
                        None => marks,
                    }
                }
                // The grip: held from the whole clip.
                _ if name == "held" => Marks { start: 0.0, contact: 0.0, end: duration },
                // The throw lands on the first time both hands reach out (the
                // grab), not the later lift where they reach furthest.
                Some((k, _)) if name == "throw" => {
                    let reach = |f: usize| limb_at(f, k).z - hip[f].z;
                    let best = (0..n).map(reach).fold(f32::MIN, f32::max);
                    let grab = (0..n).find(|&f| reach(f) >= best * 0.85).unwrap_or(0);
                    let marks = Marks { start: secs(grab.saturating_sub(12)), contact: secs(grab), end: duration };
                    // The lift and slam play whole (faster than other moves),
                    // so the slam meets the victim's landing.
                    match moves::attack(4) {
                        Some(m) => Marks { end: duration, ..marks.fit(m.startup as f32 / 60.0, 1.0) },
                        None => marks,
                    }
                }
                Some((k, rising)) => {
                    let marks = strike_marks(&column(k), &hip, clip.fps, rising);
                    let mut marks = match take_action(name).and_then(moves::attack) {
                        Some(m) => marks.fit(
                            m.startup as f32 / 60.0,
                            (m.total - m.startup) as f32 / 60.0,
                        ),
                        None => marks,
                    };
                    // An air attack ends in the air: the clip's own landing
                    // would show a grounded pose while the body still flies.
                    if name == "air_kick" {
                        let hit = (marks.contact * clip.fps) as usize;
                        if let Some(land) = (hit..n).find(|&f| grounded(f)) {
                            marks.end = marks.end.min(secs(land.saturating_sub(2))).max(marks.contact + 0.05);
                        }
                    }
                    marks
                }
                None => Marks {
                    start: 0.0,
                    contact: duration * 0.3,
                    end: duration,
                },
            };
            // Anchor at the start of the strike, not the first stored frame:
            // some clips shuffle or turn before the move proper begins.
            let first_at = ((marks.start * clip.fps).round() as usize).min(hip.len().saturating_sub(1));
            let first = hip.get(first_at).copied().unwrap_or(Vec3::ZERO);
            let at = ((marks.contact * clip.fps).round() as usize).min(hip.len().saturating_sub(1));
            let (lunge, reach) = match limb {
                Some((k, _)) if !hip.is_empty() => (hip[at].z - first.z, limb_at(at, k).z - hip[at].z),
                _ => (0.0, 0.0),
            };
            takes.insert(
                clip.name.clone(),
                Take {
                    clip: i,
                    marks,
                    duration,
                    start_hips: first,
                    stand_y: standing,
                    lunge,
                    reach,
                },
            );
        }
        let count = takes.len();
        self.captured = Some(Captured { lib, takes });
        Ok(count)
    }
    fn upload(&mut self, g: &mut Graphics) {
        if self.character.mesh.is_none() {
            self.character.mesh = Some(g.upload_skinned(
                &self.character.mesh_data,
                &self.character.textures,
                &self.character.materials,
            ));
        }
    }
}

impl FighterModel {
    /// A model with one avatar, shown on both sides.
    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        let avatar = Avatar::load(bytes)?;
        Ok(Self {
            pose: avatar.character.skeleton.rest_pose(),
            scratch: avatar.character.skeleton.rest_pose(),
            avatars: vec![avatar],
            which: [0, 0],
            lib: anims::library(),
            sides: [Side::new(), Side::new()],
            skin: [vec![], vec![]],
            globals: [vec![], vec![]],
            placement: [Mat4::IDENTITY; 2],
            impacts: Vec::new(),
            review: None,
        })
    }
    /// Adds another fighter body; returns its index for `show`.
    pub fn add_avatar(&mut self, bytes: &[u8]) -> Result<usize, String> {
        self.avatars.push(Avatar::load(bytes)?);
        Ok(self.avatars.len() - 1)
    }
    /// Loads a fight pack for one avatar (see `Avatar::set_captured`).
    pub fn set_captured(&mut self, avatar: usize, bytes: &[u8]) -> Result<usize, String> {
        self.avatars.get_mut(avatar).ok_or("no such avatar")?.set_captured(bytes)
    }
    /// Chooses the avatar for each side; a side that changes body starts fresh.
    pub fn show(&mut self, which: [usize; 2]) {
        for side in 0..2 {
            let avatar = which[side].min(self.avatars.len() - 1);
            if avatar != self.which[side] {
                self.which[side] = avatar;
                self.sides[side] = Side::new();
            }
        }
    }
    pub fn avatar(&self, side: usize) -> &Avatar {
        &self.avatars[self.which[side]]
    }
    pub fn upload(&mut self, g: &mut Graphics) {
        for avatar in &mut self.avatars {
            avatar.upload(g);
        }
    }
    pub fn reset(&mut self) {
        self.sides = [Side::new(), Side::new()];
    }
    fn bone_world(&self, side: usize, bone: usize) -> Vec3 {
        self.globals[side]
            .get(bone)
            .map_or(Vec3::ZERO, |m| self.placement[side].transform_point(m.transform_point(Vec3::ZERO)))
    }
    /// World position of a body point, for trails and impact sparks.
    pub fn joint_world(&self, side: usize, joint: Joint) -> Vec3 {
        let bones = &self.avatar(side).bones;
        let bone = match joint {
            Joint::WristL => bones[0],
            Joint::WristR => bones[1],
            Joint::AnkleL => bones[2],
            Joint::AnkleR => bones[3],
            Joint::Head => bones[4],
            _ => bones[5],
        };
        self.bone_world(side, bone)
    }
    /// The limb that lands `action`, in world space. The right-hand fighter
    /// is a mirrored copy of the same model, so its canonical lead is still
    /// the left bone.
    pub fn striker_world(&self, side: usize, action: u32) -> Vec3 {
        let limb = striker_of(self.avatar(side), action);
        let bones = self.avatar(side).bones;
        let pick = |lead: usize, rear: usize, is_lead: bool| {
            self.bone_world(side, if is_lead { lead } else { rear })
        };
        match limb {
            0 => pick(bones[0], bones[1], true),
            1 => pick(bones[0], bones[1], false),
            2 => pick(bones[2], bones[3], true),
            3 => pick(bones[2], bones[3], false),
            _ => (self.bone_world(side, bones[0]) + self.bone_world(side, bones[1])) * 0.5,
        }
    }
    pub fn root(&self, side: usize) -> Vec3 {
        self.sides[side].root
    }
    pub fn take_impacts(&mut self) -> Vec<(Vec3, f32)> {
        std::mem::take(&mut self.impacts)
    }
    /// A blow on one side's body: `dir` in the room (x away from the
    /// attacker, y up, z towards the camera), `speed` in m/s.
    pub fn push(&mut self, side: usize, part: Part, dir: Vec3, speed: f32) {
        let s = &mut self.sides[side];
        let v = s.to_body.transform_direction(dir.normalize()) * speed;
        s.ragdoll.push(part, v);
    }

    fn choose(&self, side: usize, f: &Fighter, view: &View) -> Clip {
        if f.hp == 0 {
            return Clip::Knockout;
        }
        if view.phase >= 2 && view.winner >= 0 && view.phase_time > 0.7 {
            let won = view.winner as usize == side;
            if won && view.phase == 3 && view.phase_time > DANCE_AFTER {
                return Clip::Dance;
            }
            return if won {
                Clip::Victory
            } else {
                Clip::Defeat
            };
        }
        if f.held > 0 {
            return Clip::Held;
        }
        if f.down > 0 || f.action == 15 {
            let from_air = match self.sides[side].clip {
                Clip::Down(a) => a,
                // Juggled and thrown bodies are already on the floor; anyone
                // swept off their feet on the ground falls from standing.
                Clip::Air | Clip::AirHit | Clip::Held => true,
                _ => f.y > 0,
            };
            return Clip::Down(from_air);
        }
        if f.stun > 0 || f.action == 5 {
            if f.y > 0 {
                return Clip::AirHit;
            }
            return Clip::React(view.reaction[side]);
        }
        if moves::attack(f.action).is_some() {
            return Clip::Attack(f.action);
        }
        if f.action == 3 {
            return Clip::Dash(f.dash_dir == f.facing);
        }
        if f.y > 0 {
            return Clip::Air;
        }
        match (f.guard, f.crouch) {
            (true, true) => Clip::CrouchGuard,
            (false, true) => Clip::Crouch,
            (true, false) => Clip::Guard,
            _ => Clip::Idle,
        }
    }

    /// The captured take for a state, if the pack has one.
    #[allow(clippy::too_many_arguments)]
    fn take_for(&self, side: usize, clip: Clip, frame: f32, distance: f32, jump_dir: f32, thrown: bool, variant: u32) -> Option<&'static str> {
        let captured = self.avatar(side).captured.as_ref()?;
        let has = |k: &str| captured.takes.contains_key(k);
        let pick = |keys: &[&'static str]| keys.iter().copied().find(|k| has(k));
        match clip {
            Clip::Idle => pick(&["idle"]),
            Clip::Guard => pick(&["block", "guard"]),
            Clip::Crouch | Clip::CrouchGuard => pick(&["crouch"]),
            Clip::Attack(1) => {
                if distance < 0.95 {
                    pick(&["jab_s", "jab_m", "jab_l"])
                } else if distance < 1.2 {
                    pick(&["jab_m", "jab_l", "jab_s"])
                } else {
                    pick(&["jab_l", "jab_m", "jab_s"])
                }
            }
            Clip::Attack(11) => pick(&["cross"]),
            Clip::Attack(2) => pick(&["heavy"]),
            Clip::Attack(10) => pick(&["uppercut"]),
            Clip::Attack(4) => pick(&["throw"]),
            Clip::Attack(8) => pick(&["kick"]),
            Clip::Attack(9) => pick(&["sweep"]),
            Clip::Attack(12) => pick(&["roundhouse"]),
            Clip::Attack(14) => pick(&["special"]),
            Clip::Attack(13) => pick(&["air_kick"]),
            Clip::Attack(19) => pick(&["smash"]),
            Clip::Attack(16) => pick(&["low_kick", "kick"]),
            Clip::Attack(17) => pick(&["hook", "cross"]),
            Clip::Attack(18) => pick(&["side_kick", "kick"]),
            Clip::Air if jump_dir > 0.0 => pick(&["jump_fwd", "jump"]),
            Clip::Air if jump_dir < 0.0 => pick(&["jump_back", "jump"]),
            Clip::Air => pick(&["jump"]),
            Clip::Held => pick(&["held", "thrown"]),
            Clip::AirHit if thrown => pick(&["thrown", "air_hit"]),
            Clip::AirHit => pick(&["air_hit"]),
            Clip::Dash(true) => pick(&["dash_fwd"]),
            Clip::Dash(false) => pick(&["dash_back"]),
            Clip::React(Reaction::Head) => pick(&["hit_head"]),
            Clip::React(Reaction::HeadLight) => pick(&["hit_light", "hit_head"]),
            Clip::React(Reaction::HeadSide) => pick(&["hit_side", "hit_head"]),
            Clip::React(Reaction::Spin) => pick(&["hit_spin", "hit_head"]),
            Clip::React(Reaction::Gut) => pick(&["hit_gut"]),
            Clip::React(Reaction::GutBig) => pick(&["gut_big", "hit_gut"]),
            Clip::React(Reaction::Parried | Reaction::Pushed) => pick(&["stagger"]),
            Clip::React(Reaction::GuardBreak) => pick(&["dizzy"]),
            Clip::React(Reaction::Wall) => pick(&["hit_wall", "hit_head"]),
            Clip::Down(from_air) => {
                if frame >= DOWN_FALL {
                    pick(&["getup"])
                } else if !from_air {
                    pick(&["swept"])
                } else if thrown {
                    pick(&["thrown_down", "air_down"])
                } else {
                    pick(&["air_down"])
                }
            }
            Clip::Knockout => pick(&["ko"]),
            Clip::Victory => match variant % 3 {
                1 => pick(&["victory2", "victory"]),
                2 => pick(&["victory3", "victory"]),
                _ => pick(&["victory"]),
            },
            Clip::Defeat => match variant % 2 {
                1 => pick(&["defeat2", "defeat"]),
                _ => pick(&["defeat"]),
            },
            Clip::Dance => {
                let n = DANCES.len();
                let start = self.sides[side].dance;
                (0..n).map(|i| DANCES[(start + i) % n]).find(|k| has(k)).or_else(|| pick(&["victory3", "victory"]))
            }
            _ => None,
        }
    }

    fn blend_time(from: Clip, to: Clip) -> f32 {
        match (from, to) {
            (_, Clip::Attack(_)) => 0.035,
            (_, Clip::React(_)) | (_, Clip::AirHit) => 0.04,
            (Clip::Attack(_), _) | (Clip::Dash(_), _) => 0.10,
            (Clip::Air, _) | (_, Clip::Air) => 0.08,
            (_, Clip::Down(_)) => 0.08,
            (_, Clip::Knockout) => 0.06,
            (_, Clip::Victory) | (_, Clip::Defeat) => 0.35,
            (_, Clip::Dance) => 0.6,
            _ => 0.12,
        }
    }

    pub fn update(&mut self, fighters: &[Fighter; 2], bodies: &[Body; 2], view: &View) {
        for side in 0..2 {
            let f = &fighters[side];
            let other = &fighters[1 - side];
            let body = bodies[side];
            let clip = self.choose(side, f, view);
            // A jump keeps the direction it started with; a throw's victim
            // keeps the thrown clips until it is back on its feet.
            let previous = &self.sides[side];
            let jump_dir = if previous.clip == Clip::Air {
                previous.jump_dir
            } else if f.vx * f.facing > 4 {
                1.0
            } else if f.vx * f.facing < -4 {
                -1.0
            } else {
                0.0
            };
            let thrown = match clip {
                Clip::Held => true,
                Clip::AirHit => previous.thrown || (previous.clip != Clip::AirHit && other.action == 4),
                Clip::Down(true) => previous.thrown,
                _ => false,
            };
            if clip == Clip::Dance {
                let length = self
                    .take_for(side, clip, 0.0, 0.0, 0.0, false, 0)
                    .and_then(|k| Some(self.avatar(side).captured.as_ref()?.takes.get(k)?.duration));
                let s = &mut self.sides[side];
                if s.clip != Clip::Dance {
                    // A different opening dance each match.
                    s.dance = (view.time * 7.0) as usize % DANCES.len();
                    s.dance_start = 0.0;
                } else if length.is_some_and(|length| s.clip_time + view.dt - s.dance_start >= length) {
                    // One dance after another; the take change cross-fades.
                    s.dance += 1;
                    s.dance_start = s.clip_time + view.dt;
                }
            }
            let take_key = self.take_for(side, clip, body.frame, (bodies[1 - side].x - body.x).abs(), jump_dir, thrown, view.variant);
            let dt = view.dt;
            let av = &self.avatars[self.which[side]];
            let s = &mut self.sides[side];
            s.jump_dir = jump_dir;
            s.thrown = thrown;
            let turn = if clip == Clip::Dance { 1.0 } else { 0.0 };
            s.turn += (turn - s.turn) * (dt * 2.5).min(1.0);
            let started = clip != s.clip;
            if started {
                s.blend_len = Self::blend_time(s.clip, clip);
                s.from = s.last;
                s.blend = 0.0;
                s.clip = clip;
                s.clip_time = 0.0;
            } else {
                s.clip_time += dt;
            }
            let frame = body.frame;
            // A new hit while already reeling restarts the reaction.
            let rehit = !started
                && matches!(clip, Clip::React(_) | Clip::AirHit)
                && frame + 0.5 < s.last_frame;
            if rehit {
                s.from = s.last;
                s.blend = 0.0;
                s.blend_len = 0.03;
                s.clip_time = 0.0;
            }
            let started = started || rehit;
            let clip_frames = if view.preview { frame } else { s.clip_time * 60.0 };
            let previous_frame = if started { -1.0 } else { s.last_frame };
            s.last_frame = if clip == Clip::Knockout { clip_frames } else { frame };

            // Walking is driven by distance, so planted feet stay put.
            let x = body.x;
            let facing = f.facing.signum() as f32;
            if s.last_x.is_nan() {
                s.last_x = x;
            }
            let moved = (x - s.last_x) * facing;
            s.last_x = x;
            let grounded_neutral = matches!(
                clip,
                Clip::Idle | Clip::Guard | Clip::Crouch | Clip::CrouchGuard
            );
            let speed = if dt > 0.0 { moved.abs() / dt } else { 0.0 };
            let walking = grounded_neutral && speed > 0.25 && f.previous & 3 != 0;
            let stride = if f.guard || f.crouch { 0.45 } else { 0.72 };
            if grounded_neutral {
                s.walk_phase += moved / stride;
            }
            let target = if walking { 1.0 } else { 0.0 };
            s.walk_weight += (target - s.walk_weight) * (dt * 14.0).min(1.0);

            // Physical impulses: landings, blocks and the first frame of a hit.
            let air = body.y > 0.002;
            let landed = s.airborne && !air && f.down == 0 && f.hp > 0;
            s.airborne = air;
            if landed && !view.preview {
                s.secondary.kick(&mut s.recoil, vec3(0.0, -1.7, 0.0), 2.5, 1.5);
                self.impacts.push((vec3(x, 0.0, 0.0), 0.45));
            }
            if f.blockstun > s.blockstun {
                s.secondary.kick(&mut s.recoil, vec3(0.0, -0.2, -0.7), -2.8, -2.0);
                s.block_total = f.blockstun as f32;
                s.block_clock = 0.0;
            }
            s.blockstun = f.blockstun;
            s.block_clock += dt * 60.0;
            // Guard pose on the block take: the held guard breathes a little;
            // a blocked blow rocks it back and returns, deeper for heavier
            // blows (longer blockstun).
            let block_time = {
                let recoil = if view.preview {
                    if f.blockstun > 0 { 1.0 - f.blockstun as f32 / 16.0 } else { 1.0 }
                } else {
                    (s.block_clock / s.block_total.max(1.0)).min(1.0)
                };
                let depth = (s.block_total / 20.0).clamp(0.4, 1.0);
                let breathe = if view.preview { 0.0 } else { (view.time * 2.3 + side as f32).sin() * 0.02 };
                BLOCK_HOLD + breathe * (1.0 - recoil) + (BLOCK_PEAK - BLOCK_HOLD) * depth * (recoil * std::f32::consts::PI).sin()
            };
            if started && !view.preview {
                match clip {
                    Clip::React(kind) => {
                        let (hips, torso, head) = match kind {
                            Reaction::Head => (vec3(0.0, 0.0, -0.9), -4.5, -10.0),
                            Reaction::HeadLight => (vec3(0.0, 0.0, -0.5), -2.5, -7.0),
                            Reaction::HeadSide => (vec3(0.0, 0.0, -0.7), -3.0, -9.0),
                            Reaction::Spin => (vec3(0.0, 0.1, -1.2), -6.0, -11.0),
                            Reaction::Gut => (vec3(0.0, -0.3, -1.0), 7.0, 5.0),
                            Reaction::GutBig => (vec3(0.0, -0.4, -1.4), 9.0, 7.0),
                            Reaction::Low => (vec3(0.0, -0.9, -0.3), 2.5, 2.0),
                            Reaction::Wall => (vec3(0.0, 0.0, -1.2), -7.0, -9.0),
                            Reaction::GuardBreak => (vec3(0.0, 0.2, -1.0), -6.0, -7.0),
                            Reaction::Parried | Reaction::Pushed => {
                                (vec3(0.0, 0.1, -1.0), -5.0, -5.0)
                            }
                        };
                        s.secondary.kick(&mut s.recoil, hips, torso, head);
                    }
                    Clip::AirHit => s.secondary.kick(&mut s.recoil, vec3(0.0, 0.5, -1.0), -6.0, -8.0),
                    Clip::Knockout => s.secondary.kick(&mut s.recoil, vec3(0.0, 0.0, -1.3), -8.0, -10.0),
                    _ => {}
                }
            }

            // Fighting-game idle rhythm: one bounce every 0.9 s, sides out of phase.
            let beat = view.time / 0.9 + side as f32 * 0.37;
            let lib = &self.lib;
            let mut pose = match clip {
                Clip::Idle => anims::stance(beat),
                Clip::Guard => anims::guard(beat),
                Clip::Crouch => anims::crouch(beat),
                Clip::CrouchGuard => anims::crouch_guard(beat),
                Clip::Attack(action) => lib.attack(action).unwrap().pose(frame),
                Clip::Dash(forward) => {
                    let anim = if forward { &lib.dash_forward } else { &lib.dash_back };
                    anim.pose(frame)
                }
                Clip::Air => {
                    let forward = if f.vx.abs() > 8 { (f.vx * f.facing) as f32 } else { 0.0 };
                    anims::jump(body.vy, forward)
                }
                Clip::AirHit => anims::air_hit(body.vy, frame),
                Clip::React(kind) => {
                    let anim = lib.reaction(kind);
                    let t = if kind == Reaction::Wall {
                        clip_frames
                    } else {
                        let total = (f.frame + f.stun).max(1) as f32;
                        let impact = kind.impact_frames();
                        if frame < impact {
                            frame
                        } else {
                            impact
                                + (frame - impact) * (anim.length() - impact)
                                    / (total - impact).max(1.0)
                        }
                    };
                    anim.pose(t)
                }
                Clip::Down(from_air) => {
                    let anim = if from_air { &lib.knockdown_air } else { &lib.knockdown_ground };
                    anim.pose(frame)
                }
                // Without a captured clip the grip reads as a doubled-over reaction.
                Clip::Held => lib.reaction(Reaction::Gut).pose(frame.min(12.0)),
                Clip::Knockout => lib.ko.pose(clip_frames),
                Clip::Victory | Clip::Dance => lib.victory.pose(clip_frames),
                Clip::Defeat => lib.defeat.pose(clip_frames),
            };
            // The body meets the floor: dust where the back lands (when the
            // captured fall touches down, if one is playing).
            let landing = take_key
                .and_then(|key| av.captured.as_ref()?.takes.get(key))
                .filter(|t| matches!(clip, Clip::Knockout | Clip::Down(false)) && t.marks.contact > t.marks.start)
                .map(|t| (t.marks.contact - t.marks.start) * 60.0);
            let crossed = |at: f32| previous_frame < at && s.last_frame >= at;
            let floor_hit = match clip {
                Clip::Down(true) => started,
                Clip::Down(false) => crossed(if landing.is_some() { DOWN_LAND } else { 9.0 }),
                Clip::Knockout => landing.is_some_and(crossed),
                _ => false,
            };
            if floor_hit && !view.preview {
                self.impacts.push((vec3(x - facing * 0.45, 0.0, 0.0), 1.0));
            }

            if grounded_neutral {
                anims::walk_layer(&mut pose, s.walk_phase, s.walk_weight, stride);
                if f.guard && f.blockstun > 0 {
                    let k = (f.blockstun as f32 / 12.0).min(1.0);
                    pose.hips.z -= 0.04 * k;
                    pose.torso.y -= 0.08 * k;
                    for h in 0..2 {
                        pose.hand[h].z -= 0.03 * k;
                    }
                }
            }
            // Springy torso recoil from the authoritative hit impulse.
            if !matches!(clip, Clip::Knockout | Clip::Down(_)) {
                pose.torso.y -= f.recoil as f32 / 2000.0;
            }
            // Reach: the fighter steps in so blows land on the body. The support
            // foot lifts and plants before contact (and steps back during the
            // recovery); the other foot stays planted, so nothing skates.
            let mut pop = 0.0;
            if let (Clip::Attack(action), Some(m)) = (clip, moves::attack(f.action)) {
                let hit = m.startup as f32;
                let active = hit + m.active as f32;
                let end = m.total as f32;
                pop = ((frame - hit + 1.0) / 1.5).clamp(0.0, 1.0)
                    * (1.0 - ((frame - hit - 1.5) / 3.0).clamp(0.0, 1.0));
                if action != 19 {
                    let distance = (bodies[1 - side].x - x).abs();
                    let depth = match m.height {
                        moves::Height::Low => 0.30,
                        _ => 0.16,
                    };
                    let gap = (distance - depth - lib.reach[action as usize]).clamp(-0.25, 0.36);
                    let smooth = |u: f32| {
                        let u = u.clamp(0.0, 1.0);
                        u * u * (3.0 - 2.0 * u)
                    };
                    let body_start = (hit - 6.0).max(0.0);
                    let body_in = smooth((frame - body_start) / (hit - 1.0 - body_start).max(1.0));
                    let body_out = smooth((frame - active - 2.0) / (end - active - 6.0).max(1.0));
                    let d_body = gap * body_in * (1.0 - body_out);
                    let step_start = (hit - 7.0).max(0.0);
                    let step_land = (hit - 2.0).max(step_start + 1.0);
                    let back_start = active + 3.0;
                    let back_land = (end - 5.0).max(back_start + 1.0);
                    let u_in = ((frame - step_start) / (step_land - step_start)).clamp(0.0, 1.0);
                    let u_out = ((frame - back_start) / (back_land - back_start)).clamp(0.0, 1.0);
                    let d_step = gap * smooth(u_in) * (1.0 - smooth(u_out));
                    let arc = |u: f32| if u > 0.0 && u < 1.0 { (u * std::f32::consts::PI).sin() } else { 0.0 };
                    let lift = (arc(u_in) + arc(u_out)) * 0.07 * (gap.abs() / 0.3).min(1.0);
                    pose.hips.z += d_body;
                    for h in 0..2 {
                        pose.hand[h].z += d_body;
                    }
                    let (support, kicking) = match anims::striker(action) {
                        2 => (REAR, Some(LEAD)),
                        3 => (LEAD, Some(REAR)),
                        _ => (LEAD, None),
                    };
                    if let Some(k) = kicking {
                        pose.foot[k].z += d_body;
                    }
                    pose.foot[support].z += d_step;
                    pose.foot[support].y += lift;
                    pose.foot_rot[support].y += lift * 3.0;
                }
            }

            // Cross-fades happen in bone space below, for every kind of pose.
            s.blend = 1.0;
            let w = s.blend * s.blend * (3.0 - 2.0 * s.blend);
            let mut pose = if w < 1.0 { s.from.lerp(&pose, w) } else { pose };
            s.last = pose;
            if view.preview {
                s.secondary.ready = false;
                s.recoil = Recoil::default();
            } else {
                s.recoil.step(dt);
                let (attacking, striker) = match clip {
                    Clip::Attack(a) => (true, anims::striker(a)),
                    _ => (false, 9),
                };
                s.secondary.apply(&mut pose, dt, attacking, striker);
            }
            // Hit-stop: the victim shudders in place.
            if view.freeze > 0
                && view.victim == side
                && matches!(clip, Clip::React(_) | Clip::AirHit)
            {
                pose.shift.z += (view.time * 95.0).sin() * 0.022;
            }

            // Root: authoritative position, or our own flight after a knockout.
            let mut root = vec3(x, body.y, 0.0);
            if clip == Clip::Knockout {
                let ko = s.ko.get_or_insert(Ko {
                    x,
                    y: body.y,
                    vx: f.vx as f32 * 0.06 - facing * 1.8,
                    vy: (f.vy as f32 * 0.06).max(1.2),
                });
                if !view.preview {
                    let was_airborne = ko.y > 0.0;
                    ko.vy -= GRAVITY * dt;
                    ko.x += ko.vx * dt;
                    ko.y = (ko.y + ko.vy * dt).max(0.0);
                    if ko.y <= 0.0 {
                        ko.vy = 0.0;
                        ko.vx *= (1.0 - dt * 5.0).max(0.0);
                    }
                    if ko.x < view.bounds[0] || ko.x > view.bounds[1] {
                        ko.x = ko.x.clamp(view.bounds[0], view.bounds[1]);
                        ko.vx *= -0.12;
                    }
                    // Never slide back through the winner.
                    let other = bodies[1 - side].x;
                    if (ko.x - other) * facing > -0.55 {
                        ko.x = other - facing * 0.55;
                        ko.vx = 0.0;
                    }
                    if was_airborne && ko.y <= 0.0 && landing.is_none() {
                        self.impacts.push((vec3(ko.x - facing * 0.45, 0.0, 0.0), 1.3));
                    }
                }
                root = vec3(ko.x, ko.y, 0.0);
            } else {
                s.ko = None;
            }
            s.root = root;

            // The right-hand fighter is drawn as a mirror image of the model,
            // so every pose (authored or captured) is solved unmirrored.
            let mirrored = facing < 0.0;
            let solved = pose;
            let distance = (bodies[1 - side].x - x).abs().max(0.6);
            let other_head = bodies[1 - side].y - body.y
                + if other.crouch {
                    1.0
                } else if other.down > 0 {
                    0.3
                } else {
                    1.42
                };
            let look = vec3(0.0, other_head, distance);
            let captured_pose = match (take_key, &av.captured) {
                (Some(key), Some(captured)) => {
                    let take = &captured.takes[key];
                    let (t, travel) = match clip {
                        Clip::Attack(action) => {
                            let m = moves::attack(action).unwrap();
                            let hit = m.startup as f32;
                            let open = (m.startup + m.active) as f32;
                            let t = if action == 4 && !f.connected && frame > open {
                                // A missed grab pulls its hands back instead of
                                // hauling an opponent who is not there.
                                let u = ((frame - open) / (m.total as f32 - open)).clamp(0.0, 1.0);
                                take.marks.contact - (take.marks.contact - take.marks.start) * u * u * (3.0 - 2.0 * u)
                            } else if frame <= hit {
                                take.marks.start + (take.marks.contact - take.marks.start) * (frame / hit)
                            } else {
                                take.marks.contact
                                    + (take.marks.end - take.marks.contact)
                                        * ((frame - hit) / (m.total as f32 - hit)).min(1.0)
                            };
                            (t, if action == 13 { Travel::Air(1.0) } else { Travel::Return })
                        }
                        Clip::Air => {
                            // Progress along the ballistic arc from its launch
                            // speed: takeoff and touchdown line up with the
                            // clip's whatever the jump height. The simulation
                            // owns the height, so flips turn at an even pace.
                            let (y, vy) = (body.y.max(0.0) * 1000.0, body.vy);
                            let v0 = (vy * vy + 10.0 * y).sqrt().max(1.0);
                            // Flips finish just before touchdown, feet first.
                            let u = ((v0 - vy) / (2.0 * v0) * 1.1).clamp(0.0, 1.0);
                            (take.marks.start + (take.marks.end - take.marks.start) * u, Travel::Air(1.0))
                        }
                        Clip::AirHit => {
                            // The flight lands when the simulated body does.
                            let (y, vy) = (body.y.max(0.0) * 1000.0, body.vy);
                            let land = (vy + (vy * vy + 10.0 * y).sqrt()) / 5.0;
                            let u = (frame / (frame + land).max(1.0)).clamp(0.0, 1.0);
                            let t = take.marks.start + (take.marks.contact - take.marks.start) * u;
                            (t, Travel::Air(1.0 - u))
                        }
                        Clip::React(_) => {
                            // Natural speed, slightly quicker; the tail is cut when
                            // the stun ends and the idle cross-fades in.
                            (frame / 60.0 * 1.25, Travel::Return)
                        }
                        Clip::Down(_) => {
                            let t = if frame >= DOWN_FALL {
                                let rise = arena_combat::KNOCKDOWN as f32 - DOWN_FALL;
                                let u = ((frame - DOWN_FALL) / rise).clamp(0.0, 1.0);
                                take.marks.start + (take.marks.end - take.marks.start) * u
                            } else if key == "swept" {
                                // The fall is timed to land on DOWN_LAND.
                                let fall = take.marks.contact - take.marks.start;
                                take.marks.start
                                    + fall * (frame / DOWN_LAND).min(1.0)
                                    + (frame - DOWN_LAND).max(0.0) / 60.0
                            } else {
                                // Already on the floor after a flight.
                                take.marks.start + frame / 60.0
                            };
                            (t, Travel::InPlace)
                        }
                        Clip::Dash(_) => (take.duration * (frame / 24.0).min(1.0), Travel::InPlace),
                        // The whole grip plays over the hold: lifted, flipped,
                        // on the floor exactly when the throw releases.
                        Clip::Held => {
                            let u = (frame / arena_combat::HOLD as f32).clamp(0.0, 1.0);
                            (take.marks.start + (take.marks.end - take.marks.start) * u, Travel::InPlace)
                        }
                        Clip::Knockout => (take.marks.start + clip_frames / 60.0, Travel::Keep),
                        Clip::Victory | Clip::Defeat => (clip_frames / 60.0, Travel::InPlace),
                        Clip::Dance => ((s.clip_time - s.dance_start).max(0.0), Travel::Sway),
                        Clip::Guard if key == "block" => (block_time, Travel::InPlace),
                        _ => ((view.time + side as f32 * 0.4).rem_euclid(take.duration.max(0.1)), Travel::InPlace),
                    };
                    let t = t.clamp(0.0, take.duration);
                    Some((take.clip, t, travel))
                }
                _ => None,
            };
            let captured_pose = match (self.review, &av.captured) {
                (Some((index, t)), Some(captured)) if side == 0 && index < captured.lib.clips.len() => {
                    Some((index, t.min(captured.lib.clips[index].duration()), Travel::InPlace))
                }
                _ => captured_pose,
            };
            if let (Some((clip_index, t, travel)), Some(captured)) = (captured_pose, &av.captured) {
                let take = captured.takes.values().find(|k| k.clip == clip_index).unwrap();
                // Root: the simulation owns the position. Loops and steps keep
                // the hips over it; strikes keep their step-in, scaled so the
                // blow lands on the opponent at the current distance.
                let hips_now = av.character.transform.transform_point(captured.lib.hips_at(clip_index, t));
                let moved_in = vec3(hips_now.x - take.start_hips.x, 0.0, hips_now.z - take.start_hips.z);
                let keep = match (clip, travel) {
                    (_, Travel::InPlace | Travel::Air(_) | Travel::Sway) => 0.0,
                    (_, Travel::Keep) => 1.0,
                    (Clip::Attack(action), Travel::Return) => {
                        let m = moves::attack(action);
                        let depth = match m.map(|m| m.height) {
                            Some(moves::Height::Low) => 0.30,
                            _ => 0.16,
                        };
                        // The body travels only as far as the blow needs to
                        // land; a blow that cannot reach (a whiff) barely
                        // steps. Blended over 15 cm of range, so no pop.
                        let distance = (bodies[1 - side].x - x).abs();
                        let wanted = distance - depth - take.reach;
                        let reach = m.map_or(0.0, |m| m.reach as f32 / 1000.0);
                        let lands = if f.connected { 1.0 } else { (1.0 - (distance - reach) / 0.15).clamp(0.0, 1.0) };
                        let wanted = wanted.min(0.15) + (wanted - wanted.min(0.15)) * lands;
                        if take.lunge > 0.05 { (wanted / take.lunge).clamp(0.0, 1.0) } else { 1.0 }
                    }
                    _ => 0.5,
                };
                let mut shift = match travel {
                    Travel::InPlace => vec3(-hips_now.x, 0.0, -hips_now.z),
                    // Dances keep their sway but not their travel.
                    Travel::Sway => {
                        let away = moved_in.length();
                        let keep = if away > 0.3 { moved_in * (0.3 / away) } else { moved_in };
                        vec3(-hips_now.x, 0.0, -hips_now.z) + keep
                    }
                    Travel::Air(w) => vec3(-hips_now.x, (take.stand_y - hips_now.y) * w, -hips_now.z),
                    _ => vec3(-take.start_hips.x, 0.0, -take.start_hips.z) - moved_in * (1.0 - keep),
                };
                // The rising uppercut is captured on the spot: when it will
                // connect, the body rises into the opponent over the startup
                // and settles back in the recovery. Other strikes stay put.
                if let (Clip::Attack(10), Travel::Return, Some(m)) = (clip, travel, moves::attack(10)) {
                    let distance = (bodies[1 - side].x - x).abs();
                    if distance * 1000.0 <= m.reach as f32 + 100.0 {
                        let short = (distance - 0.16 - take.reach - take.lunge.max(0.0)).clamp(0.0, 0.3);
                        let (hit, active, end) = (m.startup as f32, (m.startup + m.active) as f32, m.total as f32);
                        let smooth = |u: f32| {
                            let u = u.clamp(0.0, 1.0);
                            u * u * (3.0 - 2.0 * u)
                        };
                        let into = smooth(frame / (hit - 1.0));
                        let back = smooth((frame - active - 2.0) / (end - active - 6.0));
                        shift.z += short * into * (1.0 - back);
                    }
                }
                if view.freeze > 0 && view.victim == side && matches!(clip, Clip::React(_) | Clip::AirHit) {
                    shift.z += (view.time * 95.0).sin() * 0.022;
                }
                // Physical layer: hit and landing impulses, block pushback.
                let block = if f.guard && f.blockstun > 0 { (f.blockstun as f32 / 12.0).min(1.0) } else { 0.0 };
                shift += vec3(0.0, s.recoil.hips.x.y, s.recoil.hips.x.z) * RECOIL - vec3(0.0, 0.0, 0.06 * block);
                let body_lean = if matches!(clip, Clip::Knockout | Clip::Down(_)) {
                    0.0
                } else {
                    s.recoil.torso.x.y * RECOIL - 0.2 * block - f.recoil as f32 / 2000.0
                };
                let head_lean = s.recoil.head.x.y * RECOIL * 0.5;
                let to_model = av.character.transform.invert();
                let root_model = to_model.transform_direction(shift);
                self.pose.locals.clear();
                self.pose.locals.extend(av.character.skeleton.bones.iter().map(|b| b.bind_local));
                captured.lib.sample(clip_index, t, root_model, &mut self.pose);
                // Stance breathes into the walk cycle as the fighter moves
                // (a rig with planted feet steps by itself instead).
                let crouched = matches!(clip, Clip::Crouch | Clip::CrouchGuard);
                if (matches!(clip, Clip::Idle | Clip::Guard) || crouched) && av.ragdoll.is_none() {
                    let key = match (crouched, moved >= 0.0) {
                        (false, true) => "walk_fwd",
                        (false, false) => "walk_back",
                        (true, true) => "crouch_walk_fwd",
                        (true, false) => "crouch_walk_back",
                    };
                    if let Some(walk) = captured.takes.get(key).filter(|_| s.walk_weight > 0.01) {
                        let wt = s.walk_phase.rem_euclid(1.0) * walk.duration;
                        let walk_hips = av
                            .character
                            .transform
                            .transform_point(captured.lib.hips_at(walk.clip, wt));
                        let w_shift = vec3(-walk_hips.x, 0.0, -walk_hips.z);
                        self.scratch.locals.clear();
                        self.scratch.locals.extend(av.character.skeleton.bones.iter().map(|b| b.bind_local));
                        captured.lib.sample(walk.clip, wt, to_model.transform_direction(w_shift), &mut self.scratch);
                        let idle = self.pose.clone();
                        self.pose.blend_from(&idle, &self.scratch, s.walk_weight);
                    }
                }
                // Crouching: legs and hips come from the crouch clip, the
                // chest, arms and head from the stance or guard, so the fists
                // stay up the way a fighter crouches.
                if crouched {
                    let block = clip == Clip::CrouchGuard && captured.takes.contains_key("block");
                    let top = match (clip, block) {
                        (_, true) => "block",
                        (Clip::CrouchGuard, false) => "guard",
                        _ => "idle",
                    };
                    if let Some(top) = captured.takes.get(top) {
                        let tt = if block {
                            block_time
                        } else {
                            (view.time + side as f32 * 0.4).rem_euclid(top.duration.max(0.1))
                        };
                        self.scratch.locals.clear();
                        self.scratch.locals.extend(av.character.skeleton.bones.iter().map(|b| b.bind_local));
                        captured.lib.sample(top.clip, tt, Vec3::ZERO, &mut self.scratch);
                        for (bone, &upper) in av.upper.iter().enumerate() {
                            if upper {
                                self.pose.locals[bone].rotation = self.scratch.locals[bone].rotation;
                            }
                        }
                    }
                }
                // The uppercut rises out of a crouch: legs and hips start in
                // the crouch clip and straighten by the hit, the punch itself
                // comes from the uppercut clip.
                if let (Clip::Attack(10), Some(low)) = (clip, captured.takes.get("crouch")) {
                    let hit = moves::attack(10).map_or(13.0, |m| m.startup as f32);
                    let u = (frame / hit).clamp(0.0, 1.0);
                    let w = 1.0 - u * u * (3.0 - 2.0 * u);
                    if w > 0.01 {
                        self.scratch.locals.clear();
                        self.scratch.locals.extend(av.character.skeleton.bones.iter().map(|b| b.bind_local));
                        captured.lib.sample(low.clip, 0.0, Vec3::ZERO, &mut self.scratch);
                        for (bone, &upper) in av.upper.iter().enumerate() {
                            if upper {
                                continue;
                            }
                            let (a, b) = (self.pose.locals[bone], self.scratch.locals[bone]);
                            self.pose.locals[bone].rotation = a.rotation.slerp(b.rotation, w).normalize();
                            if bone == av.hips {
                                self.pose.locals[bone].translation.y += (b.translation.y - a.translation.y) * w;
                            }
                        }
                    }
                }
                let open = matches!(clip, Clip::Attack(14) | Clip::Attack(4));
                for hand in 0..2 {
                    av.rig.curl_fingers(&mut self.pose, hand, if open { 0.2 } else { 1.0 });
                }
                let (spine, head) = (av.bones[6], av.bones[4]);
                if body_lean.abs() > 1e-3 {
                    lean(&av.character, &mut self.pose, &mut self.globals[side], spine, body_lean);
                }
                if head_lean.abs() > 1e-3 {
                    lean(&av.character, &mut self.pose, &mut self.globals[side], head, head_lean);
                }
                if clip == Clip::Knockout {
                    root = vec3(s.ko.as_ref().map_or(x, |k| k.x), 0.0, 0.0);
                }
            } else {
                av.rig.solve(&solved, look, &mut self.pose);
            }
            s.root = root;
            // Bone-space cross-fade between whatever came before and this pose.
            let key = take_key.map(|k| k.to_string());
            if s.out_locals.locals.len() != self.pose.locals.len() {
                s.out_locals = self.pose.clone();
            }
            if started || key != s.take {
                s.from_locals = s.out_locals.clone();
                s.fade = 0.0;
                s.fade_len = if key.is_some() != s.take.is_some() { s.blend_len.max(0.08) } else { s.blend_len };
                s.take = key;
            }
            s.fade = if view.preview { 1.0 } else { (s.fade + dt / s.fade_len.max(1e-3)).min(1.0) };
            if s.fade < 1.0 {
                let w = s.fade * s.fade * (3.0 - 2.0 * s.fade);
                let target = self.pose.clone();
                self.pose.blend_from(&s.from_locals, &target, w);
            }
            // Impact frames: the striking fist or foot swells for a few frames,
            // the 3D take on a 2D smear, so the contact reads at 60 fps.
            if let (Clip::Attack(action), true) = (clip, pop > 0.0 && !view.preview) {
                let limbs: &[usize] = match striker_of(av, action) {
                    4 => &[0, 1],
                    0 => &[0],
                    1 => &[1],
                    2 => &[2],
                    _ => &[3],
                };
                let k = 1.0 + 0.28 * pop;
                for &l in limbs {
                    if let Some(bone) = av.rig.limb_bone(l) {
                        self.pose.locals[bone].scale = vec3(k, k, k);
                    }
                }
            }
            let turn = s.turn * s.turn * (3.0 - 2.0 * s.turn);
            let yaw = (std::f32::consts::FRAC_PI_2 - CAMERA_TURN) * (1.0 - turn) + DANCE_YAW * turn;
            let flip = if mirrored { -1.0 } else { 1.0 };
            let rotation = Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, vec3(flip, 1.0, 1.0))
                * Mat4::from_trs(Vec3::ZERO, Quat::from_axis_angle(Vec3::Y, yaw), vec3(1.0, 1.0, 1.0));
            s.to_body = rotation.invert();
            // Feet planted on the floor while standing and walking.
            let standing = grounded_neutral && captured_pose.is_some();
            match (&av.ragdoll, view.preview) {
                (Some(rig), false) => s.feet.apply(
                    av.hips,
                    [rig.leg(0), rig.leg(1)],
                    &av.character.skeleton,
                    &mut self.pose,
                    &mut self.globals[side],
                    av.character.transform,
                    Mat4::translation(root) * rotation,
                    standing,
                    dt,
                ),
                _ => s.feet.reset(),
            }
            s.out_locals.clone_from(&self.pose);
            // Physical layer: hits, inertia, gravity on a loose body. The
            // root's velocity comes from the simulation (or the knockout
            // flight), never from rendered positions.
            let root_velocity = match &s.ko {
                Some(ko) if clip == Clip::Knockout => vec3(ko.vx, 0.0, 0.0),
                _ => vec3(f.vx as f32, f.vy as f32, 0.0) * 0.06,
            };
            match (&av.ragdoll, view.preview) {
                (Some(rig), false) => s.ragdoll.apply(
                    rig,
                    &av.character.skeleton,
                    &mut self.pose,
                    &mut self.globals[side],
                    av.character.transform,
                    s.to_body,
                    root,
                    root_velocity,
                    drive(clip, frame),
                    dt,
                ),
                _ => s.ragdoll.reset(),
            }
            av.character
                .skeleton
                .skin_matrices(&self.pose, &mut self.globals[side], &mut self.skin[side]);
            self.placement[side] = Mat4::from_trs(root, Quat::IDENTITY, vec3(flip, 1.0, 1.0))
                * Mat4::from_trs(Vec3::ZERO, Quat::from_axis_angle(Vec3::Y, yaw), vec3(1.0, 1.0, 1.0))
                * av.character.transform;
        }
    }
}

/// How much the body is in control of itself: launched bodies trail their
/// limbs, a knockdown falls loose and takes control back for the get-up, a
/// knocked out body goes limp.
fn drive(clip: Clip, frame: f32) -> Drive {
    const FLIGHT: Drive = Drive { torso: 0.5, head: 0.3, arms: 0.18, legs: 0.25, inertia: 1.0 };
    const FALL: Drive = Drive { torso: 0.4, head: 0.25, arms: 0.12, legs: 0.15, inertia: 1.0 };
    const LIMP: Drive = Drive { torso: 0.25, head: 0.12, arms: 0.05, legs: 0.08, inertia: 1.0 };
    match clip {
        Clip::AirHit => FLIGHT,
        Clip::Down(_) => FALL.lerp(Drive::FULL, ((frame - DOWN_FALL) / 10.0).clamp(0.0, 1.0)),
        Clip::Knockout => LIMP,
        _ => Drive::FULL,
    }
}

/// The limb that lands `action` for this body: as authored, except the
/// captured heavy attack (a spinning back kick, rear foot) and the captured
/// uppercut (lead hand).
fn striker_of(av: &Avatar, action: u32) -> usize {
    let captured = |key: &str| av.captured.as_ref().is_some_and(|c| c.takes.contains_key(key));
    if action == 2 && captured("heavy") {
        3
    } else if action == 10 && captured("uppercut") {
        // The captured uppercut rises with the lead hand.
        0
    } else {
        anims::striker(action)
    }
}

/// Pitches `bone` about the character's side-to-side axis (positive leans
/// forward), on top of whatever the pose already does.
fn lean(character: &Character, pose: &mut Pose, globals: &mut Vec<Mat4>, bone: usize, angle: f32) {
    let Some(parent) = character.skeleton.bones[bone].parent else { return };
    character.skeleton.global_matrices(pose, globals);
    let parent_rotation = Quat::from_matrix(globals[parent]);
    let axis = Quat::from_matrix(character.transform).conjugate().rotate(Vec3::X);
    let turn = Quat::from_axis_angle(axis, angle);
    let local = parent_rotation.conjugate() * turn * parent_rotation;
    pose.locals[bone].rotation = (local * pose.locals[bone].rotation).normalize();
}

/// The attack a strike take stands in for.
fn take_action(key: &str) -> Option<u32> {
    Some(match key.split('_').next()? {
        "jab" => 1,
        "cross" => 11,
        "heavy" => 2,
        "uppercut" => 10,
        "throw" => 4,
        "kick" => 8,
        "sweep" => 9,
        "roundhouse" => 12,
        "special" => 14,
        "air" => 13,
        "smash" => 19,
        "low" => 16,
        "hook" => 17,
        "side" => 18,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view() -> View {
        View {
            dt: 1.0 / 60.0,
            time: 0.0,
            phase: 1,
            winner: -1,
            phase_time: 0.0,
            freeze: 0,
            victim: 1,
            reaction: [Reaction::Head; 2],
            bounds: [-3.0, 3.0],
            preview: false,
            variant: 0,
        }
    }
    fn bodies(f: &[Fighter; 2], y: f32) -> [Body; 2] {
        [
            Body { x: -1.0, y, vy: f[0].vy as f32, frame: f[0].frame as f32 },
            Body { x: 1.0, y: 0.0, vy: 0.0, frame: f[1].frame as f32 },
        ]
    }
    #[test]
    fn existing_glb_poses_every_state_with_finite_skin_matrices() {
        let bytes = std::fs::read("assets/character.glb").unwrap();
        let mut model = FighterModel::load(&bytes).unwrap();
        assert_eq!(model.avatars[0].character.skeleton.len(), 69);
        let mut game = arena_combat::Match::default();
        let mut v = view();
        for action in 0..=19 {
            game.fighters[0].action = action;
            for frame in 0..60 {
                game.fighters[0].frame = frame;
                v.time = frame as f32 / 60.0;
                model.update(&game.fighters, &bodies(&game.fighters, 0.0), &v);
            }
            assert_eq!(model.skin[0].len(), 69);
            assert!(model.skin[0].iter().all(|m| m.0.iter().all(|x| x.is_finite())));
        }
        game.fighters[0].action = 0;
        game.fighters[0].stun = 20;
        game.fighters[0].y = 500;
        game.fighters[0].vy = 30;
        model.update(&game.fighters, &bodies(&game.fighters, 0.5), &v);
        game.fighters[0].hp = 0;
        for _ in 0..360 {
            model.update(&game.fighters, &bodies(&game.fighters, 0.0), &v);
        }
        assert!(model.placement[0].0.iter().all(|x| x.is_finite()));
        // The knocked-out body comes to rest on the floor, inside the room.
        let root = model.root(0);
        assert!(root.y.abs() < 1e-3 && root.x >= -3.0 && root.x < -1.0, "{root:?}");
    }

    #[test]
    fn captured_takes_have_ordered_marks_and_pose_cleanly() {
        let Ok(pack) = std::fs::read("assets/fight.pack") else { return };
        let bytes = std::fs::read("assets/character.glb").unwrap();
        let mut model = FighterModel::load(&bytes).unwrap();
        model.set_captured(0, &pack).unwrap();
        let captured = model.avatars[0].captured.as_ref().unwrap();
        for (key, take) in &captured.takes {
            let m = take.marks;
            eprintln!("{key:<11} start {:.2} contact {:.2} end {:.2} / {:.2}", m.start, m.contact, m.end, take.duration);
            assert!(0.0 <= m.start && m.start <= m.contact && m.contact <= m.end && m.end <= take.duration + 1e-3, "{key}");
        }
        // Falls land a fraction of a second after they start.
        for key in ["swept", "ko"] {
            if let Some(take) = captured.takes.get(key) {
                let fall = take.marks.contact - take.marks.start;
                assert!((0.1..1.3).contains(&fall), "{key} falls for {fall}s");
            }
        }
        let mut game = arena_combat::Match::default();
        let v = view();
        for action in 0..=19 {
            game.fighters[1].action = action;
            game.fighters[1].down = if action == 15 { 40 } else { 0 };
            for frame in 0..72 {
                game.fighters[1].frame = frame;
                model.update(&game.fighters, &bodies(&game.fighters, 0.0), &v);
                assert!(model.skin[1].iter().all(|m| m.0.iter().all(|x| x.is_finite())), "action {action} frame {frame}");
            }
        }
    }

    #[test]
    fn the_match_winner_turns_to_the_camera_and_dances_one_dance_after_another() {
        let Ok(pack) = std::fs::read("assets/fight.pack") else { return };
        let bytes = std::fs::read("assets/character.glb").unwrap();
        let mut model = FighterModel::load(&bytes).unwrap();
        model.set_captured(0, &pack).unwrap();
        let game = arena_combat::Match::default();
        let mut v = View { phase: 3, winner: 0, ..view() };
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..(60 * 40) {
            v.phase_time = i as f32 / 60.0;
            v.time = v.phase_time;
            model.update(&game.fighters, &bodies(&game.fighters, 0.0), &v);
            if v.phase_time > DANCE_AFTER + 0.1 {
                assert_eq!(model.sides[0].clip, Clip::Dance);
                assert_eq!(model.sides[1].clip, Clip::Defeat);
                seen.extend(model.sides[0].take.clone());
            }
            assert!(model.skin[0].iter().all(|m| m.0.iter().all(|x| x.is_finite())));
        }
        // Several dances played; the body has turned (nearly) to the camera.
        assert!(seen.len() >= 3 && seen.iter().all(|k| k.starts_with("dance")), "{seen:?}");
        assert!(model.sides[0].turn > 0.95 && model.sides[1].turn < 0.05);
    }

    #[test]
    fn two_different_fighters_pose_every_state() {
        // Kachujin's rig has more bones than the engine keeps, so its clips
        // are retargeted onto a reduced skeleton.
        let (Ok(glb), Ok(pack)) = (
            std::fs::read("assets/fighters/kachujin.glb"),
            std::fs::read("assets/fighters/kachujin.pack"),
        ) else {
            return;
        };
        let bytes = std::fs::read("assets/character.glb").unwrap();
        let mut model = FighterModel::load(&bytes).unwrap();
        let other = model.add_avatar(&glb).unwrap();
        assert!(model.set_captured(other, &pack).unwrap() > 30);
        model.show([0, other]);
        let mut game = arena_combat::Match::default();
        let v = view();
        for action in [0, 1, 4, 8, 9, 10, 12, 13, 15, 19] {
            game.fighters[1].action = action;
            game.fighters[1].crouch = action == 0;
            game.fighters[1].down = if action == 15 { 40 } else { 0 };
            for frame in (0..60).step_by(3) {
                game.fighters[1].frame = frame;
                model.update(&game.fighters, &bodies(&game.fighters, 0.0), &v);
                let skin = &model.skin[1];
                assert_eq!(skin.len(), model.avatar(1).character.skeleton.len());
                assert!(skin.iter().all(|m| m.0.iter().all(|x| x.is_finite())), "action {action} frame {frame}");
            }
        }
    }

    #[test]
    fn feet_stay_on_the_floor_in_neutral_states() {
        let bytes = std::fs::read("assets/character.glb").unwrap();
        let mut model = FighterModel::load(&bytes).unwrap();
        let game = arena_combat::Match::default();
        let mut v = view();
        v.preview = true;
        model.update(&game.fighters, &bodies(&game.fighters, 0.0), &v);
        for side in 0..2 {
            for toe in [model.avatars[0].bones[2], model.avatars[0].bones[3]] {
                let y = model.bone_world(side, toe).y;
                assert!(y.abs() < 0.03, "side {side} toe at {y}");
            }
        }
    }
}
