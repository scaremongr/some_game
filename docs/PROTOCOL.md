# PULSE protocol v3

WebSocket `/ws`, UTF-8 JSON. Production requires HTTPS/WSS, an exact
`PUBLIC_ORIGIN` match and validated Telegram `initData`. Token goes in the first
message, never the URL. Unauthenticated connections expire after 5 seconds.

## Client → server

| type | Fields | Meaning |
|---|---|---|
| `auth` | `v:3`, `initData`, optional `resume` | Authenticate / resume |
| `queue` | optional `fighter` | Join public queue |
| `create` | optional `fighter` | Create private room |
| `join` | `code`, optional `fighter` | Join private room |
| `create` + `revenge` | `revenge: <player id>` | Private room for a revenge; the opponent gets `challenge` (if online) and a bot message |
| `top` | — | Leaderboard: top 50 and the caller's standing |
| `dm` | — | The player allowed the bot to write to them (revenge calls) |
| `input` | `seq`, `bits` | Complete current input state |
| `ping` | `at` | RTT timestamp |
| `rematch` | — | Vote for rematch; both players must agree |
| `leave` | — | Cancel queue / leave room / concede active match |

`fighter` is an id from `assets/fighters/roster.json` (shipped with the client,
read by the server at start). It only picks the body both clients draw; every
fighter uses the same combat rules. Unknown ids keep the previous choice
(default `medea`).

`bits`: left=1, right=2, block=4, jab=8, heavy=16, dash=32, grab=64,
kick=128, crouch=256, jump=512, special=1024, room smash=2048.
Valid masks are 0..4095; discrete edge mask is 3832.
Inputs are sent on change and at 30 Hz. `seq` is a monotonically increasing safe
integer for the session, including after reconnect; browser initializes it from
epoch milliseconds × 1000. Duplicate/out-of-order sequences and invalid masks
are ignored. Attack edges between server ticks are latched until the next tick.
Held inputs expire after 250 ms without a fresh valid packet.

## Server → client

| type | Fields |
|---|---|
| `welcome` | `v`, `resume`, `user:{id,name}` |
| `queued` | — |
| `room` | `code` |
| `match` | `code`, `side:0\|1`, `players:[{id,name,fighter},{id,name,fighter}]` |
| `state` | `state`, `paused`, `ack:[seq0,seq1]` |
| `pong` | `at` |
| `rematch` | `votes` |
| `left` | `name` |
| `lobby`, `expired` | — |
| `error` | `message` |
| `result` | `you:{rating,delta,league,rank,total,wins,losses,draws,streak,rated,leagueUp}`, `opponent:{id,name}`, `won`, `score`, `card` (one-time upload ticket for the winner, else null) |
| `top` | `players:[{rank,id,name,avatar,rating,league,wins,losses,draws}]`, `you` |
| `challenge` | `code`, `from:{id,name,avatar,rating,league}` |

`welcome` adds `rating` (the player's standing); every player object in
`welcome`/`match` carries `avatar` (`avatar/<id>.jpg`, served by the game),
`rating` and `league` (`{id,name,icon}`: Бронза <1100, Серебро, Золото 1250,
Платина 1400, Алмаз 1550, Легенда 1700+). Ratings are Elo (start 1000, K 40 for
the first 10 games, then 24); a pair of players stops moving ratings after 5
games a UTC day. Every finished online match counts; a forfeit is a loss.

Victory card: the winner's page draws a 1200×630 JPEG and `POST`s it to
`card?t=<ticket>` (≤ 700 KB, single use, 15 minutes). The game stores it at
`cards/<id>.jpg` and returns `{prepared}` — a `savePreparedInlineMessage` id the
page passes to `Telegram.WebApp.shareMessage`. Ratings, chat tables and cards
live in `DATA_DIR` (`/app/data`, a host volume in production).

`state` is the serialized `arena_combat::Match` (`combat/src/lib.rs`), sent at
20 Hz. Positions are integer millimetres. Health is 0–100; stamina is 0–1000.
`phase`: 0 countdown, 1 fight, 2 round result, 3 final result.
`winner`: -1 tie/undecided, 0 left, 1 right. `remaining` and `phase_ticks` are
60 Hz simulation ticks. `event` is monotonic within a match; `event_kind`:
1 hit, 2 block, 3 parry, 4 guard break, 5 grab, 6 counter, 7 punish, 8 breaker.

The server runs the same isolated Rust/WASM module as browser training at 60 Hz.
It accepts inputs only; position, HP, stamina, damage, time and wins sent by
clients have no authority. Hits resolve from both pre-hit states, allowing trades.
A 12-tick input buffer captures presses during hitstop. Early attack cancels
require a confirmed hit and an allowed transition in `combat/src/moves.rs`.
Rendering smooths positions toward authoritative snapshots. There is currently
**no rollback, client combat prediction or lag compensation**: high RTT delays
attacks. The HUD flags RTT above 180 ms; a geographically close server matters.

## Combat and physics in v3

Frame data (startup, active, recovery, reach, height, damage, stun, costs) lives
in `combat/src/moves.rs` and also drives animation timing. Action IDs:
0 idle, 1 jab, 2 overhead, 3 dash, 4 throw, 5 hitstun, 8 kick, 9 sweep,
10 uppercut, 11 cross, 12 roundhouse, 13 air kick, 14 special, 15 knockdown,
19 room smash. IDs 6, 7, 16, 17, 18 are presentation-only clips.

Fighters add `crouch`, `meter` (0..1000), `blockstun`, `down`, `invulnerable`,
`juggle`, `combo_damage`, `confirmed`, `prop_hit`, `air_attack`, `held` (ticks
left in a thrower's grip: the victim is pinned 600 mm in front of the thrower,
then knocked down). Between rounds and after the match airborne fighters still
fall and slide to rest; nothing else moves. High jabs miss
unprotected crouching opponents; lows beat standing guard; overheads beat low
guard; jumps evade lows. Throws cannot grab airborne or already stunned victims.
Blocking locks recovery for the move's blockstun. Counter hits reward interrupting
startup; punish events identify hits during attack recovery.

J-J-U chains jab/cross/roundhouse. U-K and crouch-K launch with an uppercut.
Combo damage scales only while the victim cannot recover. Four air hits force
landing; knockdown lasts 36 ticks and wakeup grants 12 ticks of protection.
Special costs 500 meter. Block+dash during hitstun spends a full bar to escape.

Each fighter includes integer `vx`, `y`, `vy`, `recoil`, `recoil_v` and
`wall_cooldown`. Velocities use millimetres per tick; all competitive physics
runs at 60 Hz in the shared simulation. Heavy strikes and grabs launch fighters;
horizontal momentum causes wall impacts. Walking alone cannot damage a wall.

`walls:[left,right]` contains `hp` (initially 75), `impacts`, `broken_tick`, and
`impulse`. Intact walls constrain fighter centres to +/-3000 mm. A broken wall
extends its side to 4200 mm. Breaking a wall deals additional damage. Wall state
resets each round. Clients detect impacts from counters, including across sparse
snapshots. Reconnecting clients reconstruct debris from the break tick.

Skeletal recoil follows authoritative state. Joint-based KO ragdolls and bouncing
fragments are cosmetic client simulations; neither can change combat outcomes.
Each side loads its fighter's GLB and clip pack on demand; the same body may
be shared by both independently posed fighters.
`objects` contains 20 ordered `{hp,broken_tick,impulse}` entries. Stable IDs,
positions and material kinds live in `combat/src/room.rs`. Attacks and thrown
bodies damage furnishings; masonry resists ordinary hits, floor finish requires
a powerful smash. Q/2048 smashes nearby interior in either direction. Every
object resets at the next round. Fragments reconstruct deterministically from
snapshot tick, object ID and break tick. Collision with fragments is cosmetic;
the foundation remains walkable after floor tiles shatter.

Protocol v1/v2 clients are rejected; deploy server and browser assets together.

## Lifecycle and limits

The client stores a random 192-bit resume credential in sessionStorage.
A disconnected player gets 15 seconds to reconnect; combat pauses during that
interval, then the server records a forfeit. Credentials expire 60 seconds after
disconnect. A second socket using the same credential replaces the first.
One Telegram identity may have one active session. Resuming uses the existing
validated identity; a new auth payload cannot replace it.

Waiting private rooms expire in 5 minutes. Finished rooms are cleaned up after
10 minutes from match start. Rooms and sessions live in memory: process restart
ends existing matches. This implementation is a single server; horizontal scaling
requires sticky connections plus shared matchmaking/session storage.

Per connection: 12 KiB messages, 100 messages/second, 256 KiB output backpressure
limit, 5-second ping heartbeat. Server caps concurrent sockets/sessions at 200.
These are resource bounds, not a measured concurrency guarantee. Use a reverse
proxy for public traffic. No ratings or persistent match history yet.

Telegram integration follows the official [initData validation](https://core.telegram.org/bots/webapps#validating-data-received-via-the-mini-app)
and [Mini App direct links](https://core.telegram.org/bots/webapps#direct-link-mini-apps).
HMAC validation includes all fields except `hash`, rejects duplicate fields and
accepts auth timestamps at most 1 hour old (30 seconds clock skew).
Only `--dev` permits guest identities; its default bind is loopback.
