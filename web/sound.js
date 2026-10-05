// Sound for the arena, synthesised with WebAudio: no audio files to load.
// Effects (hits, blocks, throws, breaking furniture, the round calls) are
// built from noise and oscillators per event; the music is a small step
// sequencer playing three tracks: a lo-fi groove for the lobby and two
// synthwave loops for fights. Modes: 'all', 'sfx' (no music), 'off'.
const AC = window.AudioContext || window.webkitAudioContext;
let ctx = null, master, sfxBus, musicBus, reverb, noise;
let mode = 'all', wanted = null, track = null, intensity = 0;

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
  sfxBus = ctx.createGain(); sfxBus.connect(master);
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
}
for (const e of ['pointerdown', 'keydown', 'touchend']) window.addEventListener(e, unlock, { capture: true, passive: true });
document.addEventListener('visibilitychange', () => {
  if (!ctx) return;
  if (document.hidden) ctx.suspend().catch(() => {}); else if (mode !== 'off') ctx.resume().catch(() => {});
});

function apply() {
  if (!ctx) return;
  const t = ctx.currentTime;
  sfxBus.gain.setTargetAtTime(mode === 'off' ? 0 : 0.85, t, 0.05);
  musicBus.gain.setTargetAtTime(mode === 'all' ? 0.2 : 0, t, 0.4);
  if (mode === 'off') ctx.suspend().catch(() => {}); else if (ctx.state !== 'running' && navigator.userActivation?.hasBeenActive !== false) ctx.resume().catch(() => {});
  if (mode === 'all' && wanted && !track) startTrack(wanted);
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
  const out = gain(sfxBus, 0.55 + weight * 0.45); send(out, 0.18 + weight * 0.12);
  const body = gain(out); env(body.gain, t, 1, 0.002, 0.12 + weight * 0.22);
  const o = osc('sine', 150 * pitch, t, t + 0.5, body);
  o.frequency.exponentialRampToValueAtTime(42 * pitch, t + 0.09 + weight * 0.12);
  const slap = gain(filter('bandpass', 1400 + 900 * (1 - weight), 0.8, out)); env(slap.gain, t, 0.9, 0.001, 0.035 + weight * 0.05);
  burst(t, 0.12, slap);
  if (weight > 0.5) {
    // Heavy: a crack on top and a dull floor rattle.
    const crack = gain(filter('highpass', 2500, 0.5, out)); env(crack.gain, t, 0.5, 0.001, 0.05);
    burst(t, 0.08, crack, 1.4);
    const rumble = gain(filter('lowpass', 220, 0.7, out)); env(rumble.gain, t + 0.01, 0.6 * weight, 0.01, 0.35);
    burst(t, 0.45, rumble, 0.5);
  }
}
function whoosh(t, weight) {
  const out = gain(sfxBus, 0.45 + weight * 0.4);
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
  const tick = gain(filter('highpass', 3000, 0.5, out)); env(tick.gain, t, 0.6, 0.001, 0.03); burst(t, 0.05, tick);
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
  const p = musicBus.gain; p.cancelScheduledValues(t); p.setValueAtTime(0.2 * depth, t); p.linearRampToValueAtTime(0.2, t + len);
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
// Instruments write into the music bus; `step` is a sixteenth note.
function kick(t, v = 1) {
  const g = gain(musicBus); env(g.gain, t, 1.0 * v, 0.002, 0.32);
  osc('sine', 140, t, t + 0.4, g).frequency.exponentialRampToValueAtTime(44, t + 0.11);
  const c = gain(filter('highpass', 3000, 0.5, musicBus)); env(c.gain, t, 0.25 * v, 0.001, 0.012); burst(t, 0.02, c);
}
function snare(t, v = 1, tone = 190) {
  const g = gain(filter('highpass', 1300, 0.6, musicBus)); env(g.gain, t, 0.55 * v, 0.001, 0.17); burst(t, 0.22, g);
  send(g, 0.25);
  const b = gain(musicBus); env(b.gain, t, 0.35 * v, 0.001, 0.08); osc('triangle', tone, t, t + 0.12, b);
}
function hat(t, v = 1, open = false) {
  const g = gain(filter('highpass', 7500, 0.7, musicBus)); env(g.gain, t, 0.18 * v, 0.001, open ? 0.18 : 0.035); burst(t, open ? 0.22 : 0.05, g, 1.3);
}
function bass(t, m, len, cutoff = 700, type = 'sawtooth') {
  const f = filter('lowpass', cutoff, 4, musicBus);
  f.frequency.setValueAtTime(cutoff * 2.2, t); f.frequency.exponentialRampToValueAtTime(cutoff, t + 0.12);
  const g = gain(f); env(g.gain, t, 0.42, 0.006, len);
  osc(type, hz(m), t, t + len + 0.05, g);
  const sub = gain(musicBus); env(sub.gain, t, 0.3, 0.006, len); osc('sine', hz(m - 12), t, t + len + 0.05, sub);
}
function pad(t, notes, len, bright = 1200, level = 0.08) {
  const f = filter('lowpass', bright, 0.5, musicBus); send(f, 0.5);
  const g = gain(f);
  g.gain.setValueAtTime(0.0001, t); g.gain.linearRampToValueAtTime(level, t + 0.35); g.gain.setValueAtTime(level, t + len - 0.2); g.gain.linearRampToValueAtTime(0.0001, t + len + 0.4);
  for (const m of notes) for (const d of [-7, 7]) { const o = osc('sawtooth', hz(m), t, t + len + 0.5, g); o.detune.value = d; }
}
function pluck(t, m, v = 1, type = 'square') {
  const f = filter('lowpass', 2400, 1, musicBus); send(f, 0.35);
  const g = gain(f); env(g.gain, t, 0.07 * v, 0.003, 0.22);
  osc(type, hz(m), t, t + 0.3, g);
}
function keys(t, notes, len, level = 0.07) {
  // Electric piano: sine + a touch of the octave, a soft tremolo.
  const f = filter('lowpass', 2200, 0.5, musicBus); send(f, 0.45);
  const g = gain(f); env(g.gain, t, level, 0.01, len);
  const trem = ctx.createOscillator(), depth = ctx.createGain(); trem.frequency.value = 4.5; depth.gain.value = level * 0.25;
  trem.connect(depth); depth.connect(g.gain); trem.start(t); trem.stop(t + len + 0.1);
  for (const m of notes) { osc('sine', hz(m), t, t + len, g); const h = gain(g, 0.18); osc('sine', hz(m + 12), t, t + len, h); }
}

// Tracks: tempo, bars of chords, and a function that schedules one step.
const TRACKS = {
  // Lobby: lo-fi rooftops at night. Fmaj7 Em7 Dm9 Cmaj7, swung hats.
  lobby: {
    bpm: 84, swing: 0.32,
    chords: [[53, 57, 60, 64], [52, 55, 59, 62], [50, 53, 57, 60, 64], [48, 52, 55, 59]],
    step(t, s, bar, chord, dur) {
      const root = chord[0] - 12;
      if (s === 0) keys(t, chord.map(m => m + 12), dur * 7);
      if (s === 6) keys(t, chord.slice(1).map(m => m + 12), dur * 5, 0.045);
      if (s === 0 || s === 10) kick(t, 0.7);
      if (s === 4 || s === 12) snare(t, 0.45, 240);
      if (s % 2 === 0) hat(t, s % 4 === 2 ? 0.7 : 0.45);
      if (s === 0) bass(t, root, dur * 6, 300, 'triangle');
      if (s === 10) bass(t, root + 7, dur * 4, 300, 'triangle');
      if (bar % 4 === 3 && s === 14) pluck(t, chord[3] + 12, 0.8, 'triangle');
    },
  },
  // Fight 1: night drive. Am F C G, octave bass, arp in the second half.
  night: {
    bpm: 112, swing: 0,
    chords: [[57, 60, 64], [53, 57, 60], [48, 52, 55], [55, 59, 62]],
    step(t, s, bar, chord, dur, heat) {
      const root = chord[0] - 24;
      if (s % 4 === 0) kick(t);
      if (s === 4 || s === 12) snare(t);
      if (s % 2 === 1 || heat > 0.5) hat(t, s % 2 ? 0.8 : 0.4, s % 4 === 2);
      if (s % 2 === 0) bass(t, root + (s % 4 === 2 ? 12 : 0), dur * 1.6, 500 + heat * 900);
      if (s === 0) pad(t, chord.map(m => m + 12), dur * 16, 900 + heat * 800);
      if (bar % 8 >= 4 || heat > 0.6) { const arp = [0, 1, 2, 1]; pluck(t, chord[arp[s % 4]] + 24, s % 4 === 0 ? 1 : 0.7); }
    },
  },
  // Fight 2: pulse. Dm Bb Gm A, syncopated bass, offbeat stabs.
  pulse: {
    bpm: 124, swing: 0,
    chords: [[50, 53, 57], [46, 50, 53], [43, 46, 50], [45, 49, 52]],
    step(t, s, bar, chord, dur, heat) {
      const root = chord[0] - 12;
      if (s % 4 === 0) kick(t);
      if (s === 14 && bar % 2) kick(t, 0.6);
      if (s === 4 || s === 12) { snare(t, 0.8, 210); hat(t, 0.5, true); }
      hat(t, s % 4 === 2 ? 0.9 : 0.35);
      if ([0, 3, 6, 8, 11, 14].includes(s)) bass(t, root + (s === 14 ? 7 : 0), dur * 1.2, 400 + heat * 1100);
      if (s % 4 === 2) { const f = filter('lowpass', 1600 + heat * 1500, 1, musicBus); const g = gain(f); env(g.gain, t, 0.05, 0.003, 0.14); for (const m of chord) osc('sawtooth', hz(m + 12), t, t + 0.2, g); }
      if (bar % 8 >= 4 && [0, 3, 6, 10].includes(s)) pluck(t, chord[[0, 2, 1, 2][[0, 3, 6, 10].indexOf(s)]] + 24, 0.9, 'triangle');
    },
  },
};
let timer = null, nextAt = 0, stepIndex = 0;
function startTrack(name) {
  stopTrack();
  if (!ctx || mode !== 'all') return;
  track = TRACKS[name]; nextAt = ctx.currentTime + 0.1; stepIndex = 0;
  timer = setInterval(schedule, 40); schedule();
}
function stopTrack() { clearInterval(timer); timer = null; track = null; }
function schedule() {
  if (!track || !ctx || ctx.state !== 'running') { if (ctx) nextAt = Math.max(nextAt, ctx.currentTime + 0.05); return; }
  const dur = 60 / track.bpm / 4;
  while (nextAt < ctx.currentTime + 0.18) {
    const s = stepIndex % 16, bar = Math.floor(stepIndex / 16);
    const chord = track.chords[bar % track.chords.length];
    const swing = s % 2 ? track.swing * dur : 0;
    try { track.step(nextAt + swing, s, bar, chord, dur, intensity); } catch {}
    nextAt += dur; stepIndex++;
  }
}
// Which track should play: 'lobby', 'night', 'pulse' or null for silence.
export function music(name) {
  if (name === wanted && (track || mode !== 'all')) return;
  wanted = name;
  if (!ctx) return;
  if (!name) { stopTrack(); return; }
  if (mode === 'all') {
    // A short fade between tracks.
    musicBus.gain.setTargetAtTime(0, ctx.currentTime, 0.12);
    setTimeout(() => { if (wanted === name) { startTrack(name); apply(); } }, 350);
  }
}
// 0..1: low health or the last seconds open the filters and add hats.
export function heat(value) { intensity = Math.max(0, Math.min(1, value)); }
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
    build(); mode = 'all'; sfxBus.gain.value = 0.85; musicBus.gain.value = 0.2;
    if (kind === 'music') {
      const t = TRACKS[name], dur = 60 / t.bpm / 4;
      for (let i = 0, at = 0.05; at < seconds; i++, at += dur) t.step(at + (i % 2 ? t.swing * dur : 0), i % 16, Math.floor(i / 16), t.chords[Math.floor(i / 16) % t.chords.length], dur, opts.heat || 0);
    } else effects[name](0.05, ...(opts.args || []));
    const data = (await ctx.startRendering()).getChannelData(0);
    let peak = 0, sum = 0; for (const v of data) { peak = Math.max(peak, Math.abs(v)); sum += v * v; }
    return { peak, rms: Math.sqrt(sum / data.length) };
  } finally { [ctx, master, sfxBus, musicBus, reverb, noise, mode] = saved; }
}
