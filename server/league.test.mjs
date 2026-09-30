import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createLeague, leagueOf, RATED_PER_PAIR_DAY, START } from './league.mjs';

const ann = { id: '1', name: 'Анна', photo: 'https://t.me/i/userpic/320/a.jpg' };
const bob = { id: '2', name: 'Борис' };

test('Elo: the winner gains what the loser loses, underdogs gain more', () => {
  const league = createLeague();
  const [a, b] = league.recordMatch(ann, bob, 0);
  assert.equal(a.delta, 20); assert.equal(b.delta, -20);
  assert.equal(a.rating, START + 20); assert.equal(a.wins, 1); assert.equal(b.losses, 1);
  assert.equal(a.rank, 1); assert.equal(b.rank, 2);
  // Now Boris is the underdog: beating Anna is worth more than 20.
  const [, b2] = league.recordMatch(ann, bob, 1);
  assert.ok(b2.delta > 20, `underdog gained ${b2.delta}`);
  const [d] = league.recordMatch(ann, bob, -1);
  assert.equal(d.draws, 1);
});

test('a pair stops earning rating after a few games a day; leagues and ranks', () => {
  let t = Date.parse('2026-10-01T10:00:00Z');
  const league = createLeague({ now: () => t });
  for (let i = 0; i < RATED_PER_PAIR_DAY; i++) assert.ok(league.recordMatch(ann, bob, 0)[0].delta > 0);
  const [farm] = league.recordMatch(ann, bob, 0);
  assert.equal(farm.delta, 0); assert.equal(farm.rated, false);
  t += 24 * 3600 * 1000;
  assert.ok(league.recordMatch(ann, bob, 0)[0].delta > 0, 'next day counts again');
  assert.equal(leagueOf(1000).id, 'bronze'); assert.equal(leagueOf(1100).id, 'silver'); assert.equal(leagueOf(1750).id, 'legend');
  assert.deepEqual(league.top(10).map(p => p.id), ['1', '2']);
  assert.equal(league.top(10)[0].photo, true);
});

test('league up is reported; groups keep their own tables; data survives a restart', () => {
  const dir = mkdtempSync(join(tmpdir(), 'league-'));
  try {
    const league = createLeague({ dir });
    league.touch({ ...ann });
    league.player('1').rating = 1095;
    const [up] = league.recordMatch(ann, bob, 0);
    assert.equal(up.league.id, 'silver'); assert.equal(up.leagueUp, true);
    assert.equal(league.join('-100', 'Друзья', ann), true);
    assert.equal(league.join('-100', 'Друзья', ann), false);
    league.join('-100', 'Друзья', bob);
    league.join('-200', 'Работа', ann);
    assert.deepEqual(league.sharedGroups('1', '2'), ['-100']);
    assert.deepEqual(league.groupTop('-100').map(p => p.name), ['Анна', 'Борис']);
    league.flush();
    const again = createLeague({ dir });
    assert.equal(again.player('1').rating, up.rating);
    assert.deepEqual(again.groupTop('-100').map(p => p.id), ['1', '2']);
    assert.equal(again.leave('-100', '2'), true);
    assert.deepEqual(again.sharedGroups('1', '2'), []);
    again.flush();
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
