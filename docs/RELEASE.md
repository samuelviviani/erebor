# Processo di rilascio

1. **XMRig**: `node scripts/update-xmrig-manifest.mjs <versione>`; verifica manualmente la firma GPG di `SHA256SUMS` upstream; rivedi il diff.
2. **Pool**: ricontrolla host, porte TLS e fee di `pools::CATALOG` (vedi `docs/POOLS.md`).
3. **Versione**: aggiorna `tauri.conf.json`, `Cargo.toml` (workspace), `package.json`, `CHANGELOG.md`.
4. **Secret**: `EREBOR_DEV_FEE_WALLET` (indirizzo pubblico del progetto, pubblicato anche nel README/sito per verificabilità).
5. **CI verde** su `main`.
6. **Actions › Release (manual)** con la versione → crea una **bozza** di release con installer e `SHA256SUMS`.
7. Prova gli installer su Windows e Ubuntu puliti (checklist in `PROGRESS.md`).
8. Aggiorna `apps/website/releases.json` con file, URL e SHA-256, poi pubblica la release e il sito.

## Aggiornamenti firmati (Tauri updater) — da attivare

Non ancora attivo: richiede una chiave che non deve stare nel repository.

```bash
pnpm --filter @erebor/desktop tauri signer generate -w ~/.tauri/erebor.key
```

- Salva la chiave privata e la password come secret `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Aggiungi `tauri-plugin-updater`, la chiave **pubblica** e l'endpoint (`latest.json` delle release GitHub) in `tauri.conf.json` (`plugins.updater`), `bundle.createUpdaterArtifacts: true`, permesso `updater:default`.
- Collega il toggle "Controlla aggiornamenti" (oggi disattivato) e chiedi conferma prima di installare.

## Firma del codice

- Windows: certificato Authenticode (OV/EV) via `bundle.windows.certificateThumbprint` o servizio di firma in CI.
- macOS (futuro): Developer ID + notarizzazione.
