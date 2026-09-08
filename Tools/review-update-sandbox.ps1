# Small installer regression run. Maps existing artifacts read-only; copies no game data.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if (Get-Process -Name WindowsSandboxServer,WindowsSandboxClient -ErrorAction SilentlyContinue) { throw 'An existing Windows Sandbox is open. Leave it undisturbed and run this review after it closes.' }
$sandbox = Join-Path $env:WINDIR 'System32\WindowsSandbox.exe'
if (-not (Test-Path -LiteralPath $sandbox)) { throw 'Windows Sandbox is not installed.' }
$verified = & (Join-Path $PSScriptRoot 'verify-launcher-update.ps1') -Repository $root | ConvertFrom-Json
$redist = Join-Path $root 'target\redist'
$signature = Get-AuthenticodeSignature (Join-Path $redist 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe')
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw 'The cached WebView2 runtime must have a valid Microsoft signature.' }
$out = Join-Path $root ('target\update-review-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $out | Out-Null
[pscustomobject]@{file_name=[IO.Path]::GetFileName($verified.installer);installer_sha256=$verified.sha256;launcher_sha256=($verified.binaries | Where-Object name -eq 'reborn-launcher.exe').sha256} | ConvertTo-Json | Set-Content (Join-Path $out 'expected.json')
function Xml($value) { [Security.SecurityElement]::Escape($value) }
$config = @"
<Configuration>
  <MemoryInMB>2048</MemoryInMB>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder><HostFolder>$(Xml (Join-Path $root 'dist'))</HostFolder><SandboxFolder>C:\update-review\dist</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$(Xml (Join-Path $root 'installer\beta'))</HostFolder><SandboxFolder>C:\update-review\scripts</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$(Xml $redist)</HostFolder><SandboxFolder>C:\update-review\redist</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$(Xml $out)</HostFolder><SandboxFolder>C:\update-review\out</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\update-review\scripts\update-review.ps1</Command></LogonCommand>
</Configuration>
"@
$wsb = Join-Path $out 'review.wsb'
Set-Content -LiteralPath $wsb -Value $config -Encoding UTF8
$process = Start-Process -FilePath $sandbox -ArgumentList "`"$wsb`"" -WindowStyle Hidden -PassThru
[pscustomobject]@{process_id=$process.Id;results=$out;configuration=$wsb} | ConvertTo-Json
