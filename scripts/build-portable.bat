@echo off
setlocal
chcp 65001 >nul
title AI ToolPlus - Portable Build

set "SCRIPT_DIR=%~dp0"
set "PS_SCRIPT=%SCRIPT_DIR%build-portable.ps1"

if not exist "%PS_SCRIPT%" (
    echo [ERROR] Cannot find PowerShell script: %PS_SCRIPT%
    pause
    exit /b 1
)

echo Starting portable build...
powershell -NoProfile -ExecutionPolicy Bypass -File "%PS_SCRIPT%" %*
set "EXIT_CODE=%ERRORLEVEL%"

if %EXIT_CODE% neq 0 (
    echo.
    echo [ERROR] Build failed with exit code %EXIT_CODE%.
) else (
    echo.
    echo [SUCCESS] Build finished successfully!
)

:: If double-clicked in Windows Explorer, keep the console window open
if not "%1"=="-NoPause" if not defined CI (
    echo %CMDCMDLINE% | findstr /i /c:"cmd.exe" >nul
    if %ERRORLEVEL% equ 0 (
        echo %CMDCMDLINE% | findstr /i /c:"/c" >nul
        if %ERRORLEVEL% neq 0 pause
    )
)

exit /b %EXIT_CODE%
