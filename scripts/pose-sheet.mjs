// Renders fighter poses into contact sheets for visual review.
//   node scripts/pose-sheet.mjs [spec.json] [out-prefix]
// Without a spec every attack is sampled at wind-up, contact and recovery.
// The page runs the real WASM renderer; only the fight state and the camera
// are substituted through window.arenaRenderBytes / window.arenaDebugCamera.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createArena } from '../server/index.mjs';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const { chromium } = await import('playwright');

const [specPath, prefix = 'artifacts/poses/sheet'] = process.argv.slice(2);
const attacks = {1:[7,20],11:[6,24],2:[21,47],4:[13,40],8:[11,30],9:[14,42],10:[12,46],12:[12,38],13:[7,28],14:[18,46],16:[8,24],17:[8,28],18:[12,34],19:[18,44]};
const names = {1:'jab',11:'cross',2:'overhead',4:'throw',8:'front kick',9:'sweep',10:'uppercut',12:'roundhouse',13:'air kick',14:'impulse',16:'low kick',17:'hook',18:'side kick',19:'ground pound'};
function defaultSpec() {
  const shots = [
    { label: 'stance', f0: {}, f1: {} },
    { label: 'guard / crouch guard', f0: { guard: true }, f1: { guard: true, crouch: true } },
    { label: 'crouch', f0: { crouch: true }, f1: { crouch: true } },
  ];
  for (const [id, [hit, total]] of Object.entries(attacks)) {
    const air = id === '13' ? { y: 650 } : {};
    for (const frame of [Math.max(1, hit - 5), hit, Math.round((hit + total) / 2)])
      shots.push({ label: `${names[id]} f${frame}`, f0: { action: +id, frame, ...air }, f1: {} });
  }
  for (const [dir, label] of [[1, 'dash forward'], [-1, 'dash back']])
    for (const frame of [3, 7, 13]) shots.push({ label: `${label} f${frame}`, f0: { action: 3, frame, dash_dir: dir }, f1: {} });
  for (const [vy, vx, label] of [[90, 0, 'jump rise'], [0, 0, 'jump apex'], [-80, 0, 'jump fall'], [40, 42, 'flip 1/3'], [-20, 42, 'flip 2/3'], [-60, -42, 'back flip']])
    shots.push({ label, f0: { y: 700, vy, vx }, f1: {} });
  // Reactions come from the hit event: attacker action + event kind.
  let event = 100;
  for (const [attack, kind, target, label] of [[1, 1, 1, 'hit high'], [8, 1, 1, 'hit gut'], [1, 3, 1, 'parried (left)'], [2, 4, 1, 'guard break']])
    for (const frame of [3, 12]) {
      const victim = kind === 3 ? 0 : 1;
      const f = { action: 5, stun: 30 - frame, frame };
      shots.push({ label: `${label} f${frame}`, state: { event: event++, event_kind: kind, event_target: target },
        f0: victim === 0 ? f : { action: attack, frame: 14 }, f1: victim === 1 ? f : { action: attack, frame: 14 } });
    }
  for (const [vy, label] of [[50, 'air hit up'], [-50, 'air hit down']]) shots.push({ label, f0: {}, f1: { action: 5, stun: 20, frame: 8, y: 600, vy } });
  for (const frame of [2, 14, 24, 34, 44, 52]) shots.push({ label: `knockdown f${frame}`, f0: {}, f1: { action: 15, down: 56 - frame, frame } });
  for (const frame of [6, 16, 24, 40]) shots.push({ label: `KO f${frame}`, f0: {}, f1: { hp: 0, frame }, phase: 2 });
  shots.push({ label: 'victory', f0: { frame: 40 }, f1: { hp: 0, frame: 40 }, phase: 2, state: { winner: 0 } });
  shots.push({ label: 'defeat (timeout)', f0: { frame: 40 }, f1: { frame: 40, hp: 30 }, phase: 2, state: { winner: 0 } });
  return { shots, columns: 3, x: [-560, 560] };
}
const spec = specPath ? JSON.parse(await readFile(specPath, 'utf8')) : defaultSpec();
const app = await createArena({ dev: true });
app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader'] });
const width = spec.width || 720, height = spec.height || 540;
const page = await browser.newPage({ viewport: { width, height } });
const errors = []; page.on('pageerror', e => errors.push(e.message));
await page.route('https://telegram.org/**', r => r.fulfill({ contentType: 'text/javascript', body: '' }));
try {
  await page.goto(`http://127.0.0.1:${app.server.address().port}`);
  await page.waitForSelector('#loader', { state: 'hidden', timeout: 60000 });
  await page.addStyleTag({ content: 'body>*:not(#glcanvas){display:none!important}' });
  // Optional fighters: spec.fighters = [{model, pack}, {model, pack}].
  if (spec.fighters) {
    await page.evaluate(f => { window.arenaFighters = f; }, spec.fighters);
    await page.waitForTimeout(300);
    await page.waitForFunction(() => window.arenaAvatarsReady === 3, {}, { timeout: 60000 });
  }
  await mkdir(prefix.replace(/[^/\\]*$/, ''), { recursive: true });
  const files = [];
  for (const [i, shot] of spec.shots.entries()) {
    await page.evaluate(({ shot, spec, camera }) => {
      const base = window.__baseState ||= JSON.parse(new TextDecoder().decode(window.arenaRenderBytes));
      const state = structuredClone(base);
      state.phase = shot.phase ?? 1; state.freeze = 0;
      Object.assign(state, shot.state || {});
      const x = shot.x ?? spec.x ?? [-900, 900];
      state.fighters.forEach((f, side) => Object.assign(f, { x: x[side], y: 0, action: 0, frame: 0, guard: false, crouch: false, previous: 0 }, shot['f' + side] || {}));
      window.arenaDebugCamera = shot.camera || camera;
      window.arenaRenderBytes = new TextEncoder().encode(JSON.stringify(state));
    }, { shot, spec: { x: spec.x }, camera: spec.camera || [0, 1.05, 4.6, 0, 0.92, 0, 0.62, 1] });
    await page.waitForTimeout(shot.wait ?? 160);
    const file = `${prefix}-${String(i).padStart(2, '0')}.png`;
    await page.screenshot({ path: file });
    files.push({ file, label: shot.label || '' });
  }
  // Pages of up to 12 shots keep each sheet legible.
  const cols = spec.columns || 3, scale = spec.scale || 0.5, perPage = spec.perPage || 12;
  const pages = [];
  for (let start = 0; start < files.length; start += perPage) pages.push(files.slice(start, start + perPage));
  for (const [n, group] of pages.entries()) {
    const name = pages.length > 1 ? `${prefix}-p${n + 1}` : prefix;
    const html = `<body style="margin:0;background:#111;display:grid;grid-template-columns:repeat(${cols},${width * scale}px);gap:4px;font:12px sans-serif;color:#eee">${group.map(f => `<figure style="margin:0;position:relative"><img src="${pathToFileURL(resolve(f.file))}" style="width:${width * scale}px;display:block"><figcaption style="position:absolute;left:4px;top:2px;background:#000a;padding:1px 4px">${f.label}</figcaption></figure>`).join('')}</body>`;
    const sheetHtml = resolve(`${name}.html`); await writeFile(sheetHtml, html);
    const rows = Math.ceil(group.length / cols);
    const sheet = await browser.newPage({ viewport: { width: Math.ceil(cols * (width * scale + 4)), height: Math.ceil(rows * (height * scale + 4)) } });
    await sheet.goto(pathToFileURL(sheetHtml).href); await sheet.waitForTimeout(300);
    await sheet.screenshot({ path: `${name}.png`, fullPage: true }); await sheet.close();
    console.log(`Sheet: ${name}.png (${group.length} shots)`);
  }
  if (errors.length) console.error('Page errors:', errors);
} finally { await browser.close(); await app.close(); }
