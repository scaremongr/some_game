// Sound for the arena. Effects (hits, blocks, throws, breaking furniture,
// the round calls) are synthesised with WebAudio from noise and oscillators;
// the music is recorded tracks (assets/sound, Kevin MacLeod, CC BY 4.0, see
// CREDITS.md) streamed through the same mixer. Modes: 'all', 'sfx', 'off'.
const AC = window.AudioContext || window.webkitAudioContext;
let ctx = null, master, sfxBus, musicBus, reverb, noise;
let mode = 'all', wanted = null, track = null;
// One <audio> element for every track: iOS lets an element play later only
// if it was first started from a tap, so it is created in the first tap.
let player = null, playerSource = null;
const TRACK_FILES = { lobby: 'assets/sound/lobby.mp3', fight1: 'assets/sound/fight1.mp3', fight2: 'assets/sound/fight2.mp3', fight3: 'assets/sound/fight3.mp3' };
// Mix levels: effects stay present but never harsh; music under them.
const SFX = 0.5, MUSIC = 0.17;

function init() {
  if (ctx || !AC) return ctx;
  try { ctx = new AC({ latencyHint: 'interactive' }); } catch { ctx = new AC(); }
  build();
  apply();
  return ctx;
}
// The mixing desk on the current context: buses, compressor, room, noise.
function build() {
  const comp = ctx.createDynamicsCompressor();
  comp.threshold.value = -16; comp.ratio.value = 4; comp.attack.value = 0.004; comp.release.value = 0.2;
  master = ctx.createGain(); master.gain.value = 0.9;
  master.connect(comp); comp.connect(ctx.destination);
  // Effects pass a soft top cut: hits thud rather than crack.
  const soft = ctx.createBiquadFilter(); soft.type = 'lowpass'; soft.frequency.value = 4200; soft.Q.value = 0.5; soft.connect(master);
  sfxBus = ctx.createGain(); sfxBus.connect(soft);
  musicBus = ctx.createGain(); musicBus.gain.value = 0; musicBus.connect(master);
  // A short room for the hits and a longer tail for the pads.
  reverb = ctx.createConvolver();
  const len = Math.floor(ctx.sampleRate * 1.6), ir = ctx.createBuffer(2, len, ctx.sampleRate);
  for (let c = 0; c < 2; c++) { const d = ir.getChannelData(c); for (let i = 0; i < len; i++) d[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / len, 3.2); }
  reverb.buffer = ir;
  const wet = ctx.createGain(); wet.gain.value = 0.32; reverb.connect(wet); wet.connect(master);
  noise = ctx.createBuffer(1, ctx.sampleRate * 2, ctx.sampleRate);
  const n = noise.getChannelData(0); for (let i = 0; i < n.length; i++) n[i] = Math.random() * 2 - 1;
}
// Browsers start audio only from a user gesture: the first tap unlocks it.
function unlock() {
  if (mode === 'off') return;
  init(); if (!ctx) return;
  if (ctx.state !== 'running') ctx.resume().catch(() => {});
  const b = ctx.createBuffer(1, 1, 22050), s = ctx.createBufferSource(); s.buffer = b; s.connect(ctx.destination); s.start(0);
  if (!player) {
    try {
      player = new Audio(); player.loop = true; player.preload = 'auto';
      playerSource = ctx.createMediaElementSource(player); playerSource.connect(musicBus);
      // Blessed by this tap: later tracks may start without one.
      player.src = TRACK_FILES[wanted] || TRACK_FILES.lobby; track = wanted || 'lobby';
      if (mode === 'all') player.play().catch(() => {}); else player.load();
    } catch { player = null; }
  }
}
for (const e of ['pointerdown', 'keydown', 'touchend']) window.addEventListener(e, unlock, { capture: true, passive: true });
document.addEventListener('visibilitychange', () => {
  if (!ctx) return;
  if (document.hidden) { ctx.suspend().catch(() => {}); player?.pause(); }
  else if (mode !== 'off') { ctx.resume().catch(() => {}); if (mode === 'all' && track) player?.play().catch(() => {}); }
});

function apply() {
  if (!ctx) return;
  const t = ctx.currentTime;
  sfxBus.gain.setTargetAtTime(mode === 'off' ? 0 : SFX, t, 0.05);
  musicBus.gain.setTargetAtTime(mode === 'all' ? MUSIC : 0, t, 0.4);
  if (mode === 'off') ctx.suspend().catch(() => {}); else if (ctx.state !== 'running' && navigator.userActivation?.hasBeenActive !== false) ctx.resume().catch(() => {});
  if (mode === 'all' && wanted && (track !== wanted || player?.paused)) startTrack(wanted);
}

// ---- Building blocks --------------------------------------------------------------
const hz = m => 440 * Math.pow(2, (m - 69) / 12);
function env(param, t, peak, attack, decay, floor = 0.0001) {
  param.cancelScheduledValues(t); param.setValueAtTime(floor, t);
  param.linearRampToValueAtTime(peak, t + attack);
  param.exponentialRampToValueAtTime(floor, t + attack + decay);
}
function osc(type, freq, t, end, out) { const o = ctx.createOscillator(); o.type = type; o.frequency.setValueAtTime(freq, t); o.connect(out); o.start(t); o.stop(end + 0.05); return o; }
function gain(out, value = 1) { const g = ctx.createGain(); g.gain.value = value; g.connect(out); return g; }
function filter(type, freq, q, out) { const f = ctx.createBiquadFilter(); f.type = type; f.frequency.value = freq; f.Q.value = q; f.connect(out); return f; }
function burst(t, dur, out, rate = 1) {
  const s = ctx.createBufferSource(); s.buffer = noise; s.playbackRate.value = rate;
  s.connect(out); s.start(t, Math.random() * 1.5, dur + 0.05); return s;
}
function send(node, amount) { const g = ctx.createGain(); g.gain.value = amount; node.connect(g); g.connect(reverb); }

// ---- Effects ---------------------------------------------------------------
// A body blow: a falling low thump, a slap of filtered noise and a short tail.
function blow(t, weight, pitch = 1) {
  // A muffled body blow: a soft-attack low thump and a short dull smack.
  const out = gain(sfxBus, 0.4 + weight * 0.35); send(out, 0.12 + weight * 0.1);
  const body = gain(out); env(body.gain, t, 1, 0.006, 0.1 + weight * 0.18);
  const o = osc('sine', 120 * pitch, t, t + 0.5, body);
  o.frequency.exponentialRampToValueAtTime(46 * pitch, t + 0.08 + weight * 0.1);
  const smack = gain(filter('bandpass', 700 + 300 * (1 - weight), 0.7, out)); env(smack.gain, t, 0.55, 0.004, 0.04 + weight * 0.04);
  burst(t, 0.12, smack, 0.8);
  if (weight > 0.5) {
    const rumble = gain(filter('lowpass', 200, 0.7, out)); env(rumble.gain, t + 0.01, 0.45 * weight, 0.015, 0.3);
    burst(t, 0.4, rumble, 0.5);
  }
}
function whoosh(t, weight) {
  const out = gain(sfxBus, 0.3 + weight * 0.25);
  const f = filter('bandpass', 500, 1.6, out);
  f.frequency.setValueAtTime(380 + weight * 100, t); f.frequency.exponentialRampToValueAtTime(1500 - weight * 400, t + 0.16 + weight * 0.1);
  const g = gain(f); env(g.gain, t, 1, 0.05 + weight * 0.04, 0.12 + weight * 0.1);
  burst(t, 0.35, g);
}
function block(t) {
  const out = gain(sfxBus, 0.55); send(out, 0.15);
  const knock = gain(out); env(knock.gain, t, 0.8, 0.002, 0.09);
  osc('triangle', 210, t, t + 0.15, knock).frequency.exponentialRampToValueAtTime(120, t + 0.08);
  const pad = gain(filter('bandpass', 650, 1.2, out)); env(pad.gain, t, 0.8, 0.001, 0.07);
  burst(t, 0.1, pad);
}
function parry(t) {
  const out = gain(sfxBus, 0.32); send(out, 0.5);
  for (const [f, a] of [[1180, 1], [1867, 0.6], [2643, 0.45], [3912, 0.25]]) {
    const g = gain(out); env(g.gain, t, a, 0.002, 0.55); osc('sine', f, t, t + 0.7, g);
  }
  const tick = gain(filter('bandpass', 2400, 0.8, out)); env(tick.gain, t, 0.3, 0.003, 0.03); burst(t, 0.05, tick);
}
function shatterGuard(t) {
  blow(t, 0.7, 1.2);
  const out = gain(sfxBus, 0.35); send(out, 0.3);
  const g = gain(out); env(g.gain, t, 0.6, 0.002, 0.4);
  const o = osc('sawtooth', 700, t, t + 0.5, filter('lowpass', 1800, 2, g)); o.frequency.exponentialRampToValueAtTime(140, t + 0.4);
}
function slam(t) {
  const out = gain(sfxBus, 1.0); send(out, 0.3);
  const boom = gain(out); env(boom.gain, t, 1, 0.003, 0.5);
  osc('sine', 95, t, t + 0.7, boom).frequency.exponentialRampToValueAtTime(30, t + 0.35);
  const thud = gain(filter('lowpass', 600, 0.8, out)); env(thud.gain, t, 0.9, 0.002, 0.25); burst(t, 0.3, thud, 0.7);
  const rattle = gain(filter('bandpass', 2600, 1.5, out)); env(rattle.gain, t + 0.03, 0.25, 0.01, 0.3); burst(t + 0.03, 0.35, rattle);
}
function grab(t) {
  const out = gain(sfxBus, 0.9);
  const g = gain(filter('bandpass', 900, 1, out)); env(g.gain, t, 0.8, 0.005, 0.12); burst(t, 0.15, g, 0.8);
}
function crash(t, glass) {
  const out = gain(sfxBus, 0.8); send(out, 0.25);
  for (let i = 0; i < 6; i++) {
    const at = t + Math.random() * 0.12, g = gain(filter('bandpass', 1500 + Math.random() * 2000, 2, out));
    env(g.gain, at, 0.7, 0.001, 0.04 + Math.random() * 0.05); burst(at, 0.1, g);
  }
  const body = gain(filter('lowpass', 500, 0.7, out)); env(body.gain, t, 0.7, 0.003, 0.25); burst(t, 0.3, body, 0.6);
  if (glass) for (let i = 0; i < 9; i++) {
    const at = t + 0.02 + Math.random() * 0.35, g = gain(out); env(g.gain, at, 0.12, 0.001, 0.15 + Math.random() * 0.2);
    osc('sine', 2600 + Math.random() * 4200, at, at + 0.4, g);
  }
}
function knockout(t) {
  blow(t, 1, 0.8);
  const out = gain(sfxBus, 0.8); send(out, 0.6);
  const boom = gain(out); env(boom.gain, t + 0.02, 1, 0.01, 1.4);
  osc('sine', 60, t, t + 1.6, boom).frequency.exponentialRampToValueAtTime(28, t + 1.2);
  duck(t, 0.15, 1.6);
}
function beep(t, high) {
  const out = gain(sfxBus, 0.25);
  const g = gain(out); env(g.gain, t, 1, 0.005, high ? 0.5 : 0.18);
  osc('triangle', high ? 1320 : 880, t, t + 0.6, g);
}
// The bell between rounds and at the start of a fight.
function bell(t, times = 1) {
  const out = gain(sfxBus, 0.32); send(out, 0.45);
  for (let k = 0; k < times; k++) {
    const at = t + k * 0.28;
    for (const [f, a, d] of [[523, 1, 1.4], [1047, 0.5, 1.0], [1568, 0.3, 0.7], [2794, 0.2, 0.4]]) {
      const g = gain(out); env(g.gain, at, a, 0.002, d); osc('sine', f, at, at + d + 0.1, g);
    }
  }
}
function jingle(t, won) {
  const out = gain(sfxBus, 0.28); send(out, 0.4);
  const notes = won ? [60, 64, 67, 72, 76, 79, 84] : [64, 60, 57, 52];
  notes.forEach((m, i) => {
    const at = t + i * (won ? 0.09 : 0.2), g = gain(filter('lowpass', 3000, 0.7, out));
    env(g.gain, at, 0.7, 0.005, won && i === notes.length - 1 ? 1.2 : 0.35);
    osc(won ? 'square' : 'triangle', hz(m), at, at + 1.4, g);
  });
}
function click(t) { const g = gain(sfxBus); env(g.gain, t, 0.06, 0.001, 0.03); osc('square', 1800, t, t + 0.05, g); }
// Music steps back for a moment under a big blow.
function duck(t, depth, len) {
  if (!musicBus || mode !== 'all') return;
  const p = musicBus.gain; p.cancelScheduledValues(t); p.setValueAtTime(MUSIC * depth, t); p.linearRampToValueAtTime(MUSIC, t + len);
}

const effects = { blow, whoosh, block, parry, shatterGuard, slam, grab, crash, knockout, beep, bell, jingle, click };
export function play(name, opts = {}) {
  if (mode === 'off' || !init() || ctx.state !== 'running') return;
  const f = effects[name]; if (!f) return;
  // A little ahead of the clock, so envelopes are never scheduled in the past.
  const t = ctx.currentTime + 0.02 + (opts.delay || 0);
  try {
    if (name === 'blow') f(t, opts.weight ?? 0.4, 0.9 + Math.random() * 0.2);
    else if (name === 'whoosh') f(t, opts.weight ?? 0.3);
    else if (name === 'crash') f(t, !!opts.glass);
    else if (name === 'beep') f(t, !!opts.high);
    else if (name === 'bell') f(t, opts.times || 1);
    else if (name === 'jingle') f(t, !!opts.won);
    else f(t);
    if (name === 'blow' && (opts.weight ?? 0) > 0.6) duck(t, 0.45, 0.4);
  } catch {}
}

// ---- Music ------------------------------------------------------------------
function startTrack(name) {
  if (!ctx || !player || mode !== 'all' || !TRACK_FILES[name]) return;
  const src = new URL(TRACK_FILES[name], location.href).href;
  if (player.src !== src) player.src = src;
  track = name;
  // Fade in from silence.
  const t = ctx.currentTime;
  musicBus.gain.cancelScheduledValues(t); musicBus.gain.setValueAtTime(0, t); musicBus.gain.linearRampToValueAtTime(MUSIC, t + 1.2);
  player.play().catch(() => {});
}
function stopTrack() { player?.pause(); track = null; }
// Which track should play: 'lobby', 'fight1'..'fight3' or null for silence.
export function music(name) {
  if (name === wanted && (track === name || mode !== 'all')) return;
  wanted = name;
  if (!ctx || !player) return;
  if (!name) { stopTrack(); return; }
  if (mode === 'all') {
    // A short fade out, then the next track from its start.
    musicBus.gain.setTargetAtTime(0, ctx.currentTime, 0.15);
    setTimeout(() => { if (wanted === name) { if (player.src.endsWith(TRACK_FILES[name])) player.currentTime = 0; startTrack(name); } }, 500);
  }
}
// Kept for callers: recorded tracks carry their own intensity.
export function heat() {}
export function setMode(next) {
  mode = next;
  if (next === 'off') stopTrack();
  if (next !== 'off') unlock();
  apply();
  if (next !== 'all') stopTrack();
}
export function getMode() { return mode; }
// Tests: renders an effect or a stretch of a track offline and returns the
// mix's peak and RMS (deterministic, unlike sampling the live output).
export async function measure(kind, name, opts = {}, seconds = 2) {
  const Offline = window.OfflineAudioContext || window.webkitOfflineAudioContext;
  const saved = [ctx, master, sfxBus, musicBus, reverb, noise, mode];
  ctx = new Offline(1, Math.ceil(44100 * seconds), 44100);
  try {
    build(); mode = 'all'; sfxBus.gain.value = SFX; musicBus.gain.value = MUSIC;
    if (kind === 'music') {
      // The track file through the music bus (first `seconds` of it).
      const buffer = await ctx.decodeAudioData(await (await fetch(TRACK_FILES[name])).arrayBuffer());
      const s = ctx.createBufferSource(); s.buffer = buffer; s.connect(musicBus); s.start(0);
    } else effects[name](0.05, ...(opts.args || []));
    const data = (await ctx.startRendering()).getChannelData(0);
    let peak = 0, sum = 0; for (const v of data) { peak = Math.max(peak, Math.abs(v)); sum += v * v; }
    return { peak, rms: Math.sqrt(sum / data.length) };
  } finally { [ctx, master, sfxBus, musicBus, reverb, noise, mode] = saved; }
}
// Tests: which track the player holds and whether it advances.
export function playing() { return player ? { track, src: player.src, time: player.currentTime, paused: player.paused } : null; }
