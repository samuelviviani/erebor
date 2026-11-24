# Erebor TUI (`erebor monitor`)

A terminal dashboard (ratatui + crossterm) that works in any terminal, SSH included. It talks
to the agent over the local socket, so it shows **the same state** as the app and the CLI.

```bash
erebor            # opens the dashboard (first run: guided setup)
erebor dashboard  # same thing (alias of erebor monitor)
```

If Erebor is not already running in the background, it starts. The logo shows for a second
on open (any key skips it); then only the dashboard.

```text
 ● Running  1h 02m 03s   Monero (XMR) · rx/0 · auto                      erebor 0.1.6
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

- **Top line**: state, uptime, coin and algorithm (`auto` when Erebor picked it), `DEMO` in
  demo mode.
- **Mining**: hashrate, accepted/rejected shares, pool (● connected, ◌ connecting), worker,
  miner; estimated earnings per day and month, net after electricity (needs
  `energy.pricePerKwh`), power draw, CPU share, live dev fee.
- **Devices**: one row per CPU and GPU with temperature (yellow 15 °C below the limit, red
  at 5 °C), power and load.
- **Status line**: result of the last action, errors, a pending auto-restart, what blocks
  mining from starting, or an install in progress.
- **Keys**: only those that make sense in the current state (`P` only if the miner supports
  pausing).

## Keys

| Key | Action |
|---|---|
| `S` | start mining |
| `P` | pause / resume (only if the miner supports it) |
| `X` | stop mining |
| `R` | restart mining |
| `↑` `↓` `PgUp` `PgDn` `Home` `End` | scroll the log |
| `?` | help |
| `Q`, `Esc`, `Ctrl-C` | quit **without stopping mining** (to stop: `X`, an explicit choice) |

Every action is verified: the status line says `done`, `already …` or the error (for
example `wallet_missing`).

## Behaviour

- Refreshes every second; handles resizing (≥ 76 columns: two columns; below: one; very small
  terminals are clipped without errors).
- Values that aren't available show as `n/a` (temperature, power, earnings without a price):
  never silently estimated. Estimated power is marked `(estimated)`.
- Agent unreachable: an "Agent offline" screen with the command to start it; it retries
  every second.
