# Build the Windows installer.
#
#   .\build.ps1 -Version 3.0.0
#
# Stages a payload from the workspace build output, then compiles. Nothing is
# hardcoded to one machine, which the old rebornsetupv10.iss was: it pointed at
# C:\Users\Adam\Desktop for every source file.

param(
    [string]$Version = "3.0.0",
    # Where the Reborn Dolphin build lives. Copied in whole, minus its User
    # folder: shipping a User tree into the install root is exactly how the
    # install root became the data root in 2.1.
    [string]$Runtime = "",
    # Game images. However many are given, all install. There is no fixed list
    # and no cap.
    [string[]]$GameImages = @()
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path "$PSScriptRoot\..\.."
$payload = Join-Path $root "target\installer-payload"
$dist = Join-Path $root "dist"

Write-Host "Building release binaries"
Push-Location $root
cargo build --release -p reborn-ui -p reborn-cli
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
Pop-Location

if (Test-Path $payload) { Remove-Item -Recurse -Force $payload }
New-Item -ItemType Directory -Force $payload | Out-Null

Copy-Item (Join-Path $root "target\release\reborn-launcher.exe") $payload
Copy-Item (Join-Path $root "target\release\rebornctl.exe") $payload

if ($Runtime -and (Test-Path $Runtime)) {
    Write-Host "Staging runtime from $Runtime"
    $dest = Join-Path $payload "runtime"
    New-Item -ItemType Directory -Force $dest | Out-Null
    # -Exclude User on purpose. See the note in the param block.
    Get-ChildItem $Runtime -Exclude "User" | Copy-Item -Destination $dest -Recurse -Force
} else {
    Write-Host "No runtime staged. The launcher will ask for one in Settings."
}

if ($GameImages.Count -gt 0) {
    $games = Join-Path $payload "Games"
    New-Item -ItemType Directory -Force $games | Out-Null
    foreach ($g in $GameImages) {
        if (Test-Path $g) {
            Copy-Item $g $games
            Write-Host "Staged game image: $(Split-Path $g -Leaf)"
        } else {
            Write-Warning "Game image not found, skipped: $g"
        }
    }
}

$size = (Get-ChildItem $payload -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ("Payload: {0} file(s), {1:N1} MB" -f `
    (Get-ChildItem $payload -Recurse -File).Count, ($size / 1MB))

$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (-not (Test-Path $iscc)) { throw "Inno Setup 6 not found at $iscc" }

New-Item -ItemType Directory -Force $dist | Out-Null
& $iscc (Join-Path $PSScriptRoot "reborn-launcher.iss") `
    "/DPayload=$payload" "/DOutDir=$dist" "/DAppVersion=$Version"
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

Get-ChildItem $dist -Filter "*.exe" | ForEach-Object {
    Write-Host ("Built {0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB))
}
