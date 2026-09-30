//! Самописный 2D-движок: окно, ввод, батчер спрайтов, стек сцен.
//!
//! Ниже — только miniquad, который даёт окно, GL-контекст и события.
//! Всё остальное — свой код, одинаковый для нативной сборки и для wasm.

// Движок — это API поверх игры: часть примитивов ещё не вызвана из game/,
// и это нормально.
#![allow(dead_code)]

pub mod app;
pub mod assets;
pub mod physics;
pub mod audio;
pub mod beat;
pub mod font;
pub mod gltf;
pub mod graphics;
pub mod input;
pub mod math;
pub mod math3;
pub mod mesh3;
pub mod pose;
pub mod retarget;
pub mod render3d;
pub mod skeleton;
pub mod ui;
