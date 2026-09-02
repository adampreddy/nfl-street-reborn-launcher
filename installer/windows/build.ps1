# Build the Windows installer.
#
#   .\build.ps1 -Version 3.0.0 -Runtime "C:\path\to\Reborn Dolphin" -GameImages a.iso, b.iso
#   .\build.ps1 -Channel beta -Version 3.0.0-beta.1 -Runtime "C:\path\to\Reborn Dolphin"
#
# Stages a payload from the workspace build output, then compiles. Nothing is
# hardcoded to one machine, which the old rebornsetupv10.iss was: it pointed at
# C:\Users\Adam\Desktop for every source file.

param(
    [string]$Version = "3.0.0",
    # "stable" is what players get. "beta" installs side by side with its own
    # AppId, name, folder and data root, so a beta under test never touches the
    # real install. See the channel notes in reborn-launcher.iss.
    [ValidateSet("stable", "beta")]
    [string]$Channel = "stable",
    # Where the Reborn Dolphin build lives. Staged by ALLOWLIST, see below.
    [string]$Runtime = "",
    # Game images. However many are given, all install. There is no fixed list
    # and no cap.
    [string[]]$GameImages = @(),
    # Leave the WebView2 bootstrapper out. The installer then only warns when
    # the runtime is missing, instead of installing it.
    [switch]$NoWebView2
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

# The channel marker the launcher reads at start. Only a beta carries one:
# the absence of the file IS the stable channel, so a stable install has
# nothing to get wrong.
if ($Channel -eq "beta") {
    Set-Content (Join-Path $payload "channel") "beta" -Encoding ASCII -NoNewline
    Write-Host "Channel: beta (marker staged; the launcher uses its own roots)"
} else {
    Write-Host "Channel: stable"
}

# -----------------------------------------------------------------------------
# The runtime, by allowlist
# -----------------------------------------------------------------------------
#
# The development runtime folder on this machine is 14 GB. What Dolphin needs
# to run is about 95 MB. The rest is game images (staged separately, below),
# the User tree (8 GB of saves and textures that must NEVER ship: putting a
# User folder in the install root is exactly how 2.1's install root became its
# data root), dated .bak_* copies, logs, and test notes.
#
# So this stages what is named rather than everything-minus-User. A file that
# is not on this list does not ship, and adding one is a line here, where it
# is reviewed, rather than a side effect of what happened to be in a folder.
#
# Two markers are left out on purpose:
#
#   portable.txt   makes Dolphin keep its User folder beside the exe. The
#                  launcher passes -u on every start, so the marker is not
#                  needed, and with it a bare double-click of the shipped
#                  Dolphin.exe would grow a User tree inside the install root.
#   play.txt       an empty marker from the stat capture test setup. Nothing
#                  in the launcher reads it.
$runtimeFiles = @(
    "Dolphin.exe", "DolphinTool.exe", "DSPTool.exe",
    "Qt6Core.dll", "Qt6Gui.dll", "Qt6Widgets.dll", "qt.conf",
    # ReShade and the Reborn add-on that hides play calls and splits the UI.
    "dxgi.dll", "ReShade.ini", "ReShadePreset.ini",
    "NSRShaderlessMenu.addon64", "NSRShaderlessMenu.ini",
    # Provenance and licensing. COPYING is Dolphin's GPL text and ships with it.
    "COPYING", "build_info.txt", "QuickIcon.ico"
)
$runtimeDirs = @(
    "Sys", "Languages", "QtPlugins", "LICENSES",
    "reshade-shaders", "reshade-presets"
)

if ($Runtime -and (Test-Path $Runtime)) {
    Write-Host "Staging runtime from $Runtime"
    $dest = Join-Path $payload "runtime"
    New-Item -ItemType Directory -Force $dest | Out-Null

    $missing = @()
    foreach ($f in $runtimeFiles) {
        $src = Join-Path $Runtime $f
        if (Test-Path $src -PathType Leaf) {
            Copy-Item $src $dest
        } else {
            $missing += $f
        }
    }
    foreach ($d in $runtimeDirs) {
        $src = Join-Path $Runtime $d
        if (Test-Path $src -PathType Container) {
            Copy-Item $src (Join-Path $dest $d) -Recurse
        } else {
            $missing += "$d\"
        }
    }

    # Dolphin.exe is the runtime. Everything else can be missing on a build
    # that does not carry it, and is reported; this one stops the build.
    if (-not (Test-Path (Join-Path $dest "Dolphin.exe"))) {
        throw "No Dolphin.exe in $Runtime. Nothing was built."
    }
    if ($missing.Count -gt 0) {
        Write-Warning ("Not present in the runtime folder, not staged: " + ($missing -join ", "))
    }

    # Machine paths inside the staged configuration. ReShade records the
    # folder it caches compiled effects in as an absolute path, which on
    # the development machine is that machine's own Temp folder. Shipped as
    # is, a player's ReShade would try to create C:\Users\Adam\... on their
    # PC. Dropping the key makes ReShade fall back to its default, which is
    # the player's own temp folder. Any other absolute path is a build
    # failure, because it would be one this script does not know how to fix.
    $reshadeIni = Join-Path $dest "ReShade.ini"
    if (Test-Path $reshadeIni) {
        $lines = Get-Content $reshadeIni
        $kept = $lines | Where-Object { $_ -notmatch '^IntermediateCachePath=' }
        if ($kept.Count -ne $lines.Count) {
            Set-Content $reshadeIni $kept -Encoding ASCII
            Write-Host "ReShade.ini: dropped IntermediateCachePath so ReShade uses the player's temp folder"
        }
    }
    $leaks = Get-ChildItem $dest -Recurse -Include *.ini, *.conf -File |
        Select-String -Pattern '[A-Za-z]:\\Users\\' -SimpleMatch:$false
    if ($leaks) {
        $leaks | ForEach-Object { Write-Warning ("Machine path in staged config: {0}:{1}: {2}" -f $_.Filename, $_.LineNumber, $_.Line.Trim()) }
        throw "The staged runtime carries a path to one machine's profile. Fix the source file or teach this script to rewrite it."
    }

    # What was left behind, so a review can see the allowlist did its job.
    $skipped = Get-ChildItem $Runtime | Where-Object {
        ($_.PSIsContainer -and $runtimeDirs -notcontains $_.Name) -or
        (-not $_.PSIsContainer -and $runtimeFiles -notcontains $_.Name)
    }
    if ($skipped) {
        Write-Host ("Left out of the runtime: " + (($skipped | ForEach-Object { $_.Name }) -join ", "))
    }

    $rsize = (Get-ChildItem $dest -Recurse -File | Measure-Object Length -Sum).Sum
    Write-Host ("Runtime: {0} file(s), {1:N1} MB" -f `
        (Get-ChildItem $dest -Recurse -File).Count, ($rsize / 1MB))
} else {
    Write-Host "No runtime staged. The launcher will look for one in an existing install."
}

# -----------------------------------------------------------------------------
# Game images
# -----------------------------------------------------------------------------

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

# -----------------------------------------------------------------------------
# The WebView2 bootstrapper
# -----------------------------------------------------------------------------
#
# WebView2 draws the launcher's window. Windows 11 and an updated Windows 10
# have it; a fresh image, and Windows Sandbox, do not, and without it the
# window opens blank. Microsoft's Evergreen bootstrapper is 1.7 MB and installs
# the runtime on demand, so it is bundled and run only when the runtime is
# absent. It is kept out of the repository and fetched once into target\.
#
# The signature is checked before it ships. A bootstrapper that is not signed
# by Microsoft is deleted and the build stops: this is the one file in the
# payload the team did not build, and it runs with the installer's rights.
$redistArg = $null
if (-not $NoWebView2) {
    $redist = Join-Path $root "target\redist"
    $bootstrapper = Join-Path $redist "MicrosoftEdgeWebView2Setup.exe"
    if (-not (Test-Path $bootstrapper)) {
        New-Item -ItemType Directory -Force $redist | Out-Null
        Write-Host "Downloading the WebView2 bootstrapper from Microsoft"
        Invoke-WebRequest -Uri "https://go.microsoft.com/fwlink/p/?LinkId=2124703" `
            -OutFile $bootstrapper -UseBasicParsing
    }
    $sig = Get-AuthenticodeSignature $bootstrapper
    if ($sig.Status -ne "Valid" -or $sig.SignerCertificate.Subject -notmatch "O=Microsoft Corporation") {
        Remove-Item $bootstrapper -Force
        throw "The WebView2 bootstrapper is not validly signed by Microsoft (status $($sig.Status)). Deleted. Nothing was built."
    }
    Write-Host ("WebView2 bootstrapper: {0:N1} MB, signed by {1}" -f `
        ((Get-Item $bootstrapper).Length / 1MB), $sig.SignerCertificate.Subject.Split(",")[0])
    $redistArg = "/DRedist=$redist"
} else {
    Write-Host "WebView2 bootstrapper left out. The installer will warn if the runtime is missing."
}

# -----------------------------------------------------------------------------
# Compile
# -----------------------------------------------------------------------------

$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (-not (Test-Path $iscc)) { throw "Inno Setup 6 not found at $iscc" }

# Compiled into a scratch folder, then moved. Compiling straight into dist
# failed twice on this machine with "being used by another process" at the
# moment the output was created: the previous Setup exe had been deleted,
# but a scanner still held its handle, and Windows keeps a deleted name
# reserved until the last handle closes. A fresh folder has no such history.
# A NEW folder every run, not the same one emptied. Emptying it hit the same
# trap one level up: the previous run had just moved a 60 MB exe out of it,
# a scanner still held that handle, and the folder's delete stayed pending.
# Old run folders are removed on a best-effort basis; one that is still
# held is left for the next run to remove.
$outRoot = Join-Path $root "target\installer-out"
New-Item -ItemType Directory -Force $outRoot | Out-Null
Get-ChildItem $outRoot -Directory -ErrorAction SilentlyContinue |
    ForEach-Object { Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue }
$scratch = Join-Path $outRoot (Get-Date -Format "yyyyMMdd-HHmmss")
New-Item -ItemType Directory -Force $scratch | Out-Null

$isccArgs = @(
    (Join-Path $PSScriptRoot "reborn-launcher.iss"),
    "/DPayload=$payload", "/DOutDir=$scratch", "/DAppVersion=$Version", "/DChannel=$Channel"
)
if ($redistArg) { $isccArgs += $redistArg }
& $iscc @isccArgs
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

New-Item -ItemType Directory -Force $dist | Out-Null
Get-ChildItem $scratch -Filter "*.exe" | ForEach-Object {
    $target = Join-Path $dist $_.Name
    try {
        Move-Item $_.FullName $target -Force -ErrorAction Stop
        Write-Host ("Built {0}  {1:N1} MB" -f $_.Name, ($_.Length / 1MB))
    } catch {
        Write-Warning ("Built, but could not move into dist ({0}). It is at {1}" -f $_.Exception.Message, $_.FullName)
    }
}
# The CLI beside the installer, so dist always carries the matching build.
Copy-Item (Join-Path $payload "rebornctl.exe") $dist -Force
