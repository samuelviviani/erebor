# Contributing

Thanks! Before opening a pull request:

1. Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [DEVELOPMENT.md](DEVELOPMENT.md).
2. Run `pnpm check` (typecheck, tests, lint and dependency audits; needs `cargo install --locked cargo-audit`).
3. Add tests for new logic (especially money, fees, processes and validation).
4. Update `CHANGELOG.md` when behaviour changes.

## Non-negotiable rules

- No mining without the user's action, no hidden start, no obfuscation.
- No wallet address, token or credential in the code.
- No shell command built from user input.
- No invented numbers: missing data is shown as "not available".
- New dependencies only when essential, with a compatible licence (update
  `THIRD_PARTY_NOTICES.md`).

## New miners

Implement `MinerAdapter` (`crates/erebor-core/src/miner.rs`), add it to the engine's miner
factory, pin a manifest with SHA-256 and add tests with a simulated process. Don't claim
support until it has been tested on real hardware.

By contributing you agree that your code is distributed under the MIT licence.
