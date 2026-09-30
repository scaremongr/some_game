import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createArena } from './index.mjs';
import { createGameBot } from './bot.mjs';
import { createLeague } from './league.mjs';

const settle = () => new Promise(r => setTimeout(r, 50));
const privateMsg = (text, id = 42) => ({ message: { message_id: 1, chat: { id, type: 'private' }, from: { id, first_name: 'Анна' }, text } });
const groupMsg = (text, from = { id: 42, first_name: 'Анна' }) => ({ message: { message_id: 7, chat: { id: -100, type: 'supergroup', title: 'Друзья' }, from, text } });

test('game bot: webhook secret, greeting, help, room invitation and setup', async () => {
  const calls = [];
  const telegramApi = async (method, params) => { calls.push({ method, params }); return method === 'getMe' ? { id: 123, username: 'somee_game_bot' } : true; };
  const app = await createArena({ dev: true, gameBotToken: '123:test', gameUrl: 'https://game.example/dance/', miniApp: 'https://t.me/somee_game_bot', telegramApi });
  app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
  const url = `http://127.0.0.1:${app.server.address().port}/telegram`;
  const secret = createGameBot({ token: '123:test', gameUrl: 'x/', api: async () => {} }).secret;
  const post = (update, key = secret) => fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Telegram-Bot-Api-Secret-Token': key }, body: JSON.stringify(update) });
  try {
    assert.equal((await post(privateMsg('/start'), 'wrong')).status, 403);
    assert.equal(calls.length, 0);
    assert.equal((await post(privateMsg('/start'))).status, 200); await settle();
    const greet = calls.shift();
    assert.equal(greet.method, 'sendPhoto');
    assert.equal(greet.params.chat_id, 42);
    assert.equal(greet.params.photo, 'https://game.example/dance/assets/bot/cover.jpg');
    const buttons = greet.params.reply_markup.inline_keyboard.flat();
    assert.equal(buttons[0].web_app.url, 'https://game.example/dance/index.html');
    assert.ok(buttons.some(b => b.web_app?.url.endsWith('index.html?invite=1')));
    await post(privateMsg('/start fight_ABCDEFGHIJKL')); await settle();
    assert.equal(calls.shift().params.reply_markup.inline_keyboard[0][0].web_app.url, 'https://game.example/dance/index.html?room=ABCDEFGHIJKL');
    await post(privateMsg('/help')); await settle();
    assert.match(calls.shift().params.text, /Как играть/);
    await post(privateMsg('/rating')); await settle();
    assert.match(calls.shift().params.text, /не сыграл/);
    const bot = createGameBot({ token: '123:test', gameUrl: 'https://game.example/dance/', api: async (m, p) => { calls.push({ method: m, params: p }); return m === 'getMe' ? { id: 123, username: 'b' } : true; } });
    assert.deepEqual(await bot.setup(), []);
    assert.deepEqual(calls.map(c => c.method), ['getMe', 'setWebhook', 'setChatMenuButton', 'setMyCommands', 'setMyCommands', 'setMyDescription', 'setMyShortDescription']);
    assert.equal(calls[1].params.url, 'https://game.example/dance/telegram');
    assert.equal(calls[1].params.secret_token, secret);
    assert.deepEqual(calls[1].params.allowed_updates, ['message', 'callback_query', 'my_chat_member']);
  } finally { await app.close(); }
});

test('group chats: welcome, table, join button, match news; revenge by private message', async () => {
  const calls = [];
  const league = createLeague();
  const api = async (method, params) => {
    calls.push({ method, params });
    if (method === 'sendMessage' && params.chat_id === 99) { const e = Error('blocked'); e.code = 403; throw e; }
    return method === 'getMe' ? { id: 123, username: 'somee_game_bot' } : true;
  };
  const bot = createGameBot({ token: '123:test', gameUrl: 'https://g.example/dance/', appLink: 'https://t.me/somee_game_bot', league, api });
  await bot.setup(); calls.length = 0;
  // Added to a group: a welcome with the Join button.
  await bot.handle({ my_chat_member: { chat: { id: -100, type: 'supergroup', title: 'Друзья' }, old_chat_member: { status: 'left' }, new_chat_member: { status: 'member', user: { id: 123 } } } });
  assert.equal(calls.shift().params.reply_markup.inline_keyboard[0][0].callback_data, 'join');
  // Commands for other bots and plain chatter are ignored.
  await bot.handle(groupMsg('/top@other_bot')); await bot.handle(groupMsg('привет'));
  assert.equal(calls.length, 0);
  await bot.handle(groupMsg('/top@somee_game_bot'));
  assert.match(calls.shift().params.text, /Рейтинг чата «Друзья»/);
  assert.deepEqual(league.group('-100').members, ['42']);
  // Boris joins with the button; the table message is refreshed.
  await bot.handle({ callback_query: { id: 'q', data: 'join', from: { id: 43, first_name: 'Борис' }, message: { message_id: 5, chat: { id: -100, type: 'supergroup', title: 'Друзья' } } } });
  assert.equal(calls.shift().method, 'answerCallbackQuery');
  assert.equal(calls.shift().method, 'editMessageText');
  // A fight between two members is announced in the chat; the loser, who
  // wrote to the bot before, is offered a revenge.
  league.setDm('43', true);
  const players = [{ id: '42', name: 'Анна' }, { id: '43', name: 'Борис' }];
  const results = league.recordMatch(players[0], players[1], 0);
  await bot.matchEnded({ players, winner: 0, score: [2, 1], results });
  const revenge = calls.find(c => c.params.chat_id === 43);
  assert.match(revenge.params.text, /Бой проигран 1:2/);
  assert.equal(revenge.params.reply_markup.inline_keyboard[0][0].web_app.url, 'https://g.example/dance/index.html?revenge=42');
  const news = calls.find(c => c.params.chat_id === -100);
  assert.match(news.params.text, /Анна<\/b> 2 : 1 <b>Борис/);
  // A player who blocked the bot is remembered and not written to again.
  league.touch({ id: '99', name: 'Ира' }); league.setDm('99', true);
  assert.equal(await bot.challenge('99', 'Анна', 'ABCDEFGHIJKL'), false);
  assert.equal(league.player('99').dm, false);
});
