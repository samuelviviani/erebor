#!/usr/bin/env bash
# Local release from a Mac, without GitHub Actions: macOS (universal) + Windows (NSIS,
# cross-compiled) + signed CLI binaries for macOS and Linux, latest.json, SHA256SUMS, and a DRAFT
# GitHub release. Only what people download or the updaters read is uploaded: the update
# signatures live in latest.json. Linux desktop (.deb) needs a Linux machine: build it there
# and add it with `gh release upload` (and a "linux-x86_64-deb" entry in latest.json).
#
#   EREBOR_DEV_FEE_BTC=bc1... [EREBOR_DEV_FEE_WALLET=krx...] scripts/release-local.sh 0.1.3
#
# Erebor's 2% goes to EREBOR_DEV_FEE_BTC on unMineable for every coin unMineable mines,
# and to the EREBOR_DEV_FEE_WALLET Kryptex account for the others (if set).
#
# One-time setup: brew install nsis llvm lld zig; cargo install --locked cargo-xwin cargo-zigbuild;
# rustup target add x86_64-apple-darwin x86_64-pc-windows-msvc x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
set -euo pipefail

V="${1:?usage: release-local.sh <version>}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO=samuelviviani/erebor
URL="https://github.com/$REPO/releases/download/v$V"
OUT="$ROOT/target/release-$V"

grep -q "\"version\": \"$V\"" "$ROOT/apps/desktop/src-tauri/tauri.conf.json" \
  || { echo "tauri.conf.json is not at version $V"; exit 1; }
[ -n "${EREBOR_DEV_FEE_BTC:-}${EREBOR_DEV_FEE_WALLET:-}" ] \
  || { echo "set EREBOR_DEV_FEE_BTC (bc1…) and/or EREBOR_DEV_FEE_WALLET (krx…)"; exit 1; }
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
SIGS="$ROOT/target/release-$V-sigs"
rm -rf "$SIGS" && mkdir -p "$SIGS"
cp "$MAC/dmg/Erebor_${V}_universal.dmg" "$OUT/"
cp "$MAC/macos/Erebor.app.tar.gz" "$OUT/Erebor_${V}_universal.app.tar.gz"
cp "$MAC/macos/Erebor.app.tar.gz.sig" "$SIGS/Erebor_${V}_universal.app.tar.gz.sig"
cp "$WIN/Erebor_${V}_x64-setup.exe" "$OUT/"
cp "$WIN/Erebor_${V}_x64-setup.exe.sig" "$SIGS/"

# CLI: one binary per OS/CPU, same names and key the CLI self-update expects.
# Linux via zig, linked against glibc 2.31 (Ubuntu 20.04+).
cd "$ROOT"
for t in aarch64 x86_64; do
  cargo build --release --locked -p erebor-cli --target "$t-apple-darwin"
  cp "target/$t-apple-darwin/release/erebor" "$OUT/erebor-macos-$t"
  cargo zigbuild --release --locked -p erebor-cli --target "$t-unknown-linux-gnu.2.31"
  cp "target/$t-unknown-linux-gnu/release/erebor" "$OUT/erebor-linux-$t"
  for a in "macos-$t" "linux-$t"; do
    (cd apps/desktop && pnpm tauri signer sign "$OUT/erebor-$a" >/dev/null)
    mv "$OUT/erebor-$a.sig" "$SIGS/"
  done
done

# latest.json: read by the desktop updater, and by the CLI for "version" and (since 0.1.6)
# its own signatures ("cli").
APP="Erebor_${V}_universal.app.tar.gz"
EXE="Erebor_${V}_x64-setup.exe"
# The app shows these notes under "What's new": this version's CHANGELOG section.
CHANGES="$SIGS/changes.md"
awk -v v="$V" '/^## \[/{on = index($0, "## [" v "]") == 1; next} on' "$ROOT/CHANGELOG.md" > "$CHANGES"
[ -s "$CHANGES" ] || { echo "CHANGELOG.md has no section for $V"; exit 1; }
node -e '
const fs = require("fs");
const [v, url, app, exe, sigs, changes] = process.argv.slice(1);
const sig = f => fs.readFileSync(`${sigs}/${f}.sig`, "utf8").trim();
const mac = { url: `${url}/${app}`, signature: sig(app) };
const cli = {};
for (const a of ["macos-aarch64", "macos-x86_64", "linux-aarch64", "linux-x86_64"])
  cli[`erebor-${a}`] = sig(`erebor-${a}`);
console.log(JSON.stringify({ version: v, notes: fs.readFileSync(changes, "utf8").trim(), pub_date: new Date().toISOString(),
  platforms: { "darwin-aarch64": mac, "darwin-x86_64": mac,
    "windows-x86_64": { url: `${url}/${exe}`, signature: sig(exe) } }, cli }, null, 2));
' "$V" "$URL" "$APP" "$EXE" "$SIGS" "$CHANGES" > "$OUT/latest.json"

# ponytail: CLIs up to 0.1.5 read erebor-<os>-<arch>.sig from the release: upload them
# while such CLIs may still be around, then delete this line.
cp "$SIGS"/erebor-*.sig "$OUT/"

(cd "$OUT" && shasum -a 256 * > SHA256SUMS)

NOTES="$SIGS/notes.md"
cat > "$NOTES" <<NOTES_EOF
## Download

| System | File |
|---|---|
| macOS (Apple Silicon and Intel) | [Erebor_${V}_universal.dmg]($URL/Erebor_${V}_universal.dmg) |
| Windows 10/11 | [Erebor_${V}_x64-setup.exe]($URL/Erebor_${V}_x64-setup.exe) |
| Terminal (macOS, Linux) | \`curl -fsSL https://raw.githubusercontent.com/$REPO/main/scripts/install.sh \| sh\` |

Already installed? Erebor updates itself (terminal: \`erebor update\`).

The other files are read by the automatic updates; \`SHA256SUMS\` lists every checksum.
Changes: [CHANGELOG.md](https://github.com/$REPO/blob/v$V/CHANGELOG.md).
NOTES_EOF

gh release create "v$V" "$OUT"/* --repo "$REPO" --draft --target "$(git rev-parse HEAD)" --title "Erebor v$V" \
  --notes-file "$NOTES"
echo "Draft release ready: https://github.com/$REPO/releases (publish it to start auto-updates)"
