; =====================================================================
; Inno Setup 脚本: AI ToolPlus 桌面客户端安装与卸载程序
; 参考规范: SimpleLive_Setup.iss
; =====================================================================

#ifndef MyAppName
#define MyAppName "AI ToolPlus"
#endif

#ifndef MyAppFullName
#define MyAppFullName "AI ToolPlus"
#endif

#ifndef MyAppVersion
#define MyAppVersion "0.1.16"
#endif

#ifndef MyAppPublisher
#define MyAppPublisher "aitoolplus contributors"
#endif

#ifndef MyAppURL
#define MyAppURL "https://github.com/821869798/aitoolplus"
#endif

#ifndef MyAppExeName
#define MyAppExeName "aitoolplus.exe"
#endif

#ifndef MyAppIcoName
#define MyAppIcoName "app.ico"
#endif

#ifndef MyOutputFilename
#define MyOutputFilename "aitoolplus-setup"
#endif

[Setup]
AppId={{7E61FA49-8F8D-4F70-983D-4FE7A6B296E3}
AppName={#MyAppFullName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
UninstallDisplayIcon={app}\{#MyAppIcoName}
; 允许用户在安装时选择“仅为当前用户”或“为所有用户”
PrivilegesRequiredOverridesAllowed=dialog
PrivilegesRequired=lowest
OutputDir=..\target\dist
OutputBaseFilename={#MyOutputFilename}
SetupIconFile=..\crates\aitoolplus\assets\{#MyAppIcoName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "chinesesimp"; MessagesFile: "ChineseSimplified.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\crates\aitoolplus\assets\{#MyAppIcoName}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppFullName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppIcoName}"
Name: "{autodesktop}\{#MyAppFullName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppIcoName}"; Tasks: desktopicon

[Registry]
; 1. 注册 Applications 系统应用信息
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExeName}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#MyAppFullName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExeName}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppIcoName},0"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#MyAppExeName}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey

; 2. 注册系统别名（支持 Win+R 输入 aitoolplus 直接启动）
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#MyAppExeName}"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\{#MyAppExeName}"; ValueType: string; ValueName: "Path"; ValueData: "{app}"; Flags: uninsdeletekey

; 3. 注册 URL Protocol: aitoolplus://
Root: HKA; Subkey: "Software\Classes\aitoolplus"; ValueType: string; ValueName: ""; ValueData: "URL:AI ToolPlus Protocol"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\aitoolplus"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\aitoolplus\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppIcoName},0"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\aitoolplus\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey

; 4. 注册 URL Protocol: aitoolbox:// (兼容旧版协议)
Root: HKA; Subkey: "Software\Classes\aitoolbox"; ValueType: string; ValueName: ""; ValueData: "URL:AI ToolPlus Protocol"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\aitoolbox"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\aitoolbox\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppIcoName},0"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\aitoolbox\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Flags: uninsdeletekey

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppFullName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
// 检测并清理旧版 NSIS
function InitializeSetup(): Boolean;
var
  UninstPath: String;
  UninstDir: String;
  ResultCode: Integer;
begin
  Result := True;

  // 检测旧版 NSIS 注册表卸载项
  if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus', 'UninstallString', UninstPath) or
     RegQueryStringValue(HKLM, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus', 'UninstallString', UninstPath) then
  begin
    UninstPath := RemoveQuotes(UninstPath);
    if FileExists(UninstPath) then
    begin
      if MsgBox(
        '检测到系统中已安装旧版 AI ToolPlus。' + #13#10 + #13#10 +
        '为保证正常升级，需要先卸载旧版本。' + #13#10 +
        '（注意：在卸载向导中询问“是否删除配置文件与数据”时，请选择“否”，即可保留您的配置）' + #13#10 + #13#10 +
        '是否现在启动旧版卸载程序？',
        mbConfirmation, MB_YESNO) = IDYES then
      begin
        UninstDir := ExtractFilePath(UninstPath);
        // 调用旧版 NSIS 静默卸载（/S 且 _?= 原地阻塞等待），不弹窗打扰用户并保留配置
        Exec(UninstPath, '/S _?=' + UninstDir, UninstDir, SW_HIDE, ewWaitUntilTerminated, ResultCode);

        // 检查旧版是否真正卸载
        if RegKeyExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus') or
           RegKeyExists(HKLM, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus') then
        begin
          MsgBox('旧版本尚未卸载完成，无法继续安装。安装程序即将退出。', mbError, MB_OK);
          Result := False;
          Exit;
        end;
      end
      else
      begin
        Result := False;
        Exit;
      end;
    end;
  end;
end;
