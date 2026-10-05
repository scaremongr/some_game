//! Спрайтовый батчер поверх miniquad.
//!
//! Весь кадр рисуется одним draw call: всё лежит в одном атласе, вершины
//! копятся в динамический буфер и отправляются на GPU один раз за кадр.

use miniquad::*;

use super::font;
use super::math::*;
use super::math3::Mat4;
use super::render3d::{Camera, Lighting, Renderer3D, SkinnedMesh};

/// Короткая сторона виртуального холста. Она зафиксирована, а длинная
/// тянется по пропорциям окна.
///
/// Раньше холст был жёстко 640x360, и на вертикальном экране телефона
/// от него оставалась узкая полоса посреди чёрных полей. Теперь ландшафт
/// 16:9 даёт ровно прежние 640x360, а портрет — что-то вроде 360x760.
pub const DESIGN_SHORT_SIDE: f32 = 360.0;

/// Дальше этого соотношения холст не растягивается: на сверхшироком мониторе
/// интерфейс иначе расползётся по краям, а сцена окажется в середине пустоты.
const MAX_ASPECT: f32 = 2.2;

/// Размер холста для окна заданных пропорций.
pub fn canvas_for(screen: Vec2) -> Vec2 {
    if screen.x < 1.0 || screen.y < 1.0 {
        return vec2(DESIGN_SHORT_SIDE * 16.0 / 9.0, DESIGN_SHORT_SIDE);
    }
    let aspect = screen.x / screen.y;
    if aspect >= 1.0 {
        vec2(DESIGN_SHORT_SIDE * aspect.min(MAX_ASPECT), DESIGN_SHORT_SIDE)
    } else {
        vec2(DESIGN_SHORT_SIDE, DESIGN_SHORT_SIDE / aspect.max(1.0 / MAX_ASPECT))
    }
}

const MAX_QUADS: usize = 8192;
const MAX_VERTS: usize = MAX_QUADS * 4;
const MAX_INDICES: usize = MAX_QUADS * 6;

const ATLAS_W: usize = 128;
const ATLAS_H: usize = 64;
/// Ячейка глифа в атласе: 5x7 плюс 1px промежуток, чтобы не текло соседнее.
const CELL_W: usize = font::GLYPH_W + 1;
const CELL_H: usize = font::GLYPH_H + 1;
/// Глифы кладём сеткой 16 в ряд, а не «сколько влезет»: так они занимают
/// ровно левые 96x48 пикселей, а правый верхний угол остаётся под градиент.
const COLS: usize = 16;
const GLYPH_AREA_W: usize = COLS * CELL_W;
const GLYPH_AREA_H: usize = font::CHAR_COUNT.div_ceil(COLS) * CELL_H;
/// Координаты сплошного белого пикселя — им рисуются все заливки.
const WHITE_PX: (usize, usize) = (ATLAS_W - 4, ATLAS_H - 4);
/// Радиальный градиент в свободном углу атласа: мягкие тени, свечения, блики.
const BLOB_ORIGIN: (usize, usize) = (96, 0);
const BLOB_SIZE: usize = 32;

// Раскладка атласа проверяется на этапе компиляции: сдвинув любую из
// областей, ошибку хочется увидеть сразу, а не мусором на экране.
const _: () = {
    assert!(GLYPH_AREA_W <= BLOB_ORIGIN.0);
    assert!(BLOB_ORIGIN.0 + BLOB_SIZE <= ATLAS_W);
    assert!(BLOB_ORIGIN.1 + BLOB_SIZE <= ATLAS_H);
    assert!(GLYPH_AREA_H <= WHITE_PX.1);
    assert!(WHITE_PX.0 + 2 <= ATLAS_W && WHITE_PX.1 + 2 <= ATLAS_H);
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// Letterbox: как виртуальный холст ложится на физическое окно.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub screen: Vec2,
    pub canvas: Vec2,
    pub scale: f32,
    pub offset: Vec2,
}

impl Viewport {
    fn compute(screen: Vec2) -> Viewport {
        let canvas = canvas_for(screen);
        let scale = (screen.x / canvas.x).min(screen.y / canvas.y).max(0.001);
        let used = vec2(canvas.x * scale, canvas.y * scale);
        Viewport {
            screen,
            canvas,
            scale,
            offset: vec2((screen.x - used.x) * 0.5, (screen.y - used.y) * 0.5),
        }
    }

    /// Физические пиксели окна -> виртуальные координаты холста.
    pub fn to_virtual(&self, p: Vec2) -> Vec2 {
        vec2(
            (p.x - self.offset.x) / self.scale,
            (p.y - self.offset.y) / self.scale,
        )
    }
}

pub struct Graphics {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    bindings: Bindings,
    verts: Vec<Vertex>,
    quads: usize,
    r3d: Renderer3D,
    pub view: Viewport,
}

impl Graphics {
    pub fn new() -> Graphics {
        let mut ctx = window::new_rendering_backend();

        let vertex_buffer = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Stream,
            BufferSource::empty::<Vertex>(MAX_VERTS),
        );

        // Индексы квадов постоянны, поэтому строятся один раз и навсегда.
        let mut indices: Vec<u16> = Vec::with_capacity(MAX_INDICES);
        for q in 0..MAX_QUADS {
            let b = (q * 4) as u16;
            indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        let index_buffer = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );

        let atlas = build_atlas();
        let texture = ctx.new_texture_from_rgba8(ATLAS_W as u16, ATLAS_H as u16, &atlas);
        ctx.texture_set_filter(texture, FilterMode::Nearest, MipmapFilterMode::None);

        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: VERTEX_SHADER,
                    fragment: FRAGMENT_SHADER,
                },
                ShaderMeta {
                    images: vec!["tex".to_string()],
                    uniforms: UniformBlockLayout {
                        uniforms: vec![UniformDesc::new("mvp", UniformType::Mat4)],
                    },
                },
            )
            .expect("shader compilation failed");

        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
                VertexAttribute::new("in_color", VertexFormat::Float4),
            ],
            shader,
            PipelineParams {
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                ..Default::default()
            },
        );

        let r3d = Renderer3D::new(&mut *ctx);

        Graphics {
            ctx,
            pipeline,
            bindings: Bindings {
                vertex_buffers: vec![vertex_buffer],
                index_buffer,
                images: vec![texture],
            },
            verts: Vec::with_capacity(MAX_VERTS),
            quads: 0,
            r3d,
            view: Viewport::compute({
                let (w, h) = window::screen_size();
                vec2(w, h)
            }),
        }
    }

    pub fn begin_frame(&mut self, letterbox: Color, background: Color) {
        let (sw, sh) = window::screen_size();
        self.view = Viewport::compute(vec2(sw, sh));

        // Сначала заливаем всё окно цветом полей. Глубину чистим здесь же:
        // без этого 3D-персонаж со второго кадра начнёт отбраковываться
        // по мусору в буфере.
        self.ctx.begin_default_pass(PassAction::Clear {
            color: Some((letterbox.r, letterbox.g, letterbox.b, letterbox.a)),
            depth: Some(1.0),
            // Трафарет нужен плоской тени, чтобы не накладываться сама
            // на себя; без очистки она пропадёт со второго кадра.
            stencil: Some(0),
        });

        // ...затем ужимаем область вывода до виртуального холста.
        let (x, y, w, h) = self.viewport_rect_px();
        self.ctx.apply_viewport(x, y, w, h);
        self.ctx.apply_scissor_rect(x, y, w, h);
        self.ctx.apply_pipeline(&self.pipeline);
        self.ctx.apply_bindings(&self.bindings);

        self.verts.clear();
        self.quads = 0;
        let canvas = self.view.canvas;
        self.rect(rect(0.0, 0.0, canvas.x, canvas.y), background);
    }

    pub fn end_frame(&mut self) {
        self.flush();
        self.ctx.end_render_pass();
        self.ctx.commit_frame();
    }

    /// Размер виртуального холста в его собственных координатах.
    pub fn canvas(&self) -> Vec2 {
        self.view.canvas
    }

    /// true, когда окно вытянуто по вертикали: раскладка интерфейса в этом
    /// случае другая.
    pub fn is_portrait(&self) -> bool {
        self.view.canvas.y > self.view.canvas.x
    }

    /// Прямоугольник холста в физических пикселях. GL считает y снизу вверх.
    fn viewport_rect_px(&self) -> (i32, i32, i32, i32) {
        let w = (self.view.canvas.x * self.view.scale).round() as i32;
        let h = (self.view.canvas.y * self.view.scale).round() as i32;
        let x = self.view.offset.x.round() as i32;
        let y = (self.view.screen.y - self.view.offset.y).round() as i32 - h;
        (x, y, w, h)
    }

    fn flush(&mut self) {
        if self.quads == 0 {
            return;
        }
        self.ctx.buffer_update(
            self.bindings.vertex_buffers[0],
            BufferSource::slice(&self.verts),
        );
        self.ctx.apply_uniforms(UniformsSource::table(&Uniforms {
            mvp: ortho(self.view.canvas.x, self.view.canvas.y),
        }));
        self.ctx.draw(0, (self.quads * 6) as i32, 1);
        self.verts.clear();
        self.quads = 0;
    }

    /// Базовый примитив: кусок атласа (в пикселях атласа) -> прямоугольник холста.
    pub fn quad(&mut self, dst: Rect, src: Rect, color: Color) {
        if self.quads >= MAX_QUADS {
            self.flush();
        }
        let c = [color.r, color.g, color.b, color.a];
        let (u0, v0) = (src.x / ATLAS_W as f32, src.y / ATLAS_H as f32);
        let (u1, v1) = (
            (src.x + src.w) / ATLAS_W as f32,
            (src.y + src.h) / ATLAS_H as f32,
        );
        let (x0, y0, x1, y1) = (dst.x, dst.y, dst.x + dst.w, dst.y + dst.h);
        self.verts.push(Vertex { pos: [x0, y0], uv: [u0, v0], color: c });
        self.verts.push(Vertex { pos: [x1, y0], uv: [u1, v0], color: c });
        self.verts.push(Vertex { pos: [x1, y1], uv: [u1, v1], color: c });
        self.verts.push(Vertex { pos: [x0, y1], uv: [u0, v1], color: c });
        self.quads += 1;
    }

    pub fn rect(&mut self, r: Rect, color: Color) {
        let (wx, wy) = (WHITE_PX.0 as f32 + 0.5, WHITE_PX.1 as f32 + 0.5);
        self.quad(r, rect(wx, wy, 1.0, 1.0), color);
    }

    /// A line is one rotated quad in the existing sprite batch.
    pub fn line(&mut self, a: Vec2, b: Vec2, width: f32, color: Color) {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let length = (dx * dx + dy * dy).sqrt();
        if length < 0.001 { return; }
        if self.quads >= MAX_QUADS { self.flush(); }
        let nx = -dy / length * width * 0.5;
        let ny = dx / length * width * 0.5;
        let uv = [(WHITE_PX.0 as f32 + 0.5) / ATLAS_W as f32,
                  (WHITE_PX.1 as f32 + 0.5) / ATLAS_H as f32];
        let color = [color.r, color.g, color.b, color.a];
        for pos in [[a.x + nx, a.y + ny], [b.x + nx, b.y + ny],
                    [b.x - nx, b.y - ny], [a.x - nx, a.y - ny]] {
            self.verts.push(Vertex { pos, uv, color });
        }
        self.quads += 1;
    }

    /// Мягкое радиальное пятно: тень под персонажем, свечение, вспышка доли.
    pub fn blob(&mut self, r: Rect, color: Color) {
        self.quad(
            r,
            rect(
                BLOB_ORIGIN.0 as f32,
                BLOB_ORIGIN.1 as f32,
                BLOB_SIZE as f32,
                BLOB_SIZE as f32,
            ),
            color,
        );
    }

    pub fn rect_outline(&mut self, r: Rect, t: f32, color: Color) {
        self.rect(rect(r.x, r.y, r.w, t), color);
        self.rect(rect(r.x, r.y + r.h - t, r.w, t), color);
        self.rect(rect(r.x, r.y + t, t, r.h - t * 2.0), color);
        self.rect(rect(r.x + r.w - t, r.y + t, t, r.h - t * 2.0), color);
    }

    /// Контекст нужен сценам, чтобы один раз залить меш на GPU.
    pub fn ctx(&mut self) -> &mut dyn RenderingBackend {
        &mut *self.ctx
    }

    /// Контекст вместе с белой заглушкой: в таком виде сцена может залить
    /// меш с материалами, не зная про устройство 3D-конвейера.
    pub fn upload_skinned(
        &mut self,
        data: &super::mesh3::MeshData,
        textures: &[super::gltf::TextureData],
        materials: &[super::gltf::MaterialData],
    ) -> SkinnedMesh {
        let white = self.r3d.white_texture();
        SkinnedMesh::upload_with_materials(&mut *self.ctx, data, textures, materials, white)
    }

    /// Рисует скиннинговый меш между 2D-слоями: накопленные квады сначала
    /// уходят на экран, после 3D-прохода 2D-состояние восстанавливается.
    ///
    /// `shadow` — плотность плоской тени на полу; 0 отключает её.
    pub fn draw_skinned(
        &mut self,
        mesh: &SkinnedMesh,
        skin: &[Mat4],
        model: Mat4,
        camera: &Camera,
        light: &Lighting,
        shadow: f32,
    ) {
        self.flush();

        let aspect = self.view.canvas.x / self.view.canvas.y;
        if shadow > 0.0 {
            self.r3d.draw_shadow(
                &mut *self.ctx,
                mesh,
                skin,
                model,
                camera,
                aspect,
                light,
                shadow,
            );
        }
        self.r3d
            .draw(&mut *self.ctx, mesh, skin, model, camera, aspect, light);

        self.ctx.apply_pipeline(&self.pipeline);
        self.ctx.apply_bindings(&self.bindings);
    }

    /// Uploads textures once, for meshes that share them.
    pub fn upload_textures(&mut self, textures: &[super::gltf::TextureData]) -> Vec<miniquad::TextureId> {
        super::render3d::upload_textures(&mut *self.ctx, textures)
    }

    /// A mesh whose materials point into already uploaded `textures`.
    pub fn upload_shared(
        &mut self,
        data: &super::mesh3::MeshData,
        materials: &[super::gltf::MaterialData],
        textures: &[miniquad::TextureId],
    ) -> SkinnedMesh {
        let white = self.r3d.white_texture();
        SkinnedMesh::upload_with_texture_ids(&mut *self.ctx, data, materials, textures, white)
    }

    /// Baked room geometry (see `Renderer3D::draw_baked`).
    pub fn draw_baked(
        &mut self,
        mesh: &SkinnedMesh,
        skin: &[Mat4],
        camera: &Camera,
        exposure: f32,
        glass: Option<f32>,
        lift: f32,
    ) {
        self.flush();
        let aspect = self.view.canvas.x / self.view.canvas.y;
        self.r3d
            .draw_baked(&mut *self.ctx, mesh, skin, Mat4::IDENTITY, camera, aspect, exposure, glass, lift);
        self.ctx.apply_pipeline(&self.pipeline);
        self.ctx.apply_bindings(&self.bindings);
    }

    /// A picture on a flat quad in the 3D scene (see `Renderer3D::draw_sprite`).
    pub fn draw_sprite(&mut self, texture: miniquad::TextureId, model: Mat4, camera: &Camera, tint: [f32; 4], far: f32) {
        self.flush();
        let aspect = self.view.canvas.x / self.view.canvas.y;
        self.r3d.draw_sprite(&mut *self.ctx, texture, model, camera, aspect, tint, far);
        self.ctx.apply_pipeline(&self.pipeline);
        self.ctx.apply_bindings(&self.bindings);
    }

    /// An RGBA8 picture for `draw_sprite`: linear filtering, clamped edges.
    pub fn upload_rgba(&mut self, width: u32, height: u32, rgba: &[u8]) -> miniquad::TextureId {
        self.ctx.new_texture_from_data_and_format(
            rgba,
            TextureParams {
                format: TextureFormat::RGBA8,
                wrap: TextureWrap::Clamp,
                min_filter: FilterMode::Linear,
                mag_filter: FilterMode::Linear,
                mipmap_filter: MipmapFilterMode::None,
                width,
                height,
                ..Default::default()
            },
        )
    }

    pub fn delete_texture(&mut self, texture: miniquad::TextureId) {
        self.ctx.delete_texture(texture);
    }

    pub fn text(&mut self, s: &str, pos: Vec2, scale: f32, color: Color) {
        let mut pen = pos.x;
        for ch in s.chars() {
            if ch != ' ' {
                let (cx, cy) = glyph_cell(ch);
                self.quad(
                    rect(
                        pen,
                        pos.y,
                        font::GLYPH_W as f32 * scale,
                        font::GLYPH_H as f32 * scale,
                    ),
                    rect(
                        cx as f32,
                        cy as f32,
                        font::GLYPH_W as f32,
                        font::GLYPH_H as f32,
                    ),
                    color,
                );
            }
            pen += font::ADVANCE as f32 * scale;
        }
    }

    pub fn text_width(&self, s: &str, scale: f32) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        // Последний символ не тянет за собой межбуквенный промежуток.
        (s.chars().count() as f32 * font::ADVANCE as f32 - 1.0) * scale
    }

    pub fn text_centered(&mut self, s: &str, center: Vec2, scale: f32, color: Color) {
        let w = self.text_width(s, scale);
        let h = font::GLYPH_H as f32 * scale;
        self.text(
            s,
            vec2((center.x - w * 0.5).round(), (center.y - h * 0.5).round()),
            scale,
            color,
        );
    }
}

#[repr(C)]
struct Uniforms {
    mvp: [f32; 16],
}

fn glyph_cell(ch: char) -> (usize, usize) {
    let code = if (ch as u32) >= font::FIRST_CHAR as u32 && (ch as u32) <= font::LAST_CHAR as u32 {
        ch as u32 as u8
    } else {
        b'?'
    };
    let idx = (code - font::FIRST_CHAR) as usize;
    ((idx % COLS) * CELL_W, (idx / COLS) * CELL_H)
}

/// Атлас собирается в рантайме из битовых масок шрифта — никаких файлов.
fn build_atlas() -> Vec<u8> {
    let mut px = vec![0u8; ATLAS_W * ATLAS_H * 4];

    let put = |px: &mut Vec<u8>, x: usize, y: usize| {
        let i = (y * ATLAS_W + x) * 4;
        px[i] = 255;
        px[i + 1] = 255;
        px[i + 2] = 255;
        px[i + 3] = 255;
    };

    for idx in 0..font::CHAR_COUNT {
        let ch = (font::FIRST_CHAR as usize + idx) as u8 as char;
        let bits = font::glyph(ch);
        let (cx, cy) = ((idx % COLS) * CELL_W, (idx / COLS) * CELL_H);
        for (col, mask) in bits.iter().enumerate() {
            for row in 0..font::GLYPH_H {
                if mask >> row & 1 == 1 {
                    put(&mut px, cx + col, cy + row);
                }
            }
        }
    }

    // Радиальный градиент: в центре непрозрачный, к краю сходит в ноль.
    let radius = BLOB_SIZE as f32 * 0.5;
    for y in 0..BLOB_SIZE {
        for x in 0..BLOB_SIZE {
            let dx = (x as f32 + 0.5 - radius) / radius;
            let dy = (y as f32 + 0.5 - radius) / radius;
            let d = (dx * dx + dy * dy).sqrt().min(1.0);
            let falloff = 1.0 - d;
            // В квадрате спад мягче у края и плотнее в центре.
            let alpha = (falloff * falloff * 255.0) as u8;
            let i = ((BLOB_ORIGIN.1 + y) * ATLAS_W + BLOB_ORIGIN.0 + x) * 4;
            px[i] = 255;
            px[i + 1] = 255;
            px[i + 2] = 255;
            px[i + 3] = alpha;
        }
    }

    // Блок сплошного белого 2x2 — источник цвета для всех заливок.
    for dy in 0..2 {
        for dx in 0..2 {
            put(&mut px, WHITE_PX.0 + dx, WHITE_PX.1 + dy);
        }
    }

    px
}

// Без `#version`: десктопный GL берёт GLSL 110, WebGL1 — GLSL ES 100.
// Обе версии понимают attribute/varying/texture2D, но квалификаторы точности
// есть только в ES — отсюда guard по GL_ES.
const VERTEX_SHADER: &str = r#"
attribute vec2 in_pos;
attribute vec2 in_uv;
attribute vec4 in_color;

varying vec2 v_uv;
varying vec4 v_color;

uniform mat4 mvp;

void main() {
    gl_Position = mvp * vec4(in_pos, 0.0, 1.0);
    v_uv = in_uv;
    v_color = in_color;
}
"#;

const FRAGMENT_SHADER: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif

varying vec2 v_uv;
varying vec4 v_color;

uniform sampler2D tex;

void main() {
    gl_FragColor = texture2D(tex, v_uv) * v_color;
}
"#;
