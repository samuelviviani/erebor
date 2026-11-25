import { alertText } from "./alerts";

test("what the screen already shows makes no toast", () => {
  expect(alertText("it", { code: "mining_started", detail: "Monero" })).toBeNull();
  expect(alertText("it", { code: "mining_stopped", detail: "config_changed" })).toBeNull();
  expect(alertText("en", { code: "mining_stopped", detail: "user" })).toBeNull();
});

test("engine codes and English details never reach the person", () => {
  expect(alertText("it", { code: "mining_stopped", detail: "schedule" })).toMatch(/orario/);
  expect(alertText("it", { code: "hashrate_dropped", detail: "40% of normal" })).toContain("40%");
  expect(alertText("it", { code: "hashrate_dropped", detail: "40% of normal" })).not.toContain("normal");
  expect(alertText("en", { code: "stopped_overheat", detail: "91 °C" })).toContain("91 °C");
  expect(alertText("en", { code: "restart_failed", detail: "spawn failed: x" })).toBe(
    "The miner couldn't restart",
  );
  expect(alertText("en", { code: "brand_new_code", detail: null })).toBe("brand new code");
});
