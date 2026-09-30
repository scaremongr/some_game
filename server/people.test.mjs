import { test } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { WebSocket } from 'ws';
import { createHmac } from 'node:crypto';
import { createArena } from './index.mjs';

const token = '123:people-test';
function initData(id, name, photo) {
  const p = new URLSearchParams({ auth_date: String(Math.floor(Date.now() / 1000)), user: JSON.stringify({ id, first_name: name, photo_url: photo }) });
  const check = [...p].sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => `${k}=${v}`).join('\n');
  const secret = createHmac('sha256', 'WebAppData').update(token).digest();
  p.set('hash', createHmac('sha256', secret).update(check).digest('hex'));
  return p.toString();
}

test('players carry their Telegram photos into the match; the game serves them', async () => {
  // Stand-in for Telegram's photo host: the arena fetches avatars from it.
  const jpeg = Buffer.from([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 0xff, 0xd9]);
  const cdn = http.createServer((req, res) => { res.writeHead(200, { 'Content-Type': 'image/jpeg' }); res.end(jpeg); });
  cdn.listen(0, '127.0.0.1'); await new Promise(r => cdn.once('listening', r));
  const realFetch = globalThis.fetch;
  globalThis.fetch = (url, opts) => realFetch(String(url).startsWith('https://t.me/') ? `http://127.0.0.1:${cdn.address().port}/p.jpg` : url, opts);
  const app = await createArena({ dev: false, botToken: token, origin: 'https://game.example', miniApp: 'https://t.me/test_bot', dataDir: null });
  app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
  const base = `http://127.0.0.1:${app.server.address().port}`;
  const join = async (id, name, photo) => {
    const ws = new WebSocket(base.replace('http', 'ws') + '/ws', { headers: { origin: 'https://game.example' } });
    const got = [];
    ws.on('message', d => got.push(JSON.parse(d)));
    await new Promise((r, j) => { ws.once('open', r); ws.once('error', j); });
    ws.send(JSON.stringify({ type: 'auth', v: 3, initData: initData(id, name, photo) }));
    const wait = async type => { for (let i = 0; i < 300; i++) { const m = got.find(m => m.type === type); if (m) return m; await new Promise(r => setTimeout(r, 10)); } throw Error('no ' + type); };
    return { ws, wait, send: m => ws.send(JSON.stringify(m)) };
  };
  try {
    const a = await join(501, 'Анна', 'https://t.me/i/userpic/320/anna.jpg');
    const b = await join(502, 'Борис', undefined);
    const welcome = await a.wait('welcome');
    assert.equal(welcome.user.avatar, 'avatar/501.jpg');
    assert.equal((await b.wait('welcome')).user.avatar, null);
    a.send({ type: 'queue', fighter: 'ninja' }); await a.wait('queued'); b.send({ type: 'queue' });
    const match = await a.wait('match');
    assert.deepEqual(match.players.map(p => [p.name, p.avatar]), [['Анна', 'avatar/501.jpg'], ['Борис', null]]);
    const photo = await realFetch(base + '/avatar/501.jpg');
    assert.equal(photo.headers.get('content-type'), 'image/jpeg');
    assert.deepEqual(Buffer.from(await photo.arrayBuffer()), jpeg);
    assert.equal((await realFetch(base + '/avatar/502.jpg')).status, 404);
    assert.equal((await realFetch(base + '/avatar/999.jpg')).status, 404);
    a.ws.close(); b.ws.close();
  } finally { globalThis.fetch = realFetch; await app.close(); cdn.close(); }
});
