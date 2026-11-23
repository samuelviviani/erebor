# Verifiche legali e commerciali ancora necessarie

**Nessuna revisione legale è stata effettuata.** Prima di una distribuzione pubblica o commerciale:

- [ ] **Nome e marchio "Erebor"**: ricerca di anteriorità (EUIPO, USPTO, WIPO) e rischi legati alla proprietà intellettuale tolkieniana (Middle-earth Enterprises / Tolkien Estate) e a marchi esistenti nel settore crypto/finanza.
- [ ] **Dominio** e identificativo app (`org.erebor.desktop` in `tauri.conf.json` è provvisorio).
- [ ] **Privacy Policy** conforme GDPR (titolare, basi giuridiche, trasferimenti extra-UE verso pool/CoinGecko/GitHub, diritti dell'interessato).
- [ ] **Termini di utilizzo** e limitazione di responsabilità validati.
- [ ] **Commissione di sviluppo**: inquadramento fiscale dei ricavi del progetto (cripto-attività), eventuali obblighi informativi; conformità delle informative al Codice del Consumo / Direttiva 2011/83/UE se considerata servizio a pagamento.
- [ ] **MiCA / normativa cripto-attività**: confermare che un software non-custodial che non gestisce fondi non rientri tra i servizi regolamentati.
- [ ] **Normative locali sul mining** (alcuni Paesi lo limitano) e avvertenze sui consumi.
- [ ] **Licenze**: generare l'elenco completo delle dipendenze transitive; confermare gli obblighi GPLv3 se XMRig verrà incluso nei pacchetti.
- [ ] **Termini CoinGecko** per uso in software distribuito pubblicamente (eventuale chiave API / attribuzione).
- [ ] **Firma del codice** e reputazione SmartScreen/antivirus (i miner sono spesso classificati come PUA).

## SRBMiner-MULTI

- [ ] Chiedere all'autore (doktor83) conferma scritta che scaricare la release ufficiale ed eseguirla non modificata da Erebor sia consentito dalla licenza ("incorporating into another product" è vietato senza permesso).
- [ ] In alternativa, valutare miner GPU open source per gli algoritmi supportati.
