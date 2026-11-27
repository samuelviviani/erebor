# ASICs

Erebor reads and controls ASICs through **their own firmware API**: nothing is installed on
the ASIC, no firmware is flashed, no web login is automated, no voltage, frequency or power
setting is touched. Code: `crates/erebor-core/src/asic.rs`; simulator: `asic_sim.rs`.

> **Status: verified only against the documented API formats and the simulator.** No
> physical ASIC has been tested yet. Treat every adapter as unverified until it has been
> run on real hardware (please report results in an issue: model, firmware, what worked).

## Protocol

The CGMiner API: TCP port 4028, send `{"command":"summary"}` (or with `"parameter"`), read
one JSON reply until the device closes the connection. Bitmain's BMMiner and MicroBT's
btminer speak dialects of it. The reader tolerates the known quirks: a trailing NUL byte and
`}{` between objects in old BMMiner `stats`.

Commands used: `version` (or `get_version`), `summary`, `stats` (BMMiner), `devs` and
`devdetails` (btminer), `pools`, `privileged`; writes: `restart`, `addpool`, `switchpool`,
`removepool` (only to undo a failed switch).

## Adapters and capabilities

Each read declares what the device really allows (`capabilities` with a reason when not):

| | Monitor | Restart | Set pool | Start / stop |
|---|---|---|---|---|
| Antminer (BMMiner/CGMiner API) | ✓ | if `privileged` is allowed | if `privileged` is allowed | ✗ `asic_no_stop_command` |
| WhatsMiner (btminer API 2.x) | ✓ | ✗ `whatsminer_write_api_not_implemented` | ✗ | ✗ |
| Other CGMiner-compatible (Avalon, Innosilicon, Braiins/LuxOS compat layer, …) | ✓ | if allowed | if allowed | ✗ |

- **Privileged access.** Stock Antminer firmware usually sets `api-allow` so that write
  commands work only from localhost: then Erebor shows the ASIC read-only and says so. Pools
  must then be changed on the ASIC's web page.
- **No stop.** The CGMiner API's `quit` ends the miner process, which the firmware's watchdog
  restarts: not a real stop, so Erebor doesn't offer it.
- **WhatsMiner writes** need a token derived from the admin password and AES-encrypted
  commands (API 2.0.x). Not implemented: monitoring only.
- **Model ≠ API.** Models of the same vendor differ (old S9 vs S19 vs S21 firmware, btminer
  API 1/2/3). The adapter reads what the device sends (`Type`, `Model`, `GHS 5s`/`MHS 5s`,
  `chain_rateN`, `temp2_N` or `temp_chipN`, `fanN`) and leaves out what isn't there.

Read: model, firmware, API version, hashrate (5 s and average), accepted/rejected shares,
hardware errors, uptime, per-hashboard hashrate/board temperature/chip temperature/chips,
fan speeds, highest temperature, power (only if the firmware reports it), pools (URL,
user, status, active). Algorithm from the model family where unambiguous (Antminer S/T →
SHA-256d, L → Scrypt, KS → kHeavyHash, E → Etchash; WhatsMiner M → SHA-256d).

## Bitcoin with ASICs

Payouts go to the address in the pool's **user** field (`bc1q….worker` or
`account.worker`), set through "Set pool" or a profile with an ASIC pool. The new pool is
added and switched to; previous pools stay as fallbacks. Erebor distinguishes the coin
mined, the algorithm (SHA-256d) and the payout coin (the pool decides, e.g. a pool that pays
BTC for another SHA-256d coin). CPUs and GPUs are never offered as a way to mine Bitcoin
profitably. Revenue estimates for SHA-256d/Scrypt ASICs use the same public market data as
the rest of the app (BTC, LTC) and are labelled as estimates.

## Adding ASICs

- Directly: Devices → Add device → ASIC → IP (private ranges only) and port.
- Through a gateway: choose the gateway agent in "Reached through".
- Scan: a private subnet up to /22, from this computer or a gateway. Explicit only; nothing
  is added by itself.

The same ASIC is one device whichever path reaches it (id `asic:<ip>:<port>`): adding it
twice is refused. Limit: the same ASIC behind NAT under two different addresses would not be
recognised as one.

## Gateway

An agent on a server inside the farm watches the ASICs configured on it and forwards only
the actions above, so the ASICs' APIs never have to be reachable from outside the farm
network.

```bash
# On the farm server (Linux service):
sudo -u erebor-agent erebor-agent asic add 10.0.10.21 --name S19-A
sudo -u erebor-agent erebor-agent asic list
# or from the controller:
erebor devices add 10.0.10.22 --asic --name S19-B --via farm-gateway
erebor devices discover --asic 10.0.10.0/24 --via farm-gateway
```

The gateway reads every configured ASIC every 15 s (in parallel) and streams the results to
its controllers. It refuses public addresses, unknown ASIC ids and actions the ASIC doesn't
support.

## Writing an adapter

1. Add the vendor to `Vendor` and its detection in `detect()` (from `version`/`get_version`).
2. Map its replies in `read()`; leave fields `None` when the device doesn't send them.
3. Declare honest `capabilities()`: unsupported → `no("reason_code")`, and add the reason's
   text to `apps/desktop/src/lib/i18n/` (`err.<code>`, English and Italian).
4. Add the dialect to `asic_sim.rs` and tests in `asic.rs` (read, each supported action,
   refusals).
5. Document the firmware versions it was written against, and mark it unverified until
   tested on hardware.
