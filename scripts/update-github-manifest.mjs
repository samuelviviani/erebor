#!/usr/bin/env node
// Pin an official lolMiner or cpuminer-opt release in crates/erebor-core/<miner>-manifest.json.
// GitHub publishes a SHA-256 digest for every release asset: each archive is downloaded
// from the official release, its SHA-256 checked against GitHub's, and recorded.
// Erebor verifies that SHA-256 on every install.
//
//   node scripts/update-github-manifest.mjs lolminer 1.98a
//   node scripts/update-github-manifest.mjs cpuminer v26.1
//
// Review the resulting diff before committing.
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";

// Erebor platform key (Rust std::env::consts OS-ARCH) -> archive name, per miner.
const MINERS = {
  lolminer: {
    repo: "Lolliedieb/lolMiner-releases",
    assets: (v) => ({ "linux-x86_64": `lolMiner_v${v}_Lin64.tar.gz`, "windows-x86_64": `lolMiner_v${v}_Win64.zip` }),
  },
  // Windows builds only upstream (Linux users build from source).
  cpuminer: {
    repo: "JayDDee/cpuminer-opt",
    assets: (v) => ({ "windows-x86_64": `cpuminer-opt-${v.replace(/^v/, "")}-windows.zip` }),
  },
};
const [miner, version] = process.argv.slice(2);
if (!MINERS[miner] || !/^v?\d+\.\d+[a-z]?$/.test(version ?? "")) {
  console.error("usage: update-github-manifest.mjs <lolminer|cpuminer> <version>");
  process.exit(1);
}
const { repo } = MINERS[miner];
const release = await (await fetch(`https://api.github.com/repos/${repo}/releases/tags/${version}`)).json();
const ASSETS = MINERS[miner].assets(version);

const assets = {};
for (const [key, file] of Object.entries(ASSETS)) {
  const a = release.assets?.find((x) => x.name === file);
  if (!a?.digest?.startsWith("sha256:")) throw new Error(`${file}: no published SHA-256`);
  const url = `https://github.com/${repo}/releases/download/${version}/${file}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${file}: HTTP ${res.status}`);
  const sha256 = createHash("sha256").update(Buffer.from(await res.arrayBuffer())).digest("hex");
  if (`sha256:${sha256}` !== a.digest) throw new Error(`${file}: SHA-256 mismatch ${sha256} != ${a.digest}`);
  assets[key] = { url, sha256 };
  console.log(`ok ${key} sha256=${sha256}`);
}

const out = new URL(`../crates/erebor-core/${miner}-manifest.json`, import.meta.url);
writeFileSync(
  out,
  JSON.stringify({ version, source: `https://github.com/${repo}/releases/tag/${version}`, assets }, null, 2) + "\n",
);
console.log(`wrote ${out.pathname}`);
