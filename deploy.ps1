# Build a reviewable server release. Deployment now needs a Node service, not just static files.
param([switch]$SkipBuild, [string]$OutputPath = 'pulse-server.zip')
$ErrorActionPreference = 'Stop'
$releaseRoot = $PSScriptRoot
if (-not $SkipBuild) { & (Join-Path $releaseRoot 'build-web.ps1') }
$releaseStage = Join-Path $releaseRoot ('artifacts/release-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $releaseStage | Out-Null
foreach ($folder in @('dist','server','web','docs')) {
    New-Item -ItemType Directory -Force -Path (Join-Path $releaseStage $folder) | Out-Null
}
New-Item -ItemType Directory -Force -Path (Join-Path $releaseStage 'dist/assets') | Out-Null
Copy-Item -LiteralPath (Join-Path $releaseRoot 'dist/assets/character.glb') -Destination (Join-Path $releaseStage 'dist/assets/character.glb')
$releasePack = Join-Path $releaseRoot 'dist/assets/fight.pack'
if (Test-Path -LiteralPath $releasePack) { Copy-Item -LiteralPath $releasePack -Destination (Join-Path $releaseStage 'dist/assets/fight.pack') }
foreach ($roomFile in @('room.glb', 'room-hd.glb', 'room_detail.jpg')) {
    $releaseRoom = Join-Path $releaseRoot "dist/assets/$roomFile"
    if (Test-Path -LiteralPath $releaseRoom) { Copy-Item -LiteralPath $releaseRoom -Destination (Join-Path $releaseStage "dist/assets/$roomFile") }
}
$releaseBot = Join-Path $releaseRoot 'dist/assets/bot'
if (Test-Path -LiteralPath $releaseBot) { Copy-Item -LiteralPath $releaseBot -Destination (Join-Path $releaseStage 'dist/assets') -Recurse }
$releaseBackdrop = Join-Path $releaseRoot 'dist/assets/backdrop'
if (Test-Path -LiteralPath $releaseBackdrop) { Copy-Item -LiteralPath $releaseBackdrop -Destination (Join-Path $releaseStage 'dist/assets') -Recurse }
$releaseFighters = Join-Path $releaseRoot 'dist/assets/fighters'
if (Test-Path -LiteralPath $releaseFighters) { Copy-Item -LiteralPath $releaseFighters -Destination (Join-Path $releaseStage 'dist/assets') -Recurse }
foreach ($file in @('index.html','gl.js','audio.js','pose.js','bridge.js','arena.js','arena.css','combat.js','scenery.js','sound.js','some_game.wasm','arena_combat.wasm')) {
    Copy-Item -LiteralPath (Join-Path $releaseRoot "dist/$file") -Destination (Join-Path $releaseStage 'dist')
}
foreach ($file in @('index.mjs','auth.mjs','bot.mjs','league.mjs','deploy-probe.mjs')) {
    Copy-Item -LiteralPath (Join-Path $releaseRoot "server/$file") -Destination (Join-Path $releaseStage 'server')
}
Copy-Item -LiteralPath (Join-Path $releaseRoot 'web/combat.js') -Destination (Join-Path $releaseStage 'web')
foreach ($file in @('package.json','package-lock.json','.env.example','README.md')) {
    Copy-Item -LiteralPath (Join-Path $releaseRoot $file) -Destination $releaseStage
}
foreach ($file in @('PROTOCOL.md','Caddyfile.example')) {
    Copy-Item -LiteralPath (Join-Path $releaseRoot "docs/$file") -Destination (Join-Path $releaseStage 'docs')
}
$releaseOutput = if ([IO.Path]::IsPathRooted($OutputPath)) { $OutputPath } else { Join-Path $releaseRoot $OutputPath }
# Portable ZIP paths: Windows PowerShell Compress-Archive emits backslashes,
# which some Linux extractors treat as part of the filename.
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$releaseStream = [IO.File]::Open($releaseOutput, [IO.FileMode]::Create)
$releaseArchive = New-Object IO.Compression.ZipArchive($releaseStream, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($file in Get-ChildItem -LiteralPath $releaseStage -Recurse -File -Force) {
        $relative = $file.FullName.Substring($releaseStage.Length + 1).Replace('\', '/')
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($releaseArchive, $file.FullName, $relative) | Out-Null
    }
} finally { $releaseArchive.Dispose(); $releaseStream.Dispose() }
Write-Host "Server release: $releaseOutput" -ForegroundColor Green
Write-Host 'On server: npm ci --omit=dev; configure .env; node --env-file=.env server/index.mjs'
Write-Host 'HTTPS reverse proxy example: docs/Caddyfile.example'
