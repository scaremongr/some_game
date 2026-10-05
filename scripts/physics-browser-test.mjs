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
  // Cross both former west barriers with ordinary walking and untouched props.
  await p.keyboard.down('a');
  await p.waitForFunction(()=>JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).fighters[0].x < -9000, {}, {timeout:20000});
  await p.keyboard.up('a');
  const garden = await state();
  assert.ok(garden.objects.every(o=>o.broken_tick===0));
  assert.ok(garden.walls.every(w=>w.hp===110));
  assert.equal(await p.locator('#room-name').textContent(), 'ЗИМНИЙ САД');
  await p.screenshot({path:'artifacts/open-rooms.png'});
  let crossed=false,knockedOut=false;
  for(let attack=0;attack<16;attack++){
    let s=await state();
    if(s.fighters[1].hp===0){knockedOut=true;break;}
    if(s.fighters[1].x>3000&&!crossed){crossed=true;await p.screenshot({path:'artifacts/fight-study.png'});}
    await p.keyboard.down('d');
    await p.waitForFunction(()=>{const s=JSON.parse(new TextDecoder().decode(window.arenaRenderBytes));return s.fighters[1].x-s.fighters[0].x<1100;},{},{timeout:15000});
    await p.keyboard.up('d');await p.keyboard.press('k');await p.waitForTimeout(1000);
  }
  assert.ok(crossed,'Actual hits must carry the fight into the study');
  assert.ok((await state()).walls.every(w=>w.hp===110 && w.broken_tick===0),'Exterior remains intact');
  assert.ok(knockedOut,'Attacks must reach a KO');
  await p.waitForTimeout(700);await p.screenshot({path:'artifacts/ragdoll-knockout.png'});
  assert.deepEqual(errors,[]);
  console.log('Physics browser PASS: original skinned GLB loaded; open rooms crossed on foot; heavy impulses carried fight into study; KO ragdoll rendered.');
}finally{await browser.close();await app.close();}
