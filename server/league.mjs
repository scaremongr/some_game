// Ratings, leagues and chat-group standings, kept in one JSON file.
//
// Every online match (public queue or a friend's room) moves Elo ratings;
// a pair of players stops earning rating after RATED_PER_PAIR_DAY games a day
// so two accounts cannot farm each other. Groups are Telegram chats where the
// bot lives: players join a chat's table by a command or a button.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export const START = 1000;
export const RATED_PER_PAIR_DAY = 5;
export const LEAGUES = [
  { id: 'bronze', name: 'Бронза', icon: '🥉', min: -Infinity },
  { id: 'silver', name: 'Серебро', icon: '🥈', min: 1100 },
  { id: 'gold', name: 'Золото', icon: '🥇', min: 1250 },
  { id: 'platinum', name: 'Платина', icon: '💠', min: 1400 },
  { id: 'diamond', name: 'Алмаз', icon: '💎', min: 1550 },
  { id: 'legend', name: 'Легенда', icon: '👑', min: 1700 },
];

export function leagueOf(rating) {
  let found = LEAGUES[0];
  for (const l of LEAGUES) if (rating >= l.min) found = l;
  return { id: found.id, name: found.name, icon: found.icon };
}

const fresh = (id, name) => ({ id, name, photo: null, rating: START, peak: START, games: 0, wins: 0, losses: 0, draws: 0, streak: 0, best: 0, dm: false, last: 0 });

/** `dir` null keeps everything in memory (dev and tests). */
export function createLeague({ dir = null, now = () => Date.now() } = {}) {
  const file = dir ? join(dir, 'league.json') : null;
  let data = { v: 1, players: {}, groups: {}, pairs: {} };
  if (file) {
    mkdirSync(dir, { recursive: true });
    try {
      const loaded = JSON.parse(readFileSync(file, 'utf8'));
      if (loaded?.v === 1) data = { ...data, ...loaded };
    } catch { /* first start */ }
  }
  let timer = null, order = null;
  const flush = () => {
    clearTimeout(timer); timer = null;
    if (!file) return;
    const tmp = file + '.tmp';
    writeFileSync(tmp, JSON.stringify(data));
    renameSync(tmp, file);
  };
  const changed = () => {
    order = null;
    if (file && !timer) {
      timer = setTimeout(() => { try { flush(); } catch (e) { console.warn('league save', e.message); } }, 1500);
      timer.unref?.();
    }
  };
  // Rated players, best first (ties: more wins, then earlier id).
  const ranked = () => order ??= Object.values(data.players)
    .filter(p => p.games > 0)
    .sort((a, b) => b.rating - a.rating || b.wins - a.wins || (a.id < b.id ? -1 : 1));
  const day = () => new Date(now()).toISOString().slice(0, 10);

  const api = {
    flush,
    /** Creates or refreshes a player from a signed Telegram identity. */
    touch(user) {
      const p = data.players[user.id] ??= fresh(user.id, user.name);
      p.name = user.name;
      if (user.photo !== undefined) p.photo = user.photo || null;
      if (user.dm) p.dm = true;
      changed();
      return p;
    },
    player: id => data.players[id] ?? null,
    rank(id) {
      const i = ranked().findIndex(p => p.id === id);
      return i < 0 ? null : i + 1;
    },
    /** What a player sees of their own standing. */
    card(id) {
      const p = data.players[id];
      const rating = p?.rating ?? START;
      return {
        rating, league: leagueOf(rating), rank: p ? api.rank(id) : null, total: ranked().length,
        games: p?.games ?? 0, wins: p?.wins ?? 0, losses: p?.losses ?? 0, draws: p?.draws ?? 0, streak: p?.streak ?? 0,
      };
    },
    top(n = 50, ids = null) {
      const list = ids ? ranked().filter(p => ids.includes(p.id)) : ranked();
      return list.slice(0, n).map((p, i) => ({
        rank: i + 1, id: p.id, name: p.name, photo: !!p.photo, rating: p.rating, league: leagueOf(p.rating),
        wins: p.wins, losses: p.losses, draws: p.draws,
      }));
    },
    /**
     * Records a finished match. `winner` is 0, 1 or -1 (draw). Returns what
     * changed for each side: new rating, delta, league, whether it went up.
     */
    recordMatch(a, b, winner) {
      const pa = api.touch(a), pb = api.touch(b);
      const key = [pa.id, pb.id].sort().join('|') + '|' + day();
      for (const k of Object.keys(data.pairs)) if (!k.endsWith(day())) delete data.pairs[k];
      const count = (data.pairs[key] ?? 0) + 1;
      data.pairs[key] = count;
      const rated = count <= RATED_PER_PAIR_DAY;
      const expected = 1 / (1 + 10 ** ((pb.rating - pa.rating) / 400));
      const score = winner === 0 ? 1 : winner === 1 ? 0 : 0.5;
      const k = p => (p.games < 10 ? 40 : 24);
      let da = Math.round(k(pa) * (score - expected));
      let db = Math.round(k(pb) * ((1 - score) - (1 - expected)));
      if (winner === 0) { da = Math.max(1, da); db = Math.min(-1, db); }
      if (winner === 1) { db = Math.max(1, db); da = Math.min(-1, da); }
      if (!rated) { da = 0; db = 0; }
      const apply = (p, delta, result) => {
        const before = leagueOf(p.rating);
        p.rating = Math.max(100, p.rating + delta);
        p.peak = Math.max(p.peak, p.rating);
        p.games += 1;
        p.last = now();
        if (result === 1) { p.wins += 1; p.streak = Math.max(1, p.streak + 1); p.best = Math.max(p.best, p.streak); }
        else if (result === 0) { p.losses += 1; p.streak = Math.min(-1, p.streak - 1); }
        else { p.draws += 1; p.streak = 0; }
        const after = leagueOf(p.rating);
        return { before, after };
      };
      const la = apply(pa, da, score), lb = apply(pb, db, 1 - score);
      changed();
      const view = (p, delta, l) => ({
        ...api.card(p.id), delta, rated,
        leagueUp: LEAGUES.findIndex(x => x.id === l.after.id) > LEAGUES.findIndex(x => x.id === l.before.id),
      });
      return [view(pa, da, la), view(pb, db, lb)];
    },
    setDm(id, allowed) {
      const p = data.players[id];
      if (p && p.dm !== allowed) { p.dm = allowed; changed(); }
    },
    // ---- chat groups
    join(chat, title, user) {
      api.touch(user);
      const g = data.groups[chat] ??= { title, members: [] };
      g.title = title || g.title;
      const added = !g.members.includes(user.id);
      if (added) g.members.push(user.id);
      changed();
      return added;
    },
    leave(chat, id) {
      const g = data.groups[chat];
      if (!g) return false;
      const before = g.members.length;
      g.members = g.members.filter(m => m !== id);
      changed();
      return g.members.length < before;
    },
    group: chat => data.groups[chat] ?? null,
    /** The chat's table: its members who have played, best first. */
    groupTop(chat, n = 15) {
      const g = data.groups[chat];
      return g ? api.top(n, g.members) : [];
    },
    /** Chats where both players are on the table. */
    sharedGroups(a, b) {
      return Object.entries(data.groups).filter(([, g]) => g.members.includes(a) && g.members.includes(b)).map(([chat]) => chat);
    },
    dropGroup(chat) { if (data.groups[chat]) { delete data.groups[chat]; changed(); } },
  };
  return api;
}
