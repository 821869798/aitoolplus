# AI ToolPlus Build & Package Script
# Builds release binary, creates NSIS Setup installer and Portable ZIP package.

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
    Write-Host "`n[1/3] Building release binary with cargo..." -ForegroundColor Yellow
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Cargo build failed!"
        exit $LASTEXITCODE
    }
} else {
    Write-Host "`n[1/3] Skipping cargo build as requested..." -ForegroundColor Gray
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

# 2. Package Portable ZIP
Write-Host "`n[2/3] Creating Portable ZIP package..." -ForegroundColor Yellow
$PortableStage = Join-Path $DistDir "aitoolplus-portable"
if (Test-Path $PortableStage) {
    Remove-Item -Recurse -Force $PortableStage
}
New-Item -ItemType Directory -Path $PortableStage | Out-Null
Copy-Item $ExePath -Destination $PortableStage
Copy-Item (Join-Path $RootDir "crates\aitoolplus\assets\app.ico") -Destination $PortableStage
# Create .portable marker file so app uses local data folder
New-Item -ItemType File -Path (Join-Path $PortableStage ".portable") | Out-Null

$PortableZip = Join-Path $DistDir "aitoolplus-v0.1.0-windows-x64-portable.zip"
if (Test-Path $PortableZip) {
    Remove-Item -Force $PortableZip
}
Compress-Archive -Path "$PortableStage\*" -DestinationPath $PortableZip -Force
Remove-Item -Recurse -Force $PortableStage
$ZipSizeMB = [math]::Round((Get-Item $PortableZip).Length / 1MB, 2)
Write-Host "Portable ZIP created: $PortableZip ($ZipSizeMB MB)" -ForegroundColor Green

# 3. Build NSIS Installer
Write-Host "`n[3/3] Creating NSIS Setup Installer..." -ForegroundColor Yellow
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
    & $Makensis $NsiFile
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
