// Tiles extracted video frames into labelled contact sheets.
//   node scripts/tile-frames.mjs <frames-dir> <out-prefix> [cols] [perPage] [cropTop] [cropHeight] [scale] [frameWidth] [cropLeft] [cropWidth]
import { readdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const { chromium } = await import('playwright');
const [dir, prefix, cols = '4', perPage = '16', top = '130', height = '470', scale = '0.36', width = '1280', left = '0', cropWidth = width] = process.argv.slice(2);
const files = (await readdir(dir)).filter(f => f.endsWith('.png')).sort();
const browser = await chromium.launch({ headless: true });
const w = +cropWidth * +scale, h = +height * +scale;
for (let page = 0; page * +perPage < files.length; page++) {
  const group = files.slice(page * +perPage, (page + 1) * +perPage);
  const html = `<body style="margin:0;background:#111;display:grid;grid-template-columns:repeat(${cols},${w}px);gap:3px;font:11px sans-serif;color:#fff">${group.map(f => `<div style="position:relative;width:${w}px;height:${h}px;overflow:hidden"><img src="${pathToFileURL(resolve(dir, f))}" style="width:${+width * +scale}px;position:absolute;top:${-top * scale}px;left:${-left * scale}px"><span style="position:absolute;left:3px;top:2px;background:#000a;padding:0 3px">${f}</span></div>`).join('')}</body>`;
  const file = resolve(`${prefix}-${page + 1}.html`); await writeFile(file, html);
  const rows = Math.ceil(group.length / +cols);
  const p = await browser.newPage({ viewport: { width: Math.ceil(+cols * (w + 3)), height: Math.ceil(rows * (h + 3)) } });
  await p.goto(pathToFileURL(file).href); await p.waitForTimeout(200);
  await p.screenshot({ path: `${prefix}-${page + 1}.png` }); await p.close();
  console.log(`${prefix}-${page + 1}.png`);
}
await browser.close();
