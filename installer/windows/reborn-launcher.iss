; =============================================================================
; NFL Street Reborn Launcher, Windows installer
;
; Inno Setup, reused deliberately. It is the right Windows installer and it is
; already what ships, so this is the existing rebornsetupv10.iss reduced rather
; than a replacement.
;
; ONE JOB: put files on the disk and make shortcuts.
;
; Everything a person has to decide now happens in the launcher's onboarding:
; picking mods, setting up the controller, choosing graphics, migrating an old
; install. That is not a style preference, it is what makes this script safe.
; The old installer ran a patcher, opened JSGME for mod selection, tried to back
; up user data and deleted textures on update, and none of that can be undone if
; it goes wrong at 40% through a wizard.
;
; -----------------------------------------------------------------------------
; THE THREE THINGS THAT USED TO EAT SAVES, AND WHAT STOPS THEM HERE
; -----------------------------------------------------------------------------
;
; 1. DefaultDirName was {userappdata}\NFL Street Reborn, which made the install
;    root and the data root the same folder. The uninstaller then removed the
;    folder holding the player's memory cards. Here {app} is the `app`
;    subfolder INSIDE the data root ({userappdata}\NFL Street Reborn[ Beta]\app
;    since 2026-09-12; {userappdata}\NFL Street Reborn Launcher[ Beta]\app
;    from beta.8; {localappdata}\Programs before that): the uninstaller
;    empties that subfolder and nothing beside it, so it cannot reach saves,
;    the game library or Dolphin's user folders. The stable data root is the
;    folder 2.1 used; the launcher moves a 2.1 install found there into
;    `Reborn 2.1` at its first start, and never deletes it.
;
; 2. UsePreviousAppDir was never set, so Inno defaulted it to YES. With the old
;    AppId retained, an upgrade looks up the previous install's recorded
;    directory and puts {app} back on the old data root, silently undoing the
;    fix above on exactly the machines that already have 2.1. It is set to no
;    explicitly below, and there is a check at the bottom that refuses to
;    install if {app} ever resolves to the old location anyway.
;
; 3. [UninstallDelete] removed {app}\User\StateSaves and {app}\User\Load, so
;    uninstalling destroyed savestates and a 27,000 file texture pack. This
;    script has no [UninstallDelete] at all. Inno removes what it installed and
;    nothing else.
;
; The old BackupUserData and RestoreUserData are simply gone. They passed
; wildcard paths to Inno's FileCopy, which copies one file and does not expand
; wildcards, so the backup never worked. It logged a failure and continued,
; which is worse than not having one.
;
; -----------------------------------------------------------------------------
; BUILDING
; -----------------------------------------------------------------------------
;
;   ISCC.exe reborn-launcher.iss /DPayload="C:\path\to\staged\payload"
;
; The payload folder is whatever should land in {app}. Nothing is hardcoded to
; one machine's Desktop, which the old script was, and there is no fixed list of
; game images: put however many are wanted in Games\ and they all install.
; =============================================================================

#define AppPublisher   "NFL Street Reborn"
#define AppURL         "https://nflstreetreborn.com"
#define LauncherExe    "reborn-launcher.exe"

; The channel. "stable" is what players get. "beta" installs SIDE BY SIDE
; with its own AppId, name and folder, and through the `channel` marker the
; build script lays in the payload, its own data root inside the launcher.
; That is what makes a beta a test environment rather than a gamble with the
; player's real install: nothing it writes lands where the stable launcher
; reads, and uninstalling it removes only itself.
#ifndef Channel
  #define Channel "stable"
#endif
#if Channel == "beta"
  #define AppId   "{{7C3E9A5D-2B41-4F0E-9D6A-B8E1F2C4A9B3}"
  #define AppName "NFL Street Reborn Beta"
  #define DirName "NFL Street Reborn Beta"
  ; The launcher's data root for this channel (reborn_core's PRODUCT plus
  ; the channel suffix). The program installs into `app` inside it.
  #define DataDir "NFL Street Reborn Beta"
#elif Channel == "stable"
  ; The 2.1 AppId is kept ON PURPOSE. It is how an upgrade recognizes an
  ; existing install instead of leaving two entries in Programs and Features.
  ; It is only safe alongside UsePreviousAppDir=no below, and the two must
  ; never be separated.
  #define AppId   "{{DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}"
  #define AppName "NFL Street Reborn"
  #define DirName "NFL Street Reborn"
  ; The folder 2.1 used. Its uninstaller sits there under this same AppId
  ; and is never run (see PreviousLauncherInstall); the launcher moves the
  ; 2.1 files into `Reborn 2.1` inside it at first start.
  #define DataDir "NFL Street Reborn"
#else
  #error Channel must be "stable" or "beta"
#endif

; Overridable at build time so a release does not need a source edit.
#ifndef AppVersion
  #define AppVersion "3.0.0"
#endif
#ifndef Payload
  #define Payload "..\..\payload"
#endif
#ifndef OutDir
  #define OutDir "..\..\dist"
#endif

[Setup]
; Per channel; see the defines at the top. The stable one is 2.1's AppId.
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}

; Program files, NOT the data root. See note 1 at the top.
;
; The folder is the product's name with no "Launcher" on the end: this is
; under LOCALAPPDATA\Programs, a different root from the APPDATA folder 2.1
; owns, so the plain name cannot collide with it. The application is the
; thing that carries the word, not its folder.
; Since beta.8: the program lives in `app` INSIDE the launcher's data root,
; so a player has one folder (Adam: "put everything in roaming like we used
; to"). Note 1 still holds: {app} is a subfolder the uninstaller can empty,
; never the data root itself, and InstallDirIsSafe below allows exactly this
; one subfolder under a data root and refuses every other.
DefaultDirName={userappdata}\{#DataDir}\app
DefaultGroupName={#AppName}

; See note 2. Without these two an upgrade puts {app} back on the old data root.
UsePreviousAppDir=no
UsePreviousGroup=no
UsePreviousTasks=no
; NOTE: CONTENT_AND_UPDATE_MODEL.md section 16 asks for a
; "UsePreviousData" directive as well. There is no such directive in Inno
; Setup 6; the compiler rejects it outright, which is how this was caught.
; The three above are the real ones, and UsePreviousAppDir is the one that
; matters.

; No elevation. The launcher writes only under the user's own profile, and an
; installer that asks for administrator rights it does not need trains people to
; grant them.
PrivilegesRequired=lowest

; The folder page stays. Someone with a small system drive needs to be able to
; put this elsewhere, and hiding the choice does not make it safer. Since
; 2026-10-09 a folder on another drive takes the game's data with it on a
; first install (PlaceDataBesideProgram), not only the program.
DisableDirPage=no
DisableProgramGroupPage=yes
DisableReadyPage=no

OutputDir={#OutDir}
#ifndef OfflineSuffix
  #define OfflineSuffix ""
#endif
; A launcher-only build (build.ps1 -LauncherOnly) is named Update, not
; Setup: it is the file the launcher fetches to update itself, and it cannot
; start a fresh player, so it must never be picked up as the first-run set.
#ifdef UpdateBuild
OutputBaseFilename={#AppName} Launcher Update {#AppVersion}
#else
OutputBaseFilename={#AppName} Launcher Setup {#AppVersion}{#OfflineSuffix}
#endif

; Disk spanning, passed by build.ps1 when the payload warrants it.
;
; The documentation says the threshold is 4,200,000,000 compressed bytes. The
; 6.2.0 compiler on this machine disagrees: it refuses at 2,100,000,000 with
; "Disk spanning must be enabled". That measured number is DiskSliceSize, and
; it is why build.ps1 retries on that exact message rather than trying to
; predict the compressed size in advance. Below the limit, one file is the
; better download.
#ifdef Span
DiskSpanning=yes
DiskSliceSize=2100000000
#endif
Compression=lzma2/fast
SolidCompression=yes
WizardStyle=modern

; The 2.1 branding, which is the product's own and which players recognize.
; It lives in the repository rather than on one machine's Desktop, where the
; old script pointed and where no other checkout could find it.
SetupIconFile=art\NSR.ico
WizardImageFile=art\launcher.bmp
WizardSmallImageFile=art\corner.bmp

; The welcome page exists to show that art. Inno 6 hides it by default; 2.1
; showed it, and it is the first thing a player sees.
DisableWelcomePage=no

; Never ask for a restart.
;
; Inno defaults RestartIfNeededByRun to yes, so a [Run] program returning
; 3010 (ERROR_SUCCESS_REBOOT_REQUIRED) makes Setup demand a reboot. The
; WebView2 bootstrapper can return exactly that, and it does not need one:
; the runtime works the moment it is installed. Nothing this installer puts
; on disk is in use during setup either, since the launcher is not running.
; Observed asking for a restart in Windows Sandbox, 2026-09-04.
RestartIfNeededByRun=no
AlwaysRestart=no
; x64, not x64compatible: the latter needs Inno 6.3 and the compiler here
; is 6.2.0, which rejects it. Worth pinning in a comment because the newer
; spelling is what current documentation shows.
ArchitecturesInstallIn64BitMode=x64
MinVersion=10.0.17763

UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#LauncherExe}

; The 2.1 README pages, restored 2026-09-04.
;
; I had left these out, reasoning that the launcher's own first run says what
; to do next. Wrong on both pages. The before page is the only place a player
; is told they need their own legally dumped USA copy of NFL Street 2, which
; is the single thing that makes the whole install useless if they lack it,
; and it is where BlueSwordM's Dolphin build and nfl_numbers' textures are
; credited. Permission and attribution do not belong behind a first run the
; player only reaches once the install has already finished.
;
; They live in the repository now, not on one machine's Desktop where the old
; script pointed, for the same reason the artwork moved: no other checkout
; could build.
InfoBeforeFile=text\README.txt
InfoAfterFile=text\READMEAFTER.txt

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
; The launcher on the desktop, ticked by default: it is the way in, and a
; player who wanted it and did not notice the box has to go looking in the
; Start menu for something they just installed.
Name: "desktopicon"; Description: "Desktop shortcut to the launcher"; \
  GroupDescription: "{cm:AdditionalIcons}"

; Two extras, off unless asked for. Both run the LAUNCHER with a switch,
; never Dolphin and never a bare folder path: a shortcut that went straight
; to the emulator would boot with no mods composed, no configuration
; written and no match results collected, and it would look like it worked.
Name: "gameicon"; Description: "Desktop shortcut that starts the game straight away"; \
  GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "foldericon"; Description: "Desktop shortcut to the folder the game is installed in"; \
  GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Everything staged in the payload folder, recursively.
;
; ONE line rather than an entry per file, so adding a game image or a runtime
; file needs no script change. The old script named NSR.iso explicitly, which
; meant a second image was silently not installed.
Source: "{#Payload}\*"; DestDir: "{app}"; \
  Excludes: "runtime\ReShade.ini,runtime\NSRShaderlessMenu.ini,runtime\reshade-presets\*"; \
  Flags: ignoreversion recursesubdirs createallsubdirs

; ReShade's configuration and the effect presets, installed ONLY when they are
; not already there.
;
; ReShade writes the player's in-overlay tuning straight back into these files.
; Under the blanket ignoreversion above, re-running Setup - which players do to
; repair an install - threw that tuning away every time. `onlyifdoesntexist`
; keeps a fresh install correct and leaves an existing one alone. The launcher
; restores any of these that goes missing.
Source: "{#Payload}\runtime\ReShade.ini"; DestDir: "{app}\runtime"; \
  Flags: onlyifdoesntexist skipifsourcedoesntexist
Source: "{#Payload}\runtime\NSRShaderlessMenu.ini"; DestDir: "{app}\runtime"; \
  Flags: onlyifdoesntexist skipifsourcedoesntexist
Source: "{#Payload}\runtime\reshade-presets\*"; DestDir: "{app}\runtime\reshade-presets"; \
  Flags: onlyifdoesntexist recursesubdirs createallsubdirs skipifsourcedoesntexist

; Note what is NOT here.
;
; No User folder. The launcher owns its own data root under {userappdata} and
; creates what it needs on first run. Shipping a User tree into {app} is how the
; install root became the data root in the first place.
;
; No MODS folder and no JSGME. Mods are chosen in onboarding, and the launcher
; composes them over a pristine tree by hardlink rather than moving originals
; into a !BACKUP folder it might lose.
;
; No patcher. Patching a game image is the launcher's job, where it can verify
; the result and say what happened.

#ifdef Redist
; The WebView2 runtime, which draws the launcher's window. Windows 11 and an
; updated Windows 10 already have it; a fresh image, and Windows Sandbox, do
; not, and the failure without it is a window that opens blank. The
; bootstrapper is Microsoft's own, verified by its signature at build time,
; and it downloads the runtime itself, so this step needs the network.
;
; It is started at the START of installation and waited for at the end; see
; CurStepChanged below. As a [Run] entry it ran AFTER every file was copied,
; so a clean machine waited for the copy and then waited again for a download
; that could have been happening the whole time. Measured in Windows Sandbox,
; 2026-09-04: 88 seconds for the first install and 16 for the second, when the
; runtime was already there.
;
; dontcopy, so it is only unpacked when it is actually needed. A machine that
; already has WebView2 never writes it to disk at all.
;
; WebView2File is the bootstrapper by default, 1.7 MB, which downloads the
; runtime. Built with -OfflineWebView2 it is the standalone runtime instead,
; 247 MB, which installs without a network at all. Same code path either
; way: start it, then wait for the registry to say the runtime is there.
#ifndef WebView2File
  #define WebView2File "MicrosoftEdgeWebView2Setup.exe"
#endif
[Files]
Source: "{#Redist}\{#WebView2File}"; Flags: dontcopy
#endif

[Icons]
; The Start menu gets all three; the desktop gets what was ticked.
Name: "{group}\{#AppName}"; Filename: "{app}\{#LauncherExe}"
Name: "{group}\Play {#AppName}"; Filename: "{app}\{#LauncherExe}"; \
  Parameters: "--play"; Comment: "Start the game without opening the launcher"
Name: "{group}\{#AppName} files"; Filename: "{app}\{#LauncherExe}"; \
  Parameters: "--folder"; Comment: "Open the folder the game is installed in"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
; And one in the install folder itself, so a player who goes looking for
; the program finds the way out beside it. Settings has a button for it too.
Name: "{app}\Uninstall {#AppName}"; Filename: "{uninstallexe}"

Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#LauncherExe}"; \
  Tasks: desktopicon
Name: "{userdesktop}\Play {#AppName}"; Filename: "{app}\{#LauncherExe}"; \
  Parameters: "--play"; Comment: "Start the game without opening the launcher"; \
  Tasks: gameicon
Name: "{userdesktop}\{#AppName} files"; Filename: "{app}\{#LauncherExe}"; \
  Parameters: "--folder"; Comment: "Open the folder the game is installed in"; \
  Tasks: foldericon

; Still no shortcut to Dolphin and none to a mod manager. Every one of these
; runs the launcher, which is what composes the mods, writes the emulator's
; configuration and collects match results afterwards. The files shortcut
; asks the launcher where the folder is rather than carrying a path decided
; at install time, because which folder that is depends on the emulator in
; use and on whether a 2.1 install is present.

[Registry]
; The invite link a host copies from the Online page:
; nflstreetreborn://join/<code>. Opening one starts this launcher with the
; link as its argument; it remembers the room code and lands the player on
; Online with the room filled in. Per user (HKCU), the way everything else
; here is, so no elevation. One key for both channels: the last install
; wins it, and only the stable uninstall removes it, so removing a beta
; that sat beside the stable launcher never breaks the stable one's links.
#if Channel == "stable"
Root: HKCU; Subkey: "Software\Classes\nflstreetreborn"; ValueType: string; ValueName: ""; ValueData: "URL:NFL Street Reborn invite"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\nflstreetreborn"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\nflstreetreborn\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#LauncherExe}"",0"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\nflstreetreborn\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#LauncherExe}"" ""%1"""; Flags: uninsdeletekey
#else
Root: HKCU; Subkey: "Software\Classes\nflstreetreborn"; ValueType: string; ValueName: ""; ValueData: "URL:NFL Street Reborn invite"
Root: HKCU; Subkey: "Software\Classes\nflstreetreborn"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\nflstreetreborn\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#LauncherExe}"",0"
Root: HKCU; Subkey: "Software\Classes\nflstreetreborn\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#LauncherExe}"" ""%1"""
#endif

[Run]
; Named in full on the Finish page: what is about to open is the launcher,
; not the game, and a player who ticks this should know which one they get.
Filename: "{app}\{#LauncherExe}"; Description: "{cm:LaunchProgram,{#AppName} Launcher}"; \
  Flags: nowait postinstall skipifsilent

; The self-update relaunch. The launcher downloads a verified installer, runs
; it with /SILENT /RELAUNCH=1 and exits so its files can be replaced. The
; entry above is skipped when silent, so without this one the launcher would
; never come back and the player would be left looking at nothing. Gated on
; the parameter so a plain silent install, which the sandbox smoke test runs
; and then starts the launcher itself, does not start it twice.
Filename: "{app}\{#LauncherExe}"; Flags: nowait; Check: RelaunchRequested

; NO [UninstallDelete] SECTION. See note 3 at the top. Inno removes what it
; installed. Anything the launcher created under {userappdata} survives, which
; includes memory cards, and that is the entire point.

[Code]

// Two guards. Both exist because a wrong answer here is unrecoverable, and both
// are checks rather than comments, because a comment does not fail a build.
//
// NOTE ON COMMENT STYLE: these are // line comments, not Pascal brace blocks.
// An Inno constant written inside a brace comment closes that comment at its
// own closing brace, and the compiler then reports "BEGIN expected" pointing at
// the line AFTER the comment rather than at the comment itself. Worth stating
// once, because it cost a compile here.

// The program lives INSIDE the data root, in its own `app` subfolder, so a
// player finds everything in one place (Adam: "put everything in roaming like
// we used to"). That subfolder, and only that subfolder, is a safe install
// directory under a data root: the uninstaller removes what it installed
// under `app` and cannot reach `library`, `saves` or `dolphin-users` beside
// it. The data root itself, any other folder inside it, and the older
// `Launcher` roots stay refused. Since 2026-09-12 the stable data root is
// the folder 2.1 used; the launcher moves a 2.1 install found there into
// `Reborn 2.1` at its first start, so `app` beside it is still safe.
function InstallDirIsSafe(Dir: String): Boolean;
var
  Normalized, Own: String;
  Roots: array[0..3] of String;
  I: Integer;
begin
  Normalized := Lowercase(RemoveBackslashUnlessRoot(ExpandFileName(Dir)));
  Own := Lowercase(ExpandConstant('{userappdata}\{#DataDir}\app'));
  if Normalized = Own then
  begin
    Result := True;
    exit;
  end;
  Roots[0] := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn'));
  Roots[1] := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Beta'));
  Roots[2] := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher'));
  Roots[3] := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher Beta'));
  Result := True;
  for I := 0 to 3 do
    if Pos(Roots[I] + '\', Normalized + '\') = 1 then
      Result := False;
end;

// A launcher built before 2026-09-12 self-updates with /DIR naming its own
// program folder: {userappdata}\NFL Street Reborn Launcher[ Beta]\app, or a
// folder under {localappdata}\Programs before beta.8. Those are refused by
// InstallDirIsSafe, and a refused silent self-update would leave every
// player on the old build for good. So a retired folder is redirected to
// this channel's `app` here, before any page or check looks at it; the old
// folder is then removed by RemovePreviousLauncherInstall and the launcher
// adopts the data beside it at its first start. (The launcher itself stops
// passing a retired folder from 2026-09-12 on; this is for the ones that
// are already installed.)
function RetiredProgramDir(Dir: String): Boolean;
var
  N, Programs: String;
begin
  N := Lowercase(RemoveBackslashUnlessRoot(ExpandFileName(Dir)));
  Programs := Lowercase(ExpandConstant('{localappdata}\Programs\'));
  Result := (N = Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher\app')))
    or (N = Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher Beta\app')))
    or ((Pos(Programs, N) = 1) and (N + '\' <> Programs));
end;

procedure InitializeWizard();
begin
  if RetiredProgramDir(WizardDirValue) then
  begin
    Log('Retired program folder ' + WizardDirValue + ' redirected to ' + ExpandConstant('{userappdata}\{#DataDir}\app'));
    WizardForm.DirEdit.Text := ExpandConstant('{userappdata}\{#DataDir}\app');
  end;
end;

function UnsafeInstallDirMessage(): String;
begin
  Result := 'That folder contains NFL Street Reborn save data or settings.'
    + #13#10#13#10
    + 'Choose a separate folder for the program, such as the default `app` '
    + 'folder. Your existing files have not been changed.';
end;

// A 3.0 build before beta.8 installed its program under
// {localappdata}\Programs, and builds before 2026-09-12 under
// {userappdata}\NFL Street Reborn Launcher[ Beta]\app. Left there, an
// upgrade to the new location would strand up to 8 GB of shipped content
// and a second uninstall entry. Those uninstallers remove only what they
// installed (no [UninstallDelete] section), so running one silently first
// is safe. The 2.1 uninstaller is registered under the same AppId as
// stable, at {userappdata}\NFL Street Reborn\unins000.exe, and would
// destroy saves: it has no reborn-launcher.exe beside it, so the check
// below never picks it.
function PreviousLauncherInstall(): String;
var
  Key, Value, Folder, Programs, OldStable, OldBeta: String;
begin
  Result := '';
  Key := 'Software\Microsoft\Windows\CurrentVersion\Uninstall\' + ExpandConstant('{#AppId}') + '_is1';
  if not RegQueryStringValue(HKCU, Key, 'UninstallString', Value) then
    exit;
  Value := RemoveQuotes(Trim(Value));
  if not FileExists(Value) then
    exit;
  if not FileExists(ExtractFileDir(Value) + '\{#LauncherExe}') then
    exit;
  Folder := Lowercase(ExtractFileDir(Value));
  Programs := Lowercase(ExpandConstant('{localappdata}\Programs\'));
  OldStable := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher\app'));
  OldBeta := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher Beta\app'));
  // Our own program folder only: below Programs (never Programs itself), or
  // exactly the old `app` folder of a launcher-named data root. The folder
  // this install writes to is never removed here; Inno upgrades it in place.
  if ((Pos(Programs, Folder) = 1) and (Folder + '\' <> Programs))
     or (Folder = OldStable) or (Folder = OldBeta) then
    Result := Value;
end;

procedure RemovePreviousLauncherInstall();
var
  Uninstaller: String;
  Code: Integer;
begin
  Uninstaller := PreviousLauncherInstall();
  if Uninstaller = '' then
    exit;
  Log('Removing the previous program folder first: ' + Uninstaller);
  if not Exec(Uninstaller, '/VERYSILENT /NORESTART /SUPPRESSMSGBOXES', '', SW_HIDE, ewWaitUntilTerminated, Code) then
    Log('The previous uninstaller could not be started; continuing.')
  else
    Log('The previous uninstaller finished with code ' + IntToStr(Code));
  // What the launcher itself wrote beside the program (ReShade logs under
  // runtime, for one) is not in the uninstall log, so the folder survives
  // with a handful of files. It is our own old program folder, never a data
  // root, so finish the job. The data beside it (library, saves,
  // dolphin-users) is left for the launcher to adopt at its first start.
  if DirExists(ExtractFileDir(Uninstaller)) then
  begin
    if DelTree(ExtractFileDir(Uninstaller), True, True, True) then
      Log('Removed the leftover program folder ' + ExtractFileDir(Uninstaller))
    else
      Log('Could not remove everything under ' + ExtractFileDir(Uninstaller) + '; continuing.');
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if CurPageID = wpSelectDir then
  begin
    if not InstallDirIsSafe(WizardDirValue) then
    begin
      if not WizardSilent then
        MsgBox(UnsafeInstallDirMessage(), mbError, MB_OK);
      Result := False;
    end;
  end;
end;

// Validate the final destination again at the write boundary, independently
// of wizard navigation and any future changes to which pages are displayed.
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not InstallDirIsSafe(ExpandConstant('{app}')) then
  begin
    Result := UnsafeInstallDirMessage();
    exit;
  end;
  RemovePreviousLauncherInstall();
end;

// WebView2 draws the launcher's window. It ships with Windows 11 and with any
// updated Windows 10, but a stale image may not have it, and the failure without
// it is a window that opens blank with no explanation. Checked here so the
// message names the cause instead of leaving someone staring at an empty frame.
function WebView2Present(): Boolean;
var
  Version: String;
begin
  Result :=
    RegQueryStringValue(HKLM,
      'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\'
      + '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version)
    or RegQueryStringValue(HKCU,
      'SOFTWARE\Microsoft\EdgeUpdate\Clients\'
      + '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version);
end;

#ifdef Redist
// Started when installation begins, waited for when it ends.
//
// Exec with ewNoWait gives no handle to wait on, so the wait is a poll of the
// same registry check the Check function uses. That is the honest signal
// anyway: what matters is whether the runtime is THERE, not whether a
// particular process has exited.
var
  WebView2Started: Boolean;

procedure StartWebView2;
var
  Code: Integer;
begin
  WebView2Started := False;
  if WebView2Present() then
    Exit;
  ExtractTemporaryFile('{#WebView2File}');
  WebView2Started := Exec(
    ExpandConstant('{tmp}\{#WebView2File}'),
    '/silent /install', '', SW_HIDE, ewNoWait, Code);
end;

procedure WaitForWebView2;
var
  Waited: Integer;
begin
  if not WebView2Started then
    Exit;
  // Five minutes, then give up and carry on. A slow connection must not trap
  // someone in an installer forever, and the launcher without WebView2 is a
  // fixable state: installing the runtime later makes it work.
  Waited := 0;
  while (Waited < 300000) and not WebView2Present() do
  begin
    WizardForm.StatusLabel.Caption := 'Installing Microsoft Edge WebView2';
    Sleep(500);
    Waited := Waited + 500;
    if Waited mod 2000 = 0 then
      WizardForm.Refresh();
  end;
end;

#endif

// The game's data on the drive the player chose (2026-10-09).
//
// The launcher keeps everything heavy (the imported game, the textures,
// Dolphin's folders, the saves) in its data root, {userappdata}\{#DataDir},
// whatever folder this page was given; only the program went where the
// player said. A player who picked D: to keep C: free still filled C:
// ("The game keeps installing itself on my C drive no matter how I try").
//
// When the program folder is on another drive and this PC has no data root
// yet, the data root is made a junction to a folder beside the program, so
// it lands on that drive too. A junction, not a setting: the launcher writes
// its data root's path into its own files, and every one of those paths keeps
// working through it. It is the same link Settings > Files & storage > "Move
// to another drive" leaves, which is how a player who already has data on C:
// moves it.
//
// Never when a data root is already there (an upgrade, a self-update, a 2.1
// install the launcher tucks away, or data the player already moved), and
// never beside a launcher-era root ({#DataDir} Launcher), whose entries the
// launcher adopts by renaming, which cannot cross drives. If the junction
// cannot be made, nothing is lost: the launcher makes its data root on the
// system drive, as it always did.
// A folder this install creates outside the player's profile is made
// private to the player (plus SYSTEM and Administrators) before anything
// goes in it. Under the profile everything is private already; a folder on a
// second drive inherits the drive's permissions, which let every signed-in
// account on the PC create and change files, and the program folder holds
// the launcher and Dolphin (security review, 2026-10-09). Folders that were
// already there are left as the player set them.
function OutsideProfile(Dir: String): Boolean;
begin
  Result := Pos(Lowercase(AddBackslash(ExpandConstant('{%USERPROFILE}'))), Lowercase(AddBackslash(Dir))) <> 1;
end;

function MakePrivate(Dir: String): Boolean;
var
  Tmp, Sid: String;
  Lines: TArrayOfString;
  Code, At: Integer;
begin
  Result := False;
  Tmp := ExpandConstant('{tmp}\whoami.csv');
  if not Exec(ExpandConstant('{cmd}'), '/D /C whoami /user /fo csv /nh > "' + Tmp + '"',
      '', SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) then
    exit;
  if not LoadStringsFromFile(Tmp, Lines) or (GetArrayLength(Lines) = 0) then
    exit;
  At := Pos('"S-1-', Lines[0]);
  if At = 0 then
    exit;
  Sid := Copy(Lines[0], At + 1, Length(Lines[0]));
  StringChangeEx(Sid, '"', '', True);
  Sid := Trim(Sid);
  // SYSTEM and Administrators by well-known id, so a non-English Windows
  // names them correctly.
  Result := Exec(ExpandConstant('{sys}\icacls.exe'),
      '"' + Dir + '" /inheritance:r /grant:r "*' + Sid + ':(OI)(CI)F" "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" /Q',
      '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0);
  if Result then
    Log('Made private to this account: ' + Dir)
  else
    Log('Could not make private: ' + Dir + ' (code ' + IntToStr(Code) + ')');
end;

// ssInstall, before any file is copied: the program folder (and, for the
// `...\<name>\app` shape, the folder holding it, which becomes the data root)
// made private when this install is the one creating it.
procedure MakeNewFoldersPrivate();
var
  App, Parent: String;
begin
  App := RemoveBackslashUnlessRoot(ExpandConstant('{app}'));
  if not OutsideProfile(App) then
    exit;
  Parent := ExtractFileDir(App);
  if (CompareText(ExtractFileName(App), 'app') = 0) and (Length(RemoveBackslashUnlessRoot(Parent)) > 3)
     and not DirExists(Parent) then
    if ForceDirectories(Parent) then
      MakePrivate(Parent);
  if not DirExists(App) then
    if ForceDirectories(App) then
      MakePrivate(App);
end;

function DataBesideProgram(): String;
var
  App, Parent: String;
begin
  App := RemoveBackslashUnlessRoot(ExpandConstant('{app}'));
  Parent := ExtractFileDir(App);
  // ...\NFL Street Reborn\app keeps the default shape: the data root is the
  // folder holding `app`. Anything else, or `app` at the root of a drive,
  // gets a `data` folder inside the program folder, never a drive root or a
  // folder the player did not choose for this.
  if (CompareText(ExtractFileName(App), 'app') = 0) and (Length(RemoveBackslashUnlessRoot(Parent)) > 3) then
    Result := Parent
  else
    Result := AddBackslash(App) + 'data';
end;

procedure PlaceDataBesideProgram();
var
  DataRoot, LauncherEra, Target: String;
  Code: Integer;
  Fresh: Boolean;
begin
  DataRoot := ExpandConstant('{userappdata}\{#DataDir}');
  if CompareText(ExtractFileDrive(ExpandConstant('{app}')), ExtractFileDrive(DataRoot)) = 0 then
    exit;
  if DirExists(DataRoot) or FileExists(DataRoot) then
  begin
    Log('Data root already exists; left where it is: ' + DataRoot);
    exit;
  end;
  LauncherEra := ExpandConstant('{userappdata}\{#DataDir}') + ' Launcher';
#if Channel == "beta"
  LauncherEra := ExpandConstant('{userappdata}\NFL Street Reborn Launcher Beta');
#endif
  if DirExists(LauncherEra) then
  begin
    Log('A launcher-era data root is adopted at first start; data stays on the system drive: ' + LauncherEra);
    exit;
  end;
  Target := DataBesideProgram();
  Fresh := not DirExists(Target);
  if not ForceDirectories(Target) then
  begin
    Log('Could not make ' + Target + '; data stays on the system drive');
    exit;
  end;
  // A data folder this install made, never one others can change: refused
  // (data stays on the system drive) rather than linked to a shared folder.
  if Fresh and OutsideProfile(Target) and not MakePrivate(Target) then
  begin
    RemoveDir(Target);
    exit;
  end;
  if (Pos('"', DataRoot) > 0) or (Pos('"', Target) > 0) then
    exit;
  if Exec(ExpandConstant('{cmd}'), '/D /C mklink /J "' + DataRoot + '" "' + Target + '"',
      '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0) and DirExists(DataRoot) then
    Log('Data root ' + DataRoot + ' leads to ' + Target)
  else
    Log('Could not link ' + DataRoot + ' to ' + Target + ' (code ' + IntToStr(Code) + '); data stays on the system drive');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  // ssInstall fires before a single file is copied, so the WebView2 download
  // runs alongside the copy instead of after it.
  if CurStep = ssInstall then
  begin
    MakeNewFoldersPrivate();
#ifdef Redist
    StartWebView2;
#endif
  end;
  // Before [Run] can start the launcher, which makes the data root on first
  // start if it is not there.
  if CurStep = ssPostInstall then
  begin
    PlaceDataBesideProgram();
#ifdef Redist
    WaitForWebView2;
#endif
  end;
end;

// True only when the launcher started this install to update itself. Read
// from the command line, defaulting to 0, so a person double-clicking Setup
// and the sandbox's silent first install both answer no.
function RelaunchRequested(): Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
#ifndef Redist
  // Only when the bootstrapper is not bundled. With it, the [Run] entry above
  // installs the runtime and there is nothing to ask.
  if not WebView2Present() then
  begin
    if MsgBox(
      'The Microsoft Edge WebView2 runtime was not found. The launcher uses it '
      + 'to draw its window, and without it the window opens blank.'
      + #13#10#13#10
      + 'Install anyway? WebView2 can be added afterwards and the launcher will '
      + 'then start normally.',
      mbConfirmation, MB_YESNO) = IDNO then
      Result := False;
  end;
#endif
end;
