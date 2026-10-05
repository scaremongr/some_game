// Pictures for the room, handed to the renderer as RGBA pixels
// (window.arenaImages, read by bridge.js → src/game/scenery.rs):
//   0, 1  the night city behind the windows (far layer, near rooftops)
//   2     living-room TV: the fight "on air", both players
//   3     study laptop: the player's fighter profile
//   4, 5  bedroom canvases: the player's and the opponent's portraits
// Photos come only from this server (avatar/<id>.jpg) or the fighter
// portraits: a picture from another site would lock the canvas pixels.
const images = window.arenaImages = [];
let version = 0;
function put(slot, canvas) {
  try {
    const g = canvas.getContext('2d');
    const { data } = g.getImageData(0, 0, canvas.width, canvas.height);
    images[slot] = { version: ++version, width: canvas.width, height: canvas.height, data: new Uint8Array(data.buffer) };
  } catch {}
}
const loaded = new Map();
function load(src) {
  if (!src || /^(data:|https?:)/.test(src)) return Promise.resolve(null);
  if (!loaded.has(src)) loaded.set(src, new Promise(res => { const i = new Image(); i.onload = () => res(i); i.onerror = () => res(null); i.src = src; }));
  return loaded.get(src);
}
function canvas(w, h) { const c = document.createElement('canvas'); c.width = w; c.height = h; return c; }
function draw(img) { const c = canvas(img.naturalWidth, img.naturalHeight); c.getContext('2d').drawImage(img, 0, 0); return c; }

// ---- The city ---------------------------------------------------------------
export async function loadCity() {
  const [far, near, detail] = await Promise.all([load('assets/backdrop/far.jpg'), load('assets/backdrop/near.png'), load('assets/room_detail.jpg')]);
  if (near) put(1, draw(near));
  if (far) put(0, draw(far));
  // Fine grain of floors, rugs and walls (slot 6).
  if (detail) put(6, draw(detail));
}

// ---- People's pictures --------------------------------------------------------
const font = (w, s) => `${w} ${s}px Inter, "Segoe UI", Roboto, Arial, sans-serif`;
const hues = [168, 12, 42, 210, 280, 330, 95];
function hue(seed) { return hues[[...String(seed || '?')].reduce((a, c) => a + c.charCodeAt(0), 0) % hues.length]; }
function cover(g, img, x, y, w, h) {
  const k = Math.max(w / img.width, h / img.height), sw = w / k, sh = h / k;
  g.drawImage(img, (img.width - sw) / 2, (img.height - sh) / 2, sw, sh, x, y, w, h);
}
// A face: the profile photo, or something neutral — a robot for the bot,
// a quiet silhouette for a person whose photo Telegram does not give us.
function face(g, p, img, x, y, w, h) {
  if (img) { cover(g, img, x, y, w, h); return; }
  const grad = g.createLinearGradient(x, y, x, y + h);
  grad.addColorStop(0, '#2a3a48'); grad.addColorStop(1, '#141d26');
  g.fillStyle = grad; g.fillRect(x, y, w, h);
  const cx = x + w / 2, u = Math.min(w, h);
  if (p?.bot) {
    g.fillStyle = '#ff7965'; g.beginPath(); g.roundRect ? g.roundRect(cx - u * 0.26, y + h * 0.32, u * 0.52, u * 0.4, u * 0.1) : g.rect(cx - u * 0.26, y + h * 0.32, u * 0.52, u * 0.4); g.fill();
    g.fillStyle = '#141d26'; for (const dx of [-0.1, 0.1]) { g.beginPath(); g.arc(cx + dx * u, y + h * 0.32 + u * 0.19, u * 0.065, 0, Math.PI * 2); g.fill(); }
    g.fillStyle = '#ff7965'; g.fillRect(cx - u * 0.03, y + h * 0.32 - u * 0.12, u * 0.06, u * 0.12);
    g.fillStyle = '#e9bc73'; g.beginPath(); g.arc(cx, y + h * 0.32 - u * 0.13, u * 0.05, 0, Math.PI * 2); g.fill();
    return;
  }
  g.fillStyle = '#6f8494';
  g.beginPath(); g.arc(cx, y + h * 0.4, u * 0.18, 0, Math.PI * 2); g.fill();
  g.beginPath(); g.ellipse(cx, y + h * 0.4 + u * 0.5, u * 0.34, u * 0.27, 0, Math.PI, 0); g.fill();
}
// Neutral art for a canvas with no photo: a slice of the night city.
let city = null;
function scenery(g, W, H) {
  if (!city) { g.fillStyle = '#1a2230'; g.fillRect(0, 0, W, H); return; }
  const sw = city.width * 0.22, sh = Math.min(city.height, sw * H / W);
  g.drawImage(city, city.width * 0.36, (city.height - sh) * 0.45, sw, sh, 0, 0, W, H);
}
function disc(g, p, img, cx, cy, r, ring) {
  g.save(); g.beginPath(); g.arc(cx, cy, r, 0, Math.PI * 2); g.clip(); face(g, p, img, cx - r, cy - r, r * 2, r * 2); g.restore();
  g.lineWidth = r * 0.09; g.strokeStyle = ring; g.beginPath(); g.arc(cx, cy, r, 0, Math.PI * 2); g.stroke();
}
function fit(g, s, max) { let t = s || ''; while (g.measureText(t).width > max && t.length > 2) t = t.slice(0, -2) + '…'; return t; }

// The TV: an old set showing the fight live (or the lobby's idle screen).
function tv(people, fighters, side, live) {
  const W = 512, H = 384, c = canvas(W, H), g = c.getContext('2d');
  const [a, b] = people, [pa, pb] = fighters;
  const bg = g.createRadialGradient(W / 2, H * 0.45, 20, W / 2, H / 2, W * 0.7);
  bg.addColorStop(0, '#1d3a52'); bg.addColorStop(1, '#06101a');
  g.fillStyle = bg; g.fillRect(0, 0, W, H);
  g.fillStyle = '#57f0ce'; g.font = font(900, 26); g.fillText('ϟ PULSE TV', 22, 40);
  if (live) {
    g.fillStyle = '#ff3b30'; g.beginPath(); g.arc(W - 96, 32, 8, 0, Math.PI * 2); g.fill();
    g.fillStyle = '#fff'; g.font = font(800, 20); g.fillText('ЭФИР', W - 82, 39);
    disc(g, a, pa, 132, 182, 74, '#57f0ce'); disc(g, b, pb, W - 132, 182, 74, '#ff7965');
    g.fillStyle = '#e9bc73'; g.font = font(900, 54); g.textAlign = 'center'; g.fillText('VS', W / 2, 202);
    g.font = font(800, 22); g.fillStyle = '#fff';
    g.fillText(fit(g, a?.name, 200), 132, 290); g.fillText(fit(g, b?.name, 200), W - 132, 290);
    g.fillStyle = '#0b1520cc'; g.fillRect(0, H - 54, W, 54);
    g.fillStyle = '#e9bc73'; g.font = font(800, 18); g.fillText('ПРЯМАЯ ТРАНСЛЯЦИЯ ИЗ КВАРТИРЫ', W / 2, H - 21);
    g.textAlign = 'left';
  } else {
    disc(g, a, pa, W / 2, 178, 86, '#57f0ce');
    g.textAlign = 'center'; g.fillStyle = '#fff'; g.font = font(800, 26); g.fillText(fit(g, a?.name, 420), W / 2, 302);
    g.fillStyle = '#9fb3bd'; g.font = font(700, 18); g.fillText('ЖДЁМ ТВОЙ СЛЕДУЮЩИЙ БОЙ', W / 2, 336); g.textAlign = 'left';
  }
  // Tube: scanlines, a soft vignette and rounded glass corners.
  g.fillStyle = '#00000038'; for (let y = 0; y < H; y += 3) g.fillRect(0, y, W, 1);
  const v = g.createRadialGradient(W / 2, H / 2, H * 0.35, W / 2, H / 2, W * 0.65);
  v.addColorStop(0, '#0000'); v.addColorStop(1, '#000a'); g.fillStyle = v; g.fillRect(0, 0, W, H);
  round(g, W, H, 46);
  return c;
}
// The laptop: the player's fighter profile in the game's colours.
function laptop(me, mine, fighter) {
  const W = 400, H = 324, c = canvas(W, H), g = c.getContext('2d');
  g.fillStyle = '#0b1520'; g.fillRect(0, 0, W, H);
  g.fillStyle = '#13212c'; g.fillRect(0, 0, W, 30);
  for (const [i, col] of ['#ff5f57', '#febc2e', '#28c840'].entries()) { g.fillStyle = col; g.beginPath(); g.arc(16 + i * 18, 15, 5, 0, Math.PI * 2); g.fill(); }
  g.fillStyle = '#80949f'; g.font = font(700, 13); g.fillText('pulse · профиль бойца', 80, 20);
  disc(g, me, mine, 92, 130, 58, '#57f0ce');
  g.fillStyle = '#eef5f3'; g.font = font(800, 24); g.fillText(fit(g, me?.name, 210), 170, 112);
  g.fillStyle = '#e9bc73'; g.font = font(700, 17);
  g.fillText(me?.league ? `${me.league.icon} ${me.league.name} · ${me.rating}` : 'Рейтинг — в сетевых боях', 170, 142);
  g.fillStyle = '#80949f'; g.font = font(600, 15); g.fillText('Боец: ' + (fighter?.name || '—'), 170, 168);
  g.fillStyle = '#57f0ce'; g.fillRect(24, 214, W - 48, 44);
  g.fillStyle = '#062922'; g.font = font(800, 18); g.fillText('Готов к бою  ↗', 42, 243);
  g.fillStyle = '#4b5d68'; g.font = font(600, 12); g.fillText('Решает тайминг. Побеждает тот, кто читает бой.', 26, 290);
  return c;
}
// A canvas on the wall: the photo as an old warm print with a vignette.
function portrait(p, img, W, H) {
  const c = canvas(W, H), g = c.getContext('2d');
  if (img) cover(g, img, 0, 0, W, H); else scenery(g, W, H);
  g.globalCompositeOperation = 'color'; g.fillStyle = '#7a5a3a66'; g.fillRect(0, 0, W, H);
  g.globalCompositeOperation = 'multiply'; g.fillStyle = '#f2ddb8'; g.fillRect(0, 0, W, H);
  g.globalCompositeOperation = 'source-over';
  const v = g.createRadialGradient(W / 2, H / 2, Math.min(W, H) * 0.3, W / 2, H / 2, Math.max(W, H) * 0.75);
  v.addColorStop(0, '#0000'); v.addColorStop(1, '#1a0f06cc'); g.fillStyle = v; g.fillRect(0, 0, W, H);
  // Canvas weave.
  g.fillStyle = '#ffffff0c'; for (let x = 0; x < W; x += 3) g.fillRect(x, 0, 1, H);
  g.fillStyle = '#0000000c'; for (let y = 0; y < H; y += 3) g.fillRect(0, y, W, 1);
  return c;
}
function round(g, W, H, r) {
  g.globalCompositeOperation = 'destination-in'; g.fillStyle = '#000';
  g.beginPath(); g.roundRect ? g.roundRect(0, 0, W, H, r) : g.rect(0, 0, W, H); g.fill();
  g.globalCompositeOperation = 'source-over';
}

// Who is on screen: people [left, right], their fighters, the viewer's side,
// and whether a fight is on. Redrawn only when something visible changes.
let shown = '';
export async function showPeople({ people, fighters, side = 0, live = false }) {
  const pic = p => p?.bot ? null : p?.avatar || null;
  const key = JSON.stringify([people.map(p => [p?.id, p?.name, p?.avatar, p?.bot, p?.rating]), fighters.map(f => f?.id), side, live]);
  if (key === shown) return;
  shown = key;
  [city] = await Promise.all([load('assets/backdrop/far.jpg')]);
  const faces = await Promise.all([0, 1].map(i => load(pic(people[i]))));
  if (key !== shown) return;
  const me = people[side], them = people[1 - side];
  put(2, tv(live ? people : [me, them], live ? faces : [faces[side], faces[1 - side]], side, live));
  put(3, laptop(me, faces[side], fighters[side]));
  // The canvases: the player and the opponent; no photo, a city painting.
  put(4, portrait(me, faces[side], 512, 380));
  // The small frame is an oval.
  const small = portrait(them, live ? faces[1 - side] : null, 256, 364);
  const g = small.getContext('2d');
  g.globalCompositeOperation = 'destination-in'; g.fillStyle = '#000'; g.beginPath(); g.ellipse(128, 182, 126, 180, 0, 0, Math.PI * 2); g.fill();
  put(5, small);
}
