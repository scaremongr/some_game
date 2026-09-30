//! Персонаж-заглушка: скелет, скиннинговый меш и процедурные клипы.
//!
//! Настоящей модели ещё нет, поэтому фигура собирается из капсул вокруг
//! костей. Задача этого модуля — доказать, что работает вся цепочка
//! «клип -> поза -> скиннинг -> свет», и дать чем анимировать до появления
//! .glb. Когда модель придёт, скелет и меш заменит загрузчик, а клипы
//! останутся в том же формате.

use crate::engine::math::Color;
use crate::engine::math3::*;
use crate::engine::mesh3::MeshData;
use crate::engine::skeleton::{AnimationClip, Skeleton, Track};

/// Синонимы имён костей. Загруженная модель почти наверняка называет их
/// иначе, чем наш процедурный скелет: Mixamo ставит префикс `mixamorig:`,
/// VRM — `J_Bip_C_`. Поиск по этим спискам покрывает оба случая.
pub mod bone {
    pub const HIPS: &[&str] = &["hips", "mixamorig:Hips", "J_Bip_C_Hips", "pelvis"];
    pub const SPINE: &[&str] = &["spine", "mixamorig:Spine", "J_Bip_C_Spine"];
    pub const CHEST: &[&str] = &["chest", "mixamorig:Spine2", "J_Bip_C_Chest", "upperchest"];
    pub const HEAD: &[&str] = &["head", "mixamorig:Head", "J_Bip_C_Head"];

    // Точки, по которым сравнивается поза с камерой. Списки покрывают
    // Mixamo (`LeftArm`), блендеровские риги (`upper_arm.L`) и VRM.
    pub const SHOULDER_L: &[&str] = &["mixamorig:LeftArm", "upper_arm.L", "J_Bip_L_UpperArm"];
    pub const SHOULDER_R: &[&str] = &["mixamorig:RightArm", "upper_arm.R", "J_Bip_R_UpperArm"];
    pub const ELBOW_L: &[&str] = &["mixamorig:LeftForeArm", "forearm.L", "J_Bip_L_LowerArm"];
    pub const ELBOW_R: &[&str] = &["mixamorig:RightForeArm", "forearm.R", "J_Bip_R_LowerArm"];
    pub const WRIST_L: &[&str] = &["mixamorig:LeftHand", "hand.L", "J_Bip_L_Hand"];
    pub const WRIST_R: &[&str] = &["mixamorig:RightHand", "hand.R", "J_Bip_R_Hand"];
    pub const HIP_L: &[&str] = &["mixamorig:LeftUpLeg", "thigh.L", "J_Bip_L_UpperLeg"];
    pub const HIP_R: &[&str] = &["mixamorig:RightUpLeg", "thigh.R", "J_Bip_R_UpperLeg"];
    pub const KNEE_L: &[&str] = &["mixamorig:LeftLeg", "shin.L", "J_Bip_L_LowerLeg"];
    pub const KNEE_R: &[&str] = &["mixamorig:RightLeg", "shin.R", "J_Bip_R_LowerLeg"];
    pub const ANKLE_L: &[&str] = &["mixamorig:LeftFoot", "foot.L", "J_Bip_L_Foot"];
    pub const ANKLE_R: &[&str] = &["mixamorig:RightFoot", "foot.R", "J_Bip_R_Foot"];
}

struct BoneDef {
    name: &'static str,
    parent: Option<&'static str>,
    offset: Vec3,
}

/// Скелет задан в A-позе смещениями от родителя; все бинд-повороты единичные.
/// Так направление сегмента задаётся самим смещением ребёнка, и меш строится
/// прямо по глобальным позициям суставов.
const SKELETON: &[BoneDef] = &[
    BoneDef { name: "hips", parent: None, offset: vec3(0.0, 0.95, 0.0) },
    BoneDef { name: "spine", parent: Some("hips"), offset: vec3(0.0, 0.12, 0.0) },
    BoneDef { name: "chest", parent: Some("spine"), offset: vec3(0.0, 0.17, 0.0) },
    BoneDef { name: "neck", parent: Some("chest"), offset: vec3(0.0, 0.19, 0.0) },
    BoneDef { name: "head", parent: Some("neck"), offset: vec3(0.0, 0.10, 0.0) },

    BoneDef { name: "clav.l", parent: Some("chest"), offset: vec3(0.07, 0.10, 0.0) },
    BoneDef { name: "upperarm.l", parent: Some("clav.l"), offset: vec3(0.10, -0.06, 0.0) },
    BoneDef { name: "forearm.l", parent: Some("upperarm.l"), offset: vec3(0.14, -0.24, 0.0) },
    BoneDef { name: "hand.l", parent: Some("forearm.l"), offset: vec3(0.08, -0.22, 0.0) },

    BoneDef { name: "clav.r", parent: Some("chest"), offset: vec3(-0.07, 0.10, 0.0) },
    BoneDef { name: "upperarm.r", parent: Some("clav.r"), offset: vec3(-0.10, -0.06, 0.0) },
    BoneDef { name: "forearm.r", parent: Some("upperarm.r"), offset: vec3(-0.14, -0.24, 0.0) },
    BoneDef { name: "hand.r", parent: Some("forearm.r"), offset: vec3(-0.08, -0.22, 0.0) },

    BoneDef { name: "thigh.l", parent: Some("hips"), offset: vec3(0.09, -0.05, 0.0) },
    BoneDef { name: "shin.l", parent: Some("thigh.l"), offset: vec3(0.01, -0.45, 0.0) },
    BoneDef { name: "foot.l", parent: Some("shin.l"), offset: vec3(0.0, -0.43, 0.0) },

    BoneDef { name: "thigh.r", parent: Some("hips"), offset: vec3(-0.09, -0.05, 0.0) },
    BoneDef { name: "shin.r", parent: Some("thigh.r"), offset: vec3(-0.01, -0.45, 0.0) },
    BoneDef { name: "foot.r", parent: Some("shin.r"), offset: vec3(0.0, -0.43, 0.0) },
];

pub fn build_skeleton() -> Skeleton {
    let mut names: Vec<&str> = Vec::new();
    let mut bones = Vec::with_capacity(SKELETON.len());

    for def in SKELETON {
        let parent = def.parent.map(|p| {
            names
                .iter()
                .position(|n| *n == p)
                .expect("родитель должен быть объявлен раньше ребёнка")
        });
        names.push(def.name);
        bones.push((
            def.name.to_string(),
            parent,
            Transform::from_translation(def.offset),
        ));
    }

    Skeleton::new(bones)
}

/// Цвета меша задаются в sRGB, а шейдер работает в линейном пространстве,
/// поэтому здесь же переводим.
fn linear(hex: u32) -> Vec3 {
    let c = Color::hex(hex).to_linear();
    vec3(c.r, c.g, c.b)
}

pub fn build_mesh(skeleton: &Skeleton) -> MeshData {
    let rest = skeleton.rest_pose();
    let mut globals = Vec::new();
    skeleton.global_matrices(&rest, &mut globals);

    let joint = |name: &str| skeleton.find(name).expect("нет такой кости");
    let pos = |name: &str| globals[joint(name)].transform_point(Vec3::ZERO);

    let skin = linear(0xE8B693);
    let outfit = linear(0x2E4B8F);
    let hair = linear(0x3A2A33);

    let mut m = MeshData::default();

    // Торс. Радиус в начале сегмента всегда чуть меньше, чем в конце
    // предыдущего: при точном совпадении поверхности оказываются
    // копланарными и дают z-fighting прямо на груди.
    m.add_capsule(pos("hips"), pos("spine"), 0.152, 0.114, 16, joint("hips"), joint("spine"), outfit);
    m.add_capsule(pos("spine"), pos("chest"), 0.111, 0.148, 16, joint("spine"), joint("chest"), outfit);
    m.add_capsule(pos("chest"), pos("neck"), 0.144, 0.052, 16, joint("chest"), joint("neck"), skin);
    m.add_capsule(pos("neck"), pos("head"), 0.049, 0.047, 12, joint("neck"), joint("head"), skin);

    // Голова: череп телесный, волосы — пересекающая его сфера, сдвинутая
    // вверх и назад. Пересечение, а не совпадение, поэтому шва не будет.
    m.add_sphere(pos("head") + vec3(0.0, 0.075, 0.0), vec3(0.098, 0.115, 0.10), 20, 14, joint("head"), skin);
    m.add_sphere(pos("head") + vec3(0.0, 0.098, -0.022), vec3(0.101, 0.101, 0.104), 20, 14, joint("head"), hair);

    for side in ["l", "r"] {
        let n = |part: &str| format!("{part}.{side}");

        m.add_capsule(
            pos(&n("upperarm")), pos(&n("forearm")),
            0.048, 0.036, 12,
            joint(&n("upperarm")), joint(&n("forearm")), skin,
        );
        m.add_capsule(
            pos(&n("forearm")), pos(&n("hand")),
            0.034, 0.027, 12,
            joint(&n("forearm")), joint(&n("hand")), skin,
        );
        m.add_sphere(
            pos(&n("hand")) + vec3(0.0, -0.04, 0.0),
            vec3(0.032, 0.052, 0.020), 12, 8,
            joint(&n("hand")), skin,
        );

        m.add_capsule(
            pos(&n("thigh")), pos(&n("shin")),
            0.088, 0.058, 14,
            joint(&n("thigh")), joint(&n("shin")), skin,
        );
        m.add_capsule(
            pos(&n("shin")), pos(&n("foot")),
            0.056, 0.035, 14,
            joint(&n("shin")), joint(&n("foot")), skin,
        );
        // Стопа уходит вперёд по +Z, поэтому это отдельная капсула,
        // а не продолжение голени.
        m.add_capsule(
            pos(&n("foot")),
            pos(&n("foot")) + vec3(0.0, -0.015, 0.15),
            0.040, 0.032, 10,
            joint(&n("foot")), joint(&n("foot")), outfit,
        );
    }

    m.compute_normals();
    m
}

/// Сборщик процедурных клипов: каждая дорожка задаётся функцией от фазы
/// [0, 1) и позы покоя, а ключи снимаются равномерной выборкой.
struct ClipBuilder<'a> {
    skeleton: &'a Skeleton,
    duration: f32,
    samples: usize,
    tracks: Vec<Track>,
}

impl<'a> ClipBuilder<'a> {
    fn new(skeleton: &'a Skeleton, duration: f32, samples: usize) -> ClipBuilder<'a> {
        ClipBuilder { skeleton, duration, samples, tracks: Vec::new() }
    }

    /// `f(phase, rest_local) -> local`. Ключи снимаются без дублирования
    /// последней точки: сэмплер сам смыкает конец цикла с началом.
    fn track<F>(&mut self, bone_keys: &[&str], f: F) -> &mut Self
    where
        F: Fn(f32, &Transform) -> Transform,
    {
        // Отсутствующую кость просто пропускаем: клип должен уметь лечь на
        // чужой скелет, где части костей может не быть.
        let bone = match self.skeleton.find_like(bone_keys) {
            Some(b) => b,
            None => return self,
        };
        let rest = self.skeleton.bones[bone].bind_local;

        let mut track = Track { bone, ..Default::default() };
        for i in 0..self.samples {
            let phase = i as f32 / self.samples as f32;
            let time = phase * self.duration;
            let local = f(phase, &rest);
            track.translations.push((time, local.translation));
            track.rotations.push((time, local.rotation));
        }
        self.tracks.push(track);
        self
    }

    fn build(self, name: &str) -> AnimationClip {
        AnimationClip {
            name: name.to_string(),
            duration: self.duration,
            looping: true,
            tracks: self.tracks,
        }
    }
}

const TAU: f32 = std::f32::consts::TAU;

/// Движения процедурного персонажа-заглушки. Длительности заданы в долях
/// такта; в секунды их переводит сцена, когда знает темп.
pub fn build_clips(skeleton: &Skeleton, seconds_per_beat: f32) -> Vec<AnimationClip> {
    vec![
        idle_clip(skeleton, seconds_per_beat * 2.0),
        sway_clip(skeleton, seconds_per_beat * 2.0),
        arms_up_clip(skeleton, seconds_per_beat * 2.0),
    ]
}

/// Спокойное дыхание: почти незаметное покачивание, чтобы фигура не стояла
/// мёртвой между движениями.
fn idle_clip(skeleton: &Skeleton, duration: f32) -> AnimationClip {
    let mut b = ClipBuilder::new(skeleton, duration, 16);

    b.track(bone::HIPS, |p, rest| Transform {
        translation: rest.translation + vec3(0.0, (p * TAU).sin() * 0.008, 0.0),
        rotation: Quat::from_euler((p * TAU).sin() * 0.03, 0.0, 0.0),
        ..*rest
    });
    b.track(bone::CHEST, |p, rest| Transform {
        rotation: Quat::from_euler(0.0, (p * TAU + 1.0).sin() * 0.02, 0.0),
        ..*rest
    });
    b.track(bone::HEAD, |p, rest| Transform {
        rotation: Quat::from_euler((p * TAU + 2.0).sin() * 0.04, 0.0, 0.0),
        ..*rest
    });
    for side in ["l", "r"] {
        let sign = if side == "l" { 1.0 } else { -1.0 };
        b.track(&[&format!("upperarm.{side}")], move |p, rest| Transform {
            rotation: Quat::from_euler(0.0, 0.0, sign * (p * TAU).sin() * 0.05),
            ..*rest
        });
    }

    b.build("IDLE")
}

/// Покачивание бёдрами с противоходом корпуса — базовое танцевальное движение.
fn sway_clip(skeleton: &Skeleton, duration: f32) -> AnimationClip {
    let mut b = ClipBuilder::new(skeleton, duration, 24);

    b.track(bone::HIPS, |p, rest| {
        let s = (p * TAU).sin();
        Transform {
            translation: rest.translation + vec3(s * 0.05, -(p * TAU * 2.0).cos() * 0.02, 0.0),
            rotation: Quat::from_euler(s * 0.20, 0.0, -s * 0.12),
            ..*rest
        }
    });
    b.track(bone::SPINE, |p, rest| Transform {
        rotation: Quat::from_euler(-(p * TAU).sin() * 0.10, 0.0, (p * TAU).sin() * 0.08),
        ..*rest
    });
    b.track(bone::CHEST, |p, rest| Transform {
        rotation: Quat::from_euler(-(p * TAU).sin() * 0.12, 0.0, (p * TAU).sin() * 0.06),
        ..*rest
    });
    b.track(bone::HEAD, |p, rest| Transform {
        rotation: Quat::from_euler((p * TAU).sin() * 0.15, (p * TAU * 2.0).sin() * 0.05, 0.0),
        ..*rest
    });

    for side in ["l", "r"] {
        let sign = if side == "l" { 1.0 } else { -1.0 };
        b.track(&[&format!("upperarm.{side}")], move |p, rest| {
            let s = (p * TAU).sin();
            Transform {
                rotation: Quat::from_euler(0.0, s * 0.20 * sign, sign * (0.10 + s * 0.16)),
                ..*rest
            }
        });
        b.track(&[&format!("forearm.{side}")], move |p, rest| {
            let s = (p * TAU + 0.6).sin();
            Transform {
                rotation: Quat::from_euler(0.0, 0.0, sign * (0.14 + s * 0.18)),
                ..*rest
            }
        });
        b.track(&[&format!("thigh.{side}")], move |p, rest| {
            let s = (p * TAU).sin();
            Transform {
                rotation: Quat::from_euler(0.0, 0.0, -s * 0.10 * sign),
                ..*rest
            }
        });
    }

    b.build("SWAY")
}

/// Руки вверх с раскрытием корпуса — второе движение, заметно отличающееся
/// от первого силуэтом: на нём хорошо видно кроссфейд.
fn arms_up_clip(skeleton: &Skeleton, duration: f32) -> AnimationClip {
    let mut b = ClipBuilder::new(skeleton, duration, 24);

    b.track(bone::HIPS, |p, rest| {
        let s = (p * TAU).sin();
        Transform {
            translation: rest.translation + vec3(s * 0.02, (p * TAU * 2.0).sin() * 0.03, 0.0),
            rotation: Quat::from_euler(-s * 0.10, 0.0, 0.0),
            ..*rest
        }
    });
    b.track(bone::SPINE, |p, rest| Transform {
        rotation: Quat::from_euler(0.0, -0.10 - (p * TAU).cos() * 0.05, 0.0),
        ..*rest
    });
    b.track(bone::HEAD, |p, rest| Transform {
        rotation: Quat::from_euler(0.0, -0.12, (p * TAU).sin() * 0.10),
        ..*rest
    });

    for side in ["l", "r"] {
        let sign = if side == "l" { 1.0 } else { -1.0 };
        b.track(&[&format!("upperarm.{side}")], move |p, rest| {
            let s = (p * TAU).sin();
            // Ось Z разводит руку в сторону, поэтому большой угол здесь
            // поднимает её над головой.
            Transform {
                rotation: Quat::from_euler(0.0, 0.0, sign * (2.30 + s * 0.25)),
                ..*rest
            }
        });
        b.track(&[&format!("forearm.{side}")], move |p, rest| {
            let s = (p * TAU + 1.2).sin();
            Transform {
                rotation: Quat::from_euler(0.0, s * 0.30, sign * 0.20),
                ..*rest
            }
        });
        b.track(&[&format!("thigh.{side}")], move |p, rest| {
            let s = (p * TAU).sin();
            Transform {
                rotation: Quat::from_euler(0.0, 0.0, -s * 0.06 * sign),
                ..*rest
            }
        });
    }

    b.build("ARMS UP")
}
