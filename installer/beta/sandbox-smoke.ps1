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

$ErrorActionPreference = "Continue"
$out = "C:\beta\out"
$dist = "C:\beta\dist"
New-Item -ItemType Directory -Force $out | Out-Null
$log = Join-Path $out "smoke.log"
$results = New-Object System.Collections.ArrayList

function Say($message) {
    $line = "{0}  {1}" -f (Get-Date -Format "HH:mm:ss"), $message
    Add-Content $log $line
    Write-Host $line
}
function Check($what, $ok) {
    [void]$results.Add([pscustomobject]@{ ok = [bool]$ok; check = $what })
    Say(("[{0}] {1}" -f $(if ($ok) { "PASS" } else { "FAIL" }), $what))
}
function Run-Setup($exe, $logName, $extra = @()) {
    $args = @("/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/LOG=`"$out\$logName`"") + $extra
    Say("running $(Split-Path $exe -Leaf) $($args -join ' ')")
    $p = Start-Process -FilePath $exe -ArgumentList $args -Wait -PassThru
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

    # ------------------------------------------------------------------
    # 2. The stable installer, as an upgrade over it
    # ------------------------------------------------------------------
    if ($stableSetup) {
        $code = Run-Setup $stableSetup.FullName "install-stable.log"
        Check "stable installer exits 0 over a 2.1 install" ($code -eq 0)

        $stableDir = Join-Path $env:LOCALAPPDATA "Programs\NFL Street Reborn Launcher"
        Check "stable launcher landed in Programs, not on the 2.1 data folder" (Test-Path (Join-Path $stableDir "reborn-launcher.exe"))
        Check "the runtime shipped beside it" (Test-Path (Join-Path $stableDir "runtime\Dolphin.exe"))
        Check "no channel marker in a stable install" (-not (Test-Path (Join-Path $stableDir "channel")))
        Check "nothing was installed into the 2.1 folder" (-not (Test-Path (Join-Path $old "reborn-launcher.exe")))
        $loc = (Get-ItemProperty $regKey -ErrorAction SilentlyContinue).InstallLocation
        Check "the uninstall entry now points at Programs (was the 2.1 data folder)" ($loc -like "*\Programs\NFL Street Reborn Launcher*")
        Check "the memory card is byte for byte what it was" ((Test-Path $card) -and ((Get-FileHash $card -Algorithm SHA256).Hash -eq $cardHash))

        $cli = Join-Path $stableDir "rebornctl.exe"
        if (Test-Path $cli) {
            & $cli --help *> (Join-Path $out "rebornctl-help.txt")
            $help = Get-Content (Join-Path $out "rebornctl-help.txt") -Raw
            if ($help -match "\bpaths\b") {
                & $cli paths *> (Join-Path $out "paths-stable.txt")
                $paths = Get-Content (Join-Path $out "paths-stable.txt") -Raw
                Check "stable rebornctl reports the stable data root" ($paths -match "NFL Street Reborn Launcher" -and $paths -notmatch "Launcher Beta")
            } else {
                Say("rebornctl has no paths subcommand; skipped that check")
            }
        }
        Say("WebView2 after stable install: $(WebView2Version)")
        Check "WebView2 runtime present after install (needs network in the sandbox)" ($null -ne (WebView2Version))

        # ------------------------------------------------------------------
        # 3. Uninstall stable. The 2.1 folder and the memory card must survive.
        # ------------------------------------------------------------------
        $code = Uninstall-Silently $stableDir
        Check "stable uninstaller ran" ($code -eq 0)
        Check "uninstall removed the launcher exe" (-not (Test-Path (Join-Path $stableDir "reborn-launcher.exe")))
        Check "uninstall left the memory card alone" ((Test-Path $card) -and ((Get-FileHash $card -Algorithm SHA256).Hash -eq $cardHash))
        Check "uninstall left the 2.1 texture folder alone" (Test-Path (Join-Path $old "User\Load\Textures\GN7E69\tex1_marker.dds"))
    }

    # ------------------------------------------------------------------
    # 4. The beta installer, side by side
    # ------------------------------------------------------------------
    if ($betaSetup) {
        $code = Run-Setup $betaSetup.FullName "install-beta.log"
        Check "beta installer exits 0" ($code -eq 0)
        $betaDir = Join-Path $env:LOCALAPPDATA "Programs\NFL Street Reborn Launcher Beta"
        Check "beta launcher landed in its own folder" (Test-Path (Join-Path $betaDir "reborn-launcher.exe"))
        $marker = Join-Path $betaDir "channel"
        Check "beta carries the channel marker saying beta" ((Test-Path $marker) -and ((Get-Content $marker -Raw).Trim() -eq "beta"))
        $betaKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7C3E9A5D-2B41-4F0E-9D6A-B8E1F2C4A9B3}_is1"
        Check "beta has its own uninstall entry" (Test-Path $betaKey)
        Check "the 2.1 uninstall entry was not touched by the beta" ((Get-ItemProperty $regKey -ErrorAction SilentlyContinue).DisplayVersion -ne $null)
        $cli = Join-Path $betaDir "rebornctl.exe"
        if ((Test-Path $cli) -and ((Get-Content (Join-Path $out "rebornctl-help.txt") -Raw -ErrorAction SilentlyContinue) -match "\bpaths\b")) {
            & $cli paths *> (Join-Path $out "paths-beta.txt")
            $paths = Get-Content (Join-Path $out "paths-beta.txt") -Raw
            Check "beta rebornctl reports the beta data root" ($paths -match "NFL Street Reborn Launcher Beta")
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

# Leave the sandbox open with the result on screen, so a person can look
# around, open the launcher, and close the window when done.
Start-Process notepad.exe (Join-Path $out "result.txt")
