"""Downloads the game's character models and animation packs from the live
server into assets/ (they are not in the public repository: Mixamo's terms
allow them inside a game, not as standalone files).

    python tools/fetch-assets.py            # only missing files
    python tools/fetch-assets.py --force    # refresh everything
    python tools/fetch-assets.py --from https://other.host/dance/

Fetched: assets/character.glb, assets/fight.pack and every model and pack
listed in assets/fighters/roster.json.
"""
import json
import pathlib
import sys
import urllib.request

root = pathlib.Path(__file__).resolve().parent.parent
base = sys.argv[sys.argv.index('--from') + 1] if '--from' in sys.argv else 'https://serbiamarket.duckdns.org/dance/'
force = '--force' in sys.argv


def fetch(relative):
    target = root / relative
    if target.exists() and target.stat().st_size > 0 and not force:
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(base + relative, headers={'User-Agent': 'pulse-fetch-assets/1.0'})
    with urllib.request.urlopen(request, timeout=120) as r:
        data = r.read()
    target.write_bytes(data)
    print(f'{relative}  {len(data) // 1024} KB')


roster = json.loads((root / 'assets/fighters/roster.json').read_text(encoding='utf-8'))
files = {'assets/character.glb', 'assets/fight.pack'}
for fighter in roster:
    files.update({fighter['model'], fighter['pack']})
for relative in sorted(files):
    fetch(relative)
print('assets ready')
