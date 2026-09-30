"""Fetches Mixamo clips exported by tools/mixamo-download.js.

The browser script saves lists of temporary download links
(pulse-mixamo-urls-NN.json) into Downloads; this watcher downloads every link
as soon as its list appears, before the links expire, and stops after the
final pulse-mixamo-manifest.json has been processed. Clips land in
assets-src/fight/fbx, characters (entries with "dir": "fighters") in
assets-src/fighters/fbx; the character catalogue is kept with the lists.

    python tools/mixamo-fetch.py [--downloads DIR] [--timeout MIN]
"""
import argparse
import json
import pathlib
import shutil
import sys
import time
import urllib.request

root = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--downloads", default=str(pathlib.Path.home() / "Downloads"))
parser.add_argument("--timeout", type=float, default=90.0, help="minutes")
args = parser.parse_args()
downloads = pathlib.Path(args.downloads)
source = root / "assets-src"
target = source / "fight" / "fbx"
lists = target / "lists"
lists.mkdir(parents=True, exist_ok=True)
started = time.time()
fetched, failed = 0, 0


def fetch(entry):
    global fetched, failed
    folder = source / entry.get("dir", "fight") / "fbx"
    folder.mkdir(parents=True, exist_ok=True)
    out = folder / entry["file"]
    if out.exists() and out.stat().st_size > 0:
        return
    try:
        with urllib.request.urlopen(entry["url"], timeout=60) as response, open(out, "wb") as sink:
            shutil.copyfileobj(response, sink)
        fetched += 1
        print(f"OK   {entry['file']} ({out.stat().st_size // 1024} KB)", flush=True)
    except Exception as error:  # an expired link is reported, the rest continue
        failed += 1
        print(f"FAIL {entry['file']}: {error}", flush=True)


done = False
while not done and time.time() - started < args.timeout * 60:
    for path in sorted(downloads.glob("pulse-mixamo-*.json")):
        if path.stat().st_mtime < started - 5:
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            continue  # still being written
        entries = data["clips"] if isinstance(data, dict) else data
        for entry in entries:
            if isinstance(entry, dict) and "url" in entry:
                fetch(entry)
        if path.name.startswith("pulse-mixamo-manifest"):
            done = True
        shutil.move(str(path), str(lists / path.name))
    time.sleep(3)
print(f"fetched {fetched}, failed {failed}, in {source}", flush=True)
sys.exit(0 if failed == 0 else 1)
