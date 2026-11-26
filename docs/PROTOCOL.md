# Remote protocol (v1)

Between a controller (Erebor Desktop, `erebor devices`) and `erebor-agent`. Implemented in
`crates/erebor-core/src/remote/` (`server.rs`, `client.rs`); shared types in
`crates/erebor-core/src/fleet.rs` (`Telemetry`, `Snapshot`) and `asic.rs` (`AsicStatus`).

## Transport and trust

- TCP, default port **7420** (`remote/config.json` → `listen`). TLS **1.3 only**, rustls
  with the ring provider.
- Both sides present a self-signed certificate (ECDSA P-256, generated on first use,
  `<data dir>/remote/cert.der` + `key.der`, mode 0600). Identity = SHA-256 of the DER
  certificate ("fingerprint"; short form: first 16 hex digits in groups of four).
- The agent accepts any client certificate at the TLS layer, then authorizes by
  fingerprint against `remote/controllers.json`. The controller accepts only the pinned
  agent fingerprint (`fingerprint_mismatch` otherwise), except while pairing.
- Why not a CA or HTTPS with an account: no central service, nothing to register.

## Framing

One JSON object per line (UTF-8, `\n`, max 1 MiB), many requests per connection, idle
timeout 300 s.

Request:

```json
{"v": 1, "id": "9f2c1a7e5b3d0c11", "ts": 1795680000, "cmd": "mining.start", "args": {}}
```

Reply (`id` echoed): `{"id": "…", "ok": <value>}` or `{"id": "…", "err": "<error code>"}`.

`ts` must be within ±600 s of the agent's clock (`clock_skew`), so a captured request
can't be replayed later; inside a connection TLS already prevents replay.

## Version negotiation

`hello` is answered for any `v` and any peer:

```json
{"protocol": 1, "minProtocol": 1, "agentVersion": "0.1.7", "deviceId": "…",
 "paired": true, "pairing": false, "time": 1795680000}
```

The client speaks `min(its protocol, agent protocol)` and refuses agents whose range doesn't
overlap its own (`protocol_incompatible`). Other commands with an unsupported `v` get
`protocol_version`. New optional fields don't bump the version; removing or changing the
meaning of a field does.

## Pairing

The agent creates a code with `erebor-agent pair`: 12 symbols of Crockford base32 (60 bits),
`remote/pairing.json`, 10 minutes, 5 attempts. Input is normalised (case, `-`, `O→0`,
`I/L→1`).

```
proof(role) = hex(HMAC-SHA256(key = code, "erebor-pair-v1|" + role + "|" + agentFp + "|" + controllerFp))
```

1. Controller connects without a pin, learns `agentFp`; its own `controllerFp` is the TLS
   client certificate.
2. → `{"cmd":"pair","args":{"name":"Laptop","proof": proof("controller")}}`
3. Agent recomputes with the fingerprints *it* sees; mismatch → `pairing_failed` (attempt
   counted). Match → controller trusted, code deleted, reply
   `{"proof": proof("agent"), "device": {"id","name"}}`.
4. Controller checks the agent's proof (`pairing_agent_proof_invalid`), then pins `agentFp`.

A machine in the middle would present other fingerprints, so its proofs never match. With
60 bits an offline guess of a captured proof is impractical within the 10-minute window.

## Commands

Unauthenticated: `hello`, `pair`. Any other command from an untrusted certificate:
`unauthorized`, the connection is closed, the attempt is audited and counted (10 failures in
10 minutes lock that address out).

| Command | Args | Result | REST equivalent |
|---|---|---|---|
| `device` | | `{device, hostname, agentVersion, protocol, fingerprint, hardware, gateway}` | `GET /api/v1/device` |
| `health` | | `{status:"ok", agentVersion, protocol, uptimeSecs}` | `GET /api/v1/health` |
| `hardware` | `{refresh?}` | `HardwareInfo` (cached 10 min) | `GET /api/v1/hardware` |
| `telemetry` | | `Telemetry` (below) | `GET /api/v1/telemetry` |
| `mining.status` | | `Snapshot` | `GET /api/v1/mining/status` |
| `mining.config` | | the configuration (`config.json` shape) | `GET /api/v1/mining/config` |
| `mining.config.set` | `{config}` | `{state:"completed", config}`; validated by the engine, miner paths and local-only fields kept | `PUT /api/v1/mining/config` |
| `mining.start` / `.stop` / `.restart` | | `{state:"completed", changed, snapshot}`; error if the engine didn't reach the state | `POST /api/v1/mining/{start,stop,restart}` |
| `miners` | | per miner: `kind, installed, version, verified, pinnedVersion, availableHere` | `GET /api/v1/miners` |
| `miners.install` | `{kind}` | `{state:"accepted"}` (pinned, SHA-256-verified build; progress in `snapshot.job`) | |
| `coins` | | coin options with blockers and estimates | |
| `logs` | | miner log lines | `GET /api/v1/logs` |
| `stats` | `{rangeSecs}` | statistics report | |
| `device.rename` | `{name}` | device | |
| `asics` | | `AsicStatus[]` of the ASICs this gateway watches | |
| `asics.add` / `.remove` | `{asic:{host,port,name}}` / `{id}` | status / null; private IPs only | |
| `asics.action` | `{id, action:{action:"restart"} \| {action:"setPool",url,user,password} \| {action:"switchPool",index}}` | `{state:"accepted"\|"completed"}` | |
| `asics.scan` | `{cidr, port?}` | found `{host,port}`; private, ≤ /22 | |
| `controllers` | | trusted controllers (short fingerprints) | |
| `audit` | | last 200 audit entries | |
| `unpair` | | removes the calling controller | |
| `subscribe` | `{intervalSecs?}` (1–60, default 3) | `null`, then events | WebSocket equivalent |

Anything else: `unknown_command`. There is no command that runs a shell, reads or writes an
arbitrary file, or chooses an executable.

Command results: synchronous commands return `completed` or an error; long jobs return
`accepted` and report progress in telemetry (`snapshot.job`). Controllers present them as
completed / failed / skipped / rolled_back per device.

## Event stream

After `subscribe` the agent writes events; the client sends nothing else on that
connection. It ends when the client disconnects or is revoked (`{"event":"revoked"}`).

```json
{"event": "telemetry", "ts": 1795680000, "device": "<device id>", "data": <Telemetry>}
{"event": "state", "ts": …, "device": "…", "data": {"from": "stopped", "to": "running"}}
{"event": "alert", "ts": …, "device": "…", "data": {"id", "ts", "level", "code", "detail"}}
{"event": "asics", "ts": …, "device": "…", "data": [<AsicStatus>…]}
```

Controllers keep one stream per agent and reconnect with exponential backoff (1 s → 60 s)
and ±50% jitter.

## Telemetry

`hostname, device{id,name}, os, arch, agentVersion, addresses[], systemUptimeSecs,
cpuModel, cpuUsage, memUsedBytes, memTotalBytes, gpus[{name, vendor, vramMb, memUsedMb,
utilPercent, tempC, hotspotC, fanPercent, coreClockMhz, memClockMhz, powerW, hashrate}],
snapshot{state, demo, error, startBlocker, miner, coin, coinName, algo, autoCoin, poolId,
pool, poolConnected, hashrate, uptimeSecs, accepted, rejected, cpuTempC, gpus, powerW,
powerSource ("measured"|"estimated"|"unavailable"), devFee, earnings, restartInSecs, job},
simulated, recentErrors[]`.

Unavailable values are `null` (today `hotspotC` and the clocks are always `null`: not read
on any platform yet). Invalid shares are counted by the pools Erebor uses as rejected.

## Discovery (optional)

UDP on the same port. The controller broadcasts `EREBOR-DISCOVER/1`; agents with discovery
on answer probes from private addresses only, with
`{"erebor":1,"id","name","port","version","fingerprint","pairing"}`. Off with
`erebor-agent init --no-discovery` (agent) or Settings → Devices (controller). mDNS/DNS-SD
was considered; a single broadcast needs no extra dependency and covers the same LAN case.

## Audit log

`<data dir>/remote/audit.log`, JSON lines `{ts, controller, peer, cmd, ok}` for pairing,
unauthorized attempts, revocations and every command that changes something; rotated at
1 MiB (one old file kept).
