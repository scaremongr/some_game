//! Виджеты UI.
//!
//! Логика и отрисовка разделены намеренно: логика живёт в тике с фиксированным
//! шагом, отрисовка — в кадре. Смешивать их (как в классическом immediate mode)
//! значит опрашивать ввод с частотой монитора, а это уже не детерминированно.

use miniquad::KeyCode;

use super::graphics::Graphics;
use super::input::Input;
use super::math::*;

#[derive(Clone, Copy)]
pub struct ButtonStyle {
    pub fill: Color,
    pub fill_hot: Color,
    pub fill_active: Color,
    pub border: Color,
    pub border_hot: Color,
    pub text: Color,
    pub text_hot: Color,
    pub text_scale: f32,
    pub border_width: f32,
}

impl Default for ButtonStyle {
    fn default() -> ButtonStyle {
        ButtonStyle {
            fill: Color::hex(0x1E2030),
            fill_hot: Color::hex(0x2E3350),
            fill_active: Color::hex(0x3D4470),
            border: Color::hex(0x3A3F5C),
            border_hot: Color::hex(0x8AA0FF),
            text: Color::hex(0xB8BEDC),
            text_hot: Color::hex(0xFFFFFF),
            text_scale: 2.0,
            border_width: 1.0,
        }
    }
}

/// Визуальное состояние кнопки, посчитанное в тике логики.
#[derive(Clone, Copy, Default)]
pub struct ButtonState {
    pub hovered: bool,
    pub held: bool,
    pub selected: bool,
}

impl ButtonState {
    fn hot(&self) -> bool {
        self.hovered || self.selected
    }
}

/// Опрос кнопки. Возвращает её состояние и признак срабатывания.
///
/// Срабатывание — отпускание указателя внутри кнопки: нажал, передумал, увёл
/// палец — ничего не произошло. На тач-экране это важнее, чем на мыши.
pub fn button_logic(input: &Input, r: Rect, selected: bool) -> (ButtonState, bool) {
    // Область попадания чуть больше нарисованной: палец толще курсора,
    // и на границе кнопки промах читается как «не нажалось».
    const TOUCH_SLOP: f32 = 5.0;
    let hovered = input.pointer_active && r.inset(-TOUCH_SLOP).contains(input.pointer);
    let state = ButtonState {
        hovered,
        held: hovered && input.pointer_down,
        selected,
    };
    (state, hovered && input.pointer_released)
}

pub fn draw_button(g: &mut Graphics, r: Rect, label: &str, s: ButtonState, style: &ButtonStyle) {
    let fill = if s.held {
        style.fill_active
    } else if s.hot() {
        style.fill_hot
    } else {
        style.fill
    };

    g.rect(r, fill);
    g.rect_outline(
        r,
        style.border_width,
        if s.hot() { style.border_hot } else { style.border },
    );
    g.text_centered(
        label,
        r.center(),
        style.text_scale,
        if s.hot() { style.text_hot } else { style.text },
    );

    if s.selected {
        // Маркер выбранного пункта, чтобы клавиатурная навигация читалась
        // и без опоры на цвет.
        g.text(
            ">",
            vec2(r.x + 8.0, (r.center().y - 7.0).round()),
            2.0,
            style.border_hot,
        );
    }
}

/// Перемещение по вертикальному списку стрелками/WASD с закольцовыванием.
pub fn menu_nav(input: &Input, selected: &mut usize, count: usize) {
    if count == 0 {
        return;
    }
    if input.any_pressed(&[KeyCode::Up, KeyCode::W]) {
        *selected = (*selected + count - 1) % count;
    }
    if input.any_pressed(&[KeyCode::Down, KeyCode::S]) {
        *selected = (*selected + 1) % count;
    }
}

/// Клавиши подтверждения выбора.
pub fn confirm_pressed(input: &Input) -> bool {
    input.any_pressed(&[KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space])
}

/// Клавиши «назад».
pub fn back_pressed(input: &Input) -> bool {
    input.any_pressed(&[KeyCode::Escape, KeyCode::Backspace])
}

/// Сколько символов помещается в строку холста: шрифт моноширинный, 5x7
/// плюс зазор.
pub fn line_chars(canvas_width: f32) -> usize {
    (((canvas_width - 24.0) / 6.0) as usize).max(16)
}

/// Разбивает строку по словам под ширину экрана.
///
/// Длинные куски без пробелов — а именно так выглядят сообщения браузера и
/// пути к файлам — режутся жёстко: иначе они молча уезжают за край, и
/// прочитать причину поломки становится нельзя.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let mut word = word;
        while word.len() > width {
            let (head, tail) = word.split_at(width);
            lines.push(head.to_string());
            word = tail;
        }
        match lines.last_mut() {
            Some(last) if last.len() + 1 + word.len() <= width => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Уехавшая за край строка означала бы, что причину отказа прочитать
    /// нельзя, а весь смысл её показа именно в этом.
    #[test]
    fn wrapping_never_exceeds_the_width() {
        let long = "import / TypeError: Failed to fetch dynamically imported module:                     https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision@0.10.14/vision_bundle.mjs";
        for line in wrap(long, 40) {
            assert!(line.len() <= 40, "строка длиннее ширины: {line}");
        }
        assert!(!wrap(long, 40).is_empty());
    }

    /// Слово длиннее строки не должно теряться: путь к файлу важен целиком.
    #[test]
    fn wrapping_keeps_every_character() {
        let word = "a".repeat(97);
        assert_eq!(wrap(&word, 20).join(""), word);
    }
}
