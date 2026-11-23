# Sicurezza

## Segnalare una vulnerabilità

Non aprire issue pubbliche per problemi di sicurezza. Usa **GitHub › Security › Report a vulnerability** (private vulnerability reporting) sul repository ufficiale. Indica versione, piattaforma, passi per riprodurre e impatto. Puntiamo a confermare la ricezione entro 7 giorni.

> Prima della pubblicazione del repository il maintainer deve abilitare il private vulnerability reporting (o aggiungere qui un contatto dedicato).

## Versioni supportate

Solo l'ultima release riceve correzioni di sicurezza.

## Modello di sicurezza (sintesi)

- Nessun segreto utente: solo indirizzi pubblici. Input che sembrano seed phrase o chiavi private vengono rifiutati.
- L'unico eseguibile avviato è XMRig: installato da manifest fissato con SHA-256 (URL limitati a `github.com/xmrig/xmrig/releases/download/`), oppure scelto esplicitamente dall'utente (marcato "non verificato"). L'hash del binario viene ricontrollato a ogni avvio.
- Processi avviati con argomenti fissi, mai tramite shell; tutti i campi di configurazione sono validati.
- API di XMRig solo su loopback, token casuale per sessione, file di config 0600. Nessuna porta pubblica.
- Permessi Tauri minimi (`core:default`, `dialog:allow-open`), CSP restrittiva.
- Nessuna persistenza nascosta: niente avvio automatico con il sistema; mining arrestato alla chiusura; miner orfani terminati all'avvio successivo.
- Errori senza segreti: il token API non viene mai mostrato né registrato.
- Aggiornamenti: previsti tramite updater firmato di Tauri (vedi `docs/RELEASE.md`). Le chiavi di firma non vanno mai nel repository.

## Limiti noti

- Senza firma del codice (Authenticode / Apple) i sistemi operativi mostrano avvisi.
- La firma GPG di `SHA256SUMS` di XMRig non è ancora verificata automaticamente: lo script del manifest verifica gli hash, la firma va controllata manualmente dal maintainer.
