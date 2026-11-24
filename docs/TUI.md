# Erebor TUI (`erebor monitor`)

Dashboard da terminale (ratatui + crossterm), funziona in qualsiasi terminale Ubuntu (anche via SSH). È collegata all'Agent tramite il socket locale: mostra lo **stesso stato** della GUI e della CLI.

```bash
erebor            # apre la dashboard (al primo avvio: setup guidato)
erebor dashboard  # stessa cosa (alias di erebor monitor)
```

Se Erebor non è già attivo in background, parte da solo. All'apertura compare per un secondo il logo (qualsiasi tasto lo salta); poi solo la dashboard, senza logo né titoli.

```text
 ● Running  1h 02m 03s   Monero (XMR) · rx/0 · auto                      erebor 0.1.5
── Mining ──────────────────────────────────────────────────────────────────────────────
 Hashrate  4.25 kH/s                           Earnings  0.82 EUR/day  24.60 EUR/month
 Shares    148 ✓  2 ✗  1.3% rejected           Net       0.50 EUR/day  power 0.32 EUR/day
 Pool      ● xmr.kryptex.network:7029          Power     96 W
 Worker    rig01                               CPU use   balanced · 50% of threads
 Miner     XMRig 6.26.0                        Dev fee   2% of time · now for you

── Devices · stop above 85 °C ──────────────────────────────────────────────────────────
                               TEMP     POWER    LOAD
 CPU                           67 °C
 GPU 0  RTX 3070               58 °C    120 W    98%

── Log ─────────────────────────────────────────────────────────────────────────────────
 [12:01:03] miner  speed 10s/60s/15m 4251.3 4248.9 n/a H/s
 [12:01:05] pool   accepted (148/2) diff 120001 (41 ms)

 P pause   X stop   R restart   ↑↓ log   ? help   Q quit
```

- **Riga in alto**: stato, uptime, moneta e algoritmo (`auto` se scelta da Erebor), `DEMO` in modalità demo.
- **Mining**: hashrate, share accettate/rifiutate con percentuale di scarto, pool (● connessa, ◌ in connessione), worker, miner; guadagni stimati al giorno e al mese, netto dopo il costo dell'energia (serve `energy.pricePerKwh`), consumo, uso della CPU, dev fee in tempo reale.
- **Devices**: una riga per CPU e GPU con temperatura (gialla a 15 °C dal limite, rossa a 5 °C), consumo e carico.
- **Riga sotto**: esito dell'ultima azione, errore, riavvio automatico in corso, cosa impedisce di avviare il mining, oppure installazione in corso.
- **Tasti**: solo quelli che servono nello stato attuale (`P` solo se il miner supporta la pausa).

## Tasti

| Tasto | Azione |
|---|---|
| `S` | avvia il mining |
| `P` | pausa / ripresa (solo se il miner la supporta) |
| `X` | arresta il mining |
| `R` | riavvia il mining |
| `↑` `↓` `PgUp` `PgDn` `Home` `End` | scorre i log |
| `?` | aiuto |
| `Q`, `Esc`, `Ctrl-C` | esce **senza fermare il mining** (per fermarlo: `X`, scelta esplicita) |

Ogni azione è verificata: la riga sopra i tasti dice `done`, `already …` o l'errore (es. `wallet_missing`).

## Comportamento

- Aggiornamento ogni secondo; ridimensionamento gestito (≥ 76 colonne: due colonne; sotto: una; terminali molto piccoli: si taglia senza errori).
- Valori non disponibili mostrati come `n/a` (temperatura, consumo, ricompense senza prezzo): mai stimati di nascosto. Il consumo stimato è marcato `(estimated)`.
- Agent non raggiungibile: schermata "Agent offline" con il comando per avviarlo; riprova da sola ogni secondo.
- Stile: sfondo scuro, testo chiaro, accenti oro, separatori grigi.
