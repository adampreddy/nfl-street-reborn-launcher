# Read-only consistency check for a locally built launcher update.
# This verifies the manifest and build inputs, not an installation on a clean PC.
param([string]$Repository = (Split-Path -Parent $PSScriptRoot))
$ErrorActionPreference = 'Stop'
$Repository = (Resolve-Path -LiteralPath $Repository).Path
$manifestPath = Join-Path $Repository 'dist\latest.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.schema -ne 'nsr.launcher-release.v1') { throw 'Unexpected release manifest schema.' }
$asset = $manifest.windows
if (-not $asset -or [IO.Path]::GetFileName($asset.file_name) -ne $asset.file_name) { throw 'Invalid Windows artifact name.' }
if ($asset.file_name -ne "NFL Street Reborn Launcher Update $($manifest.version).exe") { throw 'Manifest does not identify a launcher update for this version.' }
$installer = Get-Item -LiteralPath (Join-Path $Repository "dist\$($asset.file_name)")
$installerHash = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
if ($installerHash -ne $asset.sha256 -or $installer.Length -ne $asset.bytes) { throw 'Installer checksum or size differs from the release manifest.' }
$url = [uri]$asset.url
if ($url.Scheme -ne 'https' -or [uri]::UnescapeDataString($url.Segments[-1]) -ne $asset.file_name) { throw 'Manifest URL does not identify this artifact over HTTPS.' }
$staging = Join-Path $Repository 'target\installer-payload-launcher'
$binaries = foreach ($name in @('reborn-launcher.exe', 'rebornctl.exe')) {
    $release = Get-Item -LiteralPath (Join-Path $Repository "target\release\$name")
    $staged = Get-Item -LiteralPath (Join-Path $staging $name)
    $hash = (Get-FileHash -LiteralPath $release.FullName).Hash
    if ((Get-FileHash -LiteralPath $staged.FullName).Hash -ne $hash) { throw "$name in staging differs from the release build." }
    if ($installer.LastWriteTimeUtc -lt $staged.LastWriteTimeUtc) { throw "$name is newer than the installer. Rebuild the update." }
    [pscustomobject]@{ name = $name; sha256 = $hash.ToLowerInvariant() }
}
foreach ($excluded in @('User', 'Games', 'runtime')) {
    if (Test-Path -LiteralPath (Join-Path $staging $excluded)) { throw "Launcher-only staging unexpectedly contains $excluded." }
}
[pscustomobject]@{
    version = $manifest.version
    installer = $installer.FullName
    bytes = $installer.Length
    sha256 = $installerHash
    binaries = @($binaries)
    scope = 'Local manifest and staging consistency; no remote publication or installation performed.'
} | ConvertTo-Json -Depth 4
