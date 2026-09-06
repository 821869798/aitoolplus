# ==============================================================================
# AI ToolPlus - Portable Build Script (PowerShell)
# ==============================================================================
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [string]$DistDir = ""
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = (Resolve-Path "$ScriptDir\..").Path

if ([string]::IsNullOrWhiteSpace($DistDir)) {
    $DistDir = Join-Path $ProjectRoot "target\dist"
}

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " AI ToolPlus - Building Portable Package                  " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "Project root: $ProjectRoot" -ForegroundColor Gray
Write-Host "Output dir:   $DistDir" -ForegroundColor Gray

# 1. Terminate any running instances to avoid binary file locks
Write-Host "`n[1/5] Checking for running instances..." -ForegroundColor Yellow
$running = Get-Process aitoolplus -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "Stopping running aitoolplus processes..." -ForegroundColor Yellow
    $running | Stop-Process -Force
    Start-Sleep -Milliseconds 600
}

# 2. Build release binary if not skipped
if (-not $SkipBuild) {
    Write-Host "`n[2/5] Compiling release binary (cargo build --release)..." -ForegroundColor Yellow
    Push-Location $ProjectRoot
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "`n[2/5] Skipping cargo build (-SkipBuild specified)..." -ForegroundColor Yellow
}

$ExePath = Join-Path $ProjectRoot "target\release\aitoolplus.exe"
if (-not (Test-Path $ExePath)) {
    throw "Release binary not found at: $ExePath"
}

# 3. Read version from Cargo.toml
$CargoToml = Get-Content (Join-Path $ProjectRoot "Cargo.toml") -Raw
$Version = "0.1.0"
if ($CargoToml -match 'version\s*=\s*"([^"]+)"') {
    $Version = $matches[1]
}
Write-Host "Version: $Version" -ForegroundColor Green

# 4. Prepare portable folder structure
Write-Host "`n[3/5] Assembling portable directory structure..." -ForegroundColor Yellow
$PortableDirName = "aitoolplus-v$Version-windows-x64-portable"
$PortableDir = Join-Path $DistDir $PortableDirName

if (Test-Path $PortableDir) {
    Remove-Item -Path $PortableDir -Recurse -Force
}
New-Item -ItemType Directory -Path $PortableDir -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $PortableDir "data") -Force | Out-Null

# Copy release executable
Copy-Item -Path $ExePath -Destination (Join-Path $PortableDir "aitoolplus.exe") -Force

# Create .portable marker file (triggers auto portable-mode in aitoolplus-core)
Set-Content -Path (Join-Path $PortableDir ".portable") -Value "" -NoNewline

# Create Launcher batch file (dual protection: sets AITOOLPLUS_APPDATA to local data/)
$LauncherBat = @"
@echo off
setlocal
cd /d "%~dp0"
title AI ToolPlus (Portable)
echo Starting AI ToolPlus Portable...
set "AITOOLPLUS_APPDATA=%~dp0data"
start "" "%~dp0aitoolplus.exe" %*
exit /b 0
"@
$LauncherPath = Join-Path $PortableDir "run-portable.bat"
[System.IO.File]::WriteAllText($LauncherPath, $LauncherBat, [System.Text.Encoding]::ASCII)
Copy-Item -Path $LauncherPath -Destination (Join-Path $PortableDir "aitoolplus-portable.bat") -Force

# Create Readme
$ReadmeContent = @"
===============================================================================
               AI ToolPlus 绿色便携版 (Portable Edition)
===============================================================================

【说明】
本版本为免安装绿色便携版，所有配置与数据均完全隔离保存在本目录下的 data\ 文件夹中：
1. 不会向系统 %APPDATA% 写入任何文件。
2. 不会向 Windows 注册表写入自启项（除非您在设置里主动开启）。
3. 支持放入 U 盘或任意移动存储设备即插即用。

【启动方式】
- 推荐方式：双击运行 "run-portable.bat" (或 "aitoolplus-portable.bat")
- 直接运行：也可以直接双击 "aitoolplus.exe"（程序检测到 .portable 标记会自动开启便携模式）

【目录结构】
├── aitoolplus.exe           主程序
├── run-portable.bat         便携启动脚本（确保数据完全隔离到 data/）
├── aitoolplus-portable.bat  便携启动脚本别名
├── .portable                便携版标记文件
├── data/                    数据存储目录（包含配置、会话缓存、备份等）
└── README.txt               本说明文件

版本: v$Version
构建时间: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")
===============================================================================
"@
$ReadmePath = Join-Path $PortableDir "README.txt"
Set-Content -Path $ReadmePath -Value $ReadmeContent -Encoding UTF8

# 5. Create ZIP Archive
Write-Host "`n[4/5] Creating ZIP archive..." -ForegroundColor Yellow
$ZipPath = Join-Path $DistDir "$PortableDirName.zip"
if (Test-Path $ZipPath) {
    Remove-Item -Path $ZipPath -Force
}

Compress-Archive -Path "$PortableDir\*" -DestinationPath $ZipPath -CompressionLevel Optimal

# 6. Verify and output summary
Write-Host "`n[5/5] Generating checksums and summary..." -ForegroundColor Yellow
$ExeItem = Get-Item (Join-Path $PortableDir "aitoolplus.exe")
$ZipItem = Get-Item $ZipPath

$sha256 = [System.Security.Cryptography.SHA256]::Create()
$stream = [System.IO.File]::OpenRead($ZipPath)
$hashBytes = $sha256.ComputeHash($stream)
$stream.Close()
$sha256.Dispose()
$ZipHash = [System.BitConverter]::ToString($hashBytes).Replace("-", "").ToUpper()

Write-Host "`n==========================================================" -ForegroundColor Green
Write-Host " Portable Build Succeeded!                                " -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green
Write-Host "Portable Directory: $PortableDir" -ForegroundColor White
Write-Host "Portable ZIP:       $ZipPath" -ForegroundColor White
Write-Host "ZIP Size:           $([math]::Round($ZipItem.Length / 1MB, 2)) MB ($($ZipItem.Length) bytes)" -ForegroundColor White
Write-Host "EXE Size:           $([math]::Round($ExeItem.Length / 1MB, 2)) MB ($($ExeItem.Length) bytes)" -ForegroundColor White
Write-Host "SHA256 (ZIP):       $ZipHash" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Green
