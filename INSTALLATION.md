# Installation

> Download the latest version from [GitHub Releases](https://github.com/samuelviviani/erebor/releases/latest).
> Once installed, Erebor updates itself.

## Windows 10/11 (x64)

1. Download `Erebor_<version>_x64-setup.exe`.
2. Check the hash: `Get-FileHash .\Erebor_*_x64-setup.exe -Algorithm SHA256` and compare it
   with `SHA256SUMS` in the release.
3. Run the installer. SmartScreen may warn because the binary is not code-signed yet.
4. WebView2 is required (built into Windows 11; the installer fetches it on Windows 10).

Windows Defender flags every mining program as "CoinMiner". Erebor explains this and, only if
you agree, excludes its own miners folder (Windows asks for administrator approval). Erebor
never tries to get around an antivirus: the decision is yours.

## Linux (Ubuntu 22.04+ x64)

- `sudo apt install ./Erebor_<version>_amd64.deb`
- Runtime dependencies: `libwebkit2gtk-4.1-0`, `libgtk-3-0`.
- Temperature needs sensors exposed through hwmon (`lm-sensors`). Power uses RAPL
  (`/sys/class/powercap`), often readable only by root: in that case set an estimate in
  Settings › Electricity.

## macOS (Apple Silicon and Intel)

Download `Erebor_<version>_universal.dmg` and drag Erebor to Applications. The app is not
notarised by Apple yet, so macOS may ask for confirmation the first time you open it. GPU
mining is not available on macOS.

## Agent for other computers

To manage a computer, server or rig from Erebor Desktop: [Linux](docs/AGENT-LINUX.md) ·
[Windows](docs/AGENT-WINDOWS.md) · [macOS](docs/AGENT.md#macos), then pair it
([Devices](docs/DEVICES.md)).

## Miners

On first run Erebor downloads the pinned official release of each miner it needs from GitHub
and checks its SHA-256; in the advanced settings you can pick an existing binary instead
(marked "not verified"). XMRig's huge pages and MSR mod may need administrator rights: Erebor
doesn't ask for them and doesn't change the system.

## Command line (Linux, macOS, WSL2)

```bash
curl -fsSL https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install.sh | sh
# downloads the signed binary, checks its SHA-256, installs it into ~/.local/bin and starts the setup
# from a checkout, building it: sh scripts/install.sh --from-source
```

Commands, building on Ubuntu and troubleshooting: [docs/CLI.md](docs/CLI.md). The background
agent and the optional user service: [docs/AGENT.md](docs/AGENT.md).

## Uninstall

Uninstall the app from the system. Delete local data from Settings › Data › "Delete all
local data" (or delete the data folder shown there). CLI: `erebor service off`, then remove
`~/.local/bin/erebor` and the data folder ([docs/CLI.md](docs/CLI.md#data-folder)).

## Common errors

| Message | Fix |
|---|---|
| "Install or select a miner" | Let the setup install it, or `erebor miner install <miner>` |
| "The miner binary changed after verification" | Reinstall or select it again: the file was modified |
| "Blocked by the antivirus" | Allow Erebor's miners folder (the app offers to do it on Windows) |
| Pool unreachable | Check firewall, network or company proxy |
