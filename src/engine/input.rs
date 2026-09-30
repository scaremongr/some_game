//! Состояние ввода за тик.
//!
//! Мышь и касание сведены к одному «указателю»: на PC это курсор, в WebView —
//! палец. Игровой код о разнице не знает.

use std::collections::HashSet;

use miniquad::KeyCode;

use super::graphics::Viewport;
use super::math::*;

#[derive(Default)]
pub struct Input {
    down: HashSet<KeyCode>,
    pressed: HashSet<KeyCode>,
    released: HashSet<KeyCode>,

    pointer_screen: Vec2,
    previous_screen: Vec2,
    /// Позиция указателя в виртуальных координатах холста.
    pub pointer: Vec2,
    pub pointer_down: bool,
    pub pointer_pressed: bool,
    pub pointer_released: bool,
    /// false, пока указателя не было ни разу — чтобы курсор в углу не «подсвечивал»
    /// кнопку при управлении с клавиатуры.
    pub pointer_active: bool,
    /// Смещение указателя за тик, в виртуальных координатах.
    pub pointer_delta: Vec2,
    /// Сколько указатель прошёл с момента нажатия. По нему тап отличается
    /// от перетаскивания: иначе любое вращение камеры заодно переключало бы
    /// движение.
    pub drag_distance: f32,
    /// Прокрутка колеса за тик.
    pub wheel: f32,
}

/// Дальше этого порога жест считается перетаскиванием, а не тапом.
pub const DRAG_THRESHOLD: f32 = 6.0;

impl Input {
    pub fn is_dragging(&self) -> bool {
        self.pointer_down && self.drag_distance > DRAG_THRESHOLD
    }

    /// Тап — отпускание без заметного перетаскивания.
    pub fn tapped(&self) -> bool {
        self.pointer_released && self.drag_distance <= DRAG_THRESHOLD
    }

    pub fn is_down(&self, key: KeyCode) -> bool {
        self.down.contains(&key)
    }

    pub fn just_pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key)
    }

    pub fn just_released(&self, key: KeyCode) -> bool {
        self.released.contains(&key)
    }

    pub fn any_pressed(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.pressed.contains(k))
    }

    /// Направление движения: WASD или стрелки, уже нормализованное.
    pub fn move_dir(&self) -> Vec2 {
        let mut d = Vec2::ZERO;
        if self.is_down(KeyCode::A) || self.is_down(KeyCode::Left) {
            d.x -= 1.0;
        }
        if self.is_down(KeyCode::D) || self.is_down(KeyCode::Right) {
            d.x += 1.0;
        }
        if self.is_down(KeyCode::W) || self.is_down(KeyCode::Up) {
            d.y -= 1.0;
        }
        if self.is_down(KeyCode::S) || self.is_down(KeyCode::Down) {
            d.y += 1.0;
        }
        d.normalize_or_zero()
    }

    // --- вызывается из App, не из игрового кода ---

    pub(crate) fn on_key_down(&mut self, key: KeyCode, repeat: bool) {
        if !repeat {
            self.pressed.insert(key);
        }
        self.down.insert(key);
    }

    pub(crate) fn on_key_up(&mut self, key: KeyCode) {
        self.down.remove(&key);
        self.released.insert(key);
    }

    pub(crate) fn on_pointer_move(&mut self, x: f32, y: f32) {
        self.pointer_screen = vec2(x, y);
        self.pointer_active = true;
    }

    pub(crate) fn on_pointer_down(&mut self, x: f32, y: f32) {
        self.on_pointer_move(x, y);
        self.previous_screen = self.pointer_screen;
        self.pointer_down = true;
        self.pointer_pressed = true;
        self.drag_distance = 0.0;
    }

    pub(crate) fn on_pointer_up(&mut self, x: f32, y: f32) {
        self.on_pointer_move(x, y);
        self.pointer_down = false;
        self.pointer_released = true;
    }

    /// Пересчитать указатель в виртуальные координаты текущего вьюпорта.
    pub(crate) fn on_wheel(&mut self, dy: f32) {
        self.wheel += dy;
    }

    pub(crate) fn sync_pointer(&mut self, view: &Viewport) {
        let now = view.to_virtual(self.pointer_screen);
        let before = view.to_virtual(self.previous_screen);
        self.pointer_delta = now - before;
        if self.pointer_down {
            self.drag_distance += self.pointer_delta.length();
        }
        self.previous_screen = self.pointer_screen;
        self.pointer = now;
    }

    /// Разовые события живут ровно один тик логики.
    pub(crate) fn end_tick(&mut self) {
        self.pressed.clear();
        self.released.clear();
        self.pointer_pressed = false;
        self.pointer_released = false;
        self.pointer_delta = Vec2::ZERO;
        self.wheel = 0.0;
    }
}
