; SPDX-FileCopyrightText: 2026 OpenCabinet contributors
; SPDX-License-Identifier: GPL-3.0-or-later
;
; Inno Setup script for the OpenCabinet Review installer, apart from the
; designer's (RFC-0004 §1).
; Build: ISCC /DAppVersion=0.1.0-alpha.1 /DNumericVersion=0.1.0.1 opencabinet-review.iss
; The executable must already be built: cargo build --release -p opencabinet-review-app

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#ifndef NumericVersion
  #define NumericVersion "0.0.0.0"
#endif

[Setup]
; Never change AppId: Windows uses it to recognize upgrades and uninstall.
AppId={{A7CD2D19-451B-493E-8975-FFC9F8640E97}
AppName=OpenCabinet Review
AppVersion={#AppVersion}
AppVerName=OpenCabinet Review {#AppVersion}
AppPublisher=OpenCabinet contributors
AppPublisherURL=https://github.com/OpenCabinet/opencabinet
AppSupportURL=https://github.com/OpenCabinet/opencabinet/blob/main/SUPPORT.md
VersionInfoVersion={#NumericVersion}
LicenseFile=..\..\LICENSE
; Installs for the current user without administrator rights, which many
; shop computers do not give; the user may still choose all users.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
DefaultDirName={autopf}\OpenCabinet Review
DefaultGroupName=OpenCabinet
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\opencabinet-review.exe
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 or later (ADR-0014).
MinVersion=10.0
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
OutputDir=..\..\target\installer
OutputBaseFilename=OpenCabinet-Review-{#AppVersion}-windows-x64

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
; Installed for everyone (as an administrator), the installer can let
; review sessions through the Windows firewall, on private and work
; networks, for this program only. Installed for one user, Review offers
; it in its Session panel instead.
Name: "firewall"; Description: "Allow review sessions through the Windows firewall (private networks)"; Check: IsAdminInstallMode

[Files]
Source: "..\..\target\release\opencabinet-review.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Registry]
; Offers Review under "Open with" for PDF files, without taking over the
; program that opens them now.
Root: HKA; Subkey: "Software\Classes\Applications\opencabinet-review.exe\shell\open\command"; ValueType: string; ValueData: """{app}\opencabinet-review.exe"" ""%1"""; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\opencabinet-review.exe\SupportedTypes"; ValueType: string; ValueName: ".pdf"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.pdf\OpenWithList\opencabinet-review.exe"; ValueType: none; Flags: uninsdeletekey

[Icons]
Name: "{autoprograms}\OpenCabinet Review"; Filename: "{app}\opencabinet-review.exe"
Name: "{autodesktop}\OpenCabinet Review"; Filename: "{app}\opencabinet-review.exe"; Tasks: desktopicon

[Run]
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""OpenCabinet Review sessions"" dir=in action=allow program=""{app}\opencabinet-review.exe"" enable=yes profile=private,domain"; Flags: runhidden; Tasks: firewall
Filename: "{app}\opencabinet-review.exe"; Description: "{cm:LaunchProgram,OpenCabinet Review}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""OpenCabinet Review sessions"" program=""{app}\opencabinet-review.exe"""; Flags: runhidden; Check: IsAdminInstallMode; RunOnceId: "RemoveFirewallRule"
