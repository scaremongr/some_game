import http from 'node:http';
import { readFile, writeFile, readdir, unlink, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, extname, sep } from 'node:path';
import { randomBytes, randomInt } from 'node:crypto';
import { WebSocketServer, WebSocket } from 'ws';
import { validateTelegram } from './auth.mjs';
import { createGameBot, esc } from './bot.mjs';
import { createLeague } from './league.mjs';
import { simulation } from '../web/combat.js';

const root = fileURLToPath(new URL('../dist/', import.meta.url));
const token = () => randomBytes(24).toString('base64url');
// Identities of the post-deploy check (server/deploy-probe.mjs): never rated.
const PROBES = new Set(['800000000001', '800000000002']);
const DEFAULT_FIGHTER = 'medea';
// Fighter ids a client may pick: assets/fighters/roster.json shipped with the client.
async function loadRoster() {
  try {
    const roster = JSON.parse(await readFile(resolve(root, 'assets/fighters/roster.json'), 'utf8'));
    const ids = roster.map(f => f?.id).filter(id => typeof id === 'string' && /^[a-z0-9_-]{1,24}$/.test(id));
    return new Set([DEFAULT_FIGHTER, ...ids]);
  } catch { return new Set([DEFAULT_FIGHTER]); }
}
export async function createArena(options = {}) {
  const dev = options.dev ?? false;
  // The marketplace bot (opens the game via its Main Mini App) and the
  // game's own bot: initData signed by either is accepted.
  const botToken = options.botToken ?? process.env.BOT_TOKEN;
  const gameBotToken = options.gameBotToken ?? process.env.GAME_BOT_TOKEN;
  const origin = options.origin ?? process.env.PUBLIC_ORIGIN;
  const miniApp = options.miniApp ?? process.env.TELEGRAM_APP_URL ?? '';
  // Public base URL of the game (behind the reverse proxy), ends with '/'.
  const gameUrl = options.gameUrl ?? process.env.GAME_URL ?? (origin ? origin + '/dance/' : '');
  // Other bots whose Mini App may open the game (numeric ids, comma separated).
  const extraBots = (options.extraBots ?? process.env.EXTRA_BOT_IDS ?? '').split(',').map(s => s.trim()).filter(s => /^\d+$/.test(s));
  const authenticate = initData => {
    try { return validateTelegram(initData, [gameBotToken, botToken], Date.now(), extraBots); }
    catch (error) {
      if (error.detail) {
        // Field names and age only: never the data itself.
        console.warn('auth rejected', JSON.stringify(error.detail));
        if (!error.detail.ownBot && miniApp) error.message = `Игра открыта через другого бота. Откройте её по ссылке ${miniApp}?startapp=fight_home`;
      }
      throw error;
    }
  };
  if (!dev && (!(botToken || gameBotToken) || !origin || !miniApp)) throw Error('Production requires BOT_TOKEN or GAME_BOT_TOKEN, PUBLIC_ORIGIN and TELEGRAM_APP_URL. Use --dev for localhost.');
  if (gameUrl && !/^https?:\/\/[^?#]+\/$/.test(gameUrl)) throw Error('GAME_URL must be an absolute URL ending with /.');
  // Ratings and chat tables live in DATA_DIR (a volume in production); dev
  // and tests keep them in memory unless given a directory.
  const dataDir = options.dataDir !== undefined ? options.dataDir
    : process.env.DATA_DIR ?? (process.env.NODE_ENV === 'production' ? resolve(root, '../data') : null);
  const league = createLeague({ dir: dataDir });
  const bot = gameBotToken && gameUrl ? createGameBot({ token: gameBotToken, gameUrl, appLink: miniApp, league, api: options.telegramApi }) : null;
  // Avatars are served by the game (same origin); rating and league travel
  // with every player so both sides see them.
  const publicPlayer = s => {
    const card = PROBES.has(s.user.id) ? null : league.card(s.user.id);
    return { id: s.user.id, name: s.user.name, fighter: s.fighter, avatar: s.user.photo || (bot && !dev) ? `avatar/${s.user.id}.jpg` : null, rating: card?.rating ?? null, league: card?.league ?? null };
  };
  if (bot && !dev) bot.setup().then(failed => { for (const f of failed) console.warn('bot setup', f); });
  if (!dev && (new URL(origin).protocol !== 'https:' || new URL(origin).origin !== origin)) throw Error('PUBLIC_ORIGIN must be an HTTPS origin without a path or trailing slash.');
  if (miniApp && !/^https:\/\/t\.me\/[A-Za-z0-9_]+(?:\/[A-Za-z0-9_]+)?$/.test(miniApp)) throw Error('TELEGRAM_APP_URL must be a t.me Mini App URL without query.');
  const module = await WebAssembly.compile(await readFile(resolve(root, 'arena_combat.wasm')));
  const fighters = await loadRoster();
  const sessions = new Map(), users = new Map(), rooms = new Map(), queue = [];
  const avatars = new Map();
  async function avatarBytes(id) {
    // Live players and anyone on a leaderboard.
    const known = users.has(id) || !!league.player(id);
    const url = users.get(id)?.user.photo ?? league.player(id)?.photo;
    if (!url && !(known && bot && !dev)) return null;
    if (dev && url?.startsWith('assets/')) return readFile(resolve(root, url)).catch(() => null);
    const cached = avatars.get(id);
    // Misses are remembered for an hour: no Bot API call per page view.
    if (cached && cached.url === (url || 'bot') && (cached.bytes || Date.now() - cached.at < 3600_000)) return cached.bytes;
    const remember = bytes => {
      if (avatars.size > 400) avatars.delete(avatars.keys().next().value);
      avatars.set(id, { url: url || 'bot', bytes, at: Date.now() });
      return bytes;
    };
    if (!url) {
      // Telegram gives Mini Apps an SVG link only; the bot reads the JPEG.
      try {
        const bytes = await bot.photo(id);
        return remember(bytes && bytes.length <= 400_000 && bytes[0] === 0xff && bytes[1] === 0xd8 ? bytes : null);
      } catch { return remember(null); }
    }
    try {
      const r = await fetch(url, { signal: AbortSignal.timeout(6000) });
      const type = r.headers.get('content-type') || '';
      const bytes = Buffer.from(await r.arrayBuffer());
      // JPEG only: it is what Telegram serves and what the renderer decodes.
      if (!r.ok || !type.includes('jpeg') || bytes.length > 400_000 || bytes[0] !== 0xff || bytes[1] !== 0xd8) return null;
      return remember(bytes);
    } catch { return null; }
  }
  // Victory cards: the winner's page draws a JPEG and uploads it with a
  // one-time ticket; the bot turns it into a message to share to any chat.
  const cardDir = dataDir ? resolve(dataDir, 'cards') : null;
  const cards = new Map(), tickets = new Map();
  let cardNames = [];
  if (cardDir) {
    await mkdir(cardDir, { recursive: true });
    cardNames = (await readdir(cardDir)).filter(f => /^[a-f0-9]{24}\.jpg$/.test(f));
  }
  async function storeCard(bytes) {
    const name = randomBytes(12).toString('hex') + '.jpg';
    if (cardDir) await writeFile(resolve(cardDir, name), bytes); else cards.set(name, bytes);
    cardNames.push(name);
    while (cardNames.length > 500) {
      const old = cardNames.shift();
      if (cardDir) await unlink(resolve(cardDir, old)).catch(() => {}); else cards.delete(old);
    }
    return name;
  }
  async function readBody(req, limit) {
    const chunks = []; let size = 0;
    for await (const chunk of req) { size += chunk.length; if (size > limit) return null; chunks.push(chunk); }
    return Buffer.concat(chunks);
  }
  const server = http.createServer(async (req, res) => {
    try {
      const path = new URL(req.url, 'http://localhost').pathname;
      res.setHeader('X-Content-Type-Options', 'nosniff');
      // Bot updates (webhook); Telegram proves itself with the secret header.
      if (req.method === 'POST' && path === '/telegram' && bot) {
        if (req.headers['x-telegram-bot-api-secret-token'] !== bot.secret) { res.writeHead(403); return res.end(); }
        const chunks = []; let size = 0;
        for await (const chunk of req) { size += chunk.length; if (size > 65536) { res.writeHead(413); return res.end(); } chunks.push(chunk); }
        res.writeHead(200); res.end();
        let update; try { update = JSON.parse(Buffer.concat(chunks).toString('utf8')); } catch { return; }
        bot.handle(update).catch(error => console.warn('bot reply', error.message));
        return;
      }
      if (req.method === 'POST' && path === '/card') {
        const ticket = tickets.get(new URL(req.url, 'http://localhost').searchParams.get('t') || '');
        const reply = (code, body) => { res.writeHead(code, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(body)); };
        if (!ticket || ticket.expires < Date.now()) return reply(403, { error: 'Карточка устарела.' });
        const bytes = await readBody(req, 700_000);
        if (!bytes || bytes[0] !== 0xff || bytes[1] !== 0xd8) return reply(400, { error: 'Нужна картинка JPEG до 700 КБ.' });
        tickets.delete(new URL(req.url, 'http://localhost').searchParams.get('t'));
        const name = await storeCard(bytes);
        let prepared = null;
        if (bot) {
          try { prepared = await bot.prepareCard(ticket.userId, gameUrl + 'cards/' + name, ticket.caption); }
          catch (error) { console.warn('card', error.message); }
        }
        return reply(200, { prepared, url: 'cards/' + name });
      }
      if (req.method !== 'GET' && req.method !== 'HEAD') { res.writeHead(405); return res.end(); }
      const card = /^\/cards\/([a-f0-9]{24}\.jpg)$/.exec(path);
      if (card) {
        const bytes = cardDir ? await readFile(resolve(cardDir, card[1])).catch(() => null) : cards.get(card[1]);
        if (!bytes) { res.writeHead(404); return res.end(); }
        res.writeHead(200, { 'Content-Type': 'image/jpeg', 'Cache-Control': 'public, max-age=604800' });
        return res.end(req.method === 'HEAD' ? undefined : bytes);
      }
      // Profile photos of players with a live session, fetched from Telegram once.
      const avatar = /^\/avatar\/([\w-]{1,48})\.jpg$/.exec(path);
      if (avatar) {
        const bytes = await avatarBytes(avatar[1]);
        if (!bytes) { res.writeHead(404); return res.end(); }
        res.writeHead(200, { 'Content-Type': 'image/jpeg', 'Cache-Control': 'private, max-age=3600' });
        return res.end(req.method === 'HEAD' ? undefined : bytes);
      }
      if (path === '/health') { res.setHeader('Content-Type','application/json'); return res.end(JSON.stringify({ok:true, protocol:3})); }
      if (path === '/config.json') { res.setHeader('Content-Type','application/json'); res.setHeader('Cache-Control','no-store'); return res.end(JSON.stringify({dev, miniApp, protocol:3})); }
      const file = resolve(root, '.' + decodeURIComponent(path === '/' ? '/index.html' : path));
      if (!file.startsWith(root.endsWith(sep) ? root : root + sep)) { res.writeHead(403); return res.end(); }
      const mime = {'.html':'text/html; charset=utf-8','.js':'text/javascript; charset=utf-8','.css':'text/css; charset=utf-8','.wasm':'application/wasm','.glb':'model/gltf-binary','.pack':'application/octet-stream','.json':'application/json; charset=utf-8','.jpg':'image/jpeg','.png':'image/png','.mp3':'audio/mpeg'}[extname(file)];
      if (!mime) { res.writeHead(404); return res.end(); }
      const bytes = await readFile(file);
      res.setHeader('Content-Type',mime); res.setHeader('Cache-Control','no-cache'); res.setHeader('Accept-Ranges','bytes');
      // Byte ranges: Safari streams audio only from servers that serve them.
      const range = /^bytes=(\d*)-(\d*)$/.exec(req.headers.range || '');
      if (range && (range[1] || range[2])) {
        let start = range[1] ? Number(range[1]) : bytes.length - Number(range[2]);
        let end = range[1] && range[2] ? Math.min(Number(range[2]), bytes.length - 1) : bytes.length - 1;
        start = Math.max(0, start);
        if (start > end || start >= bytes.length) { res.writeHead(416, { 'Content-Range': `bytes */${bytes.length}` }); return res.end(); }
        res.writeHead(206, { 'Content-Range': `bytes ${start}-${end}/${bytes.length}`, 'Content-Length': end - start + 1 });
        return res.end(req.method === 'HEAD' ? undefined : bytes.subarray(start, end + 1));
      }
      res.end(req.method === 'HEAD' ? undefined : bytes);
    } catch { res.writeHead(404); res.end('Not found'); }
  });
  const wss = new WebSocketServer({ noServer:true, maxPayload:12288, perMessageDeflate:false });
  server.on('upgrade', (req, socket, head) => {
    let url;
    try { url = new URL(req.url, 'http://localhost'); } catch { socket.destroy(); return; }
    if (url.pathname !== '/ws' || wss.clients.size >= 200 || (!dev && req.headers.origin !== origin)) { socket.destroy(); return; }
    wss.handleUpgrade(req,socket,head,ws=>wss.emit('connection',ws,req));
  });
  function send(s, message) {
    if (s?.ws?.readyState === WebSocket.OPEN) {
      if (s.ws.bufferedAmount > 256 * 1024) { s.ws.terminate(); return; }
      s.ws.send(JSON.stringify(message));
    }
  }
  function broadcast(room, message) { room.players.forEach(s=>send(s,message)); }
  function dequeue(s) { const i=queue.indexOf(s); if(i>=0)queue.splice(i,1); }
  function removeRoom(room) {
    rooms.delete(room.code);
    for (const s of room.players) if(s.room===room) {s.room=null;s.input=0;}
  }
  function snapshot(room, state = room.sim.state()) {
    broadcast(room,{type:'state',state,paused:room.paused,ack:room.players.map(s=>s.seq)});
    if(state.phase===3) finish(room, state);
  }
  // A finished online match: ratings, the players' result, the bot's news.
  function finish(room, state) {
    if(room.recorded || room.players.length!==2) return;
    room.recorded=true;
    const users2=room.players.map(s=>s.user);
    if(users2.some(u=>PROBES.has(u.id))) return;
    const results=league.recordMatch(users2[0],users2[1],state.winner);
    const score=state.score;
    room.players.forEach((s,i)=>{
      const won=state.winner===i;
      let card=null;
      if(won) {
        card=token();
        const loser=users2[1-i];
        tickets.set(card,{userId:s.user.id,expires:Date.now()+15*60000,
          caption:`🏆 Победа в <b>PULSE</b>: <b>${esc(s.user.name)}</b> ${score[i]}:${score[1-i]} <b>${esc(loser.name)}</b>\n${results[i].league.icon} ${results[i].league.name} · рейтинг ${results[i].rating}`});
        if(tickets.size>2000) tickets.delete(tickets.keys().next().value);
      }
      send(s,{type:'result',you:results[i],opponent:{id:users2[1-i].id,name:users2[1-i].name},won,score:[score[i],score[1-i]],card});
    });
    bot?.matchEnded({players:users2,winner:state.winner,score,results}).catch(error=>console.warn('bot match',error.message));
  }
  function start(room) {
    room.sim=simulation(module,randomInt(1,0x7fffffff)); room.rematch=new Set();room.paused=false; room.updated=Date.now(); room.recorded=false;
    room.players.forEach((s,side)=>{s.input=0;s.pending=0;send(s,{type:'match',code:room.code,side,players:room.players.map(publicPlayer)});});
    snapshot(room);
  }
  function leave(s) {
    dequeue(s);
    const room=s.room;
    if(!room)return;
    if(room.sim && room.sim.state().phase!==3) {room.sim.forfeit(room.players.indexOf(s));snapshot(room);}
    broadcast(room,{type:'left',name:s.user.name});
    removeRoom(room);
  }
  function enter(s, room) { dequeue(s); s.room=room; room.players.push(s); room.updated=Date.now(); if(room.players.length===2)start(room); }
  // A room code is a meeting point: a link to a room that no longer exists
  // opens it again under the same code, and whoever follows it waits there.
  function newRoom(s, privateRoom, code = randomBytes(9).toString('base64url')) {
    const room={code,players:[],private:privateRoom,updated:Date.now(),sim:null,paused:false};
    rooms.set(room.code,room);enter(s,room);return room;
  }
  wss.on('connection', ws => {
    let session=null, count=0, windowStart=Date.now();
    ws.alive=true; ws.on('pong',()=>{ws.alive=true;});
    const authTimer=setTimeout(()=>ws.close(4001,'Authentication timeout'),5000);
    ws.on('message', data => {
      try {
        if(Date.now()-windowStart>=1000) {count=0;windowStart=Date.now();}
        if(++count>100) {ws.close(4008,'Rate limit');return;}
        const m=JSON.parse(data.toString());
        if(!m || typeof m!=='object')throw Error('Неверный пакет.');
        if(!session) {
          if(m.type!=='auth' || m.v!==3)throw Error('Обновите страницу игры.');
          let existing=typeof m.resume==='string' ? sessions.get(m.resume) : null;
          // Resume token is a short-lived bearer credential, never sent in URLs.
          if(existing && Date.now()-existing.seen<60000) {
            if(existing.ws && existing.ws.readyState===WebSocket.OPEN) {existing.ws.close(4002,'Session resumed');}
            session=existing;
          } else {
            // Dev guests may bring a same-origin test photo (assets/…jpg).
            const devPhoto = typeof m.photo === 'string' && /^assets\/[\w\/-]+\.jpg$/.test(m.photo) ? m.photo : null;
            const user=dev ? {id:'dev_'+token(),name:typeof m.name==='string'?m.name.slice(0,32):'Спарринг-партнёр',photo:devPhoto} : authenticate(m.initData);
            const prior=users.get(user.id);
            if(prior) {
              // The same player opened the game again (a link in a chat, another
              // window or phone): the new page takes the session over, the old
              // one is told and disconnected. Opened by a link, the page is
              // heading somewhere else: the old room is left.
              if(prior.ws && prior.ws!==ws && prior.ws.readyState===WebSocket.OPEN) prior.ws.close(4002,'Session moved');
              prior.user=user; session=prior; dequeue(session);
              if(m.leave===true && session.room) leave(session);
            } else {
              if(sessions.size>=200) throw Error('Арена заполнена. Попробуйте позже.');
              session={user,key:token(),room:null,input:0,pending:0,seq:-1,lastInput:0,seen:Date.now(),ws:null,fighter:DEFAULT_FIGHTER};
              sessions.set(session.key,session);users.set(user.id,session);
            }
            if(!PROBES.has(user.id)) league.touch(user);
          }
          session.ws=ws;session.seen=Date.now();clearTimeout(authTimer);
          send(session,{type:'welcome',v:3,resume:session.key,user:publicPlayer(session),rating:league.card(session.user.id)});
          if(session.room?.sim) {
            const room=session.room;
            send(session,{type:'match',code:room.code,side:room.players.indexOf(session),players:room.players.map(publicPlayer)});
            snapshot(room);
          } else if(session.room)send(session,{type:'room',code:session.room.code});
          return;
        }
        if(session.ws!==ws)return;
        session.seen=Date.now();
        if(m.type==='ping') {send(session,{type:'pong',at:m.at});return;}
        if(m.type==='input') {
          if(!Number.isSafeInteger(m.seq)||m.seq<=session.seq||!Number.isInteger(m.bits)||m.bits<0||m.bits>4095)return;
          session.seq=m.seq;session.pending|=(m.bits & ~session.input & 3832);session.input=m.bits;session.lastInput=Date.now();return;
        }
        if(m.type==='leave') {leave(session);send(session,{type:'lobby'});return;}
        if(m.type==='rematch' && session.room?.sim?.state().phase===3) {
          const room=session.room;room.rematch.add(session.key);broadcast(room,{type:'rematch',votes:room.rematch.size});
          if(room.rematch.size===2)start(room);return;
        }
        if(m.type==='top') {
          const top=league.top(50).map(p=>({...p,avatar:p.photo||(bot&&!dev)?`avatar/${p.id}.jpg`:null}));
          send(session,{type:'top',players:top,you:{...league.card(session.user.id),id:session.user.id,name:session.user.name}});return;
        }
        // The player allowed the bot to write (revenge calls reach them).
        if(m.type==='dm') {league.setDm(session.user.id,true);return;}
        if(m.type==='join' && (typeof m.code!=='string' || !/^[A-Za-z0-9_-]{12}$/.test(m.code))) throw Error('Неверная ссылка на комнату.');
        // Back into one's own room (the link to it, opened again).
        if(m.type==='join' && session.room?.code===m.code) {
          const room=session.room;
          if(room.sim) {send(session,{type:'match',code:room.code,side:room.players.indexOf(session),players:room.players.map(publicPlayer)});snapshot(room);}
          else send(session,{type:'room',code:room.code});
          return;
        }
        // Heading somewhere new: the current room or queue is left first
        // (an unfinished fight counts as conceded).
        if(['queue','create','join'].includes(m.type) && session.room) leave(session);
        if(session.room)throw Error('Сначала завершите текущую комнату.');
        // The fighter is only a look; unknown ids keep the previous choice.
        if(typeof m.fighter==='string' && fighters.has(m.fighter))session.fighter=m.fighter;
        if(m.type==='queue') {
          dequeue(session);
          let opponent;
          while(queue.length && !opponent) {const next=queue.shift();if(next.ws?.readyState===WebSocket.OPEN&&!next.room&&next!==session)opponent=next;}
          if(opponent) { const room=newRoom(opponent,false);enter(session,room); }
          else {queue.push(session);send(session,{type:'queued'});} return;
        }
        if(m.type==='create') {
          // A revenge: the room is created for a known opponent, who is called
          // in the game (if online) and by the bot.
          const target=typeof m.revenge==='string' && m.revenge!==session.user.id ? league.player(m.revenge) : null;
          const room=newRoom(session,true);
          send(session,{type:'room',code:room.code,revenge:target?{name:target.name}:null});
          if(target) {
            const online=users.get(target.id);
            if(online && !online.room && online.ws?.readyState===WebSocket.OPEN) send(online,{type:'challenge',code:room.code,from:publicPlayer(session)});
            bot?.challenge(target.id,session.user.name,room.code).catch(()=>{});
          }
          return;
        }
        if(m.type==='join') {
          const room=rooms.get(m.code);
          if(!room) {const fresh=newRoom(session,true,m.code);send(session,{type:'room',code:fresh.code,reopened:true});return;}
          if(!room.private || room.players.length!==1)throw Error('В этой комнате уже идёт бой. Найди другого соперника или создай свою комнату.');
          enter(session,room);return;
        }
        throw Error('Неизвестная команда.');
      } catch(error) {
        if(session)send(session,{type:'error',message:error.message});
        else {ws.send(JSON.stringify({type:'error',message:error.message}));ws.close(4003,'Authentication failed');}
      }
    });
    ws.on('error',()=>{});
    ws.on('close',()=>{
      clearTimeout(authTimer);
      if(session?.ws===ws) {session.ws=null;session.seen=Date.now();session.input=0;session.pending=0;dequeue(session);}
    });
  });
  let previous=performance.now(), accumulated=0, ticks=0;
  const clock=setInterval(()=>{
    const now=performance.now();accumulated+=Math.min(now-previous,100);previous=now;
    while(accumulated>=1000/60) {
      accumulated-=1000/60;ticks++;
      for(const room of rooms.values()) {
        if(!room.sim)continue;
        const missing=room.players.findIndex(s=>!s.ws || s.ws.readyState!==WebSocket.OPEN);
        room.paused=missing>=0 && room.sim.state().phase!==3;
        if(room.paused) {
          if(Date.now()-room.players[missing].seen>15000) {
            room.sim.forfeit(missing);room.paused=false;snapshot(room);
          }
        } else {
          const inputs=room.players.map(s=>{
            if(Date.now()-s.lastInput>250) {s.input=0;s.pending=0;}
            const bits=s.input|s.pending;s.pending=0;return bits;
          });
          room.sim.step(inputs[0],inputs[1]);
        }
        if(ticks%3===0)snapshot(room);
      }
    }
  },8);
  const cleanup=setInterval(()=>{
    const now=Date.now();
    for(const ws of wss.clients) {if(!ws.alive)ws.terminate();else {ws.alive=false;ws.ping();}}
    for(const room of rooms.values()) {
      // Private rooms wait for the friend half an hour.
      if((!room.sim && now-room.updated>30*60000) || (room.sim?.state().phase===3 && now-room.updated>10*60000)) {
        broadcast(room,{type:'expired'});removeRoom(room);
      }
    }
    for(const [key,s] of sessions)if(!s.ws && now-s.seen>60000){leave(s);sessions.delete(key);users.delete(s.user.id);}
  },5000);
  return {server, league, async close() {clearInterval(clock);clearInterval(cleanup);for(const ws of wss.clients)ws.terminate();wss.close();await new Promise(r=>server.close(r));try{league.flush();}catch(e){console.warn('league save',e.message);}}};
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const dev=process.argv.includes('--dev');
  const app=await createArena({dev});
  const host=process.env.HOST || '127.0.0.1', port=Number(process.env.PORT || 8080);
  app.server.listen(port,host,()=>console.log(`PULSE Arena: http://${host}:${port} (${dev?'LOCAL DEV — guest identities':'Telegram auth'})`));
  for(const signal of ['SIGINT','SIGTERM'])process.on(signal,async()=>{await app.close();process.exit(0);});
}

