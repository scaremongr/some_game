//! Каркас приложения: окно, игровой цикл с фиксированным шагом и стек сцен.

use miniquad::*;

use super::graphics::Graphics;
use super::input::Input;
use super::math::*;

/// Шаг логики. Логика всегда идёт по 60 Гц, независимо от частоты кадров,
/// иначе физика начнёт зависеть от монитора и от тормозов WebView.
pub const TICK: f64 = 1.0 / 60.0;
/// Больше этого за кадр не догоняем — иначе после сворачивания окна
/// игра проматывает минуты логики за один кадр.
const MAX_FRAME_TIME: f64 = 0.25;

/// Что сцена просит сделать со стеком после тика.
pub enum Transition {
    None,
    Push(Box<dyn Scene>),
    Pop,
    Replace(Box<dyn Scene>),
    Quit,
}

pub struct FrameCtx<'a> {
    pub input: &'a Input,
    /// Всегда равен TICK: шаг фиксированный.
    pub dt: f32,
    /// Размер виртуального холста. Меняется при повороте экрана, поэтому
    /// раскладку надо считать от него, а не от констант.
    pub screen: Vec2,
    /// Сглаженный FPS — сценам он нужен только для отладочного HUD.
    pub fps: f32,
    /// Часы приложения на момент этого тика, секунды. По ним считается
    /// позиция в музыке, поэтому важно, что это именно время тика, а не
    /// время кадра: тиков в кадре бывает несколько.
    pub now: f64,
}

pub trait Scene {
    fn update(&mut self, ctx: &FrameCtx) -> Transition;

    /// `alpha` — доля до следующего тика, для интерполяции отрисовки.
    fn draw(&mut self, g: &mut Graphics, alpha: f32);

    /// Непрозрачная сцена закрывает всё, что под ней. Верните false для
    /// оверлеев вроде паузы, чтобы уровень остался виден.
    fn opaque(&self) -> bool {
        true
    }

}

pub struct App {
    g: Graphics,
    input: Input,
    scenes: Vec<Box<dyn Scene>>,
    last_time: f64,
    accumulator: f64,
    fps: FpsCounter,
    letterbox: Color,
    background: Color,
}

impl App {
    fn new(root: Box<dyn Scene>, letterbox: Color, background: Color) -> App {
        App {
            g: Graphics::new(),
            input: Input::default(),
            scenes: vec![root],
            last_time: date::now(),
            accumulator: 0.0,
            fps: FpsCounter::new(),
            letterbox,
            background,
        }
    }

    fn apply(&mut self, t: Transition) {
        match t {
            Transition::None => {}
            Transition::Push(s) => self.scenes.push(s),
            Transition::Pop => {
                self.scenes.pop();
            }
            Transition::Replace(s) => {
                self.scenes.pop();
                self.scenes.push(s);
            }
            Transition::Quit => self.scenes.clear(),
        }
    }
}

impl EventHandler for App {
    fn update(&mut self) {
        let now = date::now();
        let frame_time = (now - self.last_time).min(MAX_FRAME_TIME).max(0.0);
        self.last_time = now;
        self.fps.push(frame_time);
        self.accumulator += frame_time;

        while self.accumulator >= TICK {
            self.accumulator -= TICK;

            self.input.sync_pointer(&self.g.view);

            let transition = {
                let ctx = FrameCtx {
                    input: &self.input,
                    dt: TICK as f32,
                    screen: self.g.canvas(),
                    fps: self.fps.value,
                    now: now - self.accumulator,
                };
                match self.scenes.last_mut() {
                    Some(scene) => scene.update(&ctx),
                    None => Transition::Quit,
                }
            };

            self.input.end_tick();
            self.apply(transition);

            if self.scenes.is_empty() {
                window::order_quit();
                return;
            }
        }
    }

    fn draw(&mut self) {
        if self.scenes.is_empty() {
            return;
        }
        let alpha = (self.accumulator / TICK) as f32;

        // Рисуем начиная с самой верхней непрозрачной сцены: то, что под ней,
        // всё равно не видно.
        let first = self
            .scenes
            .iter()
            .rposition(|s| s.opaque())
            .unwrap_or(0);

        self.g.begin_frame(self.letterbox, self.background);

        let g = &mut self.g;
        for scene in self.scenes[first..].iter_mut() {
            scene.draw(g, alpha);
        }

        self.g.end_frame();
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.input.on_pointer_move(x, y);
    }

    fn mouse_wheel_event(&mut self, _x: f32, y: f32) {
        self.input.on_wheel(y);
    }

    fn mouse_button_down_event(&mut self, _b: MouseButton, x: f32, y: f32) {
        self.input.on_pointer_down(x, y);
    }

    fn mouse_button_up_event(&mut self, _b: MouseButton, x: f32, y: f32) {
        self.input.on_pointer_up(x, y);
    }

    fn touch_event(&mut self, phase: TouchPhase, _id: u64, x: f32, y: f32) {
        match phase {
            TouchPhase::Started => self.input.on_pointer_down(x, y),
            TouchPhase::Moved => self.input.on_pointer_move(x, y),
            TouchPhase::Ended | TouchPhase::Cancelled => self.input.on_pointer_up(x, y),
        }
    }

    fn key_down_event(&mut self, key: KeyCode, _mods: KeyMods, repeat: bool) {
        self.input.on_key_down(key, repeat);
    }

    fn key_up_event(&mut self, key: KeyCode, _mods: KeyMods) {
        self.input.on_key_up(key);
    }
}

/// Сглаженный счётчик FPS — по «сырому» кадру ничего не разглядеть.
struct FpsCounter {
    value: f32,
}

impl FpsCounter {
    fn new() -> FpsCounter {
        FpsCounter { value: 0.0 }
    }

    fn push(&mut self, frame_time: f64) {
        if frame_time <= 0.0 {
            return;
        }
        let instant = (1.0 / frame_time) as f32;
        self.value = if self.value == 0.0 {
            instant
        } else {
            lerp(self.value, instant, 0.1)
        };
    }
}

pub struct Config {
    pub title: &'static str,
    pub window_width: i32,
    pub window_height: i32,
    /// Цвет полей вокруг холста при неподходящем соотношении сторон.
    pub letterbox: Color,
    pub background: Color,
    /// Кратность мультисэмплинга для окна. 0 или 1 — без сглаживания.
    pub sample_count: i32,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            title: "Game",
            window_width: 1280,
            window_height: 720,
            letterbox: Color::hex(0x000000),
            background: Color::hex(0x101018),
            // Модель — гладкая органика на тёмном фоне, её силуэт без
            // сглаживания разваливается на лесенку.
            sample_count: 4,
        }
    }
}

/// Точка входа. Блокирует поток до закрытия окна (на wasm — отдаёт управление
/// браузеру, что для miniquad одно и то же с точки зрения вызывающего кода).
pub fn run<F>(config: Config, make_root: F)
where
    F: 'static + FnOnce() -> Box<dyn Scene>,
{
    let conf = conf::Conf {
        window_title: config.title.to_string(),
        window_width: config.window_width,
        window_height: config.window_height,
        high_dpi: true,
        window_resizable: true,
        sample_count: config.sample_count,
        ..Default::default()
    };
    let (letterbox, background) = (config.letterbox, config.background);
    miniquad::start(conf, move || {
        Box::new(App::new(make_root(), letterbox, background))
    });
}
