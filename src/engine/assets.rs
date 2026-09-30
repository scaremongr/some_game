//! Поиск и загрузка файлов ассетов.
//!
//! В вебе путь всегда относителен странице, и вопросов не возникает.
//! На PC же относительный путь считается от рабочей папки процесса, а она
//! зависит от способа запуска: `cargo run` даёт корень проекта, а двойной
//! клик по exe — папку с exe. Поэтому файл ищется по нескольким корням.

/// Асинхронно читает ассет. Колбэк получает содержимое либо описание того,
/// где файл искали и не нашли.
pub fn load<F>(relative: &str, on_loaded: F)
where
    F: Fn(Result<Vec<u8>, String>) + 'static,
{
    let path = resolve(relative);

    #[cfg(not(target_arch = "wasm32"))]
    let path = match path {
        Ok(p) => p,
        Err(e) => {
            on_loaded(Err(e));
            return;
        }
    };

    let missing = format!("{relative} not found");
    miniquad::fs::load_file(&path, move |response| match response {
        Ok(bytes) => on_loaded(Ok(bytes)),
        // Текст ошибки от ОС локализован, а шрифт движка знает только ASCII,
        // поэтому сообщение пишем своё.
        Err(_) => on_loaded(Err(missing.clone())),
    });
}

#[cfg(target_arch = "wasm32")]
fn resolve(relative: &str) -> String {
    relative.to_string()
}

/// Перебирает вероятные корни и возвращает первый, где файл существует.
///
/// Порядок: рабочая папка, затем папка с exe и её родители — так покрывается
/// и `cargo run` из корня проекта, и запуск `target\debug\game.exe` напрямую.
#[cfg(not(target_arch = "wasm32"))]
fn resolve(relative: &str) -> Result<String, String> {
    use std::path::PathBuf;

    let mut roots: Vec<PathBuf> = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(PathBuf::from);
        // target\debug\game.exe -> target\debug -> target -> корень проекта.
        // Пяти уровней хватает с запасом на любую раскладку сборки.
        for _ in 0..5 {
            match dir {
                Some(d) => {
                    if !roots.contains(&d) {
                        roots.push(d.clone());
                    }
                    dir = d.parent().map(PathBuf::from);
                }
                None => break,
            }
        }
    }

    for root in &roots {
        let candidate = root.join(relative);
        if candidate.is_file() {
            println!("asset: {}", candidate.display());
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }

    // Список в консоль: в HUD он не поместится, а искать вслепую мучительно.
    eprintln!("asset '{relative}' not found, looked in:");
    for root in &roots {
        eprintln!("  {}", root.join(relative).display());
    }

    Err(format!("{relative} not found"))
}
