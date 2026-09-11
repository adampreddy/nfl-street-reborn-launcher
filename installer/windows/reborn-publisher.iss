; =============================================================================
; Reborn Publisher, Windows installer
;
; The Publisher is Adam's admin tool: plan, run and verify a release over
; reborn_core::publish. It is not shipped to players, never goes on the CDN,
; and is never in the player download or the launcher's own installer. This
; script exists so it installs on his machine like any other program instead
; of running out of a build directory.
;
; Reused from reborn-launcher.iss rather than invented fresh: same staged
; payload, same "one job, put files on disk and make shortcuts" shape, same
; no-redirect-stderr build discipline in build.ps1. What is missing here on
; purpose is everything the launcher needs and the Publisher does not: there
; is no data root to protect (the Publisher writes nothing a player's saves
; could collide with), no channel, no game images, no runtime, no textures,
; and no self-update relaunch.
;
; -----------------------------------------------------------------------------
; BUILDING
; -----------------------------------------------------------------------------
;
;   ISCC.exe reborn-publisher.iss /DPayload="C:\path\to\staged\payload"
;
; The payload folder is whatever should land in {app}: reborn-publisher.exe
; and rebornctl.exe, staged beside it because the Publisher shells out to it
; for the actual publish operations (see crates/reborn-publisher/src/roster.rs).
; Built by build.ps1 -Publisher, never by hand.
; =============================================================================

#define AppPublisher "NFL Street Reborn"
#define AppURL       "https://nflstreetreborn.com"
#define PublisherExe "reborn-publisher.exe"

; A GUID of its own. This never shares an identity with the launcher's
; AppId (stable or beta): the Publisher is a separate program with its own
; Programs and Features entry, and an upgrade of one must never touch the
; other's install directory.
#define AppId   "{{AFCB3C13-AE81-45DB-AC18-8BF63468ABE6}"
#define AppName "Reborn Publisher"
#define DirName "Reborn Publisher"

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
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}

; Program files, under the same Programs root the launcher uses, but its own
; folder: the two installers never share a directory.
; One folder in Roaming, beside the launcher's own data, the way Adam
; asked for on 2026-09-10: the program sits in its own `app` subfolder so
; an uninstall (which removes only what it installed) cannot reach the
; publisher's settings or journal beside it.
DefaultDirName={userappdata}\NFL Street Reborn Launcher\publisher\app
DefaultGroupName={#AppName}

UsePreviousAppDir=no
UsePreviousGroup=no
UsePreviousTasks=no

; No elevation. The Publisher writes only under the user's own profile
; (settings, the signing vault, the journal), so an installer that asks for
; administrator rights it does not need trains Adam to grant them anyway.
PrivilegesRequired=lowest

DisableDirPage=no
DisableProgramGroupPage=yes
DisableReadyPage=no

OutputDir={#OutDir}
OutputBaseFilename={#AppName} Setup {#AppVersion}

; No #ifdef Span here on purpose. The whole payload is two statically linked
; executables, tens of megabytes, nowhere near Inno's single-file ceiling, so
; there is nothing to size-check and nothing for build.ps1 to retry against.
Compression=lzma2/fast
SolidCompression=yes
WizardStyle=modern

; The Publisher's own icon (crates/reborn-publisher/icons/icon.ico), not the
; launcher's NSR branding: this is a different program and should look like
; one in Programs and Features and on the Start menu.
SetupIconFile=..\..\crates\reborn-publisher\icons\icon.ico

; Never ask for a restart. Nothing installed here is in use during setup;
; the Publisher is not running while its own installer runs.
RestartIfNeededByRun=no
AlwaysRestart=no
ArchitecturesInstallIn64BitMode=x64
MinVersion=10.0.17763

UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#PublisherExe}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Desktop shortcut to Reborn Publisher"; \
  GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Everything staged in the payload folder: reborn-publisher.exe and
; rebornctl.exe beside it. One line, same as the launcher, so a third file
; staged into the payload later needs no script change here.
Source: "{#Payload}\*"; DestDir: "{app}"; \
  Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#PublisherExe}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#PublisherExe}"; \
  Tasks: desktopicon

[Run]
Filename: "{app}\{#PublisherExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; \
  Flags: nowait postinstall skipifsilent

; NO [UninstallDelete] SECTION. Inno removes what it installed and nothing
; else. The signing vault, settings and journal the Publisher writes under
; the user's own profile survive an uninstall, same reasoning as the
; launcher's saves: an admin tool that quietly deleted release history on
; uninstall would be its own kind of disaster.

[Code]

// The Publisher is a Tauri window, same as the launcher, so it draws with
// WebView2 the same way and fails the same way without it: a blank window
// with no explanation. Adam's own machine has always had it, since the
// launcher needs it too, so this is a check rather than a bundle: no redist
// is carried here, just the warning that names the cause if it is ever
// missing (a fresh machine, a rebuilt one).
//
// NOTE ON COMMENT STYLE: these are // line comments, not `;` and not Pascal
// brace blocks. A `;` here is a statement separator, not a comment marker,
// and reading it as one is exactly the mistake that produced "'BEGIN'
// expected" on this file's first compile.
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
  if not WebView2Present() then
  begin
    if MsgBox(
      'The Microsoft Edge WebView2 runtime was not found. Reborn Publisher '
      + 'uses it to draw its window, and without it the window opens blank.'
      + #13#10#13#10
      + 'Install anyway? WebView2 can be added afterwards and Reborn '
      + 'Publisher will then start normally.',
      mbConfirmation, MB_YESNO) = IDNO then
      Result := False;
  end;
end;

// The Publisher used to install under {localappdata}\Programs\Reborn
// Publisher. With the same AppId and a new folder, that install would keep
// its files while its uninstall entry was overwritten: an orphan with a
// stale rebornctl.exe. Its own uninstaller runs first, the way the
// launcher's installer does for its old program folder. Our own folder
// only: below Programs, never Programs itself, and holding the Publisher.
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
  if (Pos(Programs, Lowercase(Value)) = 1)
     and (Lowercase(ExtractFileDir(Value)) + '\' <> Programs)
     and FileExists(Value)
     and FileExists(ExtractFileDir(Value) + '\{#PublisherExe}') then
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
  if DirExists(ExtractFileDir(Uninstaller)) then
  begin
    if DelTree(ExtractFileDir(Uninstaller), True, True, True) then
      Log('Removed the leftover program folder ' + ExtractFileDir(Uninstaller))
    else
      Log('Could not remove everything under ' + ExtractFileDir(Uninstaller) + '; continuing.');
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    RemovePreviousProgramsInstall();
end;
