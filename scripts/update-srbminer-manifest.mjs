#!/usr/bin/env node
// Pin an official SRBMiner-MULTI release in crates/erebor-core/srbminer-manifest.json.
// Upstream publishes MD5 hashes only (release notes and *.md5 assets): each archive is
// downloaded from the official release, its MD5 checked against the published one, and
// its SHA-256 recorded. Erebor verifies that SHA-256 on every install.
//
//   node scripts/update-srbminer-manifest.mjs 3.8.8
//
// Review the resulting diff before committing.
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("usage: update-srbminer-manifest.mjs <version>  (e.g. 3.8.8)");
  process.exit(1);
}
const dashed = version.replaceAll(".", "-");
const base = `https://github.com/doktor83/SRBMiner-Multi/releases/download/${version}/`;
// Erebor platform key (Rust std::env::consts OS-ARCH) -> [archive, md5 file].
const ASSETS = {
  "linux-x86_64": [`SRBMiner-Multi-${dashed}-Linux.tar.gz`, `SRBMiner-Multi-${dashed}-Linux.tar.md5`],
  "windows-x86_64": [`SRBMiner-Multi-${dashed}-win64.zip`, `SRBMiner-Multi-${dashed}-win64.md5`],
};

const assets = {};
for (const [key, [file, md5file]] of Object.entries(ASSETS)) {
  const md5res = await fetch(base + md5file);
  if (!md5res.ok) throw new Error(`${md5file}: HTTP ${md5res.status}`);
  const expectedMd5 = (await md5res.text()).trim().split(/\s+/)[0].toLowerCase();
  const res = await fetch(base + file);
  if (!res.ok) throw new Error(`${file}: HTTP ${res.status}`);
  const data = Buffer.from(await res.arrayBuffer());
  const md5 = createHash("md5").update(data).digest("hex");
  if (md5 !== expectedMd5) throw new Error(`${file}: MD5 mismatch ${md5} != ${expectedMd5}`);
  const sha256 = createHash("sha256").update(data).digest("hex");
  assets[key] = { url: base + file, sha256 };
  console.log(`ok ${key} md5=${md5} sha256=${sha256}`);
}

const out = new URL("../crates/erebor-core/srbminer-manifest.json", import.meta.url);
writeFileSync(
  out,
  JSON.stringify({ version, source: `https://github.com/doktor83/SRBMiner-Multi/releases/tag/${version}`, assets }, null, 2) + "\n",
);
console.log(`wrote ${out.pathname}`);
