# Erebor Agent installer for Windows (PowerShell, run as Administrator once).
#
#   Invoke-WebRequest https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install-agent.ps1 -OutFile install-agent.ps1
#   notepad install-agent.ps1          # read it first
#   powershell -ExecutionPolicy Bypass -File install-agent.ps1
#
# What it does: downloads the signed `erebor` binary of the latest release (SHA-256
# checked) to C:\Program Files\Erebor, copies it as erebor-agent.exe, creates the state
# folder C:\ProgramData\Erebor\agent (only LOCAL SERVICE and Administrators can read
# it), turns remote control on (TLS, port 7420, Windows Firewall rule for private
# networks only) and registers a scheduled task "Erebor Agent" that starts at boot as
# NT AUTHORITY\LOCAL SERVICE (no admin rights, no password stored) and restarts it if
# it fails. Log: C:\ProgramData\Erebor\agent\agent.log.
#
#   -Uninstall        remove task, firewall rule and program (keeps the state folder;
#                     -Purge deletes it too)
#   -Version vX.Y.Z   a specific release      -Listen ADDR   e.g. 100.64.0.5:7420
#
# Not verified on real Windows hardware yet (see docs/AGENT-WINDOWS.md).
param([switch]$Uninstall, [switch]$Purge, [string]$Version = "", [string]$Listen = "")
$ErrorActionPreference = "Stop"
$Repo = "samuelviviani/erebor"
$Prog = Join-Path $env:ProgramFiles "Erebor"
$Data = Join-Path $env:ProgramData "Erebor\agent"
$Task = "Erebor Agent"

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw "Run this from an Administrator PowerShell (only to install the task; the agent itself runs without admin rights)."
}

if ($Uninstall) {
  schtasks /End /TN $Task 2>$null | Out-Null
  schtasks /Delete /TN $Task /F 2>$null | Out-Null
  Remove-NetFirewallRule -DisplayName $Task -ErrorAction SilentlyContinue
  Remove-Item (Join-Path $Prog "erebor-agent.exe") -ErrorAction SilentlyContinue
  if ($Purge) { Remove-Item $Data -Recurse -Force -ErrorAction SilentlyContinue }
  Write-Host "Erebor Agent removed."
  exit 0
}

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
$asset = "erebor-windows-$arch.exe"
$base = if ($Version) { "https://github.com/$Repo/releases/download/$Version" } else { "https://github.com/$Repo/releases/latest/download" }
$tmp = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ("erebor-" + [guid]::NewGuid()))
Write-Host "Downloading $asset"
Invoke-WebRequest "$base/$asset" -OutFile (Join-Path $tmp "erebor.exe")
Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $tmp "SHA256SUMS")
$want = (Get-Content (Join-Path $tmp "SHA256SUMS") | Where-Object { $_ -match "\s$([regex]::Escape($asset))$" }) -replace "\s.*", ""
$got = (Get-FileHash (Join-Path $tmp "erebor.exe") -Algorithm SHA256).Hash.ToLower()
if (-not $want -or $want -ne $got) { throw "checksum mismatch for $asset: not installed" }
Write-Host "SHA-256 verified"

New-Item -ItemType Directory -Force -Path $Prog, $Data | Out-Null
Copy-Item (Join-Path $tmp "erebor.exe") (Join-Path $Prog "erebor.exe") -Force
Copy-Item (Join-Path $tmp "erebor.exe") (Join-Path $Prog "erebor-agent.exe") -Force
Remove-Item $tmp -Recurse -Force

# State readable only by the service account and administrators (keys live here).
icacls $Data /inheritance:r /grant:r "*S-1-5-19:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" | Out-Null

$agent = Join-Path $Prog "erebor-agent.exe"
$init = @("init", "--data-dir", $Data)
if ($Listen) { $init += @("--listen", $Listen) }
& $agent @init
# Files created by this (admin) run must belong to the service account too.
icacls $Data /grant:r "*S-1-5-19:(OI)(CI)F" /T | Out-Null

New-NetFirewallRule -DisplayName $Task -Direction Inbound -Protocol TCP -LocalPort 7420 -Profile Private,Domain -Action Allow -ErrorAction SilentlyContinue | Out-Null
New-NetFirewallRule -DisplayName $Task -Direction Inbound -Protocol UDP -LocalPort 7420 -Profile Private,Domain -Action Allow -ErrorAction SilentlyContinue | Out-Null

$cmd = "cmd.exe"
$arg = "/c `"`"$agent`" start --data-dir `"$Data`" >> `"$Data\agent.log`" 2>&1`""
$action = New-ScheduledTaskAction -Execute $cmd -Argument $arg
$trigger = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId "NT AUTHORITY\LOCALSERVICE" -LogonType ServiceAccount
$settings = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $Task -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $Task
Write-Host "Erebor Agent running (task '$Task'). Log: $Data\agent.log"
Write-Host "Pair it with a controller (this code is valid 10 minutes):"
& $agent pair --data-dir $Data
icacls $Data /grant:r "*S-1-5-19:(OI)(CI)F" /T | Out-Null
