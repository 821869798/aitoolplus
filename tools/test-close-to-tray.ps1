param([int]$TimeoutSeconds = 12)
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class CloseProbe {
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, UIntPtr w, IntPtr l);
}
'@
Get-Process aitoolplus -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = 'D:\program\rust\aitoolplus\target\release\aitoolplus.exe'
$psi.UseShellExecute = $false
$psi.EnvironmentVariables['AITOOLPLUS_HOME'] = 'C:\Users\zhuzi'
$psi.EnvironmentVariables['AITOOLPLUS_APPDATA'] = 'C:\temp\aitoolplus-close-test\appdata'
$process = [System.Diagnostics.Process]::Start($psi)
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
do {
  Start-Sleep -Milliseconds 250
  $process.Refresh()
} until ($process.MainWindowHandle -ne 0 -or (Get-Date) -gt $deadline)
if ($process.MainWindowHandle -eq 0) { throw 'window did not open' }
$hwnd = $process.MainWindowHandle
[CloseProbe]::PostMessage($hwnd, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Seconds 2
$alive = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
if (-not $alive) { throw 'process exited instead of remaining in tray' }
$alive.Refresh()
Write-Host "PASS close-to-tray: pid=$($alive.Id) responding=$($alive.Responding) hwnd=$($alive.MainWindowHandle)"
Stop-Process -Id $alive.Id -Force
