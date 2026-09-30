"""Sends the game bot's token to the server without showing it anywhere.

    1. Put the token from BotFather into secrets/game-bot-token.txt
       (one line, like 123456789:AA...). The folder stays on this computer.
    2. python tools/set-game-bot-token.py
    3. Deploy (python scripts/publish.py after deploy.ps1): the server picks it up.

The token is checked with Telegram (only the bot's @name is printed) and
written to /home/avpetrov89/pulse-arena/game-bot.env (mode 600) through SSH
standard input, so it never appears in a command line or a log.
"""
import json
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

root = Path(__file__).resolve().parent.parent
source = Path(sys.argv[1]) if len(sys.argv) > 1 else root / 'secrets' / 'game-bot-token.txt'
token = source.read_text(encoding='utf-8').strip()
if not re.fullmatch(r'\d{5,}:[A-Za-z0-9_-]{30,}', token):
    raise SystemExit(f'{source}: this does not look like a bot token')
try:
    with urllib.request.urlopen(f'https://api.telegram.org/bot{token}/getMe', timeout=20) as r:
        me = json.load(r)['result']
except Exception:
    raise SystemExit('Telegram did not accept the token (check it in BotFather).')
print('Bot: @' + me['username'] + ('' if me.get('has_main_web_app') else '  (main Mini App not configured yet)'))

sys.path.insert(0, str(root / 'scripts'))
from sshconf import ssh_command  # noqa: E402

target = '/home/avpetrov89/pulse-arena/game-bot.env'
subprocess.run(ssh_command() + [f'umask 077 && cat > {target} && chmod 600 {target}'],
               input=f'GAME_BOT_TOKEN={token}\n'.encode(), check=True)
print('Saved on the server. Deploy to apply it.')
