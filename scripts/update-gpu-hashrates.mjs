#!/usr/bin/env node
// Rebuild crates/erebor-core/gpu-hashrates.json: expected hashrate (H/s) of each GPU
// model for each catalog GPU coin, from WhatToMine's public GPU pages. The automatic
// coin choice ranks coins with these numbers instead of benchmarking every coin.
// Installed apps download the committed file, so no release is needed to update it.
//
//   node scripts/update-gpu-hashrates.mjs
//
// Review the resulting diff before committing.
import { writeFileSync } from "node:fs";

// WhatToMine algorithm -> Erebor coin id (crates/erebor-core/src/coins.rs).
const ALGOS = {
  Pearl: ["PRL"],
  Ethash: ["ETC", "ETHW", "OCTA"], // etchash runs at ethash speed
  Autolykos: ["ERG"],
  KawPow: ["RVN", "QUAI"],
  Xelishashv3: ["XEL"],
  Qhash: ["QTC"],
  Blake3: ["ALPH"],
  Octopus: ["CFX"],
  NexaPow: ["NEXA"],
  kHeavyHash: ["KAS"],
  SHA3x: ["XTM-SHA3X"],
  Cuckaroo29: ["XTM-C29"],
};
const UNITS = { "": 1, k: 1e3, M: 1e6, G: 1e9, T: 1e12, P: 1e15 };
const BASE = "https://whattomine.com";
const get = async (path) => {
  const res = await fetch(BASE + path, { headers: { "user-agent": "Mozilla/5.0" } });
  if (!res.ok) throw new Error(`${path}: HTTP ${res.status}`);
  return res.text();
};
const text = (html) =>
  html
    .replace(/<(script|style)[\s\S]*?<\/\1>/g, "")
    .replace(/<[^>]+>/g, "\n")
    .replace(/&amp;/g, "&")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
// Same normalisation as coins::gpu_key in Rust: vendor words and memory sizes dropped.
const key = (name) =>
  name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .split(" ")
    .filter((t) => t && !["nvidia", "amd", "geforce", "radeon"].includes(t) && !/^\d+gb$/.test(t))
    .join(" ");

const links = [...new Set((await get("/gpus")).match(/\/gpus\/\d+-[a-z0-9-]+/g))];
const gpus = {};
for (const link of links) {
  const lines = text(await get(link));
  const name = lines[0].replace(/ Mining Calculator.*$/, "");
  const end = lines.indexOf("Calculations");
  const rates = {};
  for (let i = 1; i < end; i++) {
    const m = lines[i].match(/^([\d.,]+) ([kMGTP]?)h\/s @ \d+W$/);
    for (const id of (m && ALGOS[lines[i - 1]]) ?? []) {
      rates[id] = Number(m[1].replaceAll(",", "")) * UNITS[m[2]];
    }
  }
  const k = key(name);
  if (Object.keys(rates).length && !gpus[k]) gpus[k] = rates; // "3080" before "3080 12GB"
  console.log(`${k}: ${JSON.stringify(rates)}`);
  await new Promise((r) => setTimeout(r, 1000)); // be gentle with WhatToMine
}
const out = { source: `${BASE}/gpus`, updated: new Date().toISOString().slice(0, 10), gpus };
writeFileSync(
  new URL("../crates/erebor-core/gpu-hashrates.json", import.meta.url),
  JSON.stringify(out, null, 1) + "\n",
);
console.log(`${Object.keys(gpus).length} GPUs written`);
