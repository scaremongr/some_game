//! 3D-математика для скелетной анимации.
//!
//! Структуры намеренно повторяют модель glTF 2.0: трансформ узла — это TRS
//! (translation / rotation-кватернион / scale), матрицы column-major. Когда
//! появится загрузчик .glb, он будет чистым парсингом без пересчётов.

use super::math::lerp;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const ZERO: Vec3 = vec3(0.0, 0.0, 0.0);
    pub const ONE: Vec3 = vec3(1.0, 1.0, 1.0);
    pub const X: Vec3 = vec3(1.0, 0.0, 0.0);
    pub const Y: Vec3 = vec3(0.0, 1.0, 0.0);
    pub const Z: Vec3 = vec3(0.0, 0.0, 1.0);

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        vec3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalize(self) -> Vec3 {
        let len = self.length();
        if len > 1e-6 {
            self * (1.0 / len)
        } else {
            Vec3::ZERO
        }
    }

    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        vec3(
            lerp(self.x, o.x, t),
            lerp(self.y, o.y, t),
            lerp(self.z, o.z, t),
        )
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl std::ops::Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        vec3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        vec3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        vec3(self.x * s, self.y * s, self.z * s)
    }
}

impl std::ops::Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        vec3(-self.x, -self.y, -self.z)
    }
}

impl std::ops::AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}

/// Кватернион (x, y, z, w). Вращения в скелете хранятся только так: они
/// корректно интерполируются, в отличие от углов Эйлера.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Quat {
        Quat::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let axis = axis.normalize();
        let (s, c) = (angle * 0.5).sin_cos();
        Quat {
            x: axis.x * s,
            y: axis.y * s,
            z: axis.z * s,
            w: c,
        }
    }

    /// Порядок YXZ (рыскание, тангаж, крен) — привычный для персонажной
    /// анимации: сначала поворот вокруг вертикали.
    /// Кратчайший поворот, переводящий направление `from` в `to`.
    ///
    /// Ось — их векторное произведение, угол — между ними. Отдельно разобран
    /// случай противоположных направлений: там векторное произведение
    /// вырождается в ноль и оси не остаётся, поэтому берём любую
    /// перпендикулярную.
    pub fn from_rotation_arc(from: Vec3, to: Vec3) -> Quat {
        let a = from.normalize();
        let b = to.normalize();
        let cosine = a.dot(b).clamp(-1.0, 1.0);

        if cosine > 0.999_999 {
            return Quat::IDENTITY;
        }
        if cosine < -0.999_999 {
            // Ищем ось подальше от `a`, иначе произведение снова выродится.
            let far = if a.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
            return Quat::from_axis_angle(a.cross(far).normalize(), std::f32::consts::PI);
        }

        Quat::from_axis_angle(a.cross(b).normalize(), cosine.acos())
    }

    /// Достаёт поворот из матрицы: столбцы нормируются, сдвиг игнорируется.
    ///
    /// Метод Шеппарда — берётся та из четырёх формул, у которой знаменатель
    /// наибольший. Наивная формула через след теряет точность у поворотов
    /// около 180 градусов, а такие в ригах встречаются постоянно.
    pub fn from_matrix(m: Mat4) -> Quat {
        let column = |i: usize| vec3(m.0[i * 4], m.0[i * 4 + 1], m.0[i * 4 + 2]).normalize();
        let (cx, cy, cz) = (column(0), column(1), column(2));
        let (m00, m01, m02) = (cx.x, cy.x, cz.x);
        let (m10, m11, m12) = (cx.y, cy.y, cz.y);
        let (m20, m21, m22) = (cx.z, cy.z, cz.z);

        let trace = m00 + m11 + m22;
        let q = if trace > 0.0 {
            let s = (trace + 1.0).sqrt() * 2.0;
            Quat { w: 0.25 * s, x: (m21 - m12) / s, y: (m02 - m20) / s, z: (m10 - m01) / s }
        } else if m00 > m11 && m00 > m22 {
            let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
            Quat { w: (m21 - m12) / s, x: 0.25 * s, y: (m01 + m10) / s, z: (m02 + m20) / s }
        } else if m11 > m22 {
            let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
            Quat { w: (m02 - m20) / s, x: (m01 + m10) / s, y: 0.25 * s, z: (m12 + m21) / s }
        } else {
            let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
            Quat { w: (m10 - m01) / s, x: (m02 + m20) / s, y: (m12 + m21) / s, z: 0.25 * s }
        };
        q.normalize()
    }

    pub fn from_euler(yaw: f32, pitch: f32, roll: f32) -> Quat {
        Quat::from_axis_angle(Vec3::Y, yaw)
            * Quat::from_axis_angle(Vec3::X, pitch)
            * Quat::from_axis_angle(Vec3::Z, roll)
    }

    pub fn dot(self, o: Quat) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w
    }

    pub fn normalize(self) -> Quat {
        let len = self.dot(self).sqrt();
        if len > 1e-6 {
            let k = 1.0 / len;
            Quat {
                x: self.x * k,
                y: self.y * k,
                z: self.z * k,
                w: self.w * k,
            }
        } else {
            Quat::IDENTITY
        }
    }

    pub fn conjugate(self) -> Quat {
        Quat {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: self.w,
        }
    }

    /// Сферическая интерполяция. При малом угле переходит на линейную —
    /// иначе деление на синус около нуля даёт мусор.
    pub fn slerp(self, mut o: Quat, t: f32) -> Quat {
        let mut cos = self.dot(o);
        // Кватернионы q и -q задают один поворот; берём ближний путь.
        if cos < 0.0 {
            o = Quat {
                x: -o.x,
                y: -o.y,
                z: -o.z,
                w: -o.w,
            };
            cos = -cos;
        }

        if cos > 0.9995 {
            return Quat {
                x: lerp(self.x, o.x, t),
                y: lerp(self.y, o.y, t),
                z: lerp(self.z, o.z, t),
                w: lerp(self.w, o.w, t),
            }
            .normalize();
        }

        let angle = cos.clamp(-1.0, 1.0).acos();
        let sin = angle.sin();
        let (a, b) = (
            ((1.0 - t) * angle).sin() / sin,
            (t * angle).sin() / sin,
        );
        Quat {
            x: self.x * a + o.x * b,
            y: self.y * a + o.y * b,
            z: self.z * a + o.z * b,
            w: self.w * a + o.w * b,
        }
    }

    pub fn rotate(self, v: Vec3) -> Vec3 {
        let q = vec3(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }
}

impl std::ops::Mul for Quat {
    type Output = Quat;
    /// Композиция поворотов: сначала `o`, затем `self`.
    fn mul(self, o: Quat) -> Quat {
        Quat {
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
        }
    }
}

/// Матрица 4x4, column-major: `m[col * 4 + row]` — в таком виде её ждёт GL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Default for Mat4 {
    fn default() -> Mat4 {
        Mat4::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);

    pub fn from_trs(t: Vec3, r: Quat, s: Vec3) -> Mat4 {
        let (x, y, z, w) = (r.x, r.y, r.z, r.w);
        let (x2, y2, z2) = (x + x, y + y, z + z);
        let (xx, xy, xz) = (x * x2, x * y2, x * z2);
        let (yy, yz, zz) = (y * y2, y * z2, z * z2);
        let (wx, wy, wz) = (w * x2, w * y2, w * z2);

        Mat4([
            (1.0 - (yy + zz)) * s.x,
            (xy + wz) * s.x,
            (xz - wy) * s.x,
            0.0,
            (xy - wz) * s.y,
            (1.0 - (xx + zz)) * s.y,
            (yz + wx) * s.y,
            0.0,
            (xz + wy) * s.z,
            (yz - wx) * s.z,
            (1.0 - (xx + yy)) * s.z,
            0.0,
            t.x,
            t.y,
            t.z,
            1.0,
        ])
    }

    pub fn translation(t: Vec3) -> Mat4 {
        Mat4::from_trs(t, Quat::IDENTITY, Vec3::ONE)
    }

    /// Первые три столбца матрицы, по 4 числа — компактная форма аффинного
    /// преобразования. В таком виде кости уезжают в шейдер.
    pub fn to_rows3(self) -> [[f32; 4]; 3] {
        let m = self.0;
        [
            [m[0], m[4], m[8], m[12]],
            [m[1], m[5], m[9], m[13]],
            [m[2], m[6], m[10], m[14]],
        ]
    }

    pub fn transform_point(self, p: Vec3) -> Vec3 {
        let m = self.0;
        vec3(
            m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
            m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
            m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
        )
    }

    /// Поворачивает направление, игнорируя перенос. Для нормалей при
    /// равномерном масштабе этого достаточно.
    pub fn transform_direction(self, d: Vec3) -> Vec3 {
        let m = self.0;
        vec3(
            m[0] * d.x + m[4] * d.y + m[8] * d.z,
            m[1] * d.x + m[5] * d.y + m[9] * d.z,
            m[2] * d.x + m[6] * d.y + m[10] * d.z,
        )
    }

    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fov_y * 0.5).tan();
        let nf = 1.0 / (near - far);
        Mat4([
            f / aspect, 0.0, 0.0, 0.0, //
            0.0, f, 0.0, 0.0, //
            0.0, 0.0, (far + near) * nf, -1.0, //
            0.0, 0.0, 2.0 * far * near * nf, 0.0,
        ])
    }

    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = (target - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);
        Mat4([
            s.x, u.x, -f.x, 0.0, //
            s.y, u.y, -f.y, 0.0, //
            s.z, u.z, -f.z, 0.0, //
            -s.dot(eye), -u.dot(eye), f.dot(eye), 1.0,
        ])
    }

    /// Обращение общей матрицы 4x4 методом алгебраических дополнений.
    /// Вырожденную возвращает как единичную — в анимации это лучше, чем NaN,
    /// расползающийся по всему скелету.
    pub fn invert(self) -> Mat4 {
        let m = self.0;
        let mut inv = [0.0f32; 16];

        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
            + m[9] * m[7] * m[14]
            + m[13] * m[6] * m[11]
            - m[13] * m[7] * m[10];
        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
            - m[8] * m[7] * m[14]
            - m[12] * m[6] * m[11]
            + m[12] * m[7] * m[10];
        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
            + m[8] * m[7] * m[13]
            + m[12] * m[5] * m[11]
            - m[12] * m[7] * m[9];
        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
            - m[8] * m[6] * m[13]
            - m[12] * m[5] * m[10]
            + m[12] * m[6] * m[9];
        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
            - m[9] * m[3] * m[14]
            - m[13] * m[2] * m[11]
            + m[13] * m[3] * m[10];
        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
            + m[8] * m[3] * m[14]
            + m[12] * m[2] * m[11]
            - m[12] * m[3] * m[10];
        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
            - m[8] * m[3] * m[13]
            - m[12] * m[1] * m[11]
            + m[12] * m[3] * m[9];
        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
            + m[8] * m[2] * m[13]
            + m[12] * m[1] * m[10]
            - m[12] * m[2] * m[9];
        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
            + m[5] * m[3] * m[14]
            + m[13] * m[2] * m[7]
            - m[13] * m[3] * m[6];
        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
            - m[4] * m[3] * m[14]
            - m[12] * m[2] * m[7]
            + m[12] * m[3] * m[6];
        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
            + m[4] * m[3] * m[13]
            + m[12] * m[1] * m[7]
            - m[12] * m[3] * m[5];
        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
            - m[4] * m[2] * m[13]
            - m[12] * m[1] * m[6]
            + m[12] * m[2] * m[5];
        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
            - m[5] * m[3] * m[10]
            - m[9] * m[2] * m[7]
            + m[9] * m[3] * m[6];
        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
            + m[4] * m[3] * m[10]
            + m[8] * m[2] * m[7]
            - m[8] * m[3] * m[6];
        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
            - m[4] * m[3] * m[9]
            - m[8] * m[1] * m[7]
            + m[8] * m[3] * m[5];
        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
            + m[4] * m[2] * m[9]
            + m[8] * m[1] * m[6]
            - m[8] * m[2] * m[5];

        let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        if det.abs() < 1e-8 {
            return Mat4::IDENTITY;
        }
        let k = 1.0 / det;
        for v in inv.iter_mut() {
            *v *= k;
        }
        Mat4(inv)
    }
}

impl std::ops::Mul for Mat4 {
    type Output = Mat4;
    /// Композиция: сначала `o`, затем `self`.
    fn mul(self, o: Mat4) -> Mat4 {
        let (a, b) = (self.0, o.0);
        let mut r = [0.0f32; 16];
        for col in 0..4 {
            for row in 0..4 {
                r[col * 4 + row] = a[row] * b[col * 4]
                    + a[4 + row] * b[col * 4 + 1]
                    + a[8 + row] * b[col * 4 + 2]
                    + a[12 + row] * b[col * 4 + 3];
            }
        }
        Mat4(r)
    }
}

/// Локальный трансформ узла — ровно то, чем оперирует glTF.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Transform {
        Transform::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn from_translation(t: Vec3) -> Transform {
        Transform {
            translation: t,
            ..Transform::IDENTITY
        }
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_trs(self.translation, self.rotation, self.scale)
    }

    /// Смешивание двух трансформов — основа кроссфейда между движениями.
    pub fn blend(&self, o: &Transform, t: f32) -> Transform {
        Transform {
            translation: self.translation.lerp(o.translation, t),
            rotation: self.rotation.slerp(o.rotation, t),
            scale: self.scale.lerp(o.scale, t),
        }
    }
}

#[cfg(test)]
mod matrix_tests {
    use super::*;

    /// Поворот, ушедший в матрицу, обязан вернуться из неё тем же. Проверяются
    /// и углы около 180 градусов: наивная формула через след теряет на них
    /// точность, а в ригах они встречаются постоянно.
    #[test]
    fn rotation_survives_a_round_trip_through_a_matrix() {
        let axes = [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            vec3(0.3, -0.5, 0.81).normalize(),
        ];
        let angles = [0.0, 0.4, 1.57, 3.0, 3.14159, -2.2];

        for axis in axes {
            for angle in angles {
                let q = Quat::from_axis_angle(axis, angle);
                let back = Quat::from_matrix(Mat4::from_trs(Vec3::ZERO, q, Vec3::ONE));
                assert!(
                    q.dot(back).abs() > 0.999,
                    "ось {axis:?} угол {angle}: вернулось {back:?} вместо {q:?}"
                );
            }
        }
    }

    /// Масштаб в матрице не должен просачиваться в поворот.
    #[test]
    fn scale_does_not_leak_into_the_rotation() {
        let q = Quat::from_axis_angle(vec3(0.2, 0.9, 0.3).normalize(), 1.2);
        let scaled = Mat4::from_trs(vec3(5.0, -2.0, 1.0), q, vec3(0.01, 0.01, 0.01));
        assert!(Quat::from_matrix(scaled).dot(q).abs() > 0.999);
    }
}
