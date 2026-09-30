// Run inside the production container after deployment. Synthetic identities are
// confined to this game's in-memory sessions; no messages are sent to Telegram.
import {createHmac} from 'node:crypto';
import assert from 'node:assert/strict';
import {WebSocket} from 'ws';
const origin=process.env.PUBLIC_ORIGIN;
const url=origin.replace('https:','wss:')+'/dance/ws';
function signed(id){
  const p=new URLSearchParams({auth_date:String(Math.floor(Date.now()/1000)),user:JSON.stringify({id,first_name:'Arena deployment check'})});
  const check=[...p].sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${k}=${v}`).join('\n');
  const secret=createHmac('sha256','WebAppData').update(process.env.BOT_TOKEN).digest();
  p.set('hash',createHmac('sha256',secret).update(check).digest('hex'));return p.toString();
}
const sockets=[];
async function player(id){
  const ws=new WebSocket(url,{origin,handshakeTimeout:10000});sockets.push(ws);
  const messages=[];ws.on('message',data=>messages.push(JSON.parse(data)));
  await new Promise((r,j)=>{ws.once('open',r);ws.once('error',()=>j(Error('Public WSS handshake failed')));});
  async function wait(type,predicate=()=>true){
    for(let i=0;i<1000;i++){
      const at=messages.findIndex(m=>m.type===type&&predicate(m));if(at>=0)return messages.splice(at,1)[0];
      if(messages.some(m=>m.type==='error'))throw Error('Game rejected deployment probe');
      await new Promise(r=>setTimeout(r,10));
    }throw Error('Timeout '+type);
  }
  ws.send(JSON.stringify({type:'auth',v:3,initData:signed(id)}));await wait('welcome');
  return {send:m=>ws.send(JSON.stringify(m)),wait};
}
try {
  const a=await player(800000000001),b=await player(800000000002);
  a.send({type:'queue'});await a.wait('queued');b.send({type:'queue'});
  const ma=await a.wait('match'),mb=await b.wait('match');assert.equal(ma.code,mb.code);
  const sa=await a.wait('state',m=>m.state.phase===1);
  const sb=await b.wait('state',m=>m.state.tick===sa.state.tick);
  assert.deepEqual(sa.state,sb.state);assert.equal(sa.state.walls[0].hp,75);
  assert.equal(sa.state.objects.length,20);assert.equal(sa.state.fighters[0].meter,400);
  a.send({type:'input',seq:1,bits:2048});
  const smashed=await a.wait('state',m=>m.state.objects.some(o=>o.hp===0));
  const same=await b.wait('state',m=>m.state.tick===smashed.state.tick);
  assert.deepEqual(smashed.state.objects,same.state.objects);
  a.send({type:'leave'});const end=await b.wait('state',m=>m.state.phase===3);assert.equal(end.state.winner,1);
  console.log('Public WSS PASS: Telegram HMAC, two players, identical combat/room state, authoritative destruction, forfeit.');
} finally {for(const ws of sockets)ws.close();}
