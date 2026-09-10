# Push a launcher update to the beta channel, start to finish:
#
#   .\Tools\publish-beta-launcher.ps1 -Version 3.0.1-beta.8
#
# Builds the launcher-only installer for the beta channel, checks the
# manifest names exactly the file that was built, uploads the file and THEN
# the manifest (a launcher that checks in between sees the old manifest, never
# a manifest pointing at a file that is not there yet), and reads both back
# from the public host to prove they are what was sent.
#
# Every beta install checks latest-beta.json on start and updates itself.
# Stable players never see this: they read latest.json, which the Publisher's
# Launcher page owns.
#
# -DryRun builds and verifies but uploads nothing.
#
# Run from PowerShell in the repository root (or a worktree of it). The
# version must sort above the one on the bucket, or nobody is offered it:
# 3.0.1-beta.8 beats 3.0.1-beta.7; a plain 3.0.1 would beat both.

param(
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$DryRun,
    [string]$Rclone = "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\Rclone.Rclone_Microsoft.Winget.Source_8wekyb3d8bbwe\rclone-v1.75.1-windows-amd64\rclone.exe",
    [string]$RcloneConfig = "$env:USERPROFILE\Reborn Keys\rclone.conf",
    [string]$Bucket = "r2:reborn-content/launcher",
    [string]$PublicBase = "https://files.nflstreetreborn.com/launcher/"
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$dist = Join-Path $root "dist"
$exeName = "NFL Street Reborn Beta Launcher Update $Version.exe"

if ($Version -notmatch '^\d+\.\d+\.\d+-beta\.\d+$') {
    throw "Version must look like 3.0.1-beta.8 (this script is for the beta channel only). Nothing was built."
}

# 1. Build. Output goes to files: under Windows PowerShell 5.1 a redirected
#    cargo stderr becomes ErrorRecords and kills the build.
$log = Join-Path $env:TEMP "publish-beta-launcher-$Version.out.log"
$err = Join-Path $env:TEMP "publish-beta-launcher-$Version.err.log"
Write-Host "Building $exeName (a few minutes)..."
$p = Start-Process powershell.exe -ArgumentList @(
    "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$root\installer\windows\build.ps1`"",
    "-LauncherOnly", "-Channel", "beta", "-Version", $Version
) -WorkingDirectory $root -Wait -PassThru -NoNewWindow -RedirectStandardOutput $log -RedirectStandardError $err
if ($p.ExitCode -ne 0) {
    Get-Content $log | Select-Object -Last 15
    if (Test-Path $err) { Get-Content $err | Select-Object -Last 10 }
    throw "The build failed (exit $($p.ExitCode)). Nothing was uploaded."
}
Get-Content $log | Select-Object -Last 3

# 2. The manifest must name exactly the file that was built.
$exe = Join-Path $dist $exeName
$manifestPath = Join-Path $dist "latest-beta.json"
if (-not (Test-Path -LiteralPath $exe)) { throw "The build wrote no $exeName. Nothing was uploaded." }
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLower()
$size = (Get-Item -LiteralPath $exe).Length
if ($manifest.version -ne $Version) { throw "latest-beta.json says $($manifest.version), the build is $Version. Nothing was uploaded." }
if ($manifest.channel -ne "beta") { throw "latest-beta.json is not for the beta channel. Nothing was uploaded." }
if ($manifest.windows.sha256 -ne $hash -or $manifest.windows.bytes -ne $size) { throw "latest-beta.json does not describe the file that was built. Nothing was uploaded." }
Write-Host ("Verified: {0} bytes, sha256 {1}..." -f $size, $hash.Substring(0, 12))

if ($DryRun) { Write-Host "Dry run: nothing uploaded."; exit 0 }

# 3. Upload, the file first, the manifest last and alone.
if (-not (Test-Path $Rclone)) { throw "rclone not found at $Rclone." }
if (-not (Test-Path $RcloneConfig)) { throw "rclone config not found at $RcloneConfig." }
foreach ($f in $exeName, "latest-beta.json") {
    Write-Host "Uploading $f..."
    & $Rclone --config $RcloneConfig copyto (Join-Path $dist $f) "$Bucket/$f" --stats-one-line --stats-log-level NOTICE
    if ($LASTEXITCODE -ne 0) { throw "rclone failed on $f (exit $LASTEXITCODE). The manifest was NOT uploaded unless this was the manifest." }
}

# 4. Read back from the public host: the file served must hash the same.
$served = Join-Path $env:TEMP "served-$Version.exe"
Invoke-WebRequest -Uri ($PublicBase + [uri]::EscapeDataString($exeName)) -OutFile $served -UseBasicParsing
$servedHash = (Get-FileHash -LiteralPath $served -Algorithm SHA256).Hash.ToLower()
Remove-Item $served -Force
$live = Invoke-RestMethod -Uri ($PublicBase + "latest-beta.json") -UseBasicParsing
if ($servedHash -ne $hash) { throw "The public host serves a different file (sha256 $($servedHash.Substring(0,12))...). Do not hand this out; tell Claude." }
if ($live.version -ne $Version -or $live.windows.sha256 -ne $hash) { throw "The live manifest does not name this build yet (says $($live.version)). Wait a minute and re-check, or tell Claude." }
Write-Host ""
Write-Host "LIVE: $Version. Every beta install updates itself on its next start."
