# Erebor Agent

L'Agent è il processo leggero che **possiede il motore di mining** (`erebor_core::engine::Engine`) e quindi i processi figli dei miner. CLI, TUI e app desktop lo comandano; lui è l'unica fonte autorevole dello stato.

## Avvio e arresto

```bash
erebor agent start               # background, staccato dal terminale (nuova sessione), log in <cartella dati>/agent.log
erebor agent start --foreground  # in questo terminale (sviluppo); Ctrl-C = arresto pulito
erebor agent status              # owner, pid, versione, protocollo, uptime, socket
erebor agent stop                # ferma il mining, poi l'agent
erebor-agent [--data-dir DIR]    # binario dedicato, equivalente a "agent start --foreground"
```

- **Mai mining automatico**: all'avvio l'agent non mina, salvo `startOnLaunch: true` impostato esplicitamente in configurazione; il mining reale richiede comunque consenso e configurazione valida (controllo nel motore).
- **Segnali**: SIGINT/SIGTERM (e `erebor agent stop`) fermano il miner (`stop("agent_exit")`), chiudono la sessione nelle statistiche, rimuovono il socket ed escono con codice 0.
- **Istanza unica**: il motore prende un lock esclusivo su `<cartella dati>/engine.lock`. Un secondo agent, l'app desktop o `erebor-node` sulla stessa cartella falliscono con `engine_busy` prima di toccare il miner. Niente miner duplicati.
- **Crash del miner**: rilevato a ogni tick (2 s); stato `error`. Il riavvio automatico avviene solo se `safety.autoRestart` è attivo (max 3 tentativi, 10/20/30 s). Se l'agent stesso viene ucciso con SIGKILL, il PID del miner resta in `miner.pid` e il miner orfano viene terminato al successivo avvio del motore.
- **Configurazione**: salvata dal motore (`config.json`, scrittura atomica). La CLI modifica la configurazione solo tramite l'agent (`update_config`), con la stessa validazione della GUI.
- **Dev fee**: gestita dal `DevFeeManager` nel motore, unico per Desktop, CLI e Agent.

## Comunicazione locale (IPC)

| | |
|---|---|
| Trasporto | Unix domain socket `<cartella dati>/agent.sock` (per percorsi molto lunghi: `$XDG_RUNTIME_DIR` o cartella temporanea dell'utente, nome = hash della cartella dati) |
| Permessi | socket `0600`, cartella dati `0700` |
| Autenticazione | a ogni connessione il server verifica che l'UID del client (`SO_PEERCRED` su Linux, `getpeereid` su macOS) sia il proprio; il client verifica lo stesso del server (`agent_untrusted` altrimenti) |
| Protocollo | v1: una riga JSON per richiesta e una per risposta. `{"v":1,"cmd":"get_status","args":{}}` → `{"ok":…}` oppure `{"err":"codice"}` |
| Comandi | quelli di `erebor_core::rpc::call` (gli stessi della GUI) + `snapshot`, `device_info`, `set_device_name`, `agent_info` (risponde a ogni versione, per diagnosticare mismatch), `shutdown` (solo l'agent) |
| Rete | nessuna porta TCP. L'API HTTP remota con token parte **solo** se l'utente ha attivato l'accesso remoto (`remote.enabled`) |

Windows: il trasporto named pipe non è ancora implementato (`ipc_unsupported_platform`); il protocollo è indipendente dal trasporto (`ipc::handle` riceve e restituisce righe), quindi l'adapter deve solo spostare byte. Su Windows resta l'app desktop.

## Chi serve il socket

Il processo che possiede il motore: Agent (`owner: "agent"`), app desktop (`"desktop"`) o `erebor-node` (`"node"`). Quindi `erebor status` funziona anche con la sola app aperta. Se l'agent è attivo quando apri l'app desktop, l'app non crea un secondo motore: si collega all'agent tramite socket e mostra stato, hashrate, pool, ricompense, temperature e consumi dell'agent; uno stop dalla GUI si vede subito in CLI e TUI.

## Aggiornamento automatico

Solo per il binario `erebor` compilato in release, su Unix. Ogni ora (primo controllo dopo 60 s) l'agent legge `latest.json` dell'ultima release, scarica `erebor-<os>-<arch>` e la sua `.sig`, verifica la firma minisign con la chiave pubblica del progetto e sostituisce il file in modo atomico (scrittura accanto + `rename`). Ogni 10 s controlla se il proprio eseguibile è cambiato (aggiornamento, `erebor update`, reinstallazione): in quel caso ferma il mining in modo pulito, scrive `<cartella dati>/resume-mining` se stava minando e fa `exec` del nuovo binario con gli stessi argomenti. Stesso PID, quindi systemd/launchd non vedono un'uscita; il nuovo agent riprende il mining e cancella il file.

## Servizio systemd (utente, opzionale)

```bash
erebor service on                # consigliato: servizio + startOnLaunch + linger (mina sempre)
erebor agent install-service     # solo il servizio: ~/.config/systemd/user/org.erebor.agent.service, enable --now
systemctl --user status org.erebor.agent
journalctl --user -u org.erebor.agent
erebor agent uninstall-service
loginctl enable-linger "$USER"    # solo se deve girare anche senza login
```

Unità utente, nessun privilegio root. `Restart=on-failure`: riavvio dopo un crash, non dopo un arresto pulito (`erebor agent stop`, `systemctl --user stop`). Disattivato di default: viene installato solo con il comando esplicito. Su macOS: LaunchAgent con `KeepAlive.SuccessfulExit=false`.

## Log

`<cartella dati>/agent.log` (avvio in background) o il journal (servizio). I log essenziali del miner sono in `erebor monitor` e nel comando `get_logs`.
