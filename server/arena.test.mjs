import { test } from 'node:test';
import assert from 'node:assert/strict';
import { WebSocket } from 'ws';
import { createArena } from './index.mjs';
import { createHmac } from 'node:crypto';
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function client(url,resume) {
  const ws=new WebSocket(url),messages=[];
  ws.on('message',d=>messages.push(JSON.parse(d)));
  await new Promise((r,j)=>{ws.once('open',r);ws.once('error',j);});
  const send=m=>ws.send(JSON.stringify(m));
  async function wait(type,predicate=()=>true,timeout=4000){
    const start=Date.now();while(Date.now()-start<timeout){const i=messages.findIndex(m=>m.type===type&&predicate(m));if(i>=0)return messages.splice(i,1)[0];await delay(10);}throw Error('Timeout: '+type);
  }
  send({type:'auth',v:3,resume,name:'Tester'});
  const welcome=await wait('welcome');
  return {ws,send,wait,messages,key:welcome.resume};
}
test('two clients: matchmaking, authoritative hits, sequence validation, resume, forfeit', {timeout:15000}, async()=>{
  const app=await createArena({dev:true});app.server.listen(0,'127.0.0.1');await new Promise(r=>app.server.once('listening',r));
  const base=`http://127.0.0.1:${app.server.address().port}`,url=base.replace('http','ws')+'/ws';
  try{
    assert.equal((await fetch(base+'/health').then(r=>r.json())).ok,true);
    assert.equal((await fetch(base+'/.env')).status,404);
    const a=await client(url),b=await client(url);
    a.send({type:'queue',fighter:'medea'});await a.wait('queued');b.send({type:'queue',fighter:'../evil'});
    const ma=await a.wait('match'),mb=await b.wait('match');assert.equal(ma.code,mb.code);assert.notEqual(ma.side,mb.side);
    // Both players learn who fights with which body; unknown ids fall back.
    assert.deepEqual(ma.players.map(p=>p.fighter),['medea','medea']);assert.deepEqual(mb.players,ma.players);
    await a.wait('state',m=>m.state.phase===1);
    a.send({type:'input',seq:1,bits:2,hp:99999,x:99999});b.send({type:'input',seq:1,bits:1});
    await delay(230);a.send({type:'input',seq:2,bits:2});b.send({type:'input',seq:2,bits:1});
    await delay(180);a.send({type:'input',seq:3,bits:8});b.send({type:'input',seq:3,bits:0});
    const hit=await a.wait('state',m=>m.state.fighters[1].hp<100);
    assert.equal(hit.state.fighters[1].hp,92);
    const same=await b.wait('state',m=>m.state.tick===hit.state.tick);assert.deepEqual(same.state,hit.state);
    a.send({type:'input',seq:2,bits:16});a.send({type:'input',seq:4,bits:8192});
    const validated=await a.wait('state',m=>m.state.tick>hit.state.tick);assert.equal(validated.ack[0],3);
    await a.wait('state',m=>m.state.tick>validated.state.tick && m.state.phase===1 && m.state.fighters[0].action===0);
    a.send({type:'input',seq:5,bits:2048});
    const smashed=await a.wait('state',m=>m.state.objects.some(o=>o.hp===0));
    assert.equal(smashed.ack[0],5);
    const otherRoom=await b.wait('state',m=>m.state.tick===smashed.state.tick);
    assert.deepEqual(otherRoom.state.objects,smashed.state.objects);
    a.ws.close();await b.wait('state',m=>m.paused);
    const resumed=await client(url,a.key);await resumed.wait('match');await resumed.wait('state',m=>!m.paused);
    resumed.send({type:'leave'});const end=await b.wait('state',m=>m.state.phase===3);assert.equal(end.state.winner,1);
    b.send({type:'queue'});await b.wait('queued');resumed.ws.close();b.ws.close();
  }finally{await app.close();}
});
test('private invitations, occupied room rejection, malformed packets and cancel',async()=>{
  const app=await createArena({dev:true});app.server.listen(0,'127.0.0.1');await new Promise(r=>app.server.once('listening',r));
  const url=`ws://127.0.0.1:${app.server.address().port}/ws`;
  try{
    const a=await client(url),b=await client(url),c=await client(url);
    a.send({type:'create'});const {code}=await a.wait('room');assert.match(code,/^[\w-]{12}$/);
    b.send({type:'join',code});await b.wait('match');c.send({type:'join',code});await c.wait('error');
    c.ws.send('{');await c.wait('error');c.send({type:'queue'});await c.wait('queued');c.send({type:'leave'});await c.wait('lobby');
    a.ws.close();b.ws.close();c.ws.close();
  }finally{await app.close();}
});
test('production refuses to start without Telegram configuration',async()=>{
  await assert.rejects(createArena({dev:false,botToken:'',origin:''}),/BOT_TOKEN/);
});
test('production WebSocket validates origin and a signed Telegram login',async()=>{
  const botToken='123:integration-test',origin='https://arena.example.com';
  const app=await createArena({botToken,origin,miniApp:'https://t.me/test_bot/arena'});
  app.server.listen(0,'127.0.0.1');await new Promise(r=>app.server.once('listening',r));
  const url=`ws://127.0.0.1:${app.server.address().port}/ws`;
  try {
    const bad=new WebSocket(url,{origin:'https://wrong.example'});
    await new Promise((resolve,reject)=>{bad.once('error',resolve);bad.once('open',()=>reject(Error('Wrong origin accepted')));});
    const ws=new WebSocket(url,{origin});
    await new Promise(r=>ws.once('open',r));
    const p=new URLSearchParams({auth_date:String(Math.floor(Date.now()/1000)),user:JSON.stringify({id:789,first_name:'Telegram Fighter'})});
    const check=[...p].sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${k}=${v}`).join('\n');
    const key=createHmac('sha256','WebAppData').update(botToken).digest();
    p.set('hash',createHmac('sha256',key).update(check).digest('hex'));
    const reply=new Promise(r=>ws.once('message',data=>r(JSON.parse(data))));
    ws.send(JSON.stringify({type:'auth',v:3,initData:p.toString(),name:'Forged client name'}));
    const message=await reply;assert.equal(message.type,'welcome');assert.equal(message.user.name,'Telegram Fighter');assert.equal(message.user.id,'789');ws.close();
  } finally {await app.close();}
});

