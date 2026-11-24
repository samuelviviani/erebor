#!/bin/sh
# Erebor installer (macOS, Linux, WSL2). One command:
#
#   curl -fsSL https://raw.githubusercontent.com/samuelviviani/erebor/main/scripts/install.sh | sh
#
# Downloads the signed `erebor` binary of the latest GitHub release for this system,
# checks its SHA-256, installs it in ~/.local/bin (no root) and starts the guided setup.
# Afterwards it updates itself (`erebor update`).
#
# Options (after `| sh -s --`):
#   --version vX.Y.Z  a specific release instead of the latest
#   --no-onboard      install only; run `erebor onboard` later
#   --prefix DIR      install into DIR/bin (default: ~/.local)
#   --from-source     build with Rust instead of downloading (also --repo URL, --ref REF)
#   --yes             no questions (from source: installs Rust with rustup if missing)
#
# What it changes: one file in <prefix>/bin. From source also: Rust via rustup if
# missing (asks first) and the sources in ~/.local/share/erebor/src. No root, no
# services, no mining: the setup asks before doing anything.
set -eu

GITHUB=samuelviviani/erebor
ONBOARD=1
PREFIX="${HOME}/.local"
REPO="${EREBOR_REPO_URL:-https://github.com/$GITHUB.git}"
REF=""
YES=0
SOURCE=0
VERSION=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-onboard) ONBOARD=0 ;;
    --prefix) PREFIX="$2"; shift ;;
    --repo) REPO="$2"; SOURCE=1; shift ;;
    --ref) REF="$2"; SOURCE=1; shift ;;
    --from-source) SOURCE=1 ;;
    --version) VERSION="$2"; shift ;;
    --yes|-y) YES=1 ;;
    -h|--help) sed -n '2,22p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

say() { printf '\033[32m◇\033[0m %s\n' "$*"; }
die() { printf '\033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }
# Questions read the terminal, also when this script itself comes from `curl | sh`.
has_tty() { (exec </dev/tty) 2>/dev/null; }
ask() {
  [ "$YES" = 1 ] && return 0
  has_tty || return 1
  printf '%s [Y/n] ' "$1"
  read -r a </dev/tty || return 1
  case "$a" in n|N|no|NO) return 1 ;; *) return 0 ;; esac
}

printf '\n  \033[1;32mEREBOR\033[0m  ·  installer\n\n'

case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *) die "unsupported system $(uname -s): on Windows use the desktop app (or WSL2)" ;;
esac
case "$(uname -m)" in
  x86_64|amd64) ARCH=x86_64 ;;
  arm64|aarch64) ARCH=aarch64 ;;
  *) die "unsupported CPU $(uname -m)" ;;
esac
say "System: $OS $ARCH"

finish() {
  say "Installed: $PREFIX/bin/erebor ($("$PREFIX/bin/erebor" --version))"
  case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *)
      shell_rc="$HOME/.profile"
      case "${SHELL:-}" in */zsh) shell_rc="$HOME/.zshrc" ;; */bash) shell_rc="$HOME/.bashrc" ;; esac
      printf '\n  %s is not in your PATH. Add it with:\n    echo '\''export PATH="%s/bin:$PATH"'\'' >> %s\n\n' \
        "$PREFIX/bin" "$PREFIX" "$shell_rc"
      ;;
  esac
  if [ "$ONBOARD" = 1 ] && has_tty; then
    say "Starting the guided setup"
    exec "$PREFIX/bin/erebor" onboard </dev/tty
  fi
  printf '\nNext: %s/bin/erebor onboard\n' "$PREFIX"
  exit 0
}

if [ "$SOURCE" = 0 ]; then
  command -v curl >/dev/null 2>&1 || die "curl is required"
  if [ -n "$VERSION" ]; then
    URL="https://github.com/$GITHUB/releases/download/$VERSION"
  else
    URL="https://github.com/$GITHUB/releases/latest/download"
  fi
  ASSET="erebor-$OS-$ARCH"
  TMP=$(mktemp -d)
  trap 'rm -rf "$TMP"' EXIT
  say "Downloading $ASSET ${VERSION:-(latest release)}"
  curl -fsSL --proto '=https' -o "$TMP/erebor" "$URL/$ASSET" || die "download failed: $URL/$ASSET"
  curl -fsSL --proto '=https' -o "$TMP/SHA256SUMS" "$URL/SHA256SUMS" || die "download failed: $URL/SHA256SUMS"
  want=$(awk -v f="$ASSET" '$2 == f { print $1 }' "$TMP/SHA256SUMS")
  if command -v sha256sum >/dev/null 2>&1; then
    got=$(sha256sum "$TMP/erebor" | awk '{ print $1 }')
  else
    got=$(shasum -a 256 "$TMP/erebor" | awk '{ print $1 }')
  fi
  [ -n "$want" ] && [ "$want" = "$got" ] || die "checksum mismatch for $ASSET: not installed"
  say "SHA-256 verified"
  mkdir -p "$PREFIX/bin"
  install -m 755 "$TMP/erebor" "$PREFIX/bin/erebor"
  finish
fi

# C compiler: the bundled SQLite and TLS code are compiled from source.
if ! command -v cc >/dev/null 2>&1; then
  if [ "$OS" = linux ]; then
    die "no C compiler. Install it, then run this again:  sudo apt install -y build-essential   (Fedora: sudo dnf install gcc)"
  else
    die "no C compiler. Install the command line tools, then run this again:  xcode-select --install"
  fi
fi

# Rust toolchain.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if ! command -v cargo >/dev/null 2>&1; then
  ask "Rust is needed to build Erebor. Install it now with rustup (official installer, in your home)?" \
    || die "Rust is required: https://rustup.rs"
  command -v curl >/dev/null 2>&1 || die "curl is required to install Rust"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  . "$HOME/.cargo/env"
fi
say "Rust: $(cargo --version)"

# Sources: this checkout, or a clone.
SRC=""
here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || true)
if [ -n "$here" ] && [ -f "$here/../crates/erebor-cli/Cargo.toml" ]; then
  SRC=$(cd "$here/.." && pwd)
elif [ -f "./crates/erebor-cli/Cargo.toml" ]; then
  SRC=$(pwd)
else
  command -v git >/dev/null 2>&1 || die "git is required to download the sources"
  SRC="${HOME}/.local/share/erebor/src"
  if [ -d "$SRC/.git" ]; then
    say "Updating sources in $SRC"
    git -C "$SRC" fetch --quiet origin
    git -C "$SRC" checkout --quiet "${REF:-origin/HEAD}"
  else
    say "Downloading sources into $SRC"
    mkdir -p "$(dirname "$SRC")"
    git clone --quiet ${REF:+--branch "$REF"} "$REPO" "$SRC"
  fi
fi
say "Sources: $SRC"

say "Building erebor (release, a few minutes the first time)"
(cd "$SRC" && cargo build --release --locked -p erebor-cli -p erebor-agent)

mkdir -p "$PREFIX/bin"
install -m 755 "$SRC/target/release/erebor" "$SRC/target/release/erebor-agent" "$PREFIX/bin/"
finish
