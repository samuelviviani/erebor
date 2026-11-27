# Development

## Requirements

- Node.js ≥ 20.9, pnpm 10
- Rust stable (via [rustup](https://rustup.rs)) with `rustfmt` and `clippy`
- Tauri 2 system dependencies:
  - **Linux (Ubuntu 22.04+)**: `sudo apt install build-essential curl wget file libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
  - **Windows**: Microsoft C++ Build Tools, WebView2 (preinstalled on Windows 11)
  - **macOS**: Xcode Command Line Tools

The CLI, agent and TUI alone need only Rust ([docs/CLI.md](docs/CLI.md#build-from-source-ubuntu)).

## Commands

| Command | What it does |
|---|---|
| `pnpm install` | JS dependencies |
| `pnpm --filter @erebor/desktop tauri dev` | desktop app with hot reload |
| `pnpm --filter @erebor/desktop dev` | UI only, at http://localhost:1420, with a simulated backend (`src/dev/mockBackend.ts`, dev only) |
| `pnpm --filter @erebor/desktop tauri build` | installers for the current platform |
| `pnpm --filter @erebor/website dev` | website at http://localhost:3000 |
| `pnpm -r typecheck` / `pnpm test` / `pnpm -r build` | JS checks |
| `pnpm lint:rust` | `cargo fmt --check` + `clippy -D warnings` |
| `pnpm check` | everything above + `cargo test` + dependency audits: run before every push and release |
| `cargo test --workspace` | Rust tests (no real mining: miner processes are simulated) |

## Build variables

| Variable | Use |
|---|---|
| `EREBOR_DEV_FEE_WALLET` | the project's Kryptex ID (compile time). Missing or invalid ⇒ no fee. **Never** put an address in the code. |
| `EREBOR_DEV_FEE_BTC` | the project's Bitcoin address, where the 2% goes for the coins unMineable mines (compile time) |
| `EREBOR_REPO_URL` | repository URL shown in the app's About section |
| `NEXT_PUBLIC_SITE_URL` | website base URL (see `apps/website/.env.example`) |

## Pinning a miner version

```bash
node scripts/update-xmrig-manifest.mjs 6.26.0          # downloads SHA256SUMS + archives, verifies, writes the manifest
node scripts/update-srbminer-manifest.mjs <version>
node scripts/update-github-manifest.mjs lolminer 1.98a  # or: cpuminer v26.1
cargo test -p erebor-core install                      # the tests validate every manifest
```

Manifests are compiled into the binary (`include_str!`). Installed miners older than the
pinned build update themselves (`autoUpdateMiners`).

`scripts/update-gpu-hashrates.mjs` refreshes `crates/erebor-core/gpu-hashrates.json`
(expected hashrate per GPU and algorithm); the app also fetches the latest copy from `main`.

## Tests and real mining

Automated tests never start a real miner or contact a pool: the engine uses a mock miner and
the frontend uses Tauri's `mockIPC`. A manual test with real mining needs a Kryptex ID of your
own.

## Conventions

- Command errors are string codes (`wallet_missing`, …) that the UI translates in
  `src/lib/i18n/` (`err.*`).
- UI text: English is the default; add every key to both `i18n/en.ts` and `i18n/it.ts`
  (the `Record<Key, string>` type enforces it). Notification texts sent outside the app
  (system notifications, Telegram) are in `crates/erebor-core/src/notify.rs`.
- Shared types: change the Rust struct and the interface in `apps/desktop/src/lib/api.ts`
  together.
- Code, comments and documentation are in English.
