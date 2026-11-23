# PROGRESS

Ultimo aggiornamento: 2026-11-23 · Versione 0.1.0 (pre-release) · **Non pronto per la produzione.**

## Semplificazione per la pubblicazione (2026-11-23) ✅ codice · ⏳ prova reale

- **Consiglio di Erebor** (`Engine::advice`, `Status.advice`): moneta più redditizia misurata su questo PC + modo più conveniente per ricevere BTC (Kryptex o unMineable, confrontando le commissioni). In Home e in Monete: avvisa se l'indirizzo Monero impedisce la GPU/BTC, se una moneta scelta a mano rende meno, se un altro pagamento rende di più.
- **Monete**: "Automatica" / "Scelgo io". In automatico l'elenco non si vede; scegliendo a mano compare un avviso.
- **Home**: tutto il computer (CPU, temperatura, RAM, consumo, GPU con grafici) e la modalità personalizzata; pagina Computer eliminata. Dati chiave in una riga senza riquadri; tolta "Connessione".
- **Barra di stato fissa** in alto su ogni pagina e nelle Impostazioni: moneta, velocità o fase (avvio, connessione, "Riavvio per applicare le modifiche…"), Avvia/Arresta. Tolto lo stato dalla barra laterale.
- **Impostazioni**: pagina intera (non più popup) con un elenco a gruppi stile iOS/macOS (Aspetto, Avvio, Sicurezza, Corrente, Dati), una riga per opzione col controllo a destra; tema "Sistema" che segue il sistema operativo; tutto il tecnico dietro "Opzioni avanzate ›" (sotto-pagina con ritorno). Tolti stato del miner, toggle aggiornamenti finto, dettagli piattaforma.
- **Più computer rimossi** dall'interfaccia (selettore e aggiunta); resta l'accesso dal telefono in Avanzate. Backend invariato.
- **Configurazione iniziale e guida** riprogettate (pannello con i passi, checklist di preparazione con consiglio GPU/CPU, guida in 3 schermate illustrate).
- **Notifiche**: i messaggi uguali si uniscono (×N); ne resta in vista una, le altre raccolte dietro e aperte al passaggio del mouse (che sospende la chiusura automatica); icona per tipo, durata 3,5 s / 6 s / 8 s.
- **Nuovo logo** vettoriale (`brand/erebor-mark-dark.svg`, `-light.svg`, `erebor-app-icon.svg`), ricavato da `brand/erebor-logo-source.png` con `brand/trace-logo.py` (potrace). L'app sceglie la versione in base al tema; icone Tauri, favicon e logo del sito rigenerati.
- **Barra laterale** a larghezza fissa (niente trascinamento); resta solo il pulsante per ridurla alle icone.
- Tolta la barra di stato dalle Impostazioni; tolta la sfocatura (`backdrop-filter`) dietro i popup e dalla barra in alto: possibile causa delle righe viste nell'app (WebKit). **Da confermare nell'app reale.**
- **Temi neutri**: scuro quasi nero (#0a0a0a) e chiaro grigio puro, senza sfumature di blu; il blu resta su pulsanti, selezioni e dettagli. Logo chiaro con inchiostro nero neutro.
- **Home alleggerita**: in alto solo due cifre affiancate ("Al mese (stima)" e "Accumulati su Kryptex/unMineable" o "Minato finora" con Monero); tolte nota Kryptex, riga dati, XMR minati per chi riceve BTC.
- **Monete in automatico**: un solo riquadro "Ora mina…" con stima al giorno e le monete adatte al PC (da provare / misurata / la più redditizia); consiglio mostrato solo se c'è da intervenire; selettore Automatica/Scelgo io accanto al titolo.
- **Guadagni**: un solo selettore di periodo (24 ore / 7 / 30 giorni) che guida tre cifre (Guadagnato, Utile dopo la corrente con link per impostare il prezzo, Tempo di mining), proiezione mensile e grafico "ora per ora"/"giorno per giorno"; tolti il riquadro "Dal guadagno all'utile", il totale ripetuto e l'avviso rosso (ora una riga discreta).
- **Wallet scalabile**: saldo in grande, poi destinazione dei guadagni; sotto solo le monete con un wallet proprio (eccezioni) e "Aggiungi un wallet" con ricerca per nome/ticker; avviso se alcune monete non possono arrivare alla destinazione principale.
- **Impostazioni con icone**: ogni riga ha icona, nome, una riga di aiuto e un controllo della misura giusta (campi compatti "€ 0,25 /kWh", "65 W", orari, giorni); interruttori senza etichette ripetute.
- **Scelta del pagamento** (onboarding e Wallet) in due passi: "In Bitcoin" / "Nella moneta minata", poi il servizio; metodi come tabella dati in `PayoutChoice.tsx` (un servizio nuovo = una riga + testi + validatore); campo su una riga che riconosce cosa si incolla e cambia opzione da solo; link per creare l'ID Kryptex.
- Guida rapida tolta dal menu: si vede solo al primo avvio.
- **Icone Impostazioni** in stile macOS/iOS: quadratino a tinta piena con glifo bianco, un colore per significato; icone luna/sole/monitor nel selettore del tema.
- **Guadagni**: tolti i Dettagli tecnici; tornato il conto "Dal guadagno all'utile" sul periodo scelto (guadagnato − corrente in kWh × prezzo = utile netto, con suggerimento se in perdita).
- **Home**: niente più cerchio con orbita; pulsanti tenui sotto il titolo: "Avvia il mining"/"Riprendi" verdi, "Arresta" rosso, "Pausa" grigio.
- Messaggi di stato (attesa della prima misura, riavvio automatico tra N s) solo nella barra in alto accanto allo stato del miner; tolti gli avvisi sotto i dati della Home.
- Modalità (Eco/Bilanciata/Prestazioni/Personalizzata) applicata subito anche durante il mining, senza conferma: il riavvio è segnalato nella barra di stato.
- Verifiche: cargo fmt/clippy puliti, cargo test workspace ok (nuovo test `advice_names_the_best_coin_and_payout_for_btc`), vitest 25/25, tsc, vite build; UI controllata in Chrome headless con backend simulato.

## CLI, Agent, TUI, Fleet (2026-11-17) ✅ codice · ✅ test · ⏳ prova su Ubuntu reale

- Nuovi crate `erebor-agent`, `erebor-cli` (binario `erebor`, con setup guidato `erebor onboard` e installer `scripts/install.sh`), `erebor-tui`; moduli `ipc` e `fleet` in `erebor-core`. Desktop e `erebor-node` servono lo stesso socket.
- Verificato su macOS arm64: `cargo fmt`, `clippy --workspace --all-targets -D warnings`, **`cargo test --workspace` 102/102** (incluso test end-to-end del binario `erebor` contro un agent reale in demo), build release, prova manuale di tutti i comandi e della TUI in un pseudo-terminale.
- Codice specifico Linux (`SO_PEERCRED`) verificato con `cargo check --target x86_64-unknown-linux-gnu` in isolamento; build completa Linux/Windows coperta dalla CI (non eseguibile qui: mancano i cross-compilatori C per sqlite/ring).
- Limiti: trasporto IPC Windows (named pipe) non implementato; l'agent non riapplica `remote.enabled` a caldo (riavviarlo); mining reale end-to-end da CLI non provato (richiede wallet e consenso dell'utente).

## Stato della verifica Rust

- 2026-11-16 (pomeriggio): rete ripristinata, Rust 1.100 installato. `cargo fmt`, `cargo clippy --workspace --all-targets -D warnings` e `cargo test --workspace` **passano (59/59)**; `tauri dev` si avvia su macOS arm64.
- Corretto un **deadlock** in `Engine::status()` (guard del mutex `job` tenuto nel letterale `Status` mentre `can_start()` lo riprendeva) e 5 segnalazioni clippy.
- XMRig fissato a **6.26.0** (`xmrig-manifest.json`, hash SHA-256 da SHA256SUMS ricalcolati localmente). Firma GPG upstream non ancora verificata.
- SupportXMR verificata online (host, TLS 443, fee 0,6%, API `www.supportxmr.com/api`).

## Blocco 1 della nuova direzione (2026-11-16) ✅

- Kryptex come pool predefinita (ID `krx…` o indirizzo XMR), login `krxID/worker`.
- Commissione 2%, turni da 120 s, nessun consenso; riga informativa in onboarding e dettagli in Impostazioni/sito.
- Onboarding a 3 passi con installazione automatica di XMRig; logo e accento oro.
- Verifiche: `cargo fmt/clippy/test` 61/61, vitest 14/14, sito 2/2, build ok, percorso verificato nel browser col backend simulato.
- **Da fare per attivare la commissione**: compilare con `EREBOR_DEV_FEE_WALLET=<ID krx del progetto>`. Senza, la commissione è disattivata e la riga non compare.
- **Blocco 2**: GPU NVIDIA con SRBMiner-MULTI, benchmark automatico, scelta dell'algoritmo più redditizio tra le monete Kryptex; scelta del miner.

## Blocco 2: GPU e scelta della moneta (2026-11-16) ✅ codice · ⏳ prova reale

- `coins.rs` (catalogo + mercato + scelta), `srbminer.rs` (adapter), installatore generico per XMRig/SRBMiner, migrazione DB v2 (`coin`, `btc_per_hash`), config v2 (`coin: AUTO`, `srbminer`).
- Verifiche: `cargo test` 72/72, clippy pulito, vitest 15/15, sito 2/2, build ok; pagina Mining verificata nel browser.
- **Da verificare sul PC con NVIDIA**: formato reale dell'API di SRBMiner (parser tollerante, testato solo su JSON ipotizzato), connessione a `prl.kryptex.network:7048` con `krxID/worker`, hashrate letto correttamente, cambio moneta automatico.
- Limiti: CPU e GPU non minano insieme (una moneta alla volta; la GPU ha la precedenza); temperatura GPU non monitorata; preset Eco/Bilanciato agiscono solo sulla CPU; grafici statistici mescolano hashrate di monete diverse; nessuna pausa con SRBMiner (solo stop).
- Licenza SRBMiner: chiedere conferma all'autore prima di una release pubblica (vedi THIRD_PARTY_NOTICES e LEGAL-CHECKLIST).

## Pagamenti in BTC su wallet proprio (2026-11-16) ✅ codice · ⏳ prova reale

- Payout rilevato dal campo wallet: Kryptex ID → Kryptex; indirizzo BTC → unMineable (`BTC:addr.worker`, porta 3333); indirizzo XMR → pool configurata (solo CPU).
- Commissione: con BTC va a `EREBOR_DEV_FEE_BTC` sullo stesso pool unMineable; altrimenti a `EREBOR_DEV_FEE_WALLET` (Kryptex).
- Verifiche: cargo test 75/75, vitest 19/19.
- Da verificare: login reale su unMineable con SRBMiner (Pearl) e XMRig, fee reale unMineable.

## Revisione UI + controllo remoto (2026-11-17) ✅ codice · ⏳ prova su più macchine

- Stato del miner sempre visibile (Home e barra laterale), con le fasi intermedie: cambio moneta, avvio, connessione, prima misura.
- Computer: grafici live (ultimi 5 min) di CPU, temperatura, RAM, consumo e per ogni GPU carico/temperatura/consumo; selettore modalità condiviso con la Home (frecce da tastiera, % di CPU).
- BTC stimati in chiaro con Kryptex/unMineable (`Status.minedBtc`, 8 decimali, controvalore).
- Impostazioni a sezioni (Generali, Avvio, Accesso remoto, Energia, Avanzate, Privacy e info); "Avanzate" non è più una pagina; box di stato laterale solo informativo; rimosse le schede commissioni.
- Controllo remoto: `rpc.rs` (unico dispatcher per IPC e HTTP), `server.rs` (tiny_http, token 256 bit, `select_miner` vietato da remoto), binario `erebor-node`, lock esclusivo sulla cartella dati (`engine_busy`), selettore computer nell'app, interfaccia web per telefono.
- Avvio automatico: app al login (LaunchAgent / XDG autostart / chiave Run), nodo come servizio (`install-service`), opzione "Avvia il mining all'apertura".
- Verifiche: cargo test 82/82, clippy pulito, vitest 24/24, build ok; nodo provato end-to-end (401 senza token, comandi con token, pagina web, secondo motore rifiutato); UI verificata in Chrome headless (desktop e viewport iPhone).
- Da verificare: `install-service` su macOS/Linux/Windows reali, app che si aggancia al nodo locale, `asset_resolver` in build di release, controllo da un secondo computer in LAN.

## Milestone

| # | Milestone | Stato |
|---|---|---|
| 1 | Fondamenta (monorepo, Tauri, React, design system, navigazione, persistenza) | ✅ codice completo · Rust non compilato |
| 2 | Mining reale (hardware, XMRig, start/stop, wallet, pool, hashrate, log, errori) | ✅ codice completo · **mai eseguito con XMRig reale** |
| 3 | Dashboard e statistiche (metriche, grafici, storico, costi, redditività) | ✅ UI verificata con backend simulato |
| 4 | Dev fee (Rust, contabilità, consenso, test) | ✅ codice + 8 test (non eseguiti) |
| 5 | UX (onboarding, benchmark, impostazioni, demo, i18n, errori, accessibilità) | ✅ UI verificata (test + browser) |
| 6 | Sito e distribuzione (sito, docs, CI, packaging, release) | 🟡 sito compilato e testato; CI/release scritte ma mai eseguite |

## Funzionalità completate (codice)

- **Core Rust** (`crates/erebor-core`): validazione indirizzi Monero (base58 Monero + Keccak, rete, rifiuto seed/chiavi); configurazione validata con scrittura atomica; catalogo pool + controllo raggiungibilità + API nodejs-pool (saldo, pagamenti, statistiche di rete) + prezzo CoinGecko; adapter `MinerAdapter` con `XmrigMiner` (config JSON, API su 127.0.0.1 con token, pausa/ripresa JSON-RPC, hot switch `PUT /1/config`, log, rilevamento crash) e `DemoMiner`; benchmark offline `--bench` con durata dichiarata; installazione da manifest fissato con SHA-256 e verifica hash a ogni avvio; `DevFeeManager`; stime economiche senza doppie sottrazioni; SQLite con migrazioni, sample 10 s, sessioni, retention 90 giorni; orchestratore con crash recovery opzionale (max 3), soglia termica, arresto economico (solo con dati affidabili), programmazione oraria, terminazione miner orfani, DB demo separato.
- **App desktop**: 25 comandi Tauri tipizzati; onboarding a 8 passi (benvenuto, hardware, wallet, pool, miner, commissioni, benchmark, riepilogo) con percorso demo; Dashboard, Mining, Hardware, Wallet, Statistiche (1h/24h/7g/30g), Impostazioni; it/en; tema scuro/chiaro; tooltip, skeleton, stati vuoti, conferme, toast dagli alert del backend; mining arrestato alla chiusura dell'app.
- **Sito**: Home, Download (solo release reali, da `releases.json`), Commissioni, Documentazione, FAQ, Open source, Privacy, Termini, sitemap, robots, Open Graph, header di sicurezza Vercel.
- **Repo**: README, LICENSE (MIT), THIRD_PARTY_NOTICES, CONTRIBUTING, SECURITY, PRIVACY, ARCHITECTURE, INSTALLATION, DEVELOPMENT, CHANGELOG, docs/RELEASE, docs/POOLS, docs/LEGAL-CHECKLIST; CI e workflow di release manuale (bozza).

## Test

### Superati (eseguiti in questa sessione)

| Suite | Risultato |
|---|---|
| `apps/desktop` vitest: format, i18n (parità chiavi it/en), onboarding completo, percorso demo start/pausa/stop, impostazioni (lingua, revoca consenso), statistiche | **12/12 ✅** |
| `apps/website` node:test: nessuna promessa di rendimento, asset di release con SHA-256 | **2/2 ✅** |
| `tsc --noEmit` desktop + sito | ✅ |
| `vite build` desktop, `next build` sito (12 pagine statiche) | ✅ |
| `pnpm audit --prod` | ✅ nessuna vulnerabilità nota |
| Algoritmo di validazione wallet portato in JS e verificato sul vettore di test (indirizzo di donazione pubblicato nel README di XMRig): checksum OK, variante alterata rifiutata | ✅ |
| Verifica visiva nel browser (backend simulato): onboarding, dashboard, statistiche; sito | ✅ |

### Non eseguibili in questo ambiente

| Test | Motivo |
|---|---|
| `tauri build` / installer | idem |
| Integrazione reale con XMRig (avvio, API, hot switch, benchmark) | nessun binario XMRig scaricabile; nessun mining reale autorizzato |
| API reali di SupportXMR e CoinGecko | rete limitata; i parser sono testati su JSON di esempio |
| Workflow GitHub Actions | repository non pubblicato |
| Lettura temperatura/potenza su Windows/Linux | solo macOS disponibile, e solo frontend eseguito |

### Checklist di verifica manuale (da fare)

- [ ] `tauri dev` su Linux e Windows: onboarding → installazione XMRig → benchmark → avvio → hashrate reale → share → stop.
- [ ] Il pulsante Stop funziona con l'API di XMRig irraggiungibile (es. porta bloccata).
- [ ] Chiusura dell'app durante il mining ⇒ processo XMRig terminato.
- [ ] Kill forzato di Erebor ⇒ al riavvio XMRig orfano terminato.
- [ ] Switch dev fee con build di test (`EREBOR_DEV_FEE_WALLET` = wallet di test personale) e `BLOCK_SECS` ridotto: verificare `PUT /1/config` e il cambio indirizzo nella pool.
- [ ] Verificare che il benchmark `--bench=10M` stampi le righe `speed 10s/60s/15m` attese.
- [ ] Verificare il formato reale di `/2/summary` della versione XMRig fissata (in particolare `connection.accepted`).
- [ ] Verificare host/porta TLS/fee di SupportXMR e il formato delle sue API.
- [ ] Riavvio dell'app ⇒ impostazioni e statistiche ritrovate.

## Problemi noti / limiti

- Pausa/ripresa e hot switch richiedono l'API XMRig con `restricted: false` (loopback + token): accettato, ma da rivedere in audit.
- Le difficoltà di rete per le stime vengono dalla prima pool del catalogo anche con pool personalizzate.
- GPU: rilevate ma non usate (by design per RandomX).
- Potenza misurata solo su Linux via RAPL (spesso solo root); altrove serve la stima dell'utente.
- Temperatura dipende da `sysinfo`: su Windows spesso non disponibile.
- Controllo remoto in HTTP non cifrato (token): solo LAN/VPN. Nessun QR per il link del telefono.
- Updater Tauri firmato: documentato, non attivo (servono chiavi).
- Il controllo TLS della pool è demandato a XMRig; il test di raggiungibilità è solo TCP.
- Sito solo in italiano; app in italiano e inglese.
- Bundle JS: ~300 kB principale + ~390 kB pagina Statistiche (Recharts, caricata on demand).
- Identificativo app `org.erebor.desktop` provvisorio.

## Prossimi interventi

1. Verificare la firma GPG upstream di SHA256SUMS di XMRig 6.26.0.
2. Eseguire `tauri build` e provare l'installer.
3. Eseguire la checklist manuale su Windows 11 e Ubuntu 22.04/24.04.
4. Test E2E sull'app reale (es. WebdriverIO + tauri-driver) per onboarding, impostazioni e dashboard.
5. Attivare l'updater firmato e la firma del codice Windows.
6. Pubblicare il repository, configurare `NEXT_PUBLIC_REPO_URL`/`EREBOR_REPO_URL`, abilitare il private vulnerability reporting.
7. Completare `docs/LEGAL-CHECKLIST.md`.
