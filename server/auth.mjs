import { createHmac, createPublicKey, timingSafeEqual, verify } from 'node:crypto';

// Telegram's Ed25519 key for third-party validation of Mini App data
// (https://core.telegram.org/bots/webapps#validating-data-for-third-party-use).
const TELEGRAM_KEY = createPublicKey({
  key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), Buffer.from('e7bf03a2fa4602af4580703d88dda5bb59f32ed8b02a56c187fe7d34caed242d', 'hex')]),
  format: 'der', type: 'spki',
});

const sorted = params => [...params].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);

/** True when Telegram signed this data for the bot with numeric id `botId`. */
export function signedForBot(params, botId) {
  const signature = params.get('signature');
  if (!signature || !/^\d+$/.test(String(botId))) return false;
  const fields = sorted(params).filter(([k]) => k !== 'hash' && k !== 'signature');
  const check = `${botId}:WebAppData\n` + fields.map(([k, v]) => `${k}=${v}`).join('\n');
  try { return verify(null, Buffer.from(check), TELEGRAM_KEY, Buffer.from(signature, 'base64url')); } catch { return false; }
}

/**
 * Validates Mini App initData signed by any of `botTokens` (one token or a
 * list: the game's own bot and the marketplace bot that also opens it).
 * `extraBots` are numeric ids of further bots, checked by Telegram's Ed25519
 * signature (no token needed). A failure carries `detail` for the server
 * log: field names, age and whether the data came from one of our bots.
 */
export function validateTelegram(initData, botTokens, now = Date.now(), extraBots = []) {
  const tokens = (Array.isArray(botTokens) ? botTokens : [botTokens]).filter(Boolean);
  if (typeof initData !== 'string' || initData.length > 8192 || !tokens.length) throw Error('Откройте игру через Telegram.');
  const params = new URLSearchParams(initData);
  if ([...params.keys()].some((key, i, keys) => keys.indexOf(key) !== i)) throw Error('Повторяющиеся поля авторизации.');
  const hash = params.get('hash');
  const fail = message => {
    const error = Error(message);
    error.detail = {
      keys: [...params.keys()].sort(),
      age: Math.floor(now / 1000) - Number(params.get('auth_date')),
      ownBot: tokens.some(t => signedForBot(params, t.split(':')[0])),
    };
    return error;
  };
  if (!/^[a-f0-9]{64}$/i.test(hash || '')) throw fail('Неверная подпись Telegram.');
  const check = sorted(params).filter(([k]) => k !== 'hash').map(([k, v]) => `${k}=${v}`).join('\n');
  const given = Buffer.from(hash, 'hex');
  const ours = tokens.some(token => {
    const key = createHmac('sha256', 'WebAppData').update(token).digest();
    return timingSafeEqual(createHmac('sha256', key).update(check).digest(), given);
  });
  if (!ours && !extraBots.some(id => signedForBot(params, id))) throw fail('Неверная подпись Telegram.');
  const age = Math.floor(now / 1000) - Number(params.get('auth_date'));
  if (!Number.isFinite(age) || age < -30 || age > 3600) throw Error('Сессия истекла. Откройте Mini App заново.');
  const user = JSON.parse(params.get('user') || 'null');
  if (!user || !Number.isSafeInteger(user.id) || user.id <= 0 || typeof user.first_name !== 'string') throw Error('Нет пользователя Telegram.');
  return { id: String(user.id), name: user.first_name.slice(0, 32), photo: telegramPhoto(user.photo_url), dm: user.allows_write_to_pm === true };
}

/** The profile photo URL from signed user data, if it is a Telegram raster image. */
export function telegramPhoto(url) {
  try {
    const u = new URL(url);
    const host = u.hostname;
    const telegram = host === 't.me' || host.endsWith('.t.me') || host === 'telegram.org' || host.endsWith('.telegram.org') || host.endsWith('.telesco.pe');
    return u.protocol === 'https:' && telegram && !/\.svg$/i.test(u.pathname) ? u.href : null;
  } catch { return null; }
}
