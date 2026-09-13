param([Parameter(Mandatory)][string]$Dolphin,[Parameter(Mandatory)][string]$Game)
$ErrorActionPreference='Stop'
$reviewRoot=[IO.Path]::GetFullPath((Join-Path (Split-Path $PSScriptRoot) 'target\live-settings-review'))
if (!(Test-Path -LiteralPath $Dolphin) -or !(Test-Path -LiteralPath $Game)) {throw 'Provide the built Dolphin and an existing game image.'}
function Read-State([string]$file) {try {Get-Content -Raw -LiteralPath $file | ConvertFrom-Json} catch {$null}}
function Wait-State([string]$file,[scriptblock]$ready) {
    $until=(Get-Date).AddSeconds(25)
    do {$value=Read-State $file;if ($value -and (& $ready $value)) {return $value};Start-Sleep -Milliseconds 150} while ((Get-Date) -lt $until)
    throw "Timed out waiting for $file. Last state: $($value | ConvertTo-Json -Compress -Depth 5)"
}
foreach ($mode in @('offline','netplay')) {
    $profile=Join-Path $reviewRoot $mode
    New-Item -ItemType Directory -Force -Path (Join-Path $profile 'Config') | Out-Null
    @'
[Core]
GFXBackend = D3D
CPUThread = False
RAMOverrideEnable = True
MEM1Size = 67108864
SlotA = 255
SlotB = 255
SIDevice0 = 6
[Analytics]
PermissionAsked = True
Enabled = False
[Interface]
ConfirmStop = False
SkipNKitWarning = True
[Display]
RenderToMain = True
Fullscreen = False
[DSP]
Muted = True
'@ | Set-Content -LiteralPath (Join-Path $profile 'Config\Dolphin.ini') -Encoding utf8NoBOM
    @'
[Settings]
InternalResolution = 3
MSAA = 1
SSAA = False
'@ | Set-Content -LiteralPath (Join-Path $profile 'Config\GFX.ini') -Encoding utf8NoBOM
    $args=@('-u',('"'+$profile+'"'),'-C','Dolphin.Core.SlotA=255','-C','Dolphin.Core.SlotB=255')
    @'
[Options]
WriteToFile = True
Verbosity = 4
[Logs]
BOOT = True
COMMON = True
CORE = True
VIDEO = True
'@ | Set-Content -LiteralPath (Join-Path $profile 'Config\Logger.ini') -Encoding utf8NoBOM
    $session=Join-Path $profile 'netplay.json'
    if ($mode -eq 'netplay') {
        @{game=$Game;mode='host'} | ConvertTo-Json | Set-Content -LiteralPath $session -Encoding utf8NoBOM
        $args+=@('--reborn-netplay-session',('"'+$session+'"'),'-C','Dolphin.NetPlay.TraversalChoice=direct','-C','Dolphin.NetPlay.HostPort=32627','-C','Dolphin.NetPlay.Nickname=LiveSettingsQA','-C','Dolphin.NetPlay.WriteSaveData=False')
    } else {$args+=@('-e',('"'+$Game+'"'))}
    $process=Start-Process -FilePath $Dolphin -WorkingDirectory (Split-Path $Dolphin) -ArgumentList $args -WindowStyle Hidden -PassThru
    try {
        $live=Join-Path $profile 'Config\RebornLive.json'
        $state=Wait-State $live {param($s) $s.pid -eq $process.Id -and $s.instance}
        if ($mode -eq 'netplay') {
            $room=Wait-State ($session+'.status') {param($s) $s.phase -eq 'lobby'}
            @{id=1;action='start'} | ConvertTo-Json | Set-Content -LiteralPath ($session+'.command') -Encoding utf8NoBOM
        }
        $state=Wait-State $live {param($s) $s.pid -eq $process.Id -and $s.running}
        if (($mode -eq 'netplay') -ne [bool]$state.netplay) {throw 'Unexpected game mode'}
        foreach ($command in @(
            @{id='volume';action='volume';value=31},
            @{id='mute';action='muted';value=$true},
            @{id='pads';action='controllers';pads=@(@{Device='XInput/0/Gamepad';'Buttons/B'='`Button B`';'Buttons/X'='`Button X`'},@{Device=''},@{Device=''},@{Device='XInput/3/Gamepad';'Buttons/A'='`Button A`'})}
        )) {
            $command.pid=$process.Id;$command.instance=$state.instance
            $command | ConvertTo-Json -Depth 5 -Compress | Set-Content -LiteralPath ($live+'.command.tmp') -Encoding utf8NoBOM
            Move-Item -LiteralPath ($live+'.command.tmp') -Destination ($live+'.command') -Force
            $reply=Wait-State $live {param($s) $s.id -eq $command.id}
            if ($reply.error -or !$reply.running) {throw 'The live change failed or stopped the game'}
        }
        if ($reply.volume -ne 31 -or !$reply.muted) {throw 'The in-memory audio settings did not change'}
        Write-Output "PASS $mode`: volume, mute, B/B and X/X, and four-player bindings applied while the game stayed running."
    } finally {
        # Only the process this test started, with memory cards disabled.
        if (!$process.HasExited) {Stop-Process -Id $process.Id; $process.WaitForExit()}
    }
}
