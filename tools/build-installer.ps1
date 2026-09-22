# AI ToolPlus Build & Package Script
# Builds release binary and creates NSIS Setup installer.

param (
    [switch]$SkipBuild = $false
)

$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
Set-Location $RootDir

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "   AI ToolPlus Windows Packaging Tool   " -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan

# 1. Build release binary if not skipped
if (-not $SkipBuild) {
    Write-Host "`n[1/2] Building release binary with cargo..." -ForegroundColor Yellow
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Cargo build failed!"
        exit $LASTEXITCODE
    }
} else {
    Write-Host "`n[1/2] Skipping cargo build as requested..." -ForegroundColor Gray
}

$ExePath = Join-Path $RootDir "target\release\aitoolplus.exe"
if (-not (Test-Path $ExePath)) {
    Write-Error "Binary not found at $ExePath"
    exit 1
}

$DistDir = Join-Path $RootDir "target\dist"
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir | Out-Null
}

# 2. Build NSIS Installer
Write-Host "`n[2/2] Creating NSIS Setup Installer..." -ForegroundColor Yellow
$Makensis = Get-Command makensis.exe -ErrorAction SilentlyContinue
if (-not $Makensis) {
    # Check scoop shim or common locations
    $ScoopMakensis = "$env:USERPROFILE\scoop\shims\makensis.exe"
    if (Test-Path $ScoopMakensis) {
        $Makensis = $ScoopMakensis
    } else {
        $ProgramFilesMakensis = "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
        if (Test-Path $ProgramFilesMakensis) {
            $Makensis = $ProgramFilesMakensis
        }
    }
}

if ($Makensis) {
    $NsiFile = Join-Path $RootDir "tools\installer.nsi"
    & $Makensis /INPUTCHARSET UTF8 $NsiFile
    if ($LASTEXITCODE -eq 0) {
        $SetupExe = Join-Path $DistDir "aitoolplus-setup.exe"
        $ExeSizeMB = [math]::Round((Get-Item $SetupExe).Length / 1MB, 2)
        Write-Host "NSIS Setup Installer created: $SetupExe ($ExeSizeMB MB)" -ForegroundColor Green
    } else {
        Write-Warning "makensis failed with exit code $LASTEXITCODE"
    }
} else {
    Write-Warning "makensis.exe not found. Please install NSIS (e.g. via 'scoop install nsis' or from nsis.sourceforge.io)."
}

Write-Host "`nAll packaging tasks completed successfully!" -ForegroundColor Cyan
