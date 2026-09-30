# Deployment

The full procedure, server layout and rollback are described in
[ARCHITECTURE.md §9](../ARCHITECTURE.md#9-deploy). Short version:

```powershell
# after the full test suite passes
.\deploy.ps1                 # dist + pulse-server.zip
python scripts/publish.py    # upload over SSH, install, probe, auto-rollback
```

- Game: https://serbiamarket.duckdns.org/dance/index.html
- Telegram: https://t.me/somee_game_bot?startapp=fight_home (bot `@somee_game_bot`)
- Health: https://serbiamarket.duckdns.org/dance/health
- VM: `avpetrov89@34.14.29.132`, container `pulse-arena`, network `barakholka_default`
- Releases: `/home/avpetrov89/pulse-arena/releases/<hash16>`, current one in
  `/home/avpetrov89/pulse-arena/current-release.txt`
- Data (ratings, chat tables, victory cards): `/home/avpetrov89/pulse-arena/data`
  mounted at `/app/data`
- Secrets on the VM only: marketplace bot token in `/home/avpetrov89/barakholka/.env`,
  game bot token in `/home/avpetrov89/pulse-arena/game-bot.env`, container env in
  `/home/avpetrov89/pulse-arena/.env` (all mode 600, rewritten by the installer)
- Caddy: the `# BEGIN/END PULSE ARENA` block in the marketplace's Caddyfile; a
  backup `Caddyfile.before-<ts>` is kept for every deploy
- The previous container stays as `pulse-arena-previous-<ts>` for a manual rollback
  (`docker stop pulse-arena && docker rename … && docker start …`)
