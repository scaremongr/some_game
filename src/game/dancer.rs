//! Персонаж и его асинхронная загрузка.
//!
//! Вынесено из сцены танца, потому что режим «повторяй за мной» использует
//! ровно то же самое: ту же модель, тот же список движений, ту же логику
//! докачки файлов. Дублировать её значило бы обречь одну из копий отстать.

use std::cell::RefCell;
use std::rc::Rc;

use crate::engine::assets;
use crate::engine::gltf;
use crate::engine::graphics::Graphics;
use crate::engine::math3::{vec3, Mat4, Quat, Vec3};
use crate::engine::mesh3::MeshData;
use crate::engine::render3d::SkinnedMesh;
use crate::engine::skeleton::{AnimationClip, AnimationPlayer, Skeleton};

use super::character;

/// Длительность кроссфейда между движениями. Короче — рвано, длиннее —
/// движение «опаздывает» за долей.
pub const FADE: f32 = 0.18;
/// Куда сцена смотрит за моделью.
const MODEL_PATH: &str = "assets/character.glb";
/// Список файлов с движениями, по одному имени в строке.
const MOVES_MANIFEST: &str = "assets/moves.txt";
/// К этой высоте приводится любая загруженная модель, чтобы кадр и свет
/// не зависели от того, в каких единицах её экспортировали.
pub const TARGET_HEIGHT: f32 = 1.65;

/// Всё, что нужно, чтобы показать и анимировать одного персонажа.
pub struct Character {
    pub skeleton: Skeleton,
    pub clips: Vec<AnimationClip>,
    /// Индексы клипов, которые считаются «движениями».
    pub moves: Vec<usize>,
    pub player: AnimationPlayer,
    pub globals: Vec<Mat4>,
    pub skin: Vec<Mat4>,

    pub mesh_data: MeshData,
    pub textures: Vec<gltf::TextureData>,
    pub materials: Vec<gltf::MaterialData>,
    /// Меш заливается на GPU при первой отрисовке: на момент создания сцены
    /// графического контекста ещё нет.
    pub mesh: Option<SkinnedMesh>,
    /// Приведение модели к общему масштабу и к стоянию ногами на полу.
    pub transform: Mat4,

    pub source: String,
    pub warnings: Vec<String>,

    /// Как эта модель стоит: уровень пола и центр по горизонтали, снятые
    /// с её собственной анимации. По ним выравниваются подгружаемые движения.
    pub reference_min_y: f32,
    pub reference_center: Vec3,
    /// Кость таза — к ней применяется поправка положения клипа.
    pub root_bone: usize,
}

impl Character {
    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        skeleton: Skeleton,
        clips: Vec<AnimationClip>,
        mesh_data: MeshData,
        textures: Vec<gltf::TextureData>,
        materials: Vec<gltf::MaterialData>,
        transform: Mat4,
        source: String,
        warnings: Vec<String>,
        reference_min_y: f32,
        reference_center: Vec3,
    ) -> Character {
        let mut player = AnimationPlayer::new(&skeleton);
        // Статичные клипы (у Mixamo это «T-Pose» на пару кадров) в список
        // движений не попадают: нажатие на такую кнопку выглядит поломкой.
        let moves: Vec<usize> = (0..clips.len())
            .filter(|i| !clips[*i].is_static())
            .collect();
        if !moves.is_empty() {
            player.play(moves[0], 0.0, 1.0);
        }

        // Для посадки клипа на пол нужна кость, которая тащит за собой всё
        // тело, — то есть корень скелета. Искать «таз» по имени тут неверно:
        // в блендеровских ригах `pelvis.L` это второстепенная кость бедра,
        // а корпус держит `spine_01`.
        let root_bone = skeleton
            .bones
            .iter()
            .position(|b| b.parent.is_none())
            .unwrap_or(0);

        Character {
            reference_min_y,
            reference_center,
            root_bone,
            player,
            moves,
            clips,
            skeleton,
            globals: Vec::new(),
            skin: Vec::new(),
            mesh_data,
            textures,
            materials,
            mesh: None,
            transform,
            source,
            warnings,
        }
    }

    pub fn procedural(seconds_per_beat: f32) -> Character {
        let skeleton = character::build_skeleton();
        let clips = character::build_clips(&skeleton, seconds_per_beat);
        let mesh_data = character::build_mesh(&skeleton);
        Character::assemble(
            skeleton,
            clips,
            mesh_data,
            Vec::new(),
            Vec::new(),
            Mat4::IDENTITY,
            "PLACEHOLDER".to_string(),
            Vec::new(),
            0.0,
            Vec3::ZERO,
        )
    }

    pub fn from_model(model: gltf::Model) -> Character {
        // Любую модель приводим к общей высоте и ставим ногами на y = 0:
        // экспортёры расходятся и в масштабе, и в положении начала координат.
        let height = model.height().max(1e-4);
        let scale = TARGET_HEIGHT / height;
        let center = model.center();
        // root_transform идёт первым: он приводит модель из пространства
        // экспортёра в нормальные единицы и оси, и уже её подгоняем по росту.
        let fit = Mat4::from_trs(
            vec3(
                -center.x * scale,
                -model.bounds_min.y * scale,
                -center.z * scale,
            ),
            Quat::IDENTITY,
            vec3(scale, scale, scale),
        );
        let transform = fit * model.root_transform;

        // Габариты сняты уже с учётом root_transform, а выравнивать клипы
        // надо в том же пространстве, в котором они считаются, — до него.
        let inverse_root = model.root_transform.invert();
        let reference_min_y = inverse_root.transform_point(model.bounds_min).y;
        let reference_center = inverse_root.transform_point(model.center());

        let source = format!("GLB  {} BONES", model.skeleton.len());

        let mut warnings = model.warnings;
        if model.clips.is_empty() {
            warnings.push("model has no animations".into());
        }

        Character::assemble(
            model.skeleton,
            model.clips,
            model.mesh,
            model.textures,
            model.materials,
            transform,
            source,
            warnings,
            reference_min_y,
            reference_center,
        )
    }

    pub fn current_move_name(&self) -> &str {
        self.player
            .current_clip()
            .and_then(|c| self.clips.get(c))
            .map(|c| c.name.as_str())
            .unwrap_or("BIND POSE")
    }

    /// Следующее движение по кругу — так удобнее пробовать с тача.
    pub fn next_move(&self) -> Option<usize> {
        if self.moves.is_empty() {
            return None;
        }
        let at = self
            .player
            .current_clip()
            .and_then(|c| self.moves.iter().position(|m| *m == c));
        let next = match at {
            Some(i) => (i + 1) % self.moves.len(),
            None => 0,
        };
        Some(self.moves[next])
    }

    /// Точка, за которой следит камера: центр всех костей в мировых
    /// координатах.
    ///
    /// Центр, а не таз: имя тазовой кости у разных ригов своё, а среднее по
    /// костям всегда внутри фигуры и не прыгает при смене движения.
    pub fn focus_point(&self) -> Vec3 {
        if self.globals.is_empty() {
            return vec3(0.0, TARGET_HEIGHT * 0.55, 0.0);
        }
        let mut sum = Vec3::ZERO;
        for m in &self.globals {
            sum += m.transform_point(Vec3::ZERO);
        }
        let local = sum * (1.0 / self.globals.len() as f32);
        self.transform.transform_point(local)
    }

    pub fn update(&mut self, time: f32, dt: f32) {
        // Движения жёстко привязаны к сетке долей, а не идут в своём темпе:
        // иначе после пары переключений танец разъедется с музыкой.
        if let Some(index) = self.player.current_clip() {
            let len = self.clips[index].duration;
            if len > 0.0 {
                self.player.set_time(time.rem_euclid(len));
            }
        }
        self.player.update(dt, &self.clips);

        self.skeleton
            .skin_matrices(&self.player.pose, &mut self.globals, &mut self.skin);
    }

    /// true, пока показывается процедурная заглушка: накладывать на неё
    /// движения из файлов бессмысленно, имена костей другие.
    pub fn mesh_is_placeholder(&self) -> bool {
        self.source == "PLACEHOLDER"
    }

    /// Добавляет клипы из отдельного файла движений в список движений.
    pub fn add_clips(&mut self, mut clips: Vec<AnimationClip>, source: &str) -> usize {
        clips.retain(|c| !c.is_static());
        let added = clips.len();
        if added == 0 {
            return 0;
        }
        for (i, clip) in clips.iter_mut().enumerate() {
            // Клип из чужого файла надо посадить на тот же пол, на котором
            // стоит собственная анимация модели.
            gltf::ground_clip(
                clip,
                &self.skeleton,
                &self.mesh_data,
                self.root_bone,
                self.reference_min_y,
                self.reference_center,
            );

            // Mixamo называет все свои клипы «mixamo.com», поэтому в списке
            // движений полезнее имя файла.
            clip.name = if added > 1 {
                format!("{source} {}", i + 1)
            } else {
                source.to_string()
            };
        }
        for clip in clips {
            self.clips.push(clip);
            self.moves.push(self.clips.len() - 1);
        }
        if self.player.current_clip().is_none() {
            if let Some(first) = self.moves.first().copied() {
                self.player.play(first, 0.0, 1.0);
            }
        }
        added
    }
}

/// Результат фоновой загрузки модели, который забирает сцена.
pub type PendingModel = Rc<RefCell<Option<Result<gltf::Model, String>>>>;
/// Список имён файлов движений из манифеста.
pub type PendingManifest = Rc<RefCell<Option<Vec<String>>>>;
/// Догруженные файлы движений: имя и содержимое либо ошибка.
pub type PendingMoves = Rc<RefCell<Vec<(String, Result<Vec<u8>, String>)>>>;
/// Просто содержимое файла, когда оно приедет.
pub type PendingBytes = Rc<RefCell<Option<Result<Vec<u8>, String>>>>;

pub fn load_bytes(path: &str) -> PendingBytes {
    let slot: PendingBytes = Rc::new(RefCell::new(None));
    let sink = slot.clone();
    assets::load(path, move |response| {
        *sink.borrow_mut() = Some(response);
    });
    slot
}


/// Персонаж вместе со всей асинхронной докачкой: модель, манифест движений,
/// сами файлы движений.
pub struct Dancer {
    pub character: Character,
    pending: PendingModel,
    manifest: PendingManifest,
    incoming_moves: PendingMoves,
    /// Файлы движений запрашиваются один раз, когда уже известен скелет.
    moves_requested: bool,
    /// Нужны ли отдельные файлы движений вообще.
    wants_moves: bool,
    /// Сообщение о загрузке, гаснет через несколько секунд.
    pub notice: Option<(String, f32)>,
}

impl Dancer {
    /// Персонаж со своим набором движений из `moves.txt`.
    pub fn new(seconds_per_beat: f32) -> Dancer {
        Dancer::build(seconds_per_beat, true)
    }

    /// Только модель, без отдельных файлов движений.
    ///
    /// Нужен там, где анимация приходит своя: тянуть по сети несколько
    /// мегабайт движений, которые никто не увидит, незачем.
    pub fn solo(seconds_per_beat: f32) -> Dancer {
        Dancer::build(seconds_per_beat, false)
    }

    fn build(seconds_per_beat: f32, wants_moves: bool) -> Dancer {
        let pending: PendingModel = Rc::new(RefCell::new(None));

        // Загрузка асинхронная: в вебе файл приезжает по сети, и синхронно
        // прочитать его нельзя в принципе.
        let slot = pending.clone();
        assets::load(MODEL_PATH, move |response| {
            let parsed = response.and_then(|bytes| gltf::load_glb(&bytes));
            *slot.borrow_mut() = Some(parsed);
        });

        // Манифест грузится параллельно с моделью; его отсутствие — норма.
        let manifest: PendingManifest = Rc::new(RefCell::new(None));
        let slot = manifest.clone();
        if wants_moves {
            assets::load(MOVES_MANIFEST, move |response| {
                let list = match response {
                    Ok(bytes) => String::from_utf8_lossy(&bytes)
                        .lines()
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .map(str::to_string)
                        .collect(),
                    Err(_) => Vec::new(),
                };
                *slot.borrow_mut() = Some(list);
            });
        }

        Dancer {
            character: Character::procedural(seconds_per_beat),
            pending,
            manifest,
            incoming_moves: Rc::new(RefCell::new(Vec::new())),
            moves_requested: false,
            wants_moves,
            notice: None,
        }
    }

    /// Забирает всё, что успело догрузиться.
    pub fn update(&mut self, dt: f32) {
        self.take_model();
        self.request_moves();
        self.take_moves();

        if let Some((_, ttl)) = &mut self.notice {
            *ttl -= dt;
            if *ttl <= 0.0 {
                self.notice = None;
            }
        }
    }

    /// Заливает меш на GPU при первой отрисовке: на момент создания сцены
    /// графического контекста ещё нет.
    pub fn ensure_mesh(&mut self, g: &mut Graphics) {
        if self.character.mesh.is_none() {
            self.character.mesh = Some(g.upload_skinned(
                &self.character.mesh_data,
                &self.character.textures,
                &self.character.materials,
            ));
        }
    }

    fn take_model(&mut self) {
        let result = match self.pending.borrow_mut().take() {
            Some(r) => r,
            None => return,
        };

        match result {
            Ok(model) => {
                let (bones, clips) = (model.skeleton.len(), model.clips.len());
                self.character = Character::from_model(model);
                self.notice = Some((format!("MODEL LOADED: {bones} BONES, {clips} CLIPS"), 4.0));
            }
            Err(e) => {
                // Заглушка остаётся, поэтому сцена продолжает работать.
                self.notice = Some((format!("NO MODEL: {}", e.to_ascii_uppercase()), 6.0));
            }
        }
    }

    fn request_moves(&mut self) {
        if !self.wants_moves || self.moves_requested || self.character.mesh_is_placeholder() {
            return;
        }
        let names = match self.manifest.borrow().clone() {
            Some(names) => names,
            None => return,
        };
        self.moves_requested = true;

        for name in names {
            let path = format!("assets/{name}");
            let slot = self.incoming_moves.clone();
            let label = name.clone();
            assets::load(&path, move |response| {
                slot.borrow_mut().push((label.clone(), response));
            });
        }
    }

    fn take_moves(&mut self) {
        let arrived: Vec<(String, Result<Vec<u8>, String>)> =
            self.incoming_moves.borrow_mut().drain(..).collect();

        for (name, payload) in arrived {
            let label = name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&name)
                .trim_end_matches(".glb")
                .to_ascii_uppercase();

            let result =
                payload.and_then(|bytes| gltf::load_clips(&bytes, &self.character.skeleton));

            match result {
                Ok(clips) if !clips.is_empty() => {
                    let added = self.character.add_clips(clips, &label);
                    if added > 0 {
                        self.notice = Some((format!("+{added} MOVE: {label}"), 3.0));
                    }
                }
                Ok(_) => self
                    .character
                    .warnings
                    .push(format!("{label}: no animation inside")),
                Err(e) => self.character.warnings.push(format!("{label}: {e}")),
            }
        }
    }
}
