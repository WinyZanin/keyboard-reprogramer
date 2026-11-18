; Inno Setup 6 script for Keyboard Reprogramer.
;
; Built by tools/package.ps1, which passes the version from Cargo.toml:
;   ISCC.exe /DAppVersion=X.Y.Z installer\inno\keyboard-reprogramer.iss
;
; Installs per user by default (no admin, %LOCALAPPDATA%\Programs); the user can choose
; "install for all users" in the first dialog, or pass /ALLUSERS on the command line.

#ifndef AppVersion
  #error AppVersion is not defined. Run tools/package.ps1 or pass /DAppVersion=X.Y.Z
#endif

#define AppName "Keyboard Reprogramer"
#define AppExe "keyboard-reprogramer.exe"
#define Root "..\.."

[Setup]
; Never change AppId: it identifies the app across versions (upgrades, uninstall).
AppId={{975F46DF-F4B1-414C-A087-065E2B3F3E58}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=WinyZanin
VersionInfoVersion={#AppVersion}
VersionInfoProductVersion={#AppVersion}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
; Must match INSTANCE_MUTEX in src/main.rs: setup and uninstall ask to close the running app.
AppMutex=KeyboardReprogramerMutex
CloseApplications=yes
SetupIconFile={#Root}\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputDir={#Root}\dist
OutputBaseFilename=keyboard-reprogramer-{#AppVersion}-x64-setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Root}\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
