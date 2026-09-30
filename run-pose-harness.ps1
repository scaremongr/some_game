# Прогон web/pose.js в headless-Chrome с поддельной камерой.
#
#   .\run-pose-harness.ps1
#
# Зачем: путь загрузки распознавания на PC не исполняется вообще (камеры в
# нативной сборке нет), и без этого прогона любая опечатка в pose.js
# обнаруживается только на телефоне.
#
# Как устроено: локальный сервер отдаёт web/, Chrome открывает страницу
# прогона с --use-fake-device-for-media-stream (синтетический поток вместо
# камеры), а страница сообщает о ходе дела GET-запросами вида /HARNESS/...,
# которые видно в логе сервера. Это единственный канал наружу из headless,
# не требующий отладочного протокола.

param(
    [int]$Port = 8099,
    [int]$TimeoutSeconds = 240
)

$ErrorActionPreference = 'Continue'
$root = $PSScriptRoot
$web = Join-Path $root 'web'
$work = Join-Path $env:TEMP 'pose-harness'

if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Force -Path $work | Out-Null

$serverLog = Join-Path $work 'server.log'
$chromeProfile = Join-Path $work 'profile'

$chrome = @(
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $chrome) { throw "не найден Chrome или Edge" }

Write-Host "сервер на :$Port ..." -ForegroundColor Cyan
$server = Start-Process -FilePath 'python' `
    -ArgumentList @('-m', 'http.server', "$Port", '--directory', $web) `
    -PassThru -NoNewWindow -RedirectStandardError $serverLog

try {
    Write-Host "браузер: $chrome" -ForegroundColor Cyan
    $browser = Start-Process -FilePath $chrome -PassThru -ArgumentList @(
        '--headless=new',
        '--disable-gpu',
        # Вместо камеры — синтетический поток. Человека в нём нет, поэтому
        # успехом считается доезд до RUNNING, а не найденная поза.
        '--use-fake-device-for-media-stream',
        '--use-fake-ui-for-media-stream',
        '--autoplay-policy=no-user-gesture-required',
        '--no-first-run',
        "--user-data-dir=$chromeProfile",
        "http://localhost:$Port/pose-harness.html"
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $seen = @{}
    $verdict = $null

    while ((Get-Date) -lt $deadline -and -not $verdict) {
        Start-Sleep -Milliseconds 700
        if (-not (Test-Path $serverLog)) { continue }

        foreach ($line in Get-Content $serverLog) {
            if ($line -notmatch 'GET /HARNESS/(\S+?)\?') { continue }
            $message = [System.Uri]::UnescapeDataString($Matches[1])
            if ($seen.ContainsKey($message)) { continue }
            $seen[$message] = $true

            $color = if ($message -like 'HARNESS FAIL*') { 'Red' }
                     elseif ($message -like 'HARNESS OK*') { 'Green' }
                     else { 'Gray' }
            Write-Host "  $message" -ForegroundColor $color

            if ($message -like 'HARNESS *' -or $message -like 'FAIL*') { $verdict = $message }
        }
    }

    if (-not $browser.HasExited) { Stop-Process -Id $browser.Id -Force }

    if (-not $verdict) {
        Write-Host "прогон не дал результата за $TimeoutSeconds c" -ForegroundColor Red
        exit 1
    }
    if ($verdict -like 'HARNESS OK*') { exit 0 }
    exit 1
}
finally {
    if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id -Force }
}
