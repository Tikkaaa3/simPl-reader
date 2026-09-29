; Inno Setup 6.7.3. Build with scripts/installer.ps1.
#if Ver != 0x06070300
  #error Build this installer with the pinned Inno Setup 6.7.3 compiler
#endif
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef PayloadDir
  #error PayloadDir must name the complete packaged reader directory
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif
#ifdef TestProfileRoot
  #define ProductName "simPl Reader Installer QA"
  #define ProductId "simPl.Reader.InstallerQA"
  #define SetupName "simPl-installer-qa"
  #define InstallFolder "simPl Installer QA"
  #define AssociationBase "simPl.Reader.InstallerQA"
  #define CapabilityKey "Software\simPl\InstallerQA\Capabilities"
#else
  #define ProductName "simPl Reader"
  #define ProductId "{{1C17657E-C087-4F08-8F1B-B58D6910D604}"
  #define SetupName "simPl-" + AppVersion + "-windows-x64-setup"
  #define InstallFolder "simPl"
  #define AssociationBase "simPl.Reader"
  #define CapabilityKey "Software\simPl\Reader\Capabilities"
#endif

[Setup]
AppId={#ProductId}
AppName={#ProductName}
AppVersion={#AppVersion}
AppPublisher=simPl
AppPublisherURL=https://github.com/Tikkaaa3/simPl-reader
AppSupportURL=https://github.com/Tikkaaa3/simPl-reader/issues
DefaultDirName={localappdata}\Programs\{#InstallFolder}
DefaultGroupName={#ProductName}
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
WizardStyle=modern dynamic
DisableWelcomePage=no
DisableDirPage=no
DisableProgramGroupPage=yes
AllowNoIcons=yes
UsePreviousTasks=yes
UninstallDisplayName={#ProductName}
UninstallDisplayIcon={app}\simPl.ico
SetupIconFile=simPl.ico
LicenseFile={#PayloadDir}\LICENSE.txt
AppMutex=Local\simPl.Reader.Running
SetupMutex=Local\simPl.Reader.Setup
CloseApplications=yes
RestartApplications=no
ChangesAssociations=yes
OutputDir={#OutputDir}
OutputBaseFilename={#SetupName}
Compression=lzma2
SolidCompression=yes
VersionInfoVersion={#AppVersion}.0
VersionInfoDescription=simPl Reader Setup
#ifdef SignInstaller
SignTool=release
SignedUninstaller=yes
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopShortcut}"; GroupDescription: "{cm:ShortcutChoices}"; Flags: unchecked
Name: "startmenuicon"; Description: "{cm:StartMenuShortcut}"; GroupDescription: "{cm:ShortcutChoices}"

[Files]
Source: "{#PayloadDir}\simPl.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\pdfium.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\third-party\*"; DestDir: "{app}\third-party"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "simPl.ico"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
; Advertise handlers for this user; never write extension defaults or UserChoice.
Root: HKCU; Subkey: "{#CapabilityKey}"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#ProductName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "{#CapabilityKey}"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Read local PDF, HTML, EPUB, text and Markdown books with simPl."
Root: HKCU; Subkey: "{#CapabilityKey}"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#ProductName}"; ValueData: "{#CapabilityKey}"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.PDF"; ValueType: string; ValueName: ""; ValueData: "PDF document"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.PDF\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.PDF\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#ProductName}"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.PDF\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.PDF\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: string; ValueName: "{#AssociationBase}.PDF"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "{#CapabilityKey}\FileAssociations"; ValueType: string; ValueName: ".pdf"; ValueData: "{#AssociationBase}.PDF"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.HTML"; ValueType: string; ValueName: ""; ValueData: "HTML document"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.HTML\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.HTML\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#ProductName}"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.HTML\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.HTML\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\.html\OpenWithProgids"; ValueType: string; ValueName: "{#AssociationBase}.HTML"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "{#CapabilityKey}\FileAssociations"; ValueType: string; ValueName: ".html"; ValueData: "{#AssociationBase}.HTML"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.EPUB"; ValueType: string; ValueName: ""; ValueData: "EPUB book"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.EPUB\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.EPUB\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#ProductName}"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.EPUB\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.EPUB\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\.epub\OpenWithProgids"; ValueType: string; ValueName: "{#AssociationBase}.EPUB"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "{#CapabilityKey}\FileAssociations"; ValueType: string; ValueName: ".epub"; ValueData: "{#AssociationBase}.EPUB"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.TXT"; ValueType: string; ValueName: ""; ValueData: "Text document"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.TXT\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.TXT\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#ProductName}"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.TXT\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.TXT\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\.txt\OpenWithProgids"; ValueType: string; ValueName: "{#AssociationBase}.TXT"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "{#CapabilityKey}\FileAssociations"; ValueType: string; ValueName: ".txt"; ValueData: "{#AssociationBase}.TXT"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.MD"; ValueType: string; ValueName: ""; ValueData: "Markdown document"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.MD\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.MD\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#ProductName}"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.MD\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\simPl.ico"",0"
Root: HKCU; Subkey: "Software\Classes\{#AssociationBase}.MD\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\simPl.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "{#AssociationBase}.MD"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "{#CapabilityKey}\FileAssociations"; ValueType: string; ValueName: ".md"; ValueData: "{#AssociationBase}.MD"

[Icons]
Name: "{userdesktop}\{#ProductName}"; Filename: "{app}\simPl.exe"; WorkingDir: "{app}"; IconFilename: "{app}\simPl.ico"; Tasks: desktopicon
Name: "{userprograms}\{#ProductName}\{#ProductName}"; Filename: "{app}\simPl.exe"; WorkingDir: "{app}"; IconFilename: "{app}\simPl.ico"; Tasks: startmenuicon
Name: "{userprograms}\{#ProductName}\{cm:UninstallShortcut}"; Filename: "{uninstallexe}"; Tasks: startmenuicon

[Run]
Filename: "{app}\simPl.exe"; Description: "{cm:LaunchReader}"; Flags: nowait postinstall skipifsilent

[CustomMessages]
english.DesktopShortcut=Create a &desktop shortcut
english.StartMenuShortcut=Create &Start menu shortcuts
english.ShortcutChoices=Shortcuts:
english.UninstallShortcut=Uninstall simPl Reader
english.LaunchReader=Open simPl Reader
english.LibraryQuestion=What should happen to your library?
english.LibraryDetails=You can keep your books for a future reinstall, or permanently delete simPl's imported copies, favourites, reading progress, settings and caches.%n%nYour original files outside simPl's library will not be deleted.%n%nLibrary folder: %1
english.KeepLibrary=Uninstall and keep my library
english.DeleteLibrary=Uninstall and delete my library
english.CancelUninstall=Cancel
english.ProfileDeleteFailed=simPl was uninstalled, but some library files could not be removed. Close programs using this folder and remove it manually if you still want to delete your library:%n%n%1
english.ProfileLocationError=Choose a different installation folder. The application must be installed separately from its library folder.

[Code]
var
  RemoveProfile: Boolean;

function ProfilePath: String;
begin
#ifdef TestProfileRoot
  Result := ExpandFileName('{#TestProfileRoot}');
#else
  Result := ExpandFileName(ExpandConstant('{localappdata}\simPl'));
#endif
end;

function IsWithin(const Child, Parent: String): Boolean;
begin
  Result := (CompareText(Child, Parent) = 0) or
    (CompareText(Copy(AddBackslash(Child), 1, Length(AddBackslash(Parent))),
      AddBackslash(Parent)) = 0);
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if CurPageID = wpSelectDir then begin
    Result := not IsWithin(ExpandFileName(WizardDirValue), ProfilePath) and
      not IsWithin(ProfilePath, ExpandFileName(WizardDirValue));
    if not Result then
      SuppressibleMsgBox(CustomMessage('ProfileLocationError'), mbError, MB_OK, IDOK);
  end;
end;

function HasArgument(const Value: String): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), Value) = 0 then begin
      Result := True;
      Exit;
    end;
end;

function InitializeUninstall: Boolean;
var
  Choice: Integer;
begin
  Result := True;
  RemoveProfile := False;
  if not DirExists(ProfilePath) then Exit;
  if UninstallSilent then begin
    // Silent uninstall preserves user data unless explicitly opted in.
    RemoveProfile := HasArgument('/PURGEUSERDATA');
    Exit;
  end;
  Choice := SuppressibleTaskDialogMsgBox(CustomMessage('LibraryQuestion'),
    FmtMessage(CustomMessage('LibraryDetails'), [ProfilePath]), mbConfirmation,
    MB_YESNOCANCEL, [CustomMessage('KeepLibrary'), CustomMessage('DeleteLibrary'),
      CustomMessage('CancelUninstall')], 0, IDYES);
  Result := (Choice = IDYES) or (Choice = IDNO);
  RemoveProfile := Choice = IDNO;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Profile: String;
begin
  if (CurUninstallStep = usPostUninstall) and RemoveProfile then begin
    Profile := ProfilePath;
    // Fixed profile root only. Never derive deletion targets from library entries
    // or {app}. Inno DelTree removes junctions without traversing their targets.
    if IsWithin(Profile, ExpandFileName(ExpandConstant('{app}'))) or
      IsWithin(ExpandFileName(ExpandConstant('{app}')), Profile) then begin
      Log('Refusing to delete overlapping application and library directories.');
      Exit;
    end;
    if DirExists(Profile) and not DelTree(Profile, True, True, True) then begin
      Log('Some library files could not be removed: ' + Profile);
      SuppressibleMsgBox(FmtMessage(CustomMessage('ProfileDeleteFailed'), [Profile]),
        mbError, MB_OK, IDOK);
    end;
  end;
end;
