; Windows installer of FireDragon Aurora, a personal build of Aurora Viewer.
;
; The installer holds no viewer program: it installs the update engine
; (engine\Update.ps1), which installs the build tools (with the user's
; consent), downloads the public fork, builds the viewer on this PC and
; installs it. "Update FireDragon Aurora" in the Start menu runs the same
; engine later. See docs/INSTALLER.md.
;
; Compiled by .github/workflows/installer.yml, or by hand with Inno Setup 6:
;   iscc installer\AuroraFD.iss    (output: installer\output\)

; Strings are taken as written: no backslash escapes (paths)
#pragma parseroption -p+

#define AppName "FireDragon Aurora"
#define AppFullName "FireDragon Aurora (based on Aurora Viewer)"
; The same folder name as engine\settings.json (folderName)
#define AppFolder "FireDragonAurora"
#define AppMutexName "Local\FireDragonAurora-update"
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
AppName={#AppFullName}
AppVersion={#AppVersion}
AppVerName={#AppFullName}
AppPublisherURL={#RepoUrl}
AppSupportURL={#RepoUrl}
AppComments=Personal build of Aurora Viewer, built on this PC from source. Not an official Aurora Viewer release.
; No administrator rights for the viewer itself: %LOCALAPPDATA%\Programs
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppFolder}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=no
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
UninstallDisplayName={#AppFullName}
UninstallDisplayIcon={app}\viewer\aurora-viewer.exe
SetupLogging=yes
CloseApplications=no

[Messages]
WelcomeLabel2=This will install [name] on your computer: a personal build of Aurora Viewer, not an official release.%n%nIt is built on this PC from its public source code. Setup installs the free tools the build needs (it asks you first), downloads the code, builds the viewer (20-60 minutes the first time, about 20 GB of free disk space) and installs it.%n%nLater, "Update {#AppName}" in the Start menu gets the latest version the same way.
FinishedLabel=Setup has finished.%n%nStart [name] from the Start menu or the desktop. If the build window reported a problem, fix what it said, then use "Update {#AppName}" in the Start menu to try again.

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
; What the engine made in the install folder. Never the user's settings,
; chat logs or cache (they are outside it; see [Code]).
Type: filesandordirs; Name: "{app}\viewer"
Type: filesandordirs; Name: "{app}\viewer.previous"
Type: filesandordirs; Name: "{app}\viewer.new"
Type: filesandordirs; Name: "{app}\viewer.swap"
Type: filesandordirs; Name: "{app}\build"
Type: filesandordirs; Name: "{app}\tools"
Type: filesandordirs; Name: "{app}\logs"
Type: files; Name: "{app}\state.json"
Type: dirifempty; Name: "{app}"

[Code]
var
  DeleteUserData: Boolean;

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
      'Also delete your viewer settings, saved logins, chat logs and cache?' + #13#10 + #13#10 +
      'They are shared with Aurora Viewer: if you also use the official Aurora Viewer on this PC, it loses them too. ' +
      'Choose No to keep them (recommended).', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Code: Integer;
  Viewer: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    Viewer := ExpandConstant('{app}\viewer\aurora-viewer.exe');
    if DeleteUserData and FileExists(Viewer) then
      { the link handler, notification identity and saved logins it registered }
      Exec(Viewer, '--uninstall', ExpandConstant('{app}\viewer'), SW_HIDE, ewWaitUntilTerminated, Code);
    { Git marks its object files read-only: clear that so they can be deleted }
    Exec(ExpandConstant('{cmd}'), '/d /c attrib -r "' + ExpandConstant('{app}') + '\*" /s /d', ExpandConstant('{app}'),
      SW_HIDE, ewWaitUntilTerminated, Code);
  end;
  if (CurUninstallStep = usPostUninstall) and DeleteUserData then
  begin
    DelTree(ExpandConstant('{userappdata}\Aurora\AuroraViewer'), True, True, True);
    DelTree(ExpandConstant('{localappdata}\Aurora\AuroraViewer'), True, True, True);
    DelTree(ExpandConstant('{userappdata}\{#AppFolder}'), True, True, True);
  end;
end;
