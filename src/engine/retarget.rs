//! Перенос позы человека на скелет персонажа.
//!
//! Задача узкая: есть тринадцать точек с камеры и есть риг с семью десятками
//! костей. Полноценной обратной кинематики тут не нужно — нужно, чтобы каждая
//! кость-конечность смотрела туда же, куда смотрит соответствующий отрезок
//! тела человека.
//!
//! Отсюда способ: для каждой кости берём её текущее направление в мировых
//! координатах, целевое направление из позы и доворачиваем кость кратчайшим
//! поворотом между ними. Кости обрабатываются сверху вниз по иерархии, чтобы
//! ребёнок доворачивался уже от повёрнутого родителя.
//!
//! Чего этот способ не умеет: разворота кости вокруг собственной оси (в позе
//! из тринадцати точек его попросту нет — кисть представлена одной точкой) и
//! переноса самой фигуры по сцене. Для танца ни то, ни другое не критично, а
//! стоило бы это заметно дороже.

use super::math3::{Quat, Vec3};
use super::pose::{Joint, Pose};
use super::skeleton::{Pose as SkeletonPose, Skeleton};

/// Кости персонажа, соответствующие суставам позы.
///
/// `None` означает «в этом риге такой кости не нашлось» — тогда отрезок просто
/// не двигается.
pub type JointBones = [Option<usize>; super::pose::JOINT_COUNT];

/// Что именно переносится на риг.
#[derive(Clone, Copy)]
pub struct Rig {
    pub joints: JointBones,
    /// Кость корпуса: доворачивается по наклону от таза к плечам. Без неё
    /// аватар стоит колом и двигает только конечностями.
    pub spine: Option<usize>,
    /// Шея: доворачивается по направлению от плеч к голове.
    pub neck: Option<usize>,
}

impl Default for Rig {
    fn default() -> Rig {
        Rig {
            joints: [None; super::pose::JOINT_COUNT],
            spine: None,
            neck: None,
        }
    }
}

/// Отрезок, который переносится: кость, её конец и суставы позы.
struct Segment {
    /// Кость, которую доворачиваем.
    bone: Joint,
    /// Сустав, задающий её конец: направление кости — от `bone` к `tip`.
    tip: Joint,
}

/// Порядок важен: сначала то, что ближе к корню. Иначе поворот плеча сбил бы
/// уже выставленное предплечье.
const SEGMENTS: [Segment; 8] = [
    Segment { bone: Joint::ShoulderL, tip: Joint::ElbowL },
    Segment { bone: Joint::ShoulderR, tip: Joint::ElbowR },
    Segment { bone: Joint::ElbowL, tip: Joint::WristL },
    Segment { bone: Joint::ElbowR, tip: Joint::WristR },
    Segment { bone: Joint::HipL, tip: Joint::KneeL },
    Segment { bone: Joint::HipR, tip: Joint::KneeR },
    Segment { bone: Joint::KneeL, tip: Joint::AnkleL },
    Segment { bone: Joint::KneeR, tip: Joint::AnkleR },
];

/// Разворачивает скелет по позе человека.
///
/// `out` приходит уже заполненным — обычно позой покоя или кадром танца.
/// Кости, для которых в позе нет данных, остаются как были: так аватар не
/// разваливается, когда распознавание теряет ногу.
///
/// `mirrored` — брать ли сустав противоположной стороны. Человек напротив
/// экрана видит себя как в зеркале: подняв правую руку, он ждёт, что аватар
/// поднимет ту, что окажется на той же стороне картинки, а это его левая.
pub fn drive(skeleton: &Skeleton, rig: &Rig, pose: &Pose, mirrored: bool, out: &mut SkeletonPose) {
    let bones = &rig.joints;
    let mut globals = Vec::with_capacity(skeleton.len());
    let mut rotations = Vec::with_capacity(skeleton.len());

    let shoulders = (
        bones[Joint::ShoulderL.index()],
        bones[Joint::ShoulderR.index()],
    );

    // Корпус и шея — первыми: они тянут за собой всё остальное, и делать их
    // после конечностей значило бы сбивать уже выставленные руки.
    if let (Some(spine), (Some(left), Some(right))) = (rig.spine, shoulders) {
        let target = pose
            .direction(Joint::HipL, Joint::ShoulderL)
            .zip(pose.direction(Joint::HipR, Joint::ShoulderR))
            .map(|(a, b)| (a + b).normalize());
        if let Some(target) = target {
            globals_and_rotations(skeleton, out, &mut globals, &mut rotations);
            let mid = (globals[left].transform_point(Vec3::ZERO)
                + globals[right].transform_point(Vec3::ZERO))
                * 0.5;
            let current = mid - globals[spine].transform_point(Vec3::ZERO);
            turn(skeleton, spine, current, target, &rotations, out);
        }
    }

    if let (Some(neck), Some(head)) = (rig.neck, bones[Joint::Head.index()]) {
        // Направление усредняется по обоим плечам: одно плечо распознавание
        // теряет заметно чаще, чем оба сразу.
        let target = pose
            .direction(Joint::ShoulderL, Joint::Head)
            .zip(pose.direction(Joint::ShoulderR, Joint::Head))
            .map(|(a, b)| (a + b).normalize());
        if let Some(target) = target {
            globals_and_rotations(skeleton, out, &mut globals, &mut rotations);
            let current = globals[head].transform_point(Vec3::ZERO)
                - globals[neck].transform_point(Vec3::ZERO);
            turn(skeleton, neck, current, target, &rotations, out);
        }
    }

    for segment in &SEGMENTS {
        let (Some(bone), Some(tip)) = (
            bones[segment.bone.index()],
            bones[segment.tip.index()],
        ) else {
            continue;
        };

        let (from, to) = if mirrored {
            (segment.bone.opposite(), segment.tip.opposite())
        } else {
            (segment.bone, segment.tip)
        };
        let Some(target) = pose.direction(from, to) else {
            continue;
        };

        // Пересчёт на каждом отрезке: предыдущий доворот уже сдвинул детей, и
        // считать от устаревших матриц значит промахиваться. Костей меньше
        // сотни, восемь проходов по ним ничего не стоят.
        globals_and_rotations(skeleton, out, &mut globals, &mut rotations);

        let current = globals[tip].transform_point(Vec3::ZERO)
            - globals[bone].transform_point(Vec3::ZERO);
        turn(skeleton, bone, current, target, &rotations, out);
    }
}

/// Доворачивает одну кость так, чтобы `current` совпало с `target`.
fn turn(
    skeleton: &Skeleton,
    bone: usize,
    current: Vec3,
    target: Vec3,
    rotations: &[Quat],
    out: &mut SkeletonPose,
) {
    if current.length() < 1e-5 {
        return;
    }
    let delta = Quat::from_rotation_arc(current.normalize(), target);
    let world = (delta * rotations[bone]).normalize();

    // Из мирового поворота в родительский: хранится всегда локальный.
    out.locals[bone].rotation = match skeleton.bones[bone].parent {
        Some(parent) => (rotations[parent].conjugate() * world).normalize(),
        None => world,
    };
}

/// Глобальные матрицы и отдельно глобальные повороты.
///
/// Повороты нужны отдельно от матриц: вытащить их обратно из матрицы можно,
/// только если в ней нет неравномерного масштаба, а гарантий этого у чужих
/// моделей нет. Составить же их напрямую из локальных кватернионов и дёшево,
/// и точно.
fn globals_and_rotations(
    skeleton: &Skeleton,
    pose: &SkeletonPose,
    globals: &mut Vec<super::math3::Mat4>,
    rotations: &mut Vec<Quat>,
) {
    skeleton.global_matrices(pose, globals);

    rotations.clear();
    for (i, bone) in skeleton.bones.iter().enumerate() {
        let local = pose.locals[i].rotation;
        rotations.push(match bone.parent {
            Some(parent) => (rotations[parent] * local).normalize(),
            None => local.normalize(),
        });
    }
}

/// Снимает позу с уже посчитанного скелета: положения тех же тринадцати
/// суставов в координатах модели.
///
/// Нужна, чтобы сравнивать аватар с тренером — обе позы оказываются в одном
/// пространстве, и сравнение сводится к тем же направлениям отрезков.
pub fn sample(globals: &[super::math3::Mat4], bones: &JointBones) -> Pose {
    let mut pose = Pose::default();
    for joint in Joint::ALL {
        match bones[joint.index()] {
            Some(bone) if bone < globals.len() => {
                pose.set(joint, globals[bone].transform_point(Vec3::ZERO), 1.0);
            }
            _ => {}
        }
    }
    pose
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::math3::{vec3, Mat4, Transform};
    use crate::engine::skeleton::Skeleton;

    /// Крестовина: таз, от него две «руки» вбок и две «ноги» вниз, по два
    /// звена каждая. Имена не важны — кости адресуются индексами.
    fn rig() -> (Skeleton, Rig) {
        // (имя, родитель, смещение от родителя)
        let parts: Vec<(String, Option<usize>, Vec3)> = vec![
            ("hips".into(), None, vec3(0.0, 0.0, 0.0)),
            ("shoulder.L".into(), Some(0), vec3(-0.2, 0.6, 0.0)),
            ("elbow.L".into(), Some(1), vec3(-0.3, 0.0, 0.0)),
            ("wrist.L".into(), Some(2), vec3(-0.3, 0.0, 0.0)),
            ("shoulder.R".into(), Some(0), vec3(0.2, 0.6, 0.0)),
            ("elbow.R".into(), Some(4), vec3(0.3, 0.0, 0.0)),
            ("wrist.R".into(), Some(5), vec3(0.3, 0.0, 0.0)),
        ];

        let bones = parts
            .iter()
            .map(|(name, parent, offset)| {
                (
                    name.clone(),
                    *parent,
                    Transform {
                        translation: *offset,
                        ..Transform::IDENTITY
                    },
                )
            })
            .collect();
        let skeleton = Skeleton::from_parts(bones, vec![Mat4::IDENTITY; parts.len()]);

        let mut map: JointBones = [None; crate::engine::pose::JOINT_COUNT];
        map[Joint::ShoulderL.index()] = Some(1);
        map[Joint::ElbowL.index()] = Some(2);
        map[Joint::WristL.index()] = Some(3);
        map[Joint::ShoulderR.index()] = Some(4);
        map[Joint::ElbowR.index()] = Some(5);
        map[Joint::WristR.index()] = Some(6);
        (skeleton, Rig { joints: map, ..Rig::default() })
    }

    /// Поза, где обе руки смотрят в заданную сторону от плеча.
    fn arms(direction: Vec3) -> Pose {
        let mut pose = Pose::default();
        pose.set(Joint::ShoulderL, vec3(-0.2, 0.6, 0.0), 1.0);
        pose.set(Joint::ShoulderR, vec3(0.2, 0.6, 0.0), 1.0);
        pose.set(Joint::ElbowL, vec3(-0.2, 0.6, 0.0) + direction * 0.3, 1.0);
        pose.set(Joint::ElbowR, vec3(0.2, 0.6, 0.0) + direction * 0.3, 1.0);
        pose.set(Joint::WristL, vec3(-0.2, 0.6, 0.0) + direction * 0.6, 1.0);
        pose.set(Joint::WristR, vec3(0.2, 0.6, 0.0) + direction * 0.6, 1.0);
        pose
    }

    fn joint_positions(skeleton: &Skeleton, pose: &SkeletonPose, map: &Rig) -> Pose {
        let mut globals = Vec::new();
        skeleton.global_matrices(pose, &mut globals);
        sample(&globals, &map.joints)
    }

    /// Главное свойство: после переноса кости смотрят туда же, куда отрезки
    /// позы. Проверяется на направлении, которого в позе покоя нет вовсе.
    #[test]
    fn bones_end_up_pointing_where_the_pose_points() {
        let (skeleton, map) = rig();
        let target = vec3(0.0, 1.0, 0.0);

        let mut out = skeleton.rest_pose();
        drive(&skeleton, &map, &arms(target), false, &mut out);

        let result = joint_positions(&skeleton, &out, &map);
        for (from, to) in [
            (Joint::ShoulderL, Joint::ElbowL),
            (Joint::ElbowL, Joint::WristL),
            (Joint::ShoulderR, Joint::ElbowR),
            (Joint::ElbowR, Joint::WristR),
        ] {
            let dir = result.direction(from, to).expect("отрезок не построен");
            assert!(
                dir.dot(target) > 0.999,
                "{from:?}->{to:?} смотрит в {dir:?}, а не в {target:?}"
            );
        }
    }

    /// Глубина переносится наравне с остальными осями: ради этого поза и
    /// трёхмерная.
    #[test]
    fn depth_is_carried_over() {
        let (skeleton, map) = rig();
        let target = vec3(0.0, 0.0, 1.0);

        let mut out = skeleton.rest_pose();
        drive(&skeleton, &map, &arms(target), false, &mut out);

        let result = joint_positions(&skeleton, &out, &map);
        let dir = result.direction(Joint::ShoulderL, Joint::ElbowL).unwrap();
        assert!(dir.z > 0.99, "рука не ушла в глубину: {dir:?}");
    }

    /// Зеркалирование меняет стороны местами: правая рука человека должна
    /// поднимать левую руку аватара.
    #[test]
    fn mirroring_swaps_sides() {
        let (skeleton, map) = rig();

        // Правая рука вверх, левая — в сторону.
        let mut pose = arms(vec3(-1.0, 0.0, 0.0));
        pose.set(Joint::ElbowR, vec3(0.2, 0.9, 0.0), 1.0);
        pose.set(Joint::WristR, vec3(0.2, 1.2, 0.0), 1.0);

        let mut out = skeleton.rest_pose();
        drive(&skeleton, &map, &pose, true, &mut out);

        let result = joint_positions(&skeleton, &out, &map);
        let left = result.direction(Joint::ShoulderL, Joint::ElbowL).unwrap();
        assert!(left.y > 0.99, "поднялась не та рука: {left:?}");
    }

    /// Потерянная точка не должна ломать остальное: аватар просто оставляет
    /// эту кость как есть.
    #[test]
    fn missing_points_leave_bones_alone() {
        let (skeleton, map) = rig();
        let mut pose = arms(vec3(0.0, 1.0, 0.0));
        pose.set(Joint::WristL, vec3(0.0, 0.0, 0.0), 0.0);

        let mut out = skeleton.rest_pose();
        drive(&skeleton, &map, &pose, false, &mut out);

        let result = joint_positions(&skeleton, &out, &map);
        // Плечо развернулось...
        let upper = result.direction(Joint::ShoulderL, Joint::ElbowL).unwrap();
        assert!(upper.y > 0.999, "{upper:?}");
        // ...а предплечье осталось продолжением плеча, а не улетело в ноль.
        let fore = result.direction(Joint::ElbowL, Joint::WristL).unwrap();
        assert!(fore.y > 0.999, "{fore:?}");
    }
}
