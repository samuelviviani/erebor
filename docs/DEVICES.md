# Managing devices

Erebor can mine on this computer **and** manage other computers, servers, mining rigs and
ASICs from one dashboard. Both work at the same time; the first-start choice ("Start
mining" / "Manage devices") only decides the setup you see first and the main screen
(Settings → Devices → Main screen switches it, nothing is lost).

No account, no Erebor server: the desktop app (or the CLI) talks directly to each device.

```text
 Erebor Desktop / erebor CLI  (controller: registry, dashboard, alerts, profiles)
        │  TLS 1.3, mutual certificates pinned at pairing, port 7420
        ├──────────────► erebor-agent  (PC / server / rig: mines locally, keeps mining offline)
        ├──────────────► erebor-agent as ASIC gateway ──► ASICs (CGMiner API, LAN only)
        └──────────────► ASIC (CGMiner API, port 4028) directly, on your LAN
```

## Device types and states

| Type | How Erebor reaches it |
|---|---|
| This computer | in-process (or its local agent) |
| Computer, Server, GPU rig, CPU rig | `erebor-agent` on that machine, paired |
| ASIC | its own firmware API (CGMiner-compatible), directly or through a gateway agent |

Connection: `online`, `offline`, `connecting`, `unreachable` (wrong address or not an
Erebor agent), `unauthorized` (not paired, or access revoked).
Mining: `idle`, `starting`, `mining`, `stopping`, `error`, `maintenance` (miner install
in progress).

Values a device doesn't report are shown as "—" (`null` in JSON), never estimated.

**Only managing other devices** (main screen "Devices" and this computer never set up for
mining): the sections about mining here (Home, Coins, Energy, Computer, the status bar,
this PC's row) are hidden; Settings → Devices → "Set up mining here" brings them back.
**Demo mode** fills the Devices section with simulated devices and history
([Simulation](SIMULATION.md#demo-mode-in-the-app)).
Simulated devices (demo agents, ASIC simulators) are always marked **Simulated**.

## Add a computer (pairing)

1. Install the agent on it: [Linux](AGENT-LINUX.md) · [Windows](AGENT-WINDOWS.md) ·
   [macOS](AGENT.md#macos).
2. On that computer: `erebor-agent pair` (Linux service: `sudo -u erebor-agent erebor-agent pair`).
   It prints a 12-symbol code (`XXXX-XXXX-XXXX`), valid **10 minutes, once**, and the
   agent's fingerprint.
3. In Erebor Desktop: **Devices → Add device → Computer or server**. Pick it from "On this
   network" or type its address, enter the code, choose the type. Or from a terminal:

   ```bash
   erebor devices add 192.168.1.20 --code ABCD-EFGH-JKMN --kind gpu-rig
   ```

What pairing does: both sides prove they know the code with an HMAC bound to both TLS
certificates, so nobody in the middle can splice in. Afterwards the controller pins the
agent's certificate fingerprint and the agent trusts the controller's certificate; the code
is never used again. Five wrong codes burn it; repeated failures from one address lock that
address out for 10 minutes.

Revoke a controller on the agent: `erebor-agent controllers`, then
`erebor-agent revoke <name or fingerprint>`. Removing a device in the app also tells its
agent to forget this computer. A new agent key (`erebor-agent rotate-key`) requires pairing
again: the app shows the device as unauthorized with the reason.

## Remote networks and VPN

Erebor never needs a public port on your router. The agent listens on `0.0.0.0:7420` by
default; any network that routes to it works: LAN, WireGuard, Tailscale, NordVPN Meshnet,
ZeroTier.

- Add the device by its VPN address (`100.x.y.z` for Tailscale/Meshnet, the WireGuard peer
  IP). LAN discovery doesn't cross a VPN: type the address.
- To accept controllers **only** through the VPN: `erebor-agent init --listen 100.64.0.5:7420`
  (the agent's VPN address) and restart the agent.
- If the agent's address changes (DHCP, new VPN IP), pair again or edit the address by
  re-adding: the record, its group and tags are kept (`device_repaired`).
- Clocks: requests older/newer than 10 minutes are refused (`clock_skew`), keep NTP on.

## Dashboard

Devices shows totals (devices, online/offline, mining, power, revenue, electricity, net per
day), **hashrate per algorithm** (never summed across algorithms), history (1 h, 24 h,
7 days, 30 days) and the device table: search, filters (online, offline, mining,
problems), group/tag filter, grouping by group or type, table or cards, sorting, and
multi-select with Start, Stop, Restart, Apply profile, Move to group and Tags. Stop,
Restart and Apply ask for confirmation and list the devices; the result is shown per
device. Lists of more than 120 devices render only the visible rows.

The device page has state, hardware, history, setup (coin, wallet, worker, mode, max
temperature, or the full configuration as JSON, validated by the device itself), log,
operations history, group, tags and its own electricity price. Actions a device can't do are
disabled with the reason (for example "ASIC firmware has no reliable stop through its API").

## Groups, tags, profiles

- **Groups**: `Home mining`, `GPU rigs`, `ASIC farm`… One group per device, with an optional
  price per kWh; start or stop all members at once.
- **Tags**: free labels, any number per device, usable in filters.
- **Profiles**: a partial configuration applied to many devices: coin (or AUTO), pool,
  public payout address, worker (`{device}` becomes each device's name), mode, max
  temperature, auto-restart; for ASICs a stratum URL and `wallet.worker`. Applying checks
  every device first (graphics card present, miner available for its OS/CPU, ASIC algorithm,
  supported features); incompatible ones are *skipped*. Each computer validates the new
  configuration itself; with "Start mining once applied", if mining doesn't start the
  previous configuration is restored (*rolled back*). At most 8 devices are contacted at a
  time; one failure never stops the others.

A profile can't set miner paths, notification channels, demo mode or other local settings.

## Energy and profitability

Electricity price per device > per group > global (Settings → Devices) > the computer's own
setting. Cost = measured (or the computer's estimated) power × 24 h × price. Revenue is an
estimate from current network data: the computer's own estimate (after the 2% Erebor fee and
pool fees), for ASICs SHA-256d/Scrypt hashrate × network reward from the same public market
data the app already uses. The table marks estimated power with `*`; totals say "estimate".
None of these figures is a guaranteed income: the pool balance is what you are paid.

Samples are stored once a minute while the app is open (`controller.db`), averaged into
15-minute rows after two days, deleted after the retention (30 days by default).

## Alerts

Device offline / back online, not authorized, miner crashed or stopped, temperature above
the threshold (computers 85 °C, ASICs 90 °C by default), hashrate below its recent average
(30%), power above it (50%), rejected shares (5%), pool not connected, agent update
available, plus the alerts each agent raises itself. Each alert fires once per device and
then waits for the cooldown (30 min); thresholds are on the Alerts page. New warnings
appear as notifications while the app is open; the history stays on the Alerts page.

## Non-custodial

Erebor never holds funds, never creates wallets, never asks for a seed phrase or a private
key, and never changes a payout address on its own: pools pay the public addresses you set.
The 2% fee is applied by each computer's own engine (shown in the app); controllers and
gateways add nothing on top, and ASICs managed by Erebor pay no fee to the project.

## Command line

```bash
erebor devices                      # totals + table (JSON with --json)
erebor devices discover             # agents on this LAN
erebor devices discover --asic 192.168.1.0/24 [--via gateway]
erebor devices add HOST --code CODE [--kind server|gpu-rig|cpu-rig|remote-pc]
erebor devices add 192.168.1.50 --asic [--port 4028] [--name S19-A] [--via gateway]
erebor devices status --group "GPU rigs" | inspect rig-01 | logs rig-01
erebor devices rename ID NAME | tag rig-01 --tags gpu,office | remove rig-01
erebor devices events | export > devices.json | import devices.json
erebor mining start --device rig-01 | stop --group gpu-rigs | status --all
erebor groups create "ASIC farm" --energy-price 0.12 | assign "ASIC farm" s19-a s19-b
erebor profiles create bitcoin-pool --asic-url stratum+tcp://pool:3333 --asic-user "bc1q….{device}"
erebor profiles apply bitcoin-pool --group "ASIC farm"
```

Exit codes: 0 ok · 1 failed · 2 usage · 3 local agent unreachable · 4 some devices failed.

## Troubleshooting

| Shown | Check |
|---|---|
| Unreachable / offline | Agent running (`erebor-agent status`)? Remote control on (`erebor-agent init`, then restart)? Firewall allows TCP 7420? Same VPN? |
| `pairing_not_active` | No code on the agent, or it expired (10 min) or was burnt (5 wrong tries): `erebor-agent pair` again |
| Not authorized | Access revoked or agent key changed: pair again |
| `fingerprint_mismatch` | A different machine answers at that address, or the agent was reinstalled: verify, then pair again |
| `clock_skew` | Fix the clock (NTP) on either computer |
| ASIC: read-only | Stock firmware allows write commands only from localhost: change pools on its web page, or use a gateway agent on the same LAN only if the firmware's `api-allow` permits it |
| No history | It fills while the app is open, one reading a minute |

More: [Protocol](PROTOCOL.md) · [ASICs](ASIC.md) · [Simulation](SIMULATION.md) ·
[Security](../SECURITY.md#remote-management)
