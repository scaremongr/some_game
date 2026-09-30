//! Сцена танца: персонаж, доли такта и переключение движений.
//!
//! Персонаж берётся из `assets/character.glb`, если файл есть; пока он
//! грузится или если его нет — работает процедурная заглушка. Так сцена
//! запускается всегда, а подкладывание модели ничего не ломает.
//!
//! Темп пока задан константой, а не разобран из музыки. Это осознанно:
//! сетка долей всё равно должна быть известна заранее, а откуда она
//! возьмётся — из константы или из карты трека — на анимацию не влияет.


use crate::engine::app::{FrameCtx, Scene, Transition};
use crate::engine::audio::Music;
use crate::engine::beat::BeatMap;
use crate::engine::graphics::Graphics;
use crate::engine::math::*;
use crate::engine::render3d::Lighting;
use crate::engine::ui;

use super::camera::StageCamera;
use super::dancer::{load_bytes, Dancer, PendingBytes, FADE, TARGET_HEIGHT};
use super::palette;
use super::rhythm::{CueState, Grade, Rhythm, LOOKAHEAD, MAX_LANES, MISS_WINDOW};

use miniquad::KeyCode;

/// Темп, пока нет разобранного трека: сцена должна работать и без музыки.
const FALLBACK_BPM: f32 = 100.0;
/// Где лежит музыка и её карта долей.
const MUSIC_PATH: &str = "assets/music/track.wav";
const BEATMAP_PATH: &str = "assets/music/track.beatmap";
/// Шаг ручной калибровки задержки вывода.
const LATENCY_STEP: f32 = 0.005;
/// Радиан на пиксель виртуального холста при перетаскивании камеры.
const ORBIT_SENSITIVITY: f32 = 0.012;
/// Поле зрения камеры.
const FOV_Y: f32 = 40.0;
/// Высота экранной кнопки движения на телефоне. Меньше 44 логических точек
/// палец уже не попадает — это общая рекомендация и для iOS, и для Android.
const TOUCH_BUTTON: f32 = 52.0;

/// Раскладка сцены под текущий холст.
///
/// Считается каждый кадр, потому что экран телефона поворачивается, а окно
/// на PC тянут мышью. Всё, что раньше было жёсткими координатами под 640x360,
/// теперь живёт здесь.
struct Layout {
    canvas: Vec2,
    portrait: bool,
    /// Полоса экранных кнопок снизу; на PC её нет.
    pad_top: f32,
    /// Дорожка подсказок.
    lane_y: f32,
    hit_x: f32,
    lane_span: f32,
}

impl Layout {
    fn new(canvas: Vec2, buttons: usize) -> Layout {
        let portrait = canvas.y > canvas.x;

        // Кнопок ровно один ряд: каждый лишний ряд отъедает низ экрана
        // у сцены, а на телефоне её и так немного.
        let pad_height = if portrait && buttons > 0 {
            TOUCH_BUTTON
        } else {
            0.0
        };
        let pad_top = canvas.y - 30.0 - pad_height;

        let lane_y = pad_top - 34.0;
        let hit_x = if portrait { 46.0 } else { 110.0 };

        Layout {
            canvas,
            portrait,
            pad_top,
            lane_y,
            hit_x,
            lane_span: canvas.x - hit_x - 16.0,
        }
    }

    /// Прямоугольник экранной кнопки движения.
    fn move_button(&self, index: usize, total: usize) -> Rect {
        let total = total.max(1);
        let gap = 6.0;
        let width = (self.canvas.x - 16.0 - gap * (total as f32 - 1.0)) / total as f32;
        rect(
            8.0 + index as f32 * (width + gap),
            self.pad_top,
            width,
            TOUCH_BUTTON,
        )
    }
}
/// Плотность плоской тени на полу.
const SHADOW_OPACITY: f32 = 0.34;

pub struct DanceScene {
    dancer: Dancer,

    camera: StageCamera,
    light: Lighting,

    music: Music,
    pending_music: PendingBytes,
    pending_beatmap: PendingBytes,
    /// Разобранная сетка долей. Пока её нет, сцена идёт по FALLBACK_BPM.
    beatmap: Option<BeatMap>,
    rhythm: Rhythm,
    /// Трек готов, но ждёт первого нажатия. Браузеры (и WebView Telegram)
    /// не дают запустить звук без жеста пользователя, поэтому автостарт
    /// невозможен в принципе.
    waiting_for_start: bool,

    /// Время трека в секундах. От аудио, когда музыка играет, иначе от
    /// игрового цикла — сцена обязана работать и без звука.
    time: f32,
    last_beat: i32,
    beat_flash: f32,
    /// Сообщение о загрузке, гаснет через несколько секунд.
    notice: Option<(String, f32)>,

    /// Искры от удачного попадания и общая вспышка сцены.
    sparks: Vec<Spark>,
    hit_pulse: f32,
}

/// Частица награды за попадание. Живёт секунду, летит и гаснет.
struct Spark {
    pos: Vec2,
    velocity: Vec2,
    life: f32,
    size: f32,
    color: Color,
}

impl DanceScene {
    pub fn new() -> DanceScene {
        let seconds_per_beat = 60.0 / FALLBACK_BPM;

        // Музыка и её карта долей грузятся асинхронно; без них сцена
        // работает по запасному темпу.
        let pending_music = load_bytes(MUSIC_PATH);
        let pending_beatmap = load_bytes(BEATMAP_PATH);

        DanceScene {
            camera: StageCamera::new(TARGET_HEIGHT),
            dancer: Dancer::new(seconds_per_beat),
            light: Lighting::default(),
            music: Music::new(),
            pending_music,
            pending_beatmap,
            beatmap: None,
            rhythm: Rhythm::new(),
            waiting_for_start: false,
            time: 0.0,
            last_beat: -1,
            beat_flash: 0.0,
            notice: None,
            sparks: Vec::new(),
            hit_pulse: 0.0,
        }
    }

    /// Отдача за попадание: чем точнее, тем ярче.
    ///
    /// Ритм-игра без такой отдачи ощущается как таблица с числами — сигнал
    /// «попал» должен приходить раньше, чем игрок прочитает счёт.
    fn celebrate(&mut self, grade: Grade, layout: &Layout) {
        let (count, speed, color) = match grade {
            Grade::Perfect => (18, 190.0, palette::PLAYER),
            Grade::Good => (9, 130.0, palette::ACCENT),
            Grade::Miss => return,
        };

        self.hit_pulse = match grade {
            Grade::Perfect => 1.0,
            _ => 0.6,
        };

        let origin = vec2(layout.hit_x, layout.lane_y);
        for i in 0..count {
            // Разлёт веером вверх: вниз частицы уходили бы под кнопки.
            let angle = -std::f32::consts::PI * (0.15 + 0.7 * i as f32 / count as f32);
            let spread = 0.55 + (i % 5) as f32 * 0.18;
            self.sparks.push(Spark {
                pos: origin,
                velocity: vec2(angle.cos(), angle.sin()) * (speed * spread),
                life: 1.0,
                size: 3.0 + (i % 3) as f32 * 2.0,
                color,
            });
        }
    }

    fn update_effects(&mut self, dt: f32) {
        self.hit_pulse = (self.hit_pulse - dt * 2.2).max(0.0);

        for spark in self.sparks.iter_mut() {
            spark.pos += spark.velocity * dt;
            // Лёгкая гравитация: без неё веер выглядит механическим.
            spark.velocity.y += 320.0 * dt;
            spark.life -= dt * 1.6;
        }
        self.sparks.retain(|s| s.life > 0.0);
    }

    fn draw_sparks(&self, g: &mut Graphics) {
        for spark in &self.sparks {
            let size = spark.size * (0.4 + spark.life);
            g.blob(
                rect(
                    spark.pos.x - size,
                    spark.pos.y - size,
                    size * 2.0,
                    size * 2.0,
                ),
                spark.color.with_alpha(spark.life.min(1.0) * 0.9),
            );
        }
    }

    fn seconds_per_beat(&self) -> f32 {
        match &self.beatmap {
            Some(map) => map.seconds_per_beat(),
            None => 60.0 / FALLBACK_BPM,
        }
    }

    fn bpm(&self) -> f32 {
        match &self.beatmap {
            Some(map) => map.bpm,
            None => FALLBACK_BPM,
        }
    }

    fn beat_position(&self) -> f32 {
        match &self.beatmap {
            Some(map) => map.beat_position(self.time),
            None => self.time / self.seconds_per_beat(),
        }
    }

    /// Клавиша экранной кнопки, по которой только что тапнули.
    ///
    /// Считается по отпусканию без перетаскивания: иначе вращение камеры,
    /// начатое с кнопки, засчиталось бы как нажатие.
    fn pad_key_at(&self, ctx: &FrameCtx, layout: &Layout) -> Option<usize> {
        if !layout.portrait || !ctx.input.tapped() {
            return None;
        }
        let total = self.dancer.character.moves.len().min(MAX_LANES);
        (0..total).find(|i| layout.move_button(*i, total).contains(ctx.input.pointer))
            .map(|i| i + 1)
    }

    /// Слежение и ручное управление камерой.
    fn update_camera(&mut self, ctx: &FrameCtx, pad_used: bool) {
        self.camera
            .fit(TARGET_HEIGHT, ctx.screen, FOV_Y.to_radians());
        self.camera.follow(self.dancer.character.focus_point(), ctx.dt);

        // Перетаскивание вращает. Порог отделяет его от тапа, которым
        // переключается движение.
        if ctx.input.is_dragging() && !pad_used {
            let d = ctx.input.pointer_delta;
            self.camera.orbit(-d.x * ORBIT_SENSITIVITY, d.y * ORBIT_SENSITIVITY);
        }

        // Клавиши — для тех, у кого мышь занята игрой.
        let keyboard_yaw = ctx.input.is_down(KeyCode::E) as i32 as f32
            - ctx.input.is_down(KeyCode::Q) as i32 as f32;
        if keyboard_yaw != 0.0 {
            self.camera.orbit(keyboard_yaw * ctx.dt * 1.4, 0.0);
        }

        if ctx.input.wheel != 0.0 {
            self.camera.zoom(ctx.input.wheel * 0.0016 * self.camera.distance);
        }
        if ctx.input.just_pressed(KeyCode::R) {
            self.camera.reset();
        }
        if ctx.input.just_pressed(KeyCode::O) {
            self.camera.auto_sway = !self.camera.auto_sway;
        }
    }

    /// Запускает трек по первому нажатию.
    ///
    /// Судить попадания без карты долей нельзя, поэтому старт ждёт и её.
    fn try_start(&mut self, ctx: &FrameCtx, pad_used: bool) {
        if !self.waiting_for_start || self.beatmap.is_none() {
            return;
        }
        let pressed = ctx.input.tapped()
            || pad_used
            || ui::confirm_pressed(ctx.input)
            || ctx.input.just_pressed(KeyCode::Key1);
        if !pressed {
            return;
        }
        self.waiting_for_start = false;
        self.rhythm.reset();
        self.time = 0.0;
        self.music.play(ctx.now);
    }

    /// Забирает музыку и карту долей, когда они догрузились.
    fn take_pending_audio(&mut self) {
        if let Some(result) = self.pending_beatmap.borrow_mut().take() {
            match result.and_then(|b| {
                BeatMap::from_text(&String::from_utf8_lossy(&b))
            }) {
                Ok(map) => {
                    self.notice = Some((
                        format!("BEATMAP: {:.1} BPM", map.bpm),
                        3.0,
                    ));
                    self.beatmap = Some(map);
                    self.rhythm.reset();
                }
                Err(e) => {
                    self.notice = Some((format!("NO BEATMAP: {}", e.to_ascii_uppercase()), 5.0))
                }
            }
        }

        if let Some(result) = self.pending_music.borrow_mut().take() {
            match result.and_then(|bytes| self.music.load(&bytes)) {
                Ok(()) => self.waiting_for_start = true,
                Err(e) => {
                    self.notice = Some((format!("NO MUSIC: {}", e.to_ascii_uppercase()), 8.0))
                }
            }
        }
    }


    /// Запрашивает файлы движений. Делается один раз и только когда скелет


    fn draw_hud(&self, g: &mut Graphics, layout: &Layout) {
        let canvas = layout.canvas;

        // На телефоне подпись сцены съедает место, которого и так нет.
        if !layout.portrait {
            g.text("DANCE FLOOR", vec2(12.0, 12.0), 2.0, palette::TITLE);
        }

        // В портрете ширины 360 не хватает, чтобы развести инфо и счёт по
        // краям одной строки, — они наезжают друг на друга. Поэтому счёт
        // занимает верхнюю строку целиком, а инфо уходит под неё.
        let info_y = if layout.portrait { 26.0 } else { 32.0 };
        g.text(
            &format!(
                "{:.0} BPM  {}  {}",
                self.bpm(),
                self.dancer.character.current_move_name(),
                self.dancer.character.source,
            ),
            vec2(12.0, info_y),
            1.0,
            palette::MUTED,
        );

        // Предупреждения разбора видны сразу: иначе на «почему-то не так»
        // уходит гораздо больше времени, чем на их чтение.
        for (i, w) in self.dancer.character.warnings.iter().take(3).enumerate() {
            g.text(
                &w.to_ascii_uppercase(),
                vec2(12.0, info_y + 14.0 + i as f32 * 10.0),
                1.0,
                palette::PLAYER.with_alpha(0.8),
            );
        }

        if let Some((text, ttl)) = &self.notice {
            g.text_centered(
                text,
                vec2(canvas.x * 0.5, layout.lane_y - 42.0),
                1.0,
                palette::ACCENT.with_alpha(ttl.min(1.0)),
            );
        }

        if self.waiting_for_start {
            g.text_centered(
                "TAP TO START",
                vec2(canvas.x * 0.5, canvas.y * 0.5),
                if layout.portrait { 2.0 } else { 3.0 },
                palette::TITLE,
            );
            g.text_centered(
                "SOUND NEEDS A TAP FIRST",
                vec2(canvas.x * 0.5, canvas.y * 0.5 + 22.0),
                1.0,
                palette::MUTED,
            );
        } else if self.beatmap.is_some() {
            self.draw_cue_lane(g, layout);
            self.draw_score(g, layout);
        } else {
            self.draw_beat_bar(g, layout);
            g.text_centered(
                "NO TRACK - SEE assets/music/README.md",
                vec2(canvas.x * 0.5, layout.lane_y),
                1.0,
                palette::MUTED,
            );
        }

        self.draw_sparks(g);
        self.draw_move_pad(g, layout);

        if !layout.portrait {
            g.text_centered(
                "1-9 - HIT THE CUE   DRAG / Q E - CAMERA   R - RESET   ESC - MENU",
                vec2(canvas.x * 0.5, canvas.y - 12.0),
                1.0,
                palette::MUTED,
            );
        }
    }

    /// Экранные кнопки движений.
    ///
    /// Без них игра на телефоне неиграбельна: вся механика завязана на
    /// клавиши 1-9, а их там нет. На PC ряд не рисуется — клавиатура удобнее.
    fn draw_move_pad(&self, g: &mut Graphics, layout: &Layout) {
        if !layout.portrait {
            return;
        }
        let total = self.dancer.character.moves.len().min(MAX_LANES);
        if total == 0 {
            return;
        }

        // Какая кнопка сейчас нужна по подсказке — её и подсвечиваем.
        let wanted = self.nearest_cue_key();

        for index in 0..total {
            let r = layout.move_button(index, total);
            let key = index + 1;
            let active = wanted == Some(key);
            let current =
                self.dancer.character.player.current_clip() == self.dancer.character.moves.get(index).copied();

            let fill = if active {
                palette::ACCENT.with_alpha(0.45 + self.beat_flash * 0.3)
            } else if current {
                palette::SPOTLIGHT.with_alpha(0.22)
            } else {
                palette::STAGE_TOP.with_alpha(0.75)
            };
            g.rect(r, fill);
            g.rect_outline(
                r,
                1.0,
                if active { palette::TITLE } else { palette::GRID_MAJOR },
            );
            g.text_centered(
                &key.to_string(),
                vec2(r.center().x, r.center().y - 6.0),
                2.0,
                if active { palette::TITLE } else { palette::MUTED },
            );

            // Подпись движения под цифрой: иначе не понять, что жмёшь.
            if let Some(clip) = self
                .dancer
                .character
                .moves
                .get(index)
                .and_then(|c| self.dancer.character.clips.get(*c))
            {
                let name: String = clip.name.chars().take(9).collect();
                g.text_centered(
                    &name,
                    vec2(r.center().x, r.center().y + 12.0),
                    1.0,
                    // На подсвеченной кнопке приглушённая подпись пропадает.
                    if active {
                        palette::TITLE
                    } else {
                        palette::MUTED.with_alpha(0.85)
                    },
                );
            }
        }
    }

    /// Клавиша ближайшей несыгранной подсказки — для подсветки кнопки.
    fn nearest_cue_key(&self) -> Option<usize> {
        self.rhythm
            .cues
            .iter()
            .filter(|c| c.state == CueState::Pending)
            .filter(|c| (c.time - self.time).abs() <= MISS_WINDOW * 2.5)
            .min_by(|a, b| {
                (a.time - self.time)
                    .abs()
                    .total_cmp(&(b.time - self.time).abs())
            })
            .map(|c| c.key)
    }

    /// Дорожка подсказок: время до доли превращается в расстояние до
    /// неподвижной линии суда.
    fn draw_cue_lane(&self, g: &mut Graphics, layout: &Layout) {
        let lane_y = layout.lane_y;
        let hit_x = layout.hit_x;

        g.rect(
            rect(hit_x, lane_y - 20.0, layout.lane_span, 40.0),
            palette::STAGE_TOP.with_alpha(0.55),
        );

        // Линия суда разгорается на попадании — это и есть «сюда надо было».
        let glow = 22.0 + self.hit_pulse * 26.0;
        g.blob(
            rect(hit_x - glow, lane_y - glow, glow * 2.0, glow * 2.0),
            palette::PLAYER.with_alpha(self.hit_pulse * 0.55),
        );
        g.rect(
            rect(hit_x - 2.0, lane_y - 24.0, 4.0, 48.0),
            palette::ACCENT.with_alpha(0.5 + self.beat_flash * 0.5 + self.hit_pulse * 0.5),
        );

        for cue in &self.rhythm.cues {
            let ahead = cue.time - self.time;
            if ahead > LOOKAHEAD || ahead < -MISS_WINDOW * 2.0 {
                continue;
            }
            let x = hit_x + (ahead / LOOKAHEAD) * layout.lane_span;

            // Приближаясь к линии суда, подсказка подрастает: так глаз
            // ловит момент, не переводя взгляд на счётчик долей.
            let closeness = 1.0 - (ahead / LOOKAHEAD).clamp(0.0, 1.0);
            let (color, base) = match cue.state {
                CueState::Pending => (palette::TITLE, 20.0 + closeness * 6.0),
                CueState::Done(Grade::Perfect) => (palette::PLAYER, 30.0),
                CueState::Done(Grade::Good) => (palette::ACCENT, 26.0),
                CueState::Done(Grade::Miss) => (palette::MUTED, 16.0),
            };
            let size = base;
            let alpha = match cue.state {
                CueState::Pending => 1.0,
                _ => 0.45,
            };

            let box_rect = rect(x - size * 0.5, lane_y - size * 0.5, size, size);
            g.rect(box_rect, palette::STAGE_BOTTOM.with_alpha(alpha * 0.8));
            g.rect_outline(box_rect, 2.0, color.with_alpha(alpha));
            g.text_centered(
                &cue.key.to_string(),
                vec2(x, lane_y),
                2.0,
                color.with_alpha(alpha),
            );
        }
    }

    fn draw_score(&self, g: &mut Graphics, layout: &Layout) {
        let canvas = layout.canvas;
        let right = canvas.x - 12.0;

        let score = format!("{}", self.rhythm.score);
        g.text(
            &score,
            vec2(right - g.text_width(&score, 2.0), 8.0),
            2.0,
            palette::TITLE,
        );

        let stats = format!(
            "x{}  {:.0}%  LAT {:+.0}",
            self.rhythm.combo,
            self.rhythm.accuracy() * 100.0,
            self.music.latency * 1000.0,
        );
        // В портрете статистика уходит влево на ту же строку, что и счёт:
        // правый край занят им, а второй строки под неё нет.
        let stats_pos = if layout.portrait {
            vec2(12.0, 10.0)
        } else {
            vec2(right - g.text_width(&stats, 1.0), 30.0)
        };
        g.text(&stats, stats_pos, 1.0, palette::MUTED);

        // Оценка последнего нажатия и знак ошибки: по нему игрок понимает,
        // спешит он или опаздывает, и куда крутить калибровку.
        if let Some((grade, error, ttl)) = self.rhythm.last {
            let color = match grade {
                Grade::Perfect => palette::PLAYER,
                Grade::Good => palette::ACCENT,
                Grade::Miss => palette::MUTED,
            };
            // Текст оценки выпрыгивает и оседает: движение читается
            // боковым зрением, а ровный текст — нет.
            let pop = 2.0 + self.hit_pulse * 1.6;
            g.text_centered(
                grade.label(),
                vec2(canvas.x * 0.5, layout.lane_y - 52.0 - self.hit_pulse * 10.0),
                pop,
                color.with_alpha(ttl.min(1.0)),
            );
            if grade != Grade::Miss {
                g.text_centered(
                    if error < 0.0 { "EARLY" } else { "LATE" },
                    vec2(canvas.x * 0.5, layout.lane_y - 30.0),
                    1.0,
                    color.with_alpha(ttl.min(1.0) * 0.7),
                );
            }
        }
    }

    /// Запасная полоса долей, когда трека нет.
    fn draw_beat_bar(&self, g: &mut Graphics, layout: &Layout) {
        let width = (layout.canvas.x - 40.0).min(212.0);
        let cell_w = (width - 24.0) / 4.0;
        let bar_x = (layout.canvas.x - width) * 0.5;
        let sub = self.beat_position().rem_euclid(4.0);
        for i in 0..4 {
            let cell = rect(
                bar_x + i as f32 * (cell_w + 8.0),
                layout.lane_y + 16.0,
                cell_w,
                8.0,
            );
            g.rect(
                cell,
                if i == sub.floor() as i32 {
                    palette::ACCENT.with_alpha(0.35 + self.beat_flash * 0.65)
                } else {
                    palette::GRID_MAJOR
                },
            );
        }
    }
}

impl Scene for DanceScene {
    fn update(&mut self, ctx: &FrameCtx) -> Transition {
        if ui::back_pressed(ctx.input) {
            return Transition::Pop;
        }

        self.dancer.update(ctx.dt);
        if let Some(notice) = self.dancer.notice.take() {
            self.notice = Some(notice);
        }
        self.take_pending_audio();

        let layout = Layout::new(ctx.screen, self.dancer.character.moves.len().min(MAX_LANES));

        // Тап по экранной кнопке — это то же самое, что нажатие цифры.
        // Разобрать его надо до всего остального: иначе он же сойдёт за
        // «тап по сцене» и переключит движение мимо судейства.
        let pad_key = self.pad_key_at(ctx, &layout);
        let pad_used = pad_key.is_some();

        self.try_start(ctx, pad_used);

        // Время трека — от звука, если он играет. Игровой цикл тут только
        // запасной вариант: судить попадания по нему нельзя.
        self.time = if self.music.is_playing() {
            self.music.position(ctx.now)
        } else if self.waiting_for_start {
            0.0
        } else {
            self.time + ctx.dt
        };

        let beat = self.beat_position().floor() as i32;
        if beat != self.last_beat {
            self.last_beat = beat;
            self.beat_flash = 1.0;
        }
        self.beat_flash = (self.beat_flash - ctx.dt * 4.0).max(0.0);
        if let Some((_, ttl)) = &mut self.notice {
            *ttl -= ctx.dt;
            if *ttl <= 0.0 {
                self.notice = None;
            }
        }

        // Калибровка задержки вывода: минус/плюс сдвигают время трека
        // относительно картинки. Без неё ритм-игра ощущается сломанной
        // на любом устройстве, потому что вывод звука везде запаздывает
        // по-своему.
        if ctx.input.just_pressed(KeyCode::Minus) {
            self.music.latency -= LATENCY_STEP;
        }
        if ctx.input.just_pressed(KeyCode::Equal) {
            self.music.latency += LATENCY_STEP;
        }

        if let Some(map) = &self.beatmap {
            self.rhythm
                .update(map, self.time, ctx.dt, self.dancer.character.moves.len());
        }

        self.update_camera(ctx, pad_used);

        // Клавиш ровно столько же, сколько кнопок на экране: иначе можно
        // нажать цифру, которой нет ни в одной подсказке.
        const DIGITS: [KeyCode; MAX_LANES] = [
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
        ];
        let pressed_key = pad_key.or_else(|| {
            DIGITS
                .iter()
                .position(|k| ctx.input.just_pressed(*k))
                .map(|i| i + 1)
        });

        let mut requested = None;
        if let Some(key) = pressed_key {
            // Сначала судейство: оно решает, какое движение включить.
            let judged = if self.beatmap.is_some() {
                self.rhythm.press(key, self.time)
            } else {
                None
            };
            requested = judged
                .or(Some(key - 1))
                .and_then(|i| self.dancer.character.moves.get(i).copied());
        }

        if requested.is_none()
            && !pad_used
            && (ui::confirm_pressed(ctx.input) || ctx.input.tapped())
        {
            requested = self.dancer.character.next_move();
        }

        if let Some(clip) = requested {
            self.dancer.character.player.play(clip, FADE, 1.0);
        }

        if let Some((grade, _, ttl)) = self.rhythm.last {
            if ttl > 0.99 {
                self.celebrate(grade, &layout);
            }
        }
        self.update_effects(ctx.dt);

        self.dancer.character.update(self.time, ctx.dt);

        Transition::None
    }

    fn draw(&mut self, g: &mut Graphics, _alpha: f32) {
        self.dancer.ensure_mesh(g);

        let canvas = g.canvas();
        let layout = Layout::new(canvas, self.dancer.character.moves.len().min(MAX_LANES));
        let camera = self.camera.camera(FOV_Y.to_radians());

        draw_stage(g, canvas, camera.horizon_y(canvas), self.beat_flash);

        // Пятно света кладётся до персонажа: 2D-слой не пишет в буфер
        // глубины, поэтому персонаж всё равно окажется поверх. Точка берётся
        // под фигурой, а не в начале координат — она же двигается.
        let ground = camera.project(self.camera.ground_focus(), canvas);
        let pool = canvas.x * 0.47;
        g.blob(
            rect(
                ground.x - pool * 0.5,
                ground.y - pool * 0.19,
                pool,
                pool * 0.38,
            ),
            palette::SPOTLIGHT.with_alpha(0.42 + self.beat_flash * 0.12),
        );

        if let Some(mesh) = &self.dancer.character.mesh {
            g.draw_skinned(
                mesh,
                &self.dancer.character.skin,
                self.dancer.character.transform,
                &camera,
                &self.light,
                SHADOW_OPACITY,
            );
        }

        self.draw_hud(g, &layout);
    }
}

/// Фон сцены: задник, пол и пульсирующий круг света за фигурой.
///
/// `horizon` приходит из камеры, а не из константы — только так 2D-пол
/// сходится ровно туда, куда уходит 3D-земля.
fn draw_stage(g: &mut Graphics, canvas: Vec2, horizon: f32, beat_flash: f32) {
    let horizon = horizon.clamp(0.0, canvas.y);

    // Градиенты набираются полосами: отдельного шейдера для фона пока нет,
    // а полсотни квадов дешевле, чем второй конвейер.
    gradient_band(g, canvas, 0.0, horizon, palette::STAGE_TOP, palette::STAGE_BOTTOM);
    gradient_band(g, canvas, horizon, canvas.y, palette::FLOOR_FAR, palette::FLOOR_NEAR);

    let glow = 300.0 + beat_flash * 34.0;
    g.blob(
        rect(
            canvas.x * 0.5 - glow * 0.5,
            horizon - glow * 0.72,
            glow,
            glow,
        ),
        palette::ACCENT.with_alpha(0.09 + beat_flash * 0.09),
    );
}

fn gradient_band(g: &mut Graphics, canvas: Vec2, top: f32, bottom: f32, from: Color, to: Color) {
    let height = bottom - top;
    if height <= 0.0 {
        return;
    }
    let bands = 24;
    let step = height / bands as f32;
    for i in 0..bands {
        let t = i as f32 / (bands - 1).max(1) as f32;
        g.rect(
            rect(0.0, top + i as f32 * step, canvas.x, step + 1.0),
            from.lerp(to, t),
        );
    }
}
