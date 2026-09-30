//! Ритм-механика: подсказки, судейство попаданий, счёт.
//!
//! Отделено от сцены намеренно — здесь нет ни рендера, ни ввода, только
//! правила. Их удобно менять и читать, не продираясь через отрисовку.

use crate::engine::beat::BeatMap;

/// Окна попадания в секундах. Числа взяты близкими к жанровой норме:
/// «идеально» примерно в кадр при 60 Гц, «мимо» — за пределами того, что
/// человек ещё считает попаданием.
pub const PERFECT_WINDOW: f32 = 0.045;
pub const GOOD_WINDOW: f32 = 0.095;
pub const MISS_WINDOW: f32 = 0.150;

/// За сколько секунд до доли подсказка появляется на дорожке.
pub const LOOKAHEAD: f32 = 2.0;

/// Как часто выдаются подсказки, в долях. Раз в такт — то, что успевает
/// прочитать человек, и то, на что рассчитаны движения длиной в два такта.
pub const BEATS_PER_CUE: i32 = 4;

/// Сколько кнопок в игре.
///
/// Четыре — не «сколько влезло», а решение: на телефоне ряд кнопок отъедает
/// низ экрана, и каждый лишний ряд уходит из сцены. Плюс четыре варианта
/// человек успевает разобрать за долю, а девять — уже нет.
///
/// Движений может быть загружено больше; в игре участвуют первые четыре,
/// то есть порядок в `moves.txt` решает, какие именно.
pub const MAX_LANES: usize = 4;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Grade {
    Perfect,
    Good,
    Miss,
}

impl Grade {
    pub fn label(self) -> &'static str {
        match self {
            Grade::Perfect => "PERFECT",
            Grade::Good => "GOOD",
            Grade::Miss => "MISS",
        }
    }

    pub fn points(self) -> u32 {
        match self {
            Grade::Perfect => 100,
            Grade::Good => 50,
            Grade::Miss => 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum CueState {
    Pending,
    Done(Grade),
}

#[derive(Clone, Copy)]
pub struct Cue {
    /// Момент доли, к которому надо нажать.
    pub time: f32,
    /// Индекс движения в списке персонажа.
    pub move_index: usize,
    /// Номер клавиши, показываемый игроку (1..9).
    pub key: usize,
    pub state: CueState,
}

pub struct Rhythm {
    pub cues: Vec<Cue>,
    pub score: u32,
    pub combo: u32,
    pub best_combo: u32,
    pub hits: u32,
    pub total: u32,
    /// Последняя оценка и сколько ей осталось светиться.
    pub last: Option<(Grade, f32, f32)>,
    /// Номер доли, для которой уже выдана подсказка.
    next_cue_beat: i32,
    /// Простейший генератор: своего добра хватает, тянуть зависимость незачем.
    seed: u32,
}

impl Rhythm {
    pub fn new() -> Rhythm {
        Rhythm {
            cues: Vec::new(),
            score: 0,
            combo: 0,
            best_combo: 0,
            hits: 0,
            total: 0,
            last: None,
            next_cue_beat: 0,
            seed: 0x2545F491,
        }
    }

    pub fn reset(&mut self) {
        let seed = self.seed;
        *self = Rhythm::new();
        self.seed = seed;
    }

    fn next_random(&mut self) -> u32 {
        // xorshift32: короткий, детерминированный, для выбора движений хватает.
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed
    }

    pub fn accuracy(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.hits as f32 / self.total as f32
        }
    }

    /// Досоздаёт подсказки на ближайшие секунды и закрывает просроченные.
    pub fn update(&mut self, map: &BeatMap, time: f32, dt: f32, move_count: usize) {
        // Подсказка не должна называть кнопку, которой нет на экране.
        let move_count = move_count.min(MAX_LANES);
        if move_count == 0 {
            return;
        }

        // Подсказки создаются заранее, чтобы игрок успел их прочитать.
        let horizon = time + LOOKAHEAD;
        let spb = map.seconds_per_beat();
        loop {
            let beat = self.next_cue_beat;
            let at = map.offset + beat as f32 * spb;
            if at > horizon {
                break;
            }
            if at >= time {
                let key = (self.next_random() as usize % move_count) + 1;
                self.cues.push(Cue {
                    time: at,
                    move_index: key - 1,
                    key,
                    state: CueState::Pending,
                });
            }
            self.next_cue_beat += BEATS_PER_CUE;
        }

        // Просроченные подсказки — промах.
        for cue in self.cues.iter_mut() {
            if cue.state == CueState::Pending && time > cue.time + MISS_WINDOW {
                cue.state = CueState::Done(Grade::Miss);
                self.combo = 0;
                self.total += 1;
                self.last = Some((Grade::Miss, 0.0, 1.0));
            }
        }

        // Уехавшие за экран больше не нужны.
        self.cues.retain(|c| c.time > time - 1.0);

        if let Some((_, _, ttl)) = &mut self.last {
            *ttl -= dt * 1.6;
            if *ttl <= 0.0 {
                self.last = None;
            }
        }
    }

    /// Обрабатывает нажатие. Возвращает движение, которое надо включить,
    /// если нажатие засчитано.
    ///
    /// Судится ближайшая непройденная подсказка в пределах окна промаха.
    /// Нажатие не по той клавише — тоже промах: иначе можно было бы жать
    /// всё подряд и всегда попадать.
    pub fn press(&mut self, key: usize, time: f32) -> Option<usize> {
        let nearest = self
            .cues
            .iter_mut()
            .filter(|c| c.state == CueState::Pending)
            .filter(|c| (c.time - time).abs() <= MISS_WINDOW)
            .min_by(|a, b| {
                (a.time - time)
                    .abs()
                    .total_cmp(&(b.time - time).abs())
            });

        let Some(cue) = nearest else {
            // Вне всяких окон — просто смена движения, без оценки.
            return None;
        };

        let error = time - cue.time;
        let grade = if key != cue.key {
            Grade::Miss
        } else if error.abs() <= PERFECT_WINDOW {
            Grade::Perfect
        } else if error.abs() <= GOOD_WINDOW {
            Grade::Good
        } else {
            Grade::Miss
        };

        cue.state = CueState::Done(grade);
        let move_index = cue.move_index;

        self.total += 1;
        self.last = Some((grade, error, 1.0));

        match grade {
            Grade::Miss => self.combo = 0,
            _ => {
                self.hits += 1;
                self.combo += 1;
                self.best_combo = self.best_combo.max(self.combo);
                self.score += grade.points() * (1 + self.combo.min(50) / 10);
            }
        }

        if grade == Grade::Miss {
            None
        } else {
            Some(move_index)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::beat::BeatMap;

    fn map(bpm: f32) -> BeatMap {
        let spb = 60.0 / bpm;
        let beats = (0..64).map(|i| i as f32 * spb).collect();
        BeatMap {
            bpm,
            offset: 0.0,
            beats,
            confidence: 1.0,
            duration: 64.0 * spb,
        }
    }

    /// Подсказки должны появляться заранее и ровно раз в такт.
    #[test]
    fn cues_appear_ahead_of_time() {
        let mut r = Rhythm::new();
        let m = map(120.0);
        r.update(&m, 0.0, 0.016, 3);

        assert!(!r.cues.is_empty());
        assert!(r.cues.iter().all(|c| c.time <= LOOKAHEAD + 1e-3));
        assert!(r.cues.iter().all(|c| (1..=3).contains(&c.key)));

        // Раз в четыре доли: при 120 BPM это ровно 2 секунды.
        if r.cues.len() >= 2 {
            let gap = r.cues[1].time - r.cues[0].time;
            assert!((gap - 2.0).abs() < 1e-3, "шаг подсказок {gap}");
        }
    }

    /// Подсказка не должна называть кнопку, которой нет на экране.
    /// Раньше при десяти загруженных движениях по дорожке уезжала «10».
    #[test]
    fn cues_never_exceed_the_lane_count() {
        let mut r = Rhythm::new();
        let m = map(120.0);
        for step in 0..20 {
            r.update(&m, step as f32 * 0.5, 0.016, 12);
        }
        assert!(!r.cues.is_empty());
        assert!(
            r.cues.iter().all(|c| (1..=MAX_LANES).contains(&c.key)),
            "клавиши: {:?}",
            r.cues.iter().map(|c| c.key).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exact_press_is_perfect() {
        let mut r = Rhythm::new();
        let m = map(120.0);
        r.update(&m, 0.0, 0.016, 3);
        let cue = r.cues[0];

        let played = r.press(cue.key, cue.time);
        assert_eq!(played, Some(cue.move_index));
        assert_eq!(r.last.unwrap().0, Grade::Perfect);
        assert_eq!(r.combo, 1);
        assert!(r.score > 0);
    }

    #[test]
    fn slightly_late_press_is_good() {
        let mut r = Rhythm::new();
        let m = map(120.0);
        r.update(&m, 0.0, 0.016, 3);
        let cue = r.cues[0];

        r.press(cue.key, cue.time + (PERFECT_WINDOW + GOOD_WINDOW) * 0.5);
        assert_eq!(r.last.unwrap().0, Grade::Good);
        assert_eq!(r.combo, 1);
    }

    /// Нажатие не по той клавише — промах, иначе можно было бы жать всё
    /// подряд и всегда попадать.
    #[test]
    fn wrong_key_is_a_miss() {
        let mut r = Rhythm::new();
        let m = map(120.0);
        r.update(&m, 0.0, 0.016, 3);
        let cue = r.cues[0];
        let wrong = if cue.key == 1 { 2 } else { 1 };

        assert_eq!(r.press(wrong, cue.time), None);
        assert_eq!(r.last.unwrap().0, Grade::Miss);
        assert_eq!(r.combo, 0);
    }

    /// Нажатие вне любых окон не судится вовсе: это просто смена движения.
    #[test]
    fn press_outside_window_is_not_judged() {
        let mut r = Rhythm::new();
        let m = map(120.0);
        r.update(&m, 0.0, 0.016, 3);
        let cue = r.cues[0];

        assert_eq!(r.press(cue.key, cue.time - MISS_WINDOW * 3.0), None);
        assert!(r.last.is_none());
        assert_eq!(r.total, 0);
    }

    /// Пропущенная подсказка обязана закрыться сама и сбросить комбо.
    #[test]
    fn expired_cue_becomes_a_miss() {
        let mut r = Rhythm::new();
        let m = map(120.0);
        r.update(&m, 0.0, 0.016, 3);
        let cue = r.cues[0];
        r.combo = 5;

        r.update(&m, cue.time + MISS_WINDOW + 0.01, 0.016, 3);

        assert_eq!(r.combo, 0);
        assert_eq!(r.last.unwrap().0, Grade::Miss);
        assert!(r.total >= 1);
    }

    #[test]
    fn combo_raises_the_score() {
        let mut r = Rhythm::new();
        let m = map(120.0);

        let mut first = 0;
        for step in 0..12 {
            r.update(&m, step as f32 * 2.0, 0.016, 3);
            let pending: Vec<_> = r
                .cues
                .iter()
                .filter(|c| c.state == CueState::Pending)
                .map(|c| (c.key, c.time))
                .collect();
            for (key, time) in pending {
                let before = r.score;
                r.press(key, time);
                if step == 0 {
                    first = r.score - before;
                }
            }
        }

        assert!(r.best_combo >= 5, "комбо {}", r.best_combo);
        assert!(r.accuracy() > 0.99);
        assert!(first > 0);
    }
}
