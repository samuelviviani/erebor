# Processo di rilascio

1. **XMRig**: `node scripts/update-xmrig-manifest.mjs <versione>`; verifica manualmente la firma GPG di `SHA256SUMS` upstream; rivedi il diff.
2. **Pool**: ricontrolla host, porte TLS e fee di `pools::CATALOG` (vedi `docs/POOLS.md`).
3. **Versione**: aggiorna `tauri.conf.json`, `Cargo.toml` (workspace), `package.json`, `CHANGELOG.md`.
4. **Secret**: `EREBOR_DEV_FEE_WALLET` (indirizzo pubblico del progetto, pubblicato anche nel README/sito per verificabilità).
5. **CI verde** su `main`.
6. **Actions › Release (manual)** con la versione → crea una **bozza** di release con installer e `SHA256SUMS`.
7. Prova gli installer su Windows e Ubuntu puliti (checklist in `PROGRESS.md`).
8. Aggiorna `apps/website/releases.json` con file, URL e SHA-256, poi pubblica la release e il sito.

## Aggiornamenti automatici (Tauri updater)

L'app installata legge `releases/latest/download/latest.json` all'avvio e ogni ora; se trova una versione nuova la scarica, verifica la firma, la installa e si riavvia. Se fallisce mostra un banner con il pulsante "Aggiorna".

- La chiave privata è in `~/.tauri/erebor.key` (generata senza password), la pubblica in `tauri.conf.json` (`plugins.updater.pubkey`). **Non perderla**: senza, le app già installate non accettano più aggiornamenti.
- Secret: `TAURI_SIGNING_PRIVATE_KEY` = contenuto di `~/.tauri/erebor.key`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = vuoto.
- L'aggiornamento parte quando la bozza viene **pubblicata** (non come pre-release): da lì `latest` punta alla nuova versione.
- Linux: si aggiorna da solo l'AppImage; chi ha installato il `.deb` aggiorna a mano.

## Firma del codice

- Windows: certificato Authenticode (OV/EV) via `bundle.windows.certificateThumbprint` o servizio di firma in CI.
- macOS (futuro): Developer ID + notarizzazione.
