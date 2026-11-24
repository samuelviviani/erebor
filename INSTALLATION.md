# Installazione

> Scarica l'ultima versione da [GitHub Releases](https://github.com/samuelviviani/erebor/releases/latest). Dopo l'installazione l'app si aggiorna da sola.

## Windows 10/11 (x64)

1. Scarica `Erebor_<versione>_x64-setup.exe` (NSIS) o `.msi` dalla release GitHub.
2. Verifica l'hash: `Get-FileHash .\Erebor_*_x64-setup.exe -Algorithm SHA256` e confrontalo con `SHA256SUMS`.
3. Avvia l'installer. SmartScreen può avvisare finché il binario non è firmato con un certificato di code signing.
4. WebView2 è richiesto (incluso in Windows 11; l'installer lo scarica su Windows 10 se manca).

## Linux (Ubuntu 22.04+ x64)

- **AppImage**: `chmod +x Erebor_*.AppImage && ./Erebor_*.AppImage`
- **deb**: `sudo apt install ./erebor_*_amd64.deb`
- Dipendenze runtime: `libwebkit2gtk-4.1-0`, `libgtk-3-0`.
- Temperatura: richiede sensori esposti via hwmon (`lm-sensors`). Potenza: RAPL (`/sys/class/powercap`), spesso leggibile solo da root; in tal caso imposta una stima in Impostazioni › Energia.

## macOS

Previsto in una fase successiva (serve firma e notarizzazione Apple).

## XMRig

Al primo avvio Erebor scarica la release ufficiale fissata da GitHub e ne verifica lo SHA-256, oppure puoi selezionare un binario esistente. Alcuni antivirus segnalano XMRig come "riskware": Erebor non aggira gli antivirus, la decisione resta tua. Huge pages e MSR mod di XMRig possono richiedere privilegi amministrativi: Erebor non li richiede e non modifica il sistema.

## Nodo headless e controllo remoto

Per minare senza tenere aperta l'app e controllare il computer da un Mac, un altro PC o il telefono:

```sh
cargo build --release -p erebor-core --bin erebor-node
mkdir -p dist/web && cp target/release/erebor-node dist/ && cp -R apps/desktop/dist/* dist/web/   # interfaccia web per il telefono
dist/erebor-node                    # stampa indirizzo, codice di accesso e link per il telefono
dist/erebor-node install-service    # avvio al login + riavvio automatico (launchd / systemd --user / Operazione pianificata)
dist/erebor-node uninstall-service
```

- Nodo e app usano la stessa cartella dati: ne gira uno solo alla volta. Con il nodo attivo, l'app su quel computer si collega al nodo.
- In alternativa, senza nodo: app → Impostazioni → Accesso remoto (funziona finché l'app è aperta).
- Dall'altro computer: Impostazioni → Accesso remoto → Aggiungi computer (indirizzo + codice). Dal telefono: apri il link `http://IP:7420/#token=…`.
- HTTP non cifrato con token: solo rete di casa o VPN (Tailscale/WireGuard). Su Linux, per avviare il nodo senza login: `loginctl enable-linger`.

## CLI, Agent e TUI da terminale

```bash
curl -fsSL https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install.sh | sh
# scarica il binario firmato, verifica lo SHA-256, installa in ~/.local/bin e avvia il setup
# dal checkout, compilando: sh scripts/install.sh --from-source
```

Compilazione su Ubuntu, comandi e risoluzione dei problemi: [docs/CLI.md](docs/CLI.md). L'Agent (`erebor agent start`, servizio systemd utente opzionale) sostituisce il nodo per l'uso locale: [docs/AGENT.md](docs/AGENT.md). Il nodo resta per l'interfaccia web del telefono e serve anche il socket locale, quindi `erebor status` funziona anche con il nodo attivo.

## Disinstallazione

Disinstalla l'app dal sistema; i dati locali si eliminano da Impostazioni › Privacy › "Elimina tutti i dati locali" (oppure cancellando la cartella dati mostrata lì).

## Errori comuni

| Messaggio | Soluzione |
|---|---|
| "Installa o seleziona XMRig" | Mining › Installazione miner |
| "Il binario XMRig è cambiato dopo la verifica" | reinstalla o riseleziona: il file è stato modificato |
| "Nessun manifest XMRig per la tua piattaforma" | seleziona un XMRig esistente |
| Pool non raggiungibile | controlla host/porta, firewall, proxy aziendale |
