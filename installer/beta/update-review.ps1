# Run only inside the disposable Windows Sandbox created by Tools/review-update-sandbox.ps1.
$ErrorActionPreference = 'Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or -not (Test-Path 'C:\update-review\out\expected.json')) {
    throw 'This test must run inside its disposable Windows Sandbox.'
}
$out = 'C:\update-review\out'
$expected = Get-Content "$out\expected.json" -Raw | ConvertFrom-Json
$checks = [Collections.Generic.List[object]]::new()
function Note($message) { Add-Content "$out\progress.log" "$(Get-Date -Format o) $message" }
function Check($name, $condition) {
    $checks.Add([pscustomobject]@{name=$name;passed=[bool]$condition})
    Note "$name : $condition"
    if (-not $condition) { throw "Check failed: $name" }
}
function RunInstaller($arguments, $label) {
    Note "Starting $label"
    $process = Start-Process -FilePath "C:\update-review\dist\$($expected.file_name)" -ArgumentList ($arguments + @('/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=`"$out\$label.log`"")) -PassThru -WindowStyle Hidden
    if (-not $process.WaitForExit(120000)) { throw "$label exceeded two minutes (PID $($process.Id))." }
    $process.Refresh()
    Note "$label exit $($process.ExitCode)"
    return $process.ExitCode
}
try {
    Note 'Installing the cached, signed WebView2 runtime inside the sandbox.'
    $redist = Start-Process -FilePath 'C:\update-review\redist\MicrosoftEdgeWebView2RuntimeInstallerX64.exe' -ArgumentList '/silent /install' -PassThru -WindowStyle Hidden
    if (-not $redist.WaitForExit(180000)) { throw 'WebView2 installation exceeded three minutes.' }
    $webviewKey = 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    Check 'WebView2 is installed' ((Test-Path "HKCU:\$webviewKey") -or (Test-Path "HKLM:\Software\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"))
    $old = Join-Path $env:APPDATA 'NFL Street Reborn'
    $data = Join-Path $env:APPDATA 'NFL Street Reborn Launcher'
    $betaData = Join-Path $env:APPDATA 'NFL Street Reborn Launcher Beta'
    $card = Join-Path $old 'User\GC\MemoryCardA.USA.raw'
    New-Item -ItemType Directory -Path (Split-Path $card),$data,$betaData -Force | Out-Null
    Set-Content -LiteralPath $card 'existing player save fixture' -NoNewline
    $cardHash = (Get-FileHash -LiteralPath $card).Hash
    $registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}_is1'
    New-Item -Path $registry -Force | Out-Null
    New-ItemProperty -Path $registry -Name InstallLocation -Value $old -Force | Out-Null
    New-ItemProperty -Path $registry -Name DisplayVersion -Value '2.1' -Force | Out-Null
    $attempt = 0
    foreach ($destination in @($old, "$old\Launcher", $data, $betaData, "$old\..\NFL Street Reborn\Nested")) {
        $attempt++
        $code = RunInstaller @('/VERYSILENT', "/DIR=`"$destination`"") "refused-$attempt"
        Check "Silent install refuses protected destination $attempt" ($code -ne 0)
        Check "Refusal $attempt needs no dialog" (-not ((Get-Content "$out\refused-$attempt.log" -Raw) -match 'Message box'))
        Check "Refusal $attempt preserves the memory card" ((Get-FileHash -LiteralPath $card).Hash -eq $cardHash)
        Check "Refusal $attempt preserves the previous uninstall location" ((Get-ItemProperty $registry).InstallLocation -eq $old)
        Check "Refusal $attempt writes no launcher" (-not (Test-Path -LiteralPath (Join-Path $destination 'reborn-launcher.exe')))
    }
    $install = 'C:\Reborn Update Review'
    New-Item -ItemType Directory -Path $install -Force | Out-Null
    $kept = @{}
    foreach ($relative in @('runtime\runtime-marker.txt','game-import\patch-marker.txt','content\texture-marker.dds','User\GC\local-card.raw','Games\other-game.iso')) {
        $file = Join-Path $install $relative
        New-Item -ItemType Directory -Path (Split-Path $file) -Force | Out-Null
        Set-Content -LiteralPath $file "Preserve existing file: $relative" -NoNewline
        $kept[$file] = (Get-FileHash -LiteralPath $file).Hash
    }
    $code = RunInstaller @('/VERYSILENT', "/DIR=`"$install`"") 'install-custom'
    Check 'Installer accepts a separate custom program directory' ($code -eq 0)
    $exe = Join-Path $install 'reborn-launcher.exe'
    Check 'Installed launcher matches the verified release binary' ((Get-FileHash -LiteralPath $exe).Hash -eq $expected.launcher_sha256)
    Check 'Custom directory is registered for uninstall' ((Get-ItemProperty $registry).InstallLocation.TrimEnd('\') -eq $install)
    $code = RunInstaller @('/SILENT', '/RELAUNCH=1', "/DIR=`"$install`"") 'update-custom'
    Check 'Silent self-update completes' ($code -eq 0)
    Start-Sleep -Seconds 8
    $launchers = @(Get-Process -Name reborn-launcher -ErrorAction SilentlyContinue | Where-Object Path -eq $exe)
    Check 'Self-update relaunches the installed launcher' ($launchers.Count -eq 1)
    Check 'The relaunched application creates its window' ($launchers[0].MainWindowHandle -ne 0)
    foreach ($file in $kept.Keys) { Check "Update preserves $(Split-Path $file -Leaf)" ((Get-FileHash -LiteralPath $file).Hash -eq $kept[$file]) }
    Check 'Update preserves the legacy memory card' ((Get-FileHash -LiteralPath $card).Hash -eq $cardHash)
    foreach ($launcher in $launchers) {
        [void]$launcher.CloseMainWindow()
        if (-not $launcher.WaitForExit(10000)) { throw 'The sandbox launcher did not close.' }
    }
    Note 'Uninstalling only the sandbox application.'
    $uninstaller = Start-Process -FilePath (Join-Path $install 'unins000.exe') -ArgumentList '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' -PassThru -WindowStyle Hidden
    if (-not $uninstaller.WaitForExit(60000)) { throw 'Sandbox uninstall exceeded one minute.' }
    Start-Sleep -Seconds 3
    Check 'Uninstall removes the launcher' (-not (Test-Path -LiteralPath $exe))
    foreach ($file in $kept.Keys) { Check "Uninstall preserves existing $(Split-Path $file -Leaf)" ((Get-FileHash -LiteralPath $file).Hash -eq $kept[$file]) }
    Check 'Uninstall preserves the legacy memory card' ((Get-FileHash -LiteralPath $card).Hash -eq $cardHash)
    $result = 'passed'
} catch {
    Note "ERROR: $($_.Exception.Message)"
    $result = 'failed'
    $failure = $_.Exception.Message
} finally {
    [pscustomobject]@{result=$result;error=$failure;installer_sha256=$expected.installer_sha256;checks=@($checks.ToArray());finished=(Get-Date -Format o)} | ConvertTo-Json -Depth 5 | Set-Content "$out\result.json"
}
