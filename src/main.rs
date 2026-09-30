use some_game::engine::app::{self, Config, Scene};
use some_game::game;
use some_game::game::palette;

fn main() {
    // Аргументом можно открыть сцену сразу, минуя меню. Нужно для проверок:
    // прокликивать меню синтетическим вводом ненадёжно, а посмотреть на
    // конкретную сцену надо часто.
    let scene = std::env::args().nth(1).unwrap_or_default();

    app::run(
        Config {
            title: "PULSE / Arena",
            window_width: 1280,
            window_height: 720,
            letterbox: palette::LETTERBOX,
            background: palette::BACKGROUND,
            ..Default::default()
        },
        move || -> Box<dyn Scene> {
            match scene.as_str() {
                "dance" => Box::new(game::dance::DanceScene::new()),
                "mirror" => Box::new(game::mirror::MirrorScene::new()),
                "clips" => Box::new(game::clips::ClipsScene::new()),
                _ => Box::new(game::fight::FightScene::new()),
            }
        },
    );
}
