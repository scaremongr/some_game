//! Воспроизведение музыки и часы трека.
//!
//! Проигрывание отдано quad-snd: он умеет и Windows, и WebAudio, то есть обе
//! наши цели. Позицию воспроизведения он не отдаёт, поэтому время трека
//! считается от момента запуска плюс поправка на задержку вывода.
//!
//! Это осознанный размен. Настоящие аудио-часы (счётчик отданных
//! устройству сэмплов) точнее и не плывут, но требуют своего микшера на
//! каждой платформе. Разница на треке в три минуты — единицы миллисекунд,
//! а вот **задержка вывода** у устройств гуляет на 50-200 мс, и её всё
//! равно приходится калибровать руками. Поэтому сначала калибровка,
//! свои часы — когда упрёмся.

use quad_snd::{AudioContext, PlaySoundParams, Sound};

pub struct Music {
    ctx: AudioContext,
    sound: Option<Sound>,
    /// Время игрового цикла в момент старта трека.
    started_at: f64,
    playing: bool,
    /// Поправка на задержку вывода, секунды. Положительная означает, что
    /// звук отстаёт от картинки и время трека надо считать меньшим.
    pub latency: f32,
    pub volume: f32,
}

impl Music {
    pub fn new() -> Music {
        Music {
            ctx: AudioContext::new(),
            sound: None,
            started_at: 0.0,
            playing: false,
            latency: 0.0,
            volume: 0.8,
        }
    }

    /// Готовит трек из байтов файла.
    ///
    /// Формат проверяется до того, как байты уйдут в звуковую библиотеку:
    /// та на неподдержанном файле не возвращает ошибку, а роняет процесс.
    /// Ошибку возвращаем наружу — сцена должна показать причину, а не молча
    /// остаться без музыки.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), String> {
        let codec = sniff(bytes);
        if !codec.playable_here() {
            return Err(codec.complaint().to_string());
        }

        if let Some(old) = self.sound.take() {
            old.delete(&self.ctx);
        }
        self.sound = Some(Sound::load(&self.ctx, bytes));
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        self.sound.is_some()
    }

    pub fn play(&mut self, now: f64) {
        let Some(sound) = &self.sound else {
            return;
        };
        sound.play(
            &self.ctx,
            PlaySoundParams {
                looped: true,
                volume: self.volume,
            },
        );
        self.started_at = now;
        self.playing = true;
    }

    /// Разрешает звук — вызывать строго внутри жеста пользователя.
    ///
    /// Браузер держит аудиоконтекст спящим, пока в нём что-нибудь не
    /// проиграет во время касания. Трек к этому моменту ещё не нужен, поэтому
    /// запускаем его беззвучно и сразу глушим: контекст просыпается, а слышно
    /// ничего не будет. Без этого режим, который стартует по жесту рукой в
    /// трёх метрах от телефона, остался бы немым.
    pub fn unlock(&mut self) {
        let Some(sound) = &self.sound else {
            return;
        };
        sound.play(
            &self.ctx,
            PlaySoundParams {
                looped: false,
                volume: 0.0,
            },
        );
        sound.stop(&self.ctx);
    }

    pub fn stop(&mut self) {
        if let Some(sound) = &self.sound {
            sound.stop(&self.ctx);
        }
        self.playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Позиция в треке, секунды. До запуска — ноль.
    pub fn position(&self, now: f64) -> f32 {
        if !self.playing {
            return 0.0;
        }
        ((now - self.started_at) as f32 - self.latency).max(0.0)
    }
}

/// Формат звукового файла, определённый по первым байтам.
///
/// Расширению верить нельзя: `.ogg` — это контейнер, внутри которого лежит
/// либо Vorbis, либо Opus, и декодеры у них разные. Именно на этом и
/// спотыкается всё: файл называется `.ogg`, а проигрывается не везде.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Codec {
    Wav,
    OggVorbis,
    OggOpus,
    Mp3,
    Unknown,
}

impl Codec {
    /// Возьмётся ли за такой файл декодер этой сборки.
    ///
    /// На PC играет audrey — только WAV и Ogg Vorbis. В браузере разбор
    /// делает сам браузер, и там проходит почти всё; какие форматы у него
    /// есть, заранее не выяснить, поэтому пропускаем всё узнаваемое.
    pub fn playable_here(self) -> bool {
        if cfg!(target_arch = "wasm32") {
            self != Codec::Unknown
        } else {
            matches!(self, Codec::Wav | Codec::OggVorbis)
        }
    }

    /// Что показать, когда формат не подходит. Только ASCII: шрифт движка
    /// другого не знает.
    pub fn complaint(self) -> &'static str {
        match self {
            Codec::OggOpus => "OPUS INSIDE THE OGG - PC PLAYS ONLY OGG VORBIS",
            Codec::Mp3 => "MP3 PLAYS ONLY IN THE BROWSER - USE OGG VORBIS",
            Codec::Unknown => "UNKNOWN AUDIO FORMAT",
            _ => "",
        }
    }
}

/// Опознаёт формат по сигнатуре.
pub fn sniff(bytes: &[u8]) -> Codec {
    if bytes.len() < 16 {
        return Codec::Unknown;
    }
    if &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Codec::Wav;
    }
    if &bytes[0..4] == b"OggS" {
        // Кодек назван в первом пакете первой страницы. Дальше начала искать
        // незачем: там уже данные, и совпадение было бы случайным.
        let head = &bytes[..bytes.len().min(96)];
        if head.windows(8).any(|w| w == b"OpusHead") {
            return Codec::OggOpus;
        }
        if head.windows(6).any(|w| w == b"vorbis") {
            return Codec::OggVorbis;
        }
        return Codec::Unknown;
    }
    // MP3: либо тег ID3, либо сразу кадр со словом синхронизации.
    if &bytes[0..3] == b"ID3" || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0) {
        return Codec::Mp3;
    }
    Codec::Unknown
}

/// Минимальный разбор WAV: заголовок RIFF и один блок данных.
///
/// Свой, а не библиотечный, потому что нужен ровно один формат и ровно для
/// инструмента анализа — в игру этот код не попадает.
pub struct Pcm {
    /// Сведённый в моно сигнал: для поиска ритма разница каналов только мешает.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

pub fn decode_wav(bytes: &[u8]) -> Result<Pcm, String> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }

    let u16at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);

    let mut format = None;
    let mut data: Option<(usize, usize)> = None;

    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32at(at + 4) as usize;
        let body = at + 8;
        if body + size > bytes.len() {
            break;
        }

        match id {
            b"fmt " if size >= 16 => {
                format = Some((
                    u16at(body),          // тип: 1 = PCM, 3 = float
                    u16at(body + 2),      // каналов
                    u32at(body + 4),      // частота
                    u16at(body + 14),     // бит на сэмпл
                ));
            }
            b"data" => data = Some((body, size)),
            _ => {}
        }
        // Блоки выровнены по чётной границе.
        at = body + size + (size & 1);
    }

    let (kind, channels, sample_rate, bits) = format.ok_or("WAV has no fmt chunk")?;
    let (start, size) = data.ok_or("WAV has no data chunk")?;
    if channels == 0 {
        return Err("WAV has zero channels".into());
    }

    let bytes_per_sample = (bits / 8) as usize;
    if bytes_per_sample == 0 {
        return Err("WAV has zero bit depth".into());
    }
    let frame = bytes_per_sample * channels as usize;
    let frames = size / frame.max(1);

    let mut samples = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut sum = 0.0f32;
        for c in 0..channels as usize {
            let at = start + f * frame + c * bytes_per_sample;
            let value = match (kind, bits) {
                (1, 16) => i16::from_le_bytes([bytes[at], bytes[at + 1]]) as f32 / 32768.0,
                (1, 8) => (bytes[at] as f32 - 128.0) / 128.0,
                (1, 24) => {
                    let v = i32::from_le_bytes([0, bytes[at], bytes[at + 1], bytes[at + 2]]) >> 8;
                    v as f32 / 8_388_608.0
                }
                (1, 32) => i32::from_le_bytes([
                    bytes[at],
                    bytes[at + 1],
                    bytes[at + 2],
                    bytes[at + 3],
                ]) as f32
                    / 2_147_483_648.0,
                (3, 32) => f32::from_le_bytes([
                    bytes[at],
                    bytes[at + 1],
                    bytes[at + 2],
                    bytes[at + 3],
                ]),
                _ => return Err(format!("unsupported WAV format {kind}, {bits} bit")),
            };
            sum += value;
        }
        samples.push(sum / channels as f32);
    }

    Ok(Pcm { samples, sample_rate })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Первые байты настоящих файлов. Расширение у всех трёх было бы `.ogg`
    /// или `.wav` — различает их только содержимое.
    fn ogg_opus() -> Vec<u8> {
        let mut b = b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x02\xbb\x07\xae\x00\x00\x00\x00\x5a\x26\x05\x2c\x01\x13".to_vec();
        b.extend_from_slice(b"OpusHead\x01\x02\x38\x01\x80\xbb\x00\x00\x00\x00\x00");
        b
    }

    fn ogg_vorbis() -> Vec<u8> {
        let mut b = b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x11\x22\x33\x44\x00\x00\x00\x00\x55\x66\x77\x88\x01\x1e".to_vec();
        b.extend_from_slice(b"\x01vorbis\x00\x00\x00\x00\x02\x44\xac\x00\x00");
        b
    }

    fn wav() -> Vec<u8> {
        let mut b = b"RIFF".to_vec();
        b.extend_from_slice(&[0x24, 0x08, 0x00, 0x00]);
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&[0u8; 16]);
        b
    }

    /// Главное, ради чего это заведено: Opus и Vorbis лежат в одинаковом
    /// контейнере с одинаковым расширением, а декодеры у них разные.
    #[test]
    fn ogg_container_is_split_by_codec() {
        assert_eq!(sniff(&ogg_opus()), Codec::OggOpus);
        assert_eq!(sniff(&ogg_vorbis()), Codec::OggVorbis);
    }

    #[test]
    fn wav_and_mp3_are_recognised() {
        assert_eq!(sniff(&wav()), Codec::Wav);
        // Заголовок ID3v2 — десять байт, дальше уже кадры.
        assert_eq!(
            sniff(b"ID3\x04\x00\x00\x00\x00\x0a\x2eTIT2\x00\x00\x00"),
            Codec::Mp3
        );
        assert_eq!(sniff(&[0xFF, 0xFB, 0x90, 0x64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), Codec::Mp3);
    }

    #[test]
    fn garbage_is_not_mistaken_for_audio() {
        assert_eq!(sniff(b"glTF\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"), Codec::Unknown);
        assert_eq!(sniff(b"short"), Codec::Unknown);
    }

    /// На PC играет только то, что понимает audrey. Ошибиться тут дорого:
    /// на неподдержанном файле звуковая библиотека роняет процесс.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn desktop_takes_only_wav_and_vorbis() {
        assert!(Codec::Wav.playable_here());
        assert!(Codec::OggVorbis.playable_here());
        assert!(!Codec::OggOpus.playable_here());
        assert!(!Codec::Mp3.playable_here());
        assert!(!Codec::Unknown.playable_here());
    }

    /// У отказа обязана быть внятная причина: иначе игрок видит тишину и
    /// не знает, что делать с файлом.
    #[test]
    fn every_refusal_explains_itself() {
        for codec in [Codec::OggOpus, Codec::Mp3, Codec::Unknown] {
            assert!(!codec.complaint().is_empty(), "{codec:?} молчит об отказе");
            assert!(codec.complaint().is_ascii(), "{codec:?}: шрифт не покажет");
        }
    }
}
