"""Versioned deployment to the existing game server, with Caddy rollback.
Receives a verified archive path and SHA256. Never logs BOT_TOKEN.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import urllib.request
import zipfile

ROOT = Path('/home/avpetrov89/pulse-arena')
MARKET = Path('/home/avpetrov89/barakholka')
archive = Path(sys.argv[1]).resolve()
digest = sys.argv[2]
assert re.fullmatch('[a-f0-9]{64}', digest)
assert archive.parent == ROOT / 'incoming'
assert hashlib.sha256(archive.read_bytes()).hexdigest() == digest
release = ROOT / 'releases' / digest[:16]
release.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(archive) as bundle:
    for entry in bundle.infolist():
        assert (release / entry.filename).resolve().is_relative_to(release)
    bundle.extractall(release)

def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)

def read(url):
    with urllib.request.urlopen(url, timeout=20) as r:
        return r.read()

env = {}
for line in (MARKET / '.env').read_text().splitlines():
    if '=' in line and not line.startswith('#'):
        key, value = line.split('=', 1)
        env[key] = value.strip().strip('"').strip("'")
origin = 'https://' + env['SITE_DOMAIN']
assert origin == 'https://serbiamarket.duckdns.org'
try:
    me = json.loads(read('https://api.telegram.org/bot' + env['BOT_TOKEN'] + '/getMe'))['result']
except Exception:
    raise SystemExit('Cannot verify existing Telegram bot; deployment stopped before routing changes.')
assert me.get('has_main_web_app'), 'Bot requires a configured Main Mini App'
username = me['username']
assert re.fullmatch('[A-Za-z0-9_]+', username)
# The game's own bot (tools/set-game-bot-token.py puts its token here). Its
# Mini App link becomes the invite link once its Main Mini App is set up in
# BotFather; until then invites keep using the marketplace bot.
game_token = ''
game_file = ROOT / 'game-bot.env'
if game_file.exists():
    for line in game_file.read_text().splitlines():
        if line.startswith('GAME_BOT_TOKEN='):
            game_token = line.split('=', 1)[1].strip()
if game_token:
    try:
        game_me = json.loads(read('https://api.telegram.org/bot' + game_token + '/getMe'))['result']
    except Exception:
        raise SystemExit('Cannot verify the game bot token; deployment stopped.')
    assert re.fullmatch('[A-Za-z0-9_]+', game_me['username'])
    if game_me.get('has_main_web_app'):
        username = game_me['username']
    print('Game bot @' + game_me['username'] + (' (main Mini App on)' if game_me.get('has_main_web_app') else ' (main Mini App not set up yet)'))
secret = ROOT / '.env'
secret.write_text('BOT_TOKEN=' + env['BOT_TOKEN'] + '\nPUBLIC_ORIGIN=' + origin
    + '\nTELEGRAM_APP_URL=https://t.me/' + username
    + '\nGAME_URL=' + origin + '/dance/'
    + '\nDATA_DIR=/app/data'
    + ('\nGAME_BOT_TOKEN=' + game_token if game_token else '')
    + '\nHOST=0.0.0.0\nPORT=8080\n')
secret.chmod(0o600)
before_home = read(origin + '/')

(release / 'Dockerfile.runtime').write_text('''FROM node:24-alpine
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts && npm cache clean --force
COPY server server
COPY web/combat.js web/combat.js
COPY dist dist
ENV NODE_ENV=production
USER node
HEALTHCHECK --interval=20s --timeout=3s CMD node -e "fetch('http://127.0.0.1:8080/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node", "server/index.mjs"]
''')
image = 'pulse-arena:' + digest[:16]
run('docker', 'build', '-f', str(release / 'Dockerfile.runtime'), '-t', image, str(release))

caddy = MARKET / 'frontend' / 'Caddyfile'
original = caddy.read_text()
block = '''
    # BEGIN PULSE ARENA
    @pulseLaunch {
        path / /index.html
        vars_regexp {query.tgWebAppStartParam} ^fight_
    }
    redir @pulseLaunch /dance/index.html?{query} 302
    redir /dance /dance/ 308
    handle_path /dance/* {
        reverse_proxy pulse-arena:8080
    }
    # END PULSE ARENA
'''
candidate = re.sub(r'\n[ \t]*# BEGIN PULSE ARENA.*?# END PULSE ARENA\n', '\n', original, flags=re.S)
assert candidate.count('encode zstd gzip') == 1
candidate = candidate.replace('encode zstd gzip', 'encode zstd gzip\n' + block, 1)
candidate_path = release / 'Caddyfile.candidate'
candidate_path.write_text(candidate)
run('docker', 'cp', str(candidate_path), 'barakholka-web-1:/tmp/pulse-Caddyfile')
run('docker', 'exec', 'barakholka-web-1', 'caddy', 'validate', '--config', '/tmp/pulse-Caddyfile', '--adapter', 'caddyfile')
backup = ROOT / ('Caddyfile.before-' + str(int(time.time())))
backup.write_text(original)
previous = None
check = subprocess.run(['docker','inspect','pulse-arena','--format','{{index .Config.Labels "app"}}'],capture_output=True,text=True)
if check.returncode == 0:
    assert check.stdout.strip() == 'pulse-arena', 'Container name belongs to another application'
    previous = 'pulse-arena-previous-' + str(int(time.time()))
    run('docker','stop','pulse-arena')
    run('docker','rename','pulse-arena',previous)

# Ratings, chat tables and victory cards outlive releases: a host folder
# owned by the deploy user (uid 1000, the same as the container's node user).
data = ROOT / 'data'
data.mkdir(exist_ok=True)
new_started = False
try:
    run('docker','run','-d','--name','pulse-arena','--label','app=pulse-arena',
        '--restart','unless-stopped','--network','barakholka_default','--env-file',str(secret),
        '--read-only','--tmpfs','/tmp','--memory','512m','--cpus','1',
        '-v',str(data)+':/app/data',
        '--cap-drop','ALL','--security-opt','no-new-privileges',image)
    new_started = True
    healthy = False
    for _ in range(30):
        probe = subprocess.run(['docker','exec','pulse-arena','node','-e',
            "fetch('http://127.0.0.1:8080/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"],capture_output=True)
        if probe.returncode == 0:
            healthy = True
            break
        time.sleep(1)
    assert healthy, 'New game container failed health check'
    # Preserve the inode of the bind-mounted file; reload, never restart Caddy.
    caddy.write_text(candidate)
    run('docker','exec','barakholka-web-1','caddy','reload','--config','/etc/caddy/Caddyfile','--adapter','caddyfile')
    health = json.loads(read(origin + '/dance/health'))
    assert health['ok'] and health['protocol'] == 3
    assert read(origin + '/') == before_home, 'Main application response changed'
    assert b'PULSE' in read(origin + '/?tgWebAppStartParam=fight_home'), 'Mini App deep-link routing failed'
    assert hashlib.sha256(read(origin + '/dance/assets/character.glb')).hexdigest() == hashlib.sha256((release/'dist/assets/character.glb').read_bytes()).hexdigest()
    run('docker','exec','pulse-arena','node','server/deploy-probe.mjs')
except Exception:
    caddy.write_text(original)
    subprocess.run(['docker','exec','barakholka-web-1','caddy','reload','--config','/etc/caddy/Caddyfile','--adapter','caddyfile'])
    if new_started:
        subprocess.run(['docker','stop','pulse-arena'])
        subprocess.run(['docker','rename','pulse-arena','pulse-arena-failed-'+str(int(time.time()))])
    if previous:
        run('docker','rename',previous,'pulse-arena')
        run('docker','start','pulse-arena')
    raise

(ROOT/'current-release.txt').write_text(str(release)+'\n')
print('DEPLOYED', origin + '/dance/index.html')
print('TELEGRAM', 'https://t.me/' + username + '?startapp=fight_home')
print('ROLLBACK_CONFIG', str(backup))
print('RELEASE', digest[:16])
