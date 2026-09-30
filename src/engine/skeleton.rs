//! Скелет, анимационные клипы и смешивание поз.
//!
//! Формат клипа повторяет glTF: независимые дорожки ключей на перенос,
//! поворот и масштаб, линейная интерполяция (для поворотов — slerp).
//!
//! Смешивание — то, ради чего вообще выбрана скелетная анимация:
//! * `blend` даёт кроссфейд между двумя движениями;
//! * `add_additive` кладёт покачивание в такт поверх текущего движения,
//!   не заменяя его.

use super::math3::*;

/// Потолок скелета. Кость занимает 3 vec4 униформ, плюс ~14 vec4 уходит на
/// матрицы и свет: 72 кости — это 230 vec4, что укладывается в 256, которые
/// реально отдаёт любой GPU новее ~2014 года.
///
/// Формально WebGL 1 гарантирует лишь 128 vec4, поэтому на совсем древнем
/// железе шейдер не слинкуется. Путь наверх, если он понадобится, — WebGL 2
/// и текстура костей вместо униформ: тогда потолок исчезает совсем.
///
/// 72 хватает на полный гуманоидный риг с пальцами: Mixamo даёт ~65 костей,
/// VRM — около 55.
pub const MAX_BONES: usize = 72;

pub struct Bone {
    pub name: String,
    /// Индекс родителя; None только у корня. Родитель всегда идёт раньше
    /// ребёнка, поэтому глобальные матрицы считаются одним проходом.
    pub parent: Option<usize>,
    /// Поза покоя в системе родителя.
    pub bind_local: Transform,
    /// Переводит из модельного пространства в пространство кости.
    pub inverse_bind: Mat4,
}

pub struct Skeleton {
    pub bones: Vec<Bone>,
    /// Поворот узлов НАД корневой костью: из пространства модели в
    /// пространство сцены.
    ///
    /// Экспортёры прячут там конверсию осей — у одного файла «вверх» это Y,
    /// у другого Z. Сами кости об этом не знают, поэтому при переносе
    /// анимации с чужого рига разворот надо учитывать явно: без него позы
    /// покоя двух T-поз выглядят повёрнутыми друг относительно друга на
    /// девяносто градусов, и ноги начинают жить своей жизнью.
    pub root_rotation: Quat,
}

impl Skeleton {
    /// Собирает скелет из готовых частей. Обратные бинд-матрицы приходят
    /// снаружи — именно так их отдаёт glTF, и пересчитывать их из позы покоя
    /// нельзя: в файле они могут не совпадать с иерархией узлов.
    pub fn from_parts(
        bones: Vec<(String, Option<usize>, Transform)>,
        inverse_binds: Vec<Mat4>,
    ) -> Skeleton {
        debug_assert_eq!(bones.len(), inverse_binds.len());
        Skeleton {
            root_rotation: Quat::IDENTITY,
            bones: bones
                .into_iter()
                .zip(inverse_binds)
                .map(|((name, parent, bind_local), inverse_bind)| Bone {
                    name,
                    parent,
                    bind_local,
                    inverse_bind,
                })
                .collect(),
        }
    }

    /// Собирает скелет из списка (имя, родитель, локальный бинд) и считает
    /// обратные бинд-матрицы сам — для процедурных скелетов.
    pub fn new(bones: Vec<(String, Option<usize>, Transform)>) -> Skeleton {
        let mut globals: Vec<Mat4> = Vec::with_capacity(bones.len());
        let mut result = Vec::with_capacity(bones.len());

        for (name, parent, bind_local) in bones {
            let local = bind_local.matrix();
            let global = match parent {
                Some(p) => globals[p] * local,
                None => local,
            };
            globals.push(global);
            result.push(Bone {
                name,
                parent,
                bind_local,
                inverse_bind: global.invert(),
            });
        }

        Skeleton { bones: result, root_rotation: Quat::IDENTITY }
    }

    pub fn len(&self) -> usize {
        self.bones.len()
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// Ищет кость по нормализованному имени.
    ///
    /// Экспортёры коверкают имена костей: Mixamo ставит префикс
    /// `mixamorig:`, а Sketchfab дописывает порядковый суффикс, из-за чего
    /// `mixamorig:LeftFoot` и `mixamorig:LeftFoot_058` — одна и та же кость,
    /// но посимвольно не совпадают.
    pub fn find_normalized(&self, name: &str) -> Option<usize> {
        if let Some(i) = self.find(name) {
            return Some(i);
        }
        let needle = normalize_bone_name(name);
        if needle.is_empty() {
            return None;
        }
        self.bones
            .iter()
            .position(|b| normalize_bone_name(&b.name) == needle)
    }

    /// Ищет кость по смыслу, а не по точному имени.
    ///
    /// Разные источники называют одно и то же по-разному: `hips`,
    /// `mixamorig:Hips`, `J_Bip_C_Hips`. Сначала пробуем точное совпадение,
    /// затем вхождение без учёта регистра — этого хватает, потому что
    /// префиксы у экспортёров стоят слева.
    pub fn find_like(&self, keys: &[&str]) -> Option<usize> {
        for key in keys {
            if let Some(i) = self.find(key) {
                return Some(i);
            }
        }
        for key in keys {
            let needle = normalize_bone_name(key);
            // Совпадений может быть несколько (`pelvis` найдётся и в
            // `pelvis.L`, и в `pelvis.R`), поэтому берём ближайшее к корню:
            // именно оно и есть та кость, которую имели в виду.
            let best = self
                .bones
                .iter()
                .enumerate()
                .filter(|(_, b)| normalize_bone_name(&b.name).contains(&needle))
                .min_by_key(|(i, _)| self.depth(*i))
                .map(|(i, _)| i);
            if best.is_some() {
                return best;
            }
        }
        None
    }

    /// Сколько предков у кости.
    pub fn depth(&self, mut bone: usize) -> usize {
        let mut d = 0;
        while let Some(p) = self.bones[bone].parent {
            bone = p;
            d += 1;
        }
        d
    }

    /// Характерный размер скелета: высота его костей в позе покоя.
    ///
    /// Нужен, потому что единицы у скелетов разные. Болванка живёт в метрах
    /// (рост ~1.5), риг из Mixamo — в сантиметрах (рост ~160). Смещения в
    /// процедурных клипах заданы числами, и без приведения к масштабу
    /// скелета подпрыгивание на 3.5 см превращается в 0.35 мм.
    pub fn height_hint(&self) -> f32 {
        let pose = self.rest_pose();
        let mut globals = Vec::new();
        self.global_matrices(&pose, &mut globals);

        let (mut min, mut max) = (f32::MAX, f32::MIN);
        for m in &globals {
            let y = m.transform_point(Vec3::ZERO).y;
            min = min.min(y);
            max = max.max(y);
        }
        if max > min {
            max - min
        } else {
            1.0
        }
    }

    /// Поза покоя.
    pub fn rest_pose(&self) -> Pose {
        Pose {
            locals: self.bones.iter().map(|b| b.bind_local).collect(),
        }
    }

    /// Локальные трансформы -> глобальные матрицы модели.
    pub fn global_matrices(&self, pose: &Pose, out: &mut Vec<Mat4>) {
        out.clear();
        for (i, bone) in self.bones.iter().enumerate() {
            let local = pose.locals[i].matrix();
            let global = match bone.parent {
                Some(p) => out[p] * local,
                None => local,
            };
            out.push(global);
        }
    }

    /// Матрицы для шейдера: глобальная поза, домноженная на обратный бинд.
    pub fn skin_matrices(&self, pose: &Pose, globals: &mut Vec<Mat4>, out: &mut Vec<Mat4>) {
        self.global_matrices(pose, globals);
        out.clear();
        for (i, bone) in self.bones.iter().enumerate() {
            out.push(globals[i] * bone.inverse_bind);
        }
    }
}

/// Кости, без которых развалится любая человеческая анимация. Их упрощение
/// не трогает, даже если по весам они выглядят малозначимыми.
///
/// Проверяется по нормализованному имени, поэтому работает и с `mixamorig:`,
/// и с блендеровскими `spine.001`.
const CORE_BONE_KEYWORDS: &[&str] = &[
    "root", "hips", "pelvis", "spine", "chest", "torso", "neck", "head",
    "shoulder", "clavicle", "upperarm", "forearm", "hand", "arm",
    "upleg", "thigh", "shin", "calf", "foot", "leg",
];

/// Пальцы формально содержат «hand», но для танца необязательны, поэтому
/// проверяются раньше основного списка.
const OPTIONAL_BONE_KEYWORDS: &[&str] = &[
    "thumb", "index", "middle", "ring", "pinky", "finger", "toe",
];

/// Можно ли пожертвовать этой костью при упрощении рига.
pub fn is_core_bone(name: &str) -> bool {
    let n = normalize_bone_name(name);
    if OPTIONAL_BONE_KEYWORDS.iter().any(|k| n.contains(k)) {
        return false;
    }
    CORE_BONE_KEYWORDS.iter().any(|k| n.contains(k))
}

/// Приводит имя кости к виду, сравнимому между экспортёрами: отбрасывает
/// префикс до двоеточия (`mixamorig:`), хвостовой числовой суффикс (`_058`)
/// и все разделители.
pub fn normalize_bone_name(name: &str) -> String {
    let base = name.rsplit(':').next().unwrap_or(name);

    // Хвост вида `_058` — нумерация экспортёра, а не часть имени кости.
    // Важно не тронуть осмысленные хвосты вроде `_End`.
    let trimmed = match base.rfind('_') {
        Some(at) if base[at + 1..].chars().all(|c| c.is_ascii_digit()) && at + 1 < base.len() => {
            &base[..at]
        }
        _ => base,
    };

    trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Поза — локальный трансформ каждой кости.
#[derive(Clone)]
pub struct Pose {
    pub locals: Vec<Transform>,
}

impl Pose {
    pub fn blend_from(&mut self, a: &Pose, b: &Pose, t: f32) {
        for i in 0..self.locals.len() {
            self.locals[i] = a.locals[i].blend(&b.locals[i], t);
        }
    }

    /// Накладывает аддитивный слой: смещения слоя считаются относительно позы
    /// покоя и домножаются на вес. Так покачивание в такт живёт поверх любого
    /// движения, а не вместо него.
    pub fn add_additive(&mut self, layer: &Pose, rest: &Pose, weight: f32) {
        if weight <= 0.0 {
            return;
        }
        for i in 0..self.locals.len() {
            let delta_t = (layer.locals[i].translation - rest.locals[i].translation) * weight;
            // Поворот-дельта: слой относительно покоя, ослабленный весом.
            let delta_r = layer.locals[i].rotation * rest.locals[i].rotation.conjugate();
            let delta_r = Quat::IDENTITY.slerp(delta_r, weight);

            self.locals[i].translation += delta_t;
            self.locals[i].rotation = (delta_r * self.locals[i].rotation).normalize();
        }
    }
}

/// Дорожка ключей одной кости. Пустой вектор означает «не трогать канал».
#[derive(Default)]
pub struct Track {
    pub bone: usize,
    pub translations: Vec<(f32, Vec3)>,
    pub rotations: Vec<(f32, Quat)>,
    pub scales: Vec<(f32, Vec3)>,
}

pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub looping: bool,
    pub tracks: Vec<Track>,
}

impl AnimationClip {
    /// Клип, в котором ничего не происходит.
    ///
    /// Такие приезжают вместе с моделями: Mixamo отдаёт «T-Pose» как
    /// анимацию на пару кадров. В списке движений ей делать нечего —
    /// нажатие на такую кнопку выглядит как сломанная игра.
    pub fn is_static(&self) -> bool {
        const MIN_DURATION: f32 = 0.35;
        const EPSILON: f32 = 1e-3;

        if self.duration < MIN_DURATION {
            return true;
        }

        // Дорожка с одинаковыми значениями — это бинд-поза, а не движение.
        self.tracks.iter().all(|track| {
            let rotations_still = match track.rotations.first() {
                Some((_, first)) => track
                    .rotations
                    .iter()
                    .all(|(_, q)| (1.0 - first.dot(*q).abs()) < EPSILON),
                None => true,
            };
            let translations_still = match track.translations.first() {
                Some((_, first)) => track
                    .translations
                    .iter()
                    .all(|(_, v)| (*v - *first).length() < EPSILON),
                None => true,
            };
            rotations_still && translations_still
        })
    }

    /// Вычисляет позу на момент `time`, начиная с позы покоя: каналы, которых
    /// в клипе нет, остаются как в покое.
    pub fn sample(&self, time: f32, rest: &Pose, out: &mut Pose) {
        out.locals.copy_from_slice(&rest.locals);

        let t = if self.looping && self.duration > 0.0 {
            time.rem_euclid(self.duration)
        } else {
            time.clamp(0.0, self.duration)
        };

        for track in &self.tracks {
            let local = &mut out.locals[track.bone];
            if let Some(v) = sample_keys(&track.translations, t, self.looping, self.duration, Vec3::lerp)
            {
                local.translation = v;
            }
            if let Some(v) = sample_keys(&track.rotations, t, self.looping, self.duration, Quat::slerp)
            {
                local.rotation = v;
            }
            if let Some(v) = sample_keys(&track.scales, t, self.looping, self.duration, Vec3::lerp) {
                local.scale = v;
            }
        }
    }
}

/// Ищет пару ключей вокруг `t` и интерполирует. У зацикленного клипа последний
/// ключ смыкается с первым, иначе на стыке цикла будет рывок.
fn sample_keys<T: Copy, F: Fn(T, T, f32) -> T>(
    keys: &[(f32, T)],
    t: f32,
    looping: bool,
    duration: f32,
    interpolate: F,
) -> Option<T> {
    match keys.len() {
        0 => return None,
        1 => return Some(keys[0].1),
        _ => {}
    }

    if t <= keys[0].0 {
        if looping {
            let (last_time, last_value) = keys[keys.len() - 1];
            let span = keys[0].0 + (duration - last_time);
            if span > 1e-6 {
                let k = (t + (duration - last_time)) / span;
                return Some(interpolate(last_value, keys[0].1, k));
            }
        }
        return Some(keys[0].1);
    }

    for w in keys.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if t <= t1 {
            let span = t1 - t0;
            let k = if span > 1e-6 { (t - t0) / span } else { 0.0 };
            return Some(interpolate(v0, v1, k));
        }
    }

    let (last_time, last_value) = keys[keys.len() - 1];
    if looping {
        let span = duration - last_time + keys[0].0;
        if span > 1e-6 {
            let k = (t - last_time) / span;
            return Some(interpolate(last_value, keys[0].1, k));
        }
    }
    Some(last_value)
}

/// Проигрыватель с кроссфейдом: держит текущий клип и уходящий предыдущий.
pub struct AnimationPlayer {
    rest: Pose,
    current: Option<PlayingClip>,
    previous: Option<PlayingClip>,
    fade_duration: f32,
    fade_elapsed: f32,

    // Переиспользуемые буферы: анимация считается каждый тик, и аллокации
    // здесь были бы чистым мусором.
    pose_a: Pose,
    pose_b: Pose,
    pub pose: Pose,
}

struct PlayingClip {
    clip: usize,
    time: f32,
    speed: f32,
}

impl AnimationPlayer {
    pub fn new(skeleton: &Skeleton) -> AnimationPlayer {
        let rest = skeleton.rest_pose();
        AnimationPlayer {
            pose_a: rest.clone(),
            pose_b: rest.clone(),
            pose: rest.clone(),
            rest,
            current: None,
            previous: None,
            fade_duration: 0.0,
            fade_elapsed: 0.0,
        }
    }

    pub fn rest(&self) -> &Pose {
        &self.rest
    }

    pub fn current_clip(&self) -> Option<usize> {
        self.current.as_ref().map(|c| c.clip)
    }

    /// Запускает клип с кроссфейдом. `fade` в секундах; 0 — мгновенно.
    pub fn play(&mut self, clip: usize, fade: f32, speed: f32) {
        if self.current.as_ref().map(|c| c.clip) == Some(clip) {
            return;
        }
        self.previous = self.current.take();
        self.current = Some(PlayingClip {
            clip,
            time: 0.0,
            speed,
        });
        self.fade_duration = if self.previous.is_some() { fade } else { 0.0 };
        self.fade_elapsed = 0.0;
    }

    /// Проматывает текущий клип на конкретное время — нужно, когда движение
    /// должно быть жёстко привязано к доле такта, а не идти само по себе.
    pub fn set_time(&mut self, time: f32) {
        if let Some(c) = self.current.as_mut() {
            c.time = time;
        }
    }

    pub fn update(&mut self, dt: f32, clips: &[AnimationClip]) {
        if let Some(c) = self.current.as_mut() {
            c.time += dt * c.speed;
        }
        if let Some(p) = self.previous.as_mut() {
            p.time += dt * p.speed;
        }

        if self.previous.is_some() {
            self.fade_elapsed += dt;
            if self.fade_elapsed >= self.fade_duration {
                self.previous = None;
            }
        }

        let current = match &self.current {
            Some(c) => c,
            None => {
                self.pose.locals.copy_from_slice(&self.rest.locals);
                return;
            }
        };

        clips[current.clip].sample(current.time, &self.rest, &mut self.pose_a);

        match (&self.previous, self.fade_duration > 1e-6) {
            (Some(prev), true) => {
                clips[prev.clip].sample(prev.time, &self.rest, &mut self.pose_b);
                let t = (self.fade_elapsed / self.fade_duration).clamp(0.0, 1.0);
                // Идём от уходящего клипа к текущему.
                self.pose.blend_from(&self.pose_b, &self.pose_a, t);
            }
            _ => self.pose.locals.copy_from_slice(&self.pose_a.locals),
        }
    }
}
