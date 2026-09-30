# Working on PULSE / Arena (agents)

Read [ARCHITECTURE.md](ARCHITECTURE.md) first: map of the code, runtime, server,
assets, build, tests, deploy and traps. Protocol: [docs/PROTOCOL.md](docs/PROTOCOL.md).

Rules agreed with the owner:
- The owner writes in Russian; answer in Russian. UI text is Russian.
- After a finished change: run the full test suite (ARCHITECTURE.md §8), then
  deploy (`.\deploy.ps1`, `python scripts/publish.py`) without asking, verify the
  live files and report the Telegram link https://t.me/somee_game_bot?startapp=fight_home.
- Never print, log or commit bot tokens or other secrets.
- Mixamo-derived files (character/fighter `.glb`, `.pack`, `assets-src/`) must not
  be committed: the repository is public. `python tools/fetch-assets.py` restores them.
- Check visuals by rendering (pose sheets, fight videos) and looking at the frames,
  not only by tests.
