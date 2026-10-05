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
read by the server at start). It picks the body both clients draw and the
fighting style (`style`: `allround`, `pressure` or `range`; the server calls
`arena_style(side, 0|1|2)` when the match starts, and the state carries
`fighters[i].style` and `styles`). Unknown ids keep the previous choice
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
| `state` | `state`, `paused`, `ack:[seq0,seq1]`, `arrived:[tick0,tick1]` (the tick each player's acknowledged input is applied at) |
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
1 hit, 2 block, 3 parry, 4 guard break, 5 grab, 6 counter, 7 punish, 8 breaker,
9 throw broken (tech or two grabs at once), 10 throw slam (the throw's damage).

The server runs the same isolated Rust/WASM module as browser training at 60 Hz.
It accepts inputs only; position, HP, stamina, damage, time and wins sent by
clients have no authority. Hits resolve from both pre-hit states, allowing trades.
A blow connects when its hitbox overlaps a hurtbox of the defender
(`combat/src/boxes.rs`): the body (1750 mm tall, 1100 crouching or sweeping or
rising into the uppercut, lifted with a jump; 500 mm wide) or the limb the
defender is striking with, from two frames before its active frames until
halfway through the recovery, drawing back meanwhile. Grabs only take the
body. Blow height bands above the striker's feet: jab 1250–1650, cross/hook
950–1550, kicks 800–1300, roundhouse 950–1700, overhead 1050–1700, sweep
0–280, low kick 100–500, uppercut 800–2400, air kick −150–700. Which guard
stops a blow is its height class, as before.
A 12-tick input buffer captures presses during hitstop. Early attack cancels
require a confirmed hit and an allowed transition in `combat/src/moves.rs`
(`cancel`); light strings may also continue after a block (`block_cancel`).
**Client prediction** (`web/predict.js`): the page runs the same WASM a few
ticks ahead of the server, so a press shows on the next frame. Each snapshot is
loaded into the client copy (`arena_alloc` + `arena_load`) and the inputs the
server has not applied yet are replayed on top (rollback); the opponent is
assumed to keep holding their last input (`fighters[i].previous`). The clock
is tuned from `ack` + `arrived`: an input made for tick t should be applied at
tick t − 1. The result screen waits for the server's phase 3. The server stays
the only authority and has no rollback of its own: a late input is applied when
it arrives. `?predict=0` turns prediction off (snapshots rendered `NET_DELAY`
ticks behind, interpolated). The HUD flags RTT above 180 ms. `createArena({dev,
latency})` (or `PULSE_DEV_LATENCY`) simulates a round trip in dev for tests.

## Combat and physics in v3

Frame data (startup, active, recovery, reach, height, damage, stun, costs) lives
in `combat/src/moves.rs` and also drives animation timing. Action IDs:
0 idle, 1 jab, 2 overhead, 3 dash, 4 throw, 5 hitstun, 8 kick, 9 sweep,
10 uppercut, 11 cross, 12 roundhouse, 13 air kick, 14 special, 15 knockdown,
16 low kick (crouch + jab), 17 hook (third jab of J-J-J), 18 side kick (second
kick of U-U), 19 room smash. IDs 6 and 7 are unused. Frame data per style:
`moves::attack_for(style, action)`; walking `moves::walk`, dashing
`moves::dash` (the pressure style's forward dash turns into an attack from
frame 8).

Fighters add `crouch`, `meter` (0..1000), `blockstun`, `down`, `invulnerable`,
`juggle`, `combo_damage`, `confirmed`, `prop_hit`, `air_attack`, `held` (ticks
left in a thrower's grip: the victim is pinned 600 mm in front of the thrower,
then slammed and knocked down) and `parry_cooldown` (ticks until a re-raised
guard gets its parry window again). Between rounds and after the match airborne
fighters still fall and slide to rest; nothing else moves. High jabs miss
unprotected crouching opponents; lows (low kick, sweep) beat standing guard;
overheads beat low guard; jumps evade lows. Throws cannot grab airborne,
stunned or blocking (blockstun) victims. Blocking locks recovery for the move's
blockstun and costs stamina (10 × damage); an empty bar breaks the guard. A guard
raised less than 6 ticks before the blow parries, unless it was lowered within
the last 18 ticks. Counter hits (interrupting startup) add 3 damage and 6 ticks
of hitstun; punish events identify hits during attack recovery. Only the back
dash slips through attacks (frames 2–8). The rising uppercut is out of reach of
high and air attacks while it rises.

A repeated button continues its string even on a whiff, once the active frames
end (`moves::string`: J-J-J jab, cross, hook; U-U front kick, side kick).
J-J-U chains jab/cross/roundhouse; crouch-J then crouch-K or crouch-U continues
a low kick into the uppercut or the sweep. U-K and crouch-K launch with an
uppercut. On block J-J, J-U, J(cross)-U and low-J continue. Combo damage scales
only while the victim cannot recover; a fighter hit in the air has no control
until landing (a full meter still escapes). Four air hits force landing;
knockdown lasts 56 ticks (42 after a throw's slam) and wakeup grants 12 ticks of
protection that ends when the fighter acts. A grabbed victim breaks the throw by
pressing grab within 10 ticks of the grab (event 9; both stagger apart, no
damage); otherwise the throw's damage lands with the slam (event 10). Special
costs 500 meter. Block+dash during hitstun spends a full bar to escape.
Measured advantage on block: jab −1, low kick −3, cross/kick −4, special −1;
overhead and roundhouse −11, sweep −16, uppercut −21 (`combat` tests).

Each fighter includes integer `vx`, `y`, `vy`, `recoil`, `recoil_v` and
`wall_cooldown`. Velocities use millimetres per tick; all competitive physics
runs at 60 Hz in the shared simulation. Heavy strikes and grabs launch fighters;
horizontal momentum causes wall impacts. Walking alone cannot damage a wall.

`walls:[left,right]` contains `hp` (110, permanent), `impacts`, `broken_tick`
(always 0), and `impulse`. Exterior bounds are +/-11500 mm. Internal doorways
are always open. Body impacts cause rebound and an impact event; the exterior
cannot be destroyed. Wall counters reset each round. Clients detect impacts
from counters, including across sparse snapshots.

Skeletal recoil follows authoritative state. Joint-based KO ragdolls and bouncing
fragments are cosmetic client simulations; neither can change combat outcomes.
Each side loads its fighter's GLB and clip pack on demand; the same body may
be shared by both independently posed fighters.
`objects` contains 20 ordered `{hp,broken_tick,impulse}` entries. Stable IDs,
positions and material kinds live in `combat/src/room.rs`. IDs 0-4 are permanent
shell; IDs 5-19 are furniture, glass, lamps and planters. Attacks and thrown
bodies damage nearby furnishings with a body-contact cooldown. Q/2048 smashes
nearby interior in either direction. Objects reset next round; fragments
reconstruct deterministically from snapshot tick, object ID and break tick.
Floors and doorways remain passable. Round starts cycle through five rooms;
`room::ROUND_CENTERS` defines the shared client/server starting positions.

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
proxy for public traffic. Ratings persist in `DATA_DIR`; there is no match
history or replay storage yet.

Telegram integration follows the official [initData validation](https://core.telegram.org/bots/webapps#validating-data-received-via-the-mini-app)
and [Mini App direct links](https://core.telegram.org/bots/webapps#direct-link-mini-apps).
HMAC validation includes all fields except `hash`, rejects duplicate fields and
accepts auth timestamps at most 1 hour old (30 seconds clock skew).
Only `--dev` permits guest identities; its default bind is loopback.
