// The game's Telegram bot. Private chat: greeting with the game art and a
// Play button, /top, /rating, invitations to a room, revenge calls after a
// lost fight. Group chats: a table of the chat's players (/top, /join), a
// Join button, and fight results between members. Updates arrive by webhook
// at <game url>/telegram; the menu button, commands and texts are set on start.
import { createHash, randomBytes } from 'node:crypto';

const DESCRIPTION = [
  'PULSE — файтинг один на один прямо в Telegram.',
  '',
  '⚡ Бои с живыми соперниками в реальном времени',
  '🏆 Рейтинг и лиги: от Бронзы до Легенды',
  '👥 Добавь бота в чат с друзьями — у чата будет своя таблица',
  '🛋️ Разрушаемая квартира: мебель, стены и окна ломаются по ходу боя',
  '',
  'Нажми «Играть», чтобы найти соперника или потренироваться с ботом.',
].join('\n');
const ABOUT = 'Файтинг 1 на 1 в Telegram: живые соперники, рейтинг, лиги и таблицы чатов. Жми «Играть»!';
const HELP = [
  '<b>Как играть</b>',
  '',
  '📱 <b>Телефон:</b> левый палец — джойстик (ходьба, вверх — прыжок, вниз — присед, два быстрых движения в сторону — рывок). Правый — кнопки УДАР, НОГА, СИЛЬНЫЙ, БЛОК, ЗАХВАТ; с джойстиком вниз они превращаются в низкий удар, подсечку и апперкот.',
  '⌨️ <b>Клавиатура:</b> A/D — ходьба, W — прыжок, C — присед, J — удар, K — сильный, U — нога, S — блок, L — захват, Space — рывок.',
  '',
  '🔗 Серии: удар, удар, удар — джеб, кросс, хук; нога, нога — прямой и боковой. Удар → удар → нога сбивает с ног. Присед + удар — низкий удар, присед + нога — подсечка, присед + сильный — апперкот. Захват проходит сквозь блок; схватили — сразу жми захват, чтобы вырваться.',
  '🏆 Бой до двух побед. За победы в сети растёт рейтинг: Бронза → Серебро → Золото → Платина → Алмаз → Легенда.',
  '👥 Добавь меня в групповой чат: /top покажет, кто в чате сильнее.',
].join('\n');
const GROUP_HELP = [
  '<b>PULSE в этом чате</b>',
  '',
  '/top — таблица игроков чата',
  '/join — встать в таблицу (или кнопка под таблицей)',
  '/leave — выйти из таблицы',
  '',
  'Бои между участниками таблицы я объявляю здесь.',
].join('\n');

export const esc = s => String(s ?? '').replace(/[&<>]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]);
const medal = rank => ['🥇', '🥈', '🥉'][rank - 1] ?? `${rank}.`;
const record = p => `${p.wins}–${p.losses}${p.draws ? '–' + p.draws : ''}`;

/**
 * `token` is the bot token; `gameUrl` the game's public base URL (ends
 * with /); `appLink` its Mini App link (https://t.me/<bot>) or ''; `league`
 * the rating store. `api` calls the Bot API (injectable for tests).
 */
export function createGameBot({ token, gameUrl, appLink = '', league = null, api }) {
  const call = api ?? (async (method, params) => {
    const r = await fetch(`https://api.telegram.org/bot${token}/${method}`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(params),
    });
    const data = await r.json().catch(() => ({}));
    if (!data.ok) {
      const error = Error(`${method}: ${data.description || r.status}`);
      error.code = data.error_code;
      throw error;
    }
    return data.result;
  });
  // Telegram echoes this in X-Telegram-Bot-Api-Secret-Token on every update.
  const secret = createHash('sha256').update('pulse-webhook:' + token).digest('hex');
  const me = { id: Number(String(token).split(':')[0]), username: null };
  const play = (url = gameUrl + 'index.html') => ({ text: '🎮 Играть', web_app: { url } });
  const invite = { text: '⚔️ Вызвать друга', web_app: { url: gameUrl + 'index.html?invite=1' } };
  // Group and shared messages cannot carry Mini App buttons: a link opens it.
  const playLink = () => ({ text: '🎮 Играть', url: (appLink || 'https://t.me/' + me.username) + '?startapp=fight_home' });
  const share = appLink ? [{ text: '📣 Поделиться игрой', url: 'https://t.me/share/url?url=' + encodeURIComponent(appLink + '?startapp=fight_home') + '&text=' + encodeURIComponent('Давай подерёмся в PULSE ⚡') }] : [];
  const throttle = new Map();
  const allowed = (key, ms) => {
    const t = Date.now();
    if ((throttle.get(key) ?? 0) > t) return false;
    throttle.set(key, t + ms);
    if (throttle.size > 5000) for (const [k, v] of throttle) if (v < t) throttle.delete(k);
    return true;
  };
  const who = from => ({ id: String(from.id), name: String(from.first_name || from.username || 'Игрок').slice(0, 32) });

  // A private message to a player; a player who has not started the bot or
  // blocked it cannot be reached, which is remembered.
  async function direct(id, params) {
    try {
      await call('sendMessage', { chat_id: Number(id), ...params });
      league?.setDm(String(id), true);
      return true;
    } catch (error) {
      if (error.code === 403 || error.code === 400) league?.setDm(String(id), false);
      return false;
    }
  }

  async function greet(chat) {
    const keyboard = [[play()], [invite], ...(share.length ? [share] : [])];
    try {
      await call('sendPhoto', {
        chat_id: chat, photo: gameUrl + 'assets/bot/cover.jpg',
        caption: '<b>PULSE ⚡ Файтинг 1 на 1</b>\n\nВыбери бойца, найди соперника или позови друга. Раунды по 60 секунд, бой до двух побед. За победы растёт рейтинг — /top.',
        parse_mode: 'HTML', reply_markup: { inline_keyboard: keyboard },
      });
    } catch {
      // The picture is optional: the buttons are what matters.
      await call('sendMessage', { chat_id: chat, text: 'PULSE ⚡ Файтинг 1 на 1. Жми «Играть»!', reply_markup: { inline_keyboard: keyboard } });
    }
  }

  function table(rows, youId) {
    if (!rows.length) return 'Пока никто не сыграл ни одного боя. Будь первым!';
    return rows.map(p => `${medal(p.rank)} ${p.id === youId ? '<b>' : ''}${esc(p.name)}${p.id === youId ? '</b>' : ''} — ${p.league.icon} ${p.rating} · ${record(p)}`).join('\n');
  }
  function groupBoard(chat) {
    const g = league?.group(chat);
    const rows = league ? league.groupTop(chat, 20) : [];
    const waiting = g ? g.members.length - rows.length : 0;
    const text = `🏆 <b>Рейтинг чата${g?.title ? ' «' + esc(g.title) + '»' : ''}</b>\n\n${table(rows)}` +
      (waiting > 0 ? `\n\n⏳ Ещё ${waiting} в таблице — ждут первого боя.` : '') +
      '\n\nБои между участниками таблицы объявляются здесь.';
    return { text, parse_mode: 'HTML', reply_markup: { inline_keyboard: [[{ text: '➕ Я в таблице', callback_data: 'join' }, { text: '🔄', callback_data: 'top' }], [playLink()]] } };
  }
  function ownCard(id) {
    const c = league.card(id);
    if (!c.games) return 'Ты ещё не сыграл ни одного боя в сети. Жми «Играть» → «Найти соперника».';
    return [
      `${c.league.icon} <b>${c.league.name}</b> · рейтинг <b>${c.rating}</b>`,
      `🏅 Место: ${c.rank} из ${c.total}`,
      `⚔️ Побед ${c.wins}, поражений ${c.losses}${c.draws ? ', ничьих ' + c.draws : ''}`,
      c.streak > 1 ? `🔥 Серия побед: ${c.streak}` : '',
    ].filter(Boolean).join('\n');
  }

  async function privateMessage(message, name, payload) {
    const chat = message.chat.id;
    const room = /^fight_([A-Za-z0-9_-]{12})$/.exec(payload)?.[1];
    league?.setDm(String(message.from.id), true);
    if (name === '/start' && room) {
      await call('sendMessage', {
        chat_id: chat, text: '⚔️ Тебя вызвали на бой! Жми кнопку, чтобы войти в комнату.',
        reply_markup: { inline_keyboard: [[{ text: '⚔️ Принять вызов', web_app: { url: `${gameUrl}index.html?room=${room}` } }]] },
      });
    } else if (name === '/help') {
      await call('sendMessage', { chat_id: chat, text: HELP, parse_mode: 'HTML', reply_markup: { inline_keyboard: [[play()]] } });
    } else if (name === '/top' && league) {
      const you = String(message.from.id);
      const rank = league.rank(you);
      const text = `🏆 <b>Лучшие бойцы PULSE</b>\n\n${table(league.top(10), you)}` +
        (rank && rank > 10 ? `\n…\n${rank}. <b>${esc(league.player(you).name)}</b> — ${league.card(you).league.icon} ${league.player(you).rating}` : '');
      await call('sendMessage', { chat_id: chat, text, parse_mode: 'HTML', reply_markup: { inline_keyboard: [[play()]] } });
    } else if (name === '/rating' && league) {
      await call('sendMessage', { chat_id: chat, text: ownCard(String(message.from.id)), parse_mode: 'HTML', reply_markup: { inline_keyboard: [[play()]] } });
    } else {
      await greet(chat);
    }
  }

  async function groupMessage(message, name) {
    const chat = String(message.chat.id);
    const title = message.chat.title || '';
    const user = who(message.from);
    if (!league) return;
    if (name === '/top' || name === '/start') {
      league.join(chat, title, user);
      await call('sendMessage', { chat_id: message.chat.id, ...groupBoard(chat) });
    } else if (name === '/join') {
      const added = league.join(chat, title, user);
      await call('sendMessage', { chat_id: message.chat.id, reply_to_message_id: message.message_id, text: added ? `✅ ${esc(user.name)} в таблице чата. /top — посмотреть.` : `${esc(user.name)}, ты уже в таблице.`, parse_mode: 'HTML' });
    } else if (name === '/leave') {
      league.leave(chat, user.id);
      await call('sendMessage', { chat_id: message.chat.id, reply_to_message_id: message.message_id, text: `👋 ${esc(user.name)} больше не в таблице чата.`, parse_mode: 'HTML' });
    } else if (name === '/help' || name === '/rating') {
      await call('sendMessage', { chat_id: message.chat.id, text: GROUP_HELP, parse_mode: 'HTML', reply_markup: { inline_keyboard: [[playLink()]] } });
    }
  }

  return {
    secret,
    me,
    /** The user's profile photo (JPEG, ~320 px) as the bot sees it, or
     * null: Mini App data only gives an SVG link, which pictures cannot use. */
    async photo(userId) {
      const r = await call('getUserProfilePhotos', { user_id: Number(userId), limit: 1 });
      const sizes = r?.photos?.[0];
      if (!sizes?.length) return null;
      const size = sizes.find(s => s.width >= 300) || sizes[sizes.length - 1];
      const file = await call('getFile', { file_id: size.file_id });
      if (!file?.file_path) return null;
      const res = await fetch(`https://api.telegram.org/file/bot${token}/${file.file_path}`, { signal: AbortSignal.timeout(6000) });
      return res.ok ? Buffer.from(await res.arrayBuffer()) : null;
    },
    /** Handles one update; resolves when replies are sent. */
    async handle(update) {
      if (update?.callback_query) {
        const q = update.callback_query;
        const chat = q.message?.chat;
        if (!chat || !league) return call('answerCallbackQuery', { callback_query_id: q.id });
        const id = String(chat.id);
        let notice = '';
        if (q.data === 'join') notice = league.join(id, chat.title || '', who(q.from)) ? 'Ты в таблице чата! Сыграй бой, чтобы появиться в рейтинге.' : 'Ты уже в таблице.';
        await call('answerCallbackQuery', { callback_query_id: q.id, text: notice || undefined });
        try { await call('editMessageText', { chat_id: chat.id, message_id: q.message.message_id, ...groupBoard(id) }); } catch { /* unchanged */ }
        return;
      }
      const member = update?.my_chat_member;
      if (member) {
        const status = member.new_chat_member?.status;
        const group = ['group', 'supergroup'].includes(member.chat?.type);
        if (group && ['member', 'administrator'].includes(status) && !['member', 'administrator'].includes(member.old_chat_member?.status)) {
          await call('sendMessage', {
            chat_id: member.chat.id, parse_mode: 'HTML',
            text: '👋 Привет! Я PULSE — файтинг 1 на 1 прямо в Telegram.\n\nЖмите «Я в таблице», сражайтесь друг с другом, а я буду вести рейтинг чата и объявлять результаты боёв.',
            reply_markup: { inline_keyboard: [[{ text: '➕ Я в таблице', callback_data: 'join' }], [playLink()]] },
          });
        } else if (group && ['left', 'kicked'].includes(status)) {
          league?.dropGroup(String(member.chat.id));
        }
        return;
      }
      const message = update?.message;
      const chat = message?.chat?.id;
      if (!Number.isSafeInteger(chat) || typeof message.text !== 'string' || !message.from) return;
      const [command, payload = ''] = message.text.trim().split(/\s+/, 2);
      const [name, target] = command.toLowerCase().split('@');
      // In groups, answer only commands (and only those addressed to us).
      if (message.chat.type === 'private') return privateMessage(message, name, payload);
      if (!name.startsWith('/') || (target && me.username && target !== me.username.toLowerCase())) return;
      return groupMessage(message, name);
    },
    /**
     * After an online match: offer the loser a revenge, announce the result
     * in chats where both players are on the table.
     */
    async matchEnded({ players, winner, score, results }) {
      if (winner === 0 || winner === 1) {
        const w = players[winner], l = players[1 - winner];
        if (league?.player(l.id)?.dm && allowed('revenge:' + l.id + ':' + w.id, 20 * 60_000)) {
          await direct(l.id, {
            parse_mode: 'HTML',
            text: `😤 Бой проигран ${score[1 - winner]}:${score[winner]}. Соперник — <b>${esc(w.name)}</b>.\nРейтинг: ${results[1 - winner].rating} (${results[1 - winner].delta})\n\nВозьмёшь реванш?`,
            reply_markup: { inline_keyboard: [[{ text: '⚔️ Реванш', web_app: { url: `${gameUrl}index.html?revenge=${encodeURIComponent(w.id)}` } }]] },
          });
        }
      }
      if (!league) return;
      for (const chat of league.sharedGroups(players[0].id, players[1].id)) {
        if (!allowed('group:' + chat, 8000)) continue;
        const line = i => `<b>${esc(players[i].name)}</b> ${results[i].league.icon} ${results[i].rating} (${results[i].delta > 0 ? '+' : ''}${results[i].delta})`;
        const head = winner < 0 ? '🤝 Ничья' : `🏆 Победа: <b>${esc(players[winner].name)}</b>`;
        try {
          await call('sendMessage', {
            chat_id: Number(chat), parse_mode: 'HTML',
            text: `🥊 <b>${esc(players[0].name)}</b> ${score[0]} : ${score[1]} <b>${esc(players[1].name)}</b>\n${head}\n\n${line(0)}\n${line(1)}`,
            reply_markup: { inline_keyboard: [[{ text: '🏆 Таблица чата', callback_data: 'top' }], [playLink()]] },
          });
        } catch (error) {
          if (error.code === 403) league.dropGroup(chat);
        }
      }
    },
    /** A revenge call: the target gets a private message with the room. */
    challenge(targetId, fromName, code) {
      if (!league?.player(targetId)?.dm || !allowed('challenge:' + targetId, 60_000)) return Promise.resolve(false);
      return direct(targetId, {
        parse_mode: 'HTML',
        text: `⚔️ <b>${esc(fromName)}</b> вызывает тебя на реванш! Комната ждёт полчаса.`,
        reply_markup: { inline_keyboard: [[{ text: '⚔️ Принять вызов', web_app: { url: `${gameUrl}index.html?room=${code}` } }]] },
      });
    },
    /** A victory card the player can share to any chat (Mini App shareMessage). */
    async prepareCard(userId, photoUrl, caption) {
      const prepared = await call('savePreparedInlineMessage', {
        user_id: Number(userId),
        result: {
          type: 'photo', id: randomBytes(12).toString('hex'), photo_url: photoUrl, thumbnail_url: photoUrl,
          photo_width: 1200, photo_height: 630, caption, parse_mode: 'HTML',
          reply_markup: { inline_keyboard: [[{ text: '⚔️ Бросить вызов', url: (appLink || 'https://t.me/' + me.username) + '?startapp=fight_home' }]] },
        },
        allow_user_chats: true, allow_group_chats: true, allow_channel_chats: true,
      });
      return prepared.id;
    },
    /** Webhook, menu button, commands and texts; each step is independent. */
    async setup() {
      const failed = [];
      try { Object.assign(me, await call('getMe', {})); } catch (error) { failed.push(error.message); }
      const steps = [
        ['setWebhook', { url: gameUrl + 'telegram', secret_token: secret, allowed_updates: ['message', 'callback_query', 'my_chat_member'] }],
        ['setChatMenuButton', { menu_button: { type: 'web_app', text: 'Играть', web_app: { url: gameUrl + 'index.html' } } }],
        ['setMyCommands', { commands: [
          { command: 'start', description: 'Играть в PULSE' }, { command: 'top', description: 'Лучшие бойцы' },
          { command: 'rating', description: 'Мой рейтинг' }, { command: 'help', description: 'Как играть' }] }],
        ['setMyCommands', { scope: { type: 'all_group_chats' }, commands: [
          { command: 'top', description: 'Рейтинг чата' }, { command: 'join', description: 'Встать в таблицу чата' },
          { command: 'leave', description: 'Выйти из таблицы' }, { command: 'help', description: 'Что я умею' }] }],
        ['setMyDescription', { description: DESCRIPTION }],
        ['setMyShortDescription', { short_description: ABOUT }],
      ];
      for (const [method, params] of steps) {
        try { await call(method, params); } catch (error) { failed.push(error.message); }
      }
      return failed;
    },
  };
}
