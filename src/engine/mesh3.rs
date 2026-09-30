//! Данные скиннингового меша и процедурный конструктор форм.
//!
//! Конструктор нужен, пока нет настоящей модели: на капсулах вокруг костей
//! видно, что скиннинг и деформация работают. Когда появится .glb, эти же
//! `Vertex3`/`MeshData` заполнит загрузчик — формат вершины уже под glTF.

use super::math3::*;

/// Вершина скиннингового меша. Индексы костей лежат во float'ах намеренно:
/// целочисленные вершинные атрибуты в WebGL 1 недоступны.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex3 {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 3],
    pub joints: [f32; 4],
    pub weights: [f32; 4],
}

/// Кусок меша с одним материалом. Персонаж почти всегда состоит из
/// нескольких: кожа, волосы, одежда — у каждого своя текстура, а значит
/// свой вызов отрисовки.
#[derive(Clone, Copy, Debug)]
pub struct SubMesh {
    pub first_index: u32,
    pub index_count: u32,
    pub material: usize,
}

#[derive(Default)]
pub struct MeshData {
    pub verts: Vec<Vertex3>,
    pub indices: Vec<u32>,
    pub submeshes: Vec<SubMesh>,
}

impl MeshData {
    /// Закрывает текущий кусок: всё, что добавилось с прошлого вызова,
    /// относится к материалу `material`.
    pub fn end_submesh(&mut self, material: usize) {
        let first = self
            .submeshes
            .last()
            .map(|s| s.first_index + s.index_count)
            .unwrap_or(0);
        let count = self.indices.len() as u32 - first;
        if count > 0 {
            self.submeshes.push(SubMesh {
                first_index: first,
                index_count: count,
                material,
            });
        }
    }
}

impl MeshData {
    /// Капсула вдоль отрезка между двумя суставами, с радиусами на концах.
    ///
    /// Вершины привязываются к `joint_a`, но ближе к `joint_b` вес плавно
    /// перетекает на него — иначе сустав будет ломаться складкой.
    #[allow(clippy::too_many_arguments)]
    pub fn add_capsule(
        &mut self,
        from: Vec3,
        to: Vec3,
        r_from: f32,
        r_to: f32,
        segments: usize,
        joint_a: usize,
        joint_b: usize,
        color: Vec3,
    ) {
        let axis = to - from;
        let length = axis.length();
        if length < 1e-5 {
            return;
        }
        let y = axis * (1.0 / length);
        // Опорный вектор не должен быть коллинеарен оси, иначе базис вырождается.
        let reference = if y.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
        let x = reference.cross(y).normalize();
        let z = x.cross(y);

        // Профиль капсулы: пары (смещение вдоль оси, радиус).
        let cap = 4usize;
        let body = 6usize;
        let mut rings: Vec<(f32, f32)> = Vec::with_capacity(cap * 2 + body + 2);

        for i in 0..=cap {
            let phi = (i as f32 / cap as f32) * std::f32::consts::FRAC_PI_2;
            rings.push((-r_from * phi.cos(), r_from * phi.sin()));
        }
        for i in 1..=body {
            let t = i as f32 / body as f32;
            rings.push((t * length, r_from + (r_to - r_from) * t));
        }
        for i in 1..=cap {
            let phi = (i as f32 / cap as f32) * std::f32::consts::FRAC_PI_2;
            rings.push((length + r_to * phi.sin(), r_to * phi.cos()));
        }

        let base = self.verts.len() as u32;
        let ring_verts = segments + 1; // шов дублируется ради разрыва UV

        for (ri, &(offset, radius)) in rings.iter().enumerate() {
            let t_along = (offset / length).clamp(0.0, 1.0);
            let (wa, wb) = joint_weights(t_along);

            for s in 0..ring_verts {
                let a = (s as f32 / segments as f32) * std::f32::consts::TAU;
                let radial = x * a.cos() + z * a.sin();
                let p = from + y * offset + radial * radius;

                self.verts.push(Vertex3 {
                    pos: p.to_array(),
                    normal: radial.to_array(), // уточняется в compute_normals
                    uv: [s as f32 / segments as f32, ri as f32 / (rings.len() - 1) as f32],
                    color: color.to_array(),
                    joints: [joint_a as f32, joint_b as f32, 0.0, 0.0],
                    weights: [wa, wb, 0.0, 0.0],
                });
            }
        }

        for ri in 0..rings.len() - 1 {
            for s in 0..segments {
                let i0 = base + (ri * ring_verts + s) as u32;
                let i1 = i0 + 1;
                let i2 = base + ((ri + 1) * ring_verts + s) as u32;
                let i3 = i2 + 1;
                self.indices
                    .extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
            }
        }
    }

    /// Сфера, жёстко привязанная к одному суставу — голова, кисти, стопы.
    pub fn add_sphere(
        &mut self,
        center: Vec3,
        radius: Vec3,
        segments: usize,
        rings: usize,
        joint: usize,
        color: Vec3,
    ) {
        let base = self.verts.len() as u32;
        let ring_verts = segments + 1;

        for r in 0..=rings {
            let phi = (r as f32 / rings as f32) * std::f32::consts::PI;
            let (sp, cp) = phi.sin_cos();
            for s in 0..ring_verts {
                let theta = (s as f32 / segments as f32) * std::f32::consts::TAU;
                let (st, ct) = theta.sin_cos();
                let n = vec3(sp * ct, cp, sp * st);
                let p = center + vec3(n.x * radius.x, n.y * radius.y, n.z * radius.z);

                self.verts.push(Vertex3 {
                    pos: p.to_array(),
                    normal: n.to_array(),
                    uv: [s as f32 / segments as f32, r as f32 / rings as f32],
                    color: color.to_array(),
                    joints: [joint as f32, joint as f32, 0.0, 0.0],
                    weights: [1.0, 0.0, 0.0, 0.0],
                });
            }
        }

        for r in 0..rings {
            for s in 0..segments {
                let i0 = base + (r * ring_verts + s) as u32;
                let i1 = i0 + 1;
                let i2 = base + ((r + 1) * ring_verts + s) as u32;
                let i3 = i2 + 1;
                self.indices
                    .extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
            }
        }
    }

    /// Пересчитывает нормали усреднением по смежным треугольникам. Для
    /// процедурных форм это надёжнее, чем выводить нормаль аналитически:
    /// на конических участках капсулы аналитика заметно врёт.
    pub fn compute_normals(&mut self) {
        for v in self.verts.iter_mut() {
            v.normal = [0.0, 0.0, 0.0];
        }

        for tri in self.indices.chunks_exact(3) {
            let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let pa = Vec3::from(self.verts[a].pos);
            let pb = Vec3::from(self.verts[b].pos);
            let pc = Vec3::from(self.verts[c].pos);
            let n = (pb - pa).cross(pc - pa);
            for &i in &[a, b, c] {
                let v = &mut self.verts[i];
                v.normal[0] += n.x;
                v.normal[1] += n.y;
                v.normal[2] += n.z;
            }
        }

        for v in self.verts.iter_mut() {
            let n = Vec3::from(v.normal).normalize();
            v.normal = n.to_array();
        }
    }
}

impl From<[f32; 3]> for Vec3 {
    fn from(a: [f32; 3]) -> Vec3 {
        vec3(a[0], a[1], a[2])
    }
}

/// Распределение веса вдоль сегмента: у начала — целиком на своей кости,
/// у дальнего конца — половина уходит на следующую, чтобы сгиб был гладким.
fn joint_weights(t: f32) -> (f32, f32) {
    const BLEND_START: f32 = 0.65;
    if t <= BLEND_START {
        return (1.0, 0.0);
    }
    let k = ((t - BLEND_START) / (1.0 - BLEND_START)).clamp(0.0, 1.0);
    // Плавная ступенька: без неё на границе зоны видно излом.
    let smooth = k * k * (3.0 - 2.0 * k);
    let wb = smooth * 0.5;
    (1.0 - wb, wb)
}
