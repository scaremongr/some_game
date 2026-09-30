"""Read-only deployment discovery. Never print credentials or authenticated URLs."""
from pathlib import Path
import json
import urllib.request

config = {}
for line in Path('/home/avpetrov89/barakholka/.env').read_text().splitlines():
    if '=' in line and not line.startswith('#'):
        key, value = line.split('=', 1)
        config[key] = value.strip().strip('"').strip("'")
print(json.dumps({key: config.get(key) for key in ['SITE_DOMAIN', 'WEBAPP_URL', 'PUBLIC_BASE_URL']}))
for method in ['getMe', 'getChatMenuButton']:
    try:
        with urllib.request.urlopen('https://api.telegram.org/bot' + config['BOT_TOKEN'] + '/' + method, timeout=15) as response:
            result = json.load(response).get('result', {})
        fields = ['username', 'has_main_web_app'] if method == 'getMe' else ['type', 'web_app']
        print(method, json.dumps({key: result.get(key) for key in fields}))
    except Exception:
        print(method, 'unavailable')
