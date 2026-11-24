# Third-party components

Direct dependencies and their licences (as declared by the packages). Before each release,
generate and review the full list including transitive dependencies, e.g. with
`cargo about` / `cargo deny` and `pnpm licenses list --prod`.

## Miners (not bundled)

Erebor **does not redistribute** any miner. On demand it downloads the official release from
the project's GitHub page, checks it against a SHA-256 pinned in this repository
(`crates/erebor-core/*-manifest.json`), extracts it and runs it **unmodified** as a separate
process with fixed arguments, talking to it through config files and a local API. Erebor is
not a derivative work and stays MIT-licensed.

| Miner | Licence | Developer fee | Notes |
|---|---|---|---|
| [XMRig](https://github.com/xmrig/xmrig) | GPL-3.0 | 1% of time (`donate-level`, the minimum of official builds, `src/donate.h`) | If XMRig were ever bundled, GPLv3 obligations would apply to it (licence, copyright, corresponding source) |
| [SRBMiner-MULTI](https://github.com/doktor83/SRBMiner-Multi) | free, closed source, © doktor83 | per algorithm (e.g. pearlhash 2%, etchash 0.65%, kawpow 0.85%) | The licence forbids modifying, redistributing or "incorporating into another product" without permission: written confirmation from the author is still pending ([LEGAL-CHECKLIST](docs/LEGAL-CHECKLIST.md)). Its stats API is read-only; Erebor doesn't enable the remote restart/shutdown URLs |
| [lolMiner](https://github.com/Lolliedieb/lolMiner-releases) | free, closed source | per algorithm (e.g. Alephium and Kaspa 0.75%, Conflux and Nexa 2%, Tari SHA3x 1%, C29 2%) | API bound to `127.0.0.1` (`--apihost`) instead of all interfaces |
| [cpuminer-opt](https://github.com/JayDDee/cpuminer-opt) | GPL-2.0 | none | Official builds exist for Windows only |

Developer fees are included in Erebor's estimates and shown in the app.

## Coin logos

`apps/desktop/public/coins/*.png`: logos of the catalog coins, downloaded from CoinGecko
(coin-images.coingecko.com) and resized (Ergo and Pearl cropped to a circle). They are
trademarks of their projects, used only to identify the coins; no affiliation or endorsement
is implied.

## Data services

- WhatToMine (`coins.json`, `asic.json`, GPU pages): network difficulty, rewards, BTC prices,
  expected GPU hashrates.
- Kryptex (`pool.kryptex.com/<coin>/api/v1/pool/stats`): pool fee per coin and network data.
- CoinGecko API: BTC and coin prices in EUR/USD. Subject to CoinGecko's terms; check their
  limits for public use.
- SupportXMR public API, when that pool is selected: subject to the pool's terms.

## Rust

| Crate | Licence |
|---|---|
| tauri, tauri-build, tauri-plugin-dialog, -opener, -updater, -process | MIT OR Apache-2.0 |
| serde, serde_json | MIT OR Apache-2.0 |
| rusqlite (bundled SQLite, public domain) | MIT |
| sysinfo | MIT |
| ureq (rustls, webpki-roots) | MIT OR Apache-2.0 (webpki-roots: MPL-2.0 / CDLA) |
| sha2, base64, flate2, tar, rand, chrono, tempfile | MIT OR Apache-2.0 |
| tiny-keccak | CC0-1.0 |
| zip | MIT |
| minisign-verify | MIT |
| clap, anstream, anstyle | MIT OR Apache-2.0 |
| dialoguer | MIT |
| ratatui, crossterm | MIT |
| ctrlc | MIT OR Apache-2.0 |

## JavaScript

| Package | Licence |
|---|---|
| react, react-dom | MIT |
| @tauri-apps/api, @tauri-apps/plugin-*, @tauri-apps/cli | MIT OR Apache-2.0 |
| lucide-react | ISC |
| recharts | MIT |
| zustand | MIT |
| next | MIT |
| tailwindcss, vite, vitest | MIT |
| typescript | Apache-2.0 |
