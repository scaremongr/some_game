//! Загрузчик glTF 2.0 в бинарном контейнере (.glb).
//!
//! Читается ровно то, что нужно персонажу: скиннинговый меш, скелет и
//! анимации. Текстуры пока не разбираются — цвет берётся из
//! `baseColorFactor` материала, а картинки потребуют декодера PNG/JPEG.
//!
//! Формат вершины и клипа в движке изначально повторяют glTF, поэтому здесь
//! только разбор и перекладывание, без пересчёта данных.

// Поля структур названы точно как ключи в JSON glTF: так не нужен ни один
// атрибут переименования, а значит нечему разъехаться.
#![allow(non_snake_case)]

use nanoserde::DeJson;

use super::math3::*;
use super::mesh3::{MeshData, Vertex3};
use super::skeleton::{AnimationClip, Skeleton, Track, MAX_BONES};

/// Распакованная картинка: всегда RGBA8, потому что рендерер знает только
/// этот формат.
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Как материал обходится с прозрачностью. Полноценного смешивания нет:
/// оно требует сортировки по глубине, а срез по порогу для волос и ресниц
/// даёт почти тот же результат без неё.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AlphaMode {
    Opaque,
    Mask,
}

pub struct MaterialData {
    pub base_color: Vec3,
    /// Индекс в `Model::textures`; None —цвет берётся только из baseColorFactor.
    pub texture: Option<usize>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
}

impl Default for MaterialData {
    fn default() -> MaterialData {
        MaterialData {
            base_color: vec3(0.8, 0.8, 0.8),
            texture: None,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
        }
    }
}

pub struct Model {
    pub mesh: MeshData,
    pub skeleton: Skeleton,
    pub clips: Vec<AnimationClip>,
    pub textures: Vec<TextureData>,
    pub materials: Vec<MaterialData>,
    /// Трансформ узлов НАД корневой костью. Экспортёры прячут там конверсию
    /// единиц и осей (у Sketchfab это масштаб 0.01 и два поворота), а в сам
    /// скелет он не входит — поэтому применяется ко всей модели целиком.
    pub root_transform: Mat4,
    /// Габариты меша в позе покоя, уже с учётом `root_transform` — по ним
    /// сцена сама наводит камеру.
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    /// Предупреждения разбора: то, что загрузилось не полностью.
    pub warnings: Vec<String>,
}

impl Model {
    pub fn height(&self) -> f32 {
        self.bounds_max.y - self.bounds_min.y
    }

    pub fn center(&self) -> Vec3 {
        (self.bounds_min + self.bounds_max) * 0.5
    }
}

pub type Result<T> = std::result::Result<T, String>;

// ---------------------------------------------------------------------------
// JSON-часть glTF. Разбирается только используемое подмножество.
// ---------------------------------------------------------------------------

#[derive(DeJson, Default)]
struct Gltf {
    #[nserde(default)]
    accessors: Vec<Accessor>,
    #[nserde(default)]
    bufferViews: Vec<BufferView>,
    #[nserde(default)]
    meshes: Vec<Mesh>,
    #[nserde(default)]
    nodes: Vec<Node>,
    #[nserde(default)]
    skins: Vec<Skin>,
    #[nserde(default)]
    animations: Vec<Animation>,
    #[nserde(default)]
    materials: Vec<Material>,
    #[nserde(default)]
    textures: Vec<Texture>,
    #[nserde(default)]
    images: Vec<Image>,
}

#[derive(DeJson, Default)]
struct Accessor {
    bufferView: Option<usize>,
    #[nserde(default)]
    byteOffset: usize,
    componentType: u32,
    count: usize,
    #[nserde(rename = "type")]
    kind: String,
    #[nserde(default)]
    normalized: bool,
}

#[derive(DeJson, Default)]
struct BufferView {
    #[nserde(default)]
    byteOffset: usize,
    byteLength: usize,
    byteStride: Option<usize>,
}

#[derive(DeJson, Default)]
struct Mesh {
    #[nserde(default)]
    primitives: Vec<Primitive>,
}

#[derive(DeJson, Default)]
struct Primitive {
    attributes: Attributes,
    indices: Option<usize>,
    material: Option<usize>,
    mode: Option<u32>,
}

#[derive(DeJson, Default)]
struct Attributes {
    POSITION: Option<usize>,
    NORMAL: Option<usize>,
    TEXCOORD_0: Option<usize>,
    COLOR_0: Option<usize>,
    JOINTS_0: Option<usize>,
    WEIGHTS_0: Option<usize>,
}

#[derive(DeJson, Default)]
struct Node {
    name: Option<String>,
    #[nserde(default)]
    children: Vec<usize>,
    translation: Option<Vec<f32>>,
    rotation: Option<Vec<f32>>,
    scale: Option<Vec<f32>>,
    matrix: Option<Vec<f32>>,
    mesh: Option<usize>,
    skin: Option<usize>,
}

#[derive(DeJson, Default)]
struct Skin {
    inverseBindMatrices: Option<usize>,
    #[nserde(default)]
    joints: Vec<usize>,
}

#[derive(DeJson, Default)]
struct Animation {
    name: Option<String>,
    #[nserde(default)]
    channels: Vec<Channel>,
    #[nserde(default)]
    samplers: Vec<Sampler>,
}

#[derive(DeJson, Default)]
struct Channel {
    sampler: usize,
    target: Target,
}

#[derive(DeJson, Default)]
struct Target {
    node: Option<usize>,
    path: String,
}

#[derive(DeJson, Default)]
struct Sampler {
    input: usize,
    output: usize,
    interpolation: Option<String>,
}

#[derive(DeJson, Default)]
struct Material {
    pbrMetallicRoughness: Option<PbrMetallicRoughness>,
    alphaMode: Option<String>,
    alphaCutoff: Option<f32>,
    doubleSided: Option<bool>,
}

#[derive(DeJson, Default)]
struct PbrMetallicRoughness {
    baseColorFactor: Option<Vec<f32>>,
    baseColorTexture: Option<TextureRef>,
}

#[derive(DeJson, Default)]
struct TextureRef {
    index: usize,
}

#[derive(DeJson, Default)]
struct Texture {
    source: Option<usize>,
}

#[derive(DeJson, Default)]
struct Image {
    bufferView: Option<usize>,
    mimeType: Option<String>,
    uri: Option<String>,
    name: Option<String>,
}

// ---------------------------------------------------------------------------
// Разбор контейнера
// ---------------------------------------------------------------------------

const GLB_MAGIC: u32 = 0x46546C67; // "glTF"
const CHUNK_JSON: u32 = 0x4E4F534A;
const CHUNK_BIN: u32 = 0x004E4942;

fn read_u32(bytes: &[u8], at: usize) -> Result<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| format!("file ends at offset {at}"))
}

/// Разбирает .glb: заголовок из 12 байт, затем чанки JSON и BIN.
pub fn load_glb(bytes: &[u8]) -> Result<Model> {
    let (doc, bin) = parse_glb(bytes)?;
    build(&doc, bin)
}

/// One mesh node of a scene file (a room): geometry around the node's
/// world position, in world orientation, so a piece can tumble about it.
pub struct SceneNode {
    pub name: String,
    pub position: Vec3,
    pub mesh: MeshData,
}

pub struct Scene {
    pub nodes: Vec<SceneNode>,
    pub textures: Vec<TextureData>,
    pub materials: Vec<MaterialData>,
}

/// Loads every mesh node separately (unlike `load_glb`, which merges a
/// model into one skinned mesh).
pub fn load_scene(bytes: &[u8]) -> Result<Scene> {
    let (doc, bin) = parse_glb(bytes)?;
    let mut warnings = Vec::new();
    let textures = decode_images(&doc, bin, &mut warnings);
    let mut materials = build_materials(&doc);
    materials.push(MaterialData::default());
    let default_material = materials.len() - 1;
    let world = world_transforms(&doc);
    let mut nodes = Vec::new();
    for (i, node) in doc.nodes.iter().enumerate() {
        let Some(mesh_index) = node.mesh else { continue };
        let w = world[i];
        let position = w.transform_point(Vec3::ZERO);
        let mut mesh = MeshData::default();
        append_mesh(
            &doc,
            bin,
            &mut mesh,
            mesh_index,
            None,
            Some(Mat4::translation(-position) * w),
            default_material,
            &mut warnings,
        )?;
        nodes.push(SceneNode {
            name: node.name.clone().unwrap_or_default(),
            position,
            mesh,
        });
    }
    if nodes.is_empty() {
        return Err("scene has no meshes".into());
    }
    Ok(Scene { nodes, textures, materials })
}

fn parse_glb(bytes: &[u8]) -> Result<(Gltf, &[u8])> {
    if read_u32(bytes, 0)? != GLB_MAGIC {
        return Err("not a .glb: glTF signature missing".into());
    }
    let version = read_u32(bytes, 4)?;
    if version != 2 {
        return Err(format!("only glTF 2.0 is supported, got version {version}"));
    }

    let mut json: Option<&[u8]> = None;
    let mut bin: Option<&[u8]> = None;

    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let length = read_u32(bytes, offset)? as usize;
        let kind = read_u32(bytes, offset + 4)?;
        let start = offset + 8;
        let end = start
            .checked_add(length)
            .filter(|e| *e <= bytes.len())
            .ok_or_else(|| format!("chunk at {offset} runs past end of file"))?;

        match kind {
            CHUNK_JSON => json = Some(&bytes[start..end]),
            CHUNK_BIN => bin = Some(&bytes[start..end]),
            // Неизвестные чанки спецификация разрешает игнорировать.
            _ => {}
        }
        // Чанки выровнены по 4 байта.
        offset = start + length.div_ceil(4) * 4;
    }

    let json = json.ok_or("no JSON chunk in .glb")?;
    let json = std::str::from_utf8(json).map_err(|e| format!("JSON is not UTF-8: {e}"))?;
    let doc: Gltf = DeJson::deserialize_json(json).map_err(|e| format!("cannot parse JSON: {e}"))?;

    Ok((doc, bin.unwrap_or(&[])))
}

/// Читает из .glb только анимации и накладывает их на уже готовый скелет.
///
/// Кости сопоставляются **по имени узла**, а не по индексу: файл с движением
/// не обязан содержать ни меша, ни скина — достаточно, чтобы кости назывались
/// так же, как в скелете персонажа. Именно так устроен Mixamo: у всех его
/// персонажей риг одинаковый, поэтому любая его анимация ложится на любую
/// его модель.
///
/// Кости, которых в скелете нет, молча пропускаются — чужой риг просто
/// анимируется частично, а не ломается.
pub fn load_clips(bytes: &[u8], target: &Skeleton) -> Result<Vec<AnimationClip>> {
    let (doc, bin) = parse_glb(bytes)?;
    let mut warnings = Vec::new();

    // Имя узла -> кость в скелете персонажа.
    let bone_of_node: Vec<usize> = doc
        .nodes
        .iter()
        .map(|n| {
            n.name
                .as_deref()
                .and_then(|name| target.find_normalized(name))
                .unwrap_or(usize::MAX)
        })
        .collect();

    // Совпасть должна не «хотя бы одна» кость, а заметная часть скелета.
    // Иначе клип тихо шевелит одну кость, и снаружи это выглядит как
    // «движение выбрано, но персонаж стоит».
    let matched = bone_of_node.iter().filter(|b| **b != usize::MAX).count();
    let animated: std::collections::HashSet<usize> = doc
        .animations
        .iter()
        .flat_map(|a| a.channels.iter())
        .filter_map(|c| c.target.node)
        .collect();
    let needed = animated.len().max(1);

    if matched * 4 < needed {
        return Err(format!(
            "different rig: only {matched} of {needed} bones matched"
        ));
    }
    if matched * 2 < needed {
        warnings.push(format!(
            "only {matched} of {needed} bones matched, the move will be partial"
        ));
    }

    Ok(clips_from(&doc, bin, &bone_of_node, target, &mut warnings))
}

// ---------------------------------------------------------------------------
// Чтение аксессоров
// ---------------------------------------------------------------------------

const COMPONENT_BYTE: u32 = 5120;
const COMPONENT_UNSIGNED_BYTE: u32 = 5121;
const COMPONENT_SHORT: u32 = 5122;
const COMPONENT_UNSIGNED_SHORT: u32 = 5123;
const COMPONENT_UNSIGNED_INT: u32 = 5125;
const COMPONENT_FLOAT: u32 = 5126;

fn component_size(t: u32) -> Result<usize> {
    match t {
        COMPONENT_BYTE | COMPONENT_UNSIGNED_BYTE => Ok(1),
        COMPONENT_SHORT | COMPONENT_UNSIGNED_SHORT => Ok(2),
        COMPONENT_UNSIGNED_INT | COMPONENT_FLOAT => Ok(4),
        other => Err(format!("unknown componentType {other}")),
    }
}

fn components_of(kind: &str) -> Result<usize> {
    match kind {
        "SCALAR" => Ok(1),
        "VEC2" => Ok(2),
        "VEC3" => Ok(3),
        "VEC4" => Ok(4),
        "MAT4" => Ok(16),
        other => Err(format!("unsupported accessor type {other}")),
    }
}

/// Читает аксессор как плоский массив f32.
///
/// Целые типы приводятся к float: нормализованные — делением на максимум
/// (так glTF хранит веса и цвета), остальные — как есть (так хранятся
/// индексы костей).
fn read_floats(doc: &Gltf, bin: &[u8], accessor_index: usize) -> Result<(Vec<f32>, usize)> {
    let acc = doc
        .accessors
        .get(accessor_index)
        .ok_or_else(|| format!("missing accessor {accessor_index}"))?;

    let comps = components_of(&acc.kind)?;
    let comp_size = component_size(acc.componentType)?;
    let mut out = Vec::with_capacity(acc.count * comps);

    let view_index = match acc.bufferView {
        Some(v) => v,
        // Аксессор без bufferView означает нули — это легальный случай.
        None => {
            out.resize(acc.count * comps, 0.0);
            return Ok((out, comps));
        }
    };
    let view = doc
        .bufferViews
        .get(view_index)
        .ok_or_else(|| format!("missing bufferView {view_index}"))?;

    let element_size = comp_size * comps;
    // byteStride задаётся, когда данные лежат вперемешку с чужими атрибутами.
    let stride = view.byteStride.unwrap_or(element_size);
    let base = view.byteOffset + acc.byteOffset;

    for i in 0..acc.count {
        let at = base + i * stride;
        if at + element_size > view.byteOffset + view.byteLength || at + element_size > bin.len() {
            return Err(format!("accessor {accessor_index} runs past buffer end"));
        }
        for c in 0..comps {
            let p = at + c * comp_size;
            let v = match acc.componentType {
                COMPONENT_FLOAT => f32::from_le_bytes([bin[p], bin[p + 1], bin[p + 2], bin[p + 3]]),
                COMPONENT_UNSIGNED_INT => {
                    u32::from_le_bytes([bin[p], bin[p + 1], bin[p + 2], bin[p + 3]]) as f32
                }
                COMPONENT_UNSIGNED_SHORT => {
                    let raw = u16::from_le_bytes([bin[p], bin[p + 1]]) as f32;
                    if acc.normalized { raw / 65535.0 } else { raw }
                }
                COMPONENT_SHORT => {
                    let raw = i16::from_le_bytes([bin[p], bin[p + 1]]) as f32;
                    if acc.normalized { (raw / 32767.0).max(-1.0) } else { raw }
                }
                COMPONENT_UNSIGNED_BYTE => {
                    let raw = bin[p] as f32;
                    if acc.normalized { raw / 255.0 } else { raw }
                }
                COMPONENT_BYTE => {
                    let raw = bin[p] as i8 as f32;
                    if acc.normalized { (raw / 127.0).max(-1.0) } else { raw }
                }
                other => return Err(format!("unknown componentType {other}")),
            };
            out.push(v);
        }
    }

    Ok((out, comps))
}

// ---------------------------------------------------------------------------
// Сборка модели
// ---------------------------------------------------------------------------

fn build(doc: &Gltf, bin: &[u8]) -> Result<Model> {
    let mut warnings = Vec::new();

    let textures = decode_images(doc, bin, &mut warnings);
    let mut materials = build_materials(doc);
    // Примитивы без материала указывают на эту запись в конце списка.
    materials.push(MaterialData::default());

    let skinned: Vec<(usize, usize, usize)> = doc
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| match (n.mesh, n.skin) {
            (Some(m), Some(s)) => Some((i, m, s)),
            _ => None,
        })
        .collect();

    let default_material = materials.len() - 1;

    let (skeleton, mesh, root_transform) = if skinned.is_empty() {
        let (s, m) = build_static(doc, bin, default_material, &mut warnings)?;
        (s, m, Mat4::IDENTITY)
    } else {
        build_skinned(doc, bin, &skinned, default_material, &mut warnings)?
    };

    let clips = match skinned.first() {
        Some(&(_, _, skin_index)) => {
            let skin = &doc.skins[skin_index];
            let (_, remap) = build_skeleton(doc, bin, skin)?;
            build_clips(doc, bin, skin, &remap, &skeleton, &mut warnings)
        }
        None => Vec::new(),
    };

    let (bounds_min, bounds_max) = bounds(&mesh, &skeleton, root_transform, clips.first());

    Ok(Model {
        mesh,
        skeleton,
        clips,
        textures,
        materials,
        root_transform,
        bounds_min,
        bounds_max,
        warnings,
    })
}

/// Обычный путь: есть скин, значит есть скелет. Все меши, привязанные к
/// одному скину, сливаются в один — персонажа часто экспортируют кусками
/// (волосы, лицо, одежда отдельными мешами).
fn build_skinned(
    doc: &Gltf,
    bin: &[u8],
    skinned: &[(usize, usize, usize)],
    default_material: usize,
    warnings: &mut Vec<String>,
) -> Result<(Skeleton, MeshData, Mat4)> {
    let skin_index = skinned[0].2;
    if skinned.iter().any(|(_, _, s)| *s != skin_index) {
        warnings.push("model has several skins, only the first is used".into());
    }

    let skin = doc
        .skins
        .get(skin_index)
        .ok_or_else(|| format!("missing skin {skin_index}"))?;
    if skin.joints.is_empty() {
        return Err("skin has no joints".into());
    }

    let (skeleton, remap) = build_skeleton(doc, bin, skin)?;

    let mut mesh = MeshData::default();
    for (_, mesh_index, skin_of_node) in skinned {
        if *skin_of_node != skin_index {
            continue;
        }
        // Трансформ узла со скиннингом спецификация велит игнорировать:
        // положение задаёт скелет.
        append_mesh(doc, bin, &mut mesh, *mesh_index, Some(&remap), None, default_material, warnings)?;
    }

    let root_transform = skeleton_root_transform(doc, skin, warnings);
    let mut skeleton = skeleton;
    // Скелет запоминает разворот своего пространства модели: он понадобится,
    // когда на этот риг будут класть анимацию из файла с другими осями.
    skeleton.root_rotation = Quat::from_matrix(root_transform);
    let mut mesh = finish_mesh(mesh, warnings)?;

    // Упрощение идёт после сборки меша: решение принимается по весам вершин.
    let skeleton = if skeleton.len() > MAX_BONES {
        reduce_skeleton(skeleton, &mut mesh, MAX_BONES, warnings)
    } else {
        skeleton
    };

    if skeleton.len() > MAX_BONES {
        return Err(format!(
            "model has {} bones after simplification, engine limit is {MAX_BONES}",
            skeleton.len()
        ));
    }

    Ok((skeleton, mesh, root_transform))
}

/// Мировой трансформ узла-родителя корневой кости.
///
/// Спецификация требует считать мировые матрицы костей по всей иерархии
/// сцены, а не только внутри скелета. Экспортёры этим пользуются: Sketchfab
/// кладёт над скелетом масштаб 0.01 и повороты осей. Общий множитель выносим
/// сюда, чтобы анимация корневой кости продолжала работать в своём
/// локальном пространстве.
fn skeleton_root_transform(doc: &Gltf, skin: &Skin, warnings: &mut Vec<String>) -> Mat4 {
    let parents = parent_map(doc);

    let roots: Vec<usize> = skin
        .joints
        .iter()
        .copied()
        .filter(|node| {
            let p = parents.get(*node).copied().unwrap_or(usize::MAX);
            p == usize::MAX || !skin.joints.contains(&p)
        })
        .collect();

    let root = match roots.first() {
        Some(r) => *r,
        None => return Mat4::IDENTITY,
    };
    if roots.len() > 1 {
        warnings.push("skeleton has several roots, using the first".into());
    }

    match parents.get(root).copied().unwrap_or(usize::MAX) {
        usize::MAX => Mat4::IDENTITY,
        parent => world_transforms(doc)[parent],
    }
}

/// Запасной путь: скелета нет. Модель показывается целиком как одна жёсткая
/// кость — так её видно и можно проверить пропорции с ориентацией, пока она
/// не риггнута.
fn build_static(
    doc: &Gltf,
    bin: &[u8],
    default_material: usize,
    warnings: &mut Vec<String>,
) -> Result<(Skeleton, MeshData)> {
    let mesh_nodes: Vec<(usize, usize)> = doc
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.mesh.map(|m| (i, m)))
        .collect();

    if mesh_nodes.is_empty() {
        return Err("model has no meshes at all".into());
    }

    warnings.push(format!(
        "NO SKELETON: {} meshes, 0 skins - model is not rigged",
        mesh_nodes.len()
    ));
    warnings.push("shown as one rigid piece, it can only bob to the beat".into());

    // Единственную кость называем hips, чтобы аддитивный пульс её нашёл.
    let skeleton = Skeleton::new(vec![("hips".to_string(), None, Transform::IDENTITY)]);
    let world = world_transforms(doc);

    let mut mesh = MeshData::default();
    for (node, mesh_index) in mesh_nodes {
        // Скелета нет, поэтому трансформ узла надо запечь прямо в вершины.
        append_mesh(
            doc,
            bin,
            &mut mesh,
            mesh_index,
            None,
            Some(world[node]),
            default_material,
            warnings,
        )?;
    }

    finish_mesh(mesh, warnings).map(|m| (skeleton, m))
}

fn finish_mesh(mut mesh: MeshData, warnings: &mut Vec<String>) -> Result<MeshData> {
    if mesh.verts.is_empty() {
        return Err("mesh has no usable primitives".into());
    }
    if normals_missing(&mesh) {
        warnings.push("model has no normals, computed from geometry".into());
        mesh.compute_normals();
    }
    Ok(mesh)
}

fn parent_map(doc: &Gltf) -> Vec<usize> {
    let mut parent = vec![usize::MAX; doc.nodes.len()];
    for (i, node) in doc.nodes.iter().enumerate() {
        for &child in &node.children {
            if child < parent.len() {
                parent[child] = i;
            }
        }
    }
    parent
}

/// Мировые трансформы всех узлов сцены.
fn world_transforms(doc: &Gltf) -> Vec<Mat4> {
    let parent = parent_map(doc);

    let mut world = vec![Mat4::IDENTITY; doc.nodes.len()];
    let mut done = vec![false; doc.nodes.len()];

    // Считаем рекурсивно вверх по цепочке родителей — без рекурсии функций,
    // чтобы не упереться в глубину стека на длинных иерархиях.
    for i in 0..doc.nodes.len() {
        if done[i] {
            continue;
        }
        let mut chain = Vec::new();
        let mut cursor = i;
        while cursor != usize::MAX && !done[cursor] {
            chain.push(cursor);
            cursor = parent[cursor];
        }
        let mut acc = if cursor == usize::MAX {
            Mat4::IDENTITY
        } else {
            world[cursor]
        };
        for &node in chain.iter().rev() {
            acc = acc * node_transform(&doc.nodes[node]).matrix();
            world[node] = acc;
            done[node] = true;
        }
    }

    world
}

fn node_transform(node: &Node) -> Transform {
    if let Some(m) = &node.matrix {
        if m.len() == 16 {
            // Матрицу раскладывать обратно в TRS не хочется, а анимация всё
            // равно приходит покомпонентно. На практике экспортёры отдают TRS.
            let mut arr = [0.0f32; 16];
            arr.copy_from_slice(m);
            return decompose(Mat4(arr));
        }
    }
    Transform {
        translation: node
            .translation
            .as_ref()
            .filter(|v| v.len() == 3)
            .map(|v| vec3(v[0], v[1], v[2]))
            .unwrap_or(Vec3::ZERO),
        rotation: node
            .rotation
            .as_ref()
            .filter(|v| v.len() == 4)
            .map(|v| Quat { x: v[0], y: v[1], z: v[2], w: v[3] }.normalize())
            .unwrap_or(Quat::IDENTITY),
        scale: node
            .scale
            .as_ref()
            .filter(|v| v.len() == 3)
            .map(|v| vec3(v[0], v[1], v[2]))
            .unwrap_or(Vec3::ONE),
    }
}

/// Раскладывает матрицу на TRS. Сдвиг и масштаб берутся напрямую, поворот —
/// из ортонормированной части. Скос (shear) при этом теряется, но в скелетах
/// его не бывает.
fn decompose(m: Mat4) -> Transform {
    let c = m.0;
    let translation = vec3(c[12], c[13], c[14]);
    let sx = vec3(c[0], c[1], c[2]).length();
    let sy = vec3(c[4], c[5], c[6]).length();
    let sz = vec3(c[8], c[9], c[10]).length();
    let scale = vec3(sx, sy, sz);

    let (ix, iy, iz) = (
        if sx > 1e-6 { 1.0 / sx } else { 0.0 },
        if sy > 1e-6 { 1.0 / sy } else { 0.0 },
        if sz > 1e-6 { 1.0 / sz } else { 0.0 },
    );
    let r = [
        c[0] * ix, c[1] * ix, c[2] * ix,
        c[4] * iy, c[5] * iy, c[6] * iy,
        c[8] * iz, c[9] * iz, c[10] * iz,
    ];

    // Классическое извлечение кватерниона из матрицы поворота через след.
    let trace = r[0] + r[4] + r[8];
    let rotation = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        Quat { w: 0.25 * s, x: (r[5] - r[7]) / s, y: (r[6] - r[2]) / s, z: (r[1] - r[3]) / s }
    } else if r[0] > r[4] && r[0] > r[8] {
        let s = (1.0 + r[0] - r[4] - r[8]).sqrt() * 2.0;
        Quat { w: (r[5] - r[7]) / s, x: 0.25 * s, y: (r[3] + r[1]) / s, z: (r[6] + r[2]) / s }
    } else if r[4] > r[8] {
        let s = (1.0 + r[4] - r[0] - r[8]).sqrt() * 2.0;
        Quat { w: (r[6] - r[2]) / s, x: (r[3] + r[1]) / s, y: 0.25 * s, z: (r[7] + r[5]) / s }
    } else {
        let s = (1.0 + r[8] - r[0] - r[4]).sqrt() * 2.0;
        Quat { w: (r[1] - r[3]) / s, x: (r[6] + r[2]) / s, y: (r[7] + r[5]) / s, z: 0.25 * s }
    };

    Transform { translation, rotation: rotation.normalize(), scale }
}

/// Строит скелет и таблицу перенумерации костей.
///
/// Движок требует, чтобы родитель шёл раньше ребёнка — иначе глобальные
/// матрицы нельзя посчитать одним проходом. Порядок костей в glTF этого не
/// гарантирует, поэтому список сортируется, а индексы в вершинах и анимациях
/// переставляются по `remap`.
fn build_skeleton(doc: &Gltf, bin: &[u8], skin: &Skin) -> Result<(Skeleton, Vec<usize>)> {
    let count = skin.joints.len();

    // node -> позиция в списке костей
    let mut joint_of_node = vec![usize::MAX; doc.nodes.len()];
    for (i, &node) in skin.joints.iter().enumerate() {
        if node >= doc.nodes.len() {
            return Err(format!("joint points at missing node {node}"));
        }
        joint_of_node[node] = i;
    }

    // Родитель кости — ближайший предок, который тоже кость.
    let mut parent_of_joint = vec![None; count];
    for (node_index, node) in doc.nodes.iter().enumerate() {
        let parent_joint = joint_of_node.get(node_index).copied().unwrap_or(usize::MAX);
        for &child in &node.children {
            if let Some(&cj) = joint_of_node.get(child) {
                if cj != usize::MAX && parent_joint != usize::MAX {
                    parent_of_joint[cj] = Some(parent_joint);
                }
            }
        }
    }

    // Топологический порядок: сначала корни, затем их дети.
    let mut order: Vec<usize> = Vec::with_capacity(count);
    let mut placed = vec![false; count];
    let mut guard = 0;
    while order.len() < count {
        let before = order.len();
        for j in 0..count {
            if placed[j] {
                continue;
            }
            let ready = match parent_of_joint[j] {
                None => true,
                Some(p) => placed[p],
            };
            if ready {
                placed[j] = true;
                order.push(j);
            }
        }
        if order.len() == before {
            return Err("bone hierarchy has a cycle".into());
        }
        guard += 1;
        if guard > count + 2 {
            return Err("cannot order bones".into());
        }
    }

    let mut remap = vec![0usize; count];
    for (new_index, &old_index) in order.iter().enumerate() {
        remap[old_index] = new_index;
    }

    // Обратные бинд-матрицы: из файла, если есть, иначе единичные
    // (спецификация разрешает их опустить).
    let inverse_binds: Vec<Mat4> = match skin.inverseBindMatrices {
        Some(acc) => {
            let (values, comps) = read_floats(doc, bin, acc)?;
            if comps != 16 || values.len() < count * 16 {
                return Err("inverseBindMatrices is not MAT4 or too short".into());
            }
            order
                .iter()
                .map(|&old| {
                    let mut m = [0.0f32; 16];
                    m.copy_from_slice(&values[old * 16..old * 16 + 16]);
                    Mat4(m)
                })
                .collect()
        }
        None => vec![Mat4::IDENTITY; count],
    };

    let bones = order
        .iter()
        .map(|&old| {
            let node = &doc.nodes[skin.joints[old]];
            (
                node.name.clone().unwrap_or_else(|| format!("bone{old}")),
                parent_of_joint[old].map(|p| remap[p]),
                node_transform(node),
            )
        })
        .collect();

    Ok((Skeleton::from_parts(bones, inverse_binds), remap))
}

/// Распаковывает все картинки модели в RGBA8.
///
/// Картинки лежат внутри .glb как куски двоичного чанка, а формат объявлен
/// в mimeType. Внешние файлы по `uri` не поддерживаются намеренно: модель
/// должна быть одним самодостаточным файлом.
fn decode_images(doc: &Gltf, bin: &[u8], warnings: &mut Vec<String>) -> Vec<TextureData> {
    let mut out = Vec::with_capacity(doc.images.len());

    for (i, image) in doc.images.iter().enumerate() {
        let label = image.name.clone().unwrap_or_else(|| format!("image {i}"));

        let bytes = match image.bufferView {
            Some(v) => match doc.bufferViews.get(v) {
                Some(view) => {
                    let end = view.byteOffset + view.byteLength;
                    match bin.get(view.byteOffset..end) {
                        Some(b) => b,
                        None => {
                            warnings.push(format!("{label}: image data out of range"));
                            out.push(placeholder_texture());
                            continue;
                        }
                    }
                }
                None => {
                    warnings.push(format!("{label}: missing bufferView"));
                    out.push(placeholder_texture());
                    continue;
                }
            },
            None => {
                warnings.push(match image.uri.as_deref() {
                    Some(_) => format!("{label}: external image, embed it into the .glb"),
                    None => format!("{label}: no image data"),
                });
                out.push(placeholder_texture());
                continue;
            }
        };

        // Формат определяем по сигнатуре, а не по mimeType: он врёт чаще.
        let decoded = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            decode_png(bytes)
        } else if bytes.starts_with(&[0xFF, 0xD8]) {
            decode_jpeg(bytes)
        } else {
            Err("unknown image format".to_string())
        };

        match decoded {
            Ok(t) => out.push(t),
            Err(e) => {
                warnings.push(format!("{label}: {e}"));
                out.push(placeholder_texture());
            }
        }
    }

    out
}

/// Заметная заглушка вместо непрочитанной картинки: пурпурный лучше,
/// чем незаметно неправильный цвет.
fn placeholder_texture() -> TextureData {
    TextureData {
        width: 1,
        height: 1,
        rgba: vec![255, 0, 255, 255],
    }
}

fn decode_png(bytes: &[u8]) -> std::result::Result<TextureData, String> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());

    let pixels = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => expand(&buf, pixels, 3, |p| [p[0], p[1], p[2], 255]),
        png::ColorType::Grayscale => expand(&buf, pixels, 1, |p| [p[0], p[0], p[0], 255]),
        png::ColorType::GrayscaleAlpha => expand(&buf, pixels, 2, |p| [p[0], p[0], p[0], p[1]]),
        // Палитру png уже развернул в RGB/RGBA, сюда попасть не должно.
        other => return Err(format!("unsupported PNG color type {other:?}")),
    };

    Ok(TextureData { width: info.width, height: info.height, rgba })
}

fn decode_jpeg(bytes: &[u8]) -> std::result::Result<TextureData, String> {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    let buf = decoder.decode().map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("no JPEG header")?;

    let pixels = (info.width as usize) * (info.height as usize);
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => expand(&buf, pixels, 3, |p| [p[0], p[1], p[2], 255]),
        jpeg_decoder::PixelFormat::L8 => expand(&buf, pixels, 1, |p| [p[0], p[0], p[0], 255]),
        other => return Err(format!("unsupported JPEG format {other:?}")),
    };

    Ok(TextureData {
        width: info.width as u32,
        height: info.height as u32,
        rgba,
    })
}

fn expand(src: &[u8], pixels: usize, step: usize, f: impl Fn(&[u8]) -> [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels * 4);
    for i in 0..pixels {
        let at = i * step;
        match src.get(at..at + step) {
            Some(p) => out.extend_from_slice(&f(p)),
            None => out.extend_from_slice(&[0, 0, 0, 255]),
        }
    }
    out
}

/// Материалы в том виде, в каком их ждёт рендерер.
fn build_materials(doc: &Gltf) -> Vec<MaterialData> {
    doc.materials
        .iter()
        .map(|m| {
            let pbr = m.pbrMetallicRoughness.as_ref();
            let base_color = match pbr.and_then(|p| p.baseColorFactor.as_ref()) {
                // baseColorFactor уже линейный, в отличие от текстуры.
                Some(f) if f.len() >= 3 => vec3(f[0], f[1], f[2]),
                _ => vec3(1.0, 1.0, 1.0),
            };

            let texture = pbr
                .and_then(|p| p.baseColorTexture.as_ref())
                .and_then(|t| doc.textures.get(t.index))
                .and_then(|t| t.source);

            // BLEND сводим к MASK: настоящее смешивание требует сортировки
            // треугольников по глубине, а для волос и ресниц порога хватает.
            let alpha_mode = match m.alphaMode.as_deref() {
                Some("MASK") | Some("BLEND") => AlphaMode::Mask,
                _ => AlphaMode::Opaque,
            };

            MaterialData {
                base_color,
                texture,
                alpha_mode,
                alpha_cutoff: m.alphaCutoff.unwrap_or(0.5),
                double_sided: m.doubleSided.unwrap_or(false),
            }
        })
        .collect()
}

fn base_color(doc: &Gltf, material: Option<usize>) -> Vec3 {
    let factor = material
        .and_then(|m| doc.materials.get(m))
        .and_then(|m| m.pbrMetallicRoughness.as_ref())
        .and_then(|p| p.baseColorFactor.as_ref());

    match factor {
        Some(f) if f.len() >= 3 => vec3(f[0], f[1], f[2]),
        // glTF хранит baseColorFactor уже в линейном пространстве, поэтому
        // преобразовывать его не нужно — в отличие от цветов из палитры.
        _ => vec3(0.8, 0.8, 0.8),
    }
}

/// Дописывает примитивы одного меша в общий буфер.
///
/// `joint_remap` задан, когда меш скиннинговый: индексы костей надо
/// переставить под порядок движка. `bake` задан для статичной модели:
/// трансформ узла тогда некому применить, кроме как вершинам.
fn append_mesh(
    doc: &Gltf,
    bin: &[u8],
    out: &mut MeshData,
    mesh_index: usize,
    joint_remap: Option<&[usize]>,
    bake: Option<Mat4>,
    default_material: usize,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let mesh = doc
        .meshes
        .get(mesh_index)
        .ok_or_else(|| format!("missing mesh {mesh_index}"))?;

    for (pi, prim) in mesh.primitives.iter().enumerate() {
        // mode 4 — треугольники, единственный режим, который умеет рендерер.
        if prim.mode.unwrap_or(4) != 4 {
            warnings.push(format!("primitive {pi} is not triangles, skipped"));
            continue;
        }

        let pos_acc = match prim.attributes.POSITION {
            Some(a) => a,
            None => {
                warnings.push(format!("primitive {pi} has no POSITION, skipped"));
                continue;
            }
        };

        let (positions, _) = read_floats(doc, bin, pos_acc)?;
        let vertex_count = positions.len() / 3;
        let base = out.verts.len() as u32;

        let normals = prim
            .attributes
            .NORMAL
            .map(|a| read_floats(doc, bin, a))
            .transpose()?
            .map(|(v, _)| v);
        let uvs = prim
            .attributes
            .TEXCOORD_0
            .map(|a| read_floats(doc, bin, a))
            .transpose()?
            .map(|(v, _)| v);
        let colors = prim
            .attributes
            .COLOR_0
            .map(|a| read_floats(doc, bin, a))
            .transpose()?;
        let joints = prim
            .attributes
            .JOINTS_0
            .map(|a| read_floats(doc, bin, a))
            .transpose()?
            .map(|(v, _)| v);
        let weights = prim
            .attributes
            .WEIGHTS_0
            .map(|a| read_floats(doc, bin, a))
            .transpose()?
            .map(|(v, _)| v);

        if joint_remap.is_some() && (joints.is_none() || weights.is_none()) {
            warnings.push(format!("primitive {pi} has no skin weights, it will not move"));
        }

        let material_color = base_color(doc, prim.material);
        let material = prim.material.unwrap_or(default_material);

        for v in 0..vertex_count {
            let mut pos = vec3(positions[v * 3], positions[v * 3 + 1], positions[v * 3 + 2]);

            let mut normal = match &normals {
                Some(n) if n.len() >= (v + 1) * 3 => vec3(n[v * 3], n[v * 3 + 1], n[v * 3 + 2]),
                // Нормали посчитаются позже из геометрии.
                _ => Vec3::ZERO,
            };

            if let Some(m) = bake {
                pos = m.transform_point(pos);
                normal = m.transform_direction(normal).normalize();
            }

            let uv = match &uvs {
                Some(t) if t.len() >= (v + 1) * 2 => [t[v * 2], t[v * 2 + 1]],
                _ => [0.0, 0.0],
            };

            let color = match &colors {
                Some((c, comps)) if c.len() >= (v + 1) * comps => {
                    let o = v * comps;
                    // Цвет вершины модулирует цвет материала — так же,
                    // как это делает сам glTF.
                    vec3(
                        c[o] * material_color.x,
                        c[o + 1] * material_color.y,
                        c[o + 2] * material_color.z,
                    )
                }
                _ => material_color,
            };

            let (mut j, mut w) = ([0.0f32; 4], [1.0f32, 0.0, 0.0, 0.0]);
            if let (Some(remap), Some(js), Some(ws)) = (joint_remap, &joints, &weights) {
                if js.len() >= (v + 1) * 4 && ws.len() >= (v + 1) * 4 {
                    let mut sum = 0.0;
                    for k in 0..4 {
                        let raw = js[v * 4 + k] as usize;
                        // Перенумерация обязательна: кости пересортированы.
                        j[k] = remap.get(raw).copied().unwrap_or(0) as f32;
                        w[k] = ws[v * 4 + k];
                        sum += w[k];
                    }
                    // Нормализация весов: после квантования сумма редко ровно 1.
                    if sum > 1e-6 {
                        for k in 0..4 {
                            w[k] /= sum;
                        }
                    } else {
                        w = [1.0, 0.0, 0.0, 0.0];
                    }
                }
            }

            out.verts.push(Vertex3 {
                pos: pos.to_array(),
                normal: normal.to_array(),
                uv,
                color: color.to_array(),
                joints: j,
                weights: w,
            });
        }

        match prim.indices {
            Some(acc) => {
                let (values, _) = read_floats(doc, bin, acc)?;
                out.indices.extend(values.iter().map(|&i| base + i as u32));
            }
            // Без индексов вершины идут подряд тройками.
            None => out
                .indices
                .extend((0..vertex_count as u32).map(|i| base + i)),
        }

        out.end_submesh(material);
    }

    Ok(())
}

fn normals_missing(mesh: &MeshData) -> bool {
    mesh.verts
        .iter()
        .any(|v| v.normal == [0.0, 0.0, 0.0])
}

/// Габариты в том пространстве, где модель реально окажется на экране.
///
/// Считать их по «сырым» POSITION нельзя: экспортёры свободно раскидывают
/// конверсию единиц между обратными бинд-матрицами и узлами над скелетом.
/// У модели из Sketchfab меш лежит в метрах, скелет — в сантиметрах, а над
/// корнем висит масштаб 0.01. Поэтому вершины прогоняются через скиннинг
/// ровно так же, как это делает вершинный шейдер.
///
/// Поза покоя тоже не годится как единственная точка отсчёта: анимации
/// Mixamo смещают всё тело относительно бинд-позы, и фигура повисает над
/// полом. Поэтому клип опрашивается в нескольких точках, а границы берутся
/// по объединению — тогда самая низкая точка движения и оказывается на полу.
pub fn bounds(
    mesh: &MeshData,
    skeleton: &Skeleton,
    root: Mat4,
    clip: Option<&AnimationClip>,
) -> (Vec3, Vec3) {
    const POSES: usize = 8;
    /// Сколько вершин достаточно опросить: для кадрирования доли миллиметра
    /// не важны, а полный проход по сотням тысяч вершин заметен на глаз.
    const TARGET_SAMPLES: usize = 40_000;

    let rest = skeleton.rest_pose();
    let mut pose = rest.clone();
    let mut globals = Vec::new();
    let mut skin = Vec::new();

    let animated = clip.filter(|c| c.duration > 0.0 && !c.tracks.is_empty());
    let poses = if animated.is_some() { POSES } else { 1 };
    let stride = (mesh.verts.len() / TARGET_SAMPLES).max(1);

    let mut min = vec3(f32::MAX, f32::MAX, f32::MAX);
    let mut max = vec3(f32::MIN, f32::MIN, f32::MIN);

    for i in 0..poses {
        match animated {
            Some(c) => {
                let t = i as f32 / poses as f32 * c.duration;
                c.sample(t, &rest, &mut pose);
            }
            None => pose.locals.copy_from_slice(&rest.locals),
        }
        skeleton.skin_matrices(&pose, &mut globals, &mut skin);

        for v in mesh.verts.iter().step_by(stride) {
            let p = vec3(v.pos[0], v.pos[1], v.pos[2]);

            let mut acc = Vec3::ZERO;
            let mut total = 0.0;
            for k in 0..4 {
                let w = v.weights[k];
                if w <= 0.0 {
                    continue;
                }
                if let Some(m) = skin.get(v.joints[k] as usize) {
                    acc += m.transform_point(p) * w;
                    total += w;
                }
            }
            let posed = if total > 1e-6 { acc * (1.0 / total) } else { p };
            let world = root.transform_point(posed);

            min = vec3(min.x.min(world.x), min.y.min(world.y), min.z.min(world.z));
            max = vec3(max.x.max(world.x), max.y.max(world.y), max.z.max(world.z));
        }
    }

    (min, max)
}

fn build_clips(
    doc: &Gltf,
    bin: &[u8],
    skin: &Skin,
    joint_remap: &[usize],
    skeleton: &Skeleton,
    warnings: &mut Vec<String>,
) -> Vec<AnimationClip> {
    // node -> кость в порядке движка. Скелет мог быть упрощён после
    // перенумерации (лишние кости слиты), поэтому сначала ищем по имени,
    // а старый индекс берём, только если он ещё существует.
    let mut bone_of_node = vec![usize::MAX; doc.nodes.len()];
    for (old, &node) in skin.joints.iter().enumerate() {
        if node >= bone_of_node.len() {
            continue;
        }
        let by_name = doc.nodes[node].name.as_deref().and_then(|n| skeleton.find(n));
        bone_of_node[node] = match by_name {
            Some(bone) => bone,
            None if joint_remap[old] < skeleton.len() => joint_remap[old],
            None => usize::MAX,
        };
    }

    clips_from(doc, bin, &bone_of_node, skeleton, warnings)
}

/// Упрощает риг до бюджета движка, сливая наименее значимые кости с их
/// родителями.
///
/// Модели часто несут кости, которых танцевальный клип никогда не касается:
/// мимика, глаза, волосы, юбка. Вершины такой кости передаются родителю —
/// в позе покоя это ничего не меняет, а дальше они просто едут вместе с ним
/// как жёсткая часть.
///
/// Отбор идёт с листьев и по суммарному весу: кость без потомков и с самым
/// малым влиянием уходит первой. Основной скелет защищён по имени, иначе
/// упрощение могло бы съесть что-то нужное.
fn reduce_skeleton(
    skeleton: Skeleton,
    mesh: &mut MeshData,
    limit: usize,
    warnings: &mut Vec<String>,
) -> Skeleton {
    let count = skeleton.len();

    let mut weight = vec![0.0f32; count];
    for v in &mesh.verts {
        for k in 0..4 {
            if let Some(w) = weight.get_mut(v.joints[k] as usize) {
                *w += v.weights[k];
            }
        }
    }

    let mut keep = vec![true; count];
    let mut children = vec![0usize; count];
    for bone in &skeleton.bones {
        if let Some(p) = bone.parent {
            children[p] += 1;
        }
    }

    let mut kept = count;
    let mut dropped_names: Vec<String> = Vec::new();

    while kept > limit {
        let victim = (0..count)
            .filter(|&i| {
                keep[i]
                    && children[i] == 0
                    && skeleton.bones[i].parent.is_some()
                    && !super::skeleton::is_core_bone(&skeleton.bones[i].name)
            })
            .min_by(|&a, &b| weight[a].total_cmp(&weight[b]));

        let victim = match victim {
            Some(v) => v,
            None => break,
        };

        keep[victim] = false;
        kept -= 1;
        if let Some(p) = skeleton.bones[victim].parent {
            children[p] -= 1;
            weight[p] += weight[victim];
        }
        if dropped_names.len() < 4 {
            dropped_names.push(skeleton.bones[victim].name.clone());
        }
    }

    if kept == count {
        return skeleton;
    }

    // Индекс выброшенной кости заменяется ближайшим сохранённым предком.
    let mut remap = vec![0usize; count];
    let mut new_index = 0usize;
    for i in 0..count {
        if keep[i] {
            remap[i] = new_index;
            new_index += 1;
        } else {
            let mut at = skeleton.bones[i].parent;
            let mut target = 0;
            while let Some(p) = at {
                if keep[p] {
                    target = remap[p];
                    break;
                }
                at = skeleton.bones[p].parent;
            }
            remap[i] = target;
        }
    }

    let mut bones = Vec::with_capacity(kept);
    let mut inverse_binds = Vec::with_capacity(kept);
    for (i, bone) in skeleton.bones.iter().enumerate() {
        if !keep[i] {
            continue;
        }
        bones.push((
            bone.name.clone(),
            bone.parent.map(|p| remap[p]),
            bone.bind_local,
        ));
        inverse_binds.push(bone.inverse_bind);
    }

    for v in mesh.verts.iter_mut() {
        let mut joints = [0usize; 4];
        let mut weights = [0.0f32; 4];
        let mut used = 0usize;

        for k in 0..4 {
            if v.weights[k] <= 0.0 {
                continue;
            }
            let j = remap[(v.joints[k] as usize).min(count - 1)];
            // После слияния несколько влияний могут указывать на одну кость.
            match (0..used).find(|&s| joints[s] == j) {
                Some(s) => weights[s] += v.weights[k],
                None => {
                    joints[used] = j;
                    weights[used] = v.weights[k];
                    used += 1;
                }
            }
        }

        let sum: f32 = weights.iter().sum();
        for k in 0..4 {
            v.joints[k] = joints[k] as f32;
            v.weights[k] = if sum > 1e-6 { weights[k] / sum } else { 0.0 };
        }
        if sum <= 1e-6 {
            v.weights[0] = 1.0;
        }
    }

    warnings.push(format!(
        "rig simplified: {count} bones -> {kept} (dropped {}...)",
        dropped_names.join(", ")
    ));

    // The model-space orientation survives the simplification: clips from
    // other files are retargeted through it.
    let mut reduced = Skeleton::from_parts(bones, inverse_binds);
    reduced.root_rotation = skeleton.root_rotation;
    reduced
}

/// Сажает клип на пол и центрирует его по горизонтали.
///
/// Нужно потому, что «правильной» абсолютной высоты таза не существует:
/// у одного файла таз отсчитывается от пола, у другого — от самого таза,
/// а у третьего бинд-поза вообще не совпадает с системой его же анимации.
/// Поэтому клип прогоняется через скиннинг, и найденный низ подгоняется
/// к тому же уровню, на котором стоит исходная анимация модели.
///
/// `reference_min_y` и `reference_center` берутся из габаритов модели, то
/// есть из её собственного клипа — он и задаёт «как эта модель стоит».
pub fn ground_clip(
    clip: &mut AnimationClip,
    skeleton: &Skeleton,
    mesh: &MeshData,
    root_bone: usize,
    reference_min_y: f32,
    reference_center: Vec3,
) {
    let (min, max) = bounds(mesh, skeleton, Mat4::IDENTITY, Some(clip));
    let center = (min + max) * 0.5;

    let offset = vec3(
        reference_center.x - center.x,
        reference_min_y - min.y,
        reference_center.z - center.z,
    );
    if offset.length() < 1e-4 {
        return;
    }

    let rest = skeleton.bones[root_bone].bind_local.translation;

    match clip.tracks.iter_mut().find(|t| t.bone == root_bone) {
        Some(track) if !track.translations.is_empty() => {
            for (_, v) in track.translations.iter_mut() {
                *v = *v + offset;
            }
        }
        Some(track) => track.translations.push((0.0, rest + offset)),
        None => clip.tracks.push(Track {
            bone: root_bone,
            translations: vec![(0.0, rest + offset)],
            ..Default::default()
        }),
    }
}

/// Переносит дорожку поворотов между ригами с разной ориентацией костей
/// в позе покоя.
///
/// Клип хранит абсолютный локальный поворот кости, а он имеет смысл только
/// вместе с системой координат этой кости. У одного экспорта конверсия осей
/// вынесена в узлы над скелетом, у другого запечена в сами кости — и тогда
/// подстановка «как есть» кладёт фигуру набок.
///
/// Поэтому берём поворот кости **относительно её позы покоя в своём файле**
/// и накладываем на позу покоя целевой кости. Для клипа из того же файла
/// поправка вырождается в единицу.
/// Перекладывает дорожку поворотов с чужого рига на наш.
///
/// # Почему поправок две
///
/// Кость хранит поворот относительно родителя, а «одинаковое движение» — это
/// одинаковый поворот в мировых координатах. Если позы покоя двух ригов
/// совпадают, разницы нет и дорожку можно брать как есть. Если не совпадают —
/// а у записей движения из видео бедро бывает развёрнуто ровно на 180° — то
/// локальный поворот означает уже совсем другое, и колени начинают гнуться не
/// в ту сторону.
///
/// Вывод. Пусть `W` — поворот кости в мировых координатах, `C(b) = Wsrc(b)⁻¹ ·
/// Wtgt(b)` — расхождение поз покоя. Хотим, чтобы мировое отклонение от покоя
/// совпало: `Wtgt_anim(b) = Wsrc_anim(b) · Wsrc_rest(b)⁻¹ · Wtgt_rest(b)`,
/// то есть `Wtgt_anim(b) = Wsrc_anim(b) · C(b)`. Локальный поворот — это
/// `Wtgt_anim(родитель)⁻¹ · Wtgt_anim(b)`, и после подстановки всё сводится к
///
/// ```text
/// q_целевой = C(родитель)⁻¹ · q_исходный · C(кость)
/// ```
///
/// Пересчёт получается ровно таким же локальным, как был, — но теперь точным.
/// При совпадающих позах покоя обе поправки единичны и дорожка не меняется.
fn retarget_rotations(track: &mut Track, parent_correction: Quat, bone_correction: Quat) {
    let left = parent_correction.conjugate();
    for (_, q) in track.rotations.iter_mut() {
        *q = (left * *q * bone_correction).normalize();
    }
}

/// Расхождение поз покоя одной кости: `Wsrc⁻¹ · Wtgt`.
fn rest_correction(source_world: Quat, target_world: Quat) -> Quat {
    (source_world.conjugate() * target_world).normalize()
}

/// Мировые повороты костей целевого скелета в позе покоя.
///
/// Родитель всегда идёт раньше ребёнка, поэтому хватает одного прохода.
fn target_world_rest(target: &Skeleton) -> Vec<Quat> {
    let mut out: Vec<Quat> = Vec::with_capacity(target.bones.len());
    for bone in &target.bones {
        let local = bone.bind_local.rotation;
        out.push(match bone.parent {
            Some(p) => (out[p] * local).normalize(),
            None => local.normalize(),
        });
    }
    out
}

/// Мировой поворот узла исходного файла в позе покоя — вместе со всеми
/// узлами над скелетом.
///
/// Узлы над корневой костью — это конверсия осей у экспортёра, и учитывать её
/// обязательно. Один файл считает «верхом» ось Y, другой Z; кости об этом не
/// знают, и если сравнивать позы покоя без разворота, две одинаковые T-позы
/// выглядят повёрнутыми друг относительно друга на девяносто градусов.
fn source_world_rest(doc: &Gltf, parents: &[Option<usize>], node: usize) -> Quat {
    let mut chain = Vec::new();
    let mut at = Some(node);
    while let Some(i) = at {
        chain.push(i);
        at = parents.get(i).copied().flatten();
    }

    let mut world = Quat::IDENTITY;
    for &i in chain.iter().rev() {
        if let Some(n) = doc.nodes.get(i) {
            world = (world * node_transform(n).rotation).normalize();
        }
    }
    world
}

/// Угол поворота кватерниона в градусах — для отчёта о том, насколько
/// разошлись риги.
fn rotation_angle(q: Quat) -> f32 {
    let w = q.normalize().w.abs().clamp(0.0, 1.0);
    2.0 * w.acos().to_degrees()
}

/// Таблица «узел -> родительский узел» по спискам детей.
fn node_parents(doc: &Gltf) -> Vec<Option<usize>> {
    let mut parents = vec![None; doc.nodes.len()];
    for (i, node) in doc.nodes.iter().enumerate() {
        for &child in &node.children {
            if child < parents.len() {
                parents[child] = Some(i);
            }
        }
    }
    parents
}

/// Приводит дорожку переносов из системы исходного файла в систему цели.
///
/// Длины костей задаёт целевой скелет, а не клип, поэтому:
/// * постоянная дорожка — это просто бинд-смещение кости, её надо выбросить,
///   иначе чужие пропорции переедут на нашу модель;
/// * меняющаяся дорожка (обычно только таз) — настоящее движение, и её надо
///   переложить как смещение относительно позы покоя своего файла.
///
/// Без этого клип из другого экспорта роняет фигуру: у одного файла таз
/// отсчитывается от пола (Y ~114), у другого — от самого таза (Y ~0).
/// Во сколько раз целевой риг крупнее исходного.
///
/// Смещения костей в позе покоя — это длины костей, поэтому их отношение и
/// даёт коэффициент. Он же чинит расхождение единиц: у персонажа из Mixamo
/// таз может лежать на -10, а в файле анимации оттуда же — на -104, и без
/// пересчёта корневое движение оказывается в десять раз крупнее фигуры.
fn rig_scale(doc: &Gltf, bone_of_node: &[usize], target: &Skeleton) -> f32 {
    let mut source_sum = 0.0;
    let mut target_sum = 0.0;

    for (node_index, &bone) in bone_of_node.iter().enumerate() {
        if bone == usize::MAX {
            continue;
        }
        let Some(node) = doc.nodes.get(node_index) else {
            continue;
        };
        source_sum += node_transform(node).translation.length();
        target_sum += target.bones[bone].bind_local.translation.length();
    }

    if source_sum > 1e-6 && target_sum > 1e-6 {
        target_sum / source_sum
    } else {
        1.0
    }
}

/// `parent_correction` turns an offset from the source parent's frame into
/// the target parent's (rigs from different exporters orient the space
/// above the hips differently: a forward step must stay forward).
fn retarget_translations(
    track: &mut Track,
    source_rest: Vec3,
    target_rest: Vec3,
    scale: f32,
    parent_correction: Quat,
) {
    const EPSILON: f32 = 1e-4;

    let first = match track.translations.first() {
        Some((_, v)) => *v,
        None => return,
    };

    let constant = track.translations.iter().all(|(_, v)| {
        (v.x - first.x).abs() < EPSILON
            && (v.y - first.y).abs() < EPSILON
            && (v.z - first.z).abs() < EPSILON
    });

    if constant {
        track.translations.clear();
        return;
    }

    let into_target = parent_correction.conjugate();
    for (_, v) in track.translations.iter_mut() {
        *v = target_rest + into_target.rotate((*v - source_rest) * scale);
    }
}

/// Собирает клипы по готовой таблице «узел -> кость». Таблица приходит либо
/// из скина того же файла, либо из сопоставления по именам, когда анимация
/// лежит в отдельном файле.
fn clips_from(
    doc: &Gltf,
    bin: &[u8],
    bone_of_node: &[usize],
    target: &Skeleton,
    warnings: &mut Vec<String>,
) -> Vec<AnimationClip> {
    let scale = rig_scale(doc, bone_of_node, target);
    if (scale - 1.0).abs() > 0.05 {
        warnings.push(format!("rig scale {scale:.3}x applied to root motion"));
    }

    // Поправки на расхождение поз покоя. Считаются один раз на файл.
    let parents = node_parents(doc);
    let world_rest = target_world_rest(target);
    let mut node_of_bone = vec![usize::MAX; target.bones.len()];
    for (node, &bone) in bone_of_node.iter().enumerate() {
        if bone != usize::MAX && node_of_bone[bone] == usize::MAX {
            node_of_bone[bone] = node;
        }
    }
    // Целевые повороты — тоже вместе с разворотом пространства модели.
    let target_root = target.root_rotation;
    let correction = |bone: usize| -> Quat {
        let node = node_of_bone[bone];
        if node == usize::MAX {
            return Quat::IDENTITY;
        }
        rest_correction(
            source_world_rest(doc, &parents, node),
            (target_root * world_rest[bone]).normalize(),
        )
    };

    // У корневой кости родителя нет, а разворот пространств компенсировать
    // всё равно надо — иначе он остаётся нескомпенсированным и заваливает
    // фигуру целиком. Роль родителя играет то, что лежит НАД скелетом: у
    // источника это узлы-предки, у цели — её собственный разворот.
    let parent_correction = |bone: usize| -> Quat {
        if let Some(p) = target.bones[bone].parent {
            return correction(p);
        }
        let above = node_of_bone
            .get(bone)
            .copied()
            .filter(|n| *n != usize::MAX)
            .and_then(|n| parents.get(n).copied().flatten())
            .map(|n| source_world_rest(doc, &parents, n))
            .unwrap_or(Quat::IDENTITY);
        rest_correction(above, target_root)
    };

    let mut worst = 0.0f32;

    let mut clips = Vec::new();

    for (ai, anim) in doc.animations.iter().enumerate() {
        let name = anim.name.clone().unwrap_or_else(|| format!("clip{ai}"));
        let mut tracks: Vec<Track> = Vec::new();
        let mut duration = 0.0f32;

        for channel in &anim.channels {
            let bone = match channel.target.node.and_then(|n| bone_of_node.get(n).copied()) {
                Some(b) if b != usize::MAX => b,
                // Каналы на узлы вне скелета (например, морфы) пропускаем.
                _ => continue,
            };

            let sampler = match anim.samplers.get(channel.sampler) {
                Some(s) => s,
                None => continue,
            };

            let interpolation = sampler.interpolation.as_deref().unwrap_or("LINEAR");
            if interpolation == "STEP" {
                let note = format!("clip {name} uses STEP, played as linear");
                if !warnings.contains(&note) {
                    warnings.push(note);
                }
            }

            let times = match read_floats(doc, bin, sampler.input) {
                Ok((t, _)) => t,
                Err(e) => {
                    warnings.push(format!("clip {name}: {e}"));
                    continue;
                }
            };
            let (values, comps) = match read_floats(doc, bin, sampler.output) {
                Ok(v) => v,
                Err(e) => {
                    warnings.push(format!("clip {name}: {e}"));
                    continue;
                }
            };

            // CUBICSPLINE хранит на каждый ключ тройку (вход, значение,
            // выход) — берём среднее и играем линейно.
            let cubic = interpolation == "CUBICSPLINE";
            let stride = if cubic { comps * 3 } else { comps };
            let value_at = |k: usize| -> &[f32] {
                let start = k * stride + if cubic { comps } else { 0 };
                &values[start..start + comps]
            };

            if let Some(t) = times.last() {
                duration = duration.max(*t);
            }

            let track = match tracks.iter_mut().position(|t| t.bone == bone) {
                Some(i) => &mut tracks[i],
                None => {
                    tracks.push(Track { bone, ..Default::default() });
                    tracks.last_mut().unwrap()
                }
            };

            let key_count = times.len().min(values.len() / stride);
            for k in 0..key_count {
                let time = times[k];
                let v = value_at(k);
                match channel.target.path.as_str() {
                    "translation" if comps >= 3 => {
                        track.translations.push((time, vec3(v[0], v[1], v[2])))
                    }
                    "rotation" if comps >= 4 => track.rotations.push((
                        time,
                        Quat { x: v[0], y: v[1], z: v[2], w: v[3] }.normalize(),
                    )),
                    "scale" if comps >= 3 => track.scales.push((time, vec3(v[0], v[1], v[2]))),
                    _ => {}
                }
            }

            // Перекладываем дорожку из системы исходного файла в целевую.
            let source_rest = channel
                .target
                .node
                .and_then(|n| doc.nodes.get(n))
                .map(node_transform)
                .unwrap_or(Transform::IDENTITY);
            let target_rest = target.bones[bone].bind_local;

            if channel.target.path == "translation" {
                retarget_translations(
                    track,
                    source_rest.translation,
                    target_rest.translation,
                    scale,
                    parent_correction(bone),
                );
            } else if channel.target.path == "rotation" {
                let bone_correction = correction(bone);
                worst = worst.max(rotation_angle(bone_correction));
                retarget_rotations(track, parent_correction(bone), bone_correction);
            }
        }

        if worst > 20.0 {
            let note = format!("rig rest differs by up to {worst:.0} deg, retargeted");
            if !warnings.contains(&note) {
                warnings.push(note);
            }
        }

        if tracks.is_empty() {
            continue;
        }

        clips.push(AnimationClip {
            name,
            duration,
            looping: true,
            tracks,
        });
    }

    clips
}

#[cfg(test)]
mod retarget_tests {
    use super::*;

    /// Кватернион и его отрицание — один и тот же поворот.
    fn same(a: Quat, b: Quat) -> bool {
        a.normalize().dot(b.normalize()).abs() > 0.999
    }

    fn track_with(q: Quat) -> Track {
        Track {
            bone: 0,
            rotations: vec![(0.0, q)],
            ..Default::default()
        }
    }

    /// Риги совпадают — дорожку трогать не за что. Это защита движений из
    /// Mixamo: у них риг тот же, что у персонажа, и любой пересчёт там был бы
    /// порчей.
    #[test]
    fn identical_rigs_leave_the_track_alone() {
        let q = Quat::from_axis_angle(vec3(0.3, 0.5, 0.8).normalize(), 0.9);
        let mut track = track_with(q);
        retarget_rotations(&mut track, Quat::IDENTITY, Quat::IDENTITY);
        assert!(same(track.rotations[0].1, q));
    }

    /// Главное свойство: мировое отклонение от позы покоя должно сохраниться,
    /// как бы ни расходились сами позы покоя.
    ///
    /// Проверяется на том самом случае, из-за которого всё и затевалось:
    /// бедро в записи движения развёрнуто ровно на 180 градусов относительно
    /// нашего рига, и старая формула гнула колено в обратную сторону.
    #[test]
    fn world_delta_survives_a_rest_mismatch() {
        // Позы покоя в мировых координатах.
        let src_parent = Quat::from_axis_angle(Vec3::Y, 0.4);
        let src_bone = src_parent * Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI);
        let tgt_parent = Quat::from_axis_angle(Vec3::Y, 0.4);
        let tgt_bone = tgt_parent;

        // Движение в исходном файле: локальный поворот кости.
        let animated_local = Quat::from_axis_angle(Vec3::X, 0.7)
            * (src_parent.conjugate() * src_bone);

        let c_parent = rest_correction(src_parent, tgt_parent);
        let c_bone = rest_correction(src_bone, tgt_bone);

        let mut track = track_with(animated_local);
        retarget_rotations(&mut track, c_parent, c_bone);

        // Родитель стоит в покое, поэтому мировой поворот цели — это её поза
        // покоя, домноженная на пересчитанный локальный поворот.
        let target_world = tgt_parent * track.rotations[0].1;

        // Ожидание: то же мировое отклонение, что и у источника.
        let source_world = src_parent * animated_local;
        let expected = source_world * src_bone.conjugate() * tgt_bone;

        assert!(
            same(target_world, expected),
            "мировое отклонение не сохранилось"
        );
    }

    /// А старая формула на этом же случае ошибается — иначе переписывать было
    /// бы нечего.
    #[test]
    fn the_local_only_formula_would_have_been_wrong_here() {
        let src_parent = Quat::from_axis_angle(Vec3::Y, 0.4);
        let src_bone = src_parent * Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI);
        let tgt_parent = Quat::from_axis_angle(Vec3::Y, 0.4);
        let tgt_bone = tgt_parent;

        let src_local_rest = src_parent.conjugate() * src_bone;
        let tgt_local_rest = tgt_parent.conjugate() * tgt_bone;
        let animated_local = Quat::from_axis_angle(Vec3::X, 0.7) * src_local_rest;

        // Как считалось раньше: поправка из локальных поз покоя.
        let old = (tgt_local_rest * src_local_rest.conjugate() * animated_local).normalize();

        let mut track = track_with(animated_local);
        retarget_rotations(
            &mut track,
            rest_correction(src_parent, tgt_parent),
            rest_correction(src_bone, tgt_bone),
        );

        assert!(
            !same(old, track.rotations[0].1),
            "старая и новая формулы совпали - тест ничего не проверяет"
        );
    }

    /// Тот случай, из-за которого поправку пришлось расширить: риги стоят в
    /// одной и той же T-позе, но записаны в разных осях — у одного файла
    /// «вверх» это Y, у другого Z.
    ///
    /// С учётом разворота пространства модели поправка обязана выродиться в
    /// единичную (позы-то одинаковые). Без учёта — нет, и именно поэтому у
    /// персонажа разъезжались ноги.
    #[test]
    fn model_space_axes_must_be_taken_into_account() {
        // Конверсия осей, которую экспортёр спрятал в узлах над скелетом.
        let axis_fix = Quat::from_axis_angle(Vec3::X, -std::f32::consts::FRAC_PI_2);

        // Одна и та же кость в мире; каждый файл хранит её в своих осях.
        let source_world = Quat::from_axis_angle(Vec3::Y, 0.6);
        let target_model = axis_fix.conjugate() * source_world;

        let with_axes = rest_correction(source_world, axis_fix * target_model);
        assert!(
            same(with_axes, Quat::IDENTITY),
            "одинаковые позы покоя должны давать единичную поправку"
        );

        let without_axes = rest_correction(source_world, target_model);
        assert!(
            !same(without_axes, Quat::IDENTITY),
            "без учёта осей поправка обязана отличаться - иначе тест пустой"
        );
        assert!(
            (rotation_angle(without_axes) - 90.0).abs() < 1.0,
            "перекос должен быть ровно тем самым разворотом осей: {} deg",
            rotation_angle(without_axes)
        );
    }

    /// У корневой кости родителя нет, и разворот пространств компенсировать
    /// нечем — если не подставить вместо родителя сам разворот, он остаётся
    /// в позе целиком и заваливает фигуру набок.
    #[test]
    fn the_root_bone_needs_the_axis_difference_as_its_parent() {
        // Источник живёт в мировых осях, цель — в своих, повёрнутых.
        let source_prefix = Quat::IDENTITY;
        let target_prefix = Quat::from_axis_angle(Vec3::X, -std::f32::consts::FRAC_PI_2);

        // Позы покоя корня совпадают в мире, но записаны по-разному.
        let source_rest = Quat::IDENTITY;
        let target_rest = target_prefix.conjugate();

        let animated = Quat::from_axis_angle(Vec3::Y, 0.5) * source_rest;

        let c_root = rest_correction(source_prefix * source_rest, target_prefix * target_rest);
        let c_parent = rest_correction(source_prefix, target_prefix);

        let mut track = track_with(animated);
        retarget_rotations(&mut track, c_parent, c_root);

        let expected = (source_prefix * animated)
            * (source_prefix * source_rest).conjugate()
            * (target_prefix * target_rest);
        assert!(
            same(target_prefix * track.rotations[0].1, expected),
            "корень встал не туда"
        );

        // А с единичной поправкой родителя — не туда: разворот осей остаётся
        // в позе.
        let mut naive = track_with(animated);
        retarget_rotations(&mut naive, Quat::IDENTITY, c_root);
        assert!(
            !same(target_prefix * naive.rotations[0].1, expected),
            "тест ничего не проверяет: обе ветки совпали"
        );
    }

    /// Поза покоя обязана переходить в позу покоя: если стоять неподвижно,
    /// фигура должна стоять так же, как наш персонаж, а не как чужой риг.
    #[test]
    fn rest_maps_to_rest() {
        let src_parent = Quat::from_axis_angle(vec3(0.2, 1.0, 0.1).normalize(), 1.1);
        let src_bone = src_parent * Quat::from_axis_angle(Vec3::Z, 2.0);
        let tgt_parent = Quat::from_axis_angle(Vec3::Y, -0.3);
        let tgt_bone = tgt_parent * Quat::from_axis_angle(Vec3::X, 0.25);

        let src_local_rest = src_parent.conjugate() * src_bone;
        let mut track = track_with(src_local_rest);
        retarget_rotations(
            &mut track,
            rest_correction(src_parent, tgt_parent),
            rest_correction(src_bone, tgt_bone),
        );

        let tgt_local_rest = tgt_parent.conjugate() * tgt_bone;
        assert!(same(track.rotations[0].1, tgt_local_rest));
    }
}
