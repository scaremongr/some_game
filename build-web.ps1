param([switch]$Serve, [switch]$Debug, [int]$Port = 8080)
$ErrorActionPreference = 'Continue'
$arenaRoot = $PSScriptRoot
Push-Location $arenaRoot
try {
    $profileName = if ($Debug) { 'debug' } else { 'release' }
    $buildArgs = @('build', '--offline', '--target', 'wasm32-unknown-unknown', '--bin', 'some_game')
    if (-not $Debug) { $buildArgs += '--release' }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw '3D client build failed' }
    & cargo build --offline --release --target wasm32-unknown-unknown --manifest-path combat/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Combat core build failed' }
    $ErrorActionPreference = 'Stop'
    $arenaDist = Join-Path $arenaRoot 'dist'
    New-Item -ItemType Directory -Force -Path $arenaDist | Out-Null
    foreach ($file in @('index.html','gl.js','audio.js','pose.js','bridge.js','arena.js','arena.css','combat.js','scenery.js','sound.js')) {
        Copy-Item -LiteralPath (Join-Path $arenaRoot "web/$file") -Destination $arenaDist -Force
    }
    Copy-Item -LiteralPath (Join-Path $arenaRoot "target/wasm32-unknown-unknown/$profileName/some_game.wasm") -Destination $arenaDist -Force
    Copy-Item -LiteralPath (Join-Path $arenaRoot 'combat/target/wasm32-unknown-unknown/release/arena_combat.wasm') -Destination $arenaDist -Force
    New-Item -ItemType Directory -Force -Path (Join-Path $arenaDist 'assets') | Out-Null
    Copy-Item -LiteralPath (Join-Path $arenaRoot 'assets/character.glb') -Destination (Join-Path $arenaDist 'assets/character.glb') -Force
    $fightPack = Join-Path $arenaRoot 'assets/fight.pack'
    if (Test-Path -LiteralPath $fightPack) { Copy-Item -LiteralPath $fightPack -Destination (Join-Path $arenaDist 'assets/fight.pack') -Force }
    # Bot pictures (the /start message cover), sent by URL from the game server.
    $botArt = Join-Path $arenaRoot 'assets/bot'
    if (Test-Path -LiteralPath $botArt) {
        New-Item -ItemType Directory -Force -Path (Join-Path $arenaDist 'assets/bot') | Out-Null
        Copy-Item -Path (Join-Path $botArt '*.jpg') -Destination (Join-Path $arenaDist 'assets/bot') -Force
    }
    # The night city behind the windows (tools/backdrop/city.py).
    $backdrop = Join-Path $arenaRoot 'assets/backdrop'
    if (Test-Path -LiteralPath $backdrop) {
        New-Item -ItemType Directory -Force -Path (Join-Path $arenaDist 'assets/backdrop') | Out-Null
        Copy-Item -Path (Join-Path $backdrop '*') -Include '*.jpg', '*.png' -Destination (Join-Path $arenaDist 'assets/backdrop') -Force
    }
    $room = Join-Path $arenaRoot 'assets/room.glb'
    if (Test-Path -LiteralPath $room) { Copy-Item -LiteralPath $room -Destination (Join-Path $arenaDist 'assets/room.glb') -Force }
    # Fighter roster: roster.json, one model + clip pack + portrait per fighter.
    $fighters = Join-Path $arenaRoot 'assets/fighters'
    if (Test-Path -LiteralPath $fighters) {
        $distFighters = Join-Path $arenaDist 'assets/fighters'
        New-Item -ItemType Directory -Force -Path $distFighters | Out-Null
        Get-ChildItem -LiteralPath $fighters -File | Where-Object { $_.Extension -in '.json', '.glb', '.pack', '.jpg' } |
            ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $distFighters -Force }
    }
    Write-Host "PULSE built: $arenaDist" -ForegroundColor Green
    if ($Serve) {
        $env:PORT = "$Port"
        & node server/index.mjs --dev
    }
} finally { Pop-Location }
