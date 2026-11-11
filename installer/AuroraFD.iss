﻿; Windows installer of Aurora Viewer +, a personal build of Aurora Viewer.
;
; The installer holds no viewer program: it installs the update engine
; (engine\Update.ps1), which installs the build tools (with the user's
; consent), downloads the public fork, builds the viewer on this PC and
; installs it. "Update Aurora Viewer +" in the Start menu runs the same
; engine later. See docs/INSTALLER.md.
;
; Compiled by .github/workflows/installer.yml, or by hand with Inno Setup
; 6.3 or newer:
;   iscc installer\AuroraFD.iss    (output: installer\output\)

; Strings are taken as written: no backslash escapes (paths)
#pragma parseroption -p+

; The product name: the "+" is an ordinary character in Windows file names,
; shortcut names and the Apps & Features entry. Folders, files and the
; mutex use the ASCII form.
#define AppName "Aurora Viewer +"
; The same folder name as engine\settings.json (folderName)
#define AppFolder "AuroraViewerPlus"
#define AppMutexName "Local\AuroraViewerPlus-update"
#define RepoUrl "https://github.com/ChromeHowl/Aurora-Viewer-1"

; The installer's own version: INSTALLER_VERSION from the workflow (the tag
; installer-v1.2.0 gives 1.2.0), 1.0.0 by hand.
#define AppVersion GetEnv("INSTALLER_VERSION")
#if AppVersion == ""
  #undef AppVersion
  #define AppVersion "1.0.0"
#endif

#define PowerShell "{sys}\WindowsPowerShell\v1.0\powershell.exe"
; the engine's path follows, quoted with doubled quotes ("") in each entry
#define Engine "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File"

[Setup]
AppId={{11496EF1-89B8-4ED0-830C-8B5431E5B821}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName}
AppPublisherURL={#RepoUrl}
AppSupportURL={#RepoUrl}
AppComments=Personal build of Aurora Viewer, built on this PC from source. Not an official Aurora Viewer release.
; No administrator rights for the viewer itself: %LOCALAPPDATA%\Programs
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppFolder}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=no
; a folder chosen with Browse gets the AuroraViewerPlus subfolder
AppendDefaultDirName=yes
AppMutex={#AppMutexName}
LicenseFile=..\LICENSE
InfoBeforeFile=notice.txt
OutputDir=output
OutputBaseFilename={#AppFolder}-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
MinVersion=10.0
; x64 Windows only: the build tools it installs are the x64 ones
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\viewer\aurora-viewer.exe
SetupLogging=yes
CloseApplications=no

[Messages]
WelcomeLabel2=This will install [name] on your computer: a personal build of Aurora Viewer, not an official release.%n%nIt is built on this PC from its public source code. Setup installs the free tools the build needs (it asks you first), downloads the code, builds the viewer (20-60 minutes the first time, about 20 GB of free disk space) and installs it.%n%nLater, "Update [name]" in the Start menu gets the latest version the same way.
FinishedLabel=Setup has finished.%n%nStart [name] from the Start menu or the desktop. If the build window reported a problem, fix what it said, then use "Update [name]" in the Start menu to try again.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "engine\Update.ps1"; DestDir: "{app}\engine"; Flags: ignoreversion
Source: "engine\settings.json"; DestDir: "{app}\engine"; Flags: ignoreversion
Source: "notice.txt"; DestDir: "{app}"; DestName: "About this build.txt"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Dirs]
Name: "{app}\logs"

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\viewer\aurora-viewer.exe"; WorkingDir: "{app}\viewer"; Comment: "Personal build of Aurora Viewer"
Name: "{group}\Update {#AppName}"; Filename: "{#PowerShell}"; Parameters: "{#Engine} ""{app}\engine\Update.ps1"" -Mode Update"; WorkingDir: "{app}"; Comment: "Download, build and install the latest version"
Name: "{group}\Go back to the previous version"; Filename: "{#PowerShell}"; Parameters: "{#Engine} ""{app}\engine\Update.ps1"" -Mode Restore"; WorkingDir: "{app}"; Comment: "Put back the version installed before the last update"
Name: "{group}\Update logs"; Filename: "{app}\logs"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\viewer\aurora-viewer.exe"; WorkingDir: "{app}\viewer"; Tasks: desktopicon

[Run]
; Not a post-install choice: the build is the installation. Its own window
; shows the progress; Setup waits for it.
Filename: "{#PowerShell}"; Parameters: "{#Engine} ""{app}\engine\Update.ps1"" -Mode Install"; WorkingDir: "{app}"; StatusMsg: "Building {#AppName} on this PC: follow the progress in its window (20-60 minutes the first time)..."; Flags: waituntilterminated

[UninstallDelete]
; What the engine made in the install folder, which Setup only accepts
; when it is new, empty or already this program's (NextButtonClick). Never
; the user's settings, chat logs or cache, nor the local patches: they are
; outside it (see [Code]).
Type: filesandordirs; Name: "{app}\viewer"
Type: filesandordirs; Name: "{app}\viewer.previous"
Type: filesandordirs; Name: "{app}\viewer.older"
Type: filesandordirs; Name: "{app}\viewer.new"
Type: filesandordirs; Name: "{app}\viewer.swap"
Type: filesandordirs; Name: "{app}\old-*"
Type: filesandordirs; Name: "{app}\build"
Type: filesandordirs; Name: "{app}\tools"
Type: filesandordirs; Name: "{app}\logs"
Type: files; Name: "{app}\state.json"
Type: dirifempty; Name: "{app}"

[Code]
var
  DeleteUserData: Boolean;

{ The install folder: new, empty, or already this program's (its engine is
  there). Never a folder of other files, whose "build", "tools" or "logs"
  the uninstaller would delete. }
function FolderAccepted(const Dir: String): Boolean;
var
  Found: TFindRec;
begin
  Result := True;
  if not DirExists(Dir) then Exit;
  if FileExists(AddBackslash(Dir) + 'engine\Update.ps1') then Exit;
  if FindFirst(AddBackslash(Dir) + '*', Found) then
  begin
    try
      repeat
        if (Found.Name <> '.') and (Found.Name <> '..') then
          Result := False;
      until (not Result) or (not FindNext(Found));
    finally
      FindClose(Found);
    end;
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Dir: String;
begin
  Result := True;
  if CurPageID = wpSelectDir then
  begin
    Dir := WizardDirValue();
    if (Pos('%', Dir) > 0) or (Pos('"', Dir) > 0) then
    begin
      MsgBox('Please choose a folder whose name has no % and no double quote.', mbError, MB_OK);
      Result := False;
    end
    else if not FolderAccepted(Dir) then
    begin
      MsgBox('This folder already holds other files.' + #13#10 + #13#10 +
        'Choose a new or empty folder, for example ' + AddBackslash(Dir) + '{#AppFolder}', mbError, MB_OK);
      Result := False;
    end;
  end;
end;

{ A viewer started from this install folder (WMI, as the engine does). }
function ViewerRunning(): Boolean;
var
  Locator, Service, Procs, Item: Variant;
  I: Integer;
  Prefix, Path: String;
begin
  Result := False;
  Prefix := Lowercase(AddBackslash(ExpandConstant('{app}')));
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Service := Locator.ConnectServer('.', 'root\CIMV2');
    Procs := Service.ExecQuery('SELECT ExecutablePath FROM Win32_Process WHERE Name = ''aurora-viewer.exe''');
    for I := 0 to Procs.Count - 1 do
    begin
      Item := Procs.ItemIndex(I);
      if not VarIsNull(Item.ExecutablePath) then
      begin
        Path := Lowercase(Item.ExecutablePath);
        if Pos(Prefix, Path) = 1 then
          Result := True;
      end;
    end;
  except
    Log('Could not list the running programs: ' + GetExceptionMessage);
  end;
end;

{ The secondlife:, hop: and x-grid-location-info: link handlers, when they
  open this program's viewer (as the viewer's own removal does: another
  viewer's handler stays). }
procedure RemoveLinkHandlers();
var
  Schemes: array[0..2] of String;
  I: Integer;
  Command, Exe: String;
begin
  Schemes[0] := 'secondlife';
  Schemes[1] := 'hop';
  Schemes[2] := 'x-grid-location-info';
  Exe := Lowercase(ExpandConstant('{app}\viewer\aurora-viewer.exe'));
  for I := 0 to 2 do
    if RegQueryStringValue(HKCU, 'Software\Classes\' + Schemes[I] + '\shell\open\command', '', Command) then
      if Pos(Exe, Lowercase(Command)) > 0 then
      begin
        Log('Removing the link handler ' + Schemes[I]);
        RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Classes\' + Schemes[I]);
      end;
end;

function InitializeUninstall(): Boolean;
begin
  Result := True;
  DeleteUserData := False;
  while ViewerRunning() do
  begin
    if MsgBox('{#AppName} is open. Close it, then click OK to uninstall it.', mbError, MB_OKCANCEL) = IDCANCEL then
    begin
      Result := False;
      Exit;
    end;
  end;
  if not UninstallSilent() then
    DeleteUserData := MsgBox('{#AppName} and its build files will be removed.' + #13#10 + #13#10 +
      'Also delete your viewer data? Yes deletes:' + #13#10 +
      '- the settings, chat logs and cache (%APPDATA%\Aurora\AuroraViewer and %LOCALAPPDATA%\Aurora\AuroraViewer);' + #13#10 +
      '- from the Windows credential store: the saved logins (passwords and second-factor authorizations), the cache key, the translation services'' keys, the SOCKS 5 proxy user name and password, and the GitHub authorization of crash reports;' + #13#10 +
      '- the Aurora add-on of Blender, the Windows notification identity and the secondlife: link handler.' + #13#10 + #13#10 +
      'All of these are shared with Aurora Viewer: an official Aurora Viewer on this PC loses them too. ' +
      'Your local patches (%APPDATA%\{#AppFolder}\local-patches) are never deleted.' + #13#10 + #13#10 +
      'Choose No to keep everything (recommended).', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Code: Integer;
  Viewer: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    Viewer := ExpandConstant('{app}\viewer\aurora-viewer.exe');
    { the viewer's own removal (uninstall.rs): link handler, notification
      identity, credential store entries, Blender add-on }
    if DeleteUserData and FileExists(Viewer) then
      Exec(Viewer, '--uninstall', ExpandConstant('{app}\viewer'), SW_HIDE, ewWaitUntilTerminated, Code);
    { always: a link handler must not point at a deleted program }
    RemoveLinkHandlers();
    { Git marks its object files read-only: clear that so they can be deleted }
    Exec(ExpandConstant('{cmd}'), '/d /c attrib -r "' + ExpandConstant('{app}') + '\*" /s /d', ExpandConstant('{app}'),
      SW_HIDE, ewWaitUntilTerminated, Code);
  end;
  if (CurUninstallStep = usPostUninstall) and DeleteUserData then
  begin
    DelTree(ExpandConstant('{userappdata}\Aurora\AuroraViewer'), True, True, True);
    DelTree(ExpandConstant('{localappdata}\Aurora\AuroraViewer'), True, True, True);
  end;
end;
