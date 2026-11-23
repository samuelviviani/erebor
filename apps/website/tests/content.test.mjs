// Guards against unverifiable marketing claims and fake download links.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const files = [];
(function walk(d) {
  for (const f of readdirSync(d)) {
    const p = join(d, f);
    if (statSync(p).isDirectory()) walk(p);
    else if (p.endsWith(".tsx")) files.push(p);
  }
})(join(root, "app"));
const text = files.map((f) => readFileSync(f, "utf8")).join("\n").toLowerCase();

test("no guaranteed-return or auto-maximisation claims", () => {
  for (const banned of ["rendimento garantito", "guadagni garantiti", "massimizza automaticamente", "guaranteed profit", "passive income"]) {
    assert.ok(!text.includes(banned), banned);
  }
});

test("release assets, when present, carry a SHA-256 and an official URL", () => {
  const r = JSON.parse(readFileSync(join(root, "releases.json"), "utf8"));
  for (const a of r.assets) {
    assert.match(a.sha256, /^[0-9a-f]{64}$/);
    assert.match(a.url, /^https:\/\//);
  }
});
