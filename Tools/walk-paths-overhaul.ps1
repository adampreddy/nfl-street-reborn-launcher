# Walk the 2026-09-12 layout in the REAL launcher window, on a scratch data root:
#
#   1. a fake 2.1 install sitting AT the beta data root (User with a memory card
#      and the 2.1 Dolphin.ini full of absolute paths, Games\NSR.iso as a
#      hardlink of the real 2.1 disc, a JSGME MODS folder, the 2.1 Dolphin exe),
#   2. a Launcher-era root beside it with a valid settings file and one
#      library file, so adoption has something to move,
#   3. the payload launcher started with APPDATA pointed at the scratch folder
#      and the WebView2 debugging port open,
#   4. Tools\drive-paths-walk.mjs over CDP: tuck and adopt observed, "I already
#      have Reborn" from Reborn 2.1, the wizard to Play, prepare_launch, then
#      the readbacks (path law, composed folder, memory card), then a real
#      launch, a screenshot of the game, a graceful close, and Dolphin.ini
#      read back once more.
#
#   .\Tools\walk-paths-overhaul.ps1                 payload = target\installer-payload
#   .\Tools\walk-paths-overhaul.ps1 -KeepLauncher   leave the window open afterwards
#
# Closes any running reborn-launcher first (single instance would hand the
# start to it). Never touches the real data roots: everything lives under
# %TEMP%\reborn-paths-walk, which is recreated on every run.
param(
    [string]$Payload = (Join-Path (Split-Path $PSScriptRoot -Parent) "target\installer-payload"),
    [switch]$KeepLauncher
)
$ErrorActionPreference = "Stop"

$scratch = Join-Path $env:TEMP "reborn-paths-walk"
if (Test-Path $scratch) { Remove-Item -Recurse -Force $scratch }
$roaming = Join-Path $scratch "Roaming"
$data = Join-Path $roaming "NFL Street Reborn Beta"
$old  = Join-Path $roaming "NFL Street Reborn Launcher Beta"
$src21 = Join-Path $env:APPDATA "NFL Street Reborn"
$card = "C:\Users\Adam\nflstreet2_clean\DOLPHIN ONLINE TEST\NFL Street Reborn Dolphin\User\GC\MemoryCardA.USA.raw"
$exe = Join-Path $Payload "reborn-launcher.exe"
foreach ($must in @($exe, "$src21\User\Config\Dolphin.ini", "$src21\Games\NSR.iso", "$src21\NFL Street Reborn Dolphin.exe", $card)) {
    if (-not (Test-Path -LiteralPath $must)) { throw "missing input: $must" }
}

# 1. The fake 2.1 install, at the beta data root itself.
New-Item -ItemType Directory -Force "$data\User\GC", "$data\User\Config", "$data\Games", "$data\MODS\!INSTLOGS", "$data\MODS\!BACKUP" | Out-Null
Copy-Item -LiteralPath "$src21\User\Config\Dolphin.ini" -Destination "$data\User\Config\Dolphin.ini"
Copy-Item -LiteralPath $card -Destination "$data\User\GC\MemoryCardA.USA.raw"
cmd /c mklink /H "$data\Games\NSR.iso" "$src21\Games\NSR.iso" | Out-Null
Copy-Item -LiteralPath "$src21\NFL Street Reborn Dolphin.exe" -Destination "$data\NFL Street Reborn Dolphin.exe"
Set-Content -LiteralPath "$data\JSGME.ini" "[JSGME]" -Encoding ASCII
Set-Content -LiteralPath "$data\MODS\JSGME.ini" "[JSGME]" -Encoding ASCII

# 2. A Launcher-era root to adopt.
New-Item -ItemType Directory -Force "$old\config", "$old\library" | Out-Null
Set-Content -LiteralPath "$old\config\settings.json" '{"schema":"nsr.settings.v1"}' -Encoding ASCII
Set-Content -LiteralPath "$old\library\adopted-marker.txt" "from the Launcher-era root" -Encoding ASCII

Write-Host "scratch:  $scratch"
Write-Host "payload:  $Payload"

# 3. One launcher at a time.
foreach ($p in Get-Process -Name reborn-launcher -ErrorAction SilentlyContinue) {
    Write-Host ("closing the running launcher {0} ({1})" -f $p.Id, $p.Path)
    Stop-Process -Id $p.Id -Force
}
Start-Sleep -Seconds 2

$env:APPDATA = $roaming
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9333"
$env:REBORN_WALK_ROOT = $data
$env:REBORN_WALK_OLD = $old
$env:REBORN_WALK_OUT = Join-Path (Split-Path $PSScriptRoot -Parent) "target\review"
$proc = Start-Process -FilePath $exe -PassThru
Write-Host ("launcher started, pid {0}; waiting for the debugging port" -f $proc.Id)
$ready = $false
for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Seconds 1
    try { $null = Invoke-WebRequest -Uri "http://127.0.0.1:9333/json/version" -UseBasicParsing -TimeoutSec 2; $ready = $true; break } catch {}
}
if (-not $ready) { Write-Host "the debugging port never answered"; if (-not $KeepLauncher) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }; exit 2 }

# 4. The walk.
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    & node (Join-Path $PSScriptRoot "drive-paths-walk.mjs")
    $code = $LASTEXITCODE
} finally {
    Pop-Location
}
Write-Host ("walk exit {0}" -f $code)
if (-not $KeepLauncher) {
    Get-Process -Name "NFL Street Reborn Dolphin" -ErrorAction SilentlyContinue | ForEach-Object { $null = $_.CloseMainWindow() }
    Start-Sleep -Seconds 3
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
}
exit $code
