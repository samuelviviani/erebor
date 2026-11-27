# Architecture

## Processes

```text
 erebor (CLI)   erebor monitor (TUI)        Desktop app (Tauri/React)
      │                │                      │
      └──── local Unix socket (peer UID checked) ┘
                       │
        ┌──────────────▼──────── one engine owner per data folder ────────┐
        │ erebor-agent   or   desktop app (in-process engine)             │
        │      └─ Arc<Mutex<Engine>>  (lock on <data dir>/engine.lock)    │
        └──────────────────────────────┬──────────────────────────────────┘
                                       │ child processes
                    XMRig · SRBMiner-MULTI · lolMiner · cpuminer-opt  (or the demo miner)
```

| Path | Role |
|---|---|
| `crates/erebor-core` | The engine and everything it needs: miners, coin choice, earnings, pools, hardware, dev fee, SQLite, config. Also `rpc` (the single command dispatcher), `ipc` (local socket and verified control) and `fleet` (device identity and the versioned status snapshot) |
| `crates/erebor-agent` | Library + `erebor-agent` binary: owns the engine, serves the socket, handles signals and self-update |
| `crates/erebor-cli` | The `erebor` binary (clap): commands, guided setup, `doctor`; embeds the agent and the TUI |
| `crates/erebor-tui` | Terminal dashboard (ratatui/crossterm) |
| `apps/desktop` | Tauri 2 app; owns the engine and serves the socket, or connects to a running agent; runs the device controller |
| `apps/website` | Public website (Next.js static export) |

Principles:

- **One source of state.** The engine lives in exactly one process per data folder (file lock;
  a second owner fails with `engine_busy`). The owner serves the socket; everyone else sends
  it the same `rpc::call` commands. Config changes always go through the engine's
  `update_config`, with the same validation for every frontend.
- **The agent owns the lifecycle when it runs.** A desktop app opened after the agent does
  not create a second engine or a second miner: it becomes a client.
- **Rules live in the engine.** `can_start` refuses real mining without a payout or a miner;
  the `DevFeeManager` is the only fee logic. No frontend duplicates these rules.
- **Verified control** (`ipc::control`): the CLI and the TUI report success only when the
  engine state actually changed.

More: [CLI](CLI.md) · [Agent](AGENT.md) · [TUI](TUI.md)

## Engine modules

```
┌──────────────────────── apps/desktop ────────────────────────┐
│ React UI (Vite, Tailwind, Zustand, Recharts)                  │
│   └─ src/lib/api.ts ── invoke("call", {cmd, args}) ─┐         │
│ src-tauri/src/main.rs ── rpc::call  <───────────────┘         │
│   └─ Arc<Mutex<Engine>> + engine loops (tick 2 s, market 10 min)
└───────────────────────────────┬──────────────────────────────┘
                                │
┌──────────────────── crates/erebor-core ──────────────────────┐
│ engine/   mod (state, types) · lifecycle · coin_choice · tick  │
│           (sampling, safety, holds) · settings · views · jobs  │
│ miner     MinerKind, MinerAdapter trait, DemoMiner            │
│ xmrig · srbminer · lolminer · cpuminer   one adapter per miner│
│ coins     catalog, market data, benchmarks, profitability     │
│ devfee    DevFeeManager (state machine, persisted ledger)     │
│ earnings  estimates: gross → fees → net → electricity         │
│ pools     catalog, reachability, pool APIs                    │
│ hardware  sysinfo + per-platform GPU/power/temperature        │
│ install   pinned manifests, download, SHA-256, extraction     │
│ update    signed self-update of the CLI                       │
│ store     SQLite (rusqlite), migrations via user_version      │
│ wallet    address validation (Kryptex, Monero, Bitcoin, …)    │
│ config    JSON config, validation, atomic writes              │
│ autostart start at login (LaunchAgent / XDG / Run key)        │
│ notify    texts for system notifications and Telegram         │
└───────────────────────────────────────────────────────────────┘
```

### Design choices

- **One core crate.** The modules are small and tightly linked; one crate tests them
  without Tauri. Split only when a second consumer needs part of it.
- **No shared JS packages.** The app and the website share nothing. The TS types in
  `apps/desktop/src/lib/api.ts` mirror the serde structs (camelCase): change both together.
- **Miners are child processes, installed at runtime** from a manifest pinned by SHA-256
  (`crates/erebor-core/*-manifest.json`, URLs limited to each project's GitHub releases).
  Erebor's packages never redistribute miner code. A binary the user picks by hand is
  marked "not verified" and never auto-updated.
- **No shell.** Miners start with `Command::new(binary)` and fixed arguments; every user
  value is validated (`AppConfig::validate`) before it reaches a miner config.
- **Miner APIs on loopback only**, on a random free port with a per-session token where the
  miner supports one; config files are `0600`.
- **Reliable stop.** Stopping kills the child process without relying on its API. The PID is
  written to `miner.pid`, so an orphaned miner is killed on the next start after a crash.
- **Polling.** The UI calls `get_status` every 2 s: simpler than events and good enough.
- **System notifications** come from the Tauri shell (`notifier` in `src-tauri/src/main.rs`),
  not the webview: it polls the engine's alerts and the controller's events every 5 s, so
  they work while the window is hidden in the tray.

### Coin choice

In automatic mode the engine ranks the coins this computer can mine by hashrate × network
reward per hash × price, net of pool and miner fees, and mines the best one. Hashrates are
measured on this PC once a coin has run; before that, GPU coins use the expected hashrate of
the GPU model from `crates/erebor-core/gpu-hashrates.json` (refreshed by
`scripts/update-gpu-hashrates.mjs`). It re-checks every 30 minutes and switches only with a
clear margin. Market data comes from WhatToMine, Kryptex pool stats and CoinGecko.

### Dev fee

`DevFeeManager::tick(effective_secs)` counts only effective mining time (running, not
paused, hashrate reported; long gaps are discarded). When the project's share falls behind
2% of the total by a full block, it switches to the `Project` target for a block of 120 s,
then back to `User`: about 2 minutes every 100 minutes. XMRig switches pool in place
(`PUT /1/config`, no RandomX dataset rebuild); other miners restart on the new target. The
ledger is stored in SQLite, so restarts neither reset nor double-charge the fee. Builds
without `EREBOR_DEV_FEE_BTC` or `EREBOR_DEV_FEE_WALLET` have no fee.

### Earnings

Every sample (10 s) stores the average hashrate, target, shares, the net BTC per hash at that
moment, the **measured** power (when available) and temperatures. User-entered power
estimates are applied at query time, so changing them never rewrites history. Gross earnings
count only `User` samples; the Erebor share is reported separately and never subtracted twice.

### Demo mode

`DemoMiner` starts no process and uses no network; market data is fixed and flagged `demo`;
statistics go to a separate database (`erebor-demo.db`).

### Device identity and snapshot

- `<data dir>/device.json`: a random 128-bit id generated once (never derived from hardware
  serials) and a name (default: hostname), shown by `erebor device show`.
- `fleet::Snapshot` (schema 1) is the typed status that `erebor status --json` prints
  ([format](CLI.md#json-output)). New optional fields don't bump the schema; removing a
  field or changing its meaning does. `ipc::PROTOCOL_VERSION` versions the socket protocol.

### Local data

The data folder (Tauri `app_data_dir`, shared by app, CLI and agent): `config.json`,
`erebor.db`, `erebor-demo.db`, `device.json`, `miners/<miner>-<version>/`, `run/` (miner
configs), `miner.pid`, `engine.lock`, `agent.sock`, `agent.log`.

## Multi-device

```text
 Desktop app ─┐                                     ┌─ erebor-agent (PC/server/rig) ── engine ── miners
 erebor CLI  ─┼─ controller (Fleet) ── TLS 7420 ────┤
              │   controller.db, remote/ identity    └─ erebor-agent (gateway) ── CGMiner API ── ASICs
              └─ this PC (in-process engine / local agent)      direct ── CGMiner API 4028 ── ASICs
```

| Module | Role |
|---|---|
| `remote` | Protocol v1: identity (self-signed cert), pinned TLS 1.3 both ways, pairing proofs, trust list, audit log, line framing |
| `remote::server` | Agent side: allowed commands (dispatches to `rpc::call` and `ipc::control_via`), event stream, ASIC gateway, discovery, rate limiting |
| `remote::client` | Controller side: `Link` (pinned connection), `pair`, `discover` |
| `controller/` | `Fleet` (`mod.rs`), split by job: `registry` (devices, groups, tags, profiles, export/import), `live` (connections, streams, ASIC polling), `rows` (table and totals), `ops` (bulk operations, profiles), `history` (samples, events, alerts), `commands` (`fleet_*` dispatcher). SQLite `controller.db` with versioned migrations and foreign keys |
| `asic` / `asic_sim` | CGMiner/BMMiner/btminer adapters with capability detection; simulators |
| `fleet` | Shared models: device identity, `Snapshot`, `Telemetry` |

Choices: std threads like the rest of the core (no async runtime): one stream thread per
agent (256 KiB stack), bulk operations with 8 workers, ASIC reads in parallel. JSON lines
over TLS instead of HTTP/WebSocket: one framing for requests and events, documented in
[PROTOCOL.md](PROTOCOL.md) with the REST equivalents. The controller lives in whichever
process asks (desktop app with background work; the CLI one-shot), sharing the same
database and certificate.

Measured (`load_many_devices`, release build, Apple M-series): 200 simulated ASICs polled
over TCP in ~55 ms, table + totals for 200 devices in ~1 ms; 720 000 samples (500 devices ×
24 h) compacted to ~24 000 rows / 3.5 MB; a 30-day fleet history query ~2 s on that raw data.
Agent idle: ~10 MB RSS, ~0% CPU.
