; AI ToolPlus NSIS Installer Script
; Modern UI 2 with Chinese and English support

!include "MUI2.nsh"
!include "FileFunc.nsh"

Name "AI ToolPlus"
OutFile "..\target\dist\aitoolplus-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\AIToolPlus"
InstallDirRegKey HKCU "Software\AIToolPlus" "InstallDir"
RequestExecutionLevel user

SetCompressor /SOLID lzma

; Interface Settings
!define MUI_ABORTWARNING
!define MUI_ICON "..\crates\aitoolplus\assets\app.ico"
!define MUI_UNICON "..\crates\aitoolplus\assets\app.ico"

; Language selection
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\aitoolplus.exe"
!define MUI_FINISHPAGE_RUN_TEXT "启动 AI ToolPlus / Launch AI ToolPlus"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Section "MainSection" SecMain
    SetOutPath "$INSTDIR"
    File "..\target\release\aitoolplus.exe"
    File "..\crates\aitoolplus\assets\app.ico"

    ; Store installation folder
    WriteRegStr HKCU "Software\AIToolPlus" "InstallDir" "$INSTDIR"

    ; Register URL Protocol aitoolbox://
    WriteRegStr HKCU "Software\Classes\aitoolbox" "" "URL:AI ToolPlus Protocol"
    WriteRegStr HKCU "Software\Classes\aitoolbox" "URL Protocol" ""
    WriteRegStr HKCU "Software\Classes\aitoolbox\DefaultIcon" "" "$INSTDIR\aitoolplus.exe,0"
    WriteRegStr HKCU "Software\Classes\aitoolbox\shell\open\command" "" '"$INSTDIR\aitoolplus.exe" "%1"'

    ; Register in Add/Remove Programs
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "DisplayName" "AI ToolPlus"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "DisplayIcon" "$INSTDIR\aitoolplus.exe"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "DisplayVersion" "0.1.0"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "Publisher" "aitoolplus contributors"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "NoRepair" 1

    ; Create Shortcuts
    CreateDirectory "$SMPROGRAMS\AI ToolPlus"
    CreateShortcut "$SMPROGRAMS\AI ToolPlus\AI ToolPlus.lnk" "$INSTDIR\aitoolplus.exe" "" "$INSTDIR\app.ico"
    CreateShortcut "$SMPROGRAMS\AI ToolPlus\卸载 AI ToolPlus.lnk" "$INSTDIR\uninstall.exe"
    CreateShortcut "$DESKTOP\AI ToolPlus.lnk" "$INSTDIR\aitoolplus.exe" "" "$INSTDIR\app.ico"

    ; Create Uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
    ; Terminate running instance
    nsExec::Exec 'taskkill /F /IM aitoolplus.exe'

    Delete "$INSTDIR\aitoolplus.exe"
    Delete "$INSTDIR\app.ico"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    ; Delete Shortcuts
    Delete "$SMPROGRAMS\AI ToolPlus\AI ToolPlus.lnk"
    Delete "$SMPROGRAMS\AI ToolPlus\卸载 AI ToolPlus.lnk"
    RMDir "$SMPROGRAMS\AI ToolPlus"
    Delete "$DESKTOP\AI ToolPlus.lnk"

    ; Delete Registry Keys
    DeleteRegKey HKCU "Software\Classes\aitoolbox"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus"
    DeleteRegKey HKCU "Software\AIToolPlus"

    ; Ask whether to remove user data
    MessageBox MB_YESNO|MB_ICONQUESTION "是否删除所有配置文件与用户数据 (%APPDATA%\aitoolplus)?$\n$\nDo you want to delete all user data (%APPDATA%\aitoolplus)?" IDNO skip_appdata
        RMDir /r "$APPDATA\aitoolplus"
    skip_appdata:
SectionEnd
