//! Диагностика модели: `cargo run --bin inspect -- assets/character.glb`
//!
//! Отвечает на вопрос «а что вообще внутри этого файла» до того, как игра
//! молча покажет заглушку: есть ли скелет, сколько костей, какие анимации,
//! в каком масштабе и ориентации лежит меш.

use std::env;
use std::fs;

use some_game::engine::gltf;
use some_game::engine::skeleton::MAX_BONES;

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/character.glb".to_string());

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("не прочитать {path}: {e}");
            std::process::exit(1);
        }
    };

    println!("файл:      {path}");
    println!("размер:    {:.1} МБ", bytes.len() as f64 / (1024.0 * 1024.0));

    let model = match gltf::load_glb(&bytes) {
        Ok(m) => m,
        Err(e) => {
            println!();
            println!("НЕ ЗАГРУЖАЕТСЯ: {e}");
            std::process::exit(2);
        }
    };

    let bones = model.skeleton.len();
    let rigged = bones > 1;

    let tris = model.mesh.indices.len() / 3;
    let mb = bytes.len() as f64 / (1024.0 * 1024.0);
    println!("вершин:    {}", model.mesh.verts.len());
    println!("треуг.:    {tris}");
    println!("кусков:    {} (по одному вызову отрисовки)", model.mesh.submeshes.len());
    println!();

    println!("материалов: {}", model.materials.len());
    println!("текстур:    {}", model.textures.len());
    for (i, t) in model.textures.iter().enumerate() {
        let pot = t.width.is_power_of_two() && t.height.is_power_of_two();
        println!(
            "  {i}  {}x{}{}",
            t.width,
            t.height,
            if pot { "" } else { "  (не степень двойки: без мипмапов)" }
        );
    }
    println!();

    // Бюджет мобильного WebView: на PC пройдёт почти всё, а в Telegram
    // тяжёлая модель означает долгую первую загрузку и просадки на телефоне.
    if tris > 40_000 {
        println!("ВНИМАНИЕ: {tris} треугольников - для Telegram Mini App много (ориентир 40k)");
    }
    if mb > 4.0 {
        println!("ВНИМАНИЕ: {mb:.1} МБ - для Telegram Mini App много (ориентир 4 МБ)");
    }
    if tris > 40_000 || mb > 4.0 {
        println!("  на PC это работает, но модель стоит упростить (decimate) и пережать текстуры");
        println!();
    }

    if rigged {
        println!("СКЕЛЕТ ЕСТЬ: {bones} костей (потолок движка {MAX_BONES})");
    } else {
        println!("СКЕЛЕТА НЕТ: модель не риггнута, её нужно риггать");
    }

    if rigged {
        // Ключевые кости ищутся так же, как их ищет игра, — чтобы сразу было
        // видно, ляжет ли на риг аддитивный пульс.
        for (label, keys) in [
            ("таз ", ["hips", "mixamorig:Hips", "J_Bip_C_Hips", "pelvis"].as_slice()),
            ("спина", ["spine", "mixamorig:Spine", "J_Bip_C_Spine"].as_slice()),
            ("голова", ["head", "mixamorig:Head", "J_Bip_C_Head"].as_slice()),
        ] {
            match model.skeleton.find_like(keys) {
                Some(i) => println!("  {label}: {}", model.skeleton.bones[i].name),
                None => println!("  {label}: НЕ НАЙДЕНА"),
            }
        }

        println!();
        println!("все кости:");
        for (i, bone) in model.skeleton.bones.iter().enumerate() {
            let parent = match bone.parent {
                Some(p) => model.skeleton.bones[p].name.as_str(),
                None => "-",
            };
            println!("  {i:>3}  {:<28} <- {parent}", bone.name);
        }
    }

    println!();
    if model.clips.is_empty() {
        println!("АНИМАЦИЙ НЕТ");
    } else {
        println!("анимации: {}", model.clips.len());
        for (i, clip) in model.clips.iter().enumerate() {
            println!(
                "  {i}  {:<28} {:.2} с, дорожек {}",
                clip.name,
                clip.duration,
                clip.tracks.len()
            );
        }
    }

    println!();
    let (min, max) = (model.bounds_min, model.bounds_max);
    println!(
        "габариты:  X {:.2}..{:.2}   Y {:.2}..{:.2}   Z {:.2}..{:.2}",
        min.x, max.x, min.y, max.y, min.z, max.z
    );
    let (w, h, d) = (max.x - min.x, max.y - min.y, max.z - min.z);
    println!("размер:    {w:.2} x {h:.2} x {d:.2}");

    // Для гуманоида высота должна заметно превышать ширину и глубину.
    // Если это не так, модель почти наверняка лежит или повёрнута.
    if h < w || h < d {
        println!("ВНИМАНИЕ: модель не вытянута вверх — вероятно, повёрнута (нужен Y-вверх)");
    }
    println!("(игра сама приводит модель к росту 1.65 и ставит ногами на пол)");

    if !model.warnings.is_empty() {
        println!();
        println!("замечания:");
        for w in &model.warnings {
            println!("  - {w}");
        }
    }
}
