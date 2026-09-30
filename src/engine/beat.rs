//! Анализ ритма: из PCM получаются темп и сетка долей.
//!
//! Считается офлайн, отдельным инструментом, а игра читает готовый результат.
//! Так и должно быть: спектральный анализ трёхминутного трека на телефоне при
//! каждом запуске — это секунды ожидания и разряженная батарея на ровном месте.
//!
//! Цепочка стандартная для определения темпа:
//! 1. спектральный поток — насколько резко вырос спектр по сравнению с
//!    предыдущим окном; удары дают всплески;
//! 2. автокорреляция этой огибающей — период, повторяющийся чаще всего;
//! 3. фаза — куда этот период приложить, чтобы доли легли на всплески.

/// Частота кадров огибающей: 44100 / 512 ≈ 86 кадров в секунду.
/// Шаг в 11.6 мс мельче любого разумного окна попадания.
const HOP: usize = 512;
const WINDOW: usize = 1024;

/// Границы поиска темпа. Ниже 60 и выше 190 в танцевальной музыке почти не
/// бывает, а широкий диапазон только добавляет ошибок кратности.
pub const MIN_BPM: f32 = 60.0;
pub const MAX_BPM: f32 = 190.0;

pub struct BeatMap {
    pub bpm: f32,
    /// Время первой доли, секунды.
    pub offset: f32,
    /// Все доли трека.
    pub beats: Vec<f32>,
    /// Насколько уверенно нашёлся период, 0..1. Низкое значение означает,
    /// что у трека плавающий темп или слабо выраженная доля.
    pub confidence: f32,
    pub duration: f32,
}

impl BeatMap {
    pub fn seconds_per_beat(&self) -> f32 {
        60.0 / self.bpm
    }

    /// Положение в долях для момента времени — дробное, как и нужно сцене.
    pub fn beat_position(&self, time: f32) -> f32 {
        (time - self.offset) / self.seconds_per_beat()
    }

    /// Насколько момент далёк от ближайшей доли, в секундах со знаком.
    /// Отрицательное — рано, положительное — поздно.
    pub fn distance_to_beat(&self, time: f32) -> f32 {
        let spb = self.seconds_per_beat();
        let b = (time - self.offset) / spb;
        (b - b.round()) * spb
    }
}

/// Текстовый формат карты: игре достаточно первых двух чисел, остальное
/// удобно глазами проверять.
impl BeatMap {
    pub fn to_text(&self) -> String {
        format!(
            "# beatmap v1\n\
             bpm {:.4}\n\
             offset {:.4}\n\
             confidence {:.3}\n\
             duration {:.3}\n\
             # долей всего: {}\n",
            self.bpm,
            self.offset,
            self.confidence,
            self.duration,
            self.beats.len()
        )
    }

    pub fn from_text(text: &str) -> Result<BeatMap, String> {
        let mut bpm = None;
        let mut offset = 0.0f32;
        let mut confidence = 0.0f32;
        let mut duration = 0.0f32;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (key, value) = match (parts.next(), parts.next()) {
                (Some(k), Some(v)) => (k, v),
                _ => continue,
            };
            let number: f32 = value.parse().map_err(|_| format!("bad value in '{line}'"))?;
            match key {
                "bpm" => bpm = Some(number),
                "offset" => offset = number,
                "confidence" => confidence = number,
                "duration" => duration = number,
                _ => {}
            }
        }

        let bpm = bpm.ok_or("beatmap has no bpm")?;
        if !(MIN_BPM / 2.0..=MAX_BPM * 2.0).contains(&bpm) {
            return Err(format!("bpm {bpm} is out of range"));
        }

        // Сами доли не храним: они однозначно восстанавливаются из темпа
        // и сдвига, а файл остаётся в пять строк.
        let spb = 60.0 / bpm;
        let mut beats = Vec::new();
        let mut t = offset;
        while t < duration {
            if t >= 0.0 {
                beats.push(t);
            }
            t += spb;
        }

        Ok(BeatMap { bpm, offset, beats, confidence, duration })
    }
}

/// Пересобирает карту с другим темпом или сдвигом — для ручной правки,
/// когда детектор ошибся вдвое или промахнулся фазой.
pub fn retime(map: &BeatMap, bpm: f32, offset: f32) -> BeatMap {
    let spb = 60.0 / bpm;
    let mut beats = Vec::new();
    let mut t = offset;
    while t < map.duration {
        if t >= 0.0 {
            beats.push(t);
        }
        t += spb;
    }
    BeatMap {
        bpm,
        offset,
        beats,
        confidence: map.confidence,
        duration: map.duration,
    }
}

/// Разбирает трек и строит сетку долей.
///
/// `samples` — моно-сигнал; стерео нужно свести заранее, потому что для
/// поиска ритма разница между каналами только мешает.
pub fn analyze(samples: &[f32], sample_rate: u32) -> BeatMap {
    let duration = samples.len() as f32 / sample_rate as f32;
    let envelope = onset_envelope(samples);
    let frame_rate = sample_rate as f32 / HOP as f32;

    let (rough_bpm, confidence) = estimate_tempo(&envelope, frame_rate);
    // Автокорреляция даёт период с точностью до кадра (11.6 мс). Для игры
    // этого мало: ошибка в 0.5% за три минуты уводит сетку на секунду.
    // Поэтому период и фаза уточняются совместно, уже с долями кадра.
    let (spb, offset) = refine(&envelope, frame_rate, 60.0 / rough_bpm);
    let bpm = 60.0 / spb;

    let mut beats = Vec::new();
    let mut t = offset;
    while t < duration {
        if t >= 0.0 {
            beats.push(t);
        }
        t += spb;
    }

    BeatMap { bpm, offset, beats, confidence, duration }
}

/// Спектральный поток: сумма приростов амплитуд по частотам между соседними
/// окнами. Растёт там, где в звуке появляется что-то новое, — то есть на
/// атаках, а не на длинных нотах.
fn onset_envelope(samples: &[f32]) -> Vec<f32> {
    if samples.len() < WINDOW {
        return Vec::new();
    }

    let window: Vec<f32> = (0..WINDOW)
        .map(|i| {
            // Окно Ханна: без него на краях окна возникают ложные всплески.
            let t = i as f32 / (WINDOW - 1) as f32;
            0.5 - 0.5 * (std::f32::consts::TAU * t).cos()
        })
        .collect();

    let bins = WINDOW / 2;
    let mut previous = vec![0.0f32; bins];
    let mut current = vec![0.0f32; bins];
    let mut envelope = Vec::with_capacity(samples.len() / HOP);

    let mut re = vec![0.0f32; WINDOW];
    let mut im = vec![0.0f32; WINDOW];

    let mut at = 0;
    while at + WINDOW <= samples.len() {
        for i in 0..WINDOW {
            re[i] = samples[at + i] * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);

        let mut flux = 0.0;
        for k in 0..bins {
            current[k] = (re[k] * re[k] + im[k] * im[k]).sqrt();
            // Считается только рост: спад громкости ударом не является.
            let diff = current[k] - previous[k];
            if diff > 0.0 {
                flux += diff;
            }
        }
        std::mem::swap(&mut previous, &mut current);

        envelope.push(flux);
        at += HOP;
    }

    normalize(&mut envelope);
    envelope
}

/// Вычитает скользящее среднее и обрезает отрицательное.
///
/// Без этого громкая часть трека перевешивает тихую, и автокорреляция
/// находит период только в припеве.
fn normalize(envelope: &mut Vec<f32>) {
    const WINDOW_FRAMES: usize = 43; // около полусекунды
    let source = envelope.clone();
    let n = source.len();

    for i in 0..n {
        let from = i.saturating_sub(WINDOW_FRAMES);
        let to = (i + WINDOW_FRAMES + 1).min(n);
        let mean: f32 = source[from..to].iter().sum::<f32>() / (to - from) as f32;
        envelope[i] = (source[i] - mean).max(0.0);
    }

    let peak = envelope.iter().cloned().fold(0.0f32, f32::max);
    if peak > 1e-6 {
        for v in envelope.iter_mut() {
            *v /= peak;
        }
    }
}

/// Ищет период автокорреляцией огибающей.
///
/// Возвращает темп и уверенность — отношение найденного пика к среднему
/// уровню корреляции.
fn estimate_tempo(envelope: &[f32], frame_rate: f32) -> (f32, f32) {
    if envelope.len() < 128 {
        return (120.0, 0.0);
    }

    let min_lag = (frame_rate * 60.0 / MAX_BPM).round() as usize;
    let max_lag = (frame_rate * 60.0 / MIN_BPM).round() as usize;
    let max_lag = max_lag.min(envelope.len() / 2);
    if min_lag >= max_lag {
        return (120.0, 0.0);
    }

    let mut scores = vec![0.0f32; max_lag + 1];
    for lag in min_lag..=max_lag {
        let mut sum = 0.0;
        for i in lag..envelope.len() {
            sum += envelope[i] * envelope[i - lag];
        }
        // Длинные задержки суммируют меньше слагаемых — нормируем.
        scores[lag] = sum / (envelope.len() - lag) as f32;
    }

    // Доли часто дают пик и на половине, и на удвоенном периоде. Складываем
    // кратные задержки: настоящий период получает поддержку от всех своих
    // гармоник, а случайный — нет.
    let mut best_lag = min_lag;
    let mut best_score = f32::MIN;
    for lag in min_lag..=max_lag {
        let mut score = scores[lag];
        for k in 2..=4 {
            let harmonic = lag * k;
            if harmonic <= max_lag {
                score += scores[harmonic] / k as f32;
            }
        }
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }

    let mean: f32 = scores[min_lag..=max_lag].iter().sum::<f32>() / (max_lag - min_lag + 1) as f32;
    let confidence = if mean > 1e-9 {
        ((scores[best_lag] / mean - 1.0) / 3.0).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let bpm = 60.0 * frame_rate / best_lag as f32;
    (fold_into_range(bpm), confidence)
}

/// Приводит темп в привычный диапазон удвоением или делением.
///
/// Автокорреляция одинаково охотно находит и половинный, и двойной темп;
/// человек же почти всегда считает долю в пределах 70..150.
fn fold_into_range(mut bpm: f32) -> f32 {
    while bpm < 70.0 {
        bpm *= 2.0;
    }
    while bpm > 150.0 {
        bpm /= 2.0;
    }
    bpm
}

/// Совместно уточняет период и фазу вокруг грубой оценки.
///
/// Критерий прямой: сумма огибающей ровно в тех точках, куда встанут доли.
/// Максимум этой суммы и есть искомая сетка. Перебор по периоду нужен
/// потому, что ошибка в период накапливается: полпроцента на трёхминутном
/// треке — это секунда расхождения к концу.
///
/// Возвращает (секунд на долю, время первой доли).
fn refine(envelope: &[f32], frame_rate: f32, rough_spb: f32) -> (f32, f32) {
    if envelope.is_empty() {
        return (rough_spb, 0.0);
    }

    const PERIOD_STEPS: usize = 240;
    const PHASE_STEPS: usize = 160;
    const SPAN: f32 = 0.04; // ±4% вокруг грубой оценки

    let mut best = (rough_spb, 0.0f32, f32::MIN);

    for p in 0..=PERIOD_STEPS {
        let k = p as f32 / PERIOD_STEPS as f32 * 2.0 - 1.0;
        let period = rough_spb * (1.0 + k * SPAN) * frame_rate;
        if period < 2.0 {
            continue;
        }

        for q in 0..PHASE_STEPS {
            let phase = q as f32 / PHASE_STEPS as f32 * period;

            let mut score = 0.0;
            let mut at = phase;
            while at < envelope.len() as f32 - 1.0 {
                score += sample_envelope(envelope, at);
                at += period;
            }
            // Нормируем на число долей, иначе короткий период выигрывает
            // просто количеством слагаемых.
            let beats = ((envelope.len() as f32 - phase) / period).max(1.0);
            let score = score / beats.sqrt();

            if score > best.2 {
                best = (period / frame_rate, phase / frame_rate, score);
            }
        }
    }

    (best.0, best.1)
}

/// Линейная интерполяция огибающей: доля почти никогда не попадает точно
/// в границу кадра.
fn sample_envelope(envelope: &[f32], at: f32) -> f32 {
    if at < 0.0 {
        return 0.0;
    }
    let i = at as usize;
    if i + 1 >= envelope.len() {
        return envelope.last().copied().unwrap_or(0.0);
    }
    let t = at - i as f32;
    envelope[i] * (1.0 - t) + envelope[i + 1] * t
}

/// Итеративное БПФ по основанию 2. Длина обязана быть степенью двойки.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());

    // Перестановка с обращением битов.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let mut len = 2;
    while len <= n {
        let angle = -std::f32::consts::TAU / len as f32;
        let (wr, wi) = (angle.cos(), angle.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (ar, ai) = (re[i + k], im[i + k]);
                let (br, bi) = (re[i + k + len / 2], im[i + k + len / 2]);
                let (tr, ti) = (br * cr - bi * ci, br * ci + bi * cr);
                re[i + k] = ar + tr;
                im[i + k] = ai + ti;
                re[i + k + len / 2] = ar - tr;
                im[i + k + len / 2] = ai - ti;
                let next_cr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = next_cr;
            }
            i += len;
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Синтетический трек: короткие щелчки строго по долям плюс слабый шум,
    /// чтобы сигнал не был стерильным.
    fn click_track(bpm: f32, seconds: f32, offset: f32, sample_rate: u32) -> Vec<f32> {
        let total = (seconds * sample_rate as f32) as usize;
        let mut out = vec![0.0f32; total];
        let spb = 60.0 / bpm;

        // Свой генератор шума: детерминированный и без зависимостей.
        let mut seed = 12345u32;
        let mut noise = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };

        let mut t = offset;
        while t < seconds {
            let start = (t * sample_rate as f32) as usize;
            let len = (0.04 * sample_rate as f32) as usize;
            for i in 0..len {
                if start + i >= total {
                    break;
                }
                let phase = i as f32 / sample_rate as f32;
                let env = (-phase * 40.0).exp();
                out[start + i] += (std::f32::consts::TAU * 90.0 * phase).sin() * env;
            }
            t += spb;
        }

        for v in out.iter_mut() {
            *v = (*v + noise() * 0.01).clamp(-1.0, 1.0);
        }
        out
    }

    fn check(bpm: f32, offset: f32) {
        let sr = 44100;
        let samples = click_track(bpm, 16.0, offset, sr);
        let map = analyze(&samples, sr);

        let bpm_error = (map.bpm - bpm).abs() / bpm;
        assert!(
            bpm_error < 0.01,
            "темп {bpm}: нашли {:.2}, ошибка {:.2}%",
            map.bpm,
            bpm_error * 100.0
        );

        // Фаза определена с точностью до целого числа долей, поэтому
        // сравнивать надо по остатку от периода.
        let spb = 60.0 / bpm;
        let mut phase_error = (map.offset - offset).rem_euclid(spb);
        if phase_error > spb * 0.5 {
            phase_error -= spb;
        }
        assert!(
            phase_error.abs() < 0.030,
            "сдвиг {offset}: нашли {:.3}, ошибка {:.0} мс",
            map.offset,
            phase_error * 1000.0
        );
    }

    #[test]
    fn finds_common_tempos() {
        check(120.0, 0.0);
        check(128.0, 0.25);
        check(100.0, 0.6);
        check(90.0, 0.13);
    }

    /// Точность темпа важнее всего: полпроцента ошибки за три минуты
    /// уводят сетку почти на секунду.
    #[test]
    fn tempo_is_precise_enough_for_long_tracks() {
        let sr = 44100;
        let samples = click_track(128.0, 20.0, 0.1, sr);
        let map = analyze(&samples, sr);

        let drift = (map.bpm - 128.0).abs() / 128.0 * 180.0;
        assert!(drift < 0.15, "за три минуты уедет на {drift:.2} с");
    }

    #[test]
    fn beatmap_survives_a_round_trip() {
        let map = BeatMap {
            bpm: 127.5,
            offset: 0.321,
            beats: vec![],
            confidence: 0.8,
            duration: 60.0,
        };
        let back = BeatMap::from_text(&map.to_text()).expect("карта должна читаться");

        assert!((back.bpm - map.bpm).abs() < 1e-3);
        assert!((back.offset - map.offset).abs() < 1e-3);
        // Доли восстанавливаются из темпа и сдвига, а не хранятся.
        assert_eq!(back.beats.len(), 127);
    }

    #[test]
    fn rejects_a_broken_beatmap() {
        assert!(BeatMap::from_text("# пусто").is_err());
        assert!(BeatMap::from_text("bpm 5000").is_err());
    }
}
