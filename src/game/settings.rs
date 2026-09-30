//! Заглушка настроек. Нужна на этом этапе, чтобы стек сцен реально работал
//! на push/pop, а не только на замене корневой сцены.

use crate::engine::app::{FrameCtx, Scene, Transition};
use crate::engine::graphics::Graphics;
use crate::engine::math::*;
use crate::engine::ui::{self, ButtonState, ButtonStyle};

use super::palette;

/// Кнопка считается от размера холста: он меняется при повороте экрана.
fn back_button(canvas: Vec2) -> Rect {
    let width = 180.0f32.min(canvas.x - 48.0);
    let height = if canvas.y > canvas.x { 48.0 } else { 34.0 };
    rect(
        (canvas.x - width) * 0.5,
        canvas.y * 0.62,
        width,
        height,
    )
}

pub struct SettingsScene {
    back: ButtonState,
    style: ButtonStyle,
}

impl SettingsScene {
    pub fn new() -> SettingsScene {
        SettingsScene {
            back: ButtonState {
                selected: true,
                ..Default::default()
            },
            style: ButtonStyle::default(),
        }
    }
}

impl Scene for SettingsScene {
    fn update(&mut self, ctx: &FrameCtx) -> Transition {
        let (mut state, clicked) = ui::button_logic(ctx.input, back_button(ctx.screen), true);
        state.selected = true;
        self.back = state;

        if clicked || ui::back_pressed(ctx.input) || ui::confirm_pressed(ctx.input) {
            return Transition::Pop;
        }
        Transition::None
    }

    fn draw(&mut self, g: &mut Graphics, _alpha: f32) {
        let canvas = g.canvas();
        let button = back_button(canvas);

        g.text_centered(
            "SETTINGS",
            vec2(canvas.x * 0.5, button.y - 90.0),
            if canvas.y > canvas.x { 3.0 } else { 4.0 },
            palette::TITLE,
        );
        g.text_centered(
            "NOTHING TO CONFIGURE YET",
            vec2(canvas.x * 0.5, button.y - 50.0),
            1.0,
            palette::MUTED,
        );

        ui::draw_button(g, button, "BACK", self.back, &self.style);

        g.text_centered(
            "TAP BACK OR PRESS ESC",
            vec2(canvas.x * 0.5, canvas.y - 22.0),
            1.0,
            palette::MUTED,
        );
    }
}
