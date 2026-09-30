import { test } from 'node:test';
import assert from 'node:assert/strict';
import { WebSocket } from 'ws';
import { createArena } from './index.mjs';

async function client(url, name) {
  const ws = new WebSocket(url), got = [];
  ws.on('message', d => got.push(JSON.parse(d)));
  await new Promise((r, j) => { ws.once('open', r); ws.once('error', j); });
  const send = m => ws.send(JSON.stringify(m));
  const wait = async (type, ok = () => true) => {
    for (let i = 0; i < 400; i++) { const k = got.findIndex(m => m.type === type && ok(m)); if (k >= 0) return got.splice(k, 1)[0]; await new Promise(r => setTimeout(r, 10)); }
    throw Error('no ' + type);
  };
  send({ type: 'auth', v: 3, name });
  const welcome = await wait('welcome');
  return { ws, send, wait, id: welcome.user.id, welcome };
}

test('a finished match moves ratings; the winner shares a card; the loser calls a revenge', async () => {
  const calls = [];
  const telegramApi = async (method, params) => { calls.push({ method, params }); return method === 'savePreparedInlineMessage' ? { id: 'prep-1', expiration_date: 0 } : true; };
  const app = await createArena({ dev: true, gameBotToken: '1:t', gameUrl: 'https://g.example/dance/', miniApp: 'https://t.me/somee_game_bot', telegramApi });
  app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
  const base = `http://127.0.0.1:${app.server.address().port}`;
  try {
    const anna = await client(base.replace('http', 'ws') + '/ws', 'Анна');
    const boris = await client(base.replace('http', 'ws') + '/ws', 'Борис');
    assert.equal(anna.welcome.rating.rating, 1000);
    assert.equal(anna.welcome.user.league.id, 'bronze');
    anna.send({ type: 'queue' }); await anna.wait('queued'); boris.send({ type: 'queue' });
    const match = await anna.wait('match');
    assert.deepEqual(match.players.map(p => p.rating), [1000, 1000]);
    await anna.wait('state', m => m.state.phase === 1);
    // Boris concedes: Anna wins.
    boris.send({ type: 'leave' });
    const won = await anna.wait('result'), lost = await boris.wait('result');
    assert.equal(won.won, true); assert.equal(won.you.delta, 20); assert.equal(won.you.rank, 1);
    assert.equal(lost.won, false); assert.equal(lost.you.delta, -20); assert.equal(lost.card, null);
    assert.equal(typeof won.card, 'string');
    // The card: a JPEG with the one-time ticket becomes a prepared message.
    const jpeg = Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0xff, 0xd9]);
    const upload = await fetch(`${base}/card?t=${won.card}`, { method: 'POST', body: jpeg }).then(r => r.json());
    assert.equal(upload.prepared, 'prep-1');
    const prepared = calls.find(c => c.method === 'savePreparedInlineMessage').params;
    assert.equal(prepared.result.photo_url, 'https://g.example/dance/' + upload.url);
    assert.match(prepared.result.caption, /Анна<\/b> \d:\d <b>Борис/);
    assert.deepEqual(Buffer.from(await (await fetch(`${base}/${upload.url}`)).arrayBuffer()), jpeg);
    assert.equal((await fetch(`${base}/card?t=${won.card}`, { method: 'POST', body: jpeg })).status, 403, 'tickets are single-use');
    // Revenge: Boris opens a room for Anna, who is called in the game.
    await anna.wait('state', m => m.state.phase === 3).catch(() => {});
    anna.send({ type: 'leave' }); await anna.wait('lobby');
    boris.send({ type: 'create', revenge: anna.id });
    const room = await boris.wait('room');
    assert.equal(room.revenge.name, 'Анна');
    const call = await anna.wait('challenge');
    assert.equal(call.code, room.code); assert.equal(call.from.name, 'Борис');
    // Leaderboard.
    anna.send({ type: 'top' });
    const top = await anna.wait('top');
    assert.deepEqual(top.players.map(p => p.name), ['Анна', 'Борис']);
    assert.equal(top.you.rank, 1);
    anna.ws.close(); boris.ws.close();
  } finally { await app.close(); }
});
