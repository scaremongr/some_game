//! Минимальная математика движка. Ничего лишнего — только то, что реально нужно 2D.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalize_or_zero(self) -> Vec2 {
        let len = self.length();
        if len > 1e-6 {
            vec2(self.x / len, self.y / len)
        } else {
            Vec2::ZERO
        }
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        vec2(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f32) -> Vec2 {
        vec2(self.x * s, self.y * s)
    }
}

impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}

/// Прямоугольник в экранных координатах: y растёт вниз.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

impl Rect {
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }

    pub fn center(&self) -> Vec2 {
        vec2(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// Прямоугольник, уменьшенный на `d` со всех сторон (отрицательное `d` — расширяет).
    pub fn inset(&self, d: f32) -> Rect {
        rect(self.x + d, self.y + d, self.w - d * 2.0, self.h - d * 2.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

pub const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

impl Color {
    /// `0xRRGGBB` — так палитру удобнее держать в одном месте.
    pub const fn hex(v: u32) -> Color {
        Color {
            r: ((v >> 16) & 0xFF) as f32 / 255.0,
            g: ((v >> 8) & 0xFF) as f32 / 255.0,
            b: (v & 0xFF) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub const fn with_alpha(self, a: f32) -> Color {
        Color { a, ..self }
    }

    /// sRGB -> линейное пространство. 3D-шейдер считает освещение в линейном
    /// и сам делает гамма-коррекцию на выходе, поэтому все цвета, уезжающие
    /// в него, обязаны пройти через это преобразование.
    pub fn to_linear(self) -> Color {
        fn ch(v: f32) -> f32 {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        rgba(ch(self.r), ch(self.g), ch(self.b), self.a)
    }

    pub fn lerp(self, o: Color, t: f32) -> Color {
        rgba(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
            self.a + (o.a - self.a) * t,
        )
    }
}

pub const WHITE: Color = rgb(1.0, 1.0, 1.0);

/// Ортографическая проекция для экранных координат (0,0) — левый верхний угол.
/// Возвращает column-major матрицу, как её ждёт GL.
pub fn ortho(width: f32, height: f32) -> [f32; 16] {
    [
        2.0 / width, 0.0,           0.0,  0.0,
        0.0,        -2.0 / height,  0.0,  0.0,
        0.0,         0.0,          -1.0,  0.0,
       -1.0,         1.0,           0.0,  1.0,
    ]
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
