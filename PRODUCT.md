# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

(Desktop app is Tauri 2 + React rendered in a webview on Windows, Linux and macOS; the website is Next.js with a static export. Both share this record.)

## Users

Two audiences, layered:

- **People who already mine.** They have done the profitability math and used to run miners by hand (e.g. SRBMiner on Linux, watching it constantly, unsure whether a download was safe). They switch to Erebor for safety and calm: verified miners, automation, control. They are the growth audience.
- **People who want to start.** They have a capable computer and want to mine without the maze of coins, pools, miners and command lines.

A newcomer must never face advanced features. Those features live behind an explicit "I already mine" / advanced path, and each one carries a plain explanation.

## Product Purpose

Erebor is a source-available, local-first, non-custodial mining manager: it mines on the user's own computer and, on request, manages their other computers, servers, rigs and ASICs from one dashboard. It picks the coin, runs verified miners and shows what mining really earns after electricity. Success is the user mining calmly, without babysitting a miner or wondering whether it is safe or worth it.

## Positioning

- Source-available (ESAL) and local-first: everything runs on the user's machine.
- Only official miner releases (XMRig, SRBMiner-MULTI, lolMiner, cpuminer-opt), checked against a SHA-256 pinned in the repository before install. Miners are downloaded, never bundled.
- Picks the coin from public profitability data (WhatToMine, Kryptex) and re-checks every half hour; the user can always choose a coin.
- Shows estimated earnings minus electricity, next to a link to the real pool balance.
- No promise of returns. Mining starts only when the user says so, stops when the app quits, and stops by itself if the computer runs too hot.

## Operating Context

- Desktop app, CLI, terminal dashboard (TUI) and a background agent all drive the same engine: one miner, one state.
- Payouts: default BTC to the user's own wallet through unMineable (no account); Kryptex account as an alternative. Experienced miners can send each coin to their own wallet on a pool of their choice (2Miners, HeroMiners, WoolyPooly, MoneroOcean, P2Pool, custom stratum) with a backup pool.
- On Windows, Defender flags every miner as "CoinMiner"; Erebor explains this and, only with consent, excludes its own miners folder.
- App and CLI update themselves from signed GitHub releases; miners auto-update to the build pinned in the app (toggle in advanced settings).
- The website is the download, docs, FAQ, fees, privacy and terms surface.

## Capabilities and Constraints

- Start/stop only: no pause button.
- Device management (added 2026-11-26 at the user's request): paired agents over mutual TLS, ASICs through their own API (directly or via a gateway agent), groups, profiles, alerts. No account or central server; off unless set up. Chosen at first start ("Start mining" / "Manage devices"); both stay available.
- Languages: English (default) and Italian.
- Erebor keeps 2% of mining time as its fee, shown in the app and on the website.
- ASIC-territory coins (SHA-256, Scrypt on CPU) are mineable but never auto-picked.
- Built since the first roadmap: energy-price / solar-driven mining, idle-only mining, GPU power limit (opt-in, lower only), CSV export, winter heating mode, system notifications. Not built: code signing (macOS notarization, Windows Authenticode), Windows local socket (CLI/TUI ↔ app).

## Brand Commitments

- Name: Erebor. Tagline: "Mining, without the maze."
- Voice: plain, honest, direct. States risks openly (mining uses a lot of power and guarantees no return).
- Assets: `brand/` (mark light/dark SVG, logos, app icon); website copies in `apps/website/public/`.

## Evidence on Hand

- Demo dashboard screenshot: `apps/website/public/screenshot-dashboard-demo.jpg`.
- Real supported hardware/coin/miner table in `README.md`.
- No testimonials, user counts, earnings figures or press exist. Do not fabricate them.

## Product Principles

1. Trust is verifiable: every claim about safety points to something the user can check (hashes, signatures, source).
2. Honest numbers: show net earnings after electricity, never imply guaranteed profit.
3. The user stays in charge: nothing mines without an explicit start, and every automation can be turned off.
4. Simple by default, deep on request: newcomers see the simple path; experienced miners get the controls behind an explicit choice.
5. Calm over attention: the app should let people stop watching their miner.
