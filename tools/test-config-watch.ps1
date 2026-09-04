$exe = 'D:\program\rust\aitoolplus\target\release\aitoolplus.exe'
$root = 'C:\temp\aitoolplus-watch-test'
Get-Process aitoolplus -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
[System.IO.Directory]::CreateDirectory((Join-Path $root 'home\.claude')) | Out-Null
[System.IO.Directory]::CreateDirectory((Join-Path $root 'appdata')) | Out-Null
$utf8 = New-Object System.Text.UTF8Encoding($false)
[IO.File]::WriteAllText(
  (Join-Path $root 'home\.claude\settings.json'),
  '{"env":{"ANTHROPIC_AUTH_TOKEN":"k1","ANTHROPIC_BASE_URL":"https://first.example"}}',
  $utf8
)
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $exe
$psi.UseShellExecute = $false
$psi.EnvironmentVariables['AITOOLPLUS_HOME'] = (Join-Path $root 'home')
$psi.EnvironmentVariables['AITOOLPLUS_APPDATA'] = (Join-Path $root 'appdata')
$process = [Diagnostics.Process]::Start($psi)
Start-Sleep -Seconds 7
[IO.File]::WriteAllText(
  (Join-Path $root 'home\.claude\settings.json'),
  '{"env":{"ANTHROPIC_AUTH_TOKEN":"k2","ANTHROPIC_BASE_URL":"https://second.example"}}',
  $utf8
)
Start-Sleep -Seconds 3
$store = [IO.File]::ReadAllText((Join-Path $root 'appdata\store.json'), [Text.Encoding]::UTF8)
if ($store -notmatch 'second.example') { throw 'watcher did not refresh updated provider' }
if ($store -notmatch 'k2') { throw 'watcher did not refresh updated token' }
Write-Host "PASS config watcher: pid=$($process.Id) refreshed second.example"
Stop-Process -Id $process.Id -Force
