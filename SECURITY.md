# Security

## Reporting a vulnerability

Please don't open public issues for security problems. Use **GitHub › Security › Report a
vulnerability** (private vulnerability reporting) on this repository. Include the version,
platform, steps to reproduce and impact. We aim to acknowledge reports within 7 days.

## Supported versions

Only the latest release receives security fixes.

## Security model (summary)

- **No user secrets.** Only public payout identifiers are stored. Input that looks like a seed
  phrase or a private key is refused.
- **Only verified miners run by default.** Each miner (XMRig, SRBMiner-MULTI, lolMiner,
  cpuminer-opt) is installed from a manifest pinned by SHA-256, with download URLs limited to
  the project's own GitHub releases. A binary picked by hand is marked "not verified". The
  binary's hash is re-checked on every start.
- **No shell.** Processes start with fixed arguments; every config field is validated.
- **Loopback only.** Miner APIs listen on `127.0.0.1` with a random per-session token where the
  miner supports one; config files are `0600`. The agent socket is `0600` and checks the peer
  UID on both sides. No port is opened to the network unless remote control is turned on
  explicitly (below).
- **Minimal Tauri permissions** (`core:default`, file picker, updater, restart, opening a short
  allowlist of payout sites) and a restrictive CSP.
- **No hidden persistence.** Start at login, start mining on launch and the always-on service
  are opt-in. Mining stops when the app quits (unless the user runs the agent as a service);
  orphaned miners are killed on the next start.
- **Signed updates.** The app and the CLI install updates only if their minisign signature
  matches the project key. Signing keys are never stored in the repository.
- **Errors without secrets.** Miner API tokens are never shown or logged.

## Known limits

- Without code signing (Authenticode / Apple notarisation) the operating systems show warnings.
- The GPG signature of XMRig's `SHA256SUMS` is not checked automatically: the manifest script
  verifies the hashes and the maintainer checks the signature by hand.
- SRBMiner-MULTI and lolMiner are closed source: Erebor verifies that the binary is the
  official one, not what it does.

## Remote management

Off until `erebor-agent init`. When on:

- **Mutual TLS 1.3** with self-signed certificates; trust is the SHA-256 fingerprint pinned
  at pairing (agent → list of controllers, controller → agent). No CA, no account, no
  central server, nothing exposed publicly by default (the agent listens where you tell it;
  no router port is ever needed).
- **Pairing** with a one-time code (60 bits, 10 minutes, 5 attempts) proven by HMAC bound to
  both certificates: a machine in the middle can't pair itself.
- **Per-controller credentials, revocable** (`erebor-agent revoke`, effective on the next
  request and on open event streams); **rotation** with `erebor-agent rotate-key`
  (controllers must pair again).
- **Replay**: TLS protects each connection; requests also carry a timestamp checked within
  ±10 minutes.
- **Rate limiting**: 32 connections at most; 10 failed pairings or unauthorized requests from
  an address lock it out for 10 minutes.
- **Explicit command list** only: no shell, no file paths, no executable choice. Remote
  configuration goes through the engine's validation; miner paths and local-only fields
  can't be changed remotely. Profiles are restricted to mining keys.
- **ASIC gateway**: only ASICs configured by the user, private addresses only, scans limited
  to a private /22 and only on request; only capabilities the device declares.
- **Audit log** of pairing, refusals, revocations and every change (`remote/audit.log`).
- **Secrets**: the TLS private key is a 0600 file in a 0700 folder (system service: owned by
  the dedicated user); the OS keychain is not used yet. The controller database holds no
  secrets; exports leave out keys and ASIC pool passwords unless asked.
- **Non-custodial**: only public payout addresses are ever sent; seed phrases and private
  keys are refused as before.

Known limits: the agent's systemd unit and Windows task are not yet verified on real
machines; WhatsMiner write commands (encrypted API) are not implemented; physical ASICs are
untested.
