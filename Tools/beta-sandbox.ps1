# Boot a clean Windows, install the built installers into it, and leave the
# evidence in a folder on this machine.
#
#   .\Tools\beta-sandbox.ps1
#
# Uses Windows Sandbox: a disposable Windows that starts from a pristine image
# every time and is thrown away when its window closes. The 2.1 install on
# this PC, its memory cards and its uninstall entry are never in the picture,
# which is the whole reason to test an installer this way rather than by
# running it here.
#
# What happens: this writes a .wsb that maps dist\ (read only), installer\beta\
# (read only) and target\beta-out\ (read-write) into the sandbox, starts the
# sandbox, and the sandbox runs installer\beta\sandbox-smoke.ps1 at logon.
# That script fakes a 2.1 install, installs stable over it, uninstalls it,
# installs the beta beside it, and checks after every step that the memory
# card is still there. Results land in target\beta-out\result.txt on this
# machine within a few minutes, and the sandbox stays open for a look around.
#
# Windows Sandbox is an optional feature. If it is not enabled this script
# says so and prints the one command that enables it. That command needs an
# administrator PowerShell and a restart, which is why it is not run here.

$ErrorActionPreference = "Stop"
$root = Resolve-Path "$PSScriptRoot\.."
$dist = Join-Path $root "dist"
$scripts = Join-Path $root "installer\beta"
$outDir = Join-Path $root "target\beta-out"
$wsb = Join-Path $root "target\reborn-beta.wsb"

$sandbox = Join-Path $env:WINDIR "System32\WindowsSandbox.exe"
if (-not (Test-Path $sandbox)) {
    Write-Host "Windows Sandbox is not enabled on this PC." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "Enable it from an ADMINISTRATOR PowerShell, then restart Windows:"
    Write-Host ""
    Write-Host '    Enable-WindowsOptionalFeature -Online -FeatureName "Containers-DisposableClientVM" -All' -ForegroundColor Cyan
    Write-Host ""
    Write-Host "It needs Windows 10/11 Pro or Enterprise with virtualization on, which this PC reports as present."
    Write-Host "After the restart, run this script again."
    exit 1
}

$setups = @(Get-ChildItem $dist -Filter "*Setup*.exe" -ErrorAction SilentlyContinue)
if ($setups.Count -eq 0) {
    throw "No installer in $dist. Build one first: .\installer\windows\build.ps1 -Runtime <dolphin folder>"
}
Write-Host "Installers that will be mapped in:"
$setups | ForEach-Object { Write-Host ("  {0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB)) }

if (Test-Path $outDir) { Remove-Item -Recurse -Force $outDir }
New-Item -ItemType Directory -Force $outDir | Out-Null

# The configuration. Paths must be absolute and must exist before the sandbox
# starts; a missing mapped folder is a refusal to start, not a warning.
$xml = @"
<Configuration>
  <VGpu>Enable</VGpu>
  <Networking>Enable</Networking>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$dist</HostFolder>
      <SandboxFolder>C:\beta\dist</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$scripts</HostFolder>
      <SandboxFolder>C:\beta\scripts</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$outDir</HostFolder>
      <SandboxFolder>C:\beta\out</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\beta\scripts\sandbox-smoke.ps1</Command>
  </LogonCommand>
</Configuration>
"@
Set-Content $wsb $xml -Encoding UTF8
Write-Host "Wrote $wsb"

Start-Process -FilePath $sandbox -ArgumentList "`"$wsb`""
Write-Host ""
Write-Host "Sandbox starting. Results will appear in:"
Write-Host "  $outDir\result.txt"
Write-Host "along with smoke.log and the installer logs. Close the sandbox window when done; nothing persists inside it."
