# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions: [SemVer](https://semver.org/).

## [Unreleased]

### Changed
- Documentation and website in English, updated to the current features.
- Code cleanup: unused functions, translations, help entries and a dependency removed.

## [0.1.6] - 2026-11-24

### Added
- CLI: `erebor config` (every setting: `get`, `set`), `erebor coins` (coins with hashrate and estimated earnings, `coins set`), `erebor logs [-f]`. No hidden commands in `erebor --help`.
- One-command install: `curl -fsSL …/scripts/install.sh | sh` downloads the release binary and checks its SHA-256 (Rust no longer needed; `--from-source` to build).
- Snapshot: `earnings.energyCostPerDay` and `earnings.netPerDay`.
- Tidier releases: update signatures live in `latest.json` (the CLI's too) instead of a `.sig` file per download; release notes have a download table.

### Changed
- Terminal dashboard: the logo shows only at start; compact screen with mining, net earnings, a device table (temperature, power, load) and the log.
- Terminal setup: every option in sections (coin, CPU threads, worker, electricity price, safety, miners, startup), with a summary and a final confirmation.

## [0.1.5] - 2026-11-24

### Fixed
- Erebor fee: time mined with a version without an active fee is no longer charged after updating (it caused a long block of mining for the project right after the update).

## [0.1.4] - 2026-11-24

### Fixed
- Windows: when the antivirus blocks the miner during install, the app now says so and explains how to allow it, instead of showing "Access denied (os error 5)".
- Initial setup stuck after a failed miner install: there is now a "Retry" button.

## [0.1.3] - 2026-11-24

### Added
- New SHA-256-verified miners: lolMiner (GPU: Alephium, Conflux, Nexa, Kaspa, Tari) and cpuminer-opt (CPU, Windows only: SHA-256 and Scrypt). Every Kryptex pool coin a computer can mine, with icons.
- Miners update themselves to the latest verified build (can be turned off).

### Changed
- English is the default language; README rewritten.
- Payouts through a Kryptex account only (other options to come).

### Removed
- Remote access and `erebor-node`: Erebor works only on the local computer.

## [0.1.2] - 2026-11-23

### Added
- `erebor power eco|balanced|max`: how much CPU mining uses; while mining, the miner restarts with the new level.
- Coloured terminal output (help, status, doctor, setup); colours turn off outside a terminal or with `NO_COLOR`. Help reordered: main commands on top, the rest under "More commands".
- Automatic updates: the desktop app (Tauri updater, with an "Update" banner if it fails) and the CLI (`erebor update`, and hourly from the agent, restarting into the new version and resuming mining). Signed CLI binaries in the releases.
- `erebor service on|off`: always mine, also after a reboot (user service, `startOnLaunch`, linger on Linux); also offered by the guided setup.
- Redesigned terminal dashboard: ASCII logo, earnings per day and month, temperature bars, contextual keys, errors explained with the command that fixes them.
- Simpler CLI: `erebor` opens the dashboard, `erebor start` / `erebor stop`, the agent starts by itself when needed, help with examples.

### Changed
- No more fee acceptance inside the software (engine, `erebor consent`, setup, app welcome screen): fees and terms are described on the website and in the README. Status and dashboard still show the dev fee live.

### Fixed
- NVIDIA/AMD GPU never used by the CLI: `gpu_mining_supported` was always false, so the setup didn't install SRBMiner. The setup now installs it when a supported GPU is present, and `erebor doctor` reports an unused GPU.

## [0.1.0] - 2026-11-23

### Added
- pnpm + Cargo monorepo: `erebor-core`, Tauri 2 app, Next.js website.
- XMRig integration: JSON config, child process, loopback API, pause/resume, hot pool switch, offline benchmark, install from a SHA-256 manifest.
- GPU mining (NVIDIA/AMD, Windows and Linux) with SRBMiner-MULTI 3.8.8 pinned by SHA-256: Pearl, Ethereum Classic, EthereumPoW, Ergo, Ravencoin, Xelis on Kryptex.
- Coin choice: automatic (measures the real hashrate and mines the most profitable coin, re-checking every 30 minutes with a 10% margin) or manual.
- Market data per coin (WhatToMine, Kryptex, CoinGecko); earnings value tracked in BTC net of pool and miner fees.
- Bitcoin payouts to the user's own wallet through unMineable (`bc1`/`1`/`3` addresses validated with BIP173/350 and base58check checksums), and per-coin wallets (`coinWallets`).
- DevFeeManager (persisted ledger), Monero address validation (base58, Keccak, network).
- SQLite statistics, earnings estimates, electricity costs, scheduling, temperature and profit thresholds, crash recovery.
- UI: onboarding, dashboard, coins, wallet, earnings, settings; English/Italian; dark/light theme; demo mode; GPU monitoring.
- Guided setup `erebor onboard` (quick start / custom / demo, validated wallet, miner install, background agent, health check, summary; `--non-interactive`) and the `scripts/install.sh` installer.
- `erebor` CLI (clap): `status`, `doctor`, `mining start|stop|pause|resume|restart` with state verification, `hardware`, `wallet`, `pools`, `agent`, `demo`, `miner install`, `device`; stable `--json` output and documented exit codes ([docs/CLI.md](docs/CLI.md)).
- `erebor-agent`: owns the engine and the miners, local Unix socket (0600, peer UID checked on both sides, JSON protocol v1), clean shutdown on SIGINT/SIGTERM, single instance, optional systemd user service.
- TUI `erebor monitor` (ratatui/crossterm): live metrics, log, start/pause/stop/restart; `Q` quits without stopping mining.
- Device id and name, typed and versioned `Snapshot` (`snapshot`, `device_info`, `set_device_name` commands).
- The desktop app serves the local socket when it owns the engine and, if an agent is already running, connects to it instead of starting a second engine.
- CI (lint, typecheck, tests, build, audit).

### Changed
- Kryptex is the default pool: mine with a Kryptex ID (`krx…`, credited in BTC on the account).
- Erebor fee: 2% of effective mining time, in 2-minute blocks, to the project's Kryptex account with a single `erebor` worker.
- Three-step onboarding (where earnings go → automatic PC setup → ready); pool, miner and performance moved to Settings.
- New logo and gold accent colour.
- Start-up services (systemd user unit/LaunchAgent) restart only after a crash, not after a clean stop.
