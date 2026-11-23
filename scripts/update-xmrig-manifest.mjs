#!/usr/bin/env node
// Pin an official XMRig release in crates/erebor-core/xmrig-manifest.json.
// Hashes come from the upstream SHA256SUMS file of that release; each archive is
// also downloaded and re-hashed locally so a mismatch aborts the update.
//
//   node scripts/update-xmrig-manifest.mjs 6.22.2
//
// Review the resulting diff (and ideally the upstream GPG signature of SHA256SUMS)
// before committing.
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("usage: update-xmrig-manifest.mjs <version>  (e.g. 6.22.2)");
  process.exit(1);
}
const base = `https://github.com/xmrig/xmrig/releases/download/v${version}/`;
// Erebor platform key (Rust std::env::consts OS-ARCH) -> upstream asset name.
const ASSETS = {
  "linux-x86_64": `xmrig-${version}-linux-static-x64.tar.gz`,
  "windows-x86_64": `xmrig-${version}-windows-x64.zip`,
  "macos-x86_64": `xmrig-${version}-macos-x64.tar.gz`,
  "macos-aarch64": `xmrig-${version}-macos-arm64.tar.gz`,
};

const sumsRes = await fetch(base + "SHA256SUMS");
if (!sumsRes.ok) throw new Error(`SHA256SUMS: HTTP ${sumsRes.status}`);
const sums = Object.fromEntries(
  (await sumsRes.text())
    .split("\n")
    .map((l) => l.trim().split(/\s+\*?/))
    .filter((p) => p.length === 2)
    .map(([h, f]) => [f, h.toLowerCase()]),
);

const assets = {};
for (const [key, file] of Object.entries(ASSETS)) {
  const expected = sums[file];
  if (!expected) {
    console.warn(`skip ${key}: ${file} not in SHA256SUMS`);
    continue;
  }
  const res = await fetch(base + file);
  if (!res.ok) throw new Error(`${file}: HTTP ${res.status}`);
  const got = createHash("sha256").update(Buffer.from(await res.arrayBuffer())).digest("hex");
  if (got !== expected) throw new Error(`${file}: hash mismatch ${got} != ${expected}`);
  assets[key] = { url: base + file, sha256: expected };
  console.log(`ok ${key} ${expected}`);
}

const out = new URL("../crates/erebor-core/xmrig-manifest.json", import.meta.url);
writeFileSync(
  out,
  JSON.stringify({ version, source: `https://github.com/xmrig/xmrig/releases/tag/v${version}`, assets }, null, 2) + "\n",
);
console.log(`wrote ${out.pathname}`);
