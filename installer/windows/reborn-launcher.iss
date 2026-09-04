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
;    folder holding the player's memory cards. Here {app} is under
;    {localappdata}\Programs and the launcher's data root is under
;    {userappdata}, on separate branches, so uninstall physically cannot reach
;    saves.
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
  #define DirName "NFL Street Reborn Launcher Beta"
#elif Channel == "stable"
  ; The 2.1 AppId is kept ON PURPOSE. It is how an upgrade recognizes an
  ; existing install instead of leaving two entries in Programs and Features.
  ; It is only safe alongside UsePreviousAppDir=no below, and the two must
  ; never be separated.
  #define AppId   "{{DCD6B9B9-11DE-4AC0-A7F5-256E7AC55A7E}"
  #define AppName "NFL Street Reborn"
  #define DirName "NFL Street Reborn Launcher"
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
DefaultDirName={localappdata}\Programs\{#DirName}
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
OutputBaseFilename={#AppName} Launcher Setup {#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

; The 2.1 branding, which is the product's own and which players recognize.
; It lives in the repository rather than on one machine's Desktop, where the
; old script pointed and where no other checkout could find it.
SetupIconFile=art\NSR.ico
WizardImageFile=art\welcome.bmp
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

; Deliberately absent, and each for a reason:
;   InfoBeforeFile, InfoAfterFile  2.1 showed a README page before and after.
;                                  The launcher's own first run says what to do
;                                  next, where it can also check it happened,
;                                  and two extra pages of text are two extra
;                                  clicks between a player and the game. Say the
;                                  word and they come back; the files still
;                                  exist beside the 2.1 script.
;   DiskSpanning                   nothing here is delivered on floppies

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; \
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
; and it downloads the runtime itself, so this step needs the network. Run
; only when the runtime is absent; on a machine that has it nothing happens.
;
; It downloads about 130 MB, so on a clean machine this step sits for roughly
; 90 seconds with no progress of its own (measured in Windows Sandbox,
; 2026-09-04: 88 seconds the first time, 16 seconds the second, when the
; runtime was already there). The status message stays short anyway: a
; paragraph on the progress bar reads worse than a pause does.
[Files]
Source: "{#Redist}\MicrosoftEdgeWebView2Setup.exe"; DestDir: "{tmp}"; \
  Flags: deleteafterinstall

[Run]
Filename: "{tmp}\MicrosoftEdgeWebView2Setup.exe"; Parameters: "/silent /install"; \
  StatusMsg: "Installing Microsoft Edge WebView2"; \
  Check: not WebView2Present; Flags: waituntilterminated
#endif

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#LauncherExe}"
Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#LauncherExe}"; \
  Tasks: desktopicon
Name: "{group}\{cm:UninstallProgram,{#AppName} Launcher}"; Filename: "{uninstallexe}"

; No shortcut to the install folder, and none to Dolphin or a mod manager. The
; launcher is the way in. A desktop full of shortcuts to internals is how people
; end up running the emulator directly and wondering why nothing is configured.

[Run]
Filename: "{app}\{#LauncherExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; \
  Flags: nowait postinstall skipifsilent

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

function InstallDirIsSafe(Dir: String): Boolean;
var
  Normalized, Old: String;
begin
  Normalized := Lowercase(RemoveBackslashUnlessRoot(Dir));
  Old := Lowercase(RemoveBackslashUnlessRoot(OldDataRoot()));
  // Equal to it, or inside it. Both are fatal for the same reason.
  Result := (Normalized <> Old) and (Pos(Old + '\', Normalized + '\') <> 1);
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if CurPageID = wpSelectDir then
  begin
    if not InstallDirIsSafe(WizardDirValue) then
    begin
      MsgBox(
        'That folder is the old NFL Street Reborn data folder, which holds '
        + 'memory cards and settings.' + #13#10#13#10
        + 'Installing the program files there would mean uninstalling later '
        + 'deletes your saves. Please choose a different folder.' + #13#10#13#10
        + WizardDirValue,
        mbError, MB_OK);
      Result := False;
    end;
  end;
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
