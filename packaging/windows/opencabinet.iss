; SPDX-FileCopyrightText: 2026 OpenCabinet contributors
; SPDX-License-Identifier: GPL-3.0-or-later
;
; Inno Setup script for the OpenCabinet Designer installer.
; Build: ISCC /DAppVersion=0.1.0-beta.1 /DNumericVersion=0.1.0.1 opencabinet.iss
; The executable must already be built: cargo build --release -p opencabinet-app

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#ifndef NumericVersion
  #define NumericVersion "0.0.0.0"
#endif

[Setup]
; Never change AppId: Windows uses it to recognize upgrades and uninstall.
AppId={{50C223FB-94D3-4EEB-A008-BD1C1249D866}
AppName=OpenCabinet Designer
AppVersion={#AppVersion}
AppVerName=OpenCabinet Designer {#AppVersion}
AppPublisher=OpenCabinet contributors
AppPublisherURL=https://github.com/OpenCabinet/opencabinet
AppSupportURL=https://github.com/OpenCabinet/opencabinet/blob/main/SUPPORT.md
VersionInfoVersion={#NumericVersion}
LicenseFile=..\..\LICENSE
; Installs for the current user without administrator rights, which many
; shop computers do not give; the user may still choose all users.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
DefaultDirName={autopf}\OpenCabinet
DefaultGroupName=OpenCabinet
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\opencabinet.exe
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 or later (ADR-0014).
MinVersion=10.0
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
OutputDir=..\..\target\installer
OutputBaseFilename=OpenCabinet-Designer-{#AppVersion}-windows-x64

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\target\release\opencabinet.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
; The bundled font, as separate files with its license (ADR-0015).
Source: "..\..\assets\fonts\noto-sans\*.ttf"; DestDir: "{app}\fonts"; Flags: ignoreversion
Source: "..\..\assets\fonts\noto-sans\OFL.txt"; DestDir: "{app}\fonts"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\OpenCabinet Designer"; Filename: "{app}\opencabinet.exe"
Name: "{autodesktop}\OpenCabinet Designer"; Filename: "{app}\opencabinet.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\opencabinet.exe"; Description: "{cm:LaunchProgram,OpenCabinet Designer}"; Flags: nowait postinstall skipifsilent
