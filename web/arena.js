import { simulation } from './combat.js';
const $ = id => document.getElementById(id);
const tg = window.Telegram?.WebApp;
const encoder = new TextEncoder();
const STEP = 1000 / 60;
// Online rendering trails the newest snapshot by ~one snapshot interval so it
// can interpolate instead of stepping at 20 Hz.
const NET_DELAY = 3.5;
const BIT = { LEFT: 1, RIGHT: 2, BLOCK: 4, LIGHT: 8, HEAVY: 16, DASH: 32, THROW: 64, KICK: 128, CROUCH: 256, JUMP: 512, SPECIAL: 1024, SMASH: 2048 };
const EDGE = 3832;
const panels = ['lobby', 'waiting', 'hud', 'controls', 'fight-tools', 'fight-tip', 'result'];
let module, local, state, mode = 'lobby', side = 0, socket, authenticated = false, connecting;
let config = { dev: false, miniApp: '' }, seq = Date.now() * 1000, lastSnapshot = 0, lastEvent = 0, eventUntil = 0;
let reconnectTimer, reconnectStarted = 0, toastTimer, paused = false, rematchPossible = true, rematchRequested = false;
let wallImpacts = [0, 0], wallRound = 0;
let sound = false, audio, bits = 0, shownBits = -1, lastSent = 0, stopped = false, trainingMode = 0;
let simTime = 0, netOffset = null, pendingEdges = 0, openedByLink = false;
const held = new Map(), latched = new Map();
const touch = matchMedia('(pointer: coarse)').matches || 'ontouchstart' in window;
const storage = { get(k) { try { return sessionStorage.getItem(k); } catch { return null; } }, set(k, v) { try { sessionStorage.setItem(k, v); } catch {} } };
const fighting = () => mode === 'local' || mode === 'online';

// ---- Fighters ------------------------------------------------------------------
// The roster ships with the client (assets/fighters/roster.json); the server
// accepts the same ids and tells both players who fights whom. A fighter is
// only a look: every body uses the same combat rules.
const DEFAULT_FIGHTER = { id: 'medea', name: 'Медея', model: 'assets/character.glb', pack: 'assets/fight.pack', portrait: 'assets/fighters/medea.jpg' };
let roster = [DEFAULT_FIGHTER], fighter = DEFAULT_FIGHTER, lobbyRival = DEFAULT_FIGHTER;

// ---- People: names and photos --------------------------------------------------
// Everyone on screen is a person: name and photo in the lobby, the fight HUD,
// the VS splash and the result. Photos come from Telegram; once signed in the
// game serves them itself (avatar/<id>.jpg), opponents' photos included.
const tgUser = tg?.initDataUnsafe?.user;
const me = { id: tgUser ? String(tgUser.id) : null, name: tgUser?.first_name || 'Игрок', photo: tgUser?.photo_url || null, avatar: null };
const BOT = { name: 'Бот', bot: true };
let people = [me, BOT], staged = null;
function initials(name, seed = name) {
  const letters = (name || '?').trim().split(/\s+/).map(w => w[0]).join('').slice(0, 2).toUpperCase();
  const hues = [168, 12, 42, 210, 280, 330, 95];
  const hue = hues[[...String(seed)].reduce((a, c) => a + c.charCodeAt(0), 0) % hues.length];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${hue},70%,58%)"/><stop offset="1" stop-color="hsl(${hue + 25},65%,40%)"/></linearGradient></defs><rect width="100" height="100" fill="url(#g)"/><text x="50" y="50" dy=".35em" text-anchor="middle" font-family="Inter,Segoe UI,Arial" font-weight="800" font-size="40" fill="#fff">${letters.replace(/[<&>]/g, '')}</text></svg>`;
  return 'data:image/svg+xml,' + encodeURIComponent(svg);
}
const BOT_PICTURE = 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="#1b2a38"/><rect x="24" y="30" width="52" height="42" rx="12" fill="#ff7965"/><circle cx="40" cy="50" r="7" fill="#1b2a38"/><circle cx="60" cy="50" r="7" fill="#1b2a38"/><rect x="47" y="16" width="6" height="14" fill="#ff7965"/><circle cx="50" cy="15" r="5" fill="#e9bc73"/></svg>');
function picture(p) { return p?.bot ? BOT_PICTURE : p?.avatar || p?.photo || initials(p?.name, p?.id || p?.name); }
function showPicture(id, p) { showPictureEl($(id), p); }
function showPictureEl(img, p) {
  if (!img) return;
  const src = picture(p);
  if (img.dataset.src === src) return;
  img.dataset.src = src; img.src = src;
  img.onerror = () => { img.onerror = null; img.src = initials(p?.name, p?.id || p?.name); };
}
// Bodies on each side (kept for the victory card).
let onStage = [DEFAULT_FIGHTER, DEFAULT_FIGHTER];
function stage(left, right) { onStage = [left, right]; window.arenaFighters = [left, right].map(f => ({ model: f.model, pack: f.pack })); }

// ---- Rating, leaderboard, victory card, revenge ----------------------------------
let myCard = null, lastResult = null, challengeTimer;
const inTelegram = () => !!tg?.initData;
function rankLine(c) { return c ? `${c.league.icon} ${c.rating}` : '🏆 РЕЙТИНГ'; }
function showMyCard(card) {
  if (card) myCard = card;
  text('my-rank', rankLine(myCard));
}
async function openLeaders() {
  if (!inTelegram() && !config.dev) { toast('Рейтинг ведётся для игроков в Telegram: открой игру через @' + botName() + '.'); return; }
  text('leaders-you', 'Загружаем таблицу…'); $('leaders-list').replaceChildren(); $('leaders').showModal();
  try { await connect(); send({ type: 'top' }); } catch (e) { text('leaders-you', e.message); }
}
function renderLeaders(m) {
  const you = m.you;
  $('leaders-you').innerHTML = you.games
    ? `Ты: <b>${you.league.icon} ${you.league.name}</b> · рейтинг <b>${you.rating}</b> · место <b>${you.rank}</b> из ${you.total}<br>Побед ${you.wins}, поражений ${you.losses}${you.streak > 1 ? ` · 🔥 серия ${you.streak}` : ''}`
    : 'Сыграй первый бой в сети («Найти соперника» или с другом) — и попадёшь в таблицу. Старт: 🥉 1000.';
  $('leaders-list').replaceChildren(...m.players.map(p => {
    const li = document.createElement('li'); if (p.id === you.id) li.className = 'me';
    const place = document.createElement('span'); place.className = 'place'; place.textContent = ['🥇', '🥈', '🥉'][p.rank - 1] || p.rank;
    const img = document.createElement('img'); img.alt = ''; showPictureEl(img, { id: p.id, name: p.name, avatar: p.avatar });
    const who = document.createElement('span'); who.className = 'who';
    const b = document.createElement('b'); b.textContent = p.name;
    const s = document.createElement('span'); s.textContent = `${p.league.icon} ${p.league.name} · ${p.wins}–${p.losses}`;
    who.append(b, s);
    const score = document.createElement('span'); score.className = 'score'; score.textContent = p.rating;
    li.append(place, img, who, score); return li;
  }));
  if (!m.players.length) $('leaders-list').innerHTML = '<li><span></span><span></span><span class="who"><b>Пока пусто</b><span>Стань первым!</span></span><span></span></li>';
}
function renderResultRating() {
  const r = lastResult;
  show('result-rating', !!r);
  show('brag', !!(r?.won && r.card));
  $('brag').disabled = false; text('brag', '📣 Похвастаться в чате');
  const canAsk = inTelegram() && tg.requestWriteAccess && tg.isVersionAtLeast?.('6.9') && !tgUser?.allows_write_to_pm && !prefs.get('pulse-notify');
  show('notify', !!(r && canAsk));
  if (!r) return;
  const y = r.you, sign = y.delta > 0 ? '+' : '';
  $('result-rating').innerHTML = `<span class="delta ${y.delta >= 0 ? 'up' : 'down'}">${y.delta ? sign + y.delta : '±0'}</span>${y.league.icon} ${y.rating} <span class="muted">· место ${y.rank ?? '—'}</span>`
    + (y.rated ? '' : '<span class="promo" style="color:var(--muted)">С этим соперником сегодня уже много боёв — рейтинг не меняется.</span>')
    + (y.leagueUp ? `<span class="promo">🎉 Новая лига: ${y.league.icon} ${y.league.name}!</span>` : '');
}
function botName() { return (config.miniApp || 'https://t.me/somee_game_bot').split('/').pop(); }
// The victory card: a 1200×630 picture drawn here, uploaded once, then sent
// by the player to any chat through Telegram's own share dialog.
async function makeCard(r) {
  const c = document.createElement('canvas'); c.width = 1200; c.height = 630;
  const g = c.getContext('2d');
  const font = (w, s) => `${w} ${s}px Inter, "Segoe UI", Roboto, Arial, sans-serif`;
  const load = src => new Promise(res => { if (!src || src.startsWith('data:')) return res(null); const i = new Image(); i.onload = () => res(i); i.onerror = () => res(null); i.src = src; });
  const mine = onStage[side] || DEFAULT_FIGHTER, theirs = onStage[1 - side] || DEFAULT_FIGHTER;
  const opp = people[1 - side] || { name: r.opponent.name };
  const [bg, pa, pb, av, ov] = await Promise.all([load('assets/bot/card-bg.jpg'), load(mine.portrait), load(theirs.portrait), load(me.avatar), load(opp.avatar)]);
  const cover = (img, x, y, w, h) => { const k = Math.max(w / img.width, h / img.height); const sw = w / k, sh = h / k; g.drawImage(img, (img.width - sw) / 2, (img.height - sh) / 2, sw, sh, x, y, w, h); };
  g.fillStyle = '#080e18'; g.fillRect(0, 0, 1200, 630);
  if (bg) { g.globalAlpha = 0.55; cover(bg, 0, 0, 1200, 630); g.globalAlpha = 1; }
  const shade = g.createLinearGradient(0, 0, 1200, 0); shade.addColorStop(0, '#080e18f2'); shade.addColorStop(0.55, '#080e18b0'); shade.addColorStop(1, '#080e1860');
  g.fillStyle = shade; g.fillRect(0, 0, 1200, 630);
  const frame = (img, x, y, w, h, color, dim) => {
    g.save(); g.beginPath(); g.roundRect?.(x, y, w, h, 18) ?? g.rect(x, y, w, h); g.clip();
    g.fillStyle = '#0f1d29'; g.fillRect(x, y, w, h);
    if (img) { if (dim) g.filter = 'grayscale(0.7) brightness(0.7)'; cover(img, x, y, w, h); g.filter = 'none'; }
    g.restore(); g.lineWidth = 6; g.strokeStyle = color; g.beginPath(); g.roundRect?.(x, y, w, h, 18) ?? g.rect(x, y, w, h); g.stroke();
  };
  frame(pa, 640, 70, 330, 440, '#57f0ce', false);
  frame(pb, 995, 284, 170, 226, '#ff7965', true);
  g.fillStyle = '#57f0ce'; g.font = font(900, 46); g.fillText('ϟ', 60, 92);
  g.fillStyle = '#fff'; g.fillText('PULSE', 100, 92);
  g.fillStyle = '#e9bc73'; g.font = font(900, 104); g.fillText('ПОБЕДА', 56, 215);
  // A conceded fight ends before the rounds do: no score to show.
  if (r.score[0] > r.score[1]) { g.fillStyle = '#fff'; g.font = font(900, 84); g.fillText(`${r.score[0]} : ${r.score[1]}`, 60, 310); }
  else { g.fillStyle = '#c7d6dc'; g.font = font(800, 44); g.fillText('соперник сдался', 62, 296); }
  const disc = (img, p, x, y, d, ring) => {
    g.save(); g.beginPath(); g.arc(x + d / 2, y + d / 2, d / 2, 0, Math.PI * 2); g.clip();
    if (img) g.drawImage(img, x, y, d, d);
    else { g.fillStyle = '#2a6f63'; g.fillRect(x, y, d, d); g.fillStyle = '#fff'; g.font = font(800, d * 0.42); g.textAlign = 'center'; g.fillText((p.name || '?').slice(0, 1).toUpperCase(), x + d / 2, y + d * 0.65); g.textAlign = 'left'; }
    g.restore(); g.lineWidth = 5; g.strokeStyle = ring; g.beginPath(); g.arc(x + d / 2, y + d / 2, d / 2, 0, Math.PI * 2); g.stroke();
  };
  const fit = (s, max, size, weight) => { g.font = font(weight, size); let t = s; while (g.measureText(t).width > max && t.length > 2) t = t.slice(0, -2) + '…'; return t; };
  disc(av, me, 60, 360, 96, '#57f0ce');
  g.fillStyle = '#fff'; g.fillText(fit(me.name, 460, 40, 800), 176, 402);
  g.fillStyle = '#e9bc73'; g.font = font(700, 26); g.fillText(`${r.you.league.icon} ${r.you.league.name} · ${r.you.rating} (${r.you.delta >= 0 ? '+' : ''}${r.you.delta})`, 176, 442);
  disc(ov, opp, 60, 478, 56, '#ff7965');
  g.fillStyle = '#a9bac5'; g.fillText(fit('соперник: ' + opp.name, 440, 26, 600), 132, 515);
  g.fillStyle = '#7f95a1'; g.font = font(600, 22); g.fillText(`Сразись со мной в Telegram: @${botName()}`, 60, 590);
  return new Promise(res => c.toBlob(res, 'image/jpeg', 0.88));
}
async function brag() {
  const r = lastResult; if (!r?.card) return;
  $('brag').disabled = true; text('brag', 'Готовим карточку…');
  try {
    const blob = await makeCard(r);
    const reply = await fetch('card?t=' + encodeURIComponent(r.card), { method: 'POST', body: blob }).then(x => x.json());
    r.card = null;
    if (reply.prepared && tg?.shareMessage && tg.isVersionAtLeast?.('8.0')) {
      tg.shareMessage(reply.prepared, sent => { if (sent) toast('Карточка отправлена!'); });
    } else {
      const link = `https://t.me/${botName()}?startapp=fight_home`;
      const url = 'https://t.me/share/url?url=' + encodeURIComponent(link) + '&text=' + encodeURIComponent(`🏆 Победа в PULSE ${r.score[0]}:${r.score[1]}! ${r.you.league.icon} ${r.you.rating}. Сразись со мной ⚡`);
      if (tg?.openTelegramLink) tg.openTelegramLink(url); else window.open(url, '_blank', 'noopener,noreferrer');
    }
    text('brag', '✅ Готово'); show('brag', false);
  } catch { $('brag').disabled = false; text('brag', '📣 Похвастаться в чате'); toast('Не получилось подготовить карточку. Попробуй ещё раз.'); }
}
// Test hooks: the victory card of the last result, the signed-in player id.
window.arenaTest = { card: () => lastResult && makeCard(lastResult), me: () => me.id };
function showChallenge(m) {
  if (fighting() && mode === 'online') return;
  showPictureEl($('challenge-avatar'), m.from); text('challenge-name', m.from.name);
  $('challenge').dataset.code = m.code; show('challenge'); haptic('heavy');
  clearTimeout(challengeTimer); challengeTimer = setTimeout(() => show('challenge', false), 90000);
}
function showPeople(list = people) {
  people = list;
  for (let i = 0; i < 2; i++) showPicture('avatar-' + i, list[i]);
}
// «VS» splash as a fight begins: who fights whom, with which fighter.
let versusTimer;
function versus(fighters) {
  for (let i = 0; i < 2; i++) {
    showPicture('vs-avatar-' + i, people[i]);
    text('vs-name-' + i, (i === side ? 'Ты' : people[i]?.name) || '');
    const p = people[i];
    text('vs-fighter-' + i, (fighters[i]?.name || '') + (p?.league ? ` · ${p.league.icon} ${p.rating}` : ''));
  }
  show('versus'); clearTimeout(versusTimer); versusTimer = setTimeout(() => show('versus', false), 2300);
}
function showMe() {
  showPicture('me-avatar', me); show('me-avatar', !!(me.photo || me.avatar || tgUser));
}
const prefs = { get(k) { try { return localStorage.getItem(k); } catch { return null; } }, set(k, v) { try { localStorage.setItem(k, v); } catch {} } };
const fighterById = id => roster.find(f => f.id === id) || roster[0];
function rival() { const others = roster.filter(f => f !== fighter); return others.length ? others[Math.floor(Math.random() * others.length)] : fighter; }
async function loadRoster() {
  try {
    const list = await fetch('assets/fighters/roster.json').then(r => r.ok ? r.json() : []);
    const valid = list.filter(f => f && [f.id, f.name, f.model, f.pack].every(v => typeof v === 'string') && /^[a-z0-9_-]{1,24}$/.test(f.id));
    if (valid.length) roster = valid;
  } catch {}
  fighter = fighterById(prefs.get('pulse-fighter'));
  lobbyRival = rival();
  renderRoster(); stage(fighter, lobbyRival);
}
function renderRoster() {
  const strip = $('fighters');
  strip.replaceChildren(...roster.map(f => {
    const card = document.createElement('button');
    card.className = 'fighter'; card.setAttribute('role', 'radio'); card.dataset.id = f.id;
    if (f.portrait) { const img = new Image(); img.src = f.portrait; img.alt = ''; img.decoding = 'async'; img.draggable = false; img.onerror = () => img.remove(); card.append(img); }
    const name = document.createElement('span'); name.textContent = f.name; card.append(name);
    card.onclick = () => pickFighter(f);
    return card;
  }));
  markFighter(); show('fighter-pick', roster.length > 1); requestAnimationFrame(stripArrows);
}
// The strip scrolls by swipe, mouse wheel or the edge arrows.
function stripArrows() {
  const strip = $('fighters'), end = strip.scrollWidth - strip.clientWidth - 2;
  $('fighters-prev').hidden = strip.scrollLeft <= 8; $('fighters-next').hidden = strip.scrollLeft >= end - 6;
}
$('fighters').addEventListener('scroll', stripArrows, { passive: true });
// Finger and mouse drags scroll the strip in script: Telegram's WebViews do
// not all honour touch-action under a page that blocks gestures. A drag never
// counts as picking the card under the finger.
(() => {
  const strip = $('fighters');
  let drag = null, suppress = false;
  strip.addEventListener('pointerdown', e => { drag = { id: e.pointerId, x: e.clientX, left: strip.scrollLeft, moved: false, t: performance.now(), v: 0 }; });
  strip.addEventListener('pointermove', e => {
    if (!drag || e.pointerId !== drag.id) return;
    const dx = e.clientX - drag.x;
    if (!drag.moved && Math.abs(dx) > 6) { drag.moved = true; strip.setPointerCapture?.(e.pointerId); strip.style.scrollSnapType = 'none'; }
    if (!drag.moved) return;
    const now = performance.now(), before = strip.scrollLeft;
    strip.scrollLeft = drag.left - dx;
    drag.v = (strip.scrollLeft - before) / Math.max(1, now - drag.t); drag.t = now;
  });
  const end = e => {
    if (!drag || e.pointerId !== drag.id) return;
    if (drag.moved) {
      suppress = true; setTimeout(() => { suppress = false; }, 0);
      let v = drag.v * 16;
      const fling = () => { if (Math.abs(v) < 0.5) { strip.style.scrollSnapType = ''; return; } strip.scrollLeft += v; v *= 0.92; requestAnimationFrame(fling); };
      requestAnimationFrame(fling);
    }
    drag = null;
  };
  strip.addEventListener('pointerup', end); strip.addEventListener('pointercancel', end);
  strip.addEventListener('click', e => { if (suppress) { e.stopPropagation(); e.preventDefault(); } }, true);
})();
$('fighters').addEventListener('wheel', e => { if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) { $('fighters').scrollLeft += e.deltaY; e.preventDefault(); } }, { passive: false });
for (const [id, dir] of [['fighters-prev', -1], ['fighters-next', 1]]) $(id).onclick = () => $('fighters').scrollBy({ left: dir * 3 * 77, behavior: 'smooth' });
window.addEventListener('resize', () => requestAnimationFrame(stripArrows));
function markFighter() { for (const card of $('fighters').children) card.setAttribute('aria-checked', String(card.dataset.id === fighter.id)); }
function pickFighter(f) {
  fighter = f; prefs.set('pulse-fighter', f.id); markFighter(); haptic();
  $('fighters').querySelector(`[data-id="${f.id}"]`)?.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' });
  if (lobbyRival === f) lobbyRival = rival();
  if (mode === 'lobby') stage(fighter, lobbyRival);
}

function show(id, visible = true) { $(id).classList.toggle('hidden', !visible); }
const shown = new Map();
function text(id, value) { value = String(value); if (shown.get(id) !== value) { shown.set(id, value); $(id).textContent = value; } }
function width(id, value) { const key = id + ':w'; if (shown.get(key) !== value) { shown.set(key, value); $(id).style.width = value; } }
function toast(message, ms = 5000) { text('toast', message); show('toast'); clearTimeout(toastTimer); toastTimer = setTimeout(() => show('toast', false), ms); }
function haptic(kind = 'light') {
  try { if (tg?.HapticFeedback) tg.HapticFeedback.impactOccurred(kind); else navigator.vibrate?.(kind === 'heavy' ? 22 : 7); } catch {}
}

function screen(next) {
  mode = next; panels.forEach(id => show(id, false)); show('announcement', false); text('combat-event', '');
  document.body.classList.toggle('fighting', fighting());
  if (next === 'lobby') show('lobby');
  if (next === 'waiting') show('waiting');
  if (fighting()) { for (const id of ['hud', 'controls', 'fight-tools']) show(id); show('fight-tip', !touch); }
  show('training-mode', next === 'local');
  if (tg?.BackButton) { if (next === 'lobby') tg.BackButton.hide(); else tg.BackButton.show(); }
  requestAnimationFrame(layout);
  if (fighting() && touch && innerHeight > innerWidth && !storage.get('pulse-rotate-hint')) {
    storage.set('pulse-rotate-hint', '1');
    // After the VS splash, not over it.
    setTimeout(() => toast('Можно играть и горизонтально — поверни телефон, бойцы станут крупнее.' + (canFullscreen() ? ' ⛶ — во весь экран.' : ''), 4500), 2800);
  }
}

// ---- Screen, safe areas, orientation --------------------------------------
function inset(name) { return (tg?.safeAreaInset?.[name] || 0) + (tg?.contentSafeAreaInset?.[name] || 0); }
function viewport() {
  const h = tg?.viewportStableHeight || innerHeight, w = innerWidth;
  const canvas = $('glcanvas'); canvas.style.width = w + 'px'; canvas.style.height = h + 'px'; document.body.style.height = h + 'px';
  const root = document.documentElement.style;
  for (const [name, css] of [['top', '--tg-top'], ['bottom', '--tg-bottom'], ['left', '--tg-left'], ['right', '--tg-right']]) root.setProperty(css, inset(name) + 'px');
  const body = document.body.classList;
  body.toggle('touch', touch); body.toggle('portrait', h > w); body.toggle('landscape', h <= w);
  if (window.wasm_exports && typeof window.resize === 'function') window.resize(canvas, window.wasm_exports.resize);
  show('fullscreen', canFullscreen());
  requestAnimationFrame(layout);
}
// Tells the renderer which screen band is free for the fighters.
function layout() {
  const h = $('glcanvas').clientHeight || innerHeight;
  let top = 0.1, bottom = 0.95;
  if (fighting()) {
    top = ($('hud').getBoundingClientRect().bottom + 6) / h;
    const tools = $('fight-tools').getBoundingClientRect();
    if (tools.height && tools.bottom < h * 0.4) top = Math.max(top, (tools.bottom + 4) / h);
    bottom = 0.985;
    if (touch && h > innerWidth) bottom = Math.min(bottom, ($('pad').getBoundingClientRect().top - 8) / h);
  }
  window.arenaLayout = [Math.min(top, 0.6), Math.max(bottom, top + 0.3)];
}
function canFullscreen() {
  if (tg?.requestFullscreen && tg.isVersionAtLeast?.('8.0')) return true;
  return !!(document.fullscreenEnabled && document.documentElement.requestFullscreen) && !tg?.initData;
}
function toggleFullscreen() {
  try {
    if (tg?.requestFullscreen && tg.isVersionAtLeast?.('8.0')) { if (tg.isFullscreen) tg.exitFullscreen(); else tg.requestFullscreen(); }
    else if (document.fullscreenElement) document.exitFullscreen();
    else document.documentElement.requestFullscreen({ navigationUI: 'hide' }).catch(() => {});
  } catch {}
}

// ---- Sound ------------------------------------------------------------------
function tone(kind) {
  if (!sound) return;
  try {
    audio ||= new (window.AudioContext || window.webkitAudioContext)(); audio.resume();
    const osc = audio.createOscillator(), gain = audio.createGain(), time = audio.currentTime;
    const heavy = kind === 5 || kind === 8 || state?.fighters[1 - state.event_target]?.action === 2;
    const duration = kind === 3 ? .13 : heavy ? .23 : .11;
    osc.type = kind === 3 ? 'sine' : 'triangle'; osc.frequency.setValueAtTime(kind === 3 ? 980 : kind === 2 ? 270 : heavy ? 92 : 165, time);
    osc.frequency.exponentialRampToValueAtTime(kind === 3 ? 1450 : 40, time + duration);
    gain.gain.setValueAtTime(heavy ? .22 : .12, time); gain.gain.exponentialRampToValueAtTime(.001, time + duration);
    osc.connect(gain); gain.connect(audio.destination); osc.start(); osc.stop(time + duration + .01);
    const buffer = audio.createBuffer(1, Math.ceil(audio.sampleRate * .14), audio.sampleRate);
    const data = buffer.getChannelData(0); for (let i = 0; i < data.length; i++) data[i] = (Math.random() * 2 - 1) * (1 - i / data.length);
    const noise = audio.createBufferSource(), filter = audio.createBiquadFilter(), crack = audio.createGain();
    noise.buffer = buffer; filter.type = 'bandpass'; filter.frequency.value = kind === 2 ? 1800 : 850;
    crack.gain.setValueAtTime(kind === 2 ? .10 : .18, time); crack.gain.exponentialRampToValueAtTime(.001, time + .10);
    noise.connect(filter); filter.connect(crack); crack.connect(audio.destination); noise.start(time);
  } catch {}
}
function setSound(on) {
  sound = on; text('sound', sound ? 'ЗВУК ВКЛ' : 'ЗВУК ВЫКЛ'); $('fight-sound').classList.toggle('on', sound);
  for (const id of ['sound', 'fight-sound']) $(id).setAttribute('aria-label', sound ? 'Выключить звук' : 'Включить звук');
  tone(3);
}

// ---- State from the simulation ------------------------------------------------
// Render tick for the 3D view: continuous, so animation is smooth at any refresh rate.
window.arenaClock = () => {
  if (!state || !fighting()) return -1;
  const now = performance.now();
  if (mode === 'local') return Math.max(0, state.tick - 1 + Math.min(1.5, Math.max(0, (now - simTime) / STEP)));
  if (netOffset === null) return -1;
  return now / STEP + netOffset - NET_DELAY;
};
function accept(next, isPaused = false) {
  state = next; paused = isPaused; lastSnapshot = performance.now();
  window.arenaRenderBytes = encoder.encode(JSON.stringify(state));
  if (!fighting()) return;
  if (wallRound !== state.round || state.tick < 5) { wallImpacts = [0, 0]; wallRound = state.round; }
  state.walls.forEach((wall, i) => {
    if (wall.impacts > wallImpacts[i]) { eventUntil = performance.now() + 1100; text('combat-event', 'УДАР О СТЕНУ'); tone(1); haptic('heavy'); }
    wallImpacts[i] = wall.impacts;
  });
  for (let i = 0; i < 2; i++) {
    const f = state.fighters[i];
    width('hp-' + i, f.hp + '%'); width('stamina-' + i, f.stamina / 10 + '%'); width('meter-' + i, f.meter / 10 + '%');
    text('health-' + i, f.hp + ' HP');
    text('score-' + i, Array.from({ length: 2 }, (_, n) => n < state.score[i] ? '●' : '○').join(' '));
  }
  text('timer', Math.ceil(state.remaining / 60)); text('round', 'РАУНД ' + state.round);
  const mine = state.fighters[side];
  const roomIndex = mine.x < -8000 ? 0 : mine.x < -3000 ? 1 : mine.x < 3000 ? 2 : mine.x < 8000 ? 3 : 4;
  text('room-name', ['ЗИМНИЙ САД', 'КУХНЯ', 'ГОСТИНАЯ', 'КАБИНЕТ', 'СПАЛЬНЯ'][roomIndex]);
  text('combo-readout', mine.combo > 1 && mine.combo_time > 0 ? `${mine.combo} УДАРА · ${mine.combo_damage}% УРОНА` : '');
  text('room-damage', 'РАЗРУШИТЬ · ' + Math.round(state.objects.filter(o => o.hp === 0).length / state.objects.length * 100) + '%');
  const special = document.querySelector('.act.special');
  special.classList.toggle('charged', mine.meter >= 500);
  special.classList.toggle('breaker', breakerReady());
  const label = paused ? 'СОПЕРНИК ПЕРЕПОДКЛЮЧАЕТСЯ' : state.phase === 0 ? String(Math.max(1, Math.ceil(state.phase_ticks / 60))) : state.phase === 2 ? (state.winner < 0 ? 'НИЧЬЯ' : state.winner === side ? 'ТВОЙ РАУНД' : 'РАУНД СОПЕРНИКА') : '';
  text('announcement', label); show('announcement', !!label);
  if (state.event !== lastEvent) {
    lastEvent = state.event; eventUntil = performance.now() + 700;
    const labels = ['', 'ПОПАДАНИЕ', 'БЛОК', 'ПАРИРОВАНИЕ', 'ЗАЩИТА СЛОМАНА', 'ЗАХВАТ', 'КОНТРАТАКА', 'НАКАЗАНИЕ', 'ВЫХОД ИЗ КОМБО'];
    const combo = state.fighters[1 - state.event_target].combo;
    text('combat-event', (combo > 1 && state.event_kind !== 2 ? combo + ' × СВЯЗКА · ' : '') + labels[state.event_kind]); tone(state.event_kind);
    haptic(state.event_kind === 2 ? 'light' : state.event_target === side ? 'heavy' : 'medium');
  }
  if (state.phase === 3) {
    show('result'); show('controls', false); show('announcement', false); clearInput();
    text('result-title', state.winner === side ? 'Твоя победа.' : 'Ещё не конец.');
    if (state.winner >= 0) showPicture('result-avatar', people[state.winner]);
    show('result-avatar', state.winner >= 0);
    text('result-score', state.score[side] + ' : ' + state.score[1 - side]);
    text('result-copy', state.winner === side ? 'Тайминг решил. Повторим?' : 'Прочитай замах. Поймай момент. Возьми реванш.');
    $('rematch').disabled = !rematchPossible || rematchRequested;
  }
}
function breakerReady() { const mine = state?.fighters[side]; return !!mine && mine.stun > 0 && mine.down === 0 && mine.meter >= 1000; }

function startPractice() {
  if (!module) return;
  if (authenticated) send({ type: 'leave' });
  clearInput(); side = 0; lastEvent = 0; rematchPossible = true; rematchRequested = false; local = simulation(module, crypto.getRandomValues(new Uint32Array(1))[0]);
  simTime = performance.now();
  // The lobby's rival is already loaded: it becomes the bot, a new one waits.
  const bot = lobbyRival !== fighter ? lobbyRival : rival(); lobbyRival = rival();
  showPeople([me, { ...BOT, name: 'Бот · ' + bot.name }]); stage(fighter, bot); lastResult = null; renderResultRating();
  screen('local'); text('result-eyebrow', 'БОЙ ОКОНЧЕН'); text('mode', 'ТРЕНИРОВКА'); text('name-0', me.name); text('name-1', 'БОТ · ' + bot.name.toUpperCase()); text('rematch', 'Ещё бой →');
  versus([fighter, bot]);
  text('connection', 'ТРЕНИРОВКА С БОТОМ'); accept(local.state());
}
function home() {
  clearInput(); local = null; if (authenticated) send({ type: 'leave' }); screen('lobby'); showPeople([me, BOT]); stage(fighter, lobbyRival);
  text('connection', authenticated ? 'АРЕНА НА СВЯЗИ' : 'TELEGRAM FIGHT CLUB');
  if (module) accept(simulation(module).state());
}

// ---- Network ------------------------------------------------------------------
function send(message) { if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message)); }
function connect() {
  if (authenticated) return Promise.resolve(); if (connecting) return connecting;
  connecting = new Promise((resolve, reject) => {
    const endpoint = new URL('ws', location.href); endpoint.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(endpoint); socket = ws;
    const deadline = setTimeout(() => { ws.close(); reject(Error('Сервер не отвечает. Тренировка доступна без сети.')); }, 8000);
    // Opened by a link (room, invite, revenge): an older page of the same
    // player gives up its room, this page goes where the link points.
    ws.onopen = () => { send({ type: 'auth', v: 3, initData: tg?.initData || '', resume: storage.get('pulse-session'), name: 'Игрок ' + Math.floor(Math.random() * 900 + 100), photo: window.arenaDevPhoto, leave: openedByLink }); openedByLink = false; };
    ws.onmessage = ({ data }) => {
      let m; try { m = JSON.parse(data); } catch { return; }
      if (m.type === 'welcome') {
        clearTimeout(deadline); authenticated = true; connecting = null; reconnectStarted = 0; stopped = false;
        storage.set('pulse-session', m.resume); text('identity', m.user.name.toUpperCase()); text('connection', 'АРЕНА НА СВЯЗИ');
        // Signed in: the game now serves the photo (the opponent sees it too).
        Object.assign(me, { id: m.user.id, name: m.user.name, avatar: m.user.avatar || null });
        showMe(); showMyCard(m.rating); resolve();
      } else if (m.type === 'queued') {
        screen('waiting'); text('waiting-title', 'Ищем соперника'); text('waiting-text', config.dev ? 'Ждём другого игрока. Открой вторую вкладку для проверки боя.' : 'Ждём другого игрока. Пока вспомни: захват проходит сквозь блок.'); show('room-share', false);
      } else if (m.type === 'room') {
        screen('waiting'); show('room-share');
        if (m.revenge) { text('waiting-title', 'Реванш!'); text('waiting-text', `Вызов отправлен: ${m.revenge.name}. Комната ждёт полчаса — можно ещё отправить ссылку.`); }
        else if (m.reopened) { text('waiting-title', 'Ты в комнате'); text('waiting-text', 'Соперника пока нет. Бой начнётся, как только он зайдёт по этой ссылке — её можно отправить ещё раз.'); }
        else { text('waiting-title', 'Вызов брошен'); text('waiting-text', 'Отправь ссылку другу. Бой начнётся, когда он присоединится. Комната ждёт полчаса.'); }
        const link = config.miniApp ? `${config.miniApp}?startapp=fight_${m.code}` : `${location.origin}${location.pathname}?room=${m.code}`;
        $('invite-link').value = link;
      } else if (m.type === 'match') {
        clearInput(); local = null; side = m.side; lastEvent = 0; rematchPossible = true; rematchRequested = false; netOffset = null;
        screen('online'); text('result-eyebrow', 'БОЙ ОКОНЧЕН'); text('mode', 'ОНЛАЙН · ВЫ ' + (side === 0 ? 'СЛЕВА' : 'СПРАВА'));
        m.players.forEach((p, i) => text('name-' + i, (i === side ? 'ВЫ · ' : '') + (p.league ? p.league.icon + ' ' : '') + p.name)); text('rematch', 'Реванш ↗'); $('rematch').disabled = false;
        showPeople(m.players.map((p, i) => ({ id: p.id, name: p.name, avatar: p.avatar || null, photo: i === side ? me.photo : null, league: p.league, rating: p.rating })));
        lastResult = null; renderResultRating(); show('challenge', false);
        const [left, right] = m.players.map(p => fighterById(p.fighter)); stage(left, right);
        versus([left, right]);
        const [mine, theirs] = side === 0 ? [left, m.players[1]] : [right, m.players[0]];
        setTimeout(() => toast(`Ты — ${mine.name}, ${side === 0 ? 'слева' : 'справа'}. Соперник: ${theirs.name} — ${fighterById(theirs.fighter).name}.`), 2400);
      } else if (m.type === 'result') {
        lastResult = m; showMyCard(m.you); renderResultRating();
      } else if (m.type === 'top') { renderLeaders(m); showMyCard(m.you.games ? m.you : null);
      } else if (m.type === 'challenge') { showChallenge(m);
      } else if (m.type === 'state' && mode === 'online') {
        const target = m.state.tick - performance.now() / STEP;
        netOffset = netOffset === null || Math.abs(target - netOffset) > 30 ? target : Math.max(target, netOffset - 0.03);
        accept(m.state, m.paused);
      } else if (m.type === 'pong') { const ping = Math.max(0, Date.now() - m.at); if (mode === 'online') text('connection', ping + ' MS' + (ping > 180 ? ' · ВЫСОКИЙ ПИНГ' : '')); }
      else if (m.type === 'rematch') { text('rematch', m.votes === 1 ? (rematchRequested ? 'Ждём согласия соперника…' : 'Принять реванш ↗') : 'Новый бой'); }
      else if (m.type === 'left') {
        if (mode === 'online') { rematchPossible = false; $('rematch').disabled = true; toast('Соперник покинул комнату.'); if (state?.phase === 3) text('result-eyebrow', 'СОПЕРНИК ВЫШЕЛ'); }
      } else if (m.type === 'expired') { toast('Время комнаты истекло. Создай новый бой.'); home(); }
      else if (m.type === 'error') {
        toast(m.message); if (!authenticated) { clearTimeout(deadline); reject(Error(m.message)); }
        if (mode === 'waiting') screen('lobby');
      }
    };
    ws.onerror = () => {};
    ws.onclose = event => {
      clearTimeout(deadline); if (socket !== ws) return;
      authenticated = false; connecting = null; reject(Error('Соединение с ареной потеряно.'));
      if (event.code === 4003 || event.code === 4002) { storage.set('pulse-session', ''); stopped = true; }
      // The game was opened again elsewhere (a link, another window or phone).
      if (event.code === 4002) { local = null; clearInput(); screen('lobby'); toast('Игра открыта в другом окне — продолжай там.', 7000); return; }
      if (mode === 'online' && !stopped) {
        reconnectStarted ||= Date.now(); text('connection', 'ВОССТАНАВЛИВАЕМ СВЯЗЬ'); clearInput();
        if (Date.now() - reconnectStarted < 16000) { clearTimeout(reconnectTimer); reconnectTimer = setTimeout(() => connect().catch(() => {}), 1000); }
        else { toast('Не удалось вернуться в бой. Можно начать новый матч.'); screen('lobby'); }
      } else if (mode === 'waiting') { screen('lobby'); toast('Связь прервалась. Поиск отменён.'); }
    };
  }); return connecting;
}
async function online(action) {
  if (!config.dev && !tg?.initData) { toast('Для сетевого боя открой Mini App в Telegram. Тренировка работает здесь.'); return; }
  screen('waiting'); show('room-share', false); text('waiting-title', 'Подключаемся'); text('waiting-text', 'Готовим арену…');
  try { await connect(); if (mode === 'waiting') send({ ...action, fighter: fighter.id }); } catch (e) { toast(e.message); if (mode === 'waiting') screen('lobby'); }
}

// ---- Input: keyboard, joystick, action pad ---------------------------------
function inputBits() {
  let value = 0; for (const mask of held.values()) value |= mask;
  const now = performance.now();
  for (const [bit, until] of latched) if (now < until) value |= bit; else latched.delete(bit);
  return value;
}
// A source (key, finger, stick) holds a mask; edge bits latch briefly so taps
// shorter than a simulation tick still register.
function setSource(source, mask, latch = 40) {
  if (!fighting()) return;
  const before = held.get(source) || 0;
  if (mask) held.set(source, mask); else held.delete(source);
  const now = performance.now();
  for (let bit = 1; bit <= 2048; bit <<= 1) if (mask & bit & EDGE && !(before & bit)) {
    latched.set(bit, Math.max(latched.get(bit) || 0, now + latch));
    pendingEdges |= bit | (mask & (BIT.LEFT | BIT.RIGHT | BIT.CROUCH | BIT.BLOCK));
  }
  bits = inputBits(); sendInput(); showPressed();
}
function showPressed() {
  if (bits === shownBits) return; shownBits = bits;
  for (const b of document.querySelectorAll('[data-bit]')) b.classList.toggle('pressed', !!(bits & Number(b.dataset.bit)));
}
function sendInput() { if (mode === 'online' && authenticated) { send({ type: 'input', seq: ++seq, bits }); lastSent = performance.now(); } }
function clearInput() { held.clear(); latched.clear(); bits = 0; pendingEdges = 0; sendInput(); showPressed(); resetStick(); }

// Floating joystick: the ring appears under the thumb anywhere in the left zone.
const stick = $('stick'), ring = stick.querySelector('.ring'), knob = stick.querySelector('.knob');
let stickId = null, origin = null, stickHoriz = 0, dashTap = { dir: 0, time: 0, released: true };
function ringRadius() { return ring.offsetWidth / 2 || 60; }
function placeRing(x, y) { const r = stick.getBoundingClientRect(); ring.style.left = (x - r.left) + 'px'; ring.style.top = (y - r.top) + 'px'; }
function resetStick() {
  stickId = null; origin = null; stick.classList.remove('active'); ring.style.left = ''; ring.style.top = '';
  knob.style.setProperty('--kx', '0px'); knob.style.setProperty('--ky', '0px');
  if (stickHoriz) dashTap.released = true; stickHoriz = 0;
}
function moveStick(x, y) {
  const R = ringRadius();
  let dx = x - origin.x, dy = y - origin.y, len = Math.hypot(dx, dy);
  if (len > R * 1.3) { const k = (len - R * 1.3) / len; origin.x += dx * k; origin.y += dy * k; placeRing(origin.x, origin.y); dx = x - origin.x; dy = y - origin.y; len = Math.hypot(dx, dy); }
  const clamp = Math.min(1, R * 0.62 / Math.max(len, 1));
  knob.style.setProperty('--kx', dx * clamp + 'px'); knob.style.setProperty('--ky', dy * clamp + 'px');
  let mask = 0;
  if (Math.abs(dx) > R * 0.3 && Math.abs(dx) > Math.abs(dy) * 0.42) mask |= dx < 0 ? BIT.LEFT : BIT.RIGHT;
  if (dy < -R * 0.5) mask |= BIT.JUMP; else if (dy > R * 0.45) mask |= BIT.CROUCH;
  const horiz = mask & 3, now = performance.now();
  if (horiz && horiz !== stickHoriz) {
    if (dashTap.dir === horiz && dashTap.released && now - dashTap.time < 300) { setSource('dash', BIT.DASH | horiz, 90); held.delete('dash'); haptic('medium'); dashTap.dir = 0; }
    else dashTap = { dir: horiz, time: now, released: false };
  }
  if (!horiz && stickHoriz) dashTap.released = true;
  if ((mask & BIT.JUMP) && !(held.get('stick') & BIT.JUMP)) haptic('light');
  stickHoriz = horiz;
  setSource('stick', mask);
}
stick.addEventListener('pointerdown', e => {
  if (stickId !== null || !fighting()) return;
  e.preventDefault(); stickId = e.pointerId; capture(stick, e.pointerId);
  const r = stick.getBoundingClientRect(), R = ringRadius();
  origin = { x: Math.min(Math.max(e.clientX, r.left + R * 0.8), r.right - R * 0.8), y: Math.min(Math.max(e.clientY, r.top + R * 0.8), r.bottom - R * 0.6) };
  placeRing(origin.x, origin.y); stick.classList.add('active'); moveStick(e.clientX, e.clientY);
});
stick.addEventListener('pointermove', e => { if (e.pointerId === stickId) moveStick(e.clientX, e.clientY); });
for (const type of ['pointerup', 'pointercancel', 'lostpointercapture'])
  stick.addEventListener(type, e => { if (e.pointerId !== stickId) return; resetStick(); setSource('stick', 0); });

// Action pad: each finger presses whatever button it is over, and can slide.
const pad = $('pad'), fingers = new Map();
function capture(el, id) { try { el.setPointerCapture(id); } catch {} }
function buttonAt(x, y) { return document.elementFromPoint(x, y)?.closest?.('#pad [data-bit]') || null; }
function press(id, button) {
  fingers.set(id, button);
  if (!button) { setSource('f' + id, 0); return; }
  let mask = Number(button.dataset.bit);
  if (mask === BIT.SPECIAL && breakerReady()) mask = BIT.BLOCK | BIT.DASH;
  setSource('f' + id, mask);
  if (mask !== BIT.BLOCK) haptic(mask & (BIT.HEAVY | BIT.SPECIAL) ? 'medium' : 'light');
}
pad.addEventListener('pointerdown', e => {
  e.preventDefault(); capture(pad, e.pointerId);
  press(e.pointerId, buttonAt(e.clientX, e.clientY) || e.target.closest?.('[data-bit]') || null);
});
pad.addEventListener('pointermove', e => {
  if (!fingers.has(e.pointerId)) return;
  const b = buttonAt(e.clientX, e.clientY);
  if (b && b !== fingers.get(e.pointerId)) press(e.pointerId, b);
});
for (const type of ['pointerup', 'pointercancel', 'lostpointercapture'])
  pad.addEventListener(type, e => { if (!fingers.has(e.pointerId)) return; fingers.delete(e.pointerId); setSource('f' + e.pointerId, 0); });
const smash = $('room-damage');
smash.addEventListener('pointerdown', e => { e.preventDefault(); capture(smash, e.pointerId); setSource('s' + e.pointerId, BIT.SMASH); haptic('medium'); });
for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) smash.addEventListener(type, e => setSource('s' + e.pointerId, 0));
for (const el of [stick, pad, smash]) el.oncontextmenu = e => e.preventDefault();

const keys = { KeyA: 1, ArrowLeft: 1, KeyD: 2, ArrowRight: 2, KeyS: 4, ArrowDown: 4, KeyJ: 8, KeyK: 16, Space: 32, KeyL: 64, KeyU: 128, KeyC: 256, KeyW: 512, ArrowUp: 512, KeyI: 1024, KeyQ: 2048 };
window.addEventListener('keydown', e => { if (keys[e.code] && !$('help').open && fighting()) { e.preventDefault(); if (!e.repeat) setSource(e.code, keys[e.code]); } });
window.addEventListener('keyup', e => { if (keys[e.code]) setSource(e.code, 0); });
window.addEventListener('blur', clearInput); document.addEventListener('visibilitychange', clearInput);

// ---- Buttons ---------------------------------------------------------------
$('practice').onclick = startPractice; $('quick').onclick = () => online({ type: 'queue' }); $('invite').onclick = () => online({ type: 'create' });
for (const id of ['cancel', 'leave', 'result-home']) $(id).onclick = home;
$('rematch').onclick = () => { if (mode === 'local') startPractice(); else { rematchRequested = true; send({ type: 'rematch' }); $('rematch').disabled = true; text('rematch', 'Ждём согласия соперника…'); } };
$('help-open').onclick = () => $('help').showModal(); $('help-close').onclick = () => $('help').close(); $('help-practice').onclick = () => { $('help').close(); startPractice(); };
$('my-rank').onclick = openLeaders; $('leaders-open').onclick = openLeaders; $('leaders-close').onclick = () => $('leaders').close();
$('brag').onclick = brag;
$('notify').onclick = () => {
  prefs.set('pulse-notify', '1'); show('notify', false);
  tg.requestWriteAccess(ok => { if (ok) { send({ type: 'dm' }); toast('Готово: бот позовёт тебя на реванш.'); } });
};
$('challenge-accept').onclick = () => { const code = $('challenge').dataset.code; show('challenge', false); if (mode === 'local') { local = null; } online({ type: 'join', code }); };
$('challenge-close').onclick = () => show('challenge', false);
$('sound').onclick = () => setSound(!sound); $('fight-sound').onclick = () => setSound(!sound);
$('fullscreen').onclick = toggleFullscreen;
$('copy').onclick = async () => { try { await navigator.clipboard.writeText($('invite-link').value); toast('Ссылка скопирована.'); } catch { $('invite-link').select(); toast('Выделена ссылка — скопируй её.'); } };
$('share').onclick = () => {
  const url = 'https://t.me/share/url?url=' + encodeURIComponent($('invite-link').value) + '&text=' + encodeURIComponent('Вызываю тебя на бой в PULSE ⚡');
  if (tg?.initData) tg.openTelegramLink(url); else window.open(url, '_blank', 'noopener,noreferrer');
};
$('training-mode').onclick = () => { trainingMode = (trainingMode + 1) % 4; text('training-mode', ['Бот: спарринг', 'Бот: манекен', 'Бот: верхний блок', 'Бот: нижний блок'][trainingMode]); };

// ---- Frame loop: local simulation at a fixed 60 Hz + adaptive resolution ----
let previous = performance.now(), accumulator = 0, slow = 0, frameAvg = STEP;
window.arenaMaxDpr = Math.min(window.devicePixelRatio || 1, 2);
function frame(now) {
  const dt = Math.min(now - previous, 100); previous = now;
  bits = inputBits(); showPressed();
  if (mode === 'local' && local && !document.hidden) {
    accumulator += dt;
    let steps = 0;
    // Every press reaches at least one simulation step, even on a slow frame.
    while (accumulator >= STEP && steps < 6) { local.step(bits | pendingEdges, trainingMode === 0 ? local.bot(1) : trainingMode === 2 ? 4 : trainingMode === 3 ? 260 : 0); accumulator -= STEP; steps++; pendingEdges = 0; }
    if (steps === 6) accumulator = 0;
    simTime = now - accumulator; accept(local.state());
  } else accumulator = 0;
  if (mode === 'online') {
    if (now - lastSent >= 33) sendInput();
    if (now - lastSnapshot > 1500 && state?.phase !== 3) { show('announcement'); text('announcement', 'ОЖИДАЕМ СВЯЗЬ…'); }
  }
  // Heavy frames on a phone: step the render resolution down, never below 1x.
  if (fighting() && !document.hidden) {
    frameAvg += (dt - frameAvg) * 0.05;
    slow = frameAvg > 25 ? slow + 1 : 0;
    if (slow > 150 && window.arenaMaxDpr > 1) { window.arenaMaxDpr = Math.max(1, window.arenaMaxDpr - 0.5); slow = 0; viewport(); }
  }
  if (now > eventUntil) text('combat-event', '');
  requestAnimationFrame(frame);
}
setInterval(() => { if (authenticated) send({ type: 'ping', at: Date.now() }); }, 2000);

try {
  if (tg) {
    tg.ready(); tg.expand(); if (tg.isVersionAtLeast?.('7.7')) tg.disableVerticalSwipes();
    tg.setHeaderColor?.('#080e18'); tg.setBackgroundColor?.('#080e18'); tg.BackButton?.onClick(home);
    for (const e of ['viewportChanged', 'safeAreaChanged', 'contentSafeAreaChanged', 'fullscreenChanged']) tg.onEvent?.(e, viewport);
    tg.onEvent?.('fullscreenFailed', () => toast('Полноэкранный режим недоступен в этой версии Telegram.'));
  }
  viewport(); window.addEventListener('resize', viewport); window.visualViewport?.addEventListener('resize', viewport);
  window.screen.orientation?.addEventListener?.('change', viewport); document.addEventListener('fullscreenchange', viewport);
  const [bytes, cfg] = await Promise.all([fetch('arena_combat.wasm').then(r => { if (!r.ok) throw Error('Нет боевого ядра. Запустите build-web.ps1.'); return r.arrayBuffer(); }), fetch('config.json').then(r => r.ok ? r.json() : config).catch(() => config)]);
  config = cfg; module = await WebAssembly.compile(bytes); accept(simulation(module).state());
  await loadRoster(); showMe(); showPeople([me, BOT]);
  // Fetch explicitly so missing WASM produces a readable loading error.
  const check = await fetch('some_game.wasm', { method: 'HEAD' }); if (!check.ok) throw Error('Нет 3D-сборки. Запустите build-web.ps1.');
  window.load('some_game.wasm');
  const started = performance.now(); await new Promise((resolve, reject) => {
    function ready() {
      // The apartment is waited for (up to 25 s) so the box room never flashes.
      const room = window.arenaRoomStatus || 0;
      if (window.wasm_exports && window.arenaModelStatus === 1 && (room !== 0 || performance.now() - started > 25000)) { viewport(); resolve(); }
      else if (window.arenaModelStatus === 1) { text('load-status', 'ОБСТАВЛЯЕМ ПЯТЬ КОМНАТ'); setTimeout(ready, 50); }
      else if (window.arenaModelStatus === -1) reject(Error('Не удалось загрузить персонажа. Обновите страницу, чтобы повторить.'));
      else if (performance.now() - started > 45000) reject(Error('Персонаж не загрузился. Проверьте сеть и обновите страницу.'));
      else { if (window.wasm_exports) text('load-status', 'ЗАГРУЖАЕМ ПЕРСОНАЖА · 4,2 МБ'); setTimeout(ready, 50); }
    } ready();
  });
  show('loader', false); for (const id of ['quick', 'invite', 'practice']) $(id).disabled = false;
  const name = tg?.initDataUnsafe?.user?.first_name; if (name) text('identity', name.toUpperCase());
  if (config.dev) text('connection', 'ЛОКАЛЬНАЯ АРЕНА');
  requestAnimationFrame(frame);
  const start = tg?.initDataUnsafe?.start_param || new URLSearchParams(location.search).get('tgWebAppStartParam');
  const room = new URLSearchParams(location.search).get('room') || (start?.startsWith('fight_') ? start.slice(6) : null);
  // The bot's «Вызвать друга» button opens the game with ?invite=1: a private
  // room is created right away and its link is ready to share.
  const inviting = new URLSearchParams(location.search).get('invite') === '1' || start === 'fight_invite';
  const revenge = new URLSearchParams(location.search).get('revenge');
  openedByLink = !!(room || revenge || inviting);
  if (room && /^[A-Za-z0-9_-]{12}$/.test(room)) online({ type: 'join', code: room });
  else if (revenge && /^[\w-]{1,40}$/.test(revenge)) online({ type: 'create', revenge });
  else if (inviting) online({ type: 'create' });
  // Inside Telegram the player signs in right away: their photo and name
  // are ready for the lobby and the «Ты» fighter before any fight.
  else if (storage.get('pulse-session') || (tg?.initData && !config.dev)) connect().catch(() => {});
} catch (error) { text('load-status', error.message); document.querySelector('.loading-line').style.display = 'none'; }
