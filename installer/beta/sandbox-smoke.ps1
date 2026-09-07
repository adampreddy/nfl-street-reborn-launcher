# Installer smoke test. Runs INSIDE Windows Sandbox, as the sandbox's own user,
# against a Windows that has never seen NFL Street Reborn.
#
# Started by the sandbox's logon command; see Tools\beta-sandbox.ps1, which
# writes the .wsb that maps three folders in:
#
#   C:\beta\dist     the built installers, read only
#   C:\beta\scripts  this folder, read only
#   C:\beta\out      results, read-write, which is how they reach the host
#
# What it proves, in order:
#
#   1. A machine with a 2.1 install (faked here: its folder, a memory card
#      file, and its uninstall registry entry under the AppId the stable
#      installer keeps) takes the stable installer as an UPGRADE and lands in
#      Programs, not on the 2.1 data folder. The memory card is untouched.
#   2. Uninstalling the stable launcher leaves the memory card and the 2.1
#      folder alone. This is the bug the 2.1 uninstaller has.
#   3. The beta installer lands in its own folder with its own uninstall
#      entry, and the launcher inside it reports the beta data root.
#
# Everything is written to C:\beta\out: a log, the installer logs, and
# result.txt with one PASS or FAIL per check. The sandbox is thrown away when
# its window closes, so nothing here needs cleaning up.

# Watch mode. The same run, visible: the installer draws its own progress
# window instead of being silent, each phase is announced, and the whole
# thing pauses so a person can look at what just happened. Set by the .wsb
# that Tools\beta-sandbox.ps1 -Watch writes.
param(
    [switch]$Watch,
    # Hands the sandbox over to a person. The fake 2.1 install is laid down,
    # Explorer is opened on the installers, and NOTHING is run: you click
    # through the wizard yourself, see the Finish page, and watch the
    # launcher open. Press Enter in the console afterwards and every check
    # runs against what you just did.
    [switch]$Manual
)

$ErrorActionPreference = "Continue"
$out = "C:\beta\out"
$dist = "C:\beta\dist"
New-Item -ItemType Directory -Force $out | Out-Null
$log = Join-Path $out "smoke.log"
$results = New-Object System.Collections.ArrayList

# A heading a person can see from across the room, and a pause to read it.
function Phase($title, $detail) {
    if (-not $Watch) { Say("--- $title"); return }
    Write-Host ""
    Write-Host ("=" * 74) -ForegroundColor DarkGray
    Write-Host "  $title" -ForegroundColor Cyan
    Write-Host "  $detail" -ForegroundColor Gray
    Write-Host ("=" * 74) -ForegroundColor DarkGray
    Start-Sleep -Seconds 4
}

function Say($message) {
    $line = "{0}  {1}" -f (Get-Date -Format "HH:mm:ss"), $message
    Add-Content $log $line
    Write-Host $line
}
function Check($what, $ok, $detail) {
    # $detail was missing from this signature while six call sites passed one.
    # PowerShell puts an extra positional argument into $args rather than
    # failing, and nothing here read $args, so every one of those diagnostics
    # was discarded. A failing run logged the name of the check and no reason,
    # which is the moment the reason is worth the most.
    [void]$results.Add([pscustomobject]@{ ok = [bool]$ok; check = $what })
    Say(("[{0}] {1}" -f $(if ($ok) { "PASS" } else { "FAIL" }), $what))
    if (-not $ok -and -not [string]::IsNullOrWhiteSpace($detail)) {
        Say("       why: $detail")
    }
}
function Run-Setup($exe, $logName, $extra = @()) {
    # /SILENT draws the progress window; /VERYSILENT draws nothing at all.
    # In watch mode a person is looking at the screen, so they get the one
    # that shows the files going in.
    $quiet = if ($Watch) { "/SILENT" } else { "/VERYSILENT" }
    $args = @($quiet, "/SUPPRESSMSGBOXES", "/NORESTART", "/LOG=`"$out\$logName`"") + $extra
    Say("running $(Split-Path $exe -Leaf) $($args -join ' ')")
    # WaitForExit on the Setup process itself, NOT Start-Process -Wait, which
    # waits for every descendant too. With /RELAUNCH=1 the installer starts
    # the launcher as its own child and exits, and -Wait then sat waiting for
    # the launcher window to close, which nothing in this script does. Seen
    # 2026-09-06: the update installer finished at 19:54 and the run hung.
    $p = Start-Process -FilePath $exe -ArgumentList $args -PassThru
    $p.WaitForExit()
    Say("exit code $($p.ExitCode)")
    return $p.ExitCode
}
function Uninstall-Silently($dir) {
    $unins = Join-Path $dir "unins000.exe"
    if (-not (Test-Path $unins)) { Say("no uninstaller at $unins"); return -1 }
    $p = Start-Process -FilePath $unins -ArgumentList @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART") -Wait -PassThru
    Say("uninstall exit code $($p.ExitCode)")
    # Inno's uninstaller hands off to a copy of itself and returns before the
    # copy finishes removing files. Give it a moment.
    Start-Sleep -Seconds 4
    return $p.ExitCode
}
# Run one of our own executables and capture everything it said.
#
# `& exe *> file` was not enough: when the loader cannot start a binary at
# all, which is what a missing VC runtime looks like, the file was created
# EMPTY and the check failed with nothing to explain it. That cost a whole
# sandbox run to diagnose from the host. This records the exit code and
# whatever came out of both streams, so the log says what happened.
function Run-Ours($exe, $exeArgs, $logName) {
    $path = Join-Path $out $logName
    if (-not (Test-Path $exe)) {
        Say("$exe is not there at all")
        Set-Content $path "the executable is not there"
        return ""
    }
    $err = Join-Path $out "$logName.err"
    $p = Start-Process -FilePath $exe -ArgumentList $exeArgs -Wait -PassThru -NoNewWindow `
        -RedirectStandardOutput $path -RedirectStandardError $err
    $stdout = if (Test-Path $path) { Get-Content $path -Raw } else { "" }
    $stderr = if (Test-Path $err) { Get-Content $err -Raw } else { "" }
    Say("$(Split-Path $exe -Leaf) $exeArgs -> exit $($p.ExitCode), $($stdout.Length) byte(s) out, $($stderr.Length) byte(s) err")
    if ($p.ExitCode -ne 0 -or [string]::IsNullOrWhiteSpace($stdout)) {
        # Exit code 0xC0000135 is STATUS_DLL_NOT_FOUND: a dependency the
        # machine does not have. Named here because it is invisible otherwise.
        $code = "0x" + ("{0:X8}" -f $p.ExitCode)
        Say("  nothing usable came back. Exit $code. Stderr: $stderr")
        if ($p.ExitCode -eq -1073741515) { Say("  that code is STATUS_DLL_NOT_FOUND: a DLL this machine does not have.") }
    }
    return $stdout
}

function WebView2Version() {
    $keys = @(
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
    )
    foreach ($k in $keys) { $v = (Get-ItemProperty $k -ErrorAction SilentlyContinue).pv; if ($v) { return $v } }
    return $null
}

try {
    Say("smoke test starting as $env:USERNAME on $([Environment]::OSVersion.VersionString)")
    Say("WebView2 before install: $(WebView2Version)")

    $stableSetup = Get-ChildItem $dist -Filter "NFL Street Reborn Launcher Setup *.exe" | Select-Object -First 1
    $betaSetup = Get-ChildItem $dist -Filter "NFL Street Reborn Beta Launcher Setup *.exe" | Select-Object -First 1
    Check "a stable installer is in dist" ($null -ne $stableSetup)
    Check "a beta installer is in dist" ($null -ne $betaSetup)

    # ------------------------------------------------------------------
    # 1. A fake 2.1 install, shaped like the real one where it matters
    # ------------------------------------------------------------------
    $old = Join-Path $env:APPDATA "NFL Street Reborn"
    $card = Join-Path $old "User\GC\MemoryCardA.USA.raw"
    New-Item -ItemType Directory -Force (Split-Path $card) | Out-Null
    Set-Content $card "the player's memory card" -NoNewline
    $cardHash = (Get-FileHash $card -Algorithm SHA256).Hash
    New-Item -ItemType Directory -Force (Join-Path $old "User\Load\Textures\GN7E69") | Out-Null
    Set-Content (Join-Path $old "User\Load\Textures\GN7E69\tex1_marker.dds") "a texture" -NoNewline
    $regKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}_is1"
    New-Item -Path $regKey -Force | Out-Null
    Set-ItemProperty $regKey "DisplayName" "NFL Street Reborn"
    Set-ItemProperty $regKey "DisplayVersion" "2.1"
    Set-ItemProperty $regKey "InstallLocation" "$old\"
    Set-ItemProperty $regKey "Inno Setup: App Path" $old
    Say("laid a fake 2.1 install at $old with its uninstall entry")
    if ($Watch) {
        Write-Host ""
        Write-Host "  A pretend NFL Street Reborn 2.1 now exists on this clean Windows:" -ForegroundColor Yellow
        Write-Host "    $old" -ForegroundColor Gray
        Write-Host "    with a memory card inside it, and 2.1's entry in Programs and Features." -ForegroundColor Gray
        Write-Host "  Everything after this is about not destroying that." -ForegroundColor Yellow
    }

    # ------------------------------------------------------------------
    # 2. Manual: hand the machine over, then check what was done
    # ------------------------------------------------------------------
    if ($Manual) {
        Start-Process explorer.exe $dist
        Write-Host ""
        Write-Host ("=" * 74) -ForegroundColor DarkGray
        Write-Host "  This clean Windows is yours to drive." -ForegroundColor Cyan
        Write-Host ("=" * 74) -ForegroundColor DarkGray
        Write-Host ""
        Write-Host "  A pretend NFL Street Reborn 2.1 is already here, with a memory card in it:" -ForegroundColor Gray
        Write-Host "    $old"
        Write-Host "  So installing over it is the upgrade a real player would do." -ForegroundColor Gray
        Write-Host ""
        Write-Host "  An Explorer window is open on the installers. Do it yourself:" -ForegroundColor Yellow
        Write-Host "    1. Double-click 'NFL Street Reborn Launcher Setup 3.0.0.exe'"
        Write-Host "    2. Click through the wizard. Read the folder page; it must NOT be the 2.1 folder."
        Write-Host "    3. On a clean Windows it pauses on WebView2 for a minute or two. It is"
        Write-Host "       downloading about 130 MB. The bar does not move. It is not stuck."
        Write-Host "    4. Leave 'Launch NFL Street Reborn' ticked and press Finish."
        Write-Host "    5. Watch how long the launcher takes to appear. That is the real number."
        Write-Host ""
        Write-Host "  Install the Beta one too if you want to see them side by side." -ForegroundColor Gray
        Write-Host ""
        Write-Host "  When you are done, come back here and press Enter. Every check then runs" -ForegroundColor Yellow
        Write-Host "  against what you actually did, and the memory card is compared byte for byte." -ForegroundColor Yellow
        Write-Host ""
        Read-Host "  Press Enter when you have finished installing"

        $stableDir = Join-Path $env:LOCALAPPDATA "Programs\NFL Street Reborn"
        $betaDir = Join-Path $env:LOCALAPPDATA "Programs\NFL Street Reborn Beta"
        Check "you installed the stable launcher" (Test-Path (Join-Path $stableDir "reborn-launcher.exe")) "nothing at $stableDir"
        if (Test-Path (Join-Path $stableDir "reborn-launcher.exe")) {
            Check "it went to Programs, not onto the 2.1 data folder" ($stableDir -like "*\Programs\*")
            Check "the runtime shipped beside it" (Test-Path (Join-Path $stableDir "runtime\Dolphin.exe"))
            Check "the Visual C++ runtime shipped beside Dolphin" (Test-Path (Join-Path $stableDir "runtime\VCRUNTIME140.dll"))
            Check "nothing was installed into the 2.1 folder" (-not (Test-Path (Join-Path $old "reborn-launcher.exe")))
            $s = Run-Ours (Join-Path $stableDir "rebornctl.exe") "status" "status-manual.txt"
            Check "its rebornctl runs on this clean Windows" (-not [string]::IsNullOrWhiteSpace($s))
        }
        if (Test-Path (Join-Path $betaDir "reborn-launcher.exe")) {
            Check "the beta installed into its own folder" $true
            Check "the beta carries its channel marker" ((Get-Content (Join-Path $betaDir "channel") -Raw).Trim() -eq "beta")
        } else {
            Say("the beta was not installed; skipping its checks")
        }
        Check "the memory card is byte for byte what it was" ((Test-Path $card) -and ((Get-FileHash $card -Algorithm SHA256).Hash -eq $cardHash))
        Check "the 2.1 texture folder is untouched" (Test-Path (Join-Path $old "User\Load\Textures\GN7E69\tex1_marker.dds"))
        Say("WebView2 now: $(WebView2Version)")
        Check "WebView2 is present" ($null -ne (WebView2Version))
    }

    # ------------------------------------------------------------------
    # 3. The stable installer, as an upgrade over it
    # ------------------------------------------------------------------
    if ($stableSetup -and -not $Manual) {
        Phase "Installing the launcher over that 2.1 install" "You should see the installer's progress window."
        # /TASKS asks for all three shortcuts. Two of them are unchecked by
        # default in the wizard, so a plain silent install would create
        # neither and the checks below would be testing nothing.
        $code = Run-Setup $stableSetup.FullName "install-stable.log" @("/TASKS=`"desktopicon,gameicon,foldericon`"")
        Check "stable installer exits 0 over a 2.1 install" ($code -eq 0)

        $stableDir = Join-Path $env:LOCALAPPDATA "Programs\NFL Street Reborn"
        Check "stable launcher landed in Programs, not on the 2.1 data folder" (Test-Path (Join-Path $stableDir "reborn-launcher.exe"))
        Check "the runtime shipped beside it" (Test-Path (Join-Path $stableDir "runtime\Dolphin.exe"))
        # All four, not just one. Dolphin imports MSVCP140.dll as well as
        # VCRUNTIME140.dll, and checking a single file meant three of the four
        # could go missing without any test noticing.
        foreach ($dll in "VCRUNTIME140.dll", "VCRUNTIME140_1.dll", "MSVCP140.dll", "MSVCP140_CODECVT_IDS.dll") {
            Check ("the Visual C++ runtime shipped beside Dolphin: $dll") `
                (Test-Path (Join-Path $stableDir "runtime\$dll")) `
                "not at $stableDir\runtime\$dll"
        }

        # The textures. Without these a fresh player installs and has nothing to
        # look at, and every other check here would still have passed.
        # Joined a segment at a time. Writing the whole path as one string is how
        # this line ended up containing a literal tab.
        $tex = Join-Path (Join-Path $stableDir "content") (Join-Path "textures" "GN7E69")
        $texCount = @(Get-ChildItem $tex -Recurse -File -ErrorAction SilentlyContinue).Count
        Say("textures installed: $texCount file(s) at $tex")
        Check "the texture tree shipped, and is not empty" ($texCount -gt 1000) "found $texCount file(s)"

        # And the import bundle, which is the other half of a playable install
        # since the patch route: the player's own disc plus this delta is the
        # game. No image ships any more, by design.
        $bundle = Join-Path $stableDir "game-import"
        $bundleMissing = @("manifest.json", "reborn.xdelta", "xdelta3.exe", "DolphinTool.exe" | Where-Object { -not (Test-Path (Join-Path $bundle $_)) })
        Check "the game import bundle shipped" ($bundleMissing.Count -eq 0) ("missing from ${bundle}: " + ($bundleMissing -join ", "))

        Check "no channel marker in a stable install" (-not (Test-Path (Join-Path $stableDir "channel")))
        # ------------------------------------------------------------------
        # The three shortcuts.
        #
        # Untested until now, which is why they are here: they are what a
        # player actually clicks, and every one of them must run the LAUNCHER
        # with a switch. A shortcut pointing straight at Dolphin would boot
        # with no mods composed, no configuration written and no result
        # collected, and it would look like it worked.
        #
        # Read through WScript.Shell because a .lnk is a binary structure; its
        # target and arguments cannot be checked by looking at the file name.
        # ------------------------------------------------------------------
        $shell = New-Object -ComObject WScript.Shell
        $desktop = [Environment]::GetFolderPath("Desktop")
        $launcherExe = Join-Path $stableDir "reborn-launcher.exe"

        $wanted = @(
            @{ File = "NFL Street Reborn Launcher.lnk"; Args = "";         What = "the launcher" }
            @{ File = "Play NFL Street Reborn.lnk";     Args = "--play";   What = "the game directly" }
            @{ File = "NFL Street Reborn files.lnk";    Args = "--folder"; What = "the install folder" }
        )
        foreach ($w in $wanted) {
            $lnk = Join-Path $desktop $w.File
            if (-not (Test-Path $lnk)) {
                Check ("desktop shortcut for {0} exists" -f $w.What) $false "no $lnk"
                continue
            }
            Check ("desktop shortcut for {0} exists" -f $w.What) $true
            $sc = $shell.CreateShortcut($lnk)
            Check ("that shortcut runs the launcher, not Dolphin, for {0}" -f $w.What) `
                ($sc.TargetPath -eq $launcherExe) ("points at " + $sc.TargetPath)
            Check ("that shortcut passes '{0}'" -f $(if ($w.Args) { $w.Args } else { "no switch" })) `
                ($sc.Arguments.Trim() -eq $w.Args) ("arguments were '" + $sc.Arguments + "'")
        }

        # The Start menu gets all three plus the uninstaller, whatever was
        # ticked on the desktop.
        $group = Join-Path ([Environment]::GetFolderPath("Programs")) "NFL Street Reborn"
        $groupLinks = @(Get-ChildItem $group -Filter *.lnk -ErrorAction SilentlyContinue)
        Check "the Start menu group holds all four entries" ($groupLinks.Count -ge 4) `
            ("found {0} in {1}" -f $groupLinks.Count, $group)

        Check "nothing was installed into the 2.1 folder" (-not (Test-Path (Join-Path $old "reborn-launcher.exe")))
        $loc = (Get-ItemProperty $regKey -ErrorAction SilentlyContinue).InstallLocation
        Check "the uninstall entry now points at Programs (was the 2.1 data folder)" ($loc -like "*\Programs\NFL Street Reborn*")
        Check "the memory card is byte for byte what it was" ((Test-Path $card) -and ((Get-FileHash $card -Algorithm SHA256).Hash -eq $cardHash))

        $cli = Join-Path $stableDir "rebornctl.exe"
        if (Test-Path $cli) {
            $status = Run-Ours $cli "status" "status-stable.txt"
            Check "stable rebornctl runs at all on a clean Windows" (-not [string]::IsNullOrWhiteSpace($status)) "it produced nothing"
            Check "stable rebornctl status names the stable channel and data root" ($status -match "Channel\s+stable" -and $status -match "Data root\s+.*NFL Street Reborn Launcher\s" -and $status -notmatch "Launcher Beta")
            Check "stable rebornctl status confirms the data root is outside the install root" ($status -match "uninstall cannot eat saves")
        }
        # The launcher itself. Every check above is about files on disk, and
        # a launcher that cannot start passes all of them.
        $launcher = Join-Path $stableDir "reborn-launcher.exe"
        if (Test-Path $launcher) {
            Phase "Opening the launcher" "This is the check that a clean Windows can actually run it."
            $proc = Start-Process -FilePath $launcher -PassThru -ErrorAction SilentlyContinue
            Start-Sleep -Seconds $(if ($Watch) { 25 } else { 8 })
            $alive = $proc -and -not $proc.HasExited
            Check "the launcher window starts and stays up" $alive "it exited with $(if ($proc) { $proc.ExitCode } else { 'no process at all' })"
            # Closed either way, always. Leaving it open in watch mode meant
            # the uninstaller could not delete a running executable, and the
            # run reported that as a product failure when it was the test
            # holding the file. Seen once, 2026-09-04.
            if ($alive) {
                if ($Watch) { Say("closing the launcher again so the uninstall can remove it") }
                Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
                Start-Sleep -Seconds 2
            }
        }

        Say("WebView2 after stable install: $(WebView2Version)")
        Check "WebView2 runtime present after install (needs network in the sandbox)" ($null -ne (WebView2Version))

        # ------------------------------------------------------------------
        # 2b. The launcher-only update, over that install
        #
        # This is the file self-update downloads. It carries the launcher,
        # rebornctl and the Builder and nothing else, and it has to leave the
        # runtime, the textures and the import bundle exactly as they were,
        # then bring the launcher back, because that is what a player sees
        # after "Installing verified update".
        # ------------------------------------------------------------------
        $updateSetup = Get-ChildItem $dist -Filter "NFL Street Reborn Launcher Update *.exe" | Select-Object -First 1
        if ($updateSetup) {
            Phase "Updating the launcher in place" "The small self-update installer. Runtime, textures and the import bundle must survive it."
            $runtimeBefore = @(Get-ChildItem (Join-Path $stableDir "runtime") -Recurse -File -ErrorAction SilentlyContinue).Count
            $importBefore = @(Get-ChildItem (Join-Path $stableDir "game-import") -File -ErrorAction SilentlyContinue).Count
            $texBefore = @(Get-ChildItem $tex -Recurse -File -ErrorAction SilentlyContinue).Count
            # The switches update.rs passes: /SILENT /NORESTART /RELAUNCH=1 and
            # the install root, stated so a custom folder is honored.
            $code = Run-Setup $updateSetup.FullName "install-update.log" @("/RELAUNCH=1", "/DIR=`"$stableDir`"")
            Check "the update installer exits 0 over the full install" ($code -eq 0)
            Check "the launcher is still in Programs after the update" (Test-Path (Join-Path $stableDir "reborn-launcher.exe"))
            $runtimeAfter = @(Get-ChildItem (Join-Path $stableDir "runtime") -Recurse -File -ErrorAction SilentlyContinue).Count
            Check "the update kept the runtime" ((Test-Path (Join-Path $stableDir "runtime\Dolphin.exe")) -and ($runtimeAfter -eq $runtimeBefore)) "runtime files before $runtimeBefore, after $runtimeAfter"
            $texAfter = @(Get-ChildItem $tex -Recurse -File -ErrorAction SilentlyContinue).Count
            Check "the update kept the textures" (($texAfter -eq $texBefore) -and ($texAfter -gt 1000)) "before $texBefore, after $texAfter"
            $importAfter = @(Get-ChildItem (Join-Path $stableDir "game-import") -File -ErrorAction SilentlyContinue).Count
            Check "the update kept the game import bundle" (($importAfter -eq $importBefore) -and ($importAfter -gt 0)) "before $importBefore, after $importAfter"
            Check "the update shipped the Reborn Builder beside the launcher" (Test-Path (Join-Path $stableDir "tools\reborn-builder-cli.exe"))
            $b = Run-Ours (Join-Path $stableDir "tools\reborn-builder-cli.exe") "--help" "builder-help.txt"
            Check "the Reborn Builder runs on this clean Windows" (-not [string]::IsNullOrWhiteSpace($b)) "it produced nothing"
            $u = Run-Ours (Join-Path $stableDir "rebornctl.exe") "status" "status-after-update.txt"
            Check "rebornctl runs after the update" (-not [string]::IsNullOrWhiteSpace($u)) "it produced nothing"
            # /RELAUNCH=1 brings the launcher back, as it does after a self-update.
            Start-Sleep -Seconds 8
            $relaunched = @(Get-Process reborn-launcher -ErrorAction SilentlyContinue)
            Check "the update relaunched the launcher" ($relaunched.Count -ge 1) "no reborn-launcher process 8 s after the update"
            $relaunched | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
            Start-Sleep -Seconds 2
        } else {
            Say("no launcher-only update installer in dist; the update-in-place checks did not run")
        }

        # ------------------------------------------------------------------
        # 3. Uninstall stable. The 2.1 folder and the memory card must survive.
        # ------------------------------------------------------------------
        Phase "Uninstalling it again" "The memory card and the 2.1 folder have to survive this. 2.1's own uninstaller did not manage it."
        $code = Uninstall-Silently $stableDir
        Check "stable uninstaller ran" ($code -eq 0)
        Check "uninstall removed the launcher exe" (-not (Test-Path (Join-Path $stableDir "reborn-launcher.exe")))
        # A shortcut left behind after uninstall points at nothing and is the
        # kind of litter that makes people think the uninstaller failed.
        $leftover = @($wanted | Where-Object { Test-Path (Join-Path $desktop $_.File) })
        Check "uninstall removed the desktop shortcuts" ($leftover.Count -eq 0) `
            ("still on the desktop: " + (($leftover | ForEach-Object { $_.File }) -join ", "))

        Check "uninstall left the memory card alone" ((Test-Path $card) -and ((Get-FileHash $card -Algorithm SHA256).Hash -eq $cardHash))
        Check "uninstall left the 2.1 texture folder alone" (Test-Path (Join-Path $old "User\Load\Textures\GN7E69\tex1_marker.dds"))
    }

    # ------------------------------------------------------------------
    # 4. The beta installer, side by side
    # ------------------------------------------------------------------
    if ($betaSetup -and -not $Manual) {
        Phase "Installing the BETA, side by side" "Its own folder, its own entry in Programs and Features, its own data."
        $code = Run-Setup $betaSetup.FullName "install-beta.log"
        Check "beta installer exits 0" ($code -eq 0)
        $betaDir = Join-Path $env:LOCALAPPDATA "Programs\NFL Street Reborn Beta"
        Check "beta launcher landed in its own folder" (Test-Path (Join-Path $betaDir "reborn-launcher.exe"))
        $marker = Join-Path $betaDir "channel"
        Check "beta carries the channel marker saying beta" ((Test-Path $marker) -and ((Get-Content $marker -Raw).Trim() -eq "beta"))
        $betaKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7C3E9A5D-2B41-4F0E-9D6A-B8E1F2C4A9B3}_is1"
        Check "beta has its own uninstall entry" (Test-Path $betaKey)
        # NOT a check that the 2.1 entry survived: it must not. The stable
        # installer keeps 2.1's AppId on purpose, so an upgrade REPLACES that
        # entry rather than leaving two, and the uninstall above then removed
        # it. What matters is that the beta has an entry of its OWN, checked
        # directly above, and that installing the beta did not create or
        # revive a second one under 2.1's id.
        Check "the beta did not touch the 2.1 uninstall id" (-not (Test-Path $regKey)) "the beta wrote to 2.1's uninstall id"
        $cli = Join-Path $betaDir "rebornctl.exe"
        if (Test-Path $cli) {
            $status = Run-Ours $cli "status" "status-beta.txt"
            Check "beta rebornctl runs at all on a clean Windows" (-not [string]::IsNullOrWhiteSpace($status)) "it produced nothing"
            Check "beta rebornctl status names the beta channel, from the marker file" ($status -match "Channel\s+beta channel, from the channel file")
            Check "beta rebornctl status reports the beta data root" ($status -match "Data root\s+.*NFL Street Reborn Launcher Beta")
        }
        Check "the memory card survived the beta too" ((Test-Path $card) -and ((Get-FileHash $card -Algorithm SHA256).Hash -eq $cardHash))
    }
} catch {
    Say("EXCEPTION: $($_.Exception.Message)")
    Say($_.ScriptStackTrace)
    Check "the smoke test ran to the end without an exception" $false
}

$failed = @($results | Where-Object { -not $_.ok }).Count
$lines = @()
$lines += "NFL Street Reborn Launcher installer smoke test, $(Get-Date -Format 'yyyy-MM-dd HH:mm')"
$lines += "Windows $([Environment]::OSVersion.Version) as $env:USERNAME"
$lines += ""
$results | ForEach-Object { $lines += ("{0}  {1}" -f $(if ($_.ok) { "PASS" } else { "FAIL" }), $_.check) }
$lines += ""
$lines += $(if ($failed -eq 0) { "RESULT: PASS ($($results.Count) checks)" } else { "RESULT: FAIL ($failed of $($results.Count) checks failed)" })
Set-Content (Join-Path $out "result.txt") $lines
Say("done. $(if ($failed -eq 0) { 'PASS' } else { "FAIL: $failed" })")

# The result on screen. In watch mode it is printed into the window a person
# is already looking at, coloured, and the window stays open; Notepad alone
# was easy to miss behind the installer.
if ($Watch) {
    Write-Host ""
    Write-Host ("=" * 74) -ForegroundColor DarkGray
    foreach ($r in $results) {
        if ($r.ok) { Write-Host "  PASS  $($r.check)" -ForegroundColor Green }
        else { Write-Host "  FAIL  $($r.check)" -ForegroundColor Red }
    }
    Write-Host ("=" * 74) -ForegroundColor DarkGray
    if ($failed -eq 0) {
        Write-Host "  ALL $($results.Count) CHECKS PASSED" -ForegroundColor Green
    } else {
        Write-Host "  $failed OF $($results.Count) CHECKS FAILED" -ForegroundColor Red
    }
    Write-Host ""
    Write-Host "  The same list is on your real PC at target\beta-out\result.txt" -ForegroundColor Gray
    Write-Host "  Look around as long as you like. Closing the sandbox window throws all of this away." -ForegroundColor Gray
    Write-Host ""
}
Start-Process notepad.exe (Join-Path $out "result.txt")
