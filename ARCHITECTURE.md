# PULSE / Arena — architecture and working guide

A 1-on-1 3D fighting game that runs as a Telegram Mini App. This file is the
entry point for anyone (human or agent) picking the project up: what is
where, how the pieces talk, how to build, test and ship, and the traps.
Player-facing details are in [README.md](README.md) (Russian); the network
protocol in [docs/PROTOCOL.md](docs/PROTOCOL.md); bot setup in
[docs/bot-kit/README.md](docs/bot-kit/README.md).
Combat research and proposed development priorities (Russian, 2026-10-01):
[docs/COMBAT_RESEARCH.md](docs/COMBAT_RESEARCH.md).

- Live game: https://serbiamarket.duckdns.org/dance/index.html
- Telegram: https://t.me/somee_game_bot?startapp=fight_home (bot `@somee_game_bot`)
- Health: https://serbiamarket.duckdns.org/dance/health

## 1. The shape of the system

```
 Telegram client ──opens──▶ Mini App page (web/index.html + arena.js)
                                 │  UI, input, net client, local training sim
                                 │  window.* bridge ◀──▶ some_game.wasm (Rust, miniquad)
                                 │                        3D renderer, fighter animation,
                                 │                        room, effects, camera
                                 │  arena_combat.wasm (Rust combat core, also in the browser
                                 │                     for training vs a bot)
                                 ▼ WebSocket /ws (JSON)
 Node server (server/index.mjs) ── authoritative 60 Hz simulation per room using the SAME
   │                                arena_combat.wasm; snapshots 20 Hz; matchmaking, rooms,
   │                                sessions, ratings (league.mjs), bot webhook (bot.mjs),
   │                                victory-card upload, avatars, static files (dist/)
   ▼
 Telegram Bot API (bot @somee_game_bot: /start, groups, revenge DMs, prepared share messages)
```

Principles:
- **Combat is deterministic and server-authoritative.** Clients send only input
  bits; HP, positions, room damage and the winner come from the server's copy
  of `combat/`. The same wasm runs local training in the browser.
- **Frame data drives animation.** `combat/src/moves.rs` (startup/active/total
  ticks) is the single source; the renderer time-warps captured clips so the
  strike lands exactly on the hit frame.
- **Visual-only things never feed back**: ragdoll after KO, debris, camera,
  effects are client-side and cosmetic.

## 2. Repository map

| Path | What |
|---|---|
| `combat/` | Crate `arena-combat` (lib + cdylib). `lib.rs` — `Match`, `Fighter`, `step()`, hit resolution, physics, walls, room damage, throws (`held`), settle after rounds, bot AI, wasm exports `arena_reset/step/bot/forfeit/state/state_len`; `moves.rs` — frame data table + cancel rules; `room.rs` — 20 stable room object IDs (0–4 fixed exterior, 5–19 breakable). |
| `src/engine/` | Engine (from an earlier "dance" game, kept): `app.rs` scene loop on miniquad; `graphics.rs` 2D sprite batch + wrappers for 3D (`draw_skinned`, `draw_baked`, `upload_*`); `render3d.rs` pipelines & GLSL (lit skinned, double-sided, mirrored, planar stencil shadow, baked-unlit, glass); `gltf.rs` GLB loader (skin, skeleton reduction to 72 bones, clip retargeting by bone name, `load_scene` per node); `skeleton.rs` (`MAX_BONES = 72`); `physics.rs` (`Fragment` debris); `math3.rs`, `mesh3.rs`, `assets.rs`, `audio.rs`, … |
| `src/game/fight.rs` | `FightScene`: reads state JSON from the page each frame, timeline interpolation, events, camera band, effects, fighters, room; loads fighters by path (`window.arenaFighters`), the baked room, fight packs. |
| `src/game/fighter_model.rs` | `FighterModel` + `Avatar` (per body: character, rig, bones, captured clips). Picks a `Clip` from `Fighter` state, maps it to a captured take (`take_for`), warps time with marks, root motion (`Travel`), layers (walk blend, crouch upper body, uppercut rising from crouch, recoil lean, fists), bone-space cross-fades, mirroring of the right fighter. Tests pose every state. |
| `src/game/mocap.rs` | Fight pack format `PFP1` (body bones, i16 quats, 30 fps), sampling, `Marks`/`strike_marks`, `strike_limb`. |
| `src/game/anims.rs`, `body.rs` | Authored key-pose animation + IK body solver: the fallback when a pack lacks a take. |
| `src/game/ragdoll.rs` | Physical layer over the animated pose: 18 joint particles (hips pinned to the animation, the rest damped springs around it), blows push the struck part, root acceleration is felt as inertia, loose parts sag, bone lengths and the floor are kept; bones are turned to follow. `Drive` per state: full control normally, loose limbs in flight and knockdowns, limp after a KO. Cosmetic. |
| `src/game/arena_props.rs` | The room: baked apartment from `assets/room.glb` (pieces `oNN_kkk` belong to combat room object NN; `glassNN_kkk` panes; `s_*` fixed) or a box-room fallback; deterministic debris from snapshot ticks. |
| `src/game/timeline.rs`, `effects.rs`, `camera.rs` | Interpolation of snapshots, particles/sparks, camera. |
| `src/game/scenery.rs` | Pictures from the page in the room: the night city behind the windows (two layers at different depths → parallax; a dark mask continues the back wall so the city shows only in the openings) and the photos on the TV, the laptop (follows the broken desk piece) and the two bedroom canvases. Screen rectangles were measured in Blender on the built room. |
| `src/game/dance.rs`, `dancer.rs`, `mirror.rs`, `clips.rs`, `menu.rs`, `rhythm.rs` | Legacy dance game scenes (run natively with args `dance`, `mirror`, `clips`). `dancer.rs` also holds `Character` used by the fight. |
| `src/bin/fightpack.rs` | Packs Mixamo clips into a fight pack, optionally retargeted onto a given fighter (`--character`). |
| `src/bin/inspect.rs`, `beatmap.rs` | Model inspector; legacy dance tool. |
| `web/` | `index.html` (all UI markup), `arena.js` (UI, input, WebSocket client, training loop, rating/leaderboard/card/revenge UI), `arena.css`, `bridge.js` (miniquad plugin: JS↔wasm imports), `gl.js` (**patched** miniquad loader), `combat.js` (loads `arena_combat.wasm` for Node and browser), `sound.js` (WebAudio: synthesised effects + step-sequencer music, modes all/sfx/off), `scenery.js` (decodes the city layers, draws the TV/laptop/canvas pictures on canvases → `window.arenaImages`), `audio.js`, `pose.js` (legacy). |
| `server/` | `index.mjs` (HTTP + WS server, rooms, sessions, clock), `auth.mjs` (Telegram initData HMAC/Ed25519), `bot.mjs` (Telegram bot), `league.mjs` (ratings, leagues, chat tables, JSON persistence), `deploy-probe.mjs` (post-deploy check run inside the container), `*.test.mjs` (node:test). |
| `scripts/` | Build/test/deploy helpers: browser tests (Playwright), `pose-sheet.mjs`, `fight-video.mjs`, `tile-frames.mjs`, `fighter-portraits.mjs`, `bot-art.mjs`, `publish.py` + `install-server.py` (deploy), `sshconf.py`, `server-inspect.py`. |
| `tools/` | Asset pipelines: Mixamo download/convert/pack, fighters import, room build/bake (`tools/room/`), the night city (`tools/backdrop/city.py` → `assets/backdrop/far.jpg`, `near.png`), `fetch-assets.py`, `set-game-bot-token.py`. |
| `assets/` | Runtime assets copied into `dist/assets` by `build-web.ps1` (see §7 for what is in git). |
| `assets-src/` | Heavy sources (Mixamo FBX/GLB, Poly Haven models/textures). Not in git; regenerable. |
| `docs/` | Protocol, deployment notes, bot kit (pictures + BotFather steps), legacy docs. |
| `dist/` | Build output served by the server. Not in git. |

## 3. Client runtime (browser / Telegram WebView)

- `web/index.html` loads `gl.js` (miniquad loader), `audio.js`, `pose.js`,
  `bridge.js` and the module `arena.js`. `arena.js` compiles
  `arena_combat.wasm`, loads the roster, then `window.load('some_game.wasm')`
  and waits for `window.arenaModelStatus === 1` and the room (`arenaRoomStatus`).
- **State to the renderer**: `arena.js` sets `window.arenaRenderBytes` (UTF-8
  JSON of `Match`, ≤16 KB) on every accepted state; `fight.rs` reads it via the
  `fight_read` import each frame and parses with nanoserde.
- **Bridge imports** (`web/bridge.js` → `extern "C"` in `fight.rs`):
  `fight_read`, `fight_model_status`, `fight_debug_camera` (8 floats camera +
  optional clip index/time for clip review), `fight_clock` (continuous render
  tick from `window.arenaClock`), `fight_layout` (free screen band),
  `fight_fighters` (model/pack per side from `window.arenaFighters`),
  `fight_avatars` (which sides show the requested body), `fight_room_status`,
  `fight_image_info`/`fight_image_copy` (page pictures for `scenery.rs`: version,
  size, RGBA pixels per slot). `fight_layout` passes four numbers: the free
  band top/bottom and left/right (the lobby frames the fighters beside the card).
- **Smoothness**: rendering is at display rate; `arenaClock` gives a fractional
  tick; online play renders `NET_DELAY = 3.5` ticks behind the newest snapshot
  and interpolates. DPR is capped at 2 and lowered adaptively
  (`window.arenaMaxDpr`, used by patched `gl.js`).
- **Training**: `arena.js` steps `simulation()` at 60 Hz locally with
  `local.bot(1)` (or dummy/guard modes).
- **Input**: bits (see PROTOCOL) from keyboard, floating joystick
  (double flick = dash), diamond pad; edge bits latched until a tick.
- Portrait and landscape layouts; Telegram fullscreen (Bot API 8.0).

## 4. Rendering and animation details

- WebGL1 (miniquad), context patched in `web/gl.js` to request a stencil
  buffer (planar shadows). **Do not replace gl.js with upstream** without
  re-applying the patch (search "ПРАВКА ПРОЕКТА") and the DPR cap.
- Skinned mesh: bones as 3×vec4 uniforms, `MAX_BONES = 72`; models with more
  bones are reduced on load (children merged). After reduction the skeleton's
  `root_rotation` must be preserved (bug fixed for Kachujin).
- Right fighter = same model with negative X scale (placement) and a
  `CullFace::Front` pipeline; poses are solved unmirrored.
- Fight pack (`assets/fight.pack`, per fighter `assets/fighters/<id>.pack`):
  clips retargeted onto that fighter's skeleton. Keys (in
  `assets-src/fight/clips.txt`, `key = file.glb [@ from-to]`): `idle, guard,
  walk_fwd/back, crouch, crouch_walk_fwd/back, jab_s/m/l, cross, heavy (spinning
  back kick), uppercut (rising lead hand), kick, low_kick, hook, side_kick,
  sweep, roundhouse,
  special, throw, held (victim in a grip), air_kick, smash, jump/jump_fwd/
  jump_back, dash_fwd/back, block (guard held on one frame, blocked blows play
  its recoil; the crouching guard takes its upper body), hit_head, hit_gut,
  hit_light (jab), hit_side (hook), hit_spin (counter hit), gut_big (side
  kick), stagger, dizzy, hit_wall, air_hit, air_down, thrown, thrown_down,
  swept, getup, ko, victory/victory2/victory3, defeat/defeat2 (picked by the
  round number), dance…dance5 (the match winner, one after another; the sources
  are Mixamo dances from the old dance game, copied as `assets-src/fight/glb/17x *.glb`)`.
  Missing keys fall back to authored poses.
- Match end (phase 3): the winner plays the victory take, then from
  `DANCE_AFTER` turns to the camera (`DANCE_YAW`) and dances (`Clip::Dance`,
  `Travel::Sway`: sway kept, travel capped at 30 cm); `fight.rs` eases the
  camera in on the winner. The page shows a banner, then a non-blocking card.
- Marks: strikes by the striking limb's reach (`strike_marks`, `Marks::fit`,
  max speed-up 2.5×); falls by hips height; jumps by feet contact; throw by
  first two-hand reach. Knockdown lasts `KNOCKDOWN = 56` ticks, a grip
  `HOLD = 32`. A strike keeps only as much of its clip's travel as the blow
  needs to land; a whiff barely steps (≤ 15 cm). The uppercut, captured on the
  spot, rises into the opponent over its startup when it will connect.
- Guard readability: the block take pose, a cyan rim on a guarding fighter and
  a barrier flash (`Effects::shield`) where a blow meets the guard.
- Physical layer (`ragdoll.rs`) runs last, after the cross-fade: `fight.rs`
  `push_bodies` maps each event to a body part and a push (jab: head back,
  hook: head sideways, uppercut: head up, kicks: the gut, lows: the legs, a
  block: the forearms, a parry: the attacker's arms). Off in preview mode.
- Baked room: `tools/room/apartment.py` authors five open furnished rooms;
  `build_room.py` supplies Blender import/material/preview helpers. CC0 Poly Haven
  assets and seamless architectural surfaces, with 22 additional model types.
  `bake_room.py` bakes Cycles bounced light, soft shadows and AgX tone into per-room
  atlases. All PBR materials stay in place until every group has been baked.
  Fixed shell/floors never scatter; owned furnishings split into rigid pieces.
  `pipeline_baked` adds view-dependent glass Fresnel and restrained floor highlights;
  fighters have matching warm/cool lighting and projected shadows.
  Adding furniture: sources, licences and steps in section 12.

## 5. Combat core essentials (`combat/`)

- Units: millimetres and ticks (60 Hz). All five rooms are accessible through
  open doorways from the start; fighter centres are bounded at +/-11500 mm.
  The exterior never breaks. Rounds last 60 s, first to 2; successive rounds
  start in living room, kitchen, study, garden, bedroom (then repeat).
- Actions: 0 idle, 1 jab, 2 heavy (overhead), 3 dash, 4 throw, 5 hitstun,
  8 kick, 9 sweep, 10 uppercut, 11 cross, 12 roundhouse, 13 air kick,
  14 special, 15 knockdown, 16 low kick, 17 hook, 18 side kick, 19 room
  smash. Crouch + J/U/K = low kick / sweep / uppercut; a repeated button
  continues its string even on a whiff (J-J-J, U-U; `moves::string`).
- Defence rules (2026-10-05 iteration, see COMBAT_RESEARCH.md §18): parry only
  for a fresh guard (`PARRY`, `PARRY_COOLDOWN`); light strings continue on
  block (`moves::block_cancel`); no throws on a blocking victim; throw tech
  (`moves::TECH`, event 9); a juggled body has no control until landing; the
  wake-up protection ends when the fighter acts; only the back dash evades; the
  rising uppercut beats high and air attacks. Block advantage is asserted by
  the test `block_advantage_matches_the_design` — retune numbers there.
- Throw: grab reach 1150 mm, victim gets `held = HOLD` ticks pinned 600 mm in
  front of the thrower, then is slammed (damage and event 10 on the slam) and
  knocked down for `KNOCKDOWN - THROWN_EARLY`. Between rounds and after the
  match airborne fighters `settle()` (land) — nothing else moves.
- Serialization: `Match` is nanoserde JSON; new fields must be
  `#[nserde(default)]` to keep old snapshots parseable.
- Changing frame data changes gameplay for everyone: keep the authored
  animation lengths in `anims.rs` in sync (test
  `every_attack_is_authored_to_its_frame_data`).

## 6. Server (`server/index.mjs`)

- HTTP: `/health`, `/config.json` (`{dev, miniApp, protocol}`), static `dist/`,
  `/avatar/<id>.jpg` (Telegram photos of live/ranked players, fetched once and
  cached), `POST /card?t=` + `/cards/<id>.jpg` (victory cards),
  `POST /telegram` (bot webhook; checks `X-Telegram-Bot-Api-Secret-Token`).
  Behind Caddy the whole app lives under `/dance/` (`handle_path` strips it).
- WS `/ws`: first message `auth` (Telegram `initData`, or dev guests), then
  `queue/create/join/leave/rematch/input/ping/top/dm`. Server → `welcome,
  queued, room, match, state, result, top, challenge, rematch, left, lobby,
  expired, error`. Details in `docs/PROTOCOL.md`.
- **Sessions**: one per Telegram user. Opening the game again takes the
  session over (old socket closed with code 4002, the old page shows a toast).
  With `leave: true` in `auth` (page opened by a link) the old room is left
  (an unfinished fight is conceded); otherwise the player returns to their
  fight. `queue/create/join` also leave the current room first.
- **Rooms**: a room code is a meeting point — `join` to a code that no longer
  exists recreates the room with that code and the joiner waits; joining one's
  own room returns to it. Private rooms wait 30 min; finished ones are removed
  after 10 min.
- Clock: `setInterval(8 ms)` steps every room at 60 Hz; snapshot every 3 ticks;
  a disconnected player pauses the room, 15 s → forfeit. Inputs expire after
  250 ms without a packet. Limits: 100 msg/s, 12 KB messages, 200 sessions.
- Match end (`finish()` on phase 3): `league.recordMatch` → `result` to both
  (the winner gets a one-time card ticket) → `bot.matchEnded` (revenge DM to
  the loser, results to shared chat groups). Deploy-probe identities
  `800000000001/2` (`PROBES`) are never rated.
- `league.mjs`: Elo (start 1000; K 40 for the first 10 games, then 24; winner
  ≥ +1), leagues Бронза <1100 · Серебро · Золото 1250 · Платина 1400 · Алмаз 1550
  · Легенда 1700; a pair stops moving ratings after 5 games per UTC day;
  chat groups with member lists; JSON file `DATA_DIR/league.json` written
  atomically (debounced 1.5 s, flushed on close). Persistence only when
  `DATA_DIR` is set or `NODE_ENV=production`.
- `bot.mjs`: webhook bot; on start sets webhook (`message, callback_query,
  my_chat_member`), menu button «Играть», private + group commands, texts.
  Private: `/start` (photo + buttons), `/start fight_CODE`, `/help`, `/top`,
  `/rating`. Groups: `/top`, `/join`, `/leave`, «➕ Я в таблице» callback,
  welcome when added. `prepareCard` → `savePreparedInlineMessage`.
  Telegram limits: `web_app` buttons only in private chats (groups/shared
  messages use `https://t.me/<bot>?startapp=…` links); DMs only to users who
  allowed writing (initData `allows_write_to_pm`, a message to the bot, or
  `WebApp.requestWriteAccess`); 403 marks them unreachable.
- `auth.mjs`: validates `initData` against both bot tokens (game bot and the
  marketplace bot `@srb_flea_market_bot`, which also opens the game); returns
  `{id, name, photo, dm}`; failures log field names only (never the data).

## 7. Assets, licences and what is in git

- **Mixamo** (characters and animations): allowed inside the game, **not** as
  standalone files — so `assets/character.glb`, `assets/fight.pack`,
  `assets/fighters/*.glb|*.pack` and all of `assets-src/` are **not** in the
  public repository. Get them with `python tools/fetch-assets.py` (downloads
  from the live server) or rebuild them from Mixamo (README, "Анимации захвата
  движения").
- In git: code, docs, `assets/room.glb` (Poly Haven CC0 + our geometry),
  `assets/bot/*.jpg`, fighter portraits `assets/fighters/*.jpg`,
  `assets/fighters/roster.json`, `assets-src/fight/clips.txt` (the clip list),
  `tools/mixamo-fight-list.json`.
- Room furniture and materials: only CC0 or CC-BY (with `CREDITS.md`) —
  where to get them and how to add them: §12.
- Legacy dance assets (`assets/moves`, `assets/music`, `character_old.glb`,
  `character_not_rigged.glb`) are not in git and not used by the fight.
- Roster: `assets/fighters/roster.json` (`id, name, model, pack, portrait`);
  the server accepts only these ids. Add a fighter: FBX in
  `assets-src/fighters/fbx` → `.\tools\import-fighters.ps1 -Only <id>` → roster
  entry → `.\build-web.ps1` → `node scripts/fighter-portraits.mjs <id>`.

## 8. Build, run, test

Requirements: Rust (tested 1.98) with `wasm32-unknown-unknown`; Node 24;
Python 3 (+ Pillow for a few helpers); PowerShell (Windows PowerShell 5.1 or
`pwsh` 7 on Linux/macOS) for `build-web.ps1`/`deploy.ps1`; Playwright
Chromium for browser tests (`npx playwright install chromium --only-shell`,
stored in `.browsers/` via `PLAYWRIGHT_BROWSERS_PATH`); Blender 5.2 only for
asset pipelines (path `C:\Program Files\Blender Foundation\Blender 5.2\blender.exe`
in scripts).

```powershell
npm ci
python tools/fetch-assets.py                 # models + packs (not in git)
.\build-web.ps1                              # -> dist/ (both wasm + web + assets)
.\build-web.ps1 -Serve                       # + local dev server on :8080 (guest identities)
cargo test --offline --lib                   # engine/game: 63 tests
cargo test --offline --manifest-path combat/Cargo.toml   # combat: 35 tests
npm test                                     # server: 15 tests (node:test)
npm run test:browser; npm run test:combat; npm run test:physics   # Playwright, need dist/
```
(`--offline` works once dependencies are in the cargo cache; drop it on a
fresh machine.)

Visual review tools (after `build-web.ps1`):
- `node scripts/ui-shots.mjs [outDir] [lobby,fight,result]` — the page UI on a
  phone in portrait and landscape inside an emulated Telegram fullscreen (safe
  areas, header buttons drawn in). Look at it after any HUD or layout change.
- `node scripts/sound-check.mjs` — renders every effect and music track offline
  and prints peak/RMS (fails on silence or clipping). Ears still decide.
- `node scripts/pose-sheet.mjs spec.json out` — contact sheet of arbitrary
  states; spec: `{shots:[{label, f0:{…fighter fields}, f1, x:[mmL,mmR],
  camera:[eye xyz, target xyz, fov, 1, clipIndex?, time?], state:{…}}],
  fighters:[{model,pack},…]}`.
- `node scripts/fight-video.mjs outDir moves|air|ground|defense|spar|ko` (env
  `FIGHTER=<id>`) records a scripted bout; ffmpeg lives in
  `.browsers/ffmpeg-*/`; `scripts/tile-frames.mjs` tiles frames.

## 9. Deploy

Production: GCP VM `34.14.29.132`, user `avpetrov89`, Docker. The game runs as
container `pulse-arena` on network `barakholka_default` next to an unrelated
marketplace app ("barakholka") whose Caddy container `barakholka-web-1`
terminates HTTPS for `serbiamarket.duckdns.org` and proxies `/dance/*` to the
game. **Never change the marketplace** beyond the `# BEGIN/END PULSE ARENA`
block that the installer manages.

Steps (from the repo root, after tests pass):
```powershell
.\deploy.ps1                 # builds dist and packs pulse-server.zip (-SkipBuild to reuse dist)
python scripts/publish.py    # uploads via SSH and runs scripts/install-server.py on the VM
```
`install-server.py` (runs on the VM): verifies the archive hash, extracts to
`/home/avpetrov89/pulse-arena/releases/<hash16>`, reads the marketplace bot
token from `/home/avpetrov89/barakholka/.env` and the game bot token from
`/home/avpetrov89/pulse-arena/game-bot.env`, writes the container `.env`
(mode 600), builds image `pulse-arena:<hash16>`, starts the new container
(read-only FS, 512 MB, volume `/home/avpetrov89/pulse-arena/data` →
`/app/data`), health-checks, updates the Caddy block (backup
`Caddyfile.before-<ts>`), runs `server/deploy-probe.mjs` (two synthetic
signed players over public WSS), and rolls everything back on failure. The
previous container is kept as `pulse-arena-previous-<ts>`.

From another machine: put that machine's public key into the VM's
`~/.ssh/authorized_keys` (via an existing machine or the GCP console), connect
once with `ssh avpetrov89@34.14.29.132` to record the host key, then use the
same two commands. SSH settings are overridable: `PULSE_SERVER`, `PULSE_SSH`,
`PULSE_SSH_KEY` (`scripts/sshconf.py`). Game bot token: place it in
`secrets/game-bot-token.txt` (git-ignored) and run
`python tools/set-game-bot-token.py` — it never prints the token.

Standing agreement with the owner: **deploy every finished change after the
full test suite passes**, then verify the live files match `dist/` (sha256 of
`/dance/<file>`), and report the links.

## 10. Conventions and traps

- Comments in Rust/JS are English in new code, Russian in older engine code;
  UI text is Russian. Keep the surrounding style.
- Some files have **CRLF line endings and/or a UTF-8 BOM** (`server/index.mjs`
  has a BOM; `web/*.js|css|html` are CRLF). Edit with tools that keep them.
- `web/gl.js` is patched (stencil, DPR cap); `build-web.ps1` copies `web/*`
  explicitly — **new web files must be added to its list** (and new server
  files to `deploy.ps1`'s list: currently `index.mjs, auth.mjs, bot.mjs,
  league.mjs, deploy-probe.mjs`).
- The client and server ship together; clients with old pages must reload
  after a combat change (protocol v3 unchanged).
- Rebuild packs for **every** fighter after changing `clips.txt` or retargeting
  code: `fightpack` for `assets/fight.pack` and `--character` for each
  `assets/fighters/<id>.glb` (`tools/import-fighters.ps1` does both steps).
- Telegram: `initData` is valid for 1 h; a Mini App opened from a group link
  cannot use `web_app` buttons; `shareMessage` needs Bot API 8.0 on the client.
- Never print or commit tokens (`secrets/`, `.env`, server env files).
- Tests that start the server in production mode must pass `dataDir: null`
  (or rely on `NODE_ENV` not being `production`) so they don't write `data/`.

## 11. State of things and ideas

Done: mocap animation for every state, 7 fighters, baked apartment arena with
breakable furniture, mobile controls, Telegram bot, ratings/leagues,
leaderboard, victory cards, revenge calls, chat tables, session takeover.

The city behind the windows: `blender -b --factory-startup --python
tools/backdrop/city.py -- --samples 256 --width 2560` (~1.5 min on an RTX 4090)
writes `assets/backdrop/far.jpg` and `near.png`; our own procedural scene, so
the images are in git. `BACKDROP_TAN_H/V` in `scenery.rs` must match the script.

Open ideas: rollback netcode / input prediction (high ping is felt), daily
quests and streaks, tournaments, cosmetics for Telegram Stars (betting Stars on
matches is not allowed by Telegram rules), post-processing (bloom, grading),
fighter shadows on furniture, compression of `room.glb`, balance tuning with
real players.

## 12. Filling the apartment: where to get assets and how to add them

The arena is built in Blender by `tools/room/build_room.py` and baked by
`tools/room/bake_room.py` into `assets/room.glb` (§4). More and better
furniture means: download a model into `assets-src/room/models/<id>/`, place
it with one `place(...)` line, preview, bake, check in game, ship.

### 12.1 Licence rules (the repository and `room.glb` are public)

`room.glb` is committed to a public repo and served to every player, so only
assets whose licence allows **redistribution in a modified form** may go in:

| Licence | Use? | Obligations |
|---|---|---|
| CC0 / public domain | yes, preferred | none |
| CC-BY 4.0 | yes | add a line to `CREDITS.md` (title, author, link, licence) — create the file with the first such asset |
| CC-BY-SA | avoid | share-alike would pull `room.glb` under SA |
| CC-BY-NC / NC-* | no | the game may be monetised (Stars) |
| CC-BY-ND / ND-* | no | decimation, recolouring and the bake are derivatives |
| "Standard", "Editorial", store/royalty-free licences (Sketchfab Standard, TurboSquid, CGTrader, BlenderKit RF, Unity/Unreal store) | no | forbid standalone redistribution; `room.glb` in a public repo is exactly that |
| AI-generated (Meshy, Tripo, Rodin, …) | only if the service grants you the output under a free licence; check the plan | note the source in `CREDITS.md` |

Downloaded sources stay out of git (`assets-src/` is ignored); record in the
`MODELS`/`TEXTURES` lists or in comments where each one came from.

### 12.2 Sources, best first

1. **Poly Haven** — https://polyhaven.com/models (CC0, photoreal, glTF, the
   style of the current room). API, no key, but a `User-Agent` header is
   required (403 without):
   - list: `https://api.polyhaven.com/assets?t=models` (also `t=textures`,
     `t=hdris`); each asset has `categories` (`furniture, seating, table,
     shelves, lighting, decorative, electronics, appliances, containers,
     plants`, …);
   - files: `https://api.polyhaven.com/files/<id>` → `gltf["1k"]["gltf"]`
     (`url` + `include` map of `.bin` and textures). A few assets have no glTF.
   - **Easiest path**: add the id to `MODELS` (or `TEXTURES`) in
     `tools/room/fetch_polyhaven.py` and run `python tools/room/fetch_polyhaven.py`
     (skips what is already downloaded).
   - Already used: Sofa_01, ArmChair_01, Ottoman_01, CoffeeTable_01,
     round_wooden_table_01, painted_wooden_chair_01/02, painted_wooden_cabinet_02,
     vintage_cabinet_01, wooden_bookshelf_worn, potted_plant_02/04,
     modern_ceiling_lamp_01, picture frames, wall_clock, throw_pillows_01,
     wicker_basket_01, vases, mantel_clock_01, book sets, side tables, WoodenTable_02.
   - Model catalogue for a detailed flat (all have glTF, checked; see the
     current MODELS list and apartment.py for models already placed):
     seating `sofa_02, sofa_03, mid_century_lounge_chair, modern_arm_chair_01,
     GreenChair_01, Rockingchair_01, vintage_day_bed, painted_wooden_sofa,
     dining_chair_02, WoodenChair_01, bar_chair_round_01, metal_stool_01..03,
     wooden_stool_01/02, painted_wooden_stool, folding_wooden_stool`;
     tables `dining_table, coffee_table_round_01, modern_coffee_table_01/02,
     industrial_coffee_table, WoodenTable_01/03, small_wooden_table_01,
     round_wooden_table_02, painted_wooden_table, ClassicConsole_01,
     ClassicNightstand_01, painted_wooden_nightstand`; storage
     `Shelf_01, painted_wooden_shelves, wooden_display_shelves_01,
     steel_frame_shelves_01..03, drawer_cabinet, modern_wooden_cabinet,
     vintage_wooden_drawer_01, painted_wooden_cabinet`; lighting
     `Chandelier_01..03, desk_lamp_arm_01, industrial_pipe_lamp,
     industrial_wall_lamp, industrial_wall_sconce, caged_hanging_light,
     vintage_oil_lamp, lantern_chandelier_01`; electronics and kitchen
     `Television_01, television_02, boombox, cassette_player, gaming_console,
     classic_laptop, vintage_radio_transceiver, vintage_microwave,
     vintage_electric_kettle, electric_stove, CoffeeCart_01, ceiling_fan`;
     decor `ornate_mirror_01, standing_picture_frame_01/02,
     hanging_picture_frame_01, vintage_grandfather_clock_01,
     vintage_telephone_wall_clock, alarm_clock_01, chess_set, dartboard,
     marble_bust_01, concrete_cat_statue, rubber_duck_toy, vintage_suitcase,
     wooden_bowl_01/02, jug_01, brass_candleholders, wooden_candlestick,
     ceramic_vase_02..04, brass_vase_03/04, antique_ceramic_vase_01,
     wine_bottles_01, wicker_basket_02, cardboard_box_01`; plants
     `potted_plant_01, anthurium_botany_01, calathea_orbifolia_01,
     pachira_aquatica_01, fern_02, planter_pot_clay, planter_box_01..03`.
   - Textures (walls, floors, fabrics, tiles): `t=textures`; HDRIs (`t=hdris`)
     could replace the flat sky colour in `build()` for richer window light.
2. **ambientCG** — https://ambientcg.com (CC0 PBR materials: wallpaper,
   fabric, tiles, wood, plaster). API:
   `https://ambientcg.com/api/v2/full_json?type=Material&q=wallpaper&include=downloadData`;
   download `https://ambientcg.com/get?file=<AssetId>_1K-JPG.zip` (e.g.
   `Wallpaper001A_1K-JPG.zip`). Unzip into
   `assets-src/room/textures/<AssetId>/`; `surface("<name>", "<AssetId>", …)`
   finds `_Color`/`_Roughness` files as well as Poly Haven's `_diff_`/`_rough_`.
3. **Sketchfab** — https://sketchfab.com (huge choice; licence per model).
   Search with a licence filter (no key needed):
   `https://api.sketchfab.com/v3/search?type=models&downloadable=true&license=cc0&q=refrigerator`
   (`license=by` for CC-BY). Download needs the owner's Sketchfab API token
   (account settings → Password & API): `GET
   https://api.sketchfab.com/v3/models/<uid>/download` with header
   `Authorization: Token <token>` → `gltf.url` (a zip with `scene.gltf`,
   `scene.bin`, `textures/`). Unzip into `assets-src/room/models/<id>/`. Check
   the face count (`faceCount` in search results) and prefer realistic,
   PBR-textured models; many are game-ready low-poly.
4. **Kenney** (https://kenney.nl, CC0) and **Quaternius** (CC0) — furniture
   kits, but stylised/low-poly: they clash with the photoreal room. Only for
   deliberate stylistic changes.
5. **Build it** — walls, counters, shelves, frames, doors and anything boxy
   are made in code with `box(...)` / `grid_wall(...)` in `build_room.py` using
   CC0 textures; procedural materials (see `rug_material()`) cover rugs,
   posters and patterns. Iconic sitcom details (a yellow frame around the door
   peephole, a coat rack, a foosball table, bar stools at the counter,
   mismatched dining chairs) are cheap this way. Evoke the style; do not
   copy a real TV set or logos one to one.

### 12.3 Adding a model

1. Download into `assets-src/room/models/<id>/` (one folder per model with a
   `.gltf` or `.glb`; `place()` imports the first one it finds).
2. In `build_apartment()` (`tools/room/apartment.py`) add, e.g.:
   ```python
   asset("Television_01", 5.2, -2.0, ("h", 0.55), "study", owner=12, y=0.9, tris=3000)
   ```
   `place(asset, owner, x, z, rot_deg, fit, y=0, tint=None, recolor=None,
   nometal=False, glow=None, tris=5000)`: `fit` scales to a height/width/depth
   in metres (`("h", 0.55)`); `rot` 0 faces the camera; `tint` multiplies the
   colour, `recolor=((r,g,b), gain)` changes hue keeping the texture,
   `nometal` fixes models that bake too dark, `glow={"part": (color, strength)}`
   makes material parts emissive (lamps); `tris` is the decimation budget.
3. **Coordinates** (game space, metres): x right, y up, z towards the camera.
   Back wall z = -3.42. The fighting lane is z = 0: furniture stays behind it.
   Rooms: garden [-12,-8], kitchen [-8,-3], living [-3,3], study [3,8],
   bedroom [8,12]. Returns between rooms end at z = -1.5, leaving free passages.
   The helper `asset(..., height=...)` can correct an imported table's height
   while keeping its authored width. `tris` is a total model budget, divided
   proportionally between submeshes.
4. **Owner**: -1 = fixed; breakable IDs use `combat/src/room.rs` as the source
   of truth for x/z. 5/6 windows, 7 coffee table, 8 side table, 9 kitchen chair,
   10 study chair, 11 kitchen cabinet, 12 desk, 13/14 sconces, 15 planter,
   16 chess table, 17 nightstand, 18 garden shelf, 19 bedroom cabinet.
   IDs 0-4 are fixed shell; legacy wall state 20/21 remains permanent too.
   Static `bake_group` values separate each room's shell, floor and furniture
   to retain texture detail. A new independent breakable needs coordinated
   updates to LAYOUT, renderer, tests and both client/server deployment.
5. **Budgets** (phones): five rooms total about 143k triangles and 14 MB
   with normals and baked lighting. Keep a prop at 1.5-6k triangles (`tris=`),
   the room under ~200k and the file under ~16 MB. Rigid room geometry uses
   one bone lookup per vertex instead of four weighted lookups. Texture resolution comes from the bake: `DETAIL` and
   `LIMIT` per group in `bake_room.py` (texels per metre, max side).
6. Preview (seconds, Cycles on GPU):
   `blender -b --factory-startup --python tools/room/build_room.py -- --preview out.png [--view living|kitchen|garden|study|bedroom|overview] [--samples 96]`
   — look at the picture; fix scale, rotation, colour.
7. Bake (~6-8 min at 128 samples on an RTX 4090; much slower on CPU):
   `blender -b --factory-startup --python tools/room/bake_room.py -- --out assets/room.glb --samples 128 --density 180`.
8. Check in game: `.\build-web.ps1`, then a pose sheet or
   `node scripts/fight-video.mjs <dir> moves` (breaks things) and look at the
   frames: nothing in the lane, pieces fly sensibly, no black faces. Run the
   tests (`arena_props` checks that every breakable object has pieces), deploy,
   commit (`room.glb` included; add `CREDITS.md` lines for CC-BY).
