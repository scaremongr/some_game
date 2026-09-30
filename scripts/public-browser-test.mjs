// Read-only production smoke: training stays local; no Telegram messages.
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const {chromium}=await import('playwright');
const browser=await chromium.launch({headless:true,args:['--enable-unsafe-swiftshader']});
try {
  const page=await browser.newPage({viewport:{width:390,height:844},isMobile:true,hasTouch:true});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  // Telegram forwards initData in the URL fragment; redirects must preserve it.
  await page.goto('https://serbiamarket.duckdns.org/?tgWebAppStartParam=fight_home#launch-check',{timeout:60000});
  await page.waitForSelector('#loader',{state:'hidden',timeout:60000});
  assert.equal(new URL(page.url()).pathname,'/dance/index.html');
  assert.equal(new URL(page.url()).hash,'#launch-check');
  assert.equal(await page.evaluate(()=>window.arenaModelStatus),1);
  const config=await page.evaluate(()=>fetch('config.json').then(r=>r.json()));
  assert.equal(config.dev,false);assert.equal(config.protocol,3);
  await page.click('#practice');
  await page.waitForFunction(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).phase===1);
  await page.screenshot({path:'artifacts/production-mobile.png'});
  await page.click('#training-mode');
  await page.click('#room-damage');
  await page.waitForFunction(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).objects.some(o=>o.hp===0));
  await page.waitForTimeout(800);
  await page.screenshot({path:'artifacts/production-room-destruction.png'});
  assert.deepEqual(errors,[]);
  console.log('Production browser PASS: mobile GLB, WASM, training, touch room destruction, deep-link redirect with preserved fragment.');
} finally {await browser.close();}
