# Changelog

Formato: [Keep a Changelog](https://keepachangelog.com/it/1.1.0/), versioni [SemVer](https://semver.org/lang/it/).

## [Unreleased]

### Added
- Setup guidato `erebor onboard` sul modello di OpenClaw (Quick start / Personalizzata / Demo, condizioni, wallet validato, installazione del miner, agent in background, controllo di salute, riepilogo; modalità `--non-interactive`) e installer `scripts/install.sh` che lo avvia dopo l'installazione. `erebor` senza argomenti apre il setup al primo avvio.
- CLI `erebor` (clap): `status`, `doctor`, `mining start|stop|pause|resume|restart` con verifica dello stato, `hardware`, `wallet`, `pools`, `agent`, `demo`, `consent`, `miner install`, `device`; output `--json` stabile e codici d'uscita documentati (docs/CLI.md).
- `erebor-agent`: possiede motore e miner, socket Unix locale (0600, UID del peer verificato da entrambe le parti, protocollo JSON v1), arresto pulito su SIGINT/SIGTERM, istanza unica, servizio systemd utente opzionale.
- TUI `erebor monitor` (ratatui/crossterm): metriche live, log, start/pausa/stop/riavvio; `Q` esce senza fermare il mining.
- Fondamenta Fleet: ID e nome del dispositivo, `Snapshot` e `DeviceReport` tipizzati e versionati (comandi `snapshot`, `device_info`, `set_device_name`).
- L'app desktop serve il socket locale quando possiede il motore e, se un agent è già attivo, si collega a lui invece di avviare un secondo motore.

### Changed
- Il mining reale richiede il consenso alle commissioni anche nel motore (`consent_required`): onboarding completato nella GUI oppure `erebor consent --accept`. Vale per tutti i frontend, la programmazione e `startOnLaunch`.
- Servizi di avvio (systemd utente/LaunchAgent): riavvio solo dopo un crash, non dopo un arresto pulito.
- `ARCHITECTURE.md` spostato in `docs/ARCHITECTURE.md`.

- Wallet per singola moneta (`coinWallets`): se impostato, quella moneta viene minata su Kryptex direttamente verso l'indirizzo dell'utente (`indirizzo/worker`, pagamento nella moneta stessa), con validazione dei formati pubblicati da unMineable.
- Interfaccia: tema blu, semaforo al passaggio del mouse, modalità con colori dedicati e indicatore animato, loghi ufficiali delle monete, monitoraggio GPU (NVIDIA/AMD), Impostazioni in finestra, Guadagni e Wallet ripensati per principianti.
- Pagamenti direttamente su un wallet Bitcoin tramite unMineable (indirizzi `bc1`/`1`/`3` validati con checksum BIP173/350 e base58check); scelta in onboarding tra Account Kryptex, Il mio wallet Bitcoin, indirizzo Monero; avviso "Bitcoin senza minare Bitcoin". Commissione verso `EREBOR_DEV_FEE_BTC` per questi utenti.
- Mining con scheda video (NVIDIA/AMD, Windows e Linux) tramite SRBMiner-MULTI 3.8.8 fissato con SHA-256: Pearl, Ethereum Classic, EthereumPoW, Ergo, Ravencoin, Xelis su Kryptex.
- Scelta della moneta: automatica (prova ogni moneta compatibile ~3 minuti, misura l'hashrate reale e mina la più redditizia, ricontrollando ogni 30 minuti con margine del 10%) o manuale dalla pagina Mining.
- Dati di mercato per moneta (WhatToMine, Kryptex, CoinGecko); valore dei guadagni tracciato in BTC al netto di fee della pool e del miner.

### Changed
- Onboarding in 3 passi (dove ricevi i guadagni → preparazione automatica del PC → pronto); pool, miner e prestazioni passano alle Impostazioni.
- Pool predefinita Kryptex: si mina con l'ID Kryptex (`krx…`, accredito in BTC sull'account) oppure con un indirizzo Monero.
- Commissione Erebor al 2% del tempo effettivo, a turni di 2 minuti, verso l'account Kryptex del progetto con un unico worker `erebor`. Niente più accettazione esplicita: la commissione è indicata nella schermata iniziale, in Impostazioni e sul sito.
- Nuovo logo e colore principale oro.

### Added
- Monorepo pnpm + Cargo: `erebor-core`, app Tauri 2, sito Next.js.
- Integrazione XMRig: config JSON, processo figlio, API loopback, pausa/ripresa, hot switch del pool, benchmark offline, installazione da manifest con SHA-256.
- DevFeeManager (1% del tempo effettivo, ledger persistito, consenso obbligatorio).
- Validazione indirizzi Monero (base58, Keccak, rete).
- Statistiche SQLite, stime economiche, costi energetici, programmazione, soglie termiche ed economiche, crash recovery.
- UI: onboarding, dashboard, mining, hardware, wallet, statistiche, impostazioni; italiano/inglese; tema scuro/chiaro; modalità demo.
- CI (lint, typecheck, test, build, audit) e workflow di release manuale (bozza).
