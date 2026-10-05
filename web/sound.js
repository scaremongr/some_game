// Sound for the arena, synthesised with WebAudio: no audio files to load.
// Effects (hits, blocks, throws, breaking furniture, the round calls) are
// built from noise and oscillators per event; the music is a small step
// sequencer playing three tracks: a lo-fi groove for the lobby and two
// synthwave loops for fights. Modes: 'all', 'sfx' (no music), 'off'.
const AC = window.AudioContext || window.webkitAudioContext;
let ctx = null, master, sfxBus, musicBus, reverb, noise;
let mode = 'all', wanted = null, track = null, intensity = 0;
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
}
for (const e of ['pointerdown', 'keydown', 'touchend']) window.addEventListener(e, unlock, { capture: true, passive: true });
document.addEventListener('visibilitychange', () => {
  if (!ctx) return;
  if (document.hidden) ctx.suspend().catch(() => {}); else if (mode !== 'off') ctx.resume().catch(() => {});
});

function apply() {
  if (!ctx) return;
  const t = ctx.currentTime;
  sfxBus.gain.setTargetAtTime(mode === 'off' ? 0 : SFX, t, 0.05);
  musicBus.gain.setTargetAtTime(mode === 'all' ? MUSIC : 0, t, 0.4);
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

function stab(t, notes, len, cutoff = 2600, level = 0.06) {
  // A bright chord hit: two detuned saws per note through a closing filter.
  const f = filter('lowpass', cutoff, 1.5, musicBus); f.frequency.setValueAtTime(cutoff * 1.6, t); f.frequency.exponentialRampToValueAtTime(cutoff * 0.5, t + len);
  send(f, 0.2);
  const g = gain(f); env(g.gain, t, level, 0.004, len);
  for (const m of notes) for (const d of [-9, 9]) { const o = osc('sawtooth', hz(m), t, t + len + 0.05, g); o.detune.value = d; }
}
function lead(t, m, len, level = 0.06) {
  // A square lead with a little vibrato and echo.
  const f = filter('lowpass', 3200, 0.8, musicBus); send(f, 0.3);
  const g = gain(f); g.gain.setValueAtTime(0.0001, t); g.gain.linearRampToValueAtTime(level, t + 0.01); g.gain.setValueAtTime(level * 0.8, t + len * 0.7); g.gain.exponentialRampToValueAtTime(0.0001, t + len);
  const o = osc('square', hz(m), t, t + len, g);
  const vib = ctx.createOscillator(), depth = ctx.createGain(); vib.frequency.value = 6; depth.gain.value = 8;
  vib.connect(depth); depth.connect(o.detune); vib.start(t + 0.08); vib.stop(t + len);
}
function clap(t, v = 1) {
  const g = gain(filter('bandpass', 1500, 0.9, musicBus)); send(g, 0.3);
  for (const d of [0, 0.011, 0.022]) { const e = gain(g); env(e.gain, t + d, 0.5 * v, 0.001, d === 0.022 ? 0.14 : 0.01); burst(t + d, 0.16, e); }
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
  // Fight 1: arcade rock. A G D A, driving bass, power stabs, a riff.
  arcade: {
    bpm: 138, swing: 0,
    chords: [[45, 52, 57], [43, 50, 55], [38, 45, 50], [45, 52, 57]],
    riff: [{ 0: 76, 2: 78, 4: 81, 7: 78, 8: 76, 10: 73, 12: 76, 14: 74 }, { 0: 73, 2: 71, 4: 69, 8: 71, 10: 73, 12: 76 }],
    step(t, s, bar, chord, dur, heat) {
      const root = chord[0];
      if (s % 4 === 0 || (s === 10 && bar % 2)) kick(t);
      if (s === 4 || s === 12) { snare(t, 1, 200); clap(t, 0.6); }
      hat(t, s % 4 === 2 ? 0.9 : 0.4, s % 4 === 2);
      if (heat > 0.5 && s % 2) hat(t, 0.35);
      if (s % 2 === 0) bass(t, root - 12 + (s === 14 ? 12 : 0), dur * 1.5, 900 + heat * 900, 'sawtooth');
      if (s === 0) stab(t, [root + 12, root + 19, root + 24], dur * 3, 2400);
      if (s === 6 || s === 11) stab(t, [root + 12, root + 19, root + 24], dur * 1.5, 2400, 0.045);
      const n = this.riff[bar % 2][s];
      if (n && (bar % 8 >= 4 || heat > 0.6)) lead(t, n, dur * 1.8);
    },
  },
  // Fight 2: electro-funk brawl. Em7 A9 in dorian, slap bass, offbeat stabs.
  brawl: {
    bpm: 128, swing: 0.08,
    chords: [[52, 55, 59, 62], [57, 61, 64, 66], [52, 55, 59, 62], [57, 61, 64, 67]],
    step(t, s, bar, chord, dur, heat) {
      const root = chord[0] - 12;
      if (s % 4 === 0) kick(t);
      if (s === 4 || s === 12) clap(t);
      hat(t, s % 2 ? 0.5 : 0.8, s % 4 === 2);
      const line = { 0: 0, 3: 12, 6: 0, 7: 10, 10: 0, 12: 12, 14: 7 };
      if (s in line) bass(t, root - 12 + line[s], dur * (s === 0 ? 2 : 1), 1100 + heat * 1000, 'square');
      if (s % 4 === 2) stab(t, chord.map(m => m + 12), dur * 0.9, 3200, 0.05);
      if (bar % 8 >= 4 || heat > 0.6) { const arp = [0, 2, 3, 1]; pluck(t, chord[arp[s % 4]] + 24, s % 4 === 0 ? 1 : 0.6); }
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
// Which track should play: 'lobby', 'arcade', 'brawl' or null for silence.
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
    build(); mode = 'all'; sfxBus.gain.value = SFX; musicBus.gain.value = MUSIC;
    if (kind === 'music') {
      const t = TRACKS[name], dur = 60 / t.bpm / 4;
      for (let i = 0, at = 0.05; at < seconds; i++, at += dur) t.step(at + (i % 2 ? t.swing * dur : 0), i % 16, Math.floor(i / 16), t.chords[Math.floor(i / 16) % t.chords.length], dur, opts.heat || 0);
    } else effects[name](0.05, ...(opts.args || []));
    const data = (await ctx.startRendering()).getChannelData(0);
    let peak = 0, sum = 0; for (const v of data) { peak = Math.max(peak, Math.abs(v)); sum += v * v; }
    return { peak, rms: Math.sqrt(sum / data.length) };
  } finally { [ctx, master, sfxBus, musicBus, reverb, noise, mode] = saved; }
}
