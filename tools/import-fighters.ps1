# Turns Mixamo characters (assets-src/fighters/fbx/*.fbx, T-pose with skin) into
# fighters: assets/fighters/<id>.glb (textures shrunk) and <id>.pack (every clip
# from assets-src/fight/clips.txt retargeted onto that character's skeleton).
#   .\tools\import-fighters.ps1                  # every character
#   .\tools\import-fighters.ps1 -Only kachujin   # one id
# The id is the first word of the file name, lower case. Add the fighter to
# assets/fighters/roster.json, then render its portrait:
#   .\build-web.ps1; node scripts/fighter-portraits.mjs <id>
param(
    [string[]]$Only = @(),
    [int]$Texture = 1024,
    [string]$Blender = 'C:\Program Files\Blender Foundation\Blender 5.2\blender.exe'
)
# Native tools report progress on stderr; failures are checked by exit code.
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root 'assets-src/fighters/fbx'
$target = Join-Path $root 'assets/fighters'
New-Item -ItemType Directory -Force -Path $target | Out-Null
if (-not (Test-Path -LiteralPath $Blender)) { throw "Blender not found: $Blender" }

Push-Location $root
try {
    & cargo build --offline --release --bin fightpack 2>$null
    if ($LASTEXITCODE -ne 0) { throw 'fightpack build failed' }
    foreach ($fbx in Get-ChildItem -LiteralPath $source -Filter *.fbx) {
        $id = ($fbx.BaseName -split '[\s_-]+')[0].ToLowerInvariant() -replace '[^a-z0-9]', ''
        if (-not $id -or ($Only.Count -and $Only -notcontains $id)) { continue }
        $glb = Join-Path $target "$id.glb"
        $pack = Join-Path $target "$id.pack"
        Write-Host "== $id  <- $($fbx.Name)" -ForegroundColor Cyan
        & $Blender -b --factory-startup --python (Join-Path $root 'tools/fighter-to-glb.py') -- $fbx.FullName $glb $Texture 2>&1 |
            Where-Object { $_ -match '^(OK|Error|Traceback)' }
        if (-not (Test-Path -LiteralPath $glb)) { Write-Warning "${id}: conversion failed"; continue }
        & (Join-Path $root 'target/release/fightpack.exe') --character $glb (Join-Path $root 'assets-src/fight/glb') (Join-Path $root 'assets-src/fight/clips.txt') $pack |
            Select-Object -Last 1
    }
} finally { Pop-Location }
