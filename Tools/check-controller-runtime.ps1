param([Parameter(Mandatory=$true)][string]$Runtime)
$ErrorActionPreference = 'Stop'
$Runtime = (Resolve-Path -LiteralPath $Runtime).Path
$root = Split-Path -Parent $PSScriptRoot
$scratch = Join-Path $root ('target/runtime-controller-probe/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
function Start-Probe([string]$Argument, [bool]$Raw) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Runtime
    $info.WorkingDirectory = Split-Path -Parent $Runtime
    $info.Arguments = '"' + $Argument + '"'
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $info.EnvironmentVariables['REBORN_RAW_CONTROLLERS'] = $(if ($Raw) { '1' } else { '0' })
    return [Diagnostics.Process]::Start($info)
}
function Read-Probe([string]$Flag, [bool]$Raw) {
    $out = Join-Path $scratch ($Flag + '-' + $Raw + '.txt')
    $process = Start-Probe ('--' + $Flag + '=' + $out) $Raw
    try {
        if (-not $process.WaitForExit(20000)) { throw "$Flag timed out" }
        if ($process.ExitCode -ne 0) { throw "$Flag exited with $($process.ExitCode)" }
        if (-not (Test-Path -LiteralPath $out)) { throw "$Flag wrote no output" }
        return [IO.File]::ReadAllText($out)
    } finally { if (-not $process.HasExited) { $process.Kill() }; $process.Dispose() }
}
$legacy = Read-Probe 'list-input-devices' $false
$raw = Read-Probe 'list-input-devices' $true
$dump = Read-Probe 'dump-device-inputs' $true
if ($legacy -match '(?m)^SDL/') { throw 'Legacy list unexpectedly contains SDL' }
$dumpNames = @($dump -split '\r?\n' | Where-Object { $_.StartsWith("DEVICE`t") } | ForEach-Object { $_.Substring(7) } | Sort-Object)
$rawNames = @($raw -split '\r?\n' | Where-Object { $_ } | Sort-Object)
if (($dumpNames -join "`n") -ne ($rawNames -join "`n")) { throw 'List and dump disagree; check for a concurrent reconnect' }
$captureDir = Join-Path $scratch 'capture'
New-Item -ItemType Directory -Path $captureDir | Out-Null
$process = Start-Probe ('--input-capture=' + $captureDir) $true
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    while (-not (Test-Path (Join-Path $captureDir 'ready'))) {
        if ($process.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Capture did not become ready' }
        Start-Sleep -Milliseconds 50
    }
    $capture = [IO.File]::ReadAllText((Join-Path $captureDir 'devices.txt'))
    $captureNames = @($capture -split '\r?\n' | Where-Object { $_.StartsWith("DEVICE`t") } | ForEach-Object { $_.Substring(7) } | Sort-Object)
    if (($captureNames -join "`n") -ne ($rawNames -join "`n")) { throw 'Capture and enumeration disagree' }
    # A nonexistent device must time out cleanly, without capturing keyboard
    # activity or input from somebody else's controller.
    [IO.File]::WriteAllText((Join-Path $captureDir 'request.txt'), "SDL/9999/Reborn test missing device`n")
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (Test-Path (Join-Path $captureDir 'result.txt'))) {
        if ($process.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Capture did not answer' }
        Start-Sleep -Milliseconds 50
    }
    if ([IO.File]::ReadAllText((Join-Path $captureDir 'result.txt')).Trim() -ne 'NONE') { throw 'Missing device captured an input' }
    [IO.File]::WriteAllText((Join-Path $captureDir 'quit'), "1`n")
    if (-not $process.WaitForExit(10000) -or $process.ExitCode -ne 0) { throw 'Capture failed to shut down cleanly' }
} finally {
    [IO.File]::WriteAllText((Join-Path $captureDir 'quit'), "1`n")
    if (-not $process.HasExited) { $process.Kill() }
    $process.Dispose()
}
[ordered]@{ result='PASS'; runtime=$Runtime; devices=$rawNames; evidence=$scratch;
    checks=@('legacy filter','raw list/dump agreement','capture readiness and device agreement','missing-device timeout','clean shutdown') } | ConvertTo-Json -Depth 3
