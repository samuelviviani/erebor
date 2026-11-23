# Erebor CLI (`erebor`)

Interfaccia da terminale di Erebor. Legge e comanda lo **stesso motore** dell'app desktop: ogni comando che riguarda il mining passa dal processo che possiede il motore (Agent, app desktop o `erebor-node`) tramite il socket locale. CLI, TUI e GUI vedono quindi sempre lo stesso stato.

## In breve

```bash
erebor               # dashboard live (al primo avvio: setup guidato)
erebor start         # avvia il mining; Erebor resta attivo in background anche chiudendo il terminale
erebor stop          # ferma il mining
erebor status        # riepilogo
erebor service on    # mina sempre: Erebor e il mining partono con il computer (off per disattivare)
erebor update        # installa l'ultima versione firmata (lo fa anche da solo, vedi sotto)
```

Non serve avviare l'agent a mano: i comandi che ne hanno bisogno (`start`, dashboard, `wallet set`, `demo`, …) lo avviano da soli. Quelli di sola lettura (`status`, `doctor`, elenchi) non avviano nulla.

### Mina sempre, anche dopo un riavvio

`erebor service on` registra Erebor come servizio utente (systemd `--user` su Linux, LaunchAgent su macOS, senza root), imposta `startOnLaunch` e riavvia l'agent sotto il servizio: dopo ogni riavvio del computer il mining riparte da solo. Su Linux attiva anche `loginctl enable-linger` per partire al boot senza login; se il sistema lo nega, la CLI indica il comando con `sudo`. Su macOS parte al login dell'utente. `erebor service` mostra lo stato, `erebor service off` lo disattiva (e ferma il mining in background). Il mining reale resta soggetto a consenso e configurazione valida.

### Aggiornamenti automatici

La CLI installata dai binari di release (o compilata in release con `scripts/install.sh`) si aggiorna da sola: l'agent controlla ogni ora le release GitHub, scarica il binario per il tuo sistema e lo installa **solo se la firma minisign corrisponde alla chiave del progetto** (la stessa dell'app desktop). Poi si riavvia sulla nuova versione mantenendo lo stesso processo (anche sotto systemd/launchd) e, se stava minando, riprende. `erebor update` fa lo stesso a mano. Le build di debug e il binario `erebor-agent` non si aggiornano da soli. Se la cartella del programma non è scrivibile (es. installato con `sudo` in `/usr/local/bin`), l'aggiornamento fallisce senza toccare nulla.

## Compilazione su Ubuntu

```bash
# Toolchain (una volta)
sudo apt update && sudo apt install -y build-essential curl git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Dalla radice del repository: solo CLI + Agent (non servono le dipendenze Tauri)
cargo build --release -p erebor-cli -p erebor-agent
export PATH="$PWD/target/release:$PATH"     # oppure: sudo install target/release/erebor /usr/local/bin/

# Test automatici (miner simulati, nessun mining reale)
cargo test -p erebor-core -p erebor-agent -p erebor-tui -p erebor-cli
```

`cargo test --workspace` compila anche l'app desktop e richiede le dipendenze Tauri (vedi [DEVELOPMENT.md](../DEVELOPMENT.md)).

## Installazione e setup guidato (consigliato)

Come OpenClaw: un comando installa e avvia subito la procedura guidata.

```bash
sh scripts/install.sh                 # dal checkout del repository
# oppure, senza clonare a mano:
curl -fsSL https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install.sh \
  | sh -s -- --repo https://github.com/samuelviviani/erebor.git
```

L'installer controlla sistema e compilatore C (su Ubuntu suggerisce `sudo apt install -y build-essential`), installa Rust con rustup se manca (chiedendo prima), compila `erebor` ed `erebor-agent` in release, li copia in `~/.local/bin` (senza root), indica come aggiungerlo al `PATH` e lancia `erebor onboard`. Opzioni: `--no-onboard`, `--prefix DIR`, `--repo URL`, `--ref REF`, `--yes`.

### `erebor onboard`

Parte da sola anche lanciando `erebor` senza argomenti la prima volta. Passi:

1. **Setup esistente**: se Erebor è già configurato, propone *Mantieni* / *Modifica* / *Ricomincia dai valori predefiniti* (le statistiche restano).
2. **Modalità**: *Quick start* (consigliata: wallet, miner ufficiale, valori sicuri), *Personalizzata* (anche pool, prestazioni, nome worker e dispositivo, temperatura massima, riavvio automatico, GPU, avvio al login), *Demo* (miner simulato).
3. **Condizioni**: dev fee, donazione XMRig, fee della pool, modello non-custodial, consumi. Rifiutare annulla senza modificare nulla.
4. **Wallet**: indirizzo pubblico (Monero, ID Kryptex o Bitcoin), validato subito; le seed phrase vengono riconosciute e rifiutate.
5. **Miner**: offre di installare XMRig fissato e verificato (SRBMiner solo in Personalizzata, se la GPU è supportata).
6. **Agent**: tenerlo attivo in background (predefinito sì), avvio al login (predefinito no), avviare il mining subito (predefinito **no**).
7. **Applicazione e controllo**: avvia l'agent, salva la configurazione tramite il motore, installa il miner, esegue il controllo di salute (come `doctor`), poi riepilogo e prossimi passi.

Tutte le risposte vengono raccolte prima: interrompere la procedura (Ctrl-C) non cambia nulla.

Senza prompt (script, provisioning):

```bash
erebor onboard --non-interactive --accept-risk --wallet <INDIRIZZO> [--pool ID] \
  [--performance eco|balanced|performance] [--no-install-miner] [--install-service] \
  [--start] [--no-background] [--skip-health] [--reset]
erebor onboard --non-interactive --mode demo --start
```

Senza `--accept-risk` il mining reale viene rifiutato (`accept_risk_required`).

## Prova rapida (modalità demo)

```bash
erebor doctor            # ambiente, hardware, miner, wallet, pool, agent
erebor agent start       # agent in background (log: <cartella dati>/agent.log)
erebor demo on           # miner simulato: nessun uso di CPU/GPU, nessuna rete
erebor mining start
erebor status
erebor monitor           # TUI; Q esce lasciando il mining attivo
erebor mining stop
erebor agent stop        # ferma mining e agent
```

## Mining reale

Parte solo con configurazione valida **e** consenso esplicito:

```bash
erebor demo off
erebor consent                   # legge commissioni e condizioni
erebor consent --accept          # (equivale all'onboarding della GUI)
erebor wallet set <INDIRIZZO>    # indirizzo pubblico Monero, ID Kryptex (krx…) o indirizzo BTC
erebor miner install xmrig       # release fissata, SHA-256 verificato
erebor pools test
erebor mining start
```

Senza consenso il motore rifiuta l'avvio (`consent_required`), anche se richiesto dalla GUI remota, dalla programmazione o da `startOnLaunch`.

## Cartella dati

Condivisa con l'app desktop: `~/.local/share/org.erebor.desktop` (Linux), `~/Library/Application Support/org.erebor.desktop` (macOS), `%APPDATA%\org.erebor.desktop` (Windows). Si cambia con `--data-dir DIR` o `EREBOR_DATA_DIR=DIR` (utile per prove isolate).

## Comandi

| Comando | Cosa fa | `--json` |
|---|---|---|
| `erebor onboard` | Setup guidato (vedi sopra); `erebor` senza argomenti lo apre al primo avvio | ✓ |
| `erebor status` | Stato, miner, moneta/algoritmo, pool, hashrate, uptime, share, temperatura, consumo, ricompense stimate, dev fee | ✓ |
| `erebor doctor [--offline]` | OS, CPU/GPU, sensori, utente/permessi, cartella dati, agent, configurazione, consenso, wallet, miner, connettività pool | ✓ |
| `erebor mining start\|stop\|pause\|resume\|restart` | Comandi verificati: riporta successo solo se lo stato è davvero cambiato | ✓ |
| `erebor hardware list` | CPU e GPU | ✓ |
| `erebor hardware info` | Dettagli, sensori live, capacità (build miner disponibili, sensori) | ✓ |
| `erebor wallet show` | Wallet di pagamento e wallet per moneta | ✓ |
| `erebor wallet set <ADDR> [--coin ID]` | Imposta e valida l'indirizzo (mai seed phrase o chiavi private) | ✓ |
| `erebor pools list` | Catalogo pool (`*` = configurata) | ✓ |
| `erebor pools test [--all]` | Compatibilità con il wallet e raggiungibilità (latenza TCP) | ✓ |
| `erebor pools set <POOL_ID>` | Usa una pool del catalogo | ✓ |
| `erebor monitor` | Apre la TUI ([TUI.md](TUI.md)) | |
| `erebor agent start [--foreground]` | Avvia l'agent ([AGENT.md](AGENT.md)) | ✓ |
| `erebor agent stop\|status` | Ferma (con il mining) / stato dell'agent | ✓ |
| `erebor agent install-service\|uninstall-service` | Avvio al login, opzionale, disattivato di default | |
| `erebor demo on\|off` | Modalità demo esplicita | ✓ |
| `erebor consent [--accept\|--revoke]` | Commissioni e condizioni; `--revoke` ferma il mining reale | ✓ |
| `erebor miner install [xmrig\|srbminer]` | Scarica e verifica il miner fissato | ✓ |
| `erebor device show` / `device name <NOME>` | Identità del dispositivo (Fleet) | ✓ |

`pause`/`resume` funzionano solo se il miner lo supporta (`pause_unsupported` altrimenti). `--json` e `--data-dir` sono opzioni globali: valgono prima o dopo il sottocomando.

## Codici d'uscita

| Codice | Significato |
|---|---|
| 0 | Riuscito (anche "nulla da fare": es. `mining start` con mining già attivo, `changed: false`) |
| 1 | Comando fallito (errore del motore, validazione, `doctor` con problemi bloccanti) |
| 2 | Uso errato (argomenti non validi) |
| 3 | Agent non raggiungibile |

## Formato JSON

Stabile e versionato. In caso di errore, con `--json`, su stdout:

```json
{"error": "consent_required", "message": "…", "hint": "erebor consent --accept"}
```

`erebor status --json` restituisce lo **Snapshot** (schema 1, definito in `crates/erebor-core/src/fleet.rs`):

```jsonc
{
  "schema": 1,
  "state": "running",            // stopped | running | paused | error
  "demo": false,
  "error": null,
  "startBlocker": null,          // perché non può partire (codice), se applicabile
  "miner": {"id": "xmrig", "name": "XMRig", "version": "6.26.0", "supportsPause": true},
  "coin": "XMR", "coinName": "Monero", "algo": "rx/0", "autoCoin": true,
  "poolId": "kryptex",
  "pool": "xmr.kryptex.network:7029",
  "poolConnected": true,
  "hashrate": 4250.0,            // H/s, null se non disponibile
  "uptimeSecs": 3723,
  "accepted": 148, "rejected": 2,
  "cpuTempC": 64.0,              // null se il sistema non lo espone
  "gpus": [{"name": "…", "tempC": null, "powerW": null, "utilPercent": null}],
  "powerW": null,
  "powerSource": "unavailable",  // measured | estimated | unavailable
  "devFee": {"enabled": true, "percent": 2.0, "active": "user"},
  "earnings": {"valuePerDay": 0.82, "currency": "EUR", "btcPerDay": 0.0000141, "live": true},
  "restartInSecs": null,
  "job": null                    // installazione/benchmark in corso
}
```

Regole di compatibilità: nuovi campi possono essere aggiunti senza cambiare `schema`; `schema` aumenta solo se un campo viene rimosso o cambia significato. I valori non disponibili sono `null`, mai inventati.

`mining … --json` → `{"changed": bool, "snapshot": Snapshot}`; `doctor --json` → `{"ok": bool, "checks": [{"id","status":"ok|warn|fail","message","hint"}]}`.

## Risoluzione dei problemi

| Messaggio / codice | Soluzione |
|---|---|
| `agent_unreachable` (exit 3) | `erebor agent start`; se fallisce, leggi `<cartella dati>/agent.log` |
| `engine_busy` | Un altro Erebor (app desktop, nodo o agent) possiede già la cartella dati: la CLI lo comanda direttamente, non serve un secondo agent |
| `consent_required` | `erebor consent`, poi `erebor consent --accept` |
| `wallet_missing` | `erebor wallet set <INDIRIZZO>` oppure prova prima con `erebor demo on` |
| `miner_not_configured` / `miner_binary_changed` | `erebor miner install xmrig` |
| `protocol_version` | CLI e agent di versioni diverse: `erebor agent stop && erebor agent start` |
| `not_agent` su `agent stop` | Il motore è dell'app desktop: chiudi l'app (il mining si ferma con lei) |
| Temperatura/consumo `n/a` | Il sistema non espone il sensore: su Ubuntu `sudo apt install lm-sensors`; i contatori RAPL sono spesso leggibili solo da root (Erebor non chiede root) |
| Pool non raggiungibile | Firewall/rete; prova `erebor pools test --all` e `erebor pools set <id>` |
