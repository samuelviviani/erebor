# Erebor Fleet — fondamenta

Obiettivo futuro: vedere e comandare più dispositivi Erebor. In questa fase esistono solo i **contratti** e l'identità locale; nessun server centrale, nessun controllo remoto nuovo.

## Cosa c'è

| Elemento | Dove | Note |
|---|---|---|
| ID dispositivo | `<cartella dati>/device.json` | 128 bit casuali, generati una volta; non derivati da seriali hardware o dall'utente |
| Nome dispositivo | `erebor device name <NOME>` | 1–64 caratteri, nessun carattere di controllo; default: hostname |
| Snapshot del miner | `fleet::Snapshot`, comando `snapshot`, `erebor status --json` | metriche tipizzate (schema 1), vedi [CLI.md](CLI.md#formato-json) |
| Report del dispositivo | `fleet::DeviceReport`, comando `device_info`, `erebor device show --json` | `{schema, device, hostname, agentVersion, hardware, snapshot}` |
| Versioni | `fleet::SCHEMA_VERSION` (dati), `ipc::PROTOCOL_VERSION` (trasporto locale) | aggiunta di campi opzionali senza bump; bump solo per rimozioni/cambi di significato |

## Trasporti

- **Locale**: socket Unix con verifica dell'UID ([AGENT.md](AGENT.md)).
- **Remoto (già esistente)**: API HTTP `POST /rpc/<comando>` con token casuale a 256 bit (`server.rs`), attiva solo se l'utente abilita l'accesso remoto; i comandi che scelgono file locali sono rifiutati (`local_only`). `snapshot` e `device_info` sono disponibili anche qui: è il punto d'aggancio per associare agent remoti.

## Vincoli di sicurezza (anche per le fasi successive)

- Nessun controllo remoto senza autenticazione; nessuna API amministrativa esposta di default.
- Nessuna esecuzione di comandi arbitrari: si inviano solo i comandi nominati di `rpc::call`, validati dal motore.
- Associazione di un agent remoto = scambio esplicito del token da parte dell'utente (come oggi "Aggiungi computer" nella GUI).

## Prossimi passi possibili

1. Separare `fleet` e i tipi del protocollo in un crate `erebor-protocol` quando esisterà un consumatore che non deve dipendere dal motore (es. un aggregatore).
2. Vista "Flotta" che interroga `device_info` dei computer già aggiunti nella GUI.
3. Trasporto cifrato (TLS con certificato fissato) prima di qualsiasi uso fuori dalla rete di casa/VPN.
