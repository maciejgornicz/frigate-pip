; FrigatePiP installer (Inno Setup 6).
; Build: cargo build --release, then
;   ISCC.exe /DAppVersion=1.0.0 installer\frigate-pip.iss
; The installer is written to target\installer.

#define AppName "FrigatePiP"
#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#define AppExe "frigate-pip.exe"
#define AppURL "https://github.com/maciejgornicz/frigate-pip"
#define RunKey "Software\Microsoft\Windows\CurrentVersion\Run"

[Setup]
AppId={{6B0F6C2E-5E7A-4E43-9C55-3A1F0B7D2C91}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=maciejgornicz
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
; Per-user installation without administrator rights: %LOCALAPPDATA%\Programs\FrigatePiP
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=auto
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
LicenseFile=..\LICENSE
OutputDir=..\target\installer
OutputBaseFilename=FrigatePiP-{#AppVersion}-installer
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; The running application is closed by the code below (it has no visible window).
CloseApplications=no
RestartApplications=no
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
VersionInfoVersion={#AppVersion}
VersionInfoDescription={#AppName} Setup
ShowLanguageDialog=auto
LanguageDetectionMethod=uilanguage

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "pl"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "pt"; MessagesFile: "compiler:Languages\Portuguese.isl"
Name: "ptbr"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"

[CustomMessages]
en.Autostart=Start FrigatePiP with Windows
pl.Autostart=Uruchamiaj FrigatePiP razem z Windows
de.Autostart=FrigatePiP mit Windows starten
es.Autostart=Iniciar FrigatePiP con Windows
pt.Autostart=Iniciar o FrigatePiP com o Windows
ptbr.Autostart=Iniciar o FrigatePiP com o Windows
fr.Autostart=Lancer FrigatePiP au démarrage de Windows
en.UserGuide=FrigatePiP user guide
pl.UserGuide=FrigatePiP – instrukcja obsługi
de.UserGuide=FrigatePiP – Benutzerhandbuch
es.UserGuide=FrigatePiP – guía del usuario
pt.UserGuide=FrigatePiP – guia do utilizador
ptbr.UserGuide=FrigatePiP – guia do usuário
fr.UserGuide=FrigatePiP – guide de l’utilisateur
en.DocLang=en
pl.DocLang=pl
de.DocLang=de
es.DocLang=es
pt.DocLang=pt
ptbr.DocLang=pt
fr.DocLang=fr

[Tasks]
Name: "autostart"; Description: "{cm:Autostart}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
; The shortcuts open the settings window (and start FrigatePiP if it is not running).
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; Parameters: "--settings"
Name: "{autoprograms}\{cm:UserGuide}"; Filename: "{#AppURL}/blob/main/docs/{cm:DocLang}/user-guide.md"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Parameters: "--settings"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "{#RunKey}"; ValueType: string; ValueName: "{#AppName}"; ValueData: """{app}\{#AppExe}"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
; A silent update starts the application again.
Filename: "{app}\{#AppExe}"; Flags: nowait; Check: WizardSilent

[Code]
const
  WM_APP_QUIT = $8004; { WM_APP + 4, see src/app.rs }
  MUTEX = 'Local\FrigatePiP.SingleInstance';

{ Asks a running FrigatePiP to exit and waits until it has. }
procedure QuitRunningApp();
var
  Wnd: HWND;
  I: Integer;
begin
  if not CheckForMutexes(MUTEX) then
    Exit;
  Wnd := FindWindowByClassName('FrigatePiP.Main');
  if Wnd <> 0 then
    PostMessage(Wnd, WM_APP_QUIT, 0, 0);
  for I := 1 to 50 do
  begin
    if not CheckForMutexes(MUTEX) then
      Break;
    Sleep(100);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  QuitRunningApp();
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    QuitRunningApp();
    { Autostart may also have been switched on in the application itself. }
    RegDeleteValue(HKEY_CURRENT_USER, '{#RunKey}', '{#AppName}');
  end;
end;
