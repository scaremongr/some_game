import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { createArena } from '../server/index.mjs';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const {chromium}=await import('playwright');
const app=await createArena({dev:true});app.server.listen(0,'127.0.0.1');await new Promise(r=>app.server.once('listening',r));
const browser=await chromium.launch({headless:true,args:['--enable-unsafe-swiftshader']});
const p=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];
p.on('pageerror',e=>errors.push(e.message));
await p.route('https://telegram.org/**',r=>r.fulfill({body:''}));
const state=()=>p.evaluate(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)));
try{
  await p.goto(`http://127.0.0.1:${app.server.address().port}`);
  await p.waitForSelector('#loader',{state:'hidden',timeout:45000});
  assert.equal(await p.evaluate(()=>window.arenaModelStatus),1);
  await p.click('#practice');await p.click('#training-mode');
  await p.waitForFunction(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).phase===1);
  let broken=false,knockedOut=false;
  for(let attack=0;attack<16;attack++){
    let s=await state();
    if(s.fighters[1].hp===0){knockedOut=true;break;}
    if(s.walls[1].hp===0&&!broken){broken=true;await p.screenshot({path:'artifacts/wall-destruction.png'});}
    await p.keyboard.down('d');
    await p.waitForFunction(()=>{const s=JSON.parse(new TextDecoder().decode(window.arenaRenderBytes));return s.fighters[1].x-s.fighters[0].x<1100;},{},{timeout:6000});
    await p.keyboard.up('d');await p.keyboard.press('k');await p.waitForTimeout(1000);
  }
  assert.ok(broken,'Heavy attacks must destroy the wall through actual player inputs');
  assert.ok(knockedOut,'Attacks must reach a KO');
  await p.waitForTimeout(700);await p.screenshot({path:'artifacts/ragdoll-knockout.png'});
  assert.deepEqual(errors,[]);
  console.log('Physics browser PASS: original skinned GLB loaded; heavy impulses broke wall; KO ragdoll rendered.');
}finally{await browser.close();await app.close();}
