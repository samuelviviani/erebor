# Privacy

Erebor è local-first e non raccoglie dati.

- **Nessun account, telemetria o analytics.**
- **Dati locali** (cartella dati dell'app): configurazione (indirizzo wallet pubblico, pool, preferenze), statistiche di mining, sessioni, contabilità della commissione, XMRig installato. Esportabili ed eliminabili dalle impostazioni.
- **Servizi esterni**:
  - la **pool** scelta: riceve indirizzo pubblico, nome worker e IP (mining); la sua API pubblica viene interrogata con l'indirizzo pubblico per saldo e pagamenti;
  - **CoinGecko**: prezzo XMR (ogni 10 minuti; nessun dato personale oltre all'IP);
  - **GitHub**: solo per scaricare XMRig;
  - **server di donazione di XMRig**: per la donazione integrata nel miner (1% del tempo).
- In modalità demo non avviene alcuna richiesta di rete per mining o prezzi.
- Il sito web è statico, senza cookie né analytics.

Nota: questo documento descrive il comportamento tecnico. Un'informativa conforme al GDPR per la distribuzione pubblica deve essere validata da un professionista (vedi `docs/LEGAL-CHECKLIST.md`).
