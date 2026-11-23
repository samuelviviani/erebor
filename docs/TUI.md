# Erebor TUI (`erebor monitor`)

Dashboard da terminale (ratatui + crossterm), funziona in qualsiasi terminale Ubuntu (anche via SSH). È collegata all'Agent tramite il socket locale: mostra lo **stesso stato** della GUI e della CLI.

```bash
erebor agent start    # se non è già attivo (o app desktop aperta)
erebor monitor
```

```text
┌───────────────────────────────────────────────────────────────DEMO  ● Paused ┐
│ EREBOR                                                                       │
└──────────────────────────────────────────────────────────────────────────────┘
┌ Metrics ─────────────────────────────────────────────────────────────────────┐
│ Hashrate         0.00 H/s                                                    │
│ Earnings         0.07 EUR / day (estimate)                                   │
│ CPU temperature  42 °C                                                       │
│ Power            n/a                                                         │
│ Uptime           1m 19s                                                      │
└──────────────────────────────────────────────────────────────────────────────┘
┌ Miner ───────────────────────────────────────────────────────────────────────┐
│ Miner            Demo (simulated) demo                                       │
│ Coin             Monero · rx/0                                               │
│ Pool             Connected                                                   │
│ Pool address     xmr.kryptex.network:7029                                    │
│ Accepted shares  2                                                           │
│ Rejected shares  0                                                           │
│ Dev fee          2% · now: user                                              │
└──────────────────────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────────────────────┐
│ pause: done                                                                  │
│ [S] Start  [P] Resume  [X] Stop  [R] Restart  [?] Help  [Q] Quit             │
└──────────────────────────────────────────────────────────────────────────────┘
```

(Schermata reale, 80×24, modalità demo, dopo aver premuto `P`. Con terminali più alti compare il pannello Logs; da 100 colonne i pannelli sono affiancati.)

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
