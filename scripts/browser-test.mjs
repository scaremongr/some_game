import assert from 'node:assert/strict';
import { mkdir, access } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createArena } from '../server/index.mjs';
try { await access('.browsers'); process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers'); } catch {}
const { chromium } = await import('playwright');
const app=await createArena({dev:true});
app.server.listen(0,'127.0.0.1');await new Promise(r=>app.server.once('listening',r));
const base=`http://127.0.0.1:${app.server.address().port}`;
const browser=await chromium.launch({headless:true,args:['--enable-unsafe-swiftshader']});
const errors=[];
await mkdir('artifacts',{recursive:true});
async function page(viewport) {
  const context=await browser.newContext({viewport,hasTouch:viewport.width<600});
  // Integration with real Telegram needs two real accounts; this tests guest transport/UI.
  await context.route('https://telegram.org/**',route=>route.fulfill({contentType:'text/javascript',body:''}));
  const p=await context.newPage();p.on('pageerror',e=>errors.push(e.message));
  await p.goto(base);await p.waitForSelector('#loader',{state:'hidden',timeout:30000});return p;
}
const rendered=p=>p.evaluate(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)));
try {
  const desktop=await page({width:1365,height:860});
  await desktop.screenshot({path:'artifacts/lobby-desktop.png'});
  const mobile=await page({width:390,height:844});
  await mobile.screenshot({path:'artifacts/lobby-mobile.png'});
  await mobile.click('#practice');await mobile.waitForTimeout(2700);
  await mobile.screenshot({path:'artifacts/training-mobile.png'});
  // Dummy bot from here on: input checks must not depend on the sparring AI.
  await mobile.click('#leave');await mobile.click('#practice');await mobile.click('#training-mode');
  await mobile.waitForFunction(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).phase===1);
  const before=await rendered(mobile);
  await mobile.keyboard.down('d');await mobile.waitForTimeout(100);await mobile.keyboard.up('d');
  assert.ok((await rendered(mobile)).fighters[0].x>before.fighters[0].x);
  await mobile.keyboard.press('k');await mobile.waitForFunction(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).fighters[0].action===2);
  assert.ok((await rendered(mobile)).fighters[0].stamina<1000);
  await mobile.click('#leave');await mobile.click('#practice');
  await mobile.waitForFunction(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).phase===1);
  // Joystick and action pad work at the same time with two thumbs.
  const cdp=await mobile.context().newCDPSession(mobile);
  const stickBox=await mobile.locator('#stick').boundingBox(),guard=await mobile.locator('.act.guard').boundingBox();
  const s0={x:stickBox.x+stickBox.width/2,y:stickBox.y+stickBox.height*0.6},g={x:guard.x+guard.width/2,y:guard.y+guard.height/2};
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{...s0,id:1}]});
  await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:s0.x+60,y:s0.y,id:1}]});
  await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:s0.x+60,y:s0.y,id:1},{...g,id:2}]});
  await mobile.waitForTimeout(150);
  assert.equal((await rendered(mobile)).fighters[0].previous & 6,6);
  assert.ok((await rendered(mobile)).fighters[0].guard);
  await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  await mobile.waitForTimeout(150);
  assert.equal((await rendered(mobile)).fighters[0].previous & 6,0);
  // Double flick on the joystick dashes (in-page events: human tempo, not CDP latency).
  await mobile.evaluate(({x,y})=>{
    const stick=document.getElementById('stick');
    const fire=(type,id,cx)=>stick.dispatchEvent(new PointerEvent(type,{pointerId:id,clientX:cx,clientY:y,bubbles:true,cancelable:true,pointerType:'touch'}));
    for(const id of [11,12]){fire('pointerdown',id,x);fire('pointermove',id,x-60);fire('pointerup',id,x-60);}
  },s0);
  await mobile.waitForFunction(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).fighters[0].action===3,{},{timeout:2000});
  await mobile.evaluate(()=>window.dispatchEvent(new Event('blur')));
  await mobile.click('#leave');await mobile.click('#quick');await desktop.click('#quick');
  await mobile.waitForSelector('#hud:not(.hidden)');await desktop.waitForSelector('#hud:not(.hidden)');
  await mobile.waitForTimeout(2700);
  await desktop.screenshot({path:'artifacts/fight-desktop.png'});
  await mobile.screenshot({path:'artifacts/fight-mobile.png'});
  const online=await rendered(mobile);assert.equal(online.phase,1);
  // Reload resumes the same server match rather than creating a second guest.
  await mobile.reload();await mobile.waitForSelector('#hud:not(.hidden)',{timeout:15000});
  assert.ok((await rendered(mobile)).tick>=online.tick);
  await mobile.click('#leave');await desktop.waitForSelector('#result:not(.hidden)');
  assert.equal((await rendered(desktop)).phase,3);
  await desktop.screenshot({path:'artifacts/result-desktop.png'});
  assert.equal(await desktop.locator('#rematch').isDisabled(),true);
  assert.deepEqual(errors,[]);
  console.log('Browser PASS: desktop/mobile rendering, training inputs, two-client PvP, reload resume, forfeit. Screenshots: artifacts/');
} finally {await browser.close();await app.close();}
