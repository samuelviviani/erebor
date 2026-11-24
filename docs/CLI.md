# Erebor CLI (`erebor`)

Erebor from the terminal. It drives **the same engine** as the desktop app: every mining
command goes through the process that owns the engine (the agent or the desktop app) over the
local socket, so the CLI, the TUI and the app always see the same state.

## In short

```bash
erebor               # live dashboard (first run: guided setup)
erebor start         # start mining; Erebor keeps running in the background after the terminal closes
erebor stop          # stop mining
erebor status        # summary
erebor power max     # use the whole CPU (eco 25% · balanced 50%, default · max 100%); `erebor power` shows the level
erebor service on    # always mine: Erebor and mining start with the computer (`off` to undo)
erebor update        # install the latest signed release (it also does this by itself, see below)
```

You don't need to start the agent by hand: commands that need it (`start`, the dashboard,
`wallet set`, `demo`, …) start it. Read-only commands (`status`, `doctor`, lists) start
nothing.

### Mine always, also after a reboot

`erebor service on` registers Erebor as a user service (systemd `--user` on Linux, a
LaunchAgent on macOS, no root), sets `startOnLaunch` and restarts the agent under the
service: mining resumes after every reboot. On Linux it also runs `loginctl enable-linger`
to start at boot without logging in; if the system refuses, the CLI prints the `sudo`
command. On macOS it starts at login. `erebor service` shows the state, `erebor service off`
turns it off (and stops background mining).

### Automatic updates

The CLI installed from release binaries updates itself: every hour the agent checks the
GitHub releases, downloads the binary for your system and installs it **only if its minisign
signature matches the project key** (the same key as the desktop app). It then restarts into
the new version in the same process (also under systemd/launchd) and resumes mining if it was
mining. `erebor update` does the same on demand. Debug builds and the `erebor-agent` binary
never update themselves. If the program's folder isn't writable (for example installed with
`sudo` into `/usr/local/bin`), the update fails without touching anything.

## Install (one command)

```bash
curl -fsSL https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install.sh | sh
```

It downloads the `erebor` binary for your system from the latest GitHub release (macOS and
Linux, x86_64 and aarch64), checks its SHA-256 against `SHA256SUMS`, copies it to
`~/.local/bin` (no root), tells you how to add that to `PATH` and runs `erebor onboard`. No
Rust needed. It updates itself afterwards (see above).

Options (after `| sh -s --`): `--version vX.Y.Z`, `--no-onboard`, `--prefix DIR`, `--yes`,
`--from-source` (build with Rust; also `--repo URL`, `--ref REF`; from a checkout:
`sh scripts/install.sh --from-source`).

## Build from source (Ubuntu)

```bash
# Toolchain (once)
sudo apt update && sudo apt install -y build-essential curl git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# From the repository root: CLI + agent only (no Tauri dependencies needed)
cargo build --release -p erebor-cli -p erebor-agent
export PATH="$PWD/target/release:$PATH"

# Tests (simulated miners, no real mining)
cargo test -p erebor-core -p erebor-agent -p erebor-tui -p erebor-cli
```

`cargo test --workspace` also builds the desktop app and needs the Tauri dependencies (see
[DEVELOPMENT.md](../DEVELOPMENT.md)).

## `erebor onboard`

Runs by itself the first time you launch `erebor` with no arguments. It shows **every**
option, in sections, each with a default (Enter accepts it):

1. **Mode**: real mining or demo (simulated miner).
2. **Payout**: your Kryptex ID, validated on the spot; seed phrases and private keys are
   recognised and refused.
3. **Mining**: coin (auto, or one this computer can mine), CPU threads (eco 25%, balanced
   50%, max 100%, custom %), worker name.
4. **Electricity**: currency and price per kWh (optional), stop when mining costs more than it
   earns.
5. **Safety**: maximum temperature, automatic miner restart.
6. **Miners**: XMRig and, with a supported GPU, SRBMiner-MULTI and lolMiner (pinned builds,
   SHA-256 verified).
7. **Running**: background agent, start with the computer, start mining now.

A final **summary** asks for confirmation: nothing changes before that (Ctrl-C changes
nothing either). Then it starts the agent, saves through the engine, installs the miners, runs
the health check and shows the next commands.

Without prompts (scripts, provisioning); fine-tune afterwards with `erebor config set`:

```bash
erebor onboard --non-interactive --wallet <KRYPTEX_ID> [--coin ID|auto] [--worker NAME] \
  [--pool ID] [--performance eco|balanced|max] [--no-install-miner] [--install-service] \
  [--start] [--no-background] [--skip-health] [--reset]
erebor onboard --non-interactive --mode demo --start
```

## Quick try (demo mode)

```bash
erebor doctor            # environment, hardware, miners, wallet, pool, agent
erebor demo on           # simulated miner: no CPU/GPU use, no network
erebor start
erebor status
erebor monitor           # TUI; Q quits and leaves mining running
erebor stop
erebor agent stop        # stop mining and the agent
```

## Real mining

It starts only with a valid configuration (payout and miner):

```bash
erebor demo off
erebor wallet set <KRYPTEX_ID>   # Kryptex ID (krx…)
erebor miner install xmrig       # pinned release, SHA-256 verified
erebor pools test
erebor start
```

Fees are described on the website and in the README; `erebor status` and the dashboard show
the dev fee live.

## Data folder

Shared with the desktop app: `~/.local/share/org.erebor.desktop` (Linux),
`~/Library/Application Support/org.erebor.desktop` (macOS), `%APPDATA%\org.erebor.desktop`
(Windows). Override with `--data-dir DIR` or `EREBOR_DATA_DIR=DIR` (handy for isolated tests).

## Commands

| Command | What it does | `--json` |
|---|---|---|
| `erebor` | Dashboard; the guided setup on first run | |
| `erebor start` / `stop` | Start / stop mining | ✓ |
| `erebor status` | State, miner, coin/algorithm, pool, hashrate, uptime, shares, temperature, power, estimated earnings, dev fee | ✓ |
| `erebor power [eco\|balanced\|max]` | CPU share used by mining; a running miner restarts with the new level | ✓ |
| `erebor service [on\|off]` | Always mine, also after a reboot | ✓ |
| `erebor update` | Install the latest signed release | ✓ |
| `erebor onboard` | Guided setup (see above) | ✓ |
| `erebor config` | Every setting as `key = value` (`config.json`, camelCase names) | ✓ |
| `erebor config get <KEY>` | One value, or all keys of a section (`config get energy`) | ✓ |
| `erebor config set <KEY> <VALUE>` | Change a setting; VALUE is JSON (`true`, `0.25`, `null`, `[…]`) or text. The engine validates it like the app does; keys managed by Erebor (miner paths, versions) stay unchanged and the CLI says so | ✓ |
| `erebor coins` | Coins: algorithm, CPU/GPU, hashrate measured here, estimated earnings per day, why a coin can't be mined (`*` = chosen) | ✓ |
| `erebor coins set <COIN\|auto>` | Mine that coin (a missing miner installs itself) or go back to automatic | ✓ |
| `erebor logs [-f]` | Engine log; `-f` keeps printing new lines | ✓ |
| `erebor doctor [--offline]` | OS, CPU/GPU, sensors, user/permissions, data folder, agent, config, wallet, miners, pool connectivity | ✓ |
| `erebor mining start\|stop\|pause\|resume\|restart` | Verified control: success only if the state actually changed | ✓ |
| `erebor hardware list` | CPU and GPUs | ✓ |
| `erebor hardware info` | Details, live sensors, capabilities | ✓ |
| `erebor wallet show` | Payout address | ✓ |
| `erebor wallet set <ADDR>` | Set and validate the payout (never seed phrases or private keys) | ✓ |
| `erebor pools list` | Pool catalog (`*` = in use) | ✓ |
| `erebor pools test [--all]` | Wallet compatibility and reachability (TCP latency) | ✓ |
| `erebor pools set <POOL_ID>` | Use a pool from the catalog | ✓ |
| `erebor monitor` | The TUI ([TUI.md](TUI.md)) | |
| `erebor agent start [--foreground]` | Start the agent ([AGENT.md](AGENT.md)) | ✓ |
| `erebor agent stop\|status` | Stop (with mining) / agent state | ✓ |
| `erebor agent install-service\|uninstall-service` | Start at login only, without `startOnLaunch` | |
| `erebor demo on\|off` | Demo mode | ✓ |
| `erebor miner install [xmrig\|srbminer\|lolminer\|cpuminer]` | Download and verify the pinned miner | ✓ |
| `erebor device show` / `device name <NAME>` | This computer's name and id | ✓ |

Every command appears in `erebor --help`: none is hidden.

Useful settings: `performance.preset` (`eco`, `balanced`, `performance`, `custom`) and
`performance.customThreadsPercent`; `pool.worker`; `safety.maxTempC`, `safety.autoRestart`;
`energy.pricePerKwh`, `energy.currency`, `energy.estimatedWatts`, `energy.stopIfUnprofitable`,
`energy.minProfitPerDay`; `schedule.enabled`, `schedule.startMinute`, `schedule.endMinute`,
`schedule.days`; `autoUpdateMiners`; `startOnLaunch`.

`pause`/`resume` work only if the miner supports it (`pause_unsupported` otherwise). `--json`
and `--data-dir` are global: they go before or after the subcommand.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (also "nothing to do", e.g. `start` while already mining: `changed: false`) |
| 1 | Command failed (engine error, validation, `doctor` with blocking problems) |
| 2 | Wrong usage (invalid arguments) |
| 3 | Agent unreachable |

## JSON output

Stable and versioned. On error, with `--json`, on stdout:

```json
{"error": "wallet_missing", "message": "…", "hint": "erebor wallet set <ADDRESS>   (or try first: erebor demo on)"}
```

`erebor status --json` prints the **Snapshot** (schema 1, defined in
`crates/erebor-core/src/fleet.rs`):

```jsonc
{
  "schema": 1,
  "state": "running",            // stopped | running | paused | error
  "demo": false,
  "error": null,
  "startBlocker": null,          // why mining can't start (code), if so
  "miner": {"id": "xmrig", "name": "XMRig", "version": "6.26.0", "supportsPause": true},
  "coin": "XMR", "coinName": "Monero", "algo": "rx/0", "autoCoin": true,
  "poolId": "kryptex",
  "pool": "xmr.kryptex.network:7029",
  "poolConnected": true,
  "hashrate": 4250.0,            // H/s, null when unavailable
  "uptimeSecs": 3723,
  "accepted": 148, "rejected": 2,
  "cpuTempC": 64.0,              // null when the system doesn't expose it
  "gpus": [{"name": "…", "tempC": null, "powerW": null, "utilPercent": null}],
  "powerW": null,
  "powerSource": "unavailable",  // measured | estimated | unavailable
  "devFee": {"enabled": true, "percent": 2.0, "active": "user"},
  "earnings": {"valuePerDay": 0.82, "currency": "EUR", "btcPerDay": 0.0000141, "live": true,
               "energyCostPerDay": 0.32, "netPerDay": 0.50},  // null without a price per kWh
  "restartInSecs": null,
  "job": null                    // install/benchmark in progress
}
```

Compatibility: new fields may be added without changing `schema`; `schema` goes up only when
a field is removed or changes meaning. Unavailable values are `null`, never invented.

`mining … --json` → `{"changed": bool, "snapshot": Snapshot}`; `doctor --json` →
`{"ok": bool, "checks": [{"id","status":"ok|warn|fail","message","hint"}]}`.

## Troubleshooting

| Message / code | Fix |
|---|---|
| `agent_unreachable` (exit 3) | `erebor agent start`; if it fails, read `<data dir>/agent.log` |
| `engine_busy` | Another Erebor (desktop app or agent) already owns the data folder: the CLI talks to it directly, no second agent needed |
| `wallet_missing` | `erebor wallet set <KRYPTEX_ID>`, or try `erebor demo on` first |
| `miner_not_configured` / `miner_binary_changed` | `erebor miner install xmrig` |
| `protocol_version` | CLI and agent versions differ: `erebor agent stop && erebor agent start` |
| `not_agent` on `agent stop` | The desktop app owns the engine: quit the app (mining stops with it) |
| Temperature/power `n/a` | The system doesn't expose the sensor: on Ubuntu `sudo apt install lm-sensors`; RAPL counters are often root-only (Erebor never asks for root) |
| Pool unreachable | Firewall/network; try `erebor pools test --all` and `erebor pools set <id>` |
