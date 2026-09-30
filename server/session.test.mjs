import { test } from 'node:test';
import assert from 'node:assert/strict';
import { WebSocket } from 'ws';
import { createHmac } from 'node:crypto';
import { createArena } from './index.mjs';

const token = '123:session-test';
function initData(id, name) {
  const p = new URLSearchParams({ auth_date: String(Math.floor(Date.now() / 1000)), user: JSON.stringify({ id, first_name: name }) });
  const check = [...p].sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => `${k}=${v}`).join('\n');
  p.set('hash', createHmac('sha256', createHmac('sha256', 'WebAppData').update(token).digest()).update(check).digest('hex'));
  return p.toString();
}

test('a player moves between pages; room links are meeting points', { timeout: 20000 }, async () => {
  const app = await createArena({ dev: false, botToken: token, origin: 'https://game.example', miniApp: 'https://t.me/test_bot', dataDir: null });
  app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
  const url = `ws://127.0.0.1:${app.server.address().port}/ws`;
  // One page of the game: a socket signed in as a Telegram user.
  async function page(id, name, extra = {}) {
    const ws = new WebSocket(url, { headers: { origin: 'https://game.example' } });
    const got = []; let closed = null;
    ws.on('message', d => got.push(JSON.parse(d)));
    ws.on('close', code => { closed = code; });
    await new Promise((r, j) => { ws.once('open', r); ws.once('error', j); });
    ws.send(JSON.stringify({ type: 'auth', v: 3, initData: initData(id, name), ...extra }));
    const wait = async (type, ok = () => true) => {
      for (let i = 0; i < 400; i++) { const k = got.findIndex(m => m.type === type && ok(m)); if (k >= 0) return got.splice(k, 1)[0]; if (got.some(m => m.type === 'error')) throw Error(got.find(m => m.type === 'error').message); await new Promise(r => setTimeout(r, 10)); }
      throw Error('no ' + type);
    };
    await wait('welcome');
    return { ws, send: m => ws.send(JSON.stringify(m)), wait, closed: () => closed, got };
  }
  try {
    // Anna waits in her room; opening the same link again returns her there.
    const anna1 = await page(1, 'Анна');
    anna1.send({ type: 'create' });
    const { code } = await anna1.wait('room');
    const anna2 = await page(1, 'Анна', { leave: true });
    await new Promise(r => setTimeout(r, 50));
    assert.equal(anna1.closed(), 4002, 'the old page is told the game moved');
    anna2.send({ type: 'join', code });
    // Her room was left when the link page took over, so the link reopens it.
    const again = await anna2.wait('room');
    assert.equal(again.code, code);
    anna2.send({ type: 'join', code });
    assert.equal((await anna2.wait('room')).code, code, 'own room: back to waiting, no error');
    // Boris follows the link: the fight starts.
    const boris = await page(2, 'Борис');
    boris.send({ type: 'join', code });
    await boris.wait('match'); await anna2.wait('match');
    // Mid-fight, Anna opens the game without a link: she is back in the fight.
    const anna3 = await page(1, 'Анна');
    const back = await anna3.wait('match');
    assert.equal(back.code, code);
    // Then she follows a link to another room: the fight is conceded.
    const anna4 = await page(1, 'Анна', { leave: true });
    const end = await boris.wait('state', m => m.state.phase === 3);
    assert.equal(end.state.winner, 1);
    // A link to a room nobody holds any more: the follower waits there.
    const fresh = 'ABCDEFGHIJKL';
    anna4.send({ type: 'join', code: fresh });
    const reopened = await anna4.wait('room');
    assert.equal(reopened.code, fresh); assert.equal(reopened.reopened, true);
    boris.send({ type: 'join', code: fresh });
    await anna4.wait('match');
    // Malformed codes are refused.
    const ira = await page(3, 'Ира');
    ira.send({ type: 'join', code: '../x' });
    await assert.rejects(ira.wait('room'), /Неверная ссылка/);
    for (const p of [anna2, anna3, anna4, boris, ira]) p.ws.close();
  } finally { await app.close(); }
});
