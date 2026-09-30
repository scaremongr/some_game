import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from "node:crypto";
import { validateTelegram } from './auth.mjs';
const token='123:test-token';
function signed(overrides={}) {
  const p=new URLSearchParams({auth_date:String(Math.floor(Date.now()/1000)),user:JSON.stringify({id:123,first_name:'Тест'}),...overrides});
  const check=[...p].sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${k}=${v}`).join('\n');
  const secret=createHmac('sha256','WebAppData').update(token).digest();
  p.set('hash',createHmac('sha256',secret).update(check).digest('hex'));return p.toString();
}
test('valid Telegram identity; tamper, stale, future, duplicate and invalid users rejected',()=>{
  assert.deepEqual(validateTelegram(signed(),token),{id:'123',name:'Тест',photo:null,dm:false});
  assert.equal(validateTelegram(signed({user:JSON.stringify({id:9,first_name:'Ф',photo_url:'https://t.me/i/userpic/320/abc.jpg'})}),token).photo,'https://t.me/i/userpic/320/abc.jpg');
  assert.equal(validateTelegram(signed({user:JSON.stringify({id:9,first_name:'Ф',photo_url:'https://evil.example/a.jpg'})}),token).photo,null);
  assert.throws(()=>validateTelegram(signed().replace('123','999'),token));
  assert.throws(()=>validateTelegram(signed({auth_date:'1'}),token));
  assert.throws(()=>validateTelegram(signed({auth_date:String(Math.floor(Date.now()/1000)+90)}),token));
  assert.throws(()=>validateTelegram(signed()+'&auth_date=1',token));
  assert.throws(()=>validateTelegram(signed({user:'{"id":-1,"first_name":"Bad"}'}),token));
  assert.throws(()=>validateTelegram(signed(),'wrong'));
});
test('data issued for another bot is rejected with a diagnostic', ()=>{
  // A foreign bot's HMAC fails against our token and carries a diagnostic.
  const foreign=(()=>{const p=new URLSearchParams({auth_date:String(Math.floor(Date.now()/1000)),user:JSON.stringify({id:5,first_name:'X'})});
    const check=[...p].sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${k}=${v}`).join('\n');
    const secret=createHmac('sha256','WebAppData').update('999:other').digest();p.set('hash',createHmac('sha256',secret).update(check).digest('hex'));return p.toString();})();
  try { validateTelegram(foreign,token); assert.fail('accepted'); }
  catch(e){ assert.equal(e.detail.ownBot,false); assert.deepEqual(e.detail.keys,['auth_date','hash','user']); }
});
test('initData from either the game bot or the marketplace bot is accepted', ()=>{
  const sign=(bot)=>{const p=new URLSearchParams({auth_date:String(Math.floor(Date.now()/1000)),user:JSON.stringify({id:7,first_name:'Игрок'})});
    const check=[...p].sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${k}=${v}`).join('\n');
    const secret=createHmac('sha256','WebAppData').update(bot).digest();p.set('hash',createHmac('sha256',secret).update(check).digest('hex'));return p.toString();};
  const tokens=['111:game-bot',token];
  assert.equal(validateTelegram(sign('111:game-bot'),tokens).id,'7');
  assert.equal(validateTelegram(sign(token),tokens).id,'7');
  assert.throws(()=>validateTelegram(sign('222:stranger'),tokens));
});
