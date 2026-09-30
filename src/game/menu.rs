//! Главный экран.

use crate::engine::app::{FrameCtx, Scene, Transition};
use crate::engine::graphics::Graphics;
use crate::engine::math::*;
use crate::engine::ui::{self, ButtonState, ButtonStyle};

use super::clips::ClipsScene;
use super::dance::DanceScene;
use super::mirror::MirrorScene;
use super::palette;
use super::settings::SettingsScene;

const TITLE: &str = "SOME GAME";
const ITEMS: [&str; 5] = ["PLAY", "MIRROR", "CLIPS", "SETTINGS", "QUIT"];

const BUTTON_W: f32 = 200.0;
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 12.0;

pub struct MenuScene {
    selected: usize,
    states: [ButtonState; ITEMS.len()],
    style: ButtonStyle,
    /// Время жизни сцены — для «дыхания» подзаголовка.
    time: f32,
}

impl MenuScene {
    pub fn new() -> MenuScene {
        MenuScene {
            selected: 0,
            states: [ButtonState::default(); ITEMS.len()],
            style: ButtonStyle::default(),
            time: 0.0,
        }
    }

    /// Кнопки считаются от размера холста: на телефоне он вытянут вверх,
    /// и жёсткие координаты там не годятся.
    fn button_rect(canvas: Vec2, index: usize) -> Rect {
        // На узком экране кнопка растягивается почти во всю ширину, чтобы
        // в неё было удобно попадать пальцем.
        let width = BUTTON_W.min(canvas.x - 48.0);
        let height = if canvas.y > canvas.x { BUTTON_H * 1.4 } else { BUTTON_H };
        let gap = BUTTON_GAP + height - BUTTON_H;
        let block = ITEMS.len() as f32 * (height + gap) - gap;
        let top = (canvas.y - block) * 0.62;
        rect(
            (canvas.x - width) * 0.5,
            top + index as f32 * (height + gap),
            width,
            height,
        )
    }

    fn activate(index: usize) -> Transition {
        match index {
            0 => Transition::Push(Box::new(DanceScene::new())),
            1 => Transition::Push(Box::new(MirrorScene::new())),
            2 => Transition::Push(Box::new(ClipsScene::new())),
            3 => Transition::Push(Box::new(SettingsScene::new())),
            4 => Transition::Quit,
            _ => Transition::None,
        }
    }
}

impl Scene for MenuScene {
    fn update(&mut self, ctx: &FrameCtx) -> Transition {
        self.time += ctx.dt;

        ui::menu_nav(ctx.input, &mut self.selected, ITEMS.len());

        let mut activated = None;
        for i in 0..ITEMS.len() {
            let (state, clicked) =
                ui::button_logic(ctx.input, Self::button_rect(ctx.screen, i), self.selected == i);

            // Наведение мышью ведёт за собой клавиатурный выбор, иначе
            // подсветятся сразу два пункта.
            if state.hovered {
                self.selected = i;
            }
            self.states[i] = state;
            if clicked {
                activated = Some(i);
            }
        }

        // Selected мог измениться после того, как состояния уже записаны.
        for (i, s) in self.states.iter_mut().enumerate() {
            s.selected = self.selected == i;
        }

        if ui::confirm_pressed(ctx.input) {
            activated = Some(self.selected);
        }

        match activated {
            Some(i) => Self::activate(i),
            None => Transition::None,
        }
    }

    fn draw(&mut self, g: &mut Graphics, _alpha: f32) {
        let canvas = g.canvas();
        let first = Self::button_rect(canvas, 0);

        // Заголовок ставится над блоком кнопок, а не по абсолютной высоте:
        // иначе на вытянутом экране он повисает в пустоте.
        let title_scale = if canvas.y > canvas.x { 4.0 } else { 5.0 };
        g.text_centered(
            TITLE,
            vec2(canvas.x * 0.5, first.y - 76.0),
            title_scale,
            palette::TITLE,
        );

        let pulse = 0.55 + 0.45 * (self.time * 1.6).sin().abs();
        g.text_centered(
            "A HAND-ROLLED 2D ENGINE",
            vec2(canvas.x * 0.5, first.y - 40.0),
            1.0,
            palette::ACCENT.with_alpha(pulse),
        );

        for (i, label) in ITEMS.iter().enumerate() {
            ui::draw_button(
                g,
                Self::button_rect(canvas, i),
                label,
                self.states[i],
                &self.style,
            );
        }

        g.text_centered(
            "TAP OR USE ARROWS AND ENTER",
            vec2(canvas.x * 0.5, canvas.y - 22.0),
            1.0,
            palette::MUTED,
        );
    }
}
