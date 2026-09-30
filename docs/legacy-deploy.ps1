# Сборка и синхронизация с сервером.
#
#   .\deploy.ps1              # собрать и отправить изменившееся
#   .\deploy.ps1 -SkipBuild   # только отправить, ничего не пересобирая
#   .\deploy.ps1 -Prune       # ещё и удалить с сервера лишнее
#   .\deploy.ps1 -All         # отправить всё заново, не сверяя
#
# Отправляется только то, что отличается: суммы файлов на сервере сверяются
# с локальными, и по сети уезжают лишь новые и изменившиеся. Иначе каждая
# правка одной строки кода тянула бы за собой десяток мегабайт ассетов.
#
# Игра кладётся в ПОДПАПКУ внутри каталога уже работающего приложения и
# раздаётся его же Caddy. Конфиг веб-сервера при этом не трогается вообще:
# последний `handle` в Caddyfile отдаёт из /srv что угодно. Это осознанно -
# на том же домене крутится боевое приложение, и ронять его из-за опечатки
# в конфиге ради тестового деплоя не стоит.

param(
    [switch]$SkipBuild,
    [switch]$Prune,
    [switch]$All,
    [string]$Server = "avpetrov89@34.14.29.132",
    [string]$Key = "$env:USERPROFILE\.ssh\id_ed25519",
    [string]$RemoteDir = "barakholka/frontend/dist/dance",
    [string]$Url = "https://serbiamarket.duckdns.org/dance/index.html"
)

# ssh пишет прогресс в stderr, а 'Stop' превратил бы это в фатальную ошибку.
$ErrorActionPreference = 'Continue'
$root = $PSScriptRoot

if (-not (Test-Path $Key)) { throw "нет ключа: $Key" }

if (-not $SkipBuild) {
    & (Join-Path $root 'build-web.ps1')
    if ($LASTEXITCODE -ne 0) { throw "сборка не удалась" }
}

$dist = Join-Path $root 'dist'
if (-not (Test-Path (Join-Path $dist 'some_game.wasm'))) { throw "в dist нет сборки" }

$sshArgs = @('-i', $Key, '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=no', $Server)

# Путь для bash из Git for Windows: `C:\x` -> `/c/x`.
function ConvertTo-UnixPath([string]$path) {
    $unix = $path -replace '\\', '/'
    if ($unix -match '^([A-Za-z]):(.*)$') {
        return '/' + $Matches[1].ToLower() + $Matches[2]
    }
    return $unix
}

# --- что лежит локально ---
$listFile = Join-Path $dist '.upload.txt'
if (Test-Path $listFile) { Remove-Item $listFile -Force }

$local = @{}
foreach ($file in Get-ChildItem $dist -Recurse -File) {
    $relative = $file.FullName.Substring($dist.Length + 1) -replace '\\', '/'
    $local[$relative] = (Get-FileHash $file.FullName -Algorithm MD5).Hash.ToLower()
}
Write-Host ("локально: {0} файлов" -f $local.Count) -ForegroundColor Cyan

& ssh @sshArgs "mkdir -p ~/$RemoteDir"
if ($LASTEXITCODE -ne 0) { throw "не создать папку на сервере" }

# --- что уже лежит на сервере ---
$remote = @{}
if (-not $All) {
    $listing = & ssh @sshArgs "cd ~/$RemoteDir && find . -type f -exec md5sum {} + 2>/dev/null; true"
    foreach ($line in $listing) {
        if ($line -match '^([0-9a-f]{32})\s+\./(.+)$') {
            $remote[$Matches[2]] = $Matches[1]
        }
    }
    Write-Host ("на сервере: {0} файлов" -f $remote.Count) -ForegroundColor Cyan
}

# --- разница ---
$changed = @()
$bytes = 0
foreach ($path in $local.Keys) {
    if ($remote.ContainsKey($path) -and $remote[$path] -eq $local[$path]) { continue }
    $changed += $path
    $bytes += (Get-Item (Join-Path $dist ($path -replace '/', '\'))).Length
}
$extra = @($remote.Keys | Where-Object { -not $local.ContainsKey($_) })

if ($changed.Count -eq 0) {
    Write-Host "всё совпадает, отправлять нечего" -ForegroundColor Green
} else {
    Write-Host ("к отправке: {0} файлов, {1:N1} MB" -f $changed.Count, ($bytes / 1MB)) -ForegroundColor Cyan
    foreach ($path in ($changed | Sort-Object)) { Write-Host "  + $path" -ForegroundColor DarkGray }

    # tar читает список из файла, по имени на строку — так переживаются
    # пробелы в именах вроде "Cake by the Ocean.ogg".
    [System.IO.File]::WriteAllText($listFile, (($changed | Sort-Object) -join "`n") + "`n", (New-Object System.Text.UTF8Encoding $false))

    # Передача идёт через отдельный sh-файл: пробрасывать вложенные кавычки
    # из PowerShell в bash и дальше в ssh - верный способ что-нибудь потерять.
    $script = Join-Path $root 'deploy-upload.sh'
    @"
set -e
cd '$(ConvertTo-UnixPath $dist)'
tar -czf - --no-recursion -T .upload.txt | ssh -i '$(ConvertTo-UnixPath $Key)' -o BatchMode=yes -o StrictHostKeyChecking=no $Server 'tar -C ~/$RemoteDir -xzf -'
"@ | Set-Content -Path $script -Encoding ascii

    # В PATH `bash` — это обычно WSL, а у него диски смонтированы как /mnt/c,
    # и путь /c/... он не найдёт. Нужен именно bash из Git for Windows.
    $bash = @(
        "$env:ProgramFiles\Git\bin\bash.exe",
        "${env:ProgramFiles(x86)}\Git\bin\bash.exe",
        "$env:LOCALAPPDATA\Programs\Git\bin\bash.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $bash) { throw "не найден bash из Git for Windows" }

    & $bash (ConvertTo-UnixPath $script)
    $transferred = $LASTEXITCODE
    Remove-Item $script -ErrorAction SilentlyContinue
    Remove-Item $listFile -ErrorAction SilentlyContinue
    if ($transferred -ne 0) { throw "передача не удалась" }
}

# --- лишнее на сервере ---
if ($extra.Count -gt 0) {
    Write-Host ("лишнее на сервере: {0} файлов" -f $extra.Count) -ForegroundColor Yellow
    foreach ($path in ($extra | Sort-Object)) { Write-Host "  - $path" -ForegroundColor DarkGray }
    if ($Prune) {
        # Каждый файл называется поимённо. Никаких масок: на этом сервере
        # рядом живёт боевое приложение, и `rm` с шаблоном тут неуместен.
        foreach ($path in $extra) {
            & ssh @sshArgs "rm -f -- ~/$RemoteDir/'$path'"
        }
        Write-Host "удалено" -ForegroundColor Yellow
    } else {
        Write-Host "  (удалить: .\deploy.ps1 -SkipBuild -Prune)" -ForegroundColor DarkGray
    }
}

Write-Host "проверяю..." -ForegroundColor Cyan
$code = (& ssh @sshArgs "curl -s -o /dev/null -w '%{http_code}' $Url")
if ($code -eq '200') {
    Write-Host "готово: $Url" -ForegroundColor Green
} else {
    Write-Host "страница ответила $code - проверьте вручную: $Url" -ForegroundColor Yellow
}
