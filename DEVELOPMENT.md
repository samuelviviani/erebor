# Sviluppo

## Requisiti

- Node.js ≥ 20.9, pnpm 10
- Rust stable (via [rustup](https://rustup.rs)), con `rustfmt` e `clippy`
- Dipendenze di sistema Tauri 2:
  - **Linux (Ubuntu)**: Ubuntu 22.04+: `sudo apt install build-essential curl wget file libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
  - **Windows**: Microsoft C++ Build Tools, WebView2 (preinstallato su Windows 11)
  - **macOS**: Xcode Command Line Tools

## Comandi

| Comando | Cosa fa |
|---|---|
| `pnpm install` | dipendenze JS |
| `pnpm --filter @erebor/desktop tauri dev` | app desktop con hot reload |
| `pnpm --filter @erebor/desktop dev` | solo UI su http://localhost:1420 con backend simulato (`src/dev/mockBackend.ts`, solo in dev) |
| `pnpm --filter @erebor/desktop tauri build` | installer per la piattaforma corrente |
| `pnpm --filter @erebor/website dev` | sito su http://localhost:3000 |
| `pnpm -r typecheck` / `pnpm -r test` / `pnpm -r build` | controlli JS |
| `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings` | lint Rust |
| `cargo test --workspace` | test Rust (nessun mining reale: i processi sono simulati) |

## Variabili di build

| Variabile | Uso |
|---|---|
| `EREBOR_DEV_FEE_WALLET` | indirizzo pubblico Monero del progetto (compile-time). Assente o non valido ⇒ nessuna commissione. **Non** inserire indirizzi nel codice. |
| `EREBOR_REPO_URL` | URL repository mostrato nelle Informazioni |
| `NEXT_PUBLIC_SITE_URL`, `NEXT_PUBLIC_REPO_URL` | sito (vedi `apps/website/.env.example`) |

## Fissare una versione di XMRig

```bash
node scripts/update-xmrig-manifest.mjs 6.22.2   # scarica SHA256SUMS + archivi, verifica, scrive il manifest
cargo test -p erebor-core install               # il manifest viene validato dai test
```

Il manifest è incluso nel binario (`include_str!`). Finché non viene fissata una versione, l'installazione automatica è disattivata e l'utente può solo selezionare un binario esistente.

## Test e mining reale

I test automatici non avviano mai XMRig né contattano pool: `engine` usa un `MockMiner`, il frontend usa `mockIPC` di Tauri. Un test manuale con mining reale richiede: manifest fissato, wallet di test personale, consenso esplicito; documentarne l'esito in `PROGRESS.md`.

## Convenzioni

- Errori dei comandi: codici stringa (`fees_not_accepted`, …) tradotti dalla UI in `src/lib/i18n.ts` (`err.*`).
- Nuovo testo UI: aggiungere la chiave sia in `it` che in `en` (il tipo `Record<Key, string>` e un test lo impongono).
- Tipi condivisi: modificare insieme la struct Rust e l'interfaccia in `apps/desktop/src/lib/api.ts`.
