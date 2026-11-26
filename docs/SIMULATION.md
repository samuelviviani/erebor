# Simulation

Everything multi-device can be tried without hardware. Simulated devices are always flagged
(`simulated: true`, "Simulated" badge, model names ending in "(Erebor simulator)"), and
their data never mixes with real data: demo agents use the demo database, ASIC simulators
are separate processes' worth of fake API.

## Demo mode in the app

With demo mode on (first start → "Try demo mode", or Settings → Advanced → Demo mode), the
Devices section shows a simulated infrastructure: the layout below, paired automatically,
with a week of made-up history so the charts and totals are populated, alerts included (the
hot ASIC, the offline server). It lives in its own registry (`<data dir>/demo-fleet/`,
recreated at every start) and never mixes with real devices: turning demo mode off shows the
real list again, unchanged. Everything in it is marked "Simulated".

## `erebor sim`: a whole infrastructure in one command

```bash
cargo build -p erebor-cli
target/debug/erebor sim --pair          # pairs everything into this computer's Erebor
# open Erebor Desktop → Devices, or:
target/debug/erebor devices
# Ctrl-C stops the simulation and removes the simulated devices (--keep keeps them)
```

It starts, on `127.0.0.1` (ports from `--base-port`, default 17420):

| Name | What |
|---|---|
| sim-pc-1, sim-pc-3 | agents mining with the demo miner |
| sim-pc-2 | agent, idle |
| sim-rig-1, sim-rig-2 | agents shown as GPU rigs, mining |
| sim-gateway | agent acting as ASIC gateway for the three below |
| sim-s19-a | Antminer S19 simulator, write commands allowed |
| sim-m30s | WhatsMiner M30S+ simulator (read-only API, reports power) |
| sim-broken | answers invalid data (error handling) |
| sim-s19-hot | Antminer at ~96 °C (temperature alerts), direct |
| sim-s19-ro | Antminer whose API is read-only, direct |
| sim-offline | agent paired and then shut down (offline handling) |

Without `--pair` it prints each agent's address and pairing code, to try the pairing flow by
hand (`erebor devices add 127.0.0.1:17421 --code …`, or the app's Add device).

What to try: start/stop/restart on a selection (ASICs report why they can't stop), apply a
profile to a group (the WhatsMiner and read-only S19 are skipped with the reason), change an
ASIC's pool, revoke the controller on an agent (`erebor-agent --data-dir <its folder>
revoke …`) and watch it turn unauthorized, stop the simulation and watch devices go offline
and the alerts appear.

## Real agents together with simulated ones

Real agents on Linux or Windows pair the same way; they appear next to the simulated ones
without the badge.

## Browser-only UI

```bash
pnpm --filter @erebor/desktop dev
# http://localhost:1420/?demo&devices&cfg={"defaultView":"devices"}
```

`?devices` adds simulated devices to the UI's mock backend (`src/dev/mockFleet.ts`).

## Automated tests

```bash
cargo test --workspace                       # unit + integration (agents, pairing, ASIC sims)
cargo test -p erebor-cli --test e2e          # real binaries: agent ↔ CLI controller
cargo test -p erebor-core --release -- --ignored load --nocapture   # scale measurement
pnpm --filter @erebor/desktop test           # UI flows against the mock backend
```

## Docker

Not provided: `erebor sim` runs the same layout in one process with no container runtime.
Real multi-host tests use real agents installed with `scripts/install-agent.sh`.

## Limits

Simulators reproduce the documented API formats, not every firmware's behaviour. Passing
these tests says the app handles those formats and failure cases; it does not prove
compatibility with any physical ASIC (see [ASIC.md](ASIC.md)).
