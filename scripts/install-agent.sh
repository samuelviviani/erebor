#!/bin/sh
# Erebor Agent installer: the background service that mines on this computer and lets
# paired controllers (Erebor Desktop, `erebor devices`) manage it. Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install-agent.sh -o install-agent.sh
#   less install-agent.sh            # read it first
#   sh install-agent.sh              # Linux: asks for sudo; macOS: per user
#
# Linux (systemd): the signed `erebor` binary of the latest release (SHA-256 checked) in
# /usr/local/bin, `erebor-agent` linked to it, a system user `erebor-agent` (no login),
# state in /var/lib/erebor-agent, the unit packaging/linux/erebor-agent.service, remote
# control on (TLS, port 7420). Logs: journalctl -u erebor-agent.
# macOS (launchd): the binary in ~/.local/bin and a LaunchAgent for this user.
#
# Options (after `sh install-agent.sh`):
#   --version vX.Y.Z   a specific release        --from-source   build with Rust here
#   --listen ADDR      e.g. 100.64.0.5:7420 (a VPN address keeps it off the LAN)
#   --no-discovery     don't answer LAN discovery
#   --no-start         install, don't enable the service
#   --uninstall        remove service, user and binary (keeps /var/lib/erebor-agent;
#                      add --purge to delete it too)
#
# Mining never starts by itself: set it up from a paired controller.
set -eu

GITHUB=samuelviviani/erebor
VERSION=""; SOURCE=0; LISTEN=""; DISCOVERY=""; START=1; UNINSTALL=0; PURGE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift ;;
    --from-source) SOURCE=1 ;;
    --listen) LISTEN="$2"; shift ;;
    --no-discovery) DISCOVERY="--no-discovery" ;;
    --no-start) START=0 ;;
    --uninstall) UNINSTALL=1 ;;
    --purge) PURGE=1 ;;
    -h|--help) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

say() { printf '\033[32m◇\033[0m %s\n' "$*"; }
die() { printf '\033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

case "$(uname -s)" in Linux) OS=linux ;; Darwin) OS=macos ;; *) die "unsupported system: on Windows use scripts/install-agent.ps1" ;; esac
case "$(uname -m)" in x86_64|amd64) ARCH=x86_64 ;; arm64|aarch64) ARCH=aarch64 ;; *) die "unsupported CPU $(uname -m)" ;; esac

if [ "$OS" = linux ]; then
  SUDO=""; [ "$(id -u)" = 0 ] || SUDO=sudo
  BIN=/usr/local/bin; DATA=/var/lib/erebor-agent; UNIT=/etc/systemd/system/erebor-agent.service
  command -v systemctl >/dev/null 2>&1 || die "systemd is required (or run 'erebor-agent start' under your own supervisor)"
else
  SUDO=""; BIN="$HOME/.local/bin"; PLIST="$HOME/Library/LaunchAgents/org.erebor.agent.plist"
fi

if [ "$UNINSTALL" = 1 ]; then
  if [ "$OS" = linux ]; then
    $SUDO systemctl disable --now erebor-agent 2>/dev/null || true
    $SUDO rm -f "$UNIT" "$BIN/erebor-agent"; $SUDO systemctl daemon-reload
    [ "$PURGE" = 1 ] && $SUDO rm -rf "$DATA"
    id erebor-agent >/dev/null 2>&1 && $SUDO userdel erebor-agent 2>/dev/null || true
    say "Removed the service$( [ "$PURGE" = 1 ] && echo ' and its data' ). $BIN/erebor stays (remove it by hand if unused)."
  else
    launchctl unload -w "$PLIST" 2>/dev/null || true
    rm -f "$PLIST" "$BIN/erebor-agent"
    say "Removed the LaunchAgent. Data stays in ~/Library/Application Support/org.erebor.desktop."
  fi
  exit 0
fi

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
if [ "$SOURCE" = 1 ]; then
  [ -f ./crates/erebor-cli/Cargo.toml ] || die "--from-source: run this from the repository root"
  command -v cargo >/dev/null 2>&1 || die "Rust is required: https://rustup.rs"
  say "Building erebor (release)"
  cargo build --release --locked -p erebor-cli
  cp target/release/erebor "$TMP/erebor"
else
  command -v curl >/dev/null 2>&1 || die "curl is required"
  URL="https://github.com/$GITHUB/releases/${VERSION:+download/$VERSION}"; [ -n "$VERSION" ] || URL="https://github.com/$GITHUB/releases/latest/download"
  ASSET="erebor-$OS-$ARCH"
  say "Downloading $ASSET ${VERSION:-(latest release)}"
  curl -fsSL --proto '=https' -o "$TMP/erebor" "$URL/$ASSET" || die "download failed: $URL/$ASSET"
  curl -fsSL --proto '=https' -o "$TMP/SHA256SUMS" "$URL/SHA256SUMS" || die "download failed: SHA256SUMS"
  want=$(awk -v f="$ASSET" '$2 == f { print $1 }' "$TMP/SHA256SUMS")
  if command -v sha256sum >/dev/null 2>&1; then got=$(sha256sum "$TMP/erebor" | awk '{print $1}'); else got=$(shasum -a 256 "$TMP/erebor" | awk '{print $1}'); fi
  [ -n "$want" ] && [ "$want" = "$got" ] || die "checksum mismatch for $ASSET: not installed"
  say "SHA-256 verified"
fi

$SUDO mkdir -p "$BIN"
$SUDO install -m 755 "$TMP/erebor" "$BIN/erebor"
$SUDO ln -sf "$BIN/erebor" "$BIN/erebor-agent"
say "Installed $BIN/erebor and $BIN/erebor-agent"

if [ "$OS" = linux ]; then
  id erebor-agent >/dev/null 2>&1 || $SUDO useradd --system --home-dir "$DATA" --no-create-home --shell /usr/sbin/nologin erebor-agent
  for g in video render; do getent group "$g" >/dev/null 2>&1 && $SUDO usermod -aG "$g" erebor-agent || true; done
  $SUDO install -d -m 700 -o erebor-agent -g erebor-agent "$DATA"
  here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo .)
  if [ -f "$here/../packaging/linux/erebor-agent.service" ]; then
    $SUDO install -m 644 "$here/../packaging/linux/erebor-agent.service" "$UNIT"
  else
    curl -fsSL --proto '=https' -o "$TMP/unit" "https://raw.githubusercontent.com/$GITHUB/main/packaging/linux/erebor-agent.service" || die "download failed: unit file"
    $SUDO install -m 644 "$TMP/unit" "$UNIT"
  fi
  $SUDO -u erebor-agent "$BIN/erebor-agent" init ${LISTEN:+--listen "$LISTEN"} $DISCOVERY
  $SUDO systemctl daemon-reload
  if [ "$START" = 1 ]; then
    $SUDO systemctl enable --now erebor-agent
    say "Service running: sudo systemctl status erebor-agent · logs: journalctl -u erebor-agent -f"
  fi
  printf '\nPair it with a controller:\n  sudo -u erebor-agent erebor-agent pair\n\n'
else
  "$BIN/erebor-agent" init ${LISTEN:+--listen "$LISTEN"} $DISCOVERY
  mkdir -p "$(dirname "$PLIST")" "$HOME/Library/Logs"
  here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo .)
  src="$here/../packaging/macos/org.erebor.agent.plist"
  [ -f "$src" ] || { curl -fsSL --proto '=https' -o "$TMP/plist" "https://raw.githubusercontent.com/$GITHUB/main/packaging/macos/org.erebor.agent.plist"; src="$TMP/plist"; }
  sed -e "s#__BIN__#$BIN#g" -e "s#__HOME__#$HOME#g" "$src" > "$PLIST"
  if [ "$START" = 1 ]; then
    launchctl unload "$PLIST" 2>/dev/null || true
    launchctl load -w "$PLIST"
    say "LaunchAgent loaded · logs: ~/Library/Logs/erebor-agent.log"
  fi
  printf '\nPair it with a controller:\n  %s/erebor-agent pair\n\n' "$BIN"
fi
