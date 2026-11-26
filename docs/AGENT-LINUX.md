# Erebor Agent on Linux

The agent mines on the computer and lets paired controllers manage it. It runs as a
systemd service, as an unprivileged user, and keeps mining when the controller is closed,
off or unreachable.

## Install (systemd)

```bash
curl -fsSL https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install-agent.sh -o install-agent.sh
less install-agent.sh                 # read what it does
sh install-agent.sh                   # asks for sudo
# options: --listen 100.64.0.5:7420 (VPN only) · --no-discovery · --version vX.Y.Z · --from-source
```

It installs the signed `erebor` binary of the latest release (SHA-256 checked) in
`/usr/local/bin` with `erebor-agent` linked to it, creates the system user `erebor-agent`
(no login shell; added to `video`/`render` to read GPUs), state in `/var/lib/erebor-agent`
(0700), the unit [`packaging/linux/erebor-agent.service`](../packaging/linux/erebor-agent.service)
(hardened: `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`), turns
remote control on and starts the service.

Manual install from a build:

```bash
cargo build --release --locked -p erebor-cli
sudo install -m 755 target/release/erebor /usr/local/bin/erebor
sudo ln -sf /usr/local/bin/erebor /usr/local/bin/erebor-agent
sudo useradd --system --home-dir /var/lib/erebor-agent --shell /usr/sbin/nologin erebor-agent
sudo install -d -m 700 -o erebor-agent -g erebor-agent /var/lib/erebor-agent
sudo install -m 644 packaging/linux/erebor-agent.service /etc/systemd/system/
sudo -u erebor-agent erebor-agent init
sudo systemctl daemon-reload && sudo systemctl enable --now erebor-agent
```

## Use

```bash
sudo systemctl status erebor-agent
journalctl -u erebor-agent -f                      # logs (journald)
sudo -u erebor-agent erebor-agent pair             # one-time code for a controller
sudo -u erebor-agent erebor-agent status           # remote setup, controllers, running?
sudo -u erebor-agent erebor-agent controllers
sudo -u erebor-agent erebor-agent revoke Laptop
sudo -u erebor-agent erebor-agent asic add 10.0.10.21 --name S19-A   # gateway
sudo systemctl restart erebor-agent                # after init/disable/--listen changes
```

Run the `erebor-agent` commands as the service user (`sudo -u erebor-agent`): as root they
refuse, because files written by root would be unreadable by the service.

Firewall (if enabled): allow TCP and UDP 7420 from your LAN/VPN only, e.g.
`sudo ufw allow from 192.168.1.0/24 to any port 7420`.

Start mining with the computer: set `startOnLaunch` from a controller (device page → Edit
as JSON, or a profile) or `sudo -u erebor-agent erebor --data-dir /var/lib/erebor-agent config set startOnLaunch true`.

## Without systemd

`erebor-agent start` runs in the foreground; use any supervisor (runit, s6, a container).
A per-user service also exists: `erebor agent install-service` (see [AGENT.md](AGENT.md)).

## Uninstall

```bash
sh install-agent.sh --uninstall            # keeps /var/lib/erebor-agent
sh install-agent.sh --uninstall --purge    # deletes it too (keys, statistics)
```

## Updates

The agent checks the signed releases every hour and replaces its binary only when the
minisign signature matches the project key. Under the system service the binary in
`/usr/local/bin` belongs to root, so updates are installed by running the installer again
(same command); the agent restarts and resumes mining. Downgrades: `--version vX.Y.Z`.

Tested: the agent and its remote protocol run on macOS and cross-compile for Linux
(glibc 2.31); the systemd unit and the installer are not yet verified on a real Linux machine.
