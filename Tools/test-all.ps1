# Run every test there is, and say plainly what is NOT tested.
#
#   .\Tools\test-all.ps1            everything that runs on this machine, ~1 min
#   .\Tools\test-all.ps1 -Sandbox   adds the full install and uninstall run in
#                                   Windows Sandbox, ~15 min, needs the network
#   .\Tools\test-all.ps1 -Manual    same, but you click through the wizard
#
# There was no single entry point before this. The suites existed, in three
# different languages, run by hand in three different ways, which meant
# "everything passes" was a claim nobody could check in one go.
#
# NOTHING here redirects a native command's stderr. In Windows PowerShell 5.1,
# 2>&1 or *>> on cargo or node wraps every stderr line in an ErrorRecord, and
# with ErrorActionPreference set to Stop that turns "Compiling reborn-ui" into
# a fatal error. That exact mistake killed a build earlier today.

param(
    # The full install, uninstall and side by side beta run inside Windows
    # Sandbox. Off by default because it needs the sandbox feature, the
    # network, and about fifteen minutes.
    [switch]$Sandbox,
    # Sandbox, but hand the wizard to a person. This is the only way to see the
    # README pages, the artwork and the Finish text, none of which a silent
    # install draws.
    [switch]$Manual,
    # Skip the Rust suite. For iterating on the checkers alone.
    [switch]$NoRust,
    # Browser interactions and visual captures. Run npm install --prefix Tools first.
    [switch]$Browser
)

$ErrorActionPreference = "Continue"
$root = Resolve-Path "$PSScriptRoot\.."
$results = New-Object System.Collections.ArrayList

function Record($layer, $name, $ok, $detail) {
    [void]$results.Add([pscustomobject]@{
        Layer = $layer; Name = $name; Ok = [bool]$ok; Detail = $detail
    })
    $mark = if ($ok) { "  ok  " } else { " FAIL " }
    Write-Host ("{0} {1,-46} {2}" -f $mark, $name, $detail)
}

Write-Host ""
Write-Host "NFL Street Reborn Launcher, full test run" -ForegroundColor Cyan
Write-Host ("  " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))
Write-Host ""

# -----------------------------------------------------------------------------
# 1. The Rust suites
# -----------------------------------------------------------------------------
if (-not $NoRust) {
    Write-Host "-- Rust ---------------------------------------------------------" -ForegroundColor DarkGray
    Push-Location $root
    # No redirection: cargo writes progress to stderr and results to stdout, so
    # capturing stdout alone gets the summaries and leaves progress on screen.
    $out = & cargo test --workspace
    $code = $LASTEXITCODE
    Pop-Location

    $passed = 0; $failed = 0; $ignored = 0; $suites = 0
    foreach ($line in $out) {
        if ($line -match "^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored") {
            $suites++
            $passed  += [int]$Matches[2]
            $failed  += [int]$Matches[3]
            $ignored += [int]$Matches[4]
        }
    }
    # A green exit with no suites at all means the run did not happen. Reporting
    # that as a pass is how a broken harness looks exactly like a healthy one.
    $ranSomething = $suites -gt 0
    Record "rust" "cargo test --workspace" (($code -eq 0) -and $ranSomething -and ($failed -eq 0)) `
        ("{0} suites, {1} passed, {2} failed, {3} ignored" -f $suites, $passed, $failed, $ignored)
    if (-not $ranSomething) {
        Write-Host "       no suite reported a result. The harness did not run." -ForegroundColor Yellow
    }
    foreach ($line in $out) {
        if ($line -match "^(test .+ \.\.\. FAILED|failures:)") { Write-Host "       $line" -ForegroundColor Yellow }
    }
    Write-Host ""
}

# -----------------------------------------------------------------------------
# 2. The static checkers
# -----------------------------------------------------------------------------
Write-Host "-- Checkers -----------------------------------------------------" -ForegroundColor DarkGray
$checkers = Get-ChildItem (Join-Path $PSScriptRoot "check-*.mjs") | Sort-Object Name
if ($checkers.Count -eq 0) {
    Record "checkers" "found any checkers at all" $false "none matched Tools\check-*.mjs"
}
foreach ($c in $checkers) {
    $o = & node $c.FullName
    $ok = ($LASTEXITCODE -eq 0)
    $last = if ($o) { ($o | Select-Object -Last 1) } else { "no output" }
    Record "checkers" $c.Name $ok $last
    if (-not $ok) { $o | Select-Object -Last 8 | ForEach-Object { Write-Host "       $_" -ForegroundColor Yellow } }
}
Write-Host ""

# -----------------------------------------------------------------------------
# Browser interactions, using a fixture bridge and the installed Edge browser.
if ($Browser) {
    $browserSuite = Join-Path $PSScriptRoot "run-browser-tests.mjs"
    $browserOutput = & node $browserSuite
    Record "browser" "interaction and responsive layout checks" ($LASTEXITCODE -eq 0) ($browserOutput | Select-Object -Last 1)
    foreach ($suite in @("review-onboarding.mjs", "check-update-notice.mjs", "review-publisher.mjs")) {
        $suiteOutput = & node (Join-Path $PSScriptRoot $suite)
        Record "browser" $suite ($LASTEXITCODE -eq 0) ($suiteOutput | Select-Object -Last 1)
    }
}

# 3. Can an installer still be built from this machine
# -----------------------------------------------------------------------------
# Not a build, which takes twenty minutes. Just the inputs, so a missing one is
# found now rather than after the compile.
Write-Host "-- Installer inputs ---------------------------------------------" -ForegroundColor DarkGray
$wizardFiles = @(
    "installer\windows\art\NSR.ico",
    "installer\windows\art\welcome.bmp",
    "installer\windows\art\corner.bmp",
    "installer\windows\text\README.txt",
    "installer\windows\text\READMEAFTER.txt"
)
foreach ($rel in $wizardFiles) {
    $p = Join-Path $root $rel
    $exists = Test-Path $p
    $size = if ($exists) { "{0:N0} bytes" -f (Get-Item $p).Length } else { "MISSING" }
    Record "installer" ("ships " + (Split-Path $rel -Leaf)) $exists $size
}

# The .iss must actually reference the two README files, not merely contain
# them in the folder. A file nobody points at ships nothing.
$iss = Join-Path $root "installer\windows\reborn-launcher.iss"
$issText = if (Test-Path $iss) { Get-Content $iss -Raw } else { "" }
Record "installer" "the wizard is told to show README.txt" `
    ($issText -match "(?m)^\s*InfoBeforeFile\s*=") "InfoBeforeFile directive present"
Record "installer" "the wizard is told to show READMEAFTER.txt" `
    ($issText -match "(?m)^\s*InfoAfterFile\s*=") "InfoAfterFile directive present"
Write-Host ""

# -----------------------------------------------------------------------------
# 4. What is sitting in dist
# -----------------------------------------------------------------------------
Write-Host "-- Built artifacts ----------------------------------------------" -ForegroundColor DarkGray
$dist = Join-Path $root "dist"
#
# NOT "Reborn Publisher Setup <version>.exe": it matches "*Setup*.exe" too,
# it is never the newer of the two once someone builds the admin tool after
# the launcher, and Select-Object -Last 1 by write time would then have this
# check quietly grading the wrong program. It is excluded by name here, the
# same way a beta build already is.
$setup = Get-ChildItem $dist -Filter "*Setup*.exe" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike "*Beta*" -and $_.Name -notlike "Reborn Publisher Setup*" } |
    Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $setup) {
    Record "dist" "a stable installer exists" $false "none in $dist"
} else {
    Record "dist" "a stable installer exists" $true ("{0}, {1:N1} MB" -f $setup.Name, ($setup.Length / 1MB))

    # A spanned build is the exe plus slices. Shipping the exe alone gives a
    # player an installer that stops partway asking for a file they never got.
    # The set is the newest exe's own: "<prefix>.exe" and "<prefix>-<n>.bin".
    # Matching every .bin in dist against every zip compared one build's zip
    # with another build's slices whenever two builds sat in dist together.
    $prefix = [System.IO.Path]::GetFileNameWithoutExtension($setup.Name)
    $slicePattern = "^" + [regex]::Escape($prefix) + "-\d+\.bin$"
    $slices = @(Get-ChildItem $dist -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -match $slicePattern })
    if ($slices.Count -gt 0) {
        # The zip named for this version: "<AppName> Launcher <version>.zip".
        $zipName = ($prefix -replace " Launcher Setup ", " Launcher ") + ".zip"
        $zip = Get-ChildItem $dist -Filter $zipName -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $zip) {
            Record "dist" "the spanned set is packed into one zip" $false `
                ("{0} slice(s) but no {1}: three separate downloads" -f $slices.Count, $zipName)
        } else {
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            $archive = [System.IO.Compression.ZipFile]::OpenRead($zip.FullName)
            try { $names = @($archive.Entries | ForEach-Object { $_.Name }) } finally { $archive.Dispose() }
            $want = @($setup.Name) + @($slices | ForEach-Object { $_.Name })
            $missing = @($want | Where-Object { $names -notcontains $_ })
            Record "dist" "the zip holds the whole spanned set" ($missing.Count -eq 0) `
                $(if ($missing.Count -eq 0) { "{0} entries, {1:N2} GB" -f $names.Count, ($zip.Length / 1GB) }
                  else { "missing from zip: " + ($missing -join ", ") })

            # A zip older than the files it claims to hold is a stale download.
            $newest = ($want | ForEach-Object { (Get-Item (Join-Path $dist $_)).LastWriteTime } |
                Measure-Object -Maximum).Maximum
            Record "dist" "the zip is not older than the build inside it" ($zip.LastWriteTime -ge $newest) `
                ("zip {0:HH:mm:ss}, newest part {1:HH:mm:ss}" -f $zip.LastWriteTime, $newest)
        }
    }
}
# The launcher-only update installer: one file, and latest.json beside it
# must pin THAT file. A manifest pointing at a different build is an update
# that fails its hash check on every player's machine.
$update = Get-ChildItem $dist -Filter "*Launcher Update *.exe" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike "*Beta*" } | Sort-Object LastWriteTime | Select-Object -Last 1
if ($update) {
    $stray = @(Get-ChildItem $dist -Filter ($update.BaseName + "-*.bin") -ErrorAction SilentlyContinue)
    Record "dist" "the update installer is one file" ($stray.Count -eq 0) `
        ("{0}, {1:N1} MB" -f $update.Name, ($update.Length / 1MB))
    $manifest = Join-Path $dist "latest.json"
    if (-not (Test-Path $manifest)) {
        Record "dist" "latest.json pins the update installer" $false "no latest.json in dist"
    } else {
        $m = Get-Content $manifest -Raw | ConvertFrom-Json
        $hash = (Get-FileHash $update.FullName -Algorithm SHA256).Hash.ToLower()
        $pinned = ($m.windows.sha256 -eq $hash) -and ([int64]$m.windows.bytes -eq $update.Length) -and ($m.windows.file_name -eq $update.Name)
        Record "dist" "latest.json pins the update installer by hash, size and name" $pinned `
            ("version {0}, sha256 {1}..., {2}" -f $m.version, $hash.Substring(0, 12), $m.windows.url)
    }
}

# The Publisher installer: Adam's admin tool, never shipped to a player and
# never on the bucket, so it is checked only when one happens to be in dist.
# Two things matter: it is one file (no zip, no slices: build.ps1 -Publisher
# never spans, and throws if a slice ever turned up beside it), and it
# carries both binaries the Publisher needs, itself and rebornctl, which it
# shells out to for the actual publish operations.
#
# Inno's compiled format has no plain "list the files inside" this script can
# read without running or unpacking the installer, and this suite must not
# install anything. So the binaries check reads the exact folder build.ps1
# -Publisher staged and fed to ISCC: reborn-publisher.iss compiles it with
# "Source: {#Payload}\*", verbatim, so what is in that folder is what is in
# the installer.
$publisherSetup = Get-ChildItem $dist -Filter "Reborn Publisher Setup *.exe" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime | Select-Object -Last 1
if ($publisherSetup) {
    $pstray = @(Get-ChildItem $dist -Filter ($publisherSetup.BaseName + "-*.bin") -ErrorAction SilentlyContinue)
    Record "dist" "the Publisher installer is one file" ($pstray.Count -eq 0) `
        ("{0}, {1:N1} MB" -f $publisherSetup.Name, ($publisherSetup.Length / 1MB))

    $publisherPayload = Join-Path $root "target\installer-payload-publisher"
    $publisherBinaries = @("reborn-publisher.exe", "rebornctl.exe")
    $missingBinaries = @($publisherBinaries | Where-Object { -not (Test-Path (Join-Path $publisherPayload $_) -PathType Leaf) })
    Record "dist" "the Publisher installer carries both binaries" ($missingBinaries.Count -eq 0) `
        $(if ($missingBinaries.Count -eq 0) { "reborn-publisher.exe and rebornctl.exe staged in $publisherPayload" }
          else { "missing from the staged payload: " + ($missingBinaries -join ", ") })
}
Write-Host ""

# -----------------------------------------------------------------------------
# 5. The update pipeline, end to end
# -----------------------------------------------------------------------------
# Publish an origin, serve it on loopback with Range support, update a scratch
# data root from it. Nine steps: a fresh install, a repair, a differential
# update that must fetch exactly two blobs, a resume answered 206 from the
# seeded offset, a replay refused by the publisher, a tampered signature
# refused by the client. It never touches the real data root. Needs a debug
# rebornctl, which cargo test does not build.
Write-Host "-- Update pipeline ----------------------------------------------" -ForegroundColor DarkGray
if (-not $NoRust) {
    Push-Location $root
    $null = & cargo build -p reborn-cli
    $built = ($LASTEXITCODE -eq 0)
    Pop-Location
    if (-not $built) {
        Record "update" "rebornctl builds" $false "cargo build -p reborn-cli failed"
    } else {
        $e2e = & py -3 (Join-Path $PSScriptRoot "e2e-update.py")
        $ok = ($LASTEXITCODE -eq 0)
        $last = if ($e2e) { ($e2e | Select-Object -Last 1) } else { "no output" }
        Record "update" "publish, serve, install, repair, update, resume" $ok $last
        if (-not $ok) { $e2e | Select-Object -Last 12 | ForEach-Object { Write-Host "       $_" -ForegroundColor Yellow } }
    }
} else {
    Write-Host "       skipped with -NoRust, it needs a rebornctl build"
}
Write-Host ""

# -----------------------------------------------------------------------------
# 6. The sandbox, on request
# -----------------------------------------------------------------------------
if ($Sandbox -or $Manual) {
    Write-Host "-- Windows Sandbox ----------------------------------------------" -ForegroundColor DarkGray
    $before = @(Get-Process WindowsSandbox* -ErrorAction SilentlyContinue)
    if ($before.Count -gt 0) {
        Record "sandbox" "no sandbox already running" $false `
            "close the open sandbox first, it holds dist read-only"
    } else {
        $sb = Join-Path $PSScriptRoot "beta-sandbox.ps1"
        $resultFile = Join-Path $root "target\beta-out\result.txt"

        # Before launching, so a result.txt left by an earlier run cannot be
        # mistaken for this one's.
        $startedAt = Get-Date
        if ($Manual) { & $sb -Manual } else { & $sb -Watch }

        # beta-sandbox.ps1 starts the sandbox and RETURNS. The run itself
        # happens inside the VM over the next few minutes, so reading the
        # result here without waiting reported "no result.txt was written"
        # for an install that had not been attempted yet.
        $waitMinutes = if ($Manual) { 60 } else { 25 }
        $deadline = $startedAt.AddMinutes($waitMinutes)
        Write-Host ("       waiting up to {0} minutes for the sandbox to finish" -f $waitMinutes)
        $sawResult = $false
        while ((Get-Date) -lt $deadline) {
            if ((Test-Path $resultFile) -and ((Get-Item $resultFile).LastWriteTime -gt $startedAt)) {
                $sawResult = $true
                break
            }
            # If every sandbox process is gone and nothing was written, the
            # window was closed before the run finished. Waiting out the full
            # deadline for a result that can never arrive helps nobody.
            if (-not $Manual -and @(Get-Process WindowsSandbox* -ErrorAction SilentlyContinue).Count -eq 0 `
                -and ((Get-Date) -gt $startedAt.AddMinutes(2))) {
                Write-Host "       the sandbox closed before writing a result" -ForegroundColor Yellow
                break
            }
            Start-Sleep -Seconds 10
        }
        if (-not $sawResult) {
            Write-Host ("       gave up after {0:N1} minutes" -f ((Get-Date) - $startedAt).TotalMinutes) -ForegroundColor Yellow
        }

        # Close it. The result and every log are on the host in beta-out, so
        # nothing is lost, and a sandbox left open blocks the next run by
        # holding dist read-only. Not in manual mode, where a person may still
        # be looking at the wizard.
        if ($sawResult -and -not $Manual) {
            @(Get-Process WindowsSandbox* -ErrorAction SilentlyContinue) | Stop-Process -Force -ErrorAction SilentlyContinue
        }

        if ((Test-Path $resultFile) -and ((Get-Item $resultFile).LastWriteTime -gt $startedAt)) {
            $txt = Get-Content $resultFile -Raw
            # The two shapes the smoke test actually writes. An earlier version
            # of this looked for "N / M", which it never writes, so a finished
            # run reported "could not read a score".
            if ($txt -match "RESULT: PASS \((\d+) checks\)") {
                Record "sandbox" "install, uninstall and beta checks" $true ("all {0} passed" -f $Matches[1])
            } elseif ($txt -match "RESULT: FAIL \((\d+) of (\d+) checks failed\)") {
                Record "sandbox" "install, uninstall and beta checks" $false `
                    ("{0} of {1} failed" -f $Matches[1], $Matches[2])
                Get-Content $resultFile | Where-Object { $_ -like "FAIL *" } |
                    ForEach-Object { Write-Host "       $_" -ForegroundColor Yellow }
            } else {
                Record "sandbox" "install, uninstall and beta checks" $false `
                    "result.txt has no RESULT line: the run did not reach the end"
            }
        } elseif (Test-Path $resultFile) {
            Record "sandbox" "install, uninstall and beta checks" $false `
                ("only a stale result.txt from {0:HH:mm:ss}, before this run started" -f (Get-Item $resultFile).LastWriteTime)
        } else {
            Record "sandbox" "install, uninstall and beta checks" $false "the sandbox never wrote a result"
        }
    }
    Write-Host ""
}

# -----------------------------------------------------------------------------
# The tally, and the honest gaps
# -----------------------------------------------------------------------------
$failedRows = @($results | Where-Object { -not $_.Ok })
Write-Host "=================================================================" -ForegroundColor Cyan
if ($failedRows.Count -eq 0) {
    Write-Host ("ALL {0} CHECKS PASSED" -f $results.Count) -ForegroundColor Green
} else {
    Write-Host ("{0} of {1} CHECKS FAILED" -f $failedRows.Count, $results.Count) -ForegroundColor Red
    $failedRows | ForEach-Object { Write-Host ("  [{0}] {1}  {2}" -f $_.Layer, $_.Name, $_.Detail) -ForegroundColor Red }
}
Write-Host "=================================================================" -ForegroundColor Cyan

Write-Host ""
Write-Host "NOT covered by any of the above:" -ForegroundColor Yellow
Write-Host "  - the wizard's pages as a person sees them: the README before and"
Write-Host "    after, the artwork, the shortcut tick boxes, the Finish text."
Write-Host "    A silent install draws none of them. Use -Manual."
Write-Host "  - the game actually running. Dolphin needs a GPU the sandbox does"
Write-Host "    not give it, so no automated test plays a down of football."
Write-Host "  - anything that needs the update host, which does not exist yet:"
Write-Host "    a real update check, download and apply against a live manifest."
Write-Host "    The unit tests cover the logic against a fake server."
if (-not ($Sandbox -or $Manual)) {
    Write-Host "  - the install itself. Add -Sandbox."
}
Write-Host ""

exit $(if ($failedRows.Count -eq 0) { 0 } else { 1 })
