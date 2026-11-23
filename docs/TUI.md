# Erebor TUI (`erebor monitor`)

Dashboard da terminale (ratatui + crossterm), funziona in qualsiasi terminale Ubuntu (anche via SSH). È collegata all'Agent tramite il socket locale: mostra lo **stesso stato** della GUI e della CLI.

```bash
erebor            # apre la dashboard (al primo avvio: setup guidato)
erebor dashboard  # stessa cosa (alias di erebor monitor)
```

Se Erebor non è già attivo in background, parte da solo. All'apertura compare per un secondo il logo (qualsiasi tasto lo salta); nei terminali alti almeno 32 righe resta in cima alla dashboard.

```text

             /\                ███████ ██████  ███████ ██████   ██████  ██████
            /::\               ██      ██   ██ ██      ██   ██ ██    ██ ██   ██
    /\     /::o:\      /\      █████   ██████  █████   ██████  ██    ██ ██████
   /::\   /:::|::\    /::\     ██      ██   ██ ██      ██   ██ ██    ██ ██   ██
  /::::\ /:o--':::\  /:o::\    ███████ ██   ██ ███████ ██████   ██████  ██   ██
 /:o::::/:::.--.:::\/::|:::\
/::|::::::::|  |:::::::|::::\  Mining, under your control.  v0.1.0

  DEMO · simulated, nothing is mined                     ● Running · 1h 02m 03s
╭ Hashrate ────────────────────────────────────────────────────────────────────╮
│ 4.25 kH/s   avg 4.14 kH/s · peak 4.33 kH/s                                   │
│             ▃▂▂▅▅▇▆▅▇▅▃▅▃▄▁                              ▃▃▆▅▄▇▅█▆▄▅▃▁▂      │
│        ▃▃▃▇▇███████████████▇▇▄▅▂                   ▄▄▄█████████████████▇█▅▂▃ │
│ ▂▁▄▄█▇████████████████████████████▆▃▄▂▃▂ ▂▁▃▂▂▅▅▄███████████████████████████ │
│ ████████████████████████████████████████████████████████████████████████████ │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Mining ──────────────────────────────╮╭ This computer ───────────────────────╮
│ Coin      Monero · rx/0              ││ Earnings  0.82 EUR / day             │
│ Pool      ● xmr.kryptex.network:7029 ││           ≈ 24.60 EUR / month        │
│ Shares    148 ✓  2 ✗  98.7%          ││ CPU       ███████░░░ 67 °C           │
│ Miner     XMRig 6.26.0               ││ Power     n/a                        │
│ Dev fee   2% of time · now for you   ││                                      │
╰──────────────────────────────────────╯╰──────────────────────────────────────╯
╭ Log ─────────────────────────────────────────────────────────────────────────╮
│ [12:01:03] miner  speed 10s/60s/15m 4251.3 4248.9 n/a H/s                    │
│ [12:01:05] pool   accepted (148/2) diff 120001 (41 ms)                       │
│ [12:01:09] devfee mining for: user                                           │
│                                                                              │
│                                                                              │
│                                                                              │
│                                                                              │
╰──────────────────────────────────────────────────────────────────────────────╯
 Q closes the dashboard; mining keeps running in the background.
  P  Pause    X  Stop    R  Restart    ↑↓  Logs    ?  Help    Q  Quit
```

(Schermata reale, 80×34, modalità demo.)

- **Hashrate**: valore attuale, media e picco della sessione, grafico live (un campione al secondo, ultimi 10 minuti).
- **Mining**: moneta, pool (● connessa, ◌ in connessione), share accettate/rifiutate con percentuale, miner, dev fee in tempo reale.
- **This computer**: guadagni stimati al giorno e al mese, temperatura CPU/GPU a barre (verde, giallo da 70 °C, rosso da 80 °C), consumo.
- **Riga sotto**: esito dell'ultima azione, oppure cosa impedisce di avviare il mining e il comando per risolverlo.
- **Tasti**: mostrati solo quelli che servono nello stato attuale.

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

Ogni azione è verificata: la riga sopra i tasti dice `done`, `already …` o l'errore (es. `consent_required`).

## Comportamento

- Aggiornamento ogni secondo; ridimensionamento gestito (≥ 100 colonne: pannelli affiancati; sotto: impilati; terminali molto piccoli: si taglia senza errori).
- Valori non disponibili mostrati come `n/a` (temperatura, consumo, ricompense senza prezzo): mai stimati di nascosto. Il consumo stimato è marcato `(estimated)`.
- Agent non raggiungibile: schermata "Agent offline" con il comando per avviarlo; riprova da sola ogni secondo.
- Stile: sfondo scuro, testo chiaro, accenti verdi, bordi grigi.
