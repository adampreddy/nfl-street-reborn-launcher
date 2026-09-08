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

param(
    # Show the whole thing happening: the installer draws its own progress
    # window, each phase is announced in a console you can read, and the
    # window stays open at the end with the results on screen. Without this
    # the run is silent and looks, from the sandbox desktop, like nothing.
    [switch]$Watch,
    # Hand the sandbox over: the scene is set, you click through the wizard
    # yourself, and the checks run against what you did when you say so.
    [switch]$Manual,
    # A folder holding a stock NFL Street 2 (USA) disc image, mapped read-only
    # at C:\beta\disc inside the sandbox, so onboarding's import step has a
    # disc to work from. Without it the wizard can be seen but the game
    # cannot be imported, since a clean Windows has no game.
    [string]$Disc = ""
)

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

# Empty it, do not delete it.
#
# Removing the directory itself fails while anything holds a handle to it,
# and the sandbox's VM worker processes keep one for a short while after the
# sandbox has closed. That killed a run outright, seconds before the handle
# would have been released. Clearing the contents needs no such handle, and a
# leftover folder is harmless.
New-Item -ItemType Directory -Force $outDir | Out-Null
Get-ChildItem $outDir -Recurse -Force -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending |
    ForEach-Object {
        try { Remove-Item $_.FullName -Force -Recurse -ErrorAction Stop } catch { }
    }
$left = @(Get-ChildItem $outDir -Recurse -File -ErrorAction SilentlyContinue)
if ($left.Count -gt 0) {
    # Results from a previous run would be read as this run's. Better to stop
    # than to report a stale score as a fresh one.
    throw ("Could not clear $outDir; {0} file(s) remain, including {1}. " -f $left.Count, $left[0].Name) +
          "Close anything holding them open and run again."
}

# What the sandbox runs when it logs on. -NoExit in watch mode keeps the
# window open after the script ends, which is the difference between seeing
# the result and seeing a window vanish.
$flags = @()
if ($Watch) { $flags += "-Watch" }
if ($Manual) { $flags += "-Manual" }
$stayOpen = if ($Watch -or $Manual) { "-NoExit " } else { "" }
$logon = "powershell.exe -NoProfile -ExecutionPolicy Bypass ${stayOpen}-File C:\beta\scripts\sandbox-smoke.ps1 $($flags -join ' ')".Trim()

$discMapping = ""
if ($Disc) {
    if (-not (Test-Path -LiteralPath $Disc -PathType Container)) { throw "-Disc must be a folder holding the disc image: $Disc" }
    $discFull = (Resolve-Path -LiteralPath $Disc).Path
    $images = @(Get-ChildItem -LiteralPath $discFull -File | Where-Object { $_.Extension -in ".iso", ".gcm", ".rvz", ".ciso", ".wbfs" })
    if ($images.Count -eq 0) { throw "No disc image in $discFull" }
    Write-Host "Disc folder mapped in at C:\beta\disc:"
    $images | ForEach-Object { Write-Host ("  {0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB)) }
    $discMapping = @"
    <MappedFolder>
      <HostFolder>$discFull</HostFolder>
      <SandboxFolder>C:\beta\disc</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
"@
}

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
$discMapping
  </MappedFolders>
  <LogonCommand>
    <Command>$logon</Command>
  </LogonCommand>
</Configuration>
"@
Set-Content $wsb $xml -Encoding UTF8
Write-Host "Wrote $wsb"

Start-Process -FilePath $sandbox -ArgumentList "`"$wsb`""
Write-Host ""
if ($Manual) {
    Write-Host "Manual mode. The sandbox sets the scene and then hands it to you:" -ForegroundColor Cyan
    Write-Host "  - a pretend 2.1 install is laid down, with a memory card in it"
    Write-Host "  - Explorer opens on the installers"
    Write-Host "  - you double-click Setup and click through the whole wizard"
    Write-Host "  - on a clean Windows it pauses ~90 seconds on WebView2, downloading 130 MB"
    Write-Host "  - you press Finish and watch the launcher open"
    Write-Host "  - then press Enter in the console and every check runs against what you did"
    Write-Host ""
    Write-Host "The sandbox desktop takes ~30 seconds to appear first." -ForegroundColor Gray
    Write-Host ""
} elseif ($Watch) {
    Write-Host "Watch mode. Inside the sandbox you will see, in order:" -ForegroundColor Cyan
    Write-Host "  1. a console window saying what it is about to do"
    Write-Host "  2. a pretend NFL Street Reborn 2.1 laid down, with a memory card in it"
    Write-Host "  3. the launcher installer, with its real progress window"
    Write-Host "  4. that install removed again, and the memory card checked afterwards"
    Write-Host "  5. the beta installed beside it"
    Write-Host "  6. the launcher opened, left on screen"
    Write-Host "  7. every check, green or red, in the console"
    Write-Host ""
    Write-Host "It takes about two minutes. The sandbox desktop can take ~30 seconds to appear first." -ForegroundColor Gray
    Write-Host ""
}
Write-Host "Results will also appear in:"
Write-Host "  $outDir\result.txt"
Write-Host "along with smoke.log and the installer logs. Close the sandbox window when done; nothing persists inside it."
