# Erebor Agent

The agent is the small background process that **owns the mining engine**
(`erebor_core::engine::Engine`) and therefore the miner processes. The CLI, the TUI and the
desktop app control it; it is the single source of truth for the mining state.

## Start and stop

```bash
erebor agent start               # background, detached from the terminal; log in <data dir>/agent.log
erebor agent start --foreground  # in this terminal (development); Ctrl-C = clean shutdown
erebor agent status              # owner, pid, version, protocol, uptime, socket
erebor agent stop                # stop mining, then the agent
erebor-agent [--data-dir DIR]    # standalone binary, same as "agent start --foreground"
```

You rarely need these: commands that need the agent (`start`, the dashboard, `wallet set`,
`demo`, …) start it by themselves.

- **No surprise mining.** The agent does not mine on start unless `startOnLaunch: true` is
  set (`erebor service on` sets it); real mining still needs a valid payout and miner.
- **Signals.** SIGINT/SIGTERM (and `erebor agent stop`) stop the miner, close the session in
  the statistics, remove the socket and exit with code 0.
- **Single instance.** The engine takes an exclusive lock on `<data dir>/engine.lock`. A
  second agent, or the desktop app on the same folder, fails with `engine_busy` before it can
  touch the miner.
- **Miner crash.** Detected on every tick (2 s), state `error`. It restarts automatically
  only with `safety.autoRestart` (up to 3 attempts, after 10/20/30 s). If the agent itself is
  killed with SIGKILL, the miner PID stays in `miner.pid` and the orphan is killed when the
  engine next starts.
- **Config.** Saved by the engine (`config.json`, atomic write). The CLI changes settings only
  through the agent (`update_config`), with the same validation as the app.

## Local IPC

| | |
|---|---|
| Transport | Unix domain socket `<data dir>/agent.sock` (for very long paths: `$XDG_RUNTIME_DIR` or the user's temp folder, named after a hash of the data folder) |
| Permissions | socket `0600`, data folder `0700` |
| Authentication | on every connection the server checks that the client's UID (`SO_PEERCRED` on Linux, `getpeereid` on macOS) is its own; the client checks the server the same way (`agent_untrusted` otherwise) |
| Protocol | v1: one JSON line per request and one per reply. `{"v":1,"cmd":"get_status","args":{}}` → `{"ok":…}` or `{"err":"code"}` |
| Commands | those of `erebor_core::rpc::call` (the same as the app), plus `agent_info` (answers any protocol version, to diagnose mismatches) and `shutdown` (agent only) |
| Network | none: no TCP port, only the local socket |

Windows: the named-pipe transport is not implemented (`ipc_unsupported_platform`); there the
desktop app runs the engine itself. The protocol is transport-agnostic (`ipc::handle` takes
and returns lines), so a pipe adapter only has to move bytes.

## Who serves the socket

Whoever owns the engine: the agent (`owner: "agent"`) or the desktop app (`"desktop"`). So
`erebor status` works with only the app open. If the agent is running when you open the app,
the app connects to it instead of starting a second engine; a stop from the app shows up in
the CLI and the TUI at once.

## Self-update

Only the release build of `erebor`, on Unix. Every hour (first check after 60 s) the agent
reads `latest.json` from the latest GitHub release, downloads `erebor-<os>-<arch>`, verifies
its minisign signature (listed in `latest.json`) against the project's public key and
replaces the file atomically (write next to it, then `rename`). Every 10 s it checks whether
its own executable changed (update, `erebor update`, reinstall): if so it stops mining
cleanly, writes `<data dir>/resume-mining` if it was mining, and `exec`s the new binary with
the same arguments. Same PID, so systemd/launchd see no exit; the new agent resumes mining
and deletes the file.

## User service (optional)

```bash
erebor service on                # recommended: service + startOnLaunch + linger (always mining)
erebor agent install-service     # service only: ~/.config/systemd/user/org.erebor.agent.service, enable --now
systemctl --user status org.erebor.agent
journalctl --user -u org.erebor.agent
erebor agent uninstall-service
loginctl enable-linger "$USER"   # only to run without logging in
```

A user unit, no root. `Restart=on-failure`: it restarts after a crash, not after a clean stop
(`erebor agent stop`, `systemctl --user stop`). Off by default: installed only by an explicit
command. On macOS: a LaunchAgent with `KeepAlive.SuccessfulExit=false`.

## Logs

`<data dir>/agent.log` (background start) or the journal (service). The miner's log is in
`erebor logs`, `erebor monitor` and the `get_logs` command.
