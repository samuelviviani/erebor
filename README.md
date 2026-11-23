# Erebor

**Mining, under your control.**

Erebor è un mining manager desktop open source, non-custodial e local-first. Gestisce il mining CPU di Monero tramite [XMRig](https://github.com/xmrig/xmrig), mostra hashrate reale, share, ricompense stimate, consumi e commissioni, e non tocca mai i tuoi fondi: la pool paga direttamente il tuo wallet.

> Stato: **pre-release (0.1.0)**. Non pronto per la produzione — vedi [PROGRESS.md](PROGRESS.md).

## Principi

- **Non-custodial** — solo indirizzi pubblici; mai seed phrase o chiavi private.
- **Trasparente** — commissione Erebor (1% del tempo di mining effettivo), donazione XMRig (1%) e fee della pool mostrate e accettate prima del primo avvio.
- **Locale** — configurazione e statistiche solo sul tuo computer; nessun account, nessuna telemetria.
- **Nessun mining nascosto** — il mining parte solo con un'azione esplicita e si arresta quando chiudi l'app.

## Struttura

| Percorso | Contenuto |
|---|---|
| `crates/erebor-core` | Libreria Rust: XMRig, dev fee, guadagni, pool, hardware, SQLite, orchestrazione, IPC locale, Fleet |
| `crates/erebor-agent` | Agent: possiede il motore e i miner, socket locale, servizio systemd utente opzionale |
| `crates/erebor-cli` | CLI `erebor` (status, doctor, mining, wallet, pool, hardware, agent) |
| `crates/erebor-tui` | Dashboard da terminale (`erebor monitor`) |
| `apps/desktop` | App Tauri 2 (React + TypeScript + Tailwind); `src-tauri` = comandi tipizzati |
| `apps/website` | Sito pubblico Next.js (export statico, compatibile Vercel) |
| `scripts/` | `update-xmrig-manifest.mjs`: fissa una release XMRig con hash verificati |
| `docs/` | Rilascio, pool, checklist legale |

Dettagli in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Avvio rapido

```bash
pnpm install
pnpm --filter @erebor/desktop tauri dev   # app desktop (richiede Rust + dipendenze Tauri)
pnpm --filter @erebor/desktop dev         # solo UI nel browser, backend simulato (demo)
pnpm --filter @erebor/website dev         # sito
pnpm test && cargo test --workspace       # test
```

Vedi [DEVELOPMENT.md](DEVELOPMENT.md) e [INSTALLATION.md](INSTALLATION.md).

## Da terminale (Ubuntu/Linux, macOS)

```bash
sh scripts/install.sh         # installa in ~/.local/bin e avvia il setup guidato (erebor onboard)

erebor doctor                 # diagnostica dell'ambiente
erebor agent start            # agent in background (non avvia il mining)
erebor demo on                # miner simulato, nessun mining reale
erebor mining start && erebor status
erebor monitor                # TUI: S/P/X/R, Q esce lasciando il mining attivo
erebor agent stop
```

CLI, TUI e app desktop comandano lo stesso motore: un solo miner, un solo stato. Il mining reale richiede `erebor consent --accept`, un wallet (`erebor wallet set`) e il miner (`erebor miner install`). Guide: [CLI](docs/CLI.md) · [Agent](docs/AGENT.md) · [TUI](docs/TUI.md) · [Fleet](docs/FLEET.md).

## Documenti

[ARCHITECTURE](docs/ARCHITECTURE.md) · [CLI](docs/CLI.md) · [AGENT](docs/AGENT.md) · [TUI](docs/TUI.md) · [FLEET](docs/FLEET.md) · [DEVELOPMENT](DEVELOPMENT.md) · [INSTALLATION](INSTALLATION.md) · [PRIVACY](PRIVACY.md) · [SECURITY](SECURITY.md) · [CONTRIBUTING](CONTRIBUTING.md) · [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md) · [PROGRESS](PROGRESS.md) · [CHANGELOG](CHANGELOG.md)

## Licenza

MIT ([LICENSE](LICENSE)). XMRig è un progetto separato, GPLv3, non incluso nei pacchetti: viene scaricato dalla release ufficiale e verificato con SHA-256.

Il mining comporta costi energetici e non garantisce alcun rendimento.
