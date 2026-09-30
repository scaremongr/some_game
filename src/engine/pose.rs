//! Поза человека и сравнение поз.
//!
//! Здесь нет ни камеры, ни машинного зрения — только представление скелета и
//! метрика похожести. Распознавание живёт в браузере (`web/pose.js`), а сюда
//! приходят готовые точки.
//!
//! Точек тринадцать: голова, плечи, локти, кисти, таз, колени, стопы. Пальцы и
//! лицо распознаванию доступны, но в танце по ним ничего не судят, а каждая
//! лишняя точка — это шум.
//!
//! Координаты трёхмерные. Плоской позы не хватает: рука, вытянутая на камеру,
//! в проекции выглядит культёй, и аватар с плоскими костями не может ни
//! потянуться вперёд, ни развернуться.
//!
//! Пространство одно у всех трёх поз — человека с камеры, тренера и аватара:
//! `x` вправо от зрителя, `y` вверх, `z` на зрителя. Сравнивать и переносить
//! можно только в общем пространстве.
//!
//! Сравниваются **направления отрезков**, а не координаты: те зависят от того,
//! где человек стоит и какого он роста, а направления — нет.

use super::math3::Vec3;

/// Сустав. Порядок совпадает с порядком, в котором точки приходят из браузера,
/// — менять его нельзя, не поменяв `web/pose.js`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Joint {
    Head,
    ShoulderL,
    ShoulderR,
    ElbowL,
    ElbowR,
    WristL,
    WristR,
    HipL,
    HipR,
    KneeL,
    KneeR,
    AnkleL,
    AnkleR,
}

pub const JOINT_COUNT: usize = 13;

impl Joint {
    pub const ALL: [Joint; JOINT_COUNT] = [
        Joint::Head,
        Joint::ShoulderL,
        Joint::ShoulderR,
        Joint::ElbowL,
        Joint::ElbowR,
        Joint::WristL,
        Joint::WristR,
        Joint::HipL,
        Joint::HipR,
        Joint::KneeL,
        Joint::KneeR,
        Joint::AnkleL,
        Joint::AnkleR,
    ];

    pub fn index(self) -> usize {
        Joint::ALL.iter().position(|j| *j == self).unwrap_or(0)
    }

    /// Сустав другой стороны тела.
    pub fn opposite(self) -> Joint {
        match self {
            Joint::ShoulderL => Joint::ShoulderR,
            Joint::ShoulderR => Joint::ShoulderL,
            Joint::ElbowL => Joint::ElbowR,
            Joint::ElbowR => Joint::ElbowL,
            Joint::WristL => Joint::WristR,
            Joint::WristR => Joint::WristL,
            Joint::HipL => Joint::HipR,
            Joint::HipR => Joint::HipL,
            Joint::KneeL => Joint::KneeR,
            Joint::KneeR => Joint::KneeL,
            Joint::AnkleL => Joint::AnkleR,
            Joint::AnkleR => Joint::AnkleL,
            other => other,
        }
    }
}

/// Ниже этого порога точка считается не найденной.
pub const MIN_CONFIDENCE: f32 = 0.5;

/// Поза: точка плюс уверенность в ней.
///
/// Уверенность нужна, потому что распознавание регулярно теряет части тела —
/// руку за спиной, ноги вне кадра. Считать такую точку наравне с уверенно
/// найденной значит портить и оценку, и позу аватара.
#[derive(Clone, Copy)]
pub struct Pose {
    points: [Vec3; JOINT_COUNT],
    confidence: [f32; JOINT_COUNT],
}

impl Default for Pose {
    fn default() -> Pose {
        Pose {
            points: [Vec3::ZERO; JOINT_COUNT],
            confidence: [0.0; JOINT_COUNT],
        }
    }
}

impl Pose {
    pub fn set(&mut self, joint: Joint, position: Vec3, confidence: f32) {
        let i = joint.index();
        self.points[i] = position;
        self.confidence[i] = confidence;
    }

    pub fn get(&self, joint: Joint) -> Vec3 {
        self.points[joint.index()]
    }

    pub fn confidence(&self, joint: Joint) -> f32 {
        self.confidence[joint.index()]
    }

    pub fn has(&self, joint: Joint) -> bool {
        self.confidence[joint.index()] >= MIN_CONFIDENCE
    }

    /// Середина между двумя точками — ей заменяются центр таза и центр плеч.
    pub fn midpoint(&self, a: Joint, b: Joint) -> Vec3 {
        (self.get(a) + self.get(b)) * 0.5
    }

    /// Направление отрезка тела. `None`, если хоть один конец не найден или
    /// точки слиплись.
    pub fn direction(&self, from: Joint, to: Joint) -> Option<Vec3> {
        if !self.has(from) || !self.has(to) {
            return None;
        }
        let d = self.get(to) - self.get(from);
        if d.length() < 1e-5 {
            return None;
        }
        Some(d.normalize())
    }

    /// Насколько скелет вообще распознан: доля уверенно найденных точек.
    pub fn coverage(&self) -> f32 {
        let found = self
            .confidence
            .iter()
            .filter(|c| **c >= MIN_CONFIDENCE)
            .count();
        found as f32 / JOINT_COUNT as f32
    }
}

/// Отрезок тела, по углу которого идёт сравнение.
///
/// Вес отражает вклад в «похоже ли движение»: руки в танце читаются заметнее
/// ног.
pub struct Limb {
    pub from: Joint,
    pub to: Joint,
    pub weight: f32,
}

pub const LIMB_COUNT: usize = 8;

pub const LIMBS: [Limb; LIMB_COUNT] = [
    Limb { from: Joint::ShoulderL, to: Joint::ElbowL, weight: 1.3 },
    Limb { from: Joint::ShoulderR, to: Joint::ElbowR, weight: 1.3 },
    Limb { from: Joint::ElbowL, to: Joint::WristL, weight: 1.1 },
    Limb { from: Joint::ElbowR, to: Joint::WristR, weight: 1.1 },
    Limb { from: Joint::HipL, to: Joint::KneeL, weight: 0.9 },
    Limb { from: Joint::HipR, to: Joint::KneeR, weight: 0.9 },
    Limb { from: Joint::KneeL, to: Joint::AnkleL, weight: 0.7 },
    Limb { from: Joint::KneeR, to: Joint::AnkleR, weight: 0.7 },
];

/// Результат сравнения двух поз.
pub struct Match {
    /// Похожесть 0..1. Ниже 0.5 — это уже «делает что-то другое».
    pub score: f32,
    /// Доля отрезков, которые удалось сравнить. Низкая означает, что человека
    /// плохо видно, а не что он плохо танцует.
    pub coverage: f32,
    /// Похожесть по каждому отрезку — для подсветки того, что не совпало.
    pub limbs: [f32; LIMB_COUNT],
}

impl Default for Match {
    fn default() -> Match {
        Match {
            score: 0.0,
            coverage: 0.0,
            limbs: [0.0; LIMB_COUNT],
        }
    }
}

/// Сравнивает две позы по направлениям отрезков.
///
/// Обе позы должны быть в одном пространстве. Зеркалить здесь ничего не надо:
/// аватар строится как отражение человека, поэтому при правильном повторе он
/// совпадает с тренером напрямую.
pub fn compare(user: &Pose, reference: &Pose) -> Match {
    let mut total_weight = 0.0;
    let mut sum = 0.0;
    let mut compared = 0usize;
    let mut limbs = [0.0f32; LIMB_COUNT];

    for (i, limb) in LIMBS.iter().enumerate() {
        let (Some(a), Some(b)) = (
            user.direction(limb.from, limb.to),
            reference.direction(limb.from, limb.to),
        ) else {
            continue;
        };

        let similarity = direction_match(a, b);
        limbs[i] = similarity;
        sum += similarity * limb.weight;
        total_weight += limb.weight;
        compared += 1;
    }

    Match {
        score: if total_weight > 0.0 { sum / total_weight } else { 0.0 },
        coverage: compared as f32 / LIMB_COUNT as f32,
        limbs,
    }
}

/// Похожесть двух направлений: 1 — совпали, 0 — противоположны.
///
/// Считается через косинус угла, приведённый к 0..1 и слегка поджатый: без
/// поджатия «примерно туда» давало бы почти максимум, и разница между точным
/// повтором и приблизительным пропадала бы.
fn direction_match(a: Vec3, b: Vec3) -> f32 {
    let normalized = ((a.dot(b) + 1.0) * 0.5).clamp(0.0, 1.0);
    normalized * normalized
}

/// Состояние камеры и распознавания.
///
/// Числа приходят из `web/pose.js` — держим их синхронно с ним.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraStatus {
    Idle,
    RequestingCamera,
    LoadingModel,
    Running,
    Denied,
    Unsupported,
    Failed,
    /// PC: камеры нет, вместо человека работает подставная фигура.
    Standin,
}

impl CameraStatus {
    #[cfg(target_arch = "wasm32")]
    fn from_code(code: i32) -> CameraStatus {
        match code {
            1 => CameraStatus::RequestingCamera,
            2 => CameraStatus::LoadingModel,
            3 => CameraStatus::Running,
            4 => CameraStatus::Denied,
            5 => CameraStatus::Unsupported,
            6 => CameraStatus::Failed,
            _ => CameraStatus::Idle,
        }
    }

    /// Короткая строка для экрана. Только ASCII: шрифт движка другого не знает.
    pub fn message(self) -> &'static str {
        match self {
            CameraStatus::Idle => "CAMERA OFF",
            CameraStatus::RequestingCamera => "ASKING FOR CAMERA...",
            CameraStatus::LoadingModel => "LOADING POSE MODEL...",
            CameraStatus::Running => "TRACKING",
            CameraStatus::Denied => "CAMERA DENIED - ALLOW IT IN TELEGRAM SETTINGS",
            CameraStatus::Unsupported => "THIS BROWSER HAS NO CAMERA API",
            CameraStatus::Failed => "CAMERA OR MODEL FAILED TO START",
            CameraStatus::Standin => "NO CAMERA ON PC - USING A STAND-IN",
        }
    }

    pub fn is_working(self) -> bool {
        matches!(self, CameraStatus::Running | CameraStatus::Standin)
    }

    /// Состояние, из которого само уже ничего не поедет.
    pub fn is_final_failure(self) -> bool {
        matches!(
            self,
            CameraStatus::Denied | CameraStatus::Unsupported | CameraStatus::Failed
        )
    }
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn pose_start();
    fn pose_stop();
    fn pose_status() -> i32;
    fn pose_aspect() -> f32;
    fn pose_read(ptr: *mut f32) -> i32;
    fn pose_error(ptr: *mut u8, capacity: i32) -> i32;
}

/// Потолок для текста ошибки. Длиннее на экран телефона всё равно не влезет.
const ERROR_CAPACITY: usize = 192;

/// Сколько чисел на сустав приходит из браузера: x, y, z, уверенность.
const FLOATS_PER_JOINT: usize = 4;

/// Источник поз: камера в вебе, подставная фигура на PC.
pub struct Camera {
    buffer: [f32; JOINT_COUNT * FLOATS_PER_JOINT],
    pose: Pose,
    fresh: bool,
    /// Подробность отказа из браузера: какой шаг упал и с чем.
    ///
    /// Без неё на телефоне видно только «не завелось», а шагов там четыре
    /// (импорт модуля, wasm, файл модели, создание распознавателя) и
    /// подключиться отладчиком к WebView внутри Telegram нельзя.
    error: String,
    #[allow(dead_code)]
    error_buffer: [u8; ERROR_CAPACITY],
    /// Часы подставной фигуры на PC.
    #[cfg(not(target_arch = "wasm32"))]
    clock: f32,
}

impl Default for Camera {
    fn default() -> Camera {
        Camera {
            buffer: [0.0; JOINT_COUNT * FLOATS_PER_JOINT],
            pose: Pose::default(),
            fresh: false,
            error: String::new(),
            error_buffer: [0; ERROR_CAPACITY],
            #[cfg(not(target_arch = "wasm32"))]
            clock: 0.0,
        }
    }
}

impl Camera {
    pub fn start(&mut self) {
        self.error.clear();
        #[cfg(target_arch = "wasm32")]
        unsafe {
            pose_start()
        }
    }

    pub fn stop(&mut self) {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            pose_stop()
        }
        self.fresh = false;
    }

    pub fn status(&self) -> CameraStatus {
        #[cfg(target_arch = "wasm32")]
        unsafe {
            return CameraStatus::from_code(pose_status());
        }
        #[cfg(not(target_arch = "wasm32"))]
        CameraStatus::Standin
    }

    /// Забирает свежие точки и переводит их в пространство сцены.
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
    pub fn update(&mut self, dt: f32) {
        self.refresh_error();

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.standin(dt);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let count = unsafe { pose_read(self.buffer.as_mut_ptr()) };
            if count <= 0 {
                return;
            }

            // Браузер нормирует x шириной кадра, а y — высотой. Чтобы
            // направления не перекашивало, x и глубину приводим к масштабу
            // высоты: «на четверть кадра вправо» и «на четверть вниз» должны
            // быть одинаковыми отрезками.
            let aspect = unsafe { pose_aspect() }.max(0.01);

            for (i, joint) in Joint::ALL.iter().enumerate() {
                let base = i * FLOATS_PER_JOINT;
                let x = self.buffer[base] * aspect;
                let y = self.buffer[base + 1];
                let z = self.buffer[base + 2] * aspect;
                let confidence = self.buffer[base + 3];
                self.pose.set(*joint, to_scene(x, y, z), confidence);
            }
            self.fresh = true;
        }
    }

    /// Подставной человек для PC: камеры тут нет, а режим должен запускаться и
    /// здесь — иначе ни аватара, ни разметку экрана нечем посмотреть, кроме
    /// как с телефона в руках.
    ///
    /// Числа выдаются в тех же единицах, что и браузер: доли кадра, `y` вниз,
    /// и проходят то же преобразование осей. Иначе подстановка проверяла бы не
    /// тот код, который работает на телефоне.
    #[cfg(not(target_arch = "wasm32"))]
    fn standin(&mut self, dt: f32) {
        self.clock += dt;
        let t = self.clock;

        // Через полторы секунды фигура поднимает обе руки: это и жест старта,
        // и единственный способ проверить его распознавание без человека.
        let raise = ((t - 1.5) / 0.6).clamp(0.0, 1.0) * ((6.5 - t) / 0.6).clamp(0.0, 1.0);
        // А потом — только правую. Симметричная фигура не показала бы, что
        // стороны не перепутаны, а перепутать их тут проще всего: между
        // камерой и аватаром зеркало.
        let raise_right = ((t - 9.0) / 0.6).clamp(0.0, 1.0) * ((13.0 - t) / 0.6).clamp(0.0, 1.0);
        let swing = (t * 1.8).sin();

        let hip_y = 0.60;
        let unit = 0.0032;
        let mut put = |joint: Joint, x: f32, y: f32, z: f32| {
            let p = to_scene(0.5 + x * unit, hip_y + y * unit, z * unit);
            self.pose.set(joint, p, 1.0);
        };

        put(Joint::HipL, -14.0, 0.0, 0.0);
        put(Joint::HipR, 14.0, 0.0, 0.0);
        put(Joint::KneeL, -15.0, 55.0, 0.0);
        put(Joint::KneeR, 15.0, 55.0, 0.0);
        put(Joint::AnkleL, -16.0, 110.0, 0.0);
        put(Joint::AnkleR, 16.0, 110.0, 0.0);
        put(Joint::ShoulderL, -22.0, -62.0, 0.0);
        put(Joint::ShoulderR, 22.0, -62.0, 0.0);
        put(Joint::Head, 0.0, -95.0, 0.0);

        // Руки машут, а в поднятом состоянии уходят вверх. У левой меняется
        // ещё и глубина — по ней видно, что аватар не плоский.
        put(Joint::ElbowL, -34.0, -28.0 + swing * 8.0 - 40.0 * raise, -20.0 * swing);
        put(Joint::ElbowR, 34.0, -28.0 - swing * 8.0 - 40.0 * (raise + raise_right), 0.0);
        put(Joint::WristL, -40.0, 4.0 + swing * 14.0 - 100.0 * raise, -40.0 * swing);
        put(Joint::WristR, 40.0, 4.0 - swing * 14.0 - 100.0 * (raise + raise_right), 0.0);

        self.fresh = true;
    }

    /// Подробность последнего отказа; пустая строка, если её нет.
    pub fn error(&self) -> &str {
        &self.error
    }

    fn refresh_error(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            let written =
                unsafe { pose_error(self.error_buffer.as_mut_ptr(), ERROR_CAPACITY as i32) };
            if written <= 0 {
                return;
            }
            let bytes = &self.error_buffer[..(written as usize).min(ERROR_CAPACITY)];
            // JS пишет только печатный ASCII, поэтому from_utf8 не подведёт,
            // но падать из-за диагностики всё равно нельзя.
            let text = String::from_utf8_lossy(bytes).to_ascii_uppercase();
            if text != self.error {
                self.error = text;
            }
        }
    }

    pub fn pose(&self) -> &Pose {
        &self.pose
    }

    pub fn has_pose(&self) -> bool {
        self.fresh && self.pose.coverage() > 0.3
    }

    /// Обе руки подняты выше плеч — команда «начали».
    ///
    /// Жест, а не касание: телефон стоит в нескольких метрах, и подходить к
    /// нему ради старта — ровно то, чего режим должен избегать.
    pub fn hands_raised(&self) -> bool {
        [
            (Joint::WristL, Joint::ShoulderL),
            (Joint::WristR, Joint::ShoulderR),
        ]
        .iter()
        .all(|(wrist, shoulder)| {
            self.pose.has(*wrist)
                && self.pose.has(*shoulder)
                && self.pose.get(*wrist).y > self.pose.get(*shoulder).y + 0.03
        })
    }
}

/// Из осей браузера в оси сцены: `y` вниз становится `y` вверх, а глубина,
/// которая у браузера тем меньше, чем ближе к камере, — глубиной на зрителя.
fn to_scene(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y: -y, z: -z }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::math3::vec3;

    /// Простая поза: руки в стороны, ноги прямые. Оси уже сценические.
    fn t_pose() -> Pose {
        let mut p = Pose::default();
        p.set(Joint::Head, vec3(0.0, 100.0, 0.0), 1.0);
        p.set(Joint::ShoulderL, vec3(-20.0, 60.0, 0.0), 1.0);
        p.set(Joint::ShoulderR, vec3(20.0, 60.0, 0.0), 1.0);
        p.set(Joint::ElbowL, vec3(-60.0, 60.0, 0.0), 1.0);
        p.set(Joint::ElbowR, vec3(60.0, 60.0, 0.0), 1.0);
        p.set(Joint::WristL, vec3(-100.0, 60.0, 0.0), 1.0);
        p.set(Joint::WristR, vec3(100.0, 60.0, 0.0), 1.0);
        p.set(Joint::HipL, vec3(-15.0, 0.0, 0.0), 1.0);
        p.set(Joint::HipR, vec3(15.0, 0.0, 0.0), 1.0);
        p.set(Joint::KneeL, vec3(-15.0, -60.0, 0.0), 1.0);
        p.set(Joint::KneeR, vec3(15.0, -60.0, 0.0), 1.0);
        p.set(Joint::AnkleL, vec3(-15.0, -120.0, 0.0), 1.0);
        p.set(Joint::AnkleR, vec3(15.0, -120.0, 0.0), 1.0);
        p
    }

    /// Та же поза, но левая рука поднята вверх.
    fn hand_up() -> Pose {
        let mut p = t_pose();
        p.set(Joint::ElbowL, vec3(-20.0, 100.0, 0.0), 1.0);
        p.set(Joint::WristL, vec3(-20.0, 140.0, 0.0), 1.0);
        p
    }

    #[test]
    fn identical_poses_score_high() {
        let m = compare(&t_pose(), &t_pose());
        assert!(m.score > 0.99, "{}", m.score);
        assert_eq!(m.coverage, 1.0);
    }

    #[test]
    fn different_poses_score_lower() {
        let same = compare(&t_pose(), &t_pose()).score;
        let other = compare(&hand_up(), &t_pose()).score;
        assert!(other < same - 0.1, "{other} против {same}");
    }

    /// Поза, повёрнутая на 180 градусов, — это уже совсем другое движение.
    #[test]
    fn rotated_pose_scores_badly() {
        let base = t_pose();
        let mut upside = Pose::default();
        for joint in Joint::ALL {
            let p = base.get(joint);
            upside.set(joint, vec3(-p.x, -p.y, p.z), 1.0);
        }
        let m = compare(&upside, &base);
        assert!(m.score < 0.1, "{}", m.score);
    }

    /// Оценка не должна зависеть от того, где человек стоит и какого он роста.
    #[test]
    fn position_and_scale_do_not_matter() {
        let base = t_pose();
        let mut moved = Pose::default();
        for joint in Joint::ALL {
            moved.set(joint, base.get(joint) * 2.5 + vec3(500.0, -300.0, 40.0), 1.0);
        }
        let m = compare(&moved, &base);
        assert!(m.score > 0.99, "{}", m.score);
    }

    /// Глубина участвует в оценке: рука, вытянутая на камеру, — не то же
    /// самое, что рука, вытянутая в сторону. Ради этого и заводились три
    /// измерения.
    #[test]
    fn depth_changes_the_score() {
        let mut forward = t_pose();
        forward.set(Joint::WristR, vec3(60.0, 60.0, 80.0), 1.0);
        let m = compare(&forward, &t_pose());
        let index = LIMBS
            .iter()
            .position(|l| l.from == Joint::ElbowR && l.to == Joint::WristR)
            .unwrap();
        assert!(m.limbs[index] < 0.8, "{}", m.limbs[index]);
    }

    /// Потерянные точки должны ронять покрытие, а не оценку: иначе «тебя плохо
    /// видно» выглядело бы как «ты плохо танцуешь».
    #[test]
    fn missing_points_lower_coverage_not_score() {
        let mut partial = t_pose();
        partial.set(Joint::AnkleL, vec3(-15.0, -120.0, 0.0), 0.0);
        partial.set(Joint::AnkleR, vec3(15.0, -120.0, 0.0), 0.0);

        let m = compare(&partial, &t_pose());
        assert!(m.coverage < 1.0);
        assert!(m.score > 0.99, "{}", m.score);
    }
}
