; AI ToolPlus NSIS Installer Script
; Modern UI 2 with Chinese and English support

Unicode true

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

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

; Installer Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\aitoolplus.exe"
!define MUI_FINISHPAGE_RUN_TEXT "$(MUI_RUN_TEXT)"
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
Var un.Checkbox_DeleteData
Var un.DeleteDataState

!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.UnConfirmShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE un.UnConfirmLeave
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages (first is default)
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

LangString MUI_RUN_TEXT ${LANG_SIMPCHINESE} "运行 AI ToolPlus"
LangString MUI_RUN_TEXT ${LANG_ENGLISH} "Launch AI ToolPlus"

LangString SHORTCUT_UNINSTALL ${LANG_SIMPCHINESE} "卸载 AI ToolPlus"
LangString SHORTCUT_UNINSTALL ${LANG_ENGLISH} "Uninstall AI ToolPlus"

LangString CHECKBOX_DELETE_DATA ${LANG_SIMPCHINESE} "同时删除所有配置文件与用户数据 (~/.aitoolplus)"
LangString CHECKBOX_DELETE_DATA ${LANG_ENGLISH} "Also delete all configuration and user data (~/.aitoolplus)"

Section "MainSection" SecMain
    ; Terminate running instance if any before overwriting
    nsExec::Exec 'taskkill /F /IM aitoolplus.exe'
    Sleep 300

    SetOutPath "$INSTDIR"
    File "..\target\release\aitoolplus.exe"
    File "..\crates\aitoolplus\assets\app.ico"

    ; Store installation folder
    WriteRegStr HKCU "Software\AIToolPlus" "InstallDir" "$INSTDIR"

    ; Register URL Protocol aitoolplus:// (primary)
    WriteRegStr HKCU "Software\Classes\aitoolplus" "" "URL:AI ToolPlus Protocol"
    WriteRegStr HKCU "Software\Classes\aitoolplus" "URL Protocol" ""
    WriteRegStr HKCU "Software\Classes\aitoolplus\DefaultIcon" "" "$INSTDIR\aitoolplus.exe,0"
    WriteRegStr HKCU "Software\Classes\aitoolplus\shell\open\command" "" '"$INSTDIR\aitoolplus.exe" "%1"'

    ; Register URL Protocol aitoolbox:// (legacy compatibility)
    WriteRegStr HKCU "Software\Classes\aitoolbox" "" "URL:AI ToolPlus Protocol"
    WriteRegStr HKCU "Software\Classes\aitoolbox" "URL Protocol" ""
    WriteRegStr HKCU "Software\Classes\aitoolbox\DefaultIcon" "" "$INSTDIR\aitoolplus.exe,0"
    WriteRegStr HKCU "Software\Classes\aitoolbox\shell\open\command" "" '"$INSTDIR\aitoolplus.exe" "%1"'

    ; Register in Add/Remove Programs
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "DisplayName" "AI ToolPlus"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "DisplayIcon" "$INSTDIR\aitoolplus.exe"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "DisplayVersion" "0.1.16"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "Publisher" "aitoolplus contributors"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus" "NoRepair" 1

    ; Create Shortcuts
    CreateDirectory "$SMPROGRAMS\AI ToolPlus"
    CreateShortcut "$SMPROGRAMS\AI ToolPlus\AI ToolPlus.lnk" "$INSTDIR\aitoolplus.exe" "" "$INSTDIR\app.ico"
    CreateShortcut "$SMPROGRAMS\AI ToolPlus\$(SHORTCUT_UNINSTALL).lnk" "$INSTDIR\uninstall.exe"
    CreateShortcut "$DESKTOP\AI ToolPlus.lnk" "$INSTDIR\aitoolplus.exe" "" "$INSTDIR\app.ico"

    ; Create Uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Function un.onInit
    ; Check command-line options for silent uninstallation
    ${GetParameters} $R0
    ${GetOptions} $R0 "/DELETEDATA" $R1
    ${IfNot} ${Errors}
        StrCpy $un.DeleteDataState "1"
    ${Else}
        StrCpy $un.DeleteDataState "0"
    ${EndIf}
FunctionEnd

Function un.UnConfirmShow
    ; Find the inner dialog page (#32770)
    FindWindow $0 "#32770" "" $HWNDPARENT

    ; Allocate RECT in dialog units: left=15, top=82, width=280, height=14
    System::Call '*(i 15, i 82, i 280, i 14) p .r2'
    System::Call 'user32::MapDialogRect(i $0, p $2)'
    System::Call '*$2(i .r3, i .r4, i .r5, i .r6)'
    System::Free $2

    ; Create Checkbox (WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX = 0x50010003)
    System::Call 'user32::CreateWindowEx(i 0, t "BUTTON", t "$(CHECKBOX_DELETE_DATA)", i 0x50010003, i r3, i r4, i r5, i r6, i $0, i 1234, i 0, i 0) i .r1'
    StrCpy $un.Checkbox_DeleteData $1

    ; Set font to match parent dialog
    SendMessage $HWNDPARENT 0x0031 0 0 $2
    SendMessage $un.Checkbox_DeleteData 0x0030 $2 1

    ; Default state: 0 (unchecked, preserving user data)
    ${If} $un.DeleteDataState == "1"
        SendMessage $un.Checkbox_DeleteData 0x00F1 1 0
    ${Else}
        SendMessage $un.Checkbox_DeleteData 0x00F1 0 0
    ${EndIf}
FunctionEnd

Function un.UnConfirmLeave
    SendMessage $un.Checkbox_DeleteData 0x00F0 0 0 $un.DeleteDataState
FunctionEnd

Section "Uninstall"
    ; Terminate running instance
    nsExec::Exec 'taskkill /F /IM aitoolplus.exe'
    Sleep 300

    Delete "$INSTDIR\aitoolplus.exe"
    Delete "$INSTDIR\app.ico"
    Delete "$INSTDIR\uninstall.exe"

    ; Clean up installation directory safely
    ${If} $INSTDIR != ""
    ${AndIf} ${FileExists} "$INSTDIR"
        RMDir /r "$INSTDIR"
    ${EndIf}

    ; Delete Shortcuts
    Delete "$SMPROGRAMS\AI ToolPlus\AI ToolPlus.lnk"
    Delete "$SMPROGRAMS\AI ToolPlus\$(SHORTCUT_UNINSTALL).lnk"
    RMDir "$SMPROGRAMS\AI ToolPlus"
    Delete "$DESKTOP\AI ToolPlus.lnk"

    ; Delete Registry Keys
    DeleteRegKey HKCU "Software\Classes\aitoolplus"
    DeleteRegKey HKCU "Software\Classes\aitoolbox"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AIToolPlus"
    DeleteRegKey HKCU "Software\AIToolPlus"

    ; Only remove user data if checkbox was checked (or /DELETEDATA specified)
    ${If} $un.DeleteDataState == "1"
        RMDir /r "$PROFILE\.aitoolplus"
        RMDir /r "$APPDATA\aitoolplus"
    ${EndIf}
SectionEnd
