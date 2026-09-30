//! Анализ трека: `cargo run --bin beatmap -- assets/music/track.wav`
//!
//! Читает WAV, находит темп и сетку долей, кладёт рядом файл `.beatmap`.
//! Игра потом читает только его — считать спектр на телефоне при каждом
//! запуске незачем.
//!
//! Играть можно и OGG (его понимают обе платформы), но анализировать удобнее
//! WAV: разбор занимает сотню строк и не тянет зависимостей. Поэтому обычный
//! путь такой — держать оба файла, один для анализа, второй для раздачи.

use std::env;
use std::fs;
use std::path::PathBuf;

use some_game::engine::audio;
use some_game::engine::beat;

fn main() {
    let mut args = env::args().skip(1);
    let path = match args.next() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("использование: beatmap <файл.wav> [--bpm N] [--offset СЕК]");
            std::process::exit(1);
        }
    };

    // Ручные значения перебивают найденные: если ухо слышит, что детектор
    // ошибся вдвое, спорить с ним смысла нет.
    let (mut forced_bpm, mut forced_offset) = (None, None);
    while let Some(flag) = args.next() {
        let value = args.next().and_then(|v| v.parse::<f32>().ok());
        match (flag.as_str(), value) {
            ("--bpm", Some(v)) => forced_bpm = Some(v),
            ("--offset", Some(v)) => forced_offset = Some(v),
            _ => {
                eprintln!("не понял аргумент {flag}");
                std::process::exit(1);
            }
        }
    }

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("не прочитать {}: {e}", path.display());
            std::process::exit(1);
        }
    };

    let pcm = match audio::decode_wav(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("нужен WAV. сконвертировать: ffmpeg -i вход.mp3 выход.wav");
            std::process::exit(2);
        }
    };

    println!("файл:      {}", path.display());
    println!(
        "длина:     {:.1} с, {} Гц",
        pcm.samples.len() as f32 / pcm.sample_rate as f32,
        pcm.sample_rate
    );
    println!("анализ...");

    let mut map = beat::analyze(&pcm.samples, pcm.sample_rate);

    if let Some(bpm) = forced_bpm {
        map = beat::retime(&map, bpm, forced_offset.unwrap_or(map.offset));
        println!("темп задан вручную: {bpm:.2}");
    } else if let Some(offset) = forced_offset {
        map = beat::retime(&map, map.bpm, offset);
    }

    println!();
    println!("темп:      {:.2} BPM", map.bpm);
    println!("первая доля: {:.3} с", map.offset);
    println!("долей:     {}", map.beats.len());
    println!(
        "уверенность: {:.0}%{}",
        map.confidence * 100.0,
        if map.confidence < 0.35 {
            "  — низкая, стоит послушать и при необходимости задать --bpm"
        } else {
            ""
        }
    );

    // Первые доли распечатываются, чтобы можно было сверить на слух
    // в любом редакторе звука.
    let preview: Vec<String> = map.beats.iter().take(8).map(|t| format!("{t:.3}")).collect();
    println!("первые доли: {}", preview.join(", "));

    let out = path.with_extension("beatmap");
    match fs::write(&out, map.to_text()) {
        Ok(()) => println!("\nзаписано: {}", out.display()),
        Err(e) => {
            eprintln!("не записать {}: {e}", out.display());
            std::process::exit(1);
        }
    }
}
