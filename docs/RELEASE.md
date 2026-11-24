# Release process

Releases are built locally on a Mac with `scripts/release-local.sh` (GitHub Actions are not
used for releases).

1. **Miners**: to pin a new miner build, run the matching script and review the diff:
   `node scripts/update-xmrig-manifest.mjs <version>` (then check the GPG signature of
   upstream `SHA256SUMS` by hand), `scripts/update-srbminer-manifest.mjs`,
   `scripts/update-github-manifest.mjs lolminer|cpuminer <version>`.
2. **Pools**: re-check hosts, ports and fees in `pools::CATALOG` ([POOLS.md](POOLS.md)).
3. **Version**: bump `Cargo.toml` (workspace), `apps/desktop/src-tauri/tauri.conf.json`,
   `apps/desktop/package.json`, `apps/website/package.json` and `CHANGELOG.md`; commit and push.
4. **Build**: `EREBOR_DEV_FEE_BTC=<project BTC address> [EREBOR_DEV_FEE_WALLET=<project Kryptex ID>] scripts/release-local.sh X.Y.Z`. The 2% goes to the BTC address on unMineable for the coins unMineable mines, to the Kryptex account for the others; a coin with neither has no fee.
   It builds macOS (universal `.dmg`), Windows (NSIS, cross-compiled with `cargo-xwin`) and
   the four signed CLI binaries (Linux via `cargo-zigbuild`), writes `latest.json` and
   `SHA256SUMS` and creates a **draft** GitHub release with a download table. One-time setup
   is listed at the top of the script.
5. **Linux desktop**: the `.deb` needs a Linux machine. Build it there, sign it with the
   updater key, upload it with `gh release upload` and add a `linux-x86_64-deb` entry to
   `latest.json`.
6. **Check** the signatures and the installers, then publish:
   `gh release edit vX.Y.Z --draft=false --latest`.

The fee wallet is never stored in the repository: pass it at build time. Builds without it
apply no fee.

## Automatic updates

The installed app reads `releases/latest/download/latest.json` at start and every hour; when
it finds a newer version it downloads it, verifies the signature, installs it and restarts. If
that fails it shows a banner with an "Update" button.

- The private key is `~/.tauri/erebor.key` (no password); the public key is in
  `tauri.conf.json` (`plugins.updater.pubkey`). **Don't lose it**: without it, installed apps
  can no longer accept updates. Never commit it.
- Updates start when the draft is **published** (not as a pre-release): only then does
  `latest` point at the new version.
- The CLI binaries `erebor-{linux,macos}-{x86_64,aarch64}` are signed with the same key; their
  signatures are in `latest.json` (`cli`). Installed CLIs check it every hour and update
  themselves ([AGENT.md](AGENT.md#self-update)).

## Code signing

- Windows: an Authenticode certificate (OV/EV) via `bundle.windows.certificateThumbprint`.
  Until then SmartScreen warns on first launch.
- macOS: Developer ID + notarisation, not set up yet.
