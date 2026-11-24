// Guards against unverifiable marketing claims.
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

test("no guaranteed-return claims", () => {
  for (const banned of ["guaranteed profit", "guaranteed return", "passive income", "rendimento garantito", "guadagni garantiti"]) {
    assert.ok(!text.includes(banned), banned);
  }
});
