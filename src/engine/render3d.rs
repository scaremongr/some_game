//! Конвейер отрисовки скиннингового меша.
//!
//! Скиннинг считается в вершинном шейдере: матрицы костей приезжают
//! униформ-массивом, каждая — тремя vec4 (аффинное преобразование, четвёртая
//! строка всегда 0,0,0,1). Так в бюджет WebGL 1 влезает вдвое больше костей,
//! чем при передаче полными mat4.

use miniquad::*;

use super::gltf::{AlphaMode, MaterialData, TextureData};
use super::math::{Color, Vec2};
use super::math3::*;
use super::mesh3::MeshData;
use super::skeleton::MAX_BONES;

/// Кусок меша, готовый к отрисовке: диапазон индексов плюс уже загруженная
/// текстура и параметры материала.
struct DrawCall {
    first_index: i32,
    index_count: i32,
    texture: TextureId,
    alpha_cutoff: f32,
    double_sided: bool,
}

pub struct SkinnedMesh {
    vertex_buffer: BufferId,
    index_buffer: BufferId,
    index_count: i32,
    calls: Vec<DrawCall>,
}

impl SkinnedMesh {
    pub fn upload(ctx: &mut dyn RenderingBackend, data: &MeshData) -> SkinnedMesh {
        let vertex_buffer = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&data.verts),
        );

        // 16-битные индексы предпочтительнее: 32-битные в WebGL 1 требуют
        // расширения OES_element_index_uint, которое есть не везде.
        let index_buffer = if data.verts.len() <= u16::MAX as usize + 1 {
            let narrow: Vec<u16> = data.indices.iter().map(|&i| i as u16).collect();
            ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&narrow),
            )
        } else {
            ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&data.indices),
            )
        };

        SkinnedMesh {
            vertex_buffer,
            index_buffer,
            index_count: data.indices.len() as i32,
            calls: Vec::new(),
        }
    }

    /// Загружает меш вместе с текстурами и материалами.
    ///
    /// Материалов у персонажа обычно несколько, а текстура в конвейере одна,
    /// поэтому меш режется на куски и рисуется по одному вызову на материал.
    pub fn upload_with_materials(
        ctx: &mut dyn RenderingBackend,
        data: &MeshData,
        textures: &[TextureData],
        materials: &[MaterialData],
        white: TextureId,
    ) -> SkinnedMesh {
        let uploaded = upload_textures(ctx, textures);
        SkinnedMesh::upload_with_texture_ids(ctx, data, materials, &uploaded, white)
    }

    /// Like `upload_with_materials`, with textures already on the GPU (so
    /// several meshes can share them).
    pub fn upload_with_texture_ids(
        ctx: &mut dyn RenderingBackend,
        data: &MeshData,
        materials: &[MaterialData],
        uploaded: &[TextureId],
        white: TextureId,
    ) -> SkinnedMesh {
        let mut mesh = SkinnedMesh::upload(ctx, data);

        mesh.calls = data
            .submeshes
            .iter()
            .map(|sub| {
                let material = materials.get(sub.material);
                let texture = material
                    .and_then(|m| m.texture)
                    .and_then(|i| uploaded.get(i).copied())
                    .unwrap_or(white);

                DrawCall {
                    first_index: sub.first_index as i32,
                    index_count: sub.index_count as i32,
                    texture,
                    // Непрозрачный материал не должен ничего отбрасывать,
                    // поэтому порог уводим ниже любого возможного значения.
                    alpha_cutoff: match material.map(|m| m.alpha_mode) {
                        Some(AlphaMode::Mask) => material.map_or(0.5, |m| m.alpha_cutoff),
                        _ => -1.0,
                    },
                    double_sided: material.is_some_and(|m| m.double_sided),
                }
            })
            .collect();

        mesh
    }

    pub fn triangle_count(&self) -> i32 {
        self.index_count / 3
    }
}

/// Uploads textures once for meshes that share them.
pub fn upload_textures(ctx: &mut dyn RenderingBackend, textures: &[TextureData]) -> Vec<TextureId> {
    textures.iter().map(|t| upload_texture(ctx, t)).collect()
}

/// Заливает картинку на GPU. Мипмапы включаются только для сторон, кратных
/// степени двойки: WebGL 1 других не мипмапит и молча рисует чёрное.
fn upload_texture(ctx: &mut dyn RenderingBackend, t: &TextureData) -> TextureId {
    let pot = t.width.is_power_of_two() && t.height.is_power_of_two();

    let id = ctx.new_texture_from_data_and_format(
        &t.rgba,
        TextureParams {
            format: TextureFormat::RGBA8,
            wrap: if pot { TextureWrap::Repeat } else { TextureWrap::Clamp },
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mipmap_filter: if pot {
                MipmapFilterMode::Linear
            } else {
                MipmapFilterMode::None
            },
            width: t.width,
            height: t.height,
            allocate_mipmaps: pot,
            ..Default::default()
        },
    );
    if pot {
        ctx.texture_generate_mipmaps(id);
    }
    id
}

/// Камера с перспективой. Умеет проецировать точку мира в координаты
/// виртуального холста — так 2D-слой (тень, подписи) попадает туда же,
/// где стоит персонаж.
#[derive(Clone, Copy)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    /// Off-centre projection in NDC: the look-at target lands at `shift`
    /// instead of the screen centre (frames the arena above touch controls).
    pub shift: Vec2,
}

impl Camera {
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        Mat4::translation(vec3(self.shift.x, self.shift.y, 0.0))
            * Mat4::perspective(self.fov_y, aspect, self.near, self.far)
            * Mat4::look_at(self.eye, self.target, Vec3::Y)
    }

    /// Экранная высота линии горизонта плоскости y = 0. Нужна, чтобы 2D-пол
    /// сходился ровно туда же, куда уходит 3D-земля: расхождение сразу читается
    /// как «персонаж висит в воздухе».
    pub fn horizon_y(&self, canvas: Vec2) -> f32 {
        // Точка на земле достаточно далеко, чтобы совпасть с точкой схода.
        let forward = (self.target - self.eye).normalize();
        let far = vec3(self.eye.x + forward.x * 4000.0, 0.0, self.eye.z + forward.z * 4000.0);
        self.project(far, canvas).y
    }

    pub fn project(&self, world: Vec3, canvas: Vec2) -> Vec2 {
        let vp = self.view_proj(canvas.x / canvas.y);
        let m = vp.0;
        let (x, y, z) = (world.x, world.y, world.z);
        let clip_x = m[0] * x + m[4] * y + m[8] * z + m[12];
        let clip_y = m[1] * x + m[5] * y + m[9] * z + m[13];
        let clip_w = m[3] * x + m[7] * y + m[11] * z + m[15];
        if clip_w.abs() < 1e-6 {
            return Vec2::ZERO;
        }
        let ndc = (clip_x / clip_w, clip_y / clip_w);
        Vec2 {
            x: (ndc.0 * 0.5 + 0.5) * canvas.x,
            // NDC растёт вверх, холст — вниз.
            y: (1.0 - (ndc.1 * 0.5 + 0.5)) * canvas.y,
        }
    }
}

/// Трёхточечная схема света: ключевой, заполняющий и контровой.
/// Контровой особенно важен — он отделяет силуэт от тёмного фона.
#[derive(Clone, Copy)]
pub struct Lighting {
    pub key_dir: Vec3,
    pub key_color: Color,
    /// Сила бликов, кладётся в альфу ключевого света.
    pub specular: f32,
    pub fill_dir: Vec3,
    pub fill_color: Color,
    pub rim_color: Color,
    pub rim_strength: f32,
    /// Откуда падает тень. Отдельно от ключевого света: тот выгодно ставить
    /// сбоку ради объёма, а тень от бокового источника растягивается через
    /// весь пол.
    pub shadow_dir: Vec3,
}

impl Default for Lighting {
    fn default() -> Lighting {
        Lighting {
            key_dir: vec3(-0.4, 0.85, 0.55),
            key_color: Color::hex(0xFFF1DC),
            specular: 0.12,
            fill_dir: vec3(0.7, 0.15, -0.4),
            fill_color: Color::hex(0x2A3358),
            rim_color: Color::hex(0x8AA0FF),
            // Раньше стояло 0.9, и контровой забеливал всё подряд: тёмные
            // волосы светились ярче, чем кожа под ключевым светом.
            rim_strength: 0.35,
            shadow_dir: vec3(-0.18, 1.0, 0.28),
        }
    }
}

#[repr(C)]
struct Uniforms3 {
    view_proj: [f32; 16],
    model: [f32; 16],
    camera_pos: [f32; 4],
    key_dir: [f32; 4],
    key_color: [f32; 4],
    fill_dir: [f32; 4],
    fill_color: [f32; 4],
    rim_color: [f32; 4],
    /// x — порог отсечения по альфе, остальное про запас.
    material: [f32; 4],
    bones: [[f32; 4]; MAX_BONES * 3],
}

pub struct Renderer3D {
    pipeline: Pipeline,
    /// Тот же конвейер без отсечения задних граней: волосы и ткань часто
    /// сделаны односторонними плоскостями и без него исчезают с изнанки.
    pipeline_double_sided: Pipeline,
    /// Для зеркальной матрицы модели (отражённый боец): порядок обхода
    /// треугольников меняется, и отсекать надо уже передние грани.
    pipeline_mirrored: Pipeline,
    /// Конвейер плоской тени: та же геометрия, сплющенная на пол.
    pipeline_shadow: Pipeline,
    /// Baked room surfaces: light is already in the texture, so the colour
    /// is shown as is. Both faces are drawn: broken pieces can show insides.
    pipeline_baked: Pipeline,
    /// Window glass: tinted and see-through, blended over what is behind.
    pipeline_glass: Pipeline,
    /// Заглушка 1x1 для материалов без текстуры: так в шейдере не нужна
    /// ветка «есть текстура / нет текстуры».
    white: TextureId,
    uniforms: Box<Uniforms3>,
}

impl Renderer3D {
    pub fn white_texture(&self) -> TextureId {
        self.white
    }
}

impl Renderer3D {
    pub fn new(ctx: &mut dyn RenderingBackend) -> Renderer3D {
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: &vertex_shader(),
                    fragment: FRAGMENT_SHADER,
                },
                ShaderMeta {
                    images: vec!["base_color".to_string()],
                    uniforms: UniformBlockLayout {
                        uniforms: uniform_layout(),
                    },
                },
            )
            .expect("3d shader compilation failed");

        let attributes = [
            VertexAttribute::new("in_pos", VertexFormat::Float3),
            VertexAttribute::new("in_normal", VertexFormat::Float3),
            VertexAttribute::new("in_uv", VertexFormat::Float2),
            VertexAttribute::new("in_color", VertexFormat::Float3),
            VertexAttribute::new("in_joints", VertexFormat::Float4),
            VertexAttribute::new("in_weights", VertexFormat::Float4),
        ];
        let params = PipelineParams {
            depth_test: Comparison::LessOrEqual,
            depth_write: true,
            cull_face: CullFace::Back,
            ..Default::default()
        };

        let pipeline = ctx.new_pipeline(&[BufferLayout::default()], &attributes, shader, params);
        let pipeline_double_sided = ctx.new_pipeline(
            &[BufferLayout::default()],
            &attributes,
            shader,
            PipelineParams { cull_face: CullFace::Nothing, ..params },
        );
        let pipeline_mirrored = ctx.new_pipeline(
            &[BufferLayout::default()],
            &attributes,
            shader,
            PipelineParams { cull_face: CullFace::Front, ..params },
        );

        // Тень использует тот же вершинный шейдер (тот же скиннинг), но
        // выводит плоский цвет: форму даёт проекция, а не освещение.
        let shadow_shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: &vertex_shader(),
                    fragment: SHADOW_FRAGMENT_SHADER,
                },
                ShaderMeta {
                    images: vec!["base_color".to_string()],
                    uniforms: UniformBlockLayout {
                        uniforms: uniform_layout(),
                    },
                },
            )
            .expect("shadow shader compilation failed");

        let pipeline_shadow = ctx.new_pipeline(
            &[BufferLayout::default()],
            &attributes,
            shadow_shader,
            PipelineParams {
                depth_test: Comparison::LessOrEqual,
                // Тень не пишет глубину: иначе она перекрыла бы сама себя
                // и персонажа, стоящего на ней.
                depth_write: false,
                cull_face: CullFace::Nothing,
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                // Трафарет пропускает каждый пиксель ровно один раз.
                // Без него тень темнеет там, где силуэт накладывается сам
                // на себя — руки над корпусом давали грязные пятна.
                stencil_test: Some(StencilState {
                    front: SHADOW_STENCIL,
                    back: SHADOW_STENCIL,
                }),
                ..Default::default()
            },
        );

        let baked_shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: &vertex_shader(),
                    fragment: BAKED_FRAGMENT_SHADER,
                },
                ShaderMeta {
                    images: vec!["base_color".to_string()],
                    uniforms: UniformBlockLayout {
                        uniforms: uniform_layout(),
                    },
                },
            )
            .expect("baked shader compilation failed");
        let pipeline_baked = ctx.new_pipeline(
            &[BufferLayout::default()],
            &attributes,
            baked_shader,
            PipelineParams { cull_face: CullFace::Nothing, ..params },
        );
        let pipeline_glass = ctx.new_pipeline(
            &[BufferLayout::default()],
            &attributes,
            baked_shader,
            PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: false,
                cull_face: CullFace::Nothing,
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                ..Default::default()
            },
        );

        let white = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);

        Renderer3D {
            pipeline,
            pipeline_double_sided,
            pipeline_mirrored,
            pipeline_shadow,
            pipeline_baked,
            pipeline_glass,
            white,
            // На стеке эта структура — десятки килобайт, что для wasm-потока
            // уже ощутимо.
            uniforms: Box::new(Uniforms3 {
                view_proj: Mat4::IDENTITY.0,
                model: Mat4::IDENTITY.0,
                camera_pos: [0.0; 4],
                key_dir: [0.0; 4],
                key_color: [0.0; 4],
                fill_dir: [0.0; 4],
                fill_color: [0.0; 4],
                rim_color: [0.0; 4],
                material: [0.0; 4],
                bones: [[0.0; 4]; MAX_BONES * 3],
            }),
        }
    }

    /// Рисует плоскую тень персонажа. Вызывается до самого персонажа.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_shadow(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        mesh: &SkinnedMesh,
        skin: &[Mat4],
        model: Mat4,
        camera: &Camera,
        aspect: f32,
        light: &Lighting,
        opacity: f32,
    ) {
        let u = &mut self.uniforms;
        u.view_proj = camera.view_proj(aspect).0;
        u.model = (shadow_projection(light.shadow_dir) * model).0;
        u.material = [-1.0, opacity, 0.0, 0.0];

        for (i, m) in skin.iter().enumerate() {
            let rows = m.to_rows3();
            u.bones[i * 3] = rows[0];
            u.bones[i * 3 + 1] = rows[1];
            u.bones[i * 3 + 2] = rows[2];
        }

        ctx.apply_pipeline(&self.pipeline_shadow);
        ctx.apply_bindings(&Bindings {
            vertex_buffers: vec![mesh.vertex_buffer],
            index_buffer: mesh.index_buffer,
            images: vec![self.white],
        });
        ctx.apply_uniforms(UniformsSource::table(&**u));
        ctx.draw(0, mesh.index_count, 1);
    }

    pub fn draw(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        mesh: &SkinnedMesh,
        skin: &[Mat4],
        model: Mat4,
        camera: &Camera,
        aspect: f32,
        light: &Lighting,
    ) {
        debug_assert!(
            skin.len() <= MAX_BONES,
            "скелет не влезает в бюджет униформ"
        );

        let u = &mut self.uniforms;
        u.view_proj = camera.view_proj(aspect).0;
        u.model = model.0;
        u.camera_pos = [camera.eye.x, camera.eye.y, camera.eye.z, 1.0];
        u.key_dir = pad(light.key_dir.normalize(), 0.0);
        u.key_color = color4(light.key_color, light.specular);
        u.fill_dir = pad(light.fill_dir.normalize(), 0.0);
        u.fill_color = color4(light.fill_color, 1.0);
        u.rim_color = color4(light.rim_color, light.rim_strength);

        // Хвост массива не обнуляем: вершины на эти кости не ссылаются.
        for (i, m) in skin.iter().enumerate() {
            let rows = m.to_rows3();
            u.bones[i * 3] = rows[0];
            u.bones[i * 3 + 1] = rows[1];
            u.bones[i * 3 + 2] = rows[2];
        }

        let m = model.0;
        let det = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[4] * (m[1] * m[10] - m[2] * m[9])
            + m[8] * (m[1] * m[6] - m[2] * m[5]);
        let single = if det < 0.0 { &self.pipeline_mirrored } else { &self.pipeline };

        // Меш без разбиения на материалы (процедурная заглушка) рисуется
        // одним вызовом с белой текстурой.
        if mesh.calls.is_empty() {
            u.material = [-1.0, 0.0, 0.0, 0.0];
            ctx.apply_pipeline(single);
            ctx.apply_bindings(&Bindings {
                vertex_buffers: vec![mesh.vertex_buffer],
                index_buffer: mesh.index_buffer,
                images: vec![self.white],
            });
            ctx.apply_uniforms(UniformsSource::table(&**u));
            ctx.draw(0, mesh.index_count, 1);
            return;
        }

        let mut current_double_sided = None;
        for call in &mesh.calls {
            if current_double_sided != Some(call.double_sided) {
                ctx.apply_pipeline(if call.double_sided {
                    &self.pipeline_double_sided
                } else {
                    single
                });
                current_double_sided = Some(call.double_sided);
            }

            ctx.apply_bindings(&Bindings {
                vertex_buffers: vec![mesh.vertex_buffer],
                index_buffer: mesh.index_buffer,
                images: vec![call.texture],
            });

            u.material = [call.alpha_cutoff, 0.0, 0.0, 0.0];
            ctx.apply_uniforms(UniformsSource::table(&**u));
            ctx.draw(call.first_index, call.index_count, 1);
        }
    }
}

impl Renderer3D {
    /// Draws baked room geometry (one matrix per piece in `skin`).
    /// `exposure` scales the baked light; `glass` blends with that alpha;
    /// `lift` raises dark faces (pieces that moved out of their baked light).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_baked(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        mesh: &SkinnedMesh,
        skin: &[Mat4],
        model: Mat4,
        camera: &Camera,
        aspect: f32,
        exposure: f32,
        glass: Option<f32>,
        lift: f32,
    ) {
        let u = &mut self.uniforms;
        u.view_proj = camera.view_proj(aspect).0;
        u.model = model.0;
        u.camera_pos = [camera.eye.x, camera.eye.y, camera.eye.z, 1.0];
        for (i, m) in skin.iter().enumerate() {
            let rows = m.to_rows3();
            u.bones[i * 3] = rows[0];
            u.bones[i * 3 + 1] = rows[1];
            u.bones[i * 3 + 2] = rows[2];
        }
        ctx.apply_pipeline(if glass.is_some() { &self.pipeline_glass } else { &self.pipeline_baked });
        let alpha = glass.unwrap_or(1.0);
        let calls: Vec<(i32, i32, TextureId, bool)> = if mesh.calls.is_empty() {
            vec![(0, mesh.index_count, self.white, false)]
        } else {
            mesh.calls
                .iter()
                .map(|c| (c.first_index, c.index_count, c.texture, c.texture != self.white))
                .collect()
        };
        for (first, count, texture, textured) in calls {
            ctx.apply_bindings(&Bindings {
                vertex_buffers: vec![mesh.vertex_buffer],
                index_buffer: mesh.index_buffer,
                images: vec![texture],
            });
            u.material = [lift, if textured { 1.0 } else { 0.0 }, exposure, alpha];
            ctx.apply_uniforms(UniformsSource::table(&**u));
            ctx.draw(first, count, 1);
        }
    }
}

/// Baked surfaces: the texture already holds light and tone (Blender's AgX
/// view transform), so it is shown as is, scaled by the room exposure.
/// Untextured pieces (glass) use their linear material colour.
const BAKED_FRAGMENT_SHADER: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif

varying vec3 v_normal;
varying vec3 v_world;
varying vec3 v_color;
varying vec2 v_uv;

uniform sampler2D base_color;
uniform vec4 material;
uniform vec4 camera_pos;
uniform vec4 key_dir;
uniform vec4 key_color;
uniform vec4 fill_dir;
uniform vec4 fill_color;
uniform vec4 rim_color;

void main() {
    vec3 c = material.y > 0.5
        ? texture2D(base_color, v_uv).rgb
        : pow(v_color, vec3(1.0 / 2.2));
    // material.x lifts shadowed faces of moved pieces towards room light.
    c = c * material.z + material.x * vec3(0.13, 0.11, 0.10) * (1.0 - c);
    gl_FragColor = vec4(c, material.w);
}
"#;

/// Порядок обязан совпадать с полями `Uniforms3`: miniquad читает структуру
/// как плоский поток байт.
fn uniform_layout() -> Vec<UniformDesc> {
    vec![
        UniformDesc::new("view_proj", UniformType::Mat4),
        UniformDesc::new("model", UniformType::Mat4),
        UniformDesc::new("camera_pos", UniformType::Float4),
        UniformDesc::new("key_dir", UniformType::Float4),
        UniformDesc::new("key_color", UniformType::Float4),
        UniformDesc::new("fill_dir", UniformType::Float4),
        UniformDesc::new("fill_color", UniformType::Float4),
        UniformDesc::new("rim_color", UniformType::Float4),
        UniformDesc::new("material", UniformType::Float4),
        UniformDesc::new("bones", UniformType::Float4).array(MAX_BONES * 3),
    ]
}

/// Матрица, сплющивающая геометрию на плоскость y = 0 вдоль направления
/// света. Классическая плоская проекция: точка съезжает по лучу до пола.
///
/// Даёт настоящий силуэт вместо пятна и стоит один лишний вызов отрисовки.
/// Расплата — в местах, где тень накладывается сама на себя (рука над
/// корпусом), она темнее; убирается это трафаретным буфером, которого в
/// вебе по умолчанию нет.
fn shadow_projection(light_dir: Vec3) -> Mat4 {
    let l = light_dir.normalize();
    // Свет почти вдоль пола растянул бы тень в бесконечность.
    let ly = l.y.max(0.25);
    Mat4([
        1.0, 0.0, 0.0, 0.0, //
        -l.x / ly, 0.0, -l.z / ly, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

fn pad(v: Vec3, w: f32) -> [f32; 4] {
    [v.x, v.y, v.z, w]
}

fn color4(c: Color, w: f32) -> [f32; 4] {
    [c.r, c.g, c.b, w]
}

fn vertex_shader() -> String {
    // Размер массива костей должен быть литералом, поэтому шейдер собирается
    // строкой из той же константы, что и Rust-сторона.
    format!(
        r#"
attribute vec3 in_pos;
attribute vec3 in_normal;
attribute vec2 in_uv;
attribute vec3 in_color;
attribute vec4 in_joints;
attribute vec4 in_weights;

uniform mat4 view_proj;
uniform mat4 model;
uniform vec4 bones[{bone_vec4}];

varying vec3 v_normal;
varying vec3 v_world;
varying vec3 v_color;
varying vec2 v_uv;

mat4 bone_matrix(int index) {{
    int i = index * 3;
    vec4 r0 = bones[i];
    vec4 r1 = bones[i + 1];
    vec4 r2 = bones[i + 2];
    return mat4(
        r0.x, r1.x, r2.x, 0.0,
        r0.y, r1.y, r2.y, 0.0,
        r0.z, r1.z, r2.z, 0.0,
        r0.w, r1.w, r2.w, 1.0);
}}

void main() {{
    mat4 skin = bone_matrix(int(in_joints.x)) * in_weights.x
              + bone_matrix(int(in_joints.y)) * in_weights.y
              + bone_matrix(int(in_joints.z)) * in_weights.z
              + bone_matrix(int(in_joints.w)) * in_weights.w;

    vec4 skinned = skin * vec4(in_pos, 1.0);
    vec4 world = model * skinned;

    // w = 0 отсекает перенос, поэтому нормаль поворачивается, но не съезжает.
    vec3 n = (model * (skin * vec4(in_normal, 0.0))).xyz;

    v_world = world.xyz;
    v_normal = n;
    v_color = in_color;
    v_uv = in_uv;

    gl_Position = view_proj * world;
}}
"#,
        bone_vec4 = MAX_BONES * 3
    )
}

/// Пиксель проходит, только если трафарет ещё не помечен, и сразу метится.
const SHADOW_STENCIL: StencilFaceState = StencilFaceState {
    fail_op: StencilOp::Keep,
    depth_fail_op: StencilOp::Keep,
    pass_op: StencilOp::Replace,
    test_func: CompareFunc::NotEqual,
    test_ref: 1,
    test_mask: 0xFF,
    write_mask: 0xFF,
};

/// Тень: плоский цвет с постоянной прозрачностью. material.y — плотность.
const SHADOW_FRAGMENT_SHADER: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif

varying vec3 v_normal;
varying vec3 v_world;
varying vec3 v_color;
varying vec2 v_uv;

uniform sampler2D base_color;
uniform vec4 material;
uniform vec4 camera_pos;
uniform vec4 key_dir;
uniform vec4 key_color;
uniform vec4 fill_dir;
uniform vec4 fill_color;
uniform vec4 rim_color;

void main() {
    gl_FragColor = vec4(0.0, 0.0, 0.0, material.y);
}
"#;

const FRAGMENT_SHADER: &str = r#"
#ifdef GL_ES
precision mediump float;
#endif

varying vec3 v_normal;
varying vec3 v_world;
varying vec3 v_color;
varying vec2 v_uv;

uniform sampler2D base_color;
uniform vec4 material;
uniform vec4 camera_pos;
uniform vec4 key_dir;
uniform vec4 key_color;
uniform vec4 fill_dir;
uniform vec4 fill_color;
uniform vec4 rim_color;

void main() {
    vec4 texel = texture2D(base_color, v_uv);

    // material.x — порог отсечения; у непрозрачных материалов он
    // отрицательный, и проверка никогда не срабатывает.
    if (texel.a < material.x) {
        discard;
    }

    // Базовый цвет в glTF хранится в sRGB, а весь расчёт света идёт
    // в линейном пространстве.
    vec3 albedo = pow(texel.rgb, vec3(2.2)) * v_color;

    vec3 n = normalize(v_normal);
    vec3 view = normalize(camera_pos.xyz - v_world);
    vec3 l = normalize(key_dir.xyz);

    // Half-lambert: тень не проваливается в чёрное, кожа читается мягче,
    // чем при честном ламберте.
    float key = dot(n, l) * 0.5 + 0.5;
    key *= key;

    float fill = max(dot(n, normalize(fill_dir.xyz)), 0.0);

    vec3 half_vec = normalize(l + view);
    float spec = pow(max(dot(n, half_vec), 0.0), 32.0) * key_color.w;

    // Контровой по краю силуэта — отделяет фигуру от тёмной сцены.
    // Показатель 4 вместо 3 поджимает его к самой кромке, а множитель по
    // альбедо не даёт тёмным волосам светиться ярче освещённой кожи:
    // без него белый ореол ложился на модель целиком.
    float rim = pow(1.0 - max(dot(n, view), 0.0), 4.0) * rim_color.w;

    // Свет должен ещё и падать со стороны кромки, иначе подсвечивается
    // и та часть силуэта, что отвёрнута от источника.
    rim *= smoothstep(-0.3, 0.3, dot(n, l));

    vec3 color = albedo * (key_color.rgb * key + fill_color.rgb * fill);
    color += key_color.rgb * spec * albedo;
    color += rim_color.rgb * rim * mix(albedo, vec3(1.0), 0.35);

    gl_FragColor = vec4(pow(color, vec3(1.0 / 2.2)), 1.0);
}
"#;
