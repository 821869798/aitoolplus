$exe = 'D:\program\rust\aitoolplus\target\release\aitoolplus.exe'
$appData = 'C:\temp\aitoolplus-deeplink-ipc\appdata'
Get-Process aitoolplus -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
Remove-Item -Recurse -Force 'C:\temp\aitoolplus-deeplink-ipc' -ErrorAction SilentlyContinue
[System.IO.Directory]::CreateDirectory($appData) | Out-Null
function New-Info([string]$argument) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $exe
  $psi.UseShellExecute = $false
  $psi.EnvironmentVariables['AITOOLPLUS_HOME'] = 'C:\Users\zhuzi'
  $psi.EnvironmentVariables['AITOOLPLUS_APPDATA'] = $appData
  if ($argument) { $psi.Arguments = '"' + $argument + '"' }
  return $psi
}
$first = [Diagnostics.Process]::Start((New-Info ''))
Start-Sleep -Seconds 7
$url = 'aitoolbox://v1/import?resource=provider&app=gemini&name=IpcRelay&category=custom&apiKey=ipc-secret&baseUrl=https%3A%2F%2Fipc.example.com&model=gemini-ipc'
$second = [Diagnostics.Process]::Start((New-Info $url))
if (-not $second.WaitForExit(5000)) { throw 'second instance did not exit' }
Start-Sleep -Seconds 3
$raw = [System.IO.File]::ReadAllText((Join-Path $appData 'store.json'), [System.Text.Encoding]::UTF8)
if ($raw -notmatch 'IpcRelay') { throw 'IPC deep-link provider was not imported' }
if ($raw -notmatch 'ipc-secret') { throw 'IPC provider key missing' }
$live = Get-Process -Id $first.Id -ErrorAction SilentlyContinue
if (-not $live) { throw 'first instance exited' }
Write-Host "PASS deep-link IPC: first=$($first.Id) secondExit=$($second.ExitCode) provider=IpcRelay"
Stop-Process -Id $first.Id -Force
