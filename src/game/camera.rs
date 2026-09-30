//! Орбитальная камера, следящая за персонажем.
//!
//! Нужна потому, что анимации Mixamo несут корневое движение: танцовщица
//! уходит вбок, а неподвижная камера теряет её из кадра. Здесь два разных
//! механизма, и их полезно не путать:
//!
//! * **слежение** — точка интереса тянется за фигурой, чтобы та не убегала;
//! * **орбита** — вокруг этой точки можно вращаться и приближаться.

use crate::engine::math::Vec2;
use crate::engine::math3::{vec3, Vec3};
use crate::engine::render3d::Camera;

/// Пока фигура не отошла дальше этого радиуса, камера стоит на месте.
///
/// Без мёртвой зоны камера ползла бы за каждым покачиванием бёдер, и кадр
/// мелко дрожал бы весь трек.
const DEADZONE: f32 = 0.22;

/// За сколько секунд камера съедает большую часть отставания.
const FOLLOW_TIME: f32 = 0.35;

const MIN_PITCH: f32 = -0.15;
const MAX_PITCH: f32 = 0.85;
const MIN_DISTANCE: f32 = 1.6;
const MAX_DISTANCE: f32 = 6.5;

/// Медленное покачивание камеры, когда игрок её не трогает: кадр перестаёт
/// быть открыточным, а голова не кружится, как от полного оборота.
const SWAY_AMPLITUDE: f32 = 0.32;
const SWAY_PERIOD: f32 = 19.0;

pub struct StageCamera {
    /// Сглаженная точка интереса.
    focus: Vec3,
    /// Куда камера смотрит по вертикали относительно точки интереса.
    pub height: f32,
    pub distance: f32,
    /// Угол, заданный игроком. Полный угол — он плюс покачивание.
    pub yaw: f32,
    pub pitch: f32,
    /// Пока true, камера сама медленно водит по дуге.
    pub auto_sway: bool,
    /// Игрок покрутил колесо — дальше кадрируем не мы.
    manual_zoom: bool,
    time: f32,
}

impl StageCamera {
    pub fn new(height: f32) -> StageCamera {
        StageCamera {
            focus: vec3(0.0, height * 0.55, 0.0),
            height: 0.0,
            distance: height * 2.15,
            yaw: 0.0,
            pitch: 0.10,
            auto_sway: true,
            manual_zoom: false,
            time: 0.0,
        }
    }

    /// Подтягивает точку интереса к фигуре.
    pub fn follow(&mut self, target: Vec3, dt: f32) {
        self.time += dt;

        let delta = target - self.focus;
        let distance = delta.length();
        if distance > DEADZONE {
            // Тянемся не к самой фигуре, а к границе мёртвой зоны: иначе
            // при выходе из неё камера дёргалась бы рывком.
            let goal = target - delta * (DEADZONE / distance);
            // Экспоненциальное сглаживание, не зависящее от частоты кадров.
            let k = 1.0 - (-dt / FOLLOW_TIME).exp();
            self.focus += (goal - self.focus) * k;
        }
    }

    /// Поворот от игрока. Ручное управление отключает автоматическое
    /// покачивание — иначе кадр уползал бы из-под рук.
    pub fn orbit(&mut self, dyaw: f32, dpitch: f32) {
        if dyaw == 0.0 && dpitch == 0.0 {
            return;
        }
        self.auto_sway = false;
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(MIN_PITCH, MAX_PITCH);
    }

    /// Подгоняет расстояние и точку взгляда под форму холста.
    ///
    /// Поле зрения задано по вертикали, поэтому на вытянутом экране фигура
    /// сама по себе оказалась бы мелкой и ровно по центру — а центр в
    /// портрете занят сценой, тогда как низ отдан кнопкам и дорожке.
    /// Поэтому в портрете фигура занимает меньшую долю кадра и поднимается
    /// выше середины.
    pub fn fit(&mut self, character_height: f32, canvas: Vec2, fov_y: f32) {
        if self.manual_zoom {
            return;
        }
        let portrait = canvas.y > canvas.x;
        // Какую долю высоты кадра занимает фигура.
        let fraction = if portrait { 0.40 } else { 0.64 };

        let visible = character_height / fraction;
        self.distance = (visible * 0.5 / (fov_y * 0.5).tan()).clamp(MIN_DISTANCE, MAX_DISTANCE);

        // Точка взгляда ниже центра фигуры поднимает её над серединой кадра.
        self.height = if portrait { -visible * 0.06 } else { 0.0 };
    }

    pub fn zoom(&mut self, delta: f32) {
        if delta == 0.0 {
            return;
        }
        self.manual_zoom = true;
        self.distance = (self.distance - delta).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    pub fn reset(&mut self) {
        self.yaw = 0.0;
        self.pitch = 0.10;
        self.auto_sway = true;
        self.manual_zoom = false;
    }

    fn total_yaw(&self) -> f32 {
        if self.auto_sway {
            let phase = self.time / SWAY_PERIOD * std::f32::consts::TAU;
            self.yaw + phase.sin() * SWAY_AMPLITUDE
        } else {
            self.yaw
        }
    }

    /// Точка на полу под персонажем — по ней 2D-слой рисует пятно света.
    pub fn ground_focus(&self) -> Vec3 {
        vec3(self.focus.x, 0.0, self.focus.z)
    }

    pub fn camera(&self, fov_y: f32) -> Camera {
        let yaw = self.total_yaw();
        let (sy, cy) = yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();

        let offset = vec3(sy * cp, sp, cy * cp) * self.distance;
        let target = self.focus + vec3(0.0, self.height, 0.0);

        Camera {
            eye: target + offset,
            target,
            fov_y,
            near: 0.1,
            far: 50.0,
            shift: crate::engine::math::Vec2::ZERO,
        }
    }
}
