param(
  [string]$Page = "claude_code",
  [string]$Tab = "providers",
  [string]$Name = "screen",
  [int]$WaitSeconds = 6,
  [Nullable[int]]$ClickX = $null,
  [Nullable[int]]$ClickY = $null,
  [string]$OpenProvider = "",
  [string]$ProviderId = "",
  [string]$FetchModels = "",
  [string]$ActiveDropdown = "",
  [string]$DialogTab = "",
  [string]$SeedAdvanced = "",
  [int]$ScrollDelta = 0,
  [Nullable[int]]$ScrollX = $null,
  [Nullable[int]]$ScrollY = $null,
  [string]$Session = ""
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
if ($Session -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_START_SESSION'] = $Session }
if ($OpenProvider -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_PROVIDER'] = $OpenProvider }
if ($ProviderId -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_PROVIDER_ID'] = $ProviderId }
if ($FetchModels -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_FETCH_MODELS'] = $FetchModels }
if ($ActiveDropdown -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_ACTIVE_DROPDOWN'] = $ActiveDropdown }
if ($ExpandPi -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_EXPAND_PI'] = $ExpandPi }
if ($DialogTab -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_DIALOG_TAB'] = $DialogTab }
if ($SeedAdvanced -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_SEED_ADVANCED'] = $SeedAdvanced }
if ($env:AITOOLPLUS_THEME_MODE) { $psi.EnvironmentVariables['AITOOLPLUS_THEME_MODE'] = $env:AITOOLPLUS_THEME_MODE }

$launched = [System.Diagnostics.Process]::Start($psi)
Write-Host "launched pid=$($launched.Id) page=$Page tab=$Tab"
Start-Sleep -Seconds $WaitSeconds
$proc = Get-Process aitoolplus -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "No aitoolplus window" }
& 'D:\program\rust\aitoolplus\tools\capture-ui.ps1' -Name $Name -ClickX $ClickX -ClickY $ClickY -ScrollDelta $ScrollDelta -ScrollX $ScrollX -ScrollY $ScrollY
