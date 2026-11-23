#!/bin/sh
# Erebor installer (macOS, Linux, WSL2): builds the `erebor` CLI and agent from source,
# installs them in ~/.local/bin (no root) and starts the guided setup.
#
#   sh scripts/install.sh                      # from a checkout of the repository
#   curl -fsSL <raw URL>/scripts/install.sh | sh -s -- --repo <git URL>
#
# Options:
#   --no-onboard     install only; run `erebor onboard` later
#   --prefix DIR     install into DIR/bin (default: ~/.local)
#   --repo URL       git repository to clone when not run from a checkout ($EREBOR_REPO_URL)
#   --ref REF        branch or tag to build (default: the repository default)
#   --yes            no questions (installs Rust with rustup if it is missing)
#
# What it changes: Rust via rustup if missing (asks first), the sources in
# ~/.local/share/erebor/src when cloning, two files in <prefix>/bin. Nothing else:
# no root, no services, no mining. The setup asks before doing anything.
set -eu

ONBOARD=1
PREFIX="${HOME}/.local"
REPO="${EREBOR_REPO_URL:-}"
REF=""
YES=0
while [ $# -gt 0 ]; do
  case "$1" in
    --no-onboard) ONBOARD=0 ;;
    --prefix) PREFIX="$2"; shift ;;
    --repo) REPO="$2"; shift ;;
    --ref) REF="$2"; shift ;;
    --yes|-y) YES=1 ;;
    -h|--help) sed -n '2,19p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'; exit 0 ;;
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
say "System: $OS $(uname -m)"

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
  [ -n "$REPO" ] || die "not run from a checkout: pass --repo <git URL> (or set EREBOR_REPO_URL)"
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
