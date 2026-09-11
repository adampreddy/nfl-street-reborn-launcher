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
    # The game itself, shipped as content\games\NSR.iso. Setup imports it, so a
    # player needs no disc, and "I already have Reborn" boots it instead of the
    # old one. Adam's call on 2026-09-10: "start with NSRV3, not have to update".
    [string]$GameImage = "",
    # Every mod pack, as content\mods\packages\<id>\<version>, so the Mods
    # library is full on first run instead of empty.
    [string]$ModPackages = "",
    # The game patch chain (chain.json + patches\), as content\game-patch, so
    # setup moves an old install's disc to the newest build offline. This is
    # what ships in place of the game itself.
    [string]$GamePatchChain = "",
    # Verified patch bundle; the player's original disc is supplied in onboarding.
    [string]$GamePatch = "",
    [string]$PromptPackages = "",
    [switch]$SpanInstaller,
    # The pristine texture tree, and the build it is for. This is what a
    # fresh player gets: without it they install and have no textures at all,
    # because the only other way one exists is by migrating a 2.1 install.
    #
    # Point it at the AUTHORITATIVE folder on the day, since the pack changes
    # constantly. It is copied as it is; nothing is filtered.
    [string]$Textures = "",
    [string]$TexturesGameId = "GN7E69",
    # Leave the WebView2 bootstrapper out. The installer then only warns when
    # the runtime is missing, instead of installing it.
    [switch]$NoWebView2,
    # Carry the whole WebView2 runtime rather than the downloader. Adds about
    # 247 MB and removes the wait on a machine that has no WebView2, which is
    # a fresh Windows or one with no usable connection. Windows 11 has the
    # runtime built in and an updated Windows 10 has it too, so the default
    # stays the 1.7 MB downloader and this is the build to hand to someone
    # who needs it.
    [switch]$OfflineWebView2,
    # What -OfflineWebView2 appends to the installer's name, so an online
    # and an offline build can sit in dist together. Empty for the build
    # that ships, which is the offline one since 2026-09-06: the plain name
    # is the one the download page, the zip and the smoke test look for.
    [string]$OfflineSuffix = " offline",
    # A launcher-only build, for self-update: the launcher, rebornctl and the
    # Reborn Builder, nothing else. It installs over a full install and leaves
    # the runtime, the textures, the game import bundle and the content in
    # place, because Inno replaces what it carries and removes nothing. It is
    # one file, so latest.json can pin it, which the spanned first-run
    # installer never can. A fresh player cannot start from it: without the
    # import bundle there is no game.
    [switch]$LauncherOnly,
    # Stage the runtime alone, for a runtime component publish. The runtime
    # is staged into its own folder and the script exits before textures,
    # game images or the Inno compile. Needs only -Runtime.
    [switch]$RuntimeOnly,
    # The Reborn Builder's console build, shipped under tools\ beside the
    # launcher so roster updates can be applied to the player's image. The
    # default is the copy the Builder keeps for its own use on this machine.
    # Only the executable ships: clean-tree.txt beside it names a folder on
    # one machine.
    [string]$Builder = (Join-Path $env:APPDATA "NFL Street Reborn\Tools\reborn-builder-cli.exe"),
    # Where latest.json says the installer can be downloaded from. Defaults
    # to the launcher folder of the content bucket, which is where update.rs
    # reads the manifest itself. Upload the exe and the manifest there.
    [string]$UpdateBase = "https://files.nflstreetreborn.com/launcher/",
    # Adam's admin tool, not the player's launcher. Builds reborn-publisher.exe,
    # stages rebornctl.exe beside it (the Publisher shells out to it for the
    # actual publish operations), and compiles one file, "Reborn Publisher
    # Setup <version>.exe", into dist. It is never on the bucket, never in the
    # launcher's own installer, and never in the player zip: none of the
    # launcher-only options below apply to it and all are rejected alongside it.
    [switch]$Publisher
)

$ErrorActionPreference = "Stop"
if ($GameImages.Count -gt 0) { throw "This installer imports the player's game in onboarding. Use -GamePatch instead of -GameImages." }
# -RuntimeOnly relaxes the other required arguments (GamePatch, Textures,
# PromptPackages) below, but never this one: without -Runtime there is
# nothing to stage.
if ($RuntimeOnly -and -not $Runtime) { throw "-RuntimeOnly needs -Runtime pointing at the folder to stage. Nothing was built." }
if ($Publisher) {
    if ($LauncherOnly -or $RuntimeOnly -or $GamePatch -or $Runtime -or $Textures -or $PromptPackages -or $SpanInstaller -or $Channel -ne "stable") {
        throw "-Publisher builds the admin tool alone. Leave -LauncherOnly, -RuntimeOnly, -GamePatch, -Runtime, -Textures, -PromptPackages, -SpanInstaller and -Channel out."
    }
} elseif ($LauncherOnly) {
    if ($GamePatch -or $Runtime -or $Textures -or $PromptPackages) { throw "-LauncherOnly ships the launcher alone. Leave -GamePatch, -Runtime, -Textures and -PromptPackages out." }
} else {
    # -RuntimeOnly needs only -Runtime, so the game-patch bundle check below
    # is skipped for it the same way the Runtime/Textures/PromptPackages
    # check further down is: neither is part of a runtime component publish.
    if (-not $RuntimeOnly) {
        foreach ($required in @("manifest.json", "reborn.xdelta", "DolphinTool.exe", "xdelta3.exe")) {
            if (-not $GamePatch -or -not (Test-Path -LiteralPath (Join-Path $GamePatch $required) -PathType Leaf)) { throw "A complete -GamePatch bundle is required ($required)." }
        }
    }
    if (-not $RuntimeOnly -and (-not $Runtime -or -not $Textures -or -not $PromptPackages)) { throw "Runtime, Textures, and PromptPackages are required for a complete first-run installer." }
}
if (-not $Publisher -and -not (Test-Path -LiteralPath $Builder -PathType Leaf)) { throw "No Reborn Builder console build at $Builder. Pass -Builder <path to reborn-builder-cli.exe>; roster updates need it. Nothing was built." }

$root = Resolve-Path "$PSScriptRoot\..\.."
$AppNameForZip = if ($Channel -eq "beta") { "NFL Street Reborn Beta" } else { "NFL Street Reborn" }
# Three staging folders, one per kind of build. The full payload holds the
# runtime and the 687 MB import bundle, restaged from their sources on every
# full build and left alone by a launcher-only build, which stages its few
# files beside it. Before this, a launcher-only build would have wiped the
# full payload, and with it the only copy of the bundle on this machine. The
# Publisher gets a third folder of its own for the same reason turned up to
# the obvious case: it is a different program entirely, so a Publisher build
# must never be able to delete a launcher payload, or vice versa.
$payloadName = if ($Publisher) { "target\installer-payload-publisher" } elseif ($LauncherOnly) { "target\installer-payload-launcher" } else { "target\installer-payload" }
$payload = Join-Path $root $payloadName
$dist = Join-Path $root "dist"

# A full build restages the import bundle from -GamePatch into the payload
# it is about to delete. Pointed at the payload's own copy, it would delete
# its source first and then have nothing to stage. -RuntimeOnly carries no
# -GamePatch at all, and -Publisher builds an entirely different program, so
# both are out of scope the same way -LauncherOnly is.
if (-not $LauncherOnly -and -not $RuntimeOnly -and -not $Publisher) {
    $bundleFrom = [System.IO.Path]::GetFullPath((Resolve-Path -LiteralPath $GamePatch).Path)
    $payloadFull = [System.IO.Path]::GetFullPath($payload)
    if ($bundleFrom.StartsWith($payloadFull, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "-GamePatch points inside $payload, which this build deletes before staging. Keep the bundle somewhere else and point -GamePatch at it. Nothing was built."
    }
}

# -RuntimeOnly ships no launcher or Publisher binary, so building one here is
# waste, and depending on it would couple a runtime-only publish to either
# program compiling, which it does not need to do.
if (-not $RuntimeOnly) {
    Write-Host "Building release binaries (stamped $Version)"
    Push-Location $root
    # The version the binaries believe they are. Read at compile time by
    # reborn-ui's launcher_version(); without it the launcher reports the
    # crate version (3.1.0) and decides every 3.0.x update is older than
    # itself. Cargo tracks the variable and rebuilds when it changes.
    $env:REBORN_BUILD_VERSION = $Version
    if ($Publisher) {
        cargo build --release -p reborn-publisher -p reborn-cli
    } else {
        cargo build --release -p reborn-ui -p reborn-cli
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    Pop-Location
}

$resolvedPayload = [System.IO.Path]::GetFullPath($payload)
$expectedPayload = [System.IO.Path]::GetFullPath((Join-Path $root $payloadName))
if ($resolvedPayload -ne $expectedPayload) { throw "Unexpected staging path." }
if (Test-Path -LiteralPath $resolvedPayload) {
    # Rename first, delete second. Remove-Item -Recurse deletes everything
    # it can and THEN fails on a file it cannot, which on 2026-09-06 twice
    # emptied this folder around one running exe (a test launcher started
    # from it) and took the only staged copy of the 687 MB import bundle
    # with it. Renaming a folder that holds an open file fails before
    # touching anything, so a rename that succeeds proves nothing is in use,
    # and a rename that fails leaves the folder as it was: the build then
    # stages beside it in a fresh folder and says so.
    $aside = "{0}.old-{1}" -f $resolvedPayload, (Get-Date -Format "yyyyMMdd-HHmmss")
    try {
        Rename-Item -LiteralPath $resolvedPayload -NewName (Split-Path $aside -Leaf) -ErrorAction Stop
        Remove-Item -LiteralPath $aside -Recurse -Force -ErrorAction SilentlyContinue
    } catch {
        $fresh = "{0}-{1}" -f $resolvedPayload, (Get-Date -Format "yyyyMMdd-HHmmss")
        Write-Warning ("{0} is in use ({1}); most likely a launcher started from it. It was left alone. Staging in {2} instead." -f $resolvedPayload, $_.Exception.Message.Trim(), $fresh)
        $payload = $fresh
        $resolvedPayload = $fresh
    }
}
New-Item -ItemType Directory -Force $payload | Out-Null

if ($Publisher) {
    # The admin tool, with rebornctl.exe beside it. The Publisher shells out
    # to rebornctl for the actual publish operations (see
    # crates/reborn-publisher/src/roster.rs); without it beside the exe those
    # commands would have nothing to run.
    Copy-Item (Join-Path $root "target\release\reborn-publisher.exe") $payload
    Copy-Item (Join-Path $root "target\release\rebornctl.exe") $payload
    Write-Host "Build: Reborn Publisher (admin tool). No runtime, Builder, textures, channel or import bundle."
} else {
    # Skipped for -RuntimeOnly along with the build above: without it these
    # files may be absent or stale, and a runtime-only publish never ships them.
    if (-not $RuntimeOnly) {
        Copy-Item (Join-Path $root "target\release\reborn-launcher.exe") $payload
        Copy-Item (Join-Path $root "target\release\rebornctl.exe") $payload
    }

    # The Reborn Builder's console build, where the launcher looks first
    # (<install>\tools). It links its own runtime; nothing goes beside it.
    $tools = Join-Path $payload "tools"
    New-Item -ItemType Directory -Force $tools | Out-Null
    Copy-Item -LiteralPath $Builder -Destination (Join-Path $tools "reborn-builder-cli.exe")
    Write-Host ("Builder: {0:N1} MB from {1}" -f ((Get-Item -LiteralPath $Builder).Length / 1MB), $Builder)
    Write-Host $(if ($LauncherOnly) { "Build: launcher only (self-update). No runtime, textures, import bundle or content." } else { "Build: complete first-run installer." })

    # The channel marker the launcher reads at start. Only a beta carries one:
    # the absence of the file IS the stable channel, so a stable install has
    # nothing to get wrong.
    if ($Channel -eq "beta") {
        Set-Content (Join-Path $payload "channel") "beta" -Encoding ASCII -NoNewline
        Write-Host "Channel: beta (marker staged; the launcher uses its own roots)"
    } else {
        Write-Host "Channel: stable"
    }
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
#                  NFL Street Reborn Dolphin.exe would grow a User tree
#                  inside the install root.
#   play.txt       an empty marker from the stat capture test setup. Nothing
#                  in the launcher reads it.
$runtimeFiles = @(
    "NFL Street Reborn Dolphin.exe", "DolphinTool.exe", "DSPTool.exe",
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
    # The baseline for a new Dolphin user folder: User\Config\*.ini and
    # User\GameSettings\*.ini, made from the Dolphin Adam tuned, and nothing
    # else from User. A runtime the launcher has run in also has Dump, Logs
    # and a Load tree under User; none of that ships. The launcher seeds each
    # new user folder from these (reborn_core::runtime::seed_user_from_template).
    foreach ($d in "User\Config", "User\GameSettings") {
        $src = Join-Path $Runtime $d
        if (Test-Path $src -PathType Container) {
            $target = Join-Path $dest $d
            New-Item -ItemType Directory -Force $target | Out-Null
            Get-ChildItem $src -Filter *.ini -File | Copy-Item -Destination $target
        } else {
            $missing += "$d\"
        }
    }

    # NFL Street Reborn Dolphin.exe is the runtime. Everything else can be
    # missing on a build that does not carry it, and is reported; this one
    # stops the build.
    if (-not (Test-Path (Join-Path $dest "NFL Street Reborn Dolphin.exe"))) {
        throw "No NFL Street Reborn Dolphin.exe in $Runtime. Nothing was built."
    }
    if ($missing.Count -gt 0) {
        Write-Warning ("Not present in the runtime folder, not staged: " + ($missing -join ", "))
    }

    # -------------------------------------------------------------------
    # The Visual C++ runtime, beside Dolphin
    # -------------------------------------------------------------------
    #
    # NFL Street Reborn Dolphin.exe imports MSVCP140.dll and VCRUNTIME140.dll, and ReShade's
    # dxgi.dll wants the same family. None of them is part of Windows: they
    # come from the Visual C++ Redistributable, which a development machine
    # has and a clean Windows does not.
    #
    # MEASURED, 2026-09-04: in Windows Sandbox on a clean Windows 11 image,
    # the installed rebornctl.exe produced zero bytes for `status`, because
    # the loader could not find VCRUNTIME140.dll. Our own binaries now link
    # the CRT statically (see .cargo/config.toml) and need nothing. Dolphin
    # cannot be relinked here, so its copies ship BESIDE the executable.
    #
    # App-local deployment rather than running vc_redist.exe, because that
    # installer requires administrator rights and this installer runs as the
    # user (PrivilegesRequired=lowest). Microsoft supports both.
    #
    # Preferred source is a Visual Studio redistributable folder, which is
    # the copy meant for shipping. System32 is the fallback: the same files,
    # already on the build machine.
    # MSVCP140_ATOMIC_WAIT.dll is what the bundle's DolphinTool.exe imports
    # and NFL Street Reborn Dolphin.exe does not. Found 2026-09-08 in Windows Sandbox: with the
    # other four beside it and this one absent, DolphinTool could not start,
    # and onboarding reported the player's disc as unreadable. The five ship
    # beside Dolphin AND beside the import tools.
    $crtNames = @("MSVCP140.dll", "MSVCP140_CODECVT_IDS.dll", "MSVCP140_ATOMIC_WAIT.dll", "VCRUNTIME140.dll", "VCRUNTIME140_1.dll")
    $crtSource = Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft Visual Studio\*\*\VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT" `
        -Directory -ErrorAction SilentlyContinue | Sort-Object FullName | Select-Object -Last 1
    if (-not $crtSource) { $crtSource = Get-Item "$env:WINDIR\System32" }
    $crtMissing = @()
    foreach ($n in $crtNames) {
        $from = Join-Path $crtSource.FullName $n
        if (Test-Path $from) { Copy-Item $from $dest -Force } else { $crtMissing += $n }
    }
    if ($crtMissing.Count -gt 0) {
        throw ("The Visual C++ runtime is not on this build machine: " + ($crtMissing -join ", ") +
               " was not found in " + $crtSource.FullName +
               ". Dolphin cannot start without it on a clean Windows. Nothing was built.")
    }
    Write-Host ("VC runtime: {0} file(s) staged beside Dolphin, from {1}" -f $crtNames.Count, $crtSource.FullName)

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
        ($_.PSIsContainer -and $runtimeDirs -notcontains $_.Name -and $_.Name -ne "User") -or
        (-not $_.PSIsContainer -and $runtimeFiles -notcontains $_.Name)
    }
    if ($skipped) {
        Write-Host ("Left out of the runtime: " + (($skipped | ForEach-Object { $_.Name }) -join ", "))
    }

    $rsize = (Get-ChildItem $dest -Recurse -File | Measure-Object Length -Sum).Sum
    Write-Host ("Runtime: {0} file(s), {1:N1} MB" -f `
        (Get-ChildItem $dest -Recurse -File).Count, ($rsize / 1MB))

    if ($RuntimeOnly) {
        # Stage the runtime alone, for a runtime component publish. Nothing else is built.
        $only = Join-Path $PSScriptRoot "..\..\target\installer-payload-runtime"
        if (Test-Path $only) { Remove-Item -Recurse -Force $only }
        New-Item -ItemType Directory (Join-Path $only "runtime") -Force | Out-Null
        Copy-Item (Join-Path $payload "runtime\*") (Join-Path $only "runtime") -Recurse -Force
        Write-Host "Runtime staged alone at $only\runtime"
        exit 0
    }
} elseif ($LauncherOnly) {
    Write-Host "No runtime staged: a launcher-only build leaves the installed one in place."
} elseif ($Publisher) {
    Write-Host "No runtime staged: Reborn Publisher is an admin tool, not a game client."
} else {
    Write-Host "No runtime staged. The launcher will look for one in an existing install."
}

# -----------------------------------------------------------------------------
# The texture tree
# -----------------------------------------------------------------------------
#
# Into the INSTALL root, under content\textures\<game id>. The launcher reads
# its data root first, so a player who migrated their own 2.1 install keeps
# theirs, and falls back to this. Composition hardlinks out of whichever one
# it finds, so this is never copied again.
if ($Textures) {
    if (-not (Test-Path $Textures -PathType Container)) {
        throw "No texture folder at $Textures. Nothing was built."
    }
    $texDest = Join-Path $payload "content\textures\$TexturesGameId"
    New-Item -ItemType Directory -Force $texDest | Out-Null
    Write-Host "Staging textures from $Textures"
    Copy-Item (Join-Path $Textures "*") $texDest -Recurse -Force
    $texFiles = @(Get-ChildItem $texDest -Recurse -File)
    if ($texFiles.Count -eq 0) {
        throw "$Textures is empty. A player would install and have no textures. Nothing was built."
    }
    Write-Host ("Textures: {0:N0} file(s), {1:N1} GB" -f `
        $texFiles.Count, (($texFiles | Measure-Object Length -Sum).Sum / 1GB))
} elseif (-not $LauncherOnly -and -not $Publisher) {
    Write-Warning "No -Textures given. A player with no 2.1 install to migrate will have no textures."
}

# -----------------------------------------------------------------------------
# Game images
# -----------------------------------------------------------------------------

if (($LauncherOnly -or $RuntimeOnly -or $Publisher) -and ($GameImage -or $ModPackages)) {
    throw "-GameImage and -ModPackages belong to a complete build only. Nothing was built."
}
if ($GameImage) {
    if (-not (Test-Path $GameImage -PathType Leaf)) {
        throw "No game image at $GameImage. Nothing was built."
    }
    # Always NSR.iso inside, whatever the source is called: the launcher's
    # library names the pristine image that way, and setup shows the name.
    $games = Join-Path $payload "content\games"
    New-Item -ItemType Directory -Force $games | Out-Null
    Copy-Item -LiteralPath $GameImage -Destination (Join-Path $games "NSR.iso")
    $gameHash = (Get-FileHash -LiteralPath (Join-Path $games "NSR.iso") -Algorithm SHA256).Hash.ToLower()
    Set-Content (Join-Path $games "NSR.iso.sha256") $gameHash -Encoding ASCII -NoNewline
    Write-Host ("Game image: NSR.iso {0:N0} bytes, sha256 {1}..." -f (Get-Item -LiteralPath $GameImage).Length, $gameHash.Substring(0, 12))
} elseif (-not $LauncherOnly -and -not $RuntimeOnly -and -not $Publisher) {
    Write-Warning "No -GameImage given. Setup will ask every player for a disc."
}
if ($GamePatchChain) {
    if (-not (Test-Path (Join-Path $GamePatchChain "chain.json") -PathType Leaf)) {
        throw "No chain.json in $GamePatchChain. Nothing was built."
    }
    Copy-Item -LiteralPath $GamePatchChain -Destination (Join-Path $payload "content\game-patch") -Recurse
    $chainFiles = @(Get-ChildItem (Join-Path $payload "content\game-patch") -Recurse -File)
    Write-Host ("Game patch chain: {0} file(s), {1:N0} MB" -f $chainFiles.Count, (($chainFiles | Measure-Object Length -Sum).Sum / 1MB))
} elseif (-not $LauncherOnly -and -not $RuntimeOnly -and -not $Publisher) {
    Write-Warning "No -GamePatchChain given. An old install's disc will only be updated by a download after setup."
}
if ($ModPackages) {
    if (-not (Test-Path $ModPackages -PathType Container)) {
        throw "No mod packages folder at $ModPackages. Nothing was built."
    }
    $packs = @(Get-ChildItem $ModPackages -Directory | Where-Object { Test-Path (Join-Path $_.FullName "*\package.json") })
    if ($packs.Count -eq 0) { throw "$ModPackages holds no <id>\<version>\package.json. Nothing was built." }
    $packDest = Join-Path $payload "content\mods\packages"
    New-Item -ItemType Directory -Force $packDest | Out-Null
    foreach ($pack in $packs) { Copy-Item -LiteralPath $pack.FullName -Destination (Join-Path $packDest $pack.Name) -Recurse }
    Write-Host ("Mod packages: {0} ({1})" -f $packs.Count, (($packs | ForEach-Object { $_.Name }) -join ", "))
}

if (-not $LauncherOnly -and -not $Publisher) {
    Copy-Item -LiteralPath $GamePatch -Destination (Join-Path $payload "game-import") -Recurse
    foreach ($n in $crtNames) { Copy-Item -LiteralPath (Join-Path $payload "runtime\$n") -Destination (Join-Path $payload "game-import\$n") -Force }
    Copy-Item -LiteralPath $PromptPackages -Destination (Join-Path $payload "content\prompt-packages") -Recurse
}
# From the repository, so it ships in both launcher builds. Not the
# Publisher: it never draws a controller and this is game player content.
if (-not $Publisher) {
    Copy-Item -LiteralPath (Join-Path $root "crates\reborn-ui\ui\art\controllers") -Destination (Join-Path $payload "content\controller-artwork-source") -Recurse
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
$webView2Arg = $null
if ($Publisher) {
    # reborn-publisher.iss has no #ifdef Redist branch at all: it only checks
    # for WebView2 and warns, the same way the launcher does when built with
    # -NoWebView2. Adam's own machine has always had it, since the launcher
    # needs it too, so bundling 1.7 to 247 MB the admin tool will never need
    # would be pure waste.
    Write-Host "WebView2 not bundled: the Publisher installer only checks for it."
} elseif (-not $NoWebView2) {
    $redist = Join-Path $root "target\redist"
    # The downloader by default; the whole runtime with -OfflineWebView2.
    $wv = if ($OfflineWebView2) {
        @{ Name = "MicrosoftEdgeWebView2RuntimeInstallerX64.exe"; Url = "https://go.microsoft.com/fwlink/?linkid=2124701"; What = "runtime" }
    } else {
        @{ Name = "MicrosoftEdgeWebView2Setup.exe"; Url = "https://go.microsoft.com/fwlink/p/?LinkId=2124703"; What = "bootstrapper" }
    }
    $wvPath = Join-Path $redist $wv.Name
    if (-not (Test-Path $wvPath)) {
        New-Item -ItemType Directory -Force $redist | Out-Null
        Write-Host "Downloading the WebView2 $($wv.What) from Microsoft"
        Invoke-WebRequest -Uri $wv.Url -OutFile $wvPath -UseBasicParsing
    }
    $sig = Get-AuthenticodeSignature $wvPath
    if ($sig.Status -ne "Valid" -or $sig.SignerCertificate.Subject -notmatch "O=Microsoft Corporation") {
        Remove-Item $wvPath -Force
        throw "The WebView2 $($wv.What) is not validly signed by Microsoft (status $($sig.Status)). Deleted. Nothing was built."
    }
    Write-Host ("WebView2 {0}: {1:N1} MB, signed by {2}" -f `
        $wv.What, ((Get-Item $wvPath).Length / 1MB), $sig.SignerCertificate.Subject.Split(",")[0])
    $redistArg = "/DRedist=$redist"
    $webView2Arg = "/DWebView2File=$($wv.Name)"
} else {
    Write-Host "WebView2 left out. The installer will warn if the runtime is missing."
}

# -----------------------------------------------------------------------------
# Compile
# -----------------------------------------------------------------------------

$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (-not (Test-Path $iscc)) { throw "Inno Setup 6 not found at $iscc" }

# Everything the wizard shows the player: the art, and the two README pages.
# ISCC fails on a missing file anyway, but its message names a line number
# rather than what is actually absent. The Publisher installer shows none of
# this: no welcome art, no README pages, just its own icon.
if ($Publisher) {
    $publisherIcon = Join-Path $root "crates\reborn-publisher\icons\icon.ico"
    if (-not (Test-Path $publisherIcon)) {
        throw "The installer is missing the icon it shows the player: $publisherIcon"
    }
} else {
    foreach ($a in "art\NSR.ico", "art\launcher.bmp", "art\corner.bmp",
                   "text\README.txt", "text\READMEAFTER.txt") {
        $p = Join-Path $PSScriptRoot $a
        if (-not (Test-Path $p)) {
            throw "The installer is missing a file the wizard shows the player: $p"
        }
    }
}

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
    ForEach-Object {
        $oldOutput = [System.IO.Path]::GetFullPath($_.FullName)
        $outputBoundary = [System.IO.Path]::GetFullPath((Join-Path $root "target\installer-out")) + [System.IO.Path]::DirectorySeparatorChar
        if (-not $oldOutput.StartsWith($outputBoundary, [System.StringComparison]::OrdinalIgnoreCase)) { throw "Unexpected installer output directory." }
        Remove-Item -LiteralPath $oldOutput -Recurse -Force -ErrorAction SilentlyContinue
    }
$scratch = Join-Path $outRoot (Get-Date -Format "yyyyMMdd-HHmmss")
New-Item -ItemType Directory -Force $scratch | Out-Null

# Disk spanning: off unless Inno refuses without it.
#
# Inno requires it above 4,200,000,000 COMPRESSED bytes, and compressed is
# the word that matters. Guessing from the raw payload got this wrong once:
# 3.9 GB of payload compressed to 2.5 GB, well under the limit, and the
# needless spanning produced a 7.5 MB stub whose 2.5 GB of slices the next
# build would have deleted.
#
# So try one file, which is the better download anyway, and turn spanning on
# only if the compiler says the output is too big. That answer is always
# right, at the cost of one wasted compress on a genuinely huge payload.
$span = $SpanInstaller.IsPresent
$payloadBytes = (Get-ChildItem $payload -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ("Payload {0:N1} GB. Trying a single file." -f ($payloadBytes / 1GB))

$issFile = if ($Publisher) { "reborn-publisher.iss" } else { "reborn-launcher.iss" }
$isccArgs = @(
    (Join-Path $PSScriptRoot $issFile),
    "/DPayload=$payload", "/DOutDir=$scratch", "/DAppVersion=$Version"
)
# Channel only means anything to the launcher script: reborn-publisher.iss
# has no such define and there is no such thing as a beta Publisher.
if (-not $Publisher) { $isccArgs += "/DChannel=$Channel" }
if ($span) { $isccArgs += "/DSpan" }
# A different file name, so the online and offline builds can sit in dist
# together rather than one silently replacing the other.
if (-not $Publisher -and $OfflineWebView2 -and $OfflineSuffix) { $isccArgs += "/DOfflineSuffix=$OfflineSuffix" }
# "Launcher Update" rather than "Launcher Setup", so the small self-update
# file and the full first-run set never get mistaken for each other in dist
# or on a download page.
if ($LauncherOnly) { $isccArgs += "/DUpdateBuild" }
if ($redistArg) { $isccArgs += $redistArg }
if ($webView2Arg) { $isccArgs += $webView2Arg }
# Compiled with retries. ISCC reads two and a half thousand files it was
# handed seconds earlier, and a scanner holding any one of them for a moment
# fails the whole compile with "being used by another process". Seen on this
# machine, 2026-09-04, on the second of two back-to-back builds. Waiting and
# asking again costs 30 seconds; failing costs the whole build.
# Run through Start-Process with the streams sent to files.
#
# NOT `& $iscc ... 2>&1`: in Windows PowerShell, redirecting a native
# command's stderr wraps every line in an ErrorRecord, which under
# ErrorActionPreference Stop fails the call whatever the exit code was. That
# turned a compile which only needed retrying into a dead build.
$outLog = Join-Path $scratch "iscc.out.log"
$errLog = Join-Path $scratch "iscc.err.log"
$attempt = 0
while ($true) {
    $attempt++
    # Quoted here, because Start-Process joins the list with spaces and
    # quotes nothing. Every path in it has a space.
    $quoted = $isccArgs | ForEach-Object {
        if ($_ -match '\s') { '"' + $_ + '"' } else { $_ }
    }
    $run = Start-Process -FilePath $iscc -ArgumentList $quoted -Wait -PassThru -NoNewWindow `
        -RedirectStandardOutput $outLog -RedirectStandardError $errLog
    $log = @()
    foreach ($f in $outLog, $errLog) {
        if (Test-Path $f) { $log += Get-Content $f }
    }
    $log | Where-Object { $_ -match "Error|Successful|Resulting" } | ForEach-Object { Write-Host $_ }
    if ($run.ExitCode -eq 0) { break }
    if ($attempt -ge 3) { throw "ISCC failed $attempt times. Last output:`n" + ($log -join "`n") }

    # Too big for one file. Inno says so plainly; turn spanning on and go
    # again rather than making anyone guess the threshold.
    # The compiler's own words, quoted from a real failure:
    #   "Disk spanning must be enabled in order to create an installation
    #    larger than 2100000000 bytes in size."
    # Note 2.1 GB, not the 4.2 GB the documentation states. Matching the real
    # message rather than a remembered number is the only way this stays true.
    if (-not $span -and ($log -match "Disk spanning must be enabled")) {
        Write-Host "The output is too large for a single file. Turning disk spanning on."
        $span = $true
        $isccArgs += "/DSpan"
        continue
    }
    Write-Warning "ISCC failed (attempt $attempt). Something is holding a file it needs. Waiting 15 seconds."
    Start-Sleep -Seconds 15
}

New-Item -ItemType Directory -Force $dist | Out-Null
# Every produced file, not just the exe. A spanned build is setup.exe plus
# setup-1.bin and friends, and moving only the exe leaves an installer that
# cannot find its own data while dist looks like it has one.
# The wildcard in the path is load bearing: Get-ChildItem -Exclude against a
# bare directory matches nothing at all, so this loop would have moved zero
# files while reporting success. Proven on this machine: 0 seen without it, 3
# with it.
Get-ChildItem (Join-Path $scratch "*") -File -Exclude "iscc.*.log" | ForEach-Object {
    # Name, path and size captured BEFORE the try, because $_ cannot be
    # trusted inside it. After Move-Item the FileInfo re-stats a path that no
    # longer exists and Length comes back 0, which is why every build printed
    # "Built ... 0.0 MB" for files of any size. And inside the catch, $_ is
    # the ErrorRecord, not the file, so $_.Name and $_.FullName were empty in
    # the one message that exists to name the file that could not be replaced.
    $name = $_.Name
    $from = $_.FullName
    $mb = $_.Length / 1MB
    $target = Join-Path $dist $name
    try {
        try {
            Move-Item $from $target -Force -ErrorAction Stop
        } catch {
            # A rename INTO dist is refused with a sharing violation for as long
            # as a Windows Sandbox VM worker (vmmemWindowsSandbox) lingers with
            # dist mapped, minutes after its window is closed, while creating a
            # file there still works. Seen 2026-09-08 19:02: the compile
            # succeeded and the move failed. Copy then delete gets the same
            # result; only if that fails too is something really holding it.
            Copy-Item -LiteralPath $from -Destination $target -Force -ErrorAction Stop
            Remove-Item -LiteralPath $from -Force -ErrorAction Stop
        }
        Write-Host ("Built {0}  {1:N1} MB" -f $name, $mb)
    } catch {
        # A failure here used to be a warning, and the build then reported
        # success while dist still held the PREVIOUS installer. Anyone reading
        # the exit code was told a stale file was the new one.
        #
        # The cause, every time it has happened: something else holds the file
        # in dist. A running Windows Sandbox maps dist in and keeps a handle on
        # everything in it, so a rebuild during a sandbox run cannot replace it.
        $held = if (Get-Process WindowsSandboxServer -ErrorAction SilentlyContinue) {
            "Windows Sandbox is running and has dist mapped into it. Close the sandbox window and build again."
        } else {
            "Something else holds it open: a scanner, an Explorer preview, or a copy still running."
        }
        throw ("Built {0}, but it could not replace the copy in dist. {1}`n  New build: {2}`n  Blocked by: {3}" -f `
            $name, $held, $from, $_.Exception.Message)
    }
}
if ($Publisher) {
    # No CLI copy beside the installer in dist, no zip, no release manifest.
    # None of that machinery is for this program: it is not on the bucket,
    # not in the launcher, and not in the player zip, and it never
    # self-updates through latest.json. Just confirm the one file is there.
    $psetup = Get-ChildItem $dist -Filter "Reborn Publisher Setup $Version.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $psetup) { throw "Reborn Publisher Setup $Version.exe is not in dist. Nothing was built." }
    $pstray = @(Get-ChildItem $dist -Filter ("Reborn Publisher Setup {0}-*.bin" -f $Version) -ErrorAction SilentlyContinue)
    if ($pstray.Count -gt 0) { throw "Reborn Publisher Setup $Version.exe has slice files beside it, which should never happen for a payload this small." }
    Write-Host ("Reborn Publisher installer: {0}  {1:N1} MB" -f $psetup.Name, ($psetup.Length / 1MB))
} else {
    # The CLI beside the installer, so dist always carries the matching build.
    Copy-Item (Join-Path $payload "rebornctl.exe") $dist -Force

    # -----------------------------------------------------------------------------
    # The release manifest the launcher's update check reads
    # -----------------------------------------------------------------------------
    #
    # latest.json (stable) or latest-beta.json (beta), written by the build that
    # made the installer, so the hash it pins is the hash of the file beside it.
    # Publishing an update is uploading BOTH files to a GitHub release tagged
    # v<version> on the launcher repository; the launcher reads the manifest from
    # the release page's latest-download URL and the installer from the URL the
    # manifest names. See crates/reborn-core/src/update.rs.
    # A spanned build has no single file to pin, so self-update cannot fetch it:
    # update.rs downloads one artifact and runs it. Writing a manifest naming
    # only the stub would publish an update that installs nothing.
    # THIS build's slices and nothing else's. A wildcard after the version
    # ("Setup 3.0.0*.bin") also matched "Setup 3.0.0-onboarding.1-1.bin", and on
    # 2026-09-06 a launcher-only build, which has no slices at all, packed six
    # files from two older full builds into one zip over the top of a good one.
    # The set is exactly "<prefix>.exe" plus "<prefix>-<n>.bin".
    $prefix = "{0} Launcher Setup {1}" -f $AppNameForZip, $Version
    $slicePattern = "^" + [regex]::Escape($prefix) + "-\d+\.bin$"
    $slices = @()
    if (-not $LauncherOnly) {
        $slices = @(Get-ChildItem $dist -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -match $slicePattern })
    }

    # One file to hand out.
    #
    # Inno cannot make a single installer above 2,100,000,000 bytes, so a build
    # carrying the textures and a disc image is always setup.exe plus slices.
    # Three links is a worse download than one, so the set is packed into a zip
    # the player extracts before running setup.
    #
    # STORED, not deflated. Inno already LZMA compressed everything inside; a
    # second pass would spend minutes to save nothing. And .NET rather than
    # Compress-Archive, which cannot write an archive this size in Windows
    # PowerShell.
    if ($slices.Count -gt 0) {
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        # Exactly the exe and its numbered slices. Anchored on this channel's
        # own name and closed at the version: a StartsWith on the prefix swept
        # in "Setup 3.0.0-onboarding.1" as well as the beta.
        $setFiles = @(Get-ChildItem $dist -File | Where-Object { $_.Name -eq "$prefix.exe" -or $_.Name -match $slicePattern })
        $zipPath = Join-Path $dist ("{0} Launcher {1}.zip" -f $AppNameForZip, $Version)
        if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
        Write-Host ("Packing {0} file(s) into one zip" -f $setFiles.Count)
        $zip = [System.IO.Compression.ZipFile]::Open($zipPath, 'Create')
        try {
            foreach ($f in $setFiles) {
                [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                    $zip, $f.FullName, $f.Name, [System.IO.Compression.CompressionLevel]::NoCompression) | Out-Null
            }
        } finally {
            $zip.Dispose()
        }
        $zipItem = Get-Item $zipPath
        # A zip that does not hold every piece is worse than no zip: setup.exe
        # would run and then ask for a slice nobody has.
        $zipCount = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            $inside = $zipCount.Entries.Count
        } finally {
            $zipCount.Dispose()
        }
        if ($inside -ne $setFiles.Count) {
            throw "The zip holds $inside of $($setFiles.Count) files. Deleting it rather than shipping half a set."
        }
        Write-Host ("Built {0}  {1:N2} GB, holding all {2} file(s)" -f $zipItem.Name, ($zipItem.Length / 1GB), $inside)
        Write-Host "That zip is the whole download. Players extract it and run setup.exe."
    }
    $setupPattern = if ($LauncherOnly) { "$AppNameForZip Launcher Update $Version.exe" } else { "$AppNameForZip Launcher Setup $Version*.exe" }
    $setup = Get-ChildItem $dist -Filter $setupPattern | Select-Object -First 1
    if ($span -or $slices.Count -gt 0) {
        # The format expression needs its own parentheses: Write-Warning ("..") -f x
        # binds -f as a parameter of Write-Warning, which is a runtime failure
        # AFTER a seven minute compile has already succeeded.
        Write-Warning (("This build is spanned across {0} file(s), so no release manifest was written: " +
                        "self-update fetches a single installer. Distribute the whole set together.") -f ($slices.Count + 1))
    } elseif ($setup) {
        $hash = (Get-FileHash $setup.FullName -Algorithm SHA256).Hash.ToLower()
        $base = $UpdateBase.TrimEnd("/") + "/"
        $assetUrl = $base + [uri]::EscapeDataString($setup.Name)
        $manifest = [ordered]@{
            schema   = "nsr.launcher-release.v1"
            version  = $Version
            channel  = $Channel
            notes    = $null
            windows  = [ordered]@{ url = $assetUrl; sha256 = $hash; bytes = $setup.Length; file_name = $setup.Name }
            macos    = $null
            linux    = $null
        }
        $manifestName = if ($Channel -eq "beta") { "latest-beta.json" } else { "latest.json" }
        $manifestPath = Join-Path $dist $manifestName
        ($manifest | ConvertTo-Json -Depth 4) | Set-Content $manifestPath -Encoding ASCII
        Write-Host ("Wrote {0}: version {1}, sha256 {2}..." -f $manifestName, $Version, $hash.Substring(0, 12))
    } else {
        Write-Warning "No Setup $Version.exe in dist, so no release manifest was written."
    }
}
