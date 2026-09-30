//! Impact effects: sparks, dust and shock rings, simulated in world space and
//! drawn as projected 2D strokes. Purely visual; timed by the render clock.
use crate::engine::{graphics::Graphics, math::*, math3::*, render3d::Camera};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Spark,
    Dust,
    Ring,
}

struct Particle {
    kind: Kind,
    pos: Vec3,
    vel: Vec3,
    life: f32,
    max: f32,
    size: f32,
    color: Color,
}

pub struct Effects {
    list: Vec<Particle>,
    seed: u32,
}

impl Effects {
    pub fn new() -> Effects {
        Effects {
            list: Vec::new(),
            seed: 0x9E37_79B9,
        }
    }
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 10_000) as f32 / 10_000.0
    }
    fn spread(&mut self) -> Vec3 {
        vec3(
            self.random() * 2.0 - 1.0,
            self.random() * 2.0 - 1.0,
            self.random() * 2.0 - 1.0,
        )
    }
    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Sparks bursting from `at`, biased along `dir` (x: attack direction).
    pub fn sparks(&mut self, at: Vec3, dir: f32, count: usize, speed: f32, color: Color) {
        for _ in 0..count {
            let r = self.spread();
            let vel = vec3(dir * (0.6 + self.random()) + r.x * 0.7, r.y * 1.1 + 0.3, r.z * 0.7)
                .normalize()
                * speed
                * (0.45 + self.random() * 0.75);
            let life = 0.16 + self.random() * 0.20;
            let size = 1.5 + self.random() * 1.8;
            self.list.push(Particle {
                kind: Kind::Spark,
                pos: at,
                vel,
                life,
                max: life,
                size,
                color,
            });
        }
    }

    /// Dust cloud rolling along the floor from `at`.
    pub fn dust(&mut self, at: Vec3, count: usize, radius: f32) {
        for _ in 0..count {
            let r = self.spread();
            let life = 0.45 + self.random() * 0.45;
            let (up, rise, size) = (self.random(), self.random(), self.random());
            self.list.push(Particle {
                kind: Kind::Dust,
                pos: at + vec3(r.x * radius * 0.4, 0.04 + up * 0.12, r.z * radius * 0.3),
                vel: vec3(r.x * 1.3, 0.25 + rise * 0.55, r.z * 0.6) * radius,
                life,
                max: life,
                size: 0.10 + size * 0.12 * radius.max(0.6),
                color: Color::hex(0xB8B2A6),
            });
        }
    }

    /// Expanding ring on the floor (ground pound, energy burst).
    pub fn ring(&mut self, at: Vec3, radius: f32, color: Color) {
        self.list.push(Particle {
            kind: Kind::Ring,
            pos: at,
            vel: Vec3::ZERO,
            life: 0.35,
            max: 0.35,
            size: radius,
            color,
        });
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.list {
            p.life -= dt;
            match p.kind {
                Kind::Spark => {
                    p.vel.y -= 7.0 * dt;
                    p.vel = p.vel * (1.0 - 3.5 * dt).max(0.0);
                }
                Kind::Dust => {
                    p.vel = p.vel * (1.0 - 2.8 * dt).max(0.0);
                    p.size += dt * 0.22;
                }
                Kind::Ring => {}
            }
            p.pos += p.vel * dt;
            if p.pos.y < 0.01 {
                p.pos.y = 0.01;
                p.vel.y = p.vel.y.abs() * 0.3;
            }
        }
        self.list.retain(|p| p.life > 0.0);
    }

    pub fn draw(&self, g: &mut Graphics, camera: &Camera, canvas: Vec2) {
        for p in &self.list {
            let k = (p.life / p.max).clamp(0.0, 1.0);
            match p.kind {
                Kind::Spark => {
                    let a = camera.project(p.pos, canvas);
                    let b = camera.project(p.pos - p.vel * 0.035, canvas);
                    g.line(a, b, p.size * (0.5 + k), p.color.with_alpha(k.powf(0.7)));
                }
                Kind::Dust => {
                    let c = camera.project(p.pos, canvas);
                    let edge = camera.project(p.pos + vec3(p.size, 0.0, 0.0), canvas);
                    let r = (edge.x - c.x).abs().max(2.0);
                    let alpha = 0.32 * k * (1.0 - k).min(0.25) * 4.0;
                    g.blob(rect(c.x - r, c.y - r, r * 2.0, r * 2.0), p.color.with_alpha(alpha));
                }
                Kind::Ring => {
                    let radius = p.size * (1.0 - k * k).max(0.05);
                    let segments = 28;
                    for j in 0..segments {
                        let a0 = j as f32 * std::f32::consts::TAU / segments as f32;
                        let a1 = (j + 1) as f32 * std::f32::consts::TAU / segments as f32;
                        let pa = camera.project(p.pos + vec3(a0.cos(), 0.0, a0.sin() * 0.6) * radius, canvas);
                        let pb = camera.project(p.pos + vec3(a1.cos(), 0.0, a1.sin() * 0.6) * radius, canvas);
                        g.line(pa, pb, 2.5 * k + 0.5, p.color.with_alpha(k));
                    }
                }
            }
        }
    }
}
