//! Captured animation clips (Mixamo) packed for the fighter.
//!
//! The pack keeps only body bones (fingers are posed procedurally as fists),
//! resampled at a fixed rate with rotations quantised to i16, so thirty clips
//! weigh about half a megabyte. Bones are matched to the skeleton by name.
//!
//! Layout, little endian: `PFP1`, u16 bone count + names (u8 length + UTF-8),
//! u16 clip count, then per clip: name, f32 fps, u16 frames, and per frame the
//! hips translation (3 × f32, model units) followed by one i16 quaternion per
//! bone.
use crate::engine::{
    math3::*,
    skeleton::{Pose, Skeleton},
};

pub const MAGIC: &[u8; 4] = b"PFP1";

/// Bones worth storing: everything except fingers, eyes and end sites.
pub fn is_body_bone(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    !(["thumb", "index", "middle", "ring", "pinky", "eye", "_end", "sleeve"]
        .iter()
        .any(|k| n.contains(k)))
}

pub struct MocapClip {
    pub name: String,
    pub fps: f32,
    pub frames: usize,
    hips: Vec<Vec3>,
    rotations: Vec<Quat>,
}
impl MocapClip {
    pub fn duration(&self) -> f32 {
        (self.frames.max(1) - 1) as f32 / self.fps
    }
}

pub struct MocapLib {
    /// Skeleton bone for each stored channel (`usize::MAX` if missing).
    bones: Vec<usize>,
    hips: usize,
    pub clips: Vec<MocapClip>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.at + n;
        let slice = self.bytes.get(self.at..end).ok_or("fight pack is truncated")?;
        self.at = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn i16(&mut self) -> Result<i16, String> {
        let b = self.take(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }
    fn f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn string(&mut self) -> Result<String, String> {
        let n = self.u8()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|e| e.to_string())
    }
}

impl MocapLib {
    pub fn parse(bytes: &[u8], skeleton: &Skeleton) -> Result<MocapLib, String> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return Err("not a fight pack".into());
        }
        let bone_count = r.u16()? as usize;
        let mut bones = Vec::with_capacity(bone_count);
        for _ in 0..bone_count {
            let name = r.string()?;
            bones.push(skeleton.find_normalized(&name).unwrap_or(usize::MAX));
        }
        let hips = skeleton
            .find_like(super::character::bone::HIPS)
            .ok_or("skeleton has no hips")?;
        let clip_count = r.u16()? as usize;
        let mut clips = Vec::with_capacity(clip_count);
        for _ in 0..clip_count {
            let name = r.string()?;
            let fps = r.f32()?;
            let frames = r.u16()? as usize;
            let mut hip_track = Vec::with_capacity(frames);
            let mut rotations = Vec::with_capacity(frames * bone_count);
            for _ in 0..frames {
                hip_track.push(vec3(r.f32()?, r.f32()?, r.f32()?));
                for _ in 0..bone_count {
                    let q = [r.i16()?, r.i16()?, r.i16()?, r.i16()?];
                    let f = |v: i16| v as f32 / 32767.0;
                    rotations.push(
                        Quat {
                            x: f(q[0]),
                            y: f(q[1]),
                            z: f(q[2]),
                            w: f(q[3]),
                        }
                        .normalize(),
                    );
                }
            }
            clips.push(MocapClip {
                name,
                fps,
                frames,
                hips: hip_track,
                rotations,
            });
        }
        Ok(MocapLib { bones, hips, clips })
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name == name)
    }

    /// Writes the clip at `time` seconds into `out` (stored bones only).
    /// `root` shifts the hips; the caller decides what to do with travel.
    pub fn sample(&self, clip: usize, time: f32, root: Vec3, out: &mut Pose) {
        let c = &self.clips[clip];
        let f = (time * c.fps).clamp(0.0, (c.frames.max(1) - 1) as f32);
        let a = f.floor() as usize;
        let b = (a + 1).min(c.frames - 1);
        let u = f - a as f32;
        let n = self.bones.len();
        for (k, &bone) in self.bones.iter().enumerate() {
            if bone == usize::MAX || bone >= out.locals.len() {
                continue;
            }
            let qa = c.rotations[a * n + k];
            let qb = c.rotations[b * n + k];
            out.locals[bone].rotation = qa.slerp(qb, u).normalize();
        }
        out.locals[self.hips].translation = c.hips[a].lerp(c.hips[b], u) + root;
    }

    /// Hips translation of a clip frame (model units), for root handling.
    pub fn hips_at(&self, clip: usize, time: f32) -> Vec3 {
        let c = &self.clips[clip];
        let f = (time * c.fps).clamp(0.0, (c.frames.max(1) - 1) as f32);
        let a = f.floor() as usize;
        let b = (a + 1).min(c.frames - 1);
        c.hips[a].lerp(c.hips[b], f - a as f32)
    }
}

/// A clip to pack: key, retargeted animation and the window (seconds) kept.
pub struct PackClip {
    pub key: String,
    pub clip: crate::engine::skeleton::AnimationClip,
    pub from: f32,
    pub to: f32,
}

/// Builds a pack from clips already retargeted onto `skeleton`.
pub fn write_pack(skeleton: &Skeleton, clips: &[PackClip], fps: f32) -> Vec<u8> {
    let stored: Vec<usize> = (0..skeleton.len())
        .filter(|&i| is_body_bone(&skeleton.bones[i].name))
        .collect();
    let hips = skeleton
        .find_like(super::character::bone::HIPS)
        .unwrap_or(0);
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(stored.len() as u16).to_le_bytes());
    let push_str = |out: &mut Vec<u8>, s: &str| {
        let b = &s.as_bytes()[..s.len().min(255)];
        out.push(b.len() as u8);
        out.extend_from_slice(b);
    };
    for &b in &stored {
        push_str(&mut out, &skeleton.bones[b].name);
    }
    out.extend_from_slice(&(clips.len() as u16).to_le_bytes());
    let rest = skeleton.rest_pose();
    let mut pose = skeleton.rest_pose();
    for PackClip { key, clip, from, to } in clips {
        let frames = (((to - from) * fps).round() as usize + 1).max(2);
        push_str(&mut out, key);
        out.extend_from_slice(&fps.to_le_bytes());
        out.extend_from_slice(&(frames as u16).to_le_bytes());
        for f in 0..frames {
            clip.sample(from + f as f32 / fps, &rest, &mut pose);
            let t = pose.locals[hips].translation;
            for v in [t.x, t.y, t.z] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for &b in &stored {
                let q = pose.locals[b].rotation.normalize();
                // Keep w positive so neighbouring frames stay in one hemisphere.
                let s = if q.w < 0.0 { -1.0 } else { 1.0 };
                for v in [q.x, q.y, q.z, q.w] {
                    let i = (v * s * 32767.0).round().clamp(-32767.0, 32767.0) as i16;
                    out.extend_from_slice(&i.to_le_bytes());
                }
            }
        }
    }
    out
}

impl MocapLib {
    /// Character-space positions of `bones` for every stored frame.
    pub fn positions(&self, clip: usize, skeleton: &Skeleton, to_c: Mat4, bones: &[usize]) -> Vec<Vec<Vec3>> {
        let c = &self.clips[clip];
        let mut pose = skeleton.rest_pose();
        let mut globals = Vec::new();
        (0..c.frames)
            .map(|f| {
                self.sample(clip, f as f32 / c.fps, Vec3::ZERO, &mut pose);
                skeleton.global_matrices(&pose, &mut globals);
                bones
                    .iter()
                    .map(|&b| to_c.transform_point(globals[b].transform_point(Vec3::ZERO)))
                    .collect()
            })
            .collect()
    }
}

/// Which tracked limb lands a captured strike (1/2 hands, 3/4 feet; 0 is the
/// hips; 5 both hands together, 6 whichever foot leads) and whether the blow
/// rises (uppercut) rather than reaching forward.
pub fn strike_limb(key: &str) -> Option<(usize, bool)> {
    match key {
        "air_kick" => return Some((6, false)),
        "throw" => return Some((5, false)),
        _ => {}
    }
    match key.split('_').next().unwrap_or(key) {
        "jab" | "special" => Some((1, false)),
        "kick" => Some((3, false)),
        "cross" => Some((2, false)),
        // A rising lead-hand uppercut.
        "uppercut" => Some((1, true)),
        // The heavy attack is a spinning back kick with the rear foot; the
        // low kick snaps the rear foot at the shin.
        "roundhouse" | "sweep" | "heavy" | "low" => Some((4, false)),
        _ => None,
    }
}

/// Clips are sped up at most this much to fit the frame data; a longer
/// wind-up or recovery is trimmed instead.
pub const MAX_SPEEDUP: f32 = 2.5;

impl Marks {
    /// Trims the wind-up and recovery so neither plays faster than
    /// `MAX_SPEEDUP` over `startup` / `recovery` seconds of the move.
    pub fn fit(self, startup: f32, recovery: f32) -> Marks {
        Marks {
            start: self.start.max(self.contact - startup * MAX_SPEEDUP),
            contact: self.contact,
            end: self.end.min(self.contact + recovery * MAX_SPEEDUP),
        }
    }
}

/// Where a captured strike starts moving, lands and settles, in seconds.
#[derive(Clone, Copy, Debug)]
pub struct Marks {
    pub start: f32,
    pub contact: f32,
    pub end: f32,
}

/// Finds the strike in a clip from the striking limb's track (character
/// space) and the hips track: contact is the furthest reach forward (or up,
/// for rising blows); the strike starts shortly before the limb leaves its
/// guard and settles once it is back.
pub fn strike_marks(limb: &[Vec3], hips: &[Vec3], fps: f32, rising: bool) -> Marks {
    let n = limb.len().max(1);
    let reach = |i: usize| {
        if rising {
            limb[i].y - hips[i].y
        } else {
            limb[i].z - hips[i].z
        }
    };
    let contact = (0..n).max_by(|&a, &b| reach(a).total_cmp(&reach(b))).unwrap_or(0);
    let home = limb[0] - hips[0];
    let away = |i: usize| ((limb[i] - hips[i]) - home).length();
    let peak = away(contact).max(0.05);
    let mut start = contact;
    while start > 0 && away(start - 1) > peak * 0.12 {
        start -= 1;
    }
    start = start.saturating_sub(3);
    let mut end = contact;
    while end + 1 < n && away(end) > peak * 0.18 {
        end += 1;
    }
    end = (end + 4).min(n - 1);
    Marks {
        start: start as f32 / fps,
        contact: contact as f32 / fps,
        end: (end.max(contact + 1)) as f32 / fps,
    }
}
