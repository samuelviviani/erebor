# Componenti di terze parti

Elenco delle dipendenze dirette e delle relative licenze (dichiarate dai pacchetti). Prima di ogni release generare l'elenco completo, incluse le dipendenze transitive, ad es. con `cargo about` / `cargo deny` e `pnpm licenses list --prod`, e verificarlo.

## XMRig (non incluso)

- Progetto: https://github.com/xmrig/xmrig — **GPL-3.0**
- Erebor **non ridistribuisce** XMRig: lo scarica dalle release ufficiali su richiesta dell'utente e lo esegue come processo separato, comunicando via file di configurazione e API HTTP locale. Erebor non è un'opera derivata e mantiene licenza MIT.
- **Se in futuro XMRig venisse incluso nei pacchetti** (sidecar), si applicherebbero gli obblighi GPLv3 per quella componente: includere licenza e copyright, fornire il codice sorgente corrispondente (o un'offerta scritta), indicare eventuali modifiche. Erebor non modifica il codice di XMRig.
- Donazione: XMRig dona di default l'1% del tempo ai propri sviluppatori (`donate-level`, minimo 1 nelle build ufficiali, `src/donate.h`). Erebor la mantiene e la dichiara.

## SRBMiner-MULTI (non incluso)

- Progetto: https://github.com/doktor83/SRBMiner-Multi — **software proprietario gratuito** (closed source), © doktor83.
- Erebor **non ridistribuisce** SRBMiner: su richiesta dell'utente scarica la release ufficiale da GitHub (archivio verificato: MD5 pubblicato dall'autore + SHA-256 fissato in `srbminer-manifest.json`), estrae solo l'eseguibile e lo avvia **senza modificarlo**, come processo separato con argomenti fissi.
- La licenza vieta estrazione, modifica, reverse engineering e ridistribuzione dei binari e "l'incorporazione in un altro prodotto" senza permesso scritto. Erebor non incorpora né modifica il binario, ma **prima di una release pubblica conviene chiedere conferma scritta all'autore** che questo uso sia consentito (vedi `docs/LEGAL-CHECKLIST.md`).
- Commissione dello sviluppatore integrata, variabile per algoritmo (es. pearlhash 2%, etchash 0,65%, kawpow 0,85%): Erebor la include nelle stime e la indica nell'app.
- L'API statistiche di SRBMiner (`--api-enable`) è in sola lettura; Erebor non abilita gli URL di riavvio/spegnimento remoto.

## Loghi delle monete

`apps/desktop/public/coins/*.png`: loghi di Monero, Pearl, Ethereum Classic, EthereumPoW, Ergo, Ravencoin, Xelis e Bitcoin, scaricati da CoinGecko (coin-images.coingecko.com) il 2026-11-16 e ridimensionati (Ergo e Pearl ritagliati in cerchio). Sono marchi dei rispettivi progetti, usati solo per identificare le monete; non implicano affiliazione o approvazione.

## Servizi di mercato

- WhatToMine (`coins.json`): difficoltà/ricompense di rete e prezzi in BTC delle monete GPU.
- Kryptex (`pool.kryptex.com/<coin>/api/v1/pool/stats`): fee della pool per moneta e dati di rete di XMR.

## Rust (crates/erebor-core, apps/desktop/src-tauri)

| Crate | Licenza |
|---|---|
| tauri, tauri-build, tauri-plugin-dialog | MIT OR Apache-2.0 |
| serde, serde_json | MIT OR Apache-2.0 |
| rusqlite (con SQLite bundled, public domain) | MIT |
| sysinfo | MIT |
| ureq (rustls, webpki-roots) | MIT OR Apache-2.0 (webpki-roots: MPL-2.0 / CDLA) |
| sha2, tiny-keccak (CC0-1.0) | MIT OR Apache-2.0 / CC0 |
| flate2, tar | MIT OR Apache-2.0 |
| zip | MIT |
| rand, chrono | MIT OR Apache-2.0 |

## JavaScript

| Pacchetto | Licenza |
|---|---|
| react, react-dom | MIT |
| @tauri-apps/api, @tauri-apps/plugin-dialog, @tauri-apps/cli | MIT OR Apache-2.0 |
| lucide-react | ISC |
| recharts | MIT |
| zustand | MIT |
| next | MIT |
| tailwindcss, vite, vitest, typescript | MIT / Apache-2.0 (typescript) |

## Servizi

- CoinGecko API (prezzi): soggetta ai termini d'uso di CoinGecko; verificarne i limiti per l'uso pubblico.
- API pubblica di SupportXMR: soggetta ai termini della pool.
