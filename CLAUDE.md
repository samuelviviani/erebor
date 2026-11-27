# Erebor — guide for coding agents

Mining manager: Rust core + Tauri 2 desktop app (React) + `erebor` CLI/TUI + remote agent.
Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the why, [DEVELOPMENT.md](DEVELOPMENT.md)
for commands and conventions, [CONTRIBUTING.md](CONTRIBUTING.md) for the non-negotiable rules.

`cargo` lives in `~/.cargo/bin` (may not be on PATH).

## Checks before saying done

```bash
pnpm check   # typecheck, vitest, fmt + clippy -D warnings, cargo test, pnpm/cargo audit
```

GitHub Actions is off for this account, so `pnpm check` *is* the CI: `release-local.sh` refuses
to release unless it passes. `cargo audit` needs `cargo install --locked cargo-audit` once.

## Where things live

| To change… | Look in |
|---|---|
| A command the UI/CLI sends (`get_status`, `start_mining`…) | `crates/erebor-core/src/rpc.rs` (one dispatcher) → `Engine` method |
| Engine state, shared types (`Status`, `Alert`) | `crates/erebor-core/src/engine/mod.rs` |
| Start/stop, crash restart, pool routes, power-limit search | `engine/lifecycle.rs` |
| Which coin to mine, auto-switch, projections | `engine/coin_choice.rs` |
| Every-2-s work: samples, dev fee, temperature, holds, schedule | `engine/tick.rs` |
| Settings changes, CSV, delete data | `engine/settings.rs` |
| Status/stats views, market data | `engine/views.rs` |
| Miner install/update, benchmark, background loops | `engine/jobs.rs` |
| Config fields and validation | `crates/erebor-core/src/config.rs` (`AppConfig::validate`) |
| Other devices (registry, live state, alerts, bulk ops) | `crates/erebor-core/src/controller/` (see `mod.rs` header) |
| Notification texts (system notifications, Telegram) | `crates/erebor-core/src/notify.rs` |
| Tray, system notifications, window, app startup | `apps/desktop/src-tauri/src/main.rs` |
| `erebor` commands | `crates/erebor-cli/src/args.rs` (clap) → `main.rs::run` → `status.rs` / `mining.rs` / `settings.rs` / `agent.rs` / `fleet.rs` |
| UI ↔ Rust types and calls | `apps/desktop/src/lib/api.ts` (mirror of the serde structs, camelCase) |
| UI state, polling | `apps/desktop/src/lib/store.ts` |
| UI text | `apps/desktop/src/lib/i18n/en.ts` and `it.ts` (same keys, type-checked) |
| Pages / navigation | `apps/desktop/src/pages/*`, `App.tsx` (`PAGES`), `components/Layout.tsx` (nav) |
| Reusable UI pieces | `components/ui.tsx` (primitives), `components/rows.tsx` (settings rows) |
| Browser-only dev backend | `apps/desktop/src/dev/mockBackend.ts` (keep it in step with new commands) |

## Recipes

- **New engine command**: method on `Engine` → arm in `rpc::call` → function in `api.ts` →
  case in `dev/mockBackend.ts` → test in the engine module's `tests.rs`.
- **New config field**: field + default in `config.rs` (validate it if a user can type it) →
  `AppConfig` in `api.ts` → default in `mockBackend.ts`.
- **New alert**: `self.alert(Level::…, "code", detail)` in the engine → `alert.code` in both
  i18n files → if warning/error, its sentence in `notify::text` (it is also pushed outside
  the app). Device alerts: `controller/history.rs::check_alerts` + `fleet.ev.code` +
  `notify::device_text`.
- **Errors** returned to the UI are string codes translated as `err.<code>` in i18n.

## House style

- English code/comments/docs; short doc comments that say why. No new dependency unless
  essential. No speculative abstractions: one file per responsibility, not per type.
- Never put a wallet address, token or credential in the code; never build a shell
  command from user input.
