// Client prediction (web/predict.js) against the real server with a
// simulated round trip: two headless clients run their 60 Hz loops, press
// and trade blows. A press shows on the presser's prediction on the next
// frame; the prediction clock settles so inputs arrive just in time; once
// both stop, both predictions agree with the server.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { WebSocket } from 'ws';
import { createArena } from './index.mjs';
import { Prediction } from '../web/predict.js';

const delay = ms => new Promise(r => setTimeout(r, ms));
const LIGHT = 8, RIGHT = 2, LEFT = 1, BLOCK = 4, KICK = 128;
const wasm = WebAssembly.compile(await readFile(new URL('../dist/arena_combat.wasm', import.meta.url)));

async function client(url, module) {
  const ws = new WebSocket(url), messages = [];
  ws.on('message', d => messages.push(JSON.parse(d)));
  await new Promise((r, j) => { ws.once('open', r); ws.once('error', j); });
  const c = { ws, messages, side: 0, held: 0, edges: 0, sent: 0, seq: 1, pred: null, last: null };
  c.send = m => ws.send(JSON.stringify(m));
  c.wait = async (type, ok = () => true, timeout = 6000) => {
    const start = Date.now();
    while (Date.now() - start < timeout) { const i = messages.findIndex(m => m.type === type && ok(m)); if (i >= 0) return messages.splice(i, 1)[0]; await delay(5); }
    throw Error('Timeout: ' + type);
  };
  c.press = (bits, hold = 50) => { c.held |= bits; c.edges |= bits; c.input(); setTimeout(() => { c.held &= ~bits; c.input(); }, hold); };
  c.input = () => { c.send({ type: 'input', seq: ++c.seq, bits: c.held | c.edges }); c.pred?.sending(c.seq); c.sent = performance.now(); };
  // The page's loop: snapshots in, prediction stepped, inputs out.
  c.frame = () => {
    const now = performance.now();
    for (const m of messages.splice(0).filter(m => m.type === 'state')) {
      c.last = m.state;
      if (!c.pred || !c.pred.snapshot(m, now)) c.pred = new Prediction(module, c.side, m.state, now);
    }
    if (c.pred) { c.pred.advance(now, c.held, c.edges); c.edges = 0; }
    if (now - c.sent >= 33) c.input();
  };
  c.send({ type: 'auth', v: 3, name: 'Tester' });
  await c.wait('welcome');
  return c;
}

for (const rtt of [60, 200]) {
  test(`prediction at ${rtt} ms round trip`, { timeout: 30000 }, async () => {
    const module = await wasm;
    const app = await createArena({ dev: true, latency: rtt });
    app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
    const url = `ws://127.0.0.1:${app.server.address().port}/ws`;
    const a = await client(url, module), b = await client(url, module);
    let loop;
    try {
      a.send({ type: 'queue' }); await a.wait('queued'); b.send({ type: 'queue' });
      a.side = (await a.wait('match')).side; b.side = (await b.wait('match')).side;
      loop = setInterval(() => { a.frame(); b.frame(); }, 1000 / 60);
      while (a.last?.phase !== 1) await delay(20);
      await delay(1500);
      const stats = a.pred.stats;
      // The clock: inputs arrive about SLACK ticks before the tick they were made for.
      assert.ok(Math.abs(stats.error) < 1.5, `clock error ${stats.error}`);
      const oneWay = rtt / 2 / (1000 / 60);
      assert.ok(stats.lead > oneWay * 2 && stats.lead < oneWay * 2 + 6, `lead ${stats.lead} for ${rtt} ms`);
      // A press shows on the presser's prediction within a frame or two,
      // while the server's word takes the round trip.
      const attacking = s => [1, 11, 17, 8, 18, 16].includes(s?.fighters[a.side].action);
      const t0 = performance.now(); a.press(LIGHT);
      let predicted = 0, confirmed = 0;
      while (!confirmed && performance.now() - t0 < 2000) {
        await delay(2);
        if (!predicted && attacking(a.pred.state())) predicted = performance.now() - t0;
        if (attacking(a.last)) confirmed = performance.now() - t0;
      }
      assert.ok(predicted > 0 && predicted < 40, `predicted after ${predicted} ms`);
      assert.ok(confirmed > predicted + rtt * 0.7, `server after ${confirmed} ms`);
      // Walk in and trade blows; then both stop and must agree with the server.
      const toward = a.side === 0 ? RIGHT : LEFT;
      a.held = toward; b.held = 0; await delay(800); a.held = 0;
      const before = stats.corrections, snaps = stats.snapshots;
      for (let i = 0; i < 10; i++) {
        a.press([LIGHT, KICK, LIGHT, LIGHT][i % 4]);
        if (i % 3 === 0) b.press(BLOCK, 200); else b.press(LIGHT);
        await delay(300);
      }
      await delay(2000);
      const rate = (stats.corrections - before) / (stats.snapshots - snaps);
      assert.ok(rate < 0.25, `corrected ${(rate * 100).toFixed(0)}% of snapshots`);
      const [pa, pb, server] = [a.pred.state(), b.pred.state(), a.last];
      for (const s of [pa, pb]) {
        assert.deepEqual(s.fighters.map(f => f.hp), server.fighters.map(f => f.hp));
        assert.deepEqual(s.fighters.map(f => f.x), server.fighters.map(f => f.x));
      }
      assert.ok(server.fighters.some(f => f.hp < 100), 'blows landed');
      console.log(`RTT ${rtt} ms: press shown after ${predicted.toFixed(0)} ms (server ${confirmed.toFixed(0)} ms), lead ${stats.lead.toFixed(1)} ticks, clock error ${stats.error.toFixed(2)}, corrected ${(rate * 100).toFixed(0)}% of snapshots, hp ${server.fighters.map(f => f.hp)}`);
    } finally {
      clearInterval(loop); a.ws.close(); b.ws.close(); await app.close();
    }
  });
}
