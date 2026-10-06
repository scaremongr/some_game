"""Prepares the recorded sound effects: Kenney "Impact Sounds" (kenney.nl,
CC0) — punches, body falls and blocks — trimmed to the attack, levelled and
written as small mono WAVs (every browser decodes them, iOS included).
    python tools/sfx.py           (downloads the pack into assets-src/sfx once)
    python tools/sfx.py --list    (duration, peak, loudness and brightness of every source)
Writes assets/sound/sfx/<id>.wav. Needs: pip install miniaudio."""
import array
import io
import math
import os
import sys
import urllib.request
import wave
import zipfile

import miniaudio

ROOT = os.path.join(os.path.dirname(__file__), "..")
SRC = os.path.join(ROOT, "assets-src", "sfx")
OUT = os.path.join(ROOT, "assets", "sound", "sfx")
PACK = "https://kenney.nl/media/pages/assets/impact-sounds/87b4ddecda-1677589768/kenney_impact-sounds.zip"
RATE = 22050
# Output id: (source prefix, how many, target RMS of the loud part, longest seconds).
GROUPS = {
    "punch": ("impactPunch_medium", 5, 0.20, 0.35),
    "heavy": ("impactPunch_heavy", 5, 0.24, 0.5),
    "block": ("impactGeneric_light", 3, 0.15, 0.3),
    "fall": ("impactSoft_heavy", 3, 0.22, 0.6),
    "wood": ("impactWood_heavy", 3, 0.20, 0.7),
}


def source(name):
    """Mono float samples of Audio/<name>.ogg from the pack."""
    path = os.path.join(SRC, "kenney_impact-sounds.zip")
    if not os.path.exists(path):
        os.makedirs(SRC, exist_ok=True)
        urllib.request.urlretrieve(PACK, path)
    with zipfile.ZipFile(path) as z:
        data = z.read(f"Audio/{name}.ogg")
    sound = miniaudio.decode(data, output_format=miniaudio.SampleFormat.FLOAT32, nchannels=1, sample_rate=RATE)
    return list(sound.samples)


def describe(x):
    peak = max(abs(v) for v in x) or 1e-9
    rms = math.sqrt(sum(v * v for v in x) / len(x))
    # Brightness: zero crossings per second, a cheap stand-in for the spectral centroid.
    crossings = sum(1 for a, b in zip(x, x[1:]) if (a < 0) != (b < 0)) * RATE / len(x) / 2
    return f"{len(x) / RATE:5.2f} s  peak {peak:.2f}  rms {rms:.3f}  ~{crossings:5.0f} Hz"


def shape(x, rms_target, longest):
    # Start at the attack (the first sample above 2% of the peak), keep
    # `longest` seconds and fade the tail out.
    peak = max(abs(v) for v in x) or 1e-9
    start = next(i for i, v in enumerate(x) if abs(v) > 0.02 * peak)
    x = x[max(0, start - 22): start + int(longest * RATE)]
    fade = min(len(x), int(0.06 * RATE))
    for i in range(fade):
        x[len(x) - fade + i] *= 1 - i / fade
    # Level by the loudest 60 ms, then keep the peak under -1 dBFS.
    window = int(0.06 * RATE)
    loud = max(math.sqrt(sum(v * v for v in x[i:i + window]) / window) for i in range(0, max(1, len(x) - window), 64))
    gain = min(rms_target / max(loud, 1e-6), 0.89 / max(abs(v) for v in x))
    return [v * gain for v in x]


def write(path, x):
    pcm = array.array("h", (max(-32767, min(32767, int(v * 32767))) for v in x))
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())


if "--list" in sys.argv:
    path = os.path.join(SRC, "kenney_impact-sounds.zip")
    source("impactPunch_medium_000")
    with zipfile.ZipFile(path) as z:
        for name in sorted(n[6:-4] for n in z.namelist() if n.startswith("Audio/") and n.endswith(".ogg")):
            print(f"{name:26} {describe(source(name))}")
    sys.exit()

os.makedirs(OUT, exist_ok=True)
for key, (prefix, count, rms, longest) in GROUPS.items():
    for i in range(count):
        x = shape(source(f"{prefix}_{i:03d}"), rms, longest)
        write(os.path.join(OUT, f"{key}{i}.wav"), x)
        print(f"{key}{i}.wav  {describe(x)}")
