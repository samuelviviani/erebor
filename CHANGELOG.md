# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versions: [SemVer](https://semver.org/).

## [Unreleased]

## [0.1.7] - 2026-11-25

### Added
- Main payout in Litecoin, Dogecoin or USDT (TRC20) through unMineable, for experienced miners (Coins & wallets › change payout; CLI: `erebor wallet set LTC:…`). Their payout thresholds are far lower than BTC's 0.00075 BTC, so the first payout comes in days.
- Optional donation in Settings › Support Erebor: 1–3% of mining time on top of the 2%, off by default and never charged on past time.
- Tips can be hidden with ×; a hidden tip comes back only when the advice changes.
- unMineable payouts carry Erebor's referral code (lower pool fee for you, a small share for the project paid by unMineable); the Kryptex sign-up link is Erebor's Kryptex Pool referral. Both are disclosed in the app and on the fees page.
- More coins: Firo, Zano, Neurai (SRBMiner), Beam, Karlsen, Radiant (lolMiner), Iron Fish (SRBMiner, 0% miner fee) and Qubitcoin, mined with your own wallet on 2Miners, HeroMiners or WoolyPooly (Beam and Iron Fish also paid in BTC through unMineable). Bitcoin and Litecoin (+DOGE) can now go to unMineable too.
- Every coin's sheet lists what mining it takes on this PC (graphics card, system, miner, payout), met or missing, with one-click miner install. Coins the PC can't mine open too, and big coins no PC can mine (ASIC-only, proof of stake) are explained at the bottom of Coins & wallets.
- Payouts beyond Kryptex. The recommended payout is now BTC straight to your own Bitcoin wallet through unMineable (no account; Kaspa, Alephium, Nexa and Conflux added to the coins it mines). A Kryptex account remains an option.
- Coins and wallets on one page: the main payout on top, then every coin with where it is paid. A coin's sheet sets its own wallet and pool (Kryptex, 2Miners, HeroMiners, WoolyPooly, MoneroOcean, P2Pool on this PC, or any stratum pool), server region, a backup pool that takes over after 3 minutes without a connection, pool minimum payout and a link to the pool balance, and whether the automatic choice may use it. A coin the main payout can't receive offers "Add wallet" right on its row. Pool payouts (SupportXMR) moved to Earnings. Onboarding lets seasoned miners skip the main payout.
- Erebor's 2% goes to the project's BTC address on unMineable when the build has one (`EREBOR_DEV_FEE_BTC`), else to its Kryptex account; it never depends on the user's payout.
- Energy-aware mining: time-of-day price bands from your bill, solar surplus read from a meter (Home Assistant, Shelly or any JSON URL; exported energy costs its feed-in price) and a winter heating credit. "Mine only when it pays" now waits and resumes by itself instead of stopping for good; every sample stores the price it was mined at.
- Mine only when the PC is idle: waits at the first keyboard or mouse input, resumes after N idle minutes (Windows, macOS, Linux with GNOME/KDE/xprintidle).
- GPU power limit (opt-in, watts per card) passed to SRBMiner (`--gpu-plimit`, reset on exit) and lolMiner (`--pl`). Needs administrator rights.
- CSV export of the mining history: Settings › Data, or `erebor export --days N`.
- `erebor doctor` checks GPU power readings and idle detection.
- Efficiency search ("Find the best limit"): tries stock power and four lower GPU limits on the running miner, keeps the one with the best net profit at today's price.
- Thermostat: a room thermometer (Home Assistant, Shelly…) makes mining wait above one temperature and resume below another.
- GPU watchdog: a card stuck below 70% of its usual hashrate for 5 minutes gets the miner restarted once; if it stays low, that becomes the new normal.
- Per-GPU hashrate, efficiency (per watt) and a warning when a card does far less than its model usually does.
- Stop for a while: "Stop for 1 hour" on Home and in the tray menu (also 3 hours); mining starts again by itself.
- Phone notifications for crashes, overheating and hashrate drops via ntfy or a Telegram bot (outgoing only, at most once per problem every 10 minutes).
- Tray icon: closing the window keeps Erebor and mining running; "Quit" in the tray exits and stops mining. A second launch shows the running app.
- Visible updates: at launch an available update installs behind a progress screen; while running, a card with the release notes and "Update now" plus a system notification. Mining interrupted by an update resumes in the new version. Release notes come from this changelog.

### Changed
- Home figures (month estimate, per day, BTC collected) roll digit by digit to each new value (NumberFlow).
- Power use is the measured draw of the hardware that mines (summed GPUs for GPU coins); the manual "estimated watts" setting is gone and the static power tile left the Home page.
- Updates download quietly and install only while not mining; during mining a small "ready · Restart" pill replaces the banner.
- Home hides the CPU mode selector when mining on the GPU (the mode never applied there).
- Documentation and website in English, updated to the current features.
- Code cleanup: unused functions, translations, help entries and a dependency removed.

### Fixed
- Erebor's 2% on coins unMineable doesn't list (Zephyr, Tari RandomX, OctaSpace, ASIC coins) now goes through unMineable with the same algorithm instead of being skipped.
- Coins & wallets: coins you can mine come first, then the ones that only need a wallet, then the ones this PC can't mine; the "add wallet" button no longer overflows its row.
- Home: the BTC balance stays on the right and on one line, whatever its digits.
- Settings: advanced options sit in one panel, so it is clear where they start and end. Shorter demo label.
- macOS: the collapsed sidebar shows square icon buttons and the window buttons fit inside it.
- "QTC" on Kryptex, 2Miners and unMineable is Quantus, not Qubitcoin: Erebor mined it with Qubitcoin's algorithm, so shares were rejected. Quantus is now its own coin (`quantus`); Qubitcoin stays "QTC" on HeroMiners. Settings that pointed at Kryptex or 2Miners move to Quantus by themselves.
- The automatic choice never picks ASIC coins, as the app always said.

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
