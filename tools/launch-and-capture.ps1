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
  [string]$Session = "",
  [string]$AntigravityDetails = "",
  [string]$AntigravityDevice = "",
  [string]$AntigravityLabel = "",
  [string]$AntigravityWindow = "",
  [string]$AntigravityTier = "",
  [string]$OpenSkill = "",
  [string]$OpenSkillGitModal = "",
  [string]$OpenMcp = "",
  [string]$OpenMcpImportJson = "",
  [string]$OpenPromptDialog = "",
  [string]$PromptId = "",
  [string]$OpenAntigravityDialog = "",
  [string]$AntigravityTab = "",
  [string]$TestToast = "",
  [string]$TestToastError = "",
  [string]$UsageSubTab = ""
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
if ($AntigravityDetails -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_ANTIGRAVITY_DETAILS'] = $AntigravityDetails }
if ($AntigravityDevice -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_ANTIGRAVITY_DEVICE'] = $AntigravityDevice }
if ($AntigravityLabel -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_ANTIGRAVITY_LABEL'] = $AntigravityLabel }
if ($AntigravityWindow -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_ANTIGRAVITY_WINDOW'] = $AntigravityWindow }
if ($AntigravityTier -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_ANTIGRAVITY_TIER'] = $AntigravityTier }
if ($OpenSkill -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_SKILL'] = $OpenSkill }
if ($OpenSkillGitModal -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_SKILL_GIT_MODAL'] = "1" }
if ($OpenAntigravityDialog -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_ANTIGRAVITY_DIALOG'] = $OpenAntigravityDialog }
if ($AntigravityTab -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_ANTIGRAVITY_TAB'] = $AntigravityTab }
if ($OpenMcp -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_MCP'] = $OpenMcp }
if ($OpenMcpImportJson -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_MCP_IMPORT_JSON'] = "1" }
if ($OpenPromptDialog -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_OPEN_PROMPT_DIALOG'] = "1" }
if ($PromptId -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_PROMPT_ID'] = $PromptId }
if ($TestToast -ne "") {
  $psi.EnvironmentVariables['AITOOLPLUS_TEST_TOAST'] = $TestToast
  $psi.EnvironmentVariables['AITOOLPLUS_TEST_TOAST_ERROR'] = $TestToastError
}
if ($UsageSubTab -ne "") { $psi.EnvironmentVariables['AITOOLPLUS_USAGE_SUBTAB'] = $UsageSubTab }
if ($env:AITOOLPLUS_THEME_MODE) { $psi.EnvironmentVariables['AITOOLPLUS_THEME_MODE'] = $env:AITOOLPLUS_THEME_MODE }
if ($env:AITOOLPLUS_TEST_UPDATE) { $psi.EnvironmentVariables['AITOOLPLUS_TEST_UPDATE'] = $env:AITOOLPLUS_TEST_UPDATE }
if ($env:AITOOLPLUS_CURRENT_VERSION) { $psi.EnvironmentVariables['AITOOLPLUS_CURRENT_VERSION'] = $env:AITOOLPLUS_CURRENT_VERSION }
if ($env:AITOOLPLUS_FORCE_PORTABLE) { $psi.EnvironmentVariables['AITOOLPLUS_FORCE_PORTABLE'] = $env:AITOOLPLUS_FORCE_PORTABLE }
if ($env:AITOOLPLUS_FORCE_INSTALLER) { $psi.EnvironmentVariables['AITOOLPLUS_FORCE_INSTALLER'] = $env:AITOOLPLUS_FORCE_INSTALLER }

$launched = [System.Diagnostics.Process]::Start($psi)
Write-Host "launched pid=$($launched.Id) page=$Page tab=$Tab"
Start-Sleep -Seconds $WaitSeconds
$proc = Get-Process aitoolplus -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "No aitoolplus window" }
& 'D:\program\rust\aitoolplus\tools\capture-ui.ps1' -Name $Name -ClickX $ClickX -ClickY $ClickY -ScrollDelta $ScrollDelta -ScrollX $ScrollX -ScrollY $ScrollY
