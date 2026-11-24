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
  UID on both sides. No port is opened to the network.
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
