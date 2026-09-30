//! Режим «повторяй за мной»: на сцене двое — тренер и ваш аватар.
//!
//! Видео с камеры не показывается вообще. Камера нужна только распознаванию:
//! по тринадцати точкам разворачивается скелет второго персонажа, и на экране
//! вы видите не себя, а свою фигуру, повторяющую ваши движения.
//!
//! Сравниваются два скелета, а не поза человека с позой персонажа. Обе фигуры
//! живут в одном пространстве и построены на одном риге, поэтому сравнение
//! сводится к направлениям костей и не требует ни зеркалирования, ни поправок
//! на рост.
//!
//! Аватар зеркальный: подняв правую руку, вы ждёте, что фигура поднимет ту,
//! что окажется на той же стороне экрана.

use crate::engine::app::{FrameCtx, Scene, Transition};
use crate::engine::audio::Music;
use crate::engine::beat::BeatMap;
use crate::engine::graphics::Graphics;
use crate::engine::math::*;
use crate::engine::math3::{vec3, Mat4};
use crate::engine::pose::{self, Camera as PoseCamera, CameraStatus, Joint, Match, Pose};
use crate::engine::render3d::Lighting;
use crate::engine::retarget::{self, Rig};
use crate::engine::skeleton::{normalize_bone_name, Pose as SkeletonPose};
use crate::engine::ui;

use super::camera::StageCamera;
use super::character::bone;
use super::dancer::{load_bytes, Dancer, PendingBytes, FADE, TARGET_HEIGHT};
use super::palette;

const MUSIC_PATH: &str = "assets/music/track.wav";
const BEATMAP_PATH: &str = "assets/music/track.beatmap";
const FALLBACK_BPM: f32 = 120.0;

const FOV_Y: f32 = 40.0;
/// Насколько тёмная плоская тень под фигурой. Без неё непонятно, стоит она на
/// полу или висит.
const SHADOW_OPACITY: f32 = 0.35;

/// Движение меняется на границе такта, а не по секундомеру: смена посреди доли
/// читается как сбой танца.
const BEATS_PER_MOVE: f32 = 16.0;

/// Сколько держать руки поднятыми, чтобы это считалось командой, а не
/// случайным взмахом.
const GESTURE_HOLD: f32 = 1.2;
/// Отсчёт перед стартом: человек стоит в двух-трёх метрах от телефона, и ему
/// нужно время встать в позицию.
const COUNTDOWN: f32 = 4.0;
/// Сколько ждать музыку, прежде чем пускать в режим без неё.
const AUDIO_WAIT: f32 = 8.0;

/// Постоянная сглаживания оценки. Мгновенная слишком дёргается: распознавание
/// шумит, и без сглаживания цифра прыгала бы на десятки процентов за кадр.
const SCORE_SMOOTHING: f32 = 0.12;
/// Насколько быстро аватар догоняет распознанную позу.
///
/// Без сглаживания фигуру трясёт: распознавание выдаёт точки с дрожью в доли
/// процента, а на длине руки это уже заметное подёргивание кисти.
const POSE_SMOOTHING: f32 = 0.35;

/// Что сейчас происходит в режиме.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Камера выключена, ждём касания.
    Off,
    /// Камера работает: ищем человека и ждём от него команды.
    Waiting,
    /// Команда принята, идёт отсчёт.
    Countdown,
    /// Играет музыка, идёт счёт.
    Dancing,
}

pub struct MirrorScene {
    dancer: Dancer,
    stage: StageCamera,
    light: Lighting,

    pose_camera: PoseCamera,
    rig: Rig,
    /// Для какого скелета собран `rig`. Модель приезжает асинхронно, и до неё
    /// собирать нечего.
    rig_built_for: usize,

    /// Сглаженная поза человека — из неё разворачивается аватар.
    smoothed: Pose,
    /// Поза аватара: тот же риг, что у тренера, но развёрнутый по человеку.
    /// Поза покоя модели — основа, от которой строится аватар.
    bind: Option<SkeletonPose>,
    avatar_pose: Option<SkeletonPose>,
    avatar_globals: Vec<Mat4>,
    avatar_skin: Vec<Mat4>,

    result: Match,
    score: f32,
    total: f32,
    samples: u32,

    music: Music,
    pending_music: PendingBytes,
    pending_beatmap: PendingBytes,
    beatmap: Option<BeatMap>,

    phase: Phase,
    /// Сколько уже держатся поднятые руки.
    gesture: f32,
    countdown: f32,
    /// Время трека, секунды. От аудио, когда музыка играет.
    time: f32,
    /// Часы сцены — идут всегда, по ним живёт интерфейс.
    clock: f32,
    camera_started: f32,
    move_started: f32,
    beat_flash: f32,
    last_beat: i32,
}

impl MirrorScene {
    pub fn new() -> MirrorScene {
        let mut stage = StageCamera::new(TARGET_HEIGHT);
        // Покачивание уводило бы фигуры из кадра: их тут две, и запаса по
        // краям почти нет.
        stage.auto_sway = false;

        MirrorScene {
            dancer: Dancer::new(60.0 / FALLBACK_BPM),
            stage,
            light: Lighting::default(),
            pose_camera: PoseCamera::default(),
            rig: Rig::default(),
            rig_built_for: usize::MAX,
            smoothed: Pose::default(),
            bind: None,
            avatar_pose: None,
            avatar_globals: Vec::new(),
            avatar_skin: Vec::new(),
            result: Match::default(),
            score: 0.0,
            total: 0.0,
            samples: 0,
            music: Music::new(),
            pending_music: load_bytes(MUSIC_PATH),
            pending_beatmap: load_bytes(BEATMAP_PATH),
            beatmap: None,
            // Экран «коснитесь, чтобы включить камеру» существует только ради
            // браузера: тот не даёт ни камеру, ни звук без жеста. На PC ни
            // того ни другого не требуется, и спрашивать не о чем.
            phase: if cfg!(target_arch = "wasm32") {
                Phase::Off
            } else {
                Phase::Waiting
            },
            gesture: 0.0,
            countdown: 0.0,
            time: 0.0,
            clock: 0.0,
            camera_started: 0.0,
            move_started: 0.0,
            beat_flash: 0.0,
            last_beat: -1,
        }
    }

    fn status(&self) -> CameraStatus {
        self.pose_camera.status()
    }

    fn seconds_per_beat(&self) -> f32 {
        match &self.beatmap {
            Some(map) => map.seconds_per_beat(),
            None => 60.0 / FALLBACK_BPM,
        }
    }

    fn beat_position(&self) -> f32 {
        match &self.beatmap {
            Some(map) => map.beat_position(self.time),
            None => self.time / self.seconds_per_beat(),
        }
    }

    /// Ищет кости персонажа, соответствующие суставам позы. Пересобирается,
    /// когда приезжает другая модель.
    fn ensure_rig(&mut self) {
        let skeleton = &self.dancer.character.skeleton;
        if self.rig_built_for == skeleton.len() {
            return;
        }
        self.rig_built_for = skeleton.len();

        let names: [(Joint, &[&str]); pose::JOINT_COUNT] = [
            (Joint::Head, bone::HEAD),
            (Joint::ShoulderL, bone::SHOULDER_L),
            (Joint::ShoulderR, bone::SHOULDER_R),
            (Joint::ElbowL, bone::ELBOW_L),
            (Joint::ElbowR, bone::ELBOW_R),
            (Joint::WristL, bone::WRIST_L),
            (Joint::WristR, bone::WRIST_R),
            (Joint::HipL, bone::HIP_L),
            (Joint::HipR, bone::HIP_R),
            (Joint::KneeL, bone::KNEE_L),
            (Joint::KneeR, bone::KNEE_R),
            (Joint::AnkleL, bone::ANKLE_L),
            (Joint::AnkleR, bone::ANKLE_R),
        ];

        let mut rig = Rig::default();
        for (joint, keys) in names {
            rig.joints[joint.index()] = skeleton.find_like(keys);
        }
        rig.spine = skeleton.find_like(bone::SPINE);
        // Шея — родитель головы. Отдельного списка имён не надо: «кость выше
        // головы» и есть шея в любом человеческом риге.
        // Проверка имени обязательна: если шеи в риге нет, родителем головы
        // окажется грудь, и «наклон головы» превращался бы в наклон всего
        // корпуса.
        rig.neck = rig.joints[Joint::Head.index()]
            .and_then(|head| skeleton.bones[head].parent)
            .filter(|n| normalize_bone_name(&skeleton.bones[*n].name).contains("neck"));

        self.rig = rig;
        self.bind = None;
        self.avatar_pose = None;
    }

    fn mapped_joints(&self) -> usize {
        self.rig.joints.iter().filter(|b| b.is_some()).count()
    }

    /// Подтягивает сглаженную позу к свежей.
    fn smooth_pose(&mut self) {
        let fresh = *self.pose_camera.pose();
        for joint in Joint::ALL {
            if !fresh.has(joint) {
                // Точку потеряли — оставляем последнюю известную, но гасим
                // уверенность, чтобы кость перестали двигать.
                let keep = self.smoothed.get(joint);
                self.smoothed.set(joint, keep, fresh.confidence(joint));
                continue;
            }
            let target = fresh.get(joint);
            let current = if self.smoothed.has(joint) {
                self.smoothed.get(joint)
            } else {
                target
            };
            self.smoothed.set(
                joint,
                current + (target - current) * POSE_SMOOTHING,
                fresh.confidence(joint),
            );
        }
    }

    /// Разворачивает скелет аватара по позе человека.
    fn drive_avatar(&mut self) {
        let character = &self.dancer.character;
        if character.skeleton.len() == 0 {
            return;
        }

        // Основа — текущий кадр танца, а не поза покоя.
        //
        // Во-первых, от предыдущего кадра аватара считать нельзя: мелкие
        // ошибки доворота копились бы и фигуру постепенно скручивало.
        // Во-вторых, поза покоя у модели своя и бывает какой угодно (у нашей —
        // с разведёнными в стороны ногами), а кости, которых в позе человека
        // нет вовсе — пальцы, стопы, скрутки, — остаются как есть. С танца они
        // выглядят живыми, с позы покоя — сломанными.
        // Основа — поза покоя, а не кадр танца: аватар должен повторять только
        // за человеком, и от танца в нём не остаётся ничего. Кости, которых в
        // позе человека нет вовсе — пальцы, стопы, скрутки, — так и стоят в
        // покое, и это единственное, что не двигается.
        let base = self
            .bind
            .get_or_insert_with(|| character.skeleton.rest_pose());
        let pose = self.avatar_pose.get_or_insert_with(|| base.clone());
        pose.locals.clone_from(&base.locals);

        // Голову распознавание отдаёт носом, а нос заметно впереди оси шеи.
        // Направив шею прямо на него, мы бы навсегда наклонили голову вперёд —
        // именно так она и оказывалась опущенной. Глубину носа поэтому
        // приравниваем к плечам: наклоны вбок и вверх-вниз остаются, поклона
        // на пустом месте больше нет.
        let mut scene = self.smoothed;
        if scene.has(Joint::Head) && scene.has(Joint::ShoulderL) && scene.has(Joint::ShoulderR) {
            let shoulders = scene.midpoint(Joint::ShoulderL, Joint::ShoulderR);
            let mut head = scene.get(Joint::Head);
            head.z = shoulders.z;
            scene.set(Joint::Head, head, scene.confidence(Joint::Head));
        }

        // Скелет живёт в пространстве модели, а поза приходит в пространстве
        // сцены. Между ними у моделей из сети бывает что угодно: поворот осей
        // (Z вверх вместо Y), масштаб в сотню раз. Не переведя позу, мы
        // доворачивали бы кости к целям из чужой системы координат — фигуру
        // просто складывает.
        let to_model = character.transform.invert();
        let mut local = scene;
        for joint in Joint::ALL {
            local.set(
                joint,
                to_model.transform_point(scene.get(joint)),
                scene.confidence(joint),
            );
        }

        retarget::drive(&character.skeleton, &self.rig, &local, true, pose);
        character
            .skeleton
            .skin_matrices(pose, &mut self.avatar_globals, &mut self.avatar_skin);
    }

    /// Меняет движение на границе такта.
    fn advance_move(&mut self) {
        let beats = self.beat_position();
        let idle = self.dancer.character.player.current_clip().is_none();
        if !idle && beats - self.move_started < BEATS_PER_MOVE {
            return;
        }
        self.move_started = beats;
        if let Some(next) = self.dancer.character.next_move() {
            self.dancer.character.player.play(next, FADE, 1.0);
        }
    }

    fn take_pending_audio(&mut self) {
        if let Some(result) = self.pending_beatmap.borrow_mut().take() {
            if let Ok(map) = result.and_then(|b| BeatMap::from_text(&String::from_utf8_lossy(&b))) {
                self.beatmap = Some(map);
            }
        }
        if let Some(Ok(bytes)) = self.pending_music.borrow_mut().take() {
            // Отказ звука не должен уносить режим: без музыки он работает.
            let _ = self.music.load(&bytes);
        }
    }

    fn begin_countdown(&mut self) {
        self.phase = Phase::Countdown;
        self.countdown = COUNTDOWN;
        self.gesture = 0.0;
        self.score = 0.0;
        self.total = 0.0;
        self.samples = 0;
    }

    fn begin_dance(&mut self, now: f64) {
        self.phase = Phase::Dancing;
        self.time = 0.0;
        self.move_started = 0.0;
        self.last_beat = -1;
        self.music.play(now);
    }

    // --- отрисовка ---

    fn draw_hud(&self, g: &mut Graphics, canvas: Vec2) {
        let status = self.status();
        let center = vec2(canvas.x * 0.5, canvas.y * 0.5);

        g.text("MIRROR", vec2(12.0, 10.0), 2.0, palette::TITLE);

        if self.phase == Phase::Off {
            if !self.music.is_loaded() && self.clock <= AUDIO_WAIT {
                g.text_centered("LOADING MUSIC...", center, 2.0, palette::MUTED);
                return;
            }
            g.text_centered("TAP TO TURN ON THE CAMERA", center, 2.0, palette::TITLE);
            g.text_centered(
                "STAND THE PHONE UP AND STEP BACK 2-3 METERS",
                center + vec2(0.0, 22.0),
                1.0,
                palette::MUTED,
            );
            return;
        }

        if status.is_final_failure() {
            let mut y = center.y;
            g.text_centered(status.message(), vec2(center.x, y), 1.0, palette::MATCH_BAD);
            y += 16.0;
            // Причину надо показать целиком: подключиться отладчиком к WebView
            // внутри Telegram нельзя.
            for line in ui::wrap(self.pose_camera.error(), ui::line_chars(canvas.x)) {
                g.text_centered(&line, vec2(center.x, y), 1.0, palette::MUTED);
                y += 10.0;
            }
            return;
        }

        if !status.is_working() {
            g.text_centered(status.message(), center, 1.0, palette::TITLE);
            if status == CameraStatus::LoadingModel {
                g.text_centered(
                    &format!(
                        "ABOUT 15 MB, FIRST TIME ONLY   {:.0}S",
                        self.clock - self.camera_started
                    ),
                    center + vec2(0.0, 16.0),
                    1.0,
                    palette::MUTED,
                );
            }
            return;
        }

        let top = vec2(canvas.x * 0.5, canvas.y * 0.13);
        match self.phase {
            Phase::Off => {}
            Phase::Waiting => self.draw_waiting(g, canvas, top),
            Phase::Countdown => {
                let left = self.countdown.ceil().max(1.0) as i32;
                g.text_centered(&format!("{left}"), top, 6.0, palette::TITLE);
                g.text_centered("GET READY", top + vec2(0.0, 34.0), 2.0, palette::ACCENT);
            }
            Phase::Dancing => self.draw_score(g, canvas),
        }

        if self.mapped_joints() < pose::JOINT_COUNT {
            g.text(
                &format!(
                    "RIG MAPPED: {} OF {}",
                    self.mapped_joints(),
                    pose::JOINT_COUNT
                ),
                vec2(12.0, 28.0),
                1.0,
                palette::PLAYER.with_alpha(0.8),
            );
        }
    }

    fn draw_waiting(&self, g: &mut Graphics, canvas: Vec2, top: Vec2) {
        if !self.pose_camera.has_pose() {
            g.text_centered("STEP BACK SO THE WHOLE BODY FITS", top, 1.0, palette::PLAYER);
            return;
        }

        g.text_centered("RAISE BOTH HANDS TO START", top, 2.0, palette::TITLE);

        // Полоса удержания: без неё непонятно, засчитывается жест или нет.
        let width = (canvas.x - 96.0).min(220.0);
        let bar = rect((canvas.x - width) * 0.5, top.y + 18.0, width, 6.0);
        g.rect(bar, palette::STAGE_TOP.with_alpha(0.7));
        let filled = (self.gesture / GESTURE_HOLD).clamp(0.0, 1.0);
        g.rect(rect(bar.x, bar.y, bar.w * filled, bar.h), palette::ACCENT);

        if self.samples > 0 {
            g.text_centered(
                &format!("LAST RUN: {:.0}%", self.total / self.samples as f32 * 100.0),
                top + vec2(0.0, 34.0),
                1.0,
                palette::MUTED,
            );
        }
    }

    fn draw_score(&self, g: &mut Graphics, canvas: Vec2) {
        if !self.pose_camera.has_pose() {
            g.text_centered(
                "CAN'T SEE YOU",
                vec2(canvas.x * 0.5, canvas.y * 0.13),
                2.0,
                palette::PLAYER,
            );
            return;
        }

        let color = quality_color(self.score);
        g.text_centered(
            &format!("{}%", (self.score * 100.0).round() as i32),
            vec2(canvas.x * 0.5, canvas.y * 0.10),
            4.0,
            color,
        );
        g.text_centered(
            verdict(self.score),
            vec2(canvas.x * 0.5, canvas.y * 0.10 + 26.0),
            2.0,
            color,
        );

        let width = (canvas.x - 48.0).min(320.0);
        let bar = rect((canvas.x - width) * 0.5, canvas.y - 26.0, width, 8.0);
        g.rect(bar, palette::STAGE_TOP.with_alpha(0.8));
        g.rect(
            rect(bar.x, bar.y, bar.w * self.score.clamp(0.0, 1.0), bar.h),
            color,
        );

        let average = if self.samples > 0 {
            self.total / self.samples as f32
        } else {
            0.0
        };
        g.text_centered(
            &format!(
                "AVERAGE {:.0}%   {}",
                average * 100.0,
                self.dancer.character.current_move_name()
            ),
            vec2(canvas.x * 0.5, canvas.y - 12.0),
            1.0,
            palette::MUTED,
        );
    }
}

fn verdict(score: f32) -> &'static str {
    if score > 0.85 {
        "PERFECT"
    } else if score > 0.7 {
        "GREAT"
    } else if score > 0.55 {
        "OK"
    } else {
        "OFF BEAT"
    }
}

/// Зелёный к красному через жёлтый: промежуточное состояние должно читаться,
/// а не выглядеть «почти зелёным».
fn quality_color(quality: f32) -> Color {
    if quality > 0.5 {
        palette::MATCH_GOOD.lerp(palette::MATCH_FAIR, (1.0 - quality) * 2.0)
    } else {
        palette::MATCH_FAIR.lerp(palette::MATCH_BAD, 1.0 - quality * 2.0)
    }
}

impl Scene for MirrorScene {
    fn update(&mut self, ctx: &FrameCtx) -> Transition {
        if ui::back_pressed(ctx.input) {
            self.music.stop();
            self.pose_camera.stop();
            return Transition::Pop;
        }

        self.clock += ctx.dt;
        self.dancer.update(ctx.dt);
        self.take_pending_audio();
        self.ensure_rig();

        let pressed = ctx.input.tapped() || ui::confirm_pressed(ctx.input);

        if self.phase == Phase::Off {
            // Ждём музыку прежде, чем принять касание: звук разрешается только
            // проигрыванием во время жеста, а другого жеста потом не будет.
            // Если трек так и не приехал, через несколько секунд пускаем без
            // него — режим работает и молча.
            let audio_settled = self.music.is_loaded() || self.clock > AUDIO_WAIT;
            if pressed && audio_settled {
                self.phase = Phase::Waiting;
                self.camera_started = self.clock;
                self.music.unlock();
                self.pose_camera.start();
            }
            return Transition::None;
        }

        self.stage.fit(TARGET_HEIGHT, ctx.screen, FOV_Y.to_radians());
        self.stage.follow(vec3(0.0, TARGET_HEIGHT * 0.52, 0.0), ctx.dt);

        self.pose_camera.update(ctx.dt);
        if self.pose_camera.has_pose() {
            self.smooth_pose();
        }

        match self.phase {
            Phase::Off => {}
            Phase::Waiting => {
                // Касание тоже годится: на компьютере жеста может не быть.
                if self.pose_camera.hands_raised() {
                    self.gesture += ctx.dt;
                } else {
                    self.gesture = 0.0;
                }
                if self.gesture >= GESTURE_HOLD || pressed {
                    self.begin_countdown();
                }
            }
            Phase::Countdown => {
                self.countdown -= ctx.dt;
                if self.countdown <= 0.0 {
                    self.begin_dance(ctx.now);
                }
            }
            Phase::Dancing => {
                self.time = if self.music.is_playing() {
                    self.music.position(ctx.now)
                } else {
                    self.time + ctx.dt
                };

                let beat = self.beat_position().floor() as i32;
                if beat != self.last_beat {
                    self.last_beat = beat;
                    self.beat_flash = 1.0;
                }
                self.beat_flash = (self.beat_flash - ctx.dt * 4.0).max(0.0);

                self.advance_move();
            }
        }

        self.dancer.character.update(self.time, ctx.dt);
        self.drive_avatar();

        // Сравниваются две фигуры, а не человек с фигурой: обе в одном
        // пространстве и на одном риге, так что разница — чистая разница поз.
        if self.phase == Phase::Dancing && self.pose_camera.has_pose() {
            let coach = retarget::sample(&self.dancer.character.globals, &self.rig.joints);
            let avatar = retarget::sample(&self.avatar_globals, &self.rig.joints);
            self.result = pose::compare(&avatar, &coach);
            self.score += (self.result.score - self.score) * SCORE_SMOOTHING;
            self.total += self.result.score;
            self.samples += 1;
        } else if self.phase != Phase::Dancing {
            self.result = Match::default();
        }

        Transition::None
    }

    fn draw(&mut self, g: &mut Graphics, _alpha: f32) {
        self.dancer.ensure_mesh(g);

        let canvas = g.canvas();
        let camera = self.stage.camera(FOV_Y.to_radians());

        // Пол: без него фигуры висят в пустоте и не читается, что они стоят
        // рядом, а не одна за другой.
        let horizon = camera.horizon_y(canvas);
        g.rect(rect(0.0, 0.0, canvas.x, horizon), palette::STAGE_TOP);
        g.rect(
            rect(0.0, horizon, canvas.x, canvas.y - horizon),
            palette::FLOOR_FAR.lerp(palette::FLOOR_NEAR, 0.5),
        );

        // Тренер не рисуется: он есть только как источник движения и как то,
        // с чем сверяется поза. На экране одна фигура — ваша, во весь кадр.
        if let (Some(mesh), false) = (&self.dancer.character.mesh, self.avatar_skin.is_empty()) {
            g.draw_skinned(
                mesh,
                &self.avatar_skin,
                self.dancer.character.transform,
                &camera,
                &self.light,
                SHADOW_OPACITY,
            );
        }

        self.draw_hud(g, canvas);
    }
}
