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
;    subfolder INSIDE the data root ({userappdata}\NFL Street Reborn
;    Launcher\app since beta.8; {localappdata}\Programs before that): the
;    uninstaller empties that subfolder and nothing beside it, so it cannot
;    reach saves, the game library or Dolphin's user folders.
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
  ; The launcher's data root for this channel (reborn_core's PRODUCT_DATA
  ; plus the channel suffix). The program installs into `app` inside it.
  #define DataDir "NFL Street Reborn Launcher Beta"
#elif Channel == "stable"
  ; The 2.1 AppId is kept ON PURPOSE. It is how an upgrade recognizes an
  ; existing install instead of leaving two entries in Programs and Features.
  ; It is only safe alongside UsePreviousAppDir=no below, and the two must
  ; never be separated.
  #define AppId   "{{DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}"
  #define AppName "NFL Street Reborn"
  #define DirName "NFL Street Reborn"
  #define DataDir "NFL Street Reborn Launcher"
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
; put this elsewhere, and hiding the choice does not make it safer.
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

UninstallDisplayName={#AppName} Launcher
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
  Flags: ignoreversion recursesubdirs createallsubdirs

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
Name: "{group}\{#AppName} Launcher"; Filename: "{app}\{#LauncherExe}"
Name: "{group}\Play {#AppName}"; Filename: "{app}\{#LauncherExe}"; \
  Parameters: "--play"; Comment: "Start the game without opening the launcher"
Name: "{group}\{#AppName} files"; Filename: "{app}\{#LauncherExe}"; \
  Parameters: "--folder"; Comment: "Open the folder the game is installed in"
Name: "{group}\{cm:UninstallProgram,{#AppName} Launcher}"; Filename: "{uninstallexe}"
; And one in the install folder itself, so a player who goes looking for
; the program finds the way out beside it. Settings has a button for it too.
Name: "{app}\Uninstall {#AppName} Launcher"; Filename: "{uninstallexe}"

Name: "{userdesktop}\{#AppName} Launcher"; Filename: "{app}\{#LauncherExe}"; \
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

// The old 2.1 data root. If the install directory ever resolves here, the
// uninstaller would delete the player's memory cards, and every reassurance at
// the top of this file stops being true.
function OldDataRoot(): String;
begin
  Result := ExpandConstant('{userappdata}\NFL Street Reborn');
end;

// The program lives INSIDE the data root, in its own `app` subfolder, so a
// player finds everything in one place (Adam: "put everything in roaming like
// we used to"). That subfolder, and only that subfolder, is a safe install
// directory under a data root: the uninstaller removes what it installed
// under `app` and cannot reach `library`, `saves` or `dolphin-users` beside
// it. The data root itself, or any other folder inside it, stays refused.
function InstallDirIsSafe(Dir: String): Boolean;
var
  Normalized, Old, Current, Beta: String;
begin
  Normalized := Lowercase(RemoveBackslashUnlessRoot(ExpandFileName(Dir)));
  Old := Lowercase(RemoveBackslashUnlessRoot(OldDataRoot()));
  Current := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher'));
  Beta := Lowercase(ExpandConstant('{userappdata}\NFL Street Reborn Launcher Beta'));
  if (Normalized = Current + '\app') or (Normalized = Beta + '\app') then
  begin
    Result := True;
    exit;
  end;
  // Equal to it, or inside it. Both are fatal for the same reason.
  Result := (Pos(Old + '\', Normalized + '\') <> 1)
    and (Pos(Current + '\', Normalized + '\') <> 1)
    and (Pos(Beta + '\', Normalized + '\') <> 1);
end;

function UnsafeInstallDirMessage(): String;
begin
  Result := 'That folder contains NFL Street Reborn save data or settings.'
    + #13#10#13#10
    + 'Choose a separate folder for the program, such as the default `app` '
    + 'folder. Your existing files have not been changed.';
end;

// A 3.0 build before beta.8 installed its program under
// {localappdata}\Programs. Left there, an upgrade to the new location would
// strand up to 8 GB of shipped content and a second uninstall entry. That
// older uninstaller removes only what it installed (it has no
// [UninstallDelete] section), so it is safe to run silently first. The 2.1
// uninstaller lives in {userappdata}\NFL Street Reborn and would destroy
// saves; it is never under Programs, so this never runs it.
function PreviousProgramsInstall(): String;
var
  Key, Value, Programs: String;
begin
  Result := '';
  Key := 'Software\Microsoft\Windows\CurrentVersion\Uninstall\' + ExpandConstant('{#AppId}') + '_is1';
  if not RegQueryStringValue(HKCU, Key, 'UninstallString', Value) then
    exit;
  Value := RemoveQuotes(Trim(Value));
  Programs := Lowercase(ExpandConstant('{localappdata}\Programs\'));
  if (Pos(Programs, Lowercase(Value)) = 1) and FileExists(Value) then
    Result := Value;
end;

procedure RemovePreviousProgramsInstall();
var
  Uninstaller: String;
  Code: Integer;
begin
  Uninstaller := PreviousProgramsInstall();
  if Uninstaller = '' then
    exit;
  Log('Removing the previous program folder first: ' + Uninstaller);
  if not Exec(Uninstaller, '/VERYSILENT /NORESTART /SUPPRESSMSGBOXES', '', SW_HIDE, ewWaitUntilTerminated, Code) then
    Log('The previous uninstaller could not be started; continuing.')
  else
    Log('The previous uninstaller finished with code ' + IntToStr(Code));
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
  RemovePreviousProgramsInstall();
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

procedure CurStepChanged(CurStep: TSetupStep);
begin
  // ssInstall fires before a single file is copied, so the download runs
  // alongside the copy instead of after it.
  if CurStep = ssInstall then
    StartWebView2;
  if CurStep = ssPostInstall then
    WaitForWebView2;
end;
#endif

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
