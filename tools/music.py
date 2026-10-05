"""Prepares the game's music: Kevin MacLeod tracks (incompetech.com,
CC BY 4.0, credited in CREDITS.md and in the game's help) at an even
loudness and a phone-friendly bitrate.
    python tools/music.py        (downloads missing sources into assets-src/music)
Writes assets/sound/<id>.mp3. Needs: pip install lameenc miniaudio."""
import array
import math
import os
import urllib.parse
import urllib.request

import lameenc
import miniaudio

ROOT = os.path.join(os.path.dirname(__file__), "..")
SRC = os.path.join(ROOT, "assets-src", "music")
OUT = os.path.join(ROOT, "assets", "sound")
TRACKS = {
    # id: incompetech title — lobby first, then the fight tracks.
    "lobby": "Club Diver",
    "fight1": "Volatile Reaction",
    "fight2": "Exit the Premises",
    "fight3": "Raving Energy",
}
TARGET_RMS = 0.16  # the same perceived level for every track
KBPS = 112

os.makedirs(SRC, exist_ok=True)
os.makedirs(OUT, exist_ok=True)
for key, title in TRACKS.items():
    src = os.path.join(SRC, title + ".mp3")
    if not os.path.exists(src):
        url = "https://incompetech.com/music/royalty-free/mp3-royaltyfree/" + urllib.parse.quote(title) + ".mp3"
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req) as r, open(src, "wb") as f:
            f.write(r.read())
    sound = miniaudio.decode_file(src, output_format=miniaudio.SampleFormat.SIGNED16, nchannels=2, sample_rate=44100)
    samples = sound.samples
    rms = math.sqrt(sum(s * s for s in samples[::97]) / len(samples[::97])) / 32768
    gain = TARGET_RMS / max(rms, 1e-4)
    peak = max(abs(s) for s in samples[::7]) / 32768
    gain = min(gain, 0.97 / max(peak, 1e-4))  # never clip
    scaled = array.array("h", (max(-32768, min(32767, int(s * gain))) for s in samples))
    enc = lameenc.Encoder()
    enc.set_bit_rate(KBPS)
    enc.set_in_sample_rate(44100)
    enc.set_channels(2)
    enc.set_quality(2)
    data = enc.encode(scaled.tobytes()) + enc.flush()
    out = os.path.join(OUT, key + ".mp3")
    with open(out, "wb") as f:
        f.write(data)
    print(f"{key:7} {title:20} {sound.duration:6.1f}s rms {rms:.3f} gain {gain:.2f} -> {len(data) // 1024} KB")
