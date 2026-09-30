//! Режим «Clips»: цельный танец под трек целиком.
//!
//! Не игра — здесь ничего не нажимают. Задача одна: проиграть длинную запись
//! танца ровно под музыку и не разъехаться с ней за три-пять минут.
//!
//! # Как держится синхронизация
//!
//! Время анимации не накапливается по кадрам, а каждый кадр берётся заново от
//! часов аудио:
//!
//! ```text
//! время анимации = (позиция в треке - сдвиг) * скорость
//! ```
//!
//! Это принципиально. Прибавляй мы `dt` кадр за кадром — за пять минут набежал
//! бы разъезд на десятые доли секунды от одних только пропущенных кадров, и
//! чем слабее устройство, тем сильнее. От часов аудио ошибка не копится вовсе:
//! каждый кадр — это заново вычисленное абсолютное время.
//!
//! Два числа, которые надо подобрать, живут в манифесте рядом с файлами:
//!
//! * `сдвиг` — в какой секунде трека находится первый кадр анимации;
//! * `скорость` — поправка, если запись сделана в другом темпе, чем трек.
//!
//! Подбираются один раз и вручную: автоматически их не вывести — надо видеть,
//! попадает движение в долю или нет. Поэтому оба правятся прямо на ходу
//! клавишами, а текущие значения показаны на экране, чтобы вписать их в
//! манифест и больше к этому не возвращаться.

use crate::engine::app::{FrameCtx, Scene, Transition};
use crate::engine::audio::Music;
use crate::engine::gltf;
use crate::engine::graphics::Graphics;
use crate::engine::math::*;
use crate::engine::render3d::Lighting;
use crate::engine::ui;

use super::camera::StageCamera;
use super::dancer::{load_bytes, Dancer, PendingBytes, TARGET_HEIGHT};
use super::palette;

use miniquad::KeyCode;

const MANIFEST_PATH: &str = "assets/clips.txt";
const FOV_Y: f32 = 40.0;
const SHADOW_OPACITY: f32 = 0.35;

/// Шаг подстройки сдвига, секунды. Меньше уже не различить на слух.
const OFFSET_STEP: f32 = 0.02;
/// Шаг подстройки скорости. 0.002 — это примерно четверть доли за три минуты.
const SPEED_STEP: f32 = 0.002;

/// Строка манифеста: что играть и чем это выравнивать.
struct Entry {
    name: String,
    animation: String,
    music: String,
    /// В какой секунде трека находится первый кадр анимации.
    offset: f32,
    /// Поправка темпа записи относительно трека.
    speed: f32,
}

/// Разбирает манифест. Строка: `название | анимация | музыка | сдвиг | скорость`.
///
/// Сдвиг и скорость необязательны — без них 0 и 1. Кривые строки молча
/// пропускаются: манифест правят руками, и одна опечатка не должна уносить
/// с собой остальные клипы.
fn parse_manifest(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if parts.len() < 3 || parts[1].is_empty() || parts[2].is_empty() {
            continue;
        }
        out.push(Entry {
            name: if parts[0].is_empty() {
                parts[1].to_string()
            } else {
                parts[0].to_string()
            },
            animation: parts[1].to_string(),
            music: parts[2].to_string(),
            offset: parts.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0),
            speed: parts
                .get(4)
                .and_then(|v| v.parse().ok())
                .filter(|s: &f32| *s > 0.05)
                .unwrap_or(1.0),
        });
    }
    out
}

/// Что уже приехало по сети для выбранного клипа.
#[derive(Default)]
struct Loading {
    animation: Option<PendingBytes>,
    music: Option<PendingBytes>,
    animation_done: bool,
    music_done: bool,
}

pub struct ClipsScene {
    dancer: Dancer,
    stage: StageCamera,
    light: Lighting,
    music: Music,

    manifest: PendingBytes,
    entries: Vec<Entry>,
    /// Какой клип выбран; `None`, пока манифест не разобран.
    selected: Option<usize>,
    loading: Loading,
    /// Индекс проигрываемого клипа в списке анимаций персонажа.
    clip: Option<usize>,

    offset: f32,
    speed: f32,
    playing: bool,
    /// Позиция в треке, секунды.
    time: f32,
    clock: f32,
    notice: Option<(String, f32)>,
    /// Причина, по которой клип не играет. В отличие от сообщений о загрузке
    /// не гаснет: если не показать её постоянно, человек увидит вечное
    /// «LOADING...» и не узнает, что не так с файлом.
    problem: Option<String>,
}

impl ClipsScene {
    pub fn new() -> ClipsScene {
        ClipsScene {
            // Движения из moves.txt тут не нужны: анимация своя.
            dancer: Dancer::solo(0.5),
            stage: StageCamera::new(TARGET_HEIGHT),
            light: Lighting::default(),
            music: Music::new(),
            manifest: load_bytes(MANIFEST_PATH),
            entries: Vec::new(),
            selected: None,
            loading: Loading::default(),
            clip: None,
            offset: 0.0,
            speed: 1.0,
            playing: false,
            time: 0.0,
            clock: 0.0,
            notice: None,
            problem: None,
        }
    }

    fn entry(&self) -> Option<&Entry> {
        self.selected.and_then(|i| self.entries.get(i))
    }

    /// Длительность выбранной анимации, секунды.
    fn duration(&self) -> f32 {
        self.clip
            .and_then(|i| self.dancer.character.clips.get(i))
            .map(|c| c.duration)
            .unwrap_or(0.0)
    }

    fn say(&mut self, text: String, seconds: f32) {
        self.notice = Some((text, seconds));
    }

    fn fail(&mut self, text: String) {
        self.problem = Some(text);
    }

    fn take_manifest(&mut self) {
        let Some(result) = self.manifest.borrow_mut().take() else {
            return;
        };
        match result {
            Ok(bytes) => {
                self.entries = parse_manifest(&String::from_utf8_lossy(&bytes));
                if self.entries.is_empty() {
                    self.fail("NO CLIPS LISTED IN ASSETS/CLIPS.TXT".into());
                } else {
                    self.select(0);
                }
            }
            Err(e) => self.fail(format!("NO CLIPS.TXT: {}", e.to_ascii_uppercase())),
        }
    }

    /// Начинает загрузку клипа: анимация и музыка тянутся параллельно.
    fn select(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        self.selected = Some(index);
        self.offset = entry.offset;
        self.speed = entry.speed;
        self.playing = false;
        self.time = 0.0;
        self.clip = None;
        self.problem = None;
        self.music.stop();

        self.loading = Loading {
            animation: Some(load_bytes(&format!("assets/{}", entry.animation))),
            music: Some(load_bytes(&format!("assets/{}", entry.music))),
            animation_done: false,
            music_done: false,
        };
        let name = entry.name.clone();
        self.say(format!("LOADING {}", name.to_ascii_uppercase()), 4.0);
    }

    fn take_clip(&mut self) {
        // Анимацию можно накладывать только на настоящий скелет: у заглушки
        // кости другие, и имена не совпадут.
        if self.dancer.character.mesh_is_placeholder() {
            return;
        }

        if !self.loading.animation_done {
            let arrived = self
                .loading
                .animation
                .as_ref()
                .and_then(|slot| slot.borrow_mut().take());
            if let Some(result) = arrived {
                self.loading.animation_done = true;
                self.apply_animation(result);
            }
        }

        if !self.loading.music_done {
            let arrived = self
                .loading
                .music
                .as_ref()
                .and_then(|slot| slot.borrow_mut().take());
            if let Some(result) = arrived {
                self.loading.music_done = true;
                match result.and_then(|bytes| self.music.load(&bytes)) {
                    Ok(()) => {}
                    Err(e) => self.fail(format!("NO MUSIC: {}", e.to_ascii_uppercase())),
                }
            }
        }
    }

    fn apply_animation(&mut self, result: Result<Vec<u8>, String>) {
        let name = self
            .entry()
            .map(|e| e.name.to_ascii_uppercase())
            .unwrap_or_default();

        let parsed = result
            .and_then(|bytes| gltf::load_clips(&bytes, &self.dancer.character.skeleton));
        match parsed {
            Ok(clips) if !clips.is_empty() => {
                let before = self.dancer.character.clips.len();
                let added = self.dancer.character.add_clips(clips, &name);
                if added == 0 {
                    self.fail("CLIP HAS NO MOVEMENT IN IT".into());
                    return;
                }
                self.clip = Some(before);
                self.dancer.character.player.play(before, 0.0, 1.0);
                let seconds = self.duration();
                self.say(format!("{name}: {:.0}:{:02.0}", seconds / 60.0, seconds % 60.0), 4.0);
            }
            Ok(_) => self.fail("CLIP HAS NO ANIMATION INSIDE".into()),
            Err(e) => self.fail(format!("BAD CLIP: {}", e.to_ascii_uppercase())),
        }
    }

    fn ready(&self) -> bool {
        self.clip.is_some() && self.music.is_loaded()
    }

    fn start(&mut self, now: f64) {
        if !self.ready() {
            return;
        }
        self.time = 0.0;
        self.playing = true;
        self.music.play(now);
    }

    /// Время внутри анимации для текущей позиции трека.
    fn animation_time(&self) -> f32 {
        let duration = self.duration();
        if duration <= 0.0 {
            return 0.0;
        }
        ((self.time - self.offset) * self.speed).rem_euclid(duration)
    }

    fn handle_tuning(&mut self, input: &crate::engine::input::Input) {
        let mut changed = false;
        if input.any_pressed(&[KeyCode::Left, KeyCode::LeftBracket]) {
            self.offset -= OFFSET_STEP;
            changed = true;
        }
        if input.any_pressed(&[KeyCode::Right, KeyCode::RightBracket]) {
            self.offset += OFFSET_STEP;
            changed = true;
        }
        if input.just_pressed(KeyCode::Minus) {
            self.speed -= SPEED_STEP;
            changed = true;
        }
        if input.just_pressed(KeyCode::Equal) {
            self.speed += SPEED_STEP;
            changed = true;
        }
        self.speed = self.speed.clamp(0.5, 2.0);

        if changed {
            // Строка сразу в том виде, в каком её вписывать в манифест.
            let line = self
                .entry()
                .map(|e| {
                    format!(
                        "{} | {} | {} | {:.2} | {:.3}",
                        e.name, e.animation, e.music, self.offset, self.speed
                    )
                })
                .unwrap_or_default();
            self.say(line.to_ascii_uppercase(), 6.0);
        }
    }

    fn draw_hud(&self, g: &mut Graphics, canvas: Vec2) {
        let title = self.entry().map(|e| e.name.as_str()).unwrap_or("CLIPS");
        g.text(&title.to_ascii_uppercase(), vec2(12.0, 10.0), 2.0, palette::TITLE);

        if let Some((text, _)) = &self.notice {
            g.text(text, vec2(12.0, 30.0), 1.0, palette::ACCENT);
        }

        let duration = self.duration();
        if !self.ready() {
            let center = vec2(canvas.x * 0.5, canvas.y * 0.5);
            match &self.problem {
                Some(text) => {
                    // С переносом: причина бывает длиннее экрана, а прочитать
                    // её надо целиком.
                    let mut y = center.y - 10.0;
                    for line in ui::wrap(text, ui::line_chars(canvas.x)) {
                        g.text_centered(&line, vec2(center.x, y), 1.0, palette::MATCH_BAD);
                        y += 12.0;
                    }
                    g.text_centered(
                        "SEE ASSETS/README.MD",
                        vec2(center.x, y + 6.0),
                        1.0,
                        palette::MUTED,
                    );
                }
                None => g.text_centered("LOADING...", center, 2.0, palette::MUTED),
            }
            return;
        }

        if !self.playing {
            g.text_centered(
                "TAP TO PLAY",
                vec2(canvas.x * 0.5, canvas.y * 0.42),
                3.0,
                palette::TITLE,
            );
        }

        // Полоса времени с подписью. Общая длительность берётся у анимации:
        // сколько играет звук, quad-snd не сообщает.
        let width = (canvas.x - 48.0).min(420.0);
        let bar = rect((canvas.x - width) * 0.5, canvas.y - 34.0, width, 6.0);
        g.rect(bar, palette::STAGE_TOP.with_alpha(0.85));
        let progress = if duration > 0.0 {
            (self.animation_time() / duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        g.rect(rect(bar.x, bar.y, bar.w * progress, bar.h), palette::ACCENT);

        g.text(
            &format!("{}  /  {}", clock(self.animation_time()), clock(duration)),
            vec2(bar.x, bar.y - 12.0),
            1.0,
            palette::MUTED,
        );

        // Настройки синхронизации всегда на виду: их для того и крутят, чтобы
        // потом вписать в манифест.
        let sync = format!("OFFSET {:+.2}S   SPEED {:.3}", self.offset, self.speed);
        g.text(
            &sync,
            vec2(bar.x + bar.w - sync.len() as f32 * 6.0, bar.y - 12.0),
            1.0,
            palette::MUTED,
        );

        g.text_centered(
            "ARROWS - OFFSET   -/= - SPEED   TAP - RESTART   ESC - MENU",
            vec2(canvas.x * 0.5, canvas.y - 14.0),
            1.0,
            palette::MUTED,
        );
    }
}

/// Секунды в `м:сс`.
fn clock(seconds: f32) -> String {
    let seconds = seconds.max(0.0);
    format!("{}:{:02}", (seconds / 60.0) as i32, (seconds % 60.0) as i32)
}

impl Scene for ClipsScene {
    fn update(&mut self, ctx: &FrameCtx) -> Transition {
        if ui::back_pressed(ctx.input) {
            self.music.stop();
            return Transition::Pop;
        }

        self.clock += ctx.dt;
        self.dancer.update(ctx.dt);
        self.take_manifest();
        self.take_clip();

        if let Some((_, ttl)) = &mut self.notice {
            *ttl -= ctx.dt;
            if *ttl <= 0.0 {
                self.notice = None;
            }
        }

        // Экран «коснитесь» нужен только браузеру: тот не запускает звук без
        // жеста. На PC ждать нечего.
        let autostart = !cfg!(target_arch = "wasm32") && !self.playing && self.ready();
        if autostart || ctx.input.tapped() || ui::confirm_pressed(ctx.input) {
            self.start(ctx.now);
        }
        self.handle_tuning(ctx.input);

        // Позиция берётся у аудио, а не накапливается по кадрам: только так
        // танец не уползёт от музыки к концу трека.
        if self.playing && self.music.is_playing() {
            self.time = self.music.position(ctx.now);
        }

        self.stage.fit(TARGET_HEIGHT, ctx.screen, FOV_Y.to_radians());
        self.stage.follow(self.dancer.character.focus_point(), ctx.dt);
        if ctx.input.is_dragging() {
            let d = ctx.input.pointer_delta;
            self.stage.orbit(-d.x * 0.012, d.y * 0.012);
        }
        if ctx.input.just_pressed(KeyCode::R) {
            self.stage.reset();
        }

        self.dancer.character.update(self.animation_time(), ctx.dt);
        Transition::None
    }

    fn draw(&mut self, g: &mut Graphics, _alpha: f32) {
        self.dancer.ensure_mesh(g);

        let canvas = g.canvas();
        let camera = self.stage.camera(FOV_Y.to_radians());

        let horizon = camera.horizon_y(canvas);
        g.rect(rect(0.0, 0.0, canvas.x, horizon), palette::STAGE_TOP);
        g.rect(
            rect(0.0, horizon, canvas.x, canvas.y - horizon),
            palette::FLOOR_FAR.lerp(palette::FLOOR_NEAR, 0.5),
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

        self.draw_hud(g, canvas);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_reads_a_full_line() {
        let entries = parse_manifest("Ночь | clips/night.glb | music/night.ogg | -0.35 | 1.004");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Ночь");
        assert_eq!(entries[0].animation, "clips/night.glb");
        assert_eq!(entries[0].music, "music/night.ogg");
        assert!((entries[0].offset + 0.35).abs() < 1e-6);
        assert!((entries[0].speed - 1.004).abs() < 1e-6);
    }

    /// Выравнивание необязательно: без него клип просто играет с нуля.
    #[test]
    fn tuning_is_optional() {
        let entries = parse_manifest("A | a.glb | a.ogg");
        assert_eq!(entries[0].offset, 0.0);
        assert_eq!(entries[0].speed, 1.0);
    }

    /// Манифест правят руками, и одна кривая строка не должна уносить с собой
    /// остальные клипы.
    #[test]
    fn bad_lines_are_skipped_not_fatal() {
        let entries = parse_manifest(
            "# комментарий\n\
             сломано\n\
             | | \n\
             B | b.glb | b.ogg | вовсе не число | тоже\n\
             C | c.glb | c.ogg | 1.5",
        );
        assert_eq!(entries.len(), 2, "должны остаться B и C");
        assert_eq!(entries[0].name, "B");
        // Нечисловое выравнивание — не повод терять клип.
        assert_eq!(entries[0].offset, 0.0);
        assert_eq!(entries[0].speed, 1.0);
        assert!((entries[1].offset - 1.5).abs() < 1e-6);
    }

    /// Нулевая или отрицательная скорость остановила бы анимацию намертво.
    #[test]
    fn nonsense_speed_falls_back_to_one() {
        let entries = parse_manifest("A | a.glb | a.ogg | 0 | 0");
        assert_eq!(entries[0].speed, 1.0);
    }

    #[test]
    fn clock_formats_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(9.4), "0:09");
        assert_eq!(clock(185.0), "3:05");
    }
}
