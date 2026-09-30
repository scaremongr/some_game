# Imports clips downloaded by tools/mixamo-download.js and rebuilds assets/fight.pack.
#   .\tools\import-mixamo.ps1                 # from the user's Downloads folder
#   .\tools\import-mixamo.ps1 -From D:\clips  # from elsewhere
# Steps: move "NNN Name.fbx" + manifest into assets-src/fight/fbx, convert every
# FBX to GLB with Blender, pack the clips listed in assets-src/fight/clips.txt.
param(
    [string]$From = (Join-Path $env:USERPROFILE 'Downloads'),
    [string]$Blender = 'C:\Program Files\Blender Foundation\Blender 5.2\blender.exe'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$fbx = Join-Path $root 'assets-src/fight/fbx'
$glb = Join-Path $root 'assets-src/fight/glb'
New-Item -ItemType Directory -Force -Path $fbx, $glb | Out-Null

$moved = 0
foreach ($file in Get-ChildItem -LiteralPath $From -File | Where-Object { $_.Name -match '^\d{3} .+\.fbx$' -or $_.Name -like 'pulse-mixamo-manifest*.json' }) {
    Move-Item -LiteralPath $file.FullName -Destination (Join-Path $fbx $file.Name) -Force
    $moved++
}
Write-Host "Moved $moved files into $fbx"

if (-not (Test-Path -LiteralPath $Blender)) { throw "Blender not found: $Blender" }
& $Blender -b --factory-startup --python (Join-Path $root 'tools/fbx-to-glb.py') -- $fbx $glb 2>&1 |
    Where-Object { $_ -match '^(OK|FAIL|converted)' }

Push-Location $root
try {
    & cargo run --offline --release --bin fightpack -- $glb (Join-Path $root 'assets-src/fight/clips.txt') (Join-Path $root 'assets/fight.pack')
} finally { Pop-Location }
