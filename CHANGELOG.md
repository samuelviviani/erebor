# Changelog

Formato: [Keep a Changelog](https://keepachangelog.com/it/1.1.0/), versioni [SemVer](https://semver.org/lang/it/).

## [Unreleased]

### Added
- CLI: `erebor config` (ogni impostazione: `get`, `set`), `erebor coins` (monete con hashrate e guadagno stimato, `coins set`), `erebor logs [-f]`. Nessun comando nascosto in `erebor --help`.
- Installazione con un comando: `curl -fsSL …/scripts/install.sh | sh` scarica il binario della release e ne verifica lo SHA-256 (non serve più Rust; `--from-source` per compilare).
- Snapshot: `earnings.energyCostPerDay` e `earnings.netPerDay`.

### Changed
- Dashboard da terminale: il logo compare solo all'avvio; schermata compatta con mining, guadagno netto, tabella dispositivi (temperatura, consumo, carico) e log.
- Setup da terminale: tutte le opzioni in sezioni (moneta, thread CPU, worker, prezzo energia, sicurezza, miner, avvio), riepilogo e conferma finale.

## [0.1.5] - 2026-11-24

### Fixed
- Commissione Erebor: il tempo minato con una versione senza commissione attiva non viene più addebitato dopo l'aggiornamento (prima causava un lungo blocco di mining per il progetto appena aggiornati).

## [0.1.4] - 2026-11-24

### Fixed
- Windows: se l'antivirus blocca il miner durante l'installazione, l'app ora lo dice e spiega come consentirlo, invece di mostrare "Accesso negato (os error 5)".
- Setup iniziale bloccato dopo un'installazione del miner fallita: ora c'è il pulsante "Riprova".

## [0.1.3] - 2026-11-24

### Added
- Nuovi miner verificati con SHA-256: lolMiner (GPU: Alephium, Conflux, Nexa, Kaspa, Tari) e cpuminer-opt (CPU, solo Windows: SHA-256 e Scrypt). Tutte le monete del pool Kryptex minabili da un computer, con icone.
- I miner si aggiornano da soli all'ultima versione verificata (disattivabile).

### Changed
- Inglese come lingua predefinita; README riscritto.
- Pagamenti solo tramite account Kryptex (altre opzioni in arrivo).

### Removed
- Accesso remoto e `erebor-node`: Erebor lavora solo sul computer locale.

## [0.1.2] - 2026-11-23

### Added
- `erebor power eco|balanced|max`: quanta CPU usa il mining; se sta minando, il miner riparte da solo con il nuovo livello.
- Output del terminale a colori (help, stato, doctor, setup); i colori si disattivano da soli fuori da un terminale o con `NO_COLOR`. Help riordinato: comandi principali in alto, gli altri sotto "More commands".
- Aggiornamento automatico: l'app desktop (Tauri updater, banner con pulsante "Aggiorna" se fallisce) e la CLI (`erebor update`, e ogni ora dall'agent, con riavvio sulla nuova versione e ripresa del mining). Binari CLI firmati nelle release.
- `erebor service on|off`: mina sempre, anche dopo un riavvio (servizio utente, `startOnLaunch`, linger su Linux); proposto anche nel setup guidato.
- Dashboard da terminale ridisegnata: logo ASCII (splash all'apertura e intestazione nei terminali alti), guadagni al giorno e al mese, temperature a barre, tasti contestuali, errori spiegati con il comando per risolverli.
- CLI più semplice: `erebor` apre la dashboard, `erebor start` / `erebor stop`, l'agent parte da solo quando serve, help con esempi.

### Changed
- Niente più accettazione delle commissioni nel software (motore, `erebor consent`, setup, schermata iniziale dell'app): commissioni e condizioni sono descritte sul sito e nel README. Stato e dashboard continuano a mostrare la dev fee in tempo reale.

### Fixed
- GPU NVIDIA/AMD mai usata dalla CLI: `gpu_mining_supported` era sempre falso, quindi il setup non installava SRBMiner. Ora il setup lo installa quando c'è una GPU supportata e `erebor doctor` segnala la GPU non usata.

## [0.1.0] - 2026-11-23

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
