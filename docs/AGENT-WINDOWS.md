# Erebor Agent on Windows

> Not yet verified on a real Windows machine: the agent cross-compiles for Windows and
> the protocol is cross-platform, but the installer and the scheduled task below are
> untested. Please report results.

## Install

From an Administrator PowerShell (only the installation needs it; the agent runs without
admin rights):

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install-agent.ps1 -OutFile install-agent.ps1
notepad install-agent.ps1            # read what it does
powershell -ExecutionPolicy Bypass -File install-agent.ps1 [-Listen 100.64.0.5:7420] [-Version vX.Y.Z]
```

It downloads the signed `erebor-windows-x86_64.exe` of the release (SHA-256 checked) into
`C:\Program Files\Erebor` as `erebor.exe` and `erebor-agent.exe`, creates
`C:\ProgramData\Erebor\agent` (only LOCAL SERVICE and Administrators), turns remote control
on, adds firewall rules for TCP/UDP 7420 on private and domain networks only, and registers
the scheduled task **Erebor Agent**: at startup, as `NT AUTHORITY\LOCAL SERVICE`, restarted
every minute if it stops. It prints a pairing code at the end.

Why a scheduled task and not a Windows service: a service binary must answer the Service
Control Manager, which the agent doesn't implement yet; the task gives the same start at
boot, restart on failure and least privilege.

## Use

```powershell
& "C:\Program Files\Erebor\erebor-agent.exe" pair --data-dir C:\ProgramData\Erebor\agent
& "C:\Program Files\Erebor\erebor-agent.exe" status --data-dir C:\ProgramData\Erebor\agent
Get-Content C:\ProgramData\Erebor\agent\agent.log -Wait        # log
Stop-ScheduledTask "Erebor Agent"; Start-ScheduledTask "Erebor Agent"
```

On Windows there is no local socket: the agent is managed over the remote protocol (from
Erebor Desktop or `erebor devices` on another computer). Defender flags miners as
"CoinMiner": exclude `C:\ProgramData\Erebor\agent\miners` only if you trust the verified
builds Erebor installs.

## Uninstall

```powershell
powershell -ExecutionPolicy Bypass -File install-agent.ps1 -Uninstall [-Purge]
```
