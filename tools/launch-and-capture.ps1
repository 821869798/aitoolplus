param(
  [string]$Page = "claude_code",
  [string]$Tab = "providers",
  [string]$Name = "screen",
  [int]$WaitSeconds = 7
)
Get-Process aitoolplus -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = 'D:\program\rust\aitoolplus\target\release\aitoolplus.exe'
$psi.UseShellExecute = $false
$psi.EnvironmentVariables['AITOOLPLUS_HOME'] = 'C:\Users\zhuzi'
$psi.EnvironmentVariables['AITOOLPLUS_APPDATA'] = 'C:\temp\aitoolplus-visual\appdata'
$psi.EnvironmentVariables['AITOOLPLUS_START_PAGE'] = $Page
$psi.EnvironmentVariables['AITOOLPLUS_START_TAB'] = $Tab
$launched = [System.Diagnostics.Process]::Start($psi)
Write-Host "launched pid=$($launched.Id) page=$Page tab=$Tab"
Start-Sleep -Seconds $WaitSeconds
$proc = Get-Process aitoolplus -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "No aitoolplus window" }
& 'D:\program\rust\aitoolplus\tools\capture-ui.ps1' -Name $Name
