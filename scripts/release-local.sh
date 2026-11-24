#!/usr/bin/env bash
# Local release from a Mac, without GitHub Actions: macOS (universal) + Windows (NSIS,
# cross-compiled) + signed CLI binaries for macOS, latest.json, SHA256SUMS, and a DRAFT
# GitHub release. Linux (.deb/AppImage) needs a Linux machine: build it there and add the
# files with `gh release upload` (and its entry to latest.json).
#
#   EREBOR_DEV_FEE_WALLET=krx... scripts/release-local.sh 0.1.3
#
# One-time setup: brew install nsis llvm lld; cargo install --locked cargo-xwin;
# rustup target add x86_64-apple-darwin x86_64-pc-windows-msvc
set -euo pipefail

V="${1:?usage: release-local.sh <version>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO=samuelviviani/erebor
URL="https://github.com/$REPO/releases/download/v$V"
OUT="$ROOT/target/release-$V"

grep -q "\"version\": \"$V\"" "$ROOT/apps/desktop/src-tauri/tauri.conf.json" \
  || { echo "tauri.conf.json is not at version $V"; exit 1; }
[ -n "${EREBOR_DEV_FEE_WALLET:-}" ] || { echo "EREBOR_DEV_FEE_WALLET not set"; exit 1; }
git -C "$ROOT" diff --quiet HEAD || { echo "commit and push first: the tag points at HEAD"; exit 1; }

export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH"
export EREBOR_REPO_URL="https://github.com/$REPO"
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/erebor.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

cd "$ROOT/apps/desktop"
pnpm tauri build --target universal-apple-darwin --bundles app,dmg
pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis

rm -rf "$OUT" && mkdir -p "$OUT"
MAC="$ROOT/target/universal-apple-darwin/release/bundle"
WIN="$ROOT/target/x86_64-pc-windows-msvc/release/bundle/nsis"
cp "$MAC/dmg/Erebor_${V}_universal.dmg" "$OUT/"
cp "$MAC/macos/Erebor.app.tar.gz" "$OUT/Erebor_${V}_universal.app.tar.gz"
cp "$MAC/macos/Erebor.app.tar.gz.sig" "$OUT/Erebor_${V}_universal.app.tar.gz.sig"
cp "$WIN/Erebor_${V}_x64-setup.exe" "$WIN/Erebor_${V}_x64-setup.exe.sig" "$OUT/"

# CLI: one binary per macOS CPU, same names and key the CLI self-update expects.
cd "$ROOT"
for t in aarch64 x86_64; do
  cargo build --release --locked -p erebor-cli --target "$t-apple-darwin"
  cp "target/$t-apple-darwin/release/erebor" "$OUT/erebor-macos-$t"
  (cd apps/desktop && pnpm tauri signer sign "$OUT/erebor-macos-$t" >/dev/null)
done

# latest.json read by the desktop updater (and, for "version", by the CLI).
sig() { cat "$OUT/$1.sig"; }
APP="Erebor_${V}_universal.app.tar.gz"
EXE="Erebor_${V}_x64-setup.exe"
node -e '
const [v, url, app, appSig, exe, exeSig] = process.argv.slice(1);
const mac = { url: `${url}/${app}`, signature: appSig };
console.log(JSON.stringify({ version: v, notes: `See CHANGELOG.md for ${v}`, pub_date: new Date().toISOString(),
  platforms: { "darwin-aarch64": mac, "darwin-x86_64": mac,
    "windows-x86_64": { url: `${url}/${exe}`, signature: exeSig } } }, null, 2));
' "$V" "$URL" "$APP" "$(sig "$APP")" "$EXE" "$(sig "$EXE")" > "$OUT/latest.json"

(cd "$OUT" && shasum -a 256 * > SHA256SUMS)

gh release create "v$V" "$OUT"/* --repo "$REPO" --draft --target "$(git rev-parse HEAD)" --title "Erebor v$V" \
  --notes "Draft: Linux build to add. See CHANGELOG.md."
echo "Draft release ready: https://github.com/$REPO/releases (publish it to start auto-updates)"
