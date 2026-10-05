//! Игровой слой: сцены и палитра. Движка тут нет — только то, что про игру.

pub mod camera;
pub mod character;
pub mod clips;
pub mod dance;
pub mod dancer;
pub mod menu;
pub mod mirror;
pub mod rhythm;
pub mod settings;
pub mod fight;
pub mod anims;
pub mod body;
pub mod effects;
pub mod fighter_model;
pub mod mocap;
pub mod ragdoll;
pub mod arena_props;
pub mod timeline;

use crate::engine::math::Color;

/// Палитра в одном месте: менять цвета игры, не открывая сцены.
pub mod palette {
    use super::Color;

    pub const LETTERBOX: Color = Color::hex(0x05060A);
    pub const BACKGROUND: Color = Color::hex(0x12141F);
    pub const STAGE_TOP: Color = Color::hex(0x0A0C15);
    pub const STAGE_BOTTOM: Color = Color::hex(0x1B1E33);
    pub const FLOOR_FAR: Color = Color::hex(0x11142A);
    pub const FLOOR_NEAR: Color = Color::hex(0x1A1E38);
    /// Пятно света на полу: без него тень не на чем читать.
    pub const SPOTLIGHT: Color = Color::hex(0x6F7CC4);
    pub const GRID_MAJOR: Color = Color::hex(0x272C42);
    pub const TITLE: Color = Color::hex(0xF2F4FF);
    pub const ACCENT: Color = Color::hex(0x8AA0FF);
    pub const MUTED: Color = Color::hex(0x5A6180);
    pub const PLAYER: Color = Color::hex(0xFFD479);

    /// Оценка совпадения позы. Три ступени, а не градиент из двух цветов:
    /// «наполовину похоже» должно читаться отдельно, а не как «почти
    /// зелёное».
    pub const MATCH_GOOD: Color = Color::hex(0x6FE08A);
    pub const MATCH_FAIR: Color = Color::hex(0xF2C25C);
    pub const MATCH_BAD: Color = Color::hex(0xFF6B7A);
    /// Тень танца на человеке: тёмный силуэт под его скелетом. Не чёрный —
    /// на тёмной комнате чистый чёрный сливается с фоном.
    pub const SHADOW: Color = Color::hex(0x10131F);
}
