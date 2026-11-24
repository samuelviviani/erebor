# Architettura

## Processi e interfacce

```text
 erebor (CLI)   erebor monitor (TUI)        App desktop (Tauri/React)
      │                │                      │
      └──── socket Unix locale (UID verificato) ┘
                       │
        ┌──────────────▼──── un solo proprietario del motore ─────────────┐
        │ erebor-agent   oppure   app desktop (motore interno)            │
        │      └─ Arc<Mutex<Engine>>  (lock su <cartella dati>/engine.lock) │
        └──────────────────────────────┬──────────────────────────────────┘
                                       │ processi figli
                                 XMRig / SRBMiner  (o DemoMiner simulato)
```

| Crate | Ruolo |
|---|---|
| `crates/erebor-core` | Motore condiviso: miner, configurazione, wallet, pool, hashrate, statistiche, hardware, dev fee, avvio/arresto, errori. Più `rpc` (dispatcher unico dei comandi), `ipc` (socket locale + controllo verificato), `fleet` (identità e snapshot versionati) |
| `crates/erebor-agent` | Lib + binario `erebor-agent`: possiede il motore, serve il socket, gestisce segnali, arresto pulito |
| `crates/erebor-cli` | Binario `erebor` (clap); include l'agent (`agent start --foreground`) e la TUI |
| `crates/erebor-tui` | Dashboard ratatui/crossterm, `fmt` condiviso con la CLI |
| `apps/desktop` | App Tauri; serve il socket quando possiede il motore, altrimenti si collega all'agent |

Scelte:

- **Una sola fonte di stato**: il motore vive in un solo processo per cartella dati (lock). Chi lo possiede serve il socket; tutti gli altri lo comandano con gli stessi comandi di `rpc::call`. Nessuna configurazione divergente: le modifiche passano da `update_config` del motore.
- **Agent proprietario del ciclo di vita** quando è attivo: l'app desktop aperta dopo l'agent non crea un secondo motore e non avvia un secondo miner.
- **Protocollo nel core, non in un crate separato** (`erebor-protocol` del piano): oggi ogni consumatore dipende già dal core; si separerà quando un consumatore non dovrà dipendere dal motore (vedi [FLEET.md](FLEET.md)).
- **Dev fee nel motore**: il `DevFeeManager` è unico; `can_start` rifiuta il mining reale senza wallet o miner. Nessun frontend duplica queste regole.
- **Controllo verificato** (`ipc::control`): CLI e TUI riportano successo solo se lo stato del motore è davvero cambiato.

Documenti: [CLI](CLI.md) · [AGENT](AGENT.md) · [TUI](TUI.md) · [FLEET](FLEET.md)

## Motore e app desktop (dettaglio)

```
┌──────────────────────── apps/desktop ────────────────────────┐
│ React UI (Vite, Tailwind, Zustand, Recharts)                  │
│   └─ src/lib/api.ts  ── invoke() tipizzato ──┐                │
│ src-tauri/src/main.rs (comandi Tauri)  <─────┘                │
│   └─ Arc<Mutex<Engine>>  +  thread: tick 2s, mercato 10 min   │
└───────────────────────────────┬──────────────────────────────┘
                                │
┌──────────────────── crates/erebor-core ──────────────────────┐
│ engine   ciclo di vita, sessioni, sample 10s, sicurezza, job  │
│ miner    trait MinerAdapter + DemoMiner                       │
│ xmrig    config JSON, processo figlio, API HTTP su 127.0.0.1  │
│ devfee   DevFeeManager (macchina a stati, ledger persistito)  │
│ earnings stime: lordo → donazione → fee pool → netto → costi  │
│ pools    catalogo, raggiungibilità, API nodejs-pool, prezzo   │
│ hardware sysinfo + adapter per piattaforma (GPU, RAPL)        │
│ install  manifest fissato, download, SHA-256, estrazione      │
│ store    SQLite (rusqlite), migrazioni con user_version       │
│ wallet   validazione indirizzi Monero (base58 + Keccak)       │
│ config   configurazione JSON, validazione, scrittura atomica  │
└───────────────────────────────────────────────────────────────┘
```

### Scelte principali

- **Un solo crate core** al posto dei crate separati suggeriti (`mining-core`, `hardware`, `earnings`, `dev-fee`, `pools`): i moduli sono piccoli e fortemente collegati; un crate li testa senza Tauri. Si possono separare quando un secondo consumatore lo giustifica.
- **Niente `packages/*` condivisi**: l'app e il sito non condividono componenti né tipi. I tipi TS in `apps/desktop/src/lib/api.ts` rispecchiano le struct serde (camelCase).
- **XMRig come processo figlio** (non sidecar): il binario viene installato a runtime da un manifest fissato con hash, così i pacchetti di Erebor non ridistribuiscono codice GPL e l'utente può selezionare un binario proprio (marcato "non verificato").
- **Avvio senza shell**: `Command::new(binary).arg("--config=<file>")`. Tutti i valori utente sono validati (`AppConfig::validate`) prima di finire nel JSON di XMRig.
- **API XMRig** su `127.0.0.1`, porta libera casuale, token casuale per sessione; file di config con permessi 0600.
- **Stop affidabile**: l'arresto uccide il processo figlio senza dipendere dall'API. Il PID è salvato su file: se Erebor crasha, all'avvio successivo un eventuale XMRig orfano viene terminato.
- **Polling**: la UI interroga `get_status` ogni 2 s. Più semplice degli eventi e sufficiente.

### Dev fee

`DevFeeManager::tick(effective_secs)` accumula solo il tempo di mining effettivo (in esecuzione, non in pausa, hashrate > 0, gap > 10 s scartati). Quando il debito `rate × totale − progetto` raggiunge 60 s passa al target `Project`; torna a `User` quando il debito è saldato. Con 1%: ~1 minuto ogni ~100 minuti. Il passaggio usa `PUT /1/config` di XMRig (hot reload, senza rigenerare il dataset RandomX); se fallisce, il miner viene riavviato sul target giusto. Il ledger è salvato in SQLite: i riavvii non azzerano né raddoppiano la commissione. Disattivata se manca il consenso o se la build non ha `EREBOR_DEV_FEE_WALLET` (validato come indirizzo mainnet).

### Stime economiche

Ogni sample (10 s) salva hashrate medio, target, share, `xmr_per_hash` (= ricompensa blocco / difficoltà al momento), potenza **misurata** (se disponibile) e temperatura. Le potenze stimate dall'utente sono applicate in fase di query, così modificarle non riscrive lo storico. Il lordo include solo i sample `user`; la quota Erebor è riportata a parte e non sottratta due volte.

### Demo vs produzione

La modalità demo usa `DemoMiner` (nessun processo, nessuna rete), dati di mercato fissi marcati `demo` e un database separato (`erebor-demo.db`).

### Dati locali

Cartella dati dell'app (Tauri `app_data_dir`): `config.json`, `erebor.db`, `erebor-demo.db`, `miners/xmrig-<versione>/`, `run/xmrig-config.json`, `miner.pid`.
