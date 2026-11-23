import { duration, hashrate, minutesToTime, money, timeToMinutes, xmr } from "./format";

test("missing values are never shown as zero", () => {
  expect(hashrate(null)).toBe("—");
  expect(xmr(undefined)).toBe("—");
  expect(money(null, "EUR")).toBe("—");
  expect(duration(NaN)).toBe("—");
});

test("hashrate units", () => {
  expect(hashrate(950)).toBe("950 H/s");
  expect(hashrate(4123.4)).toBe("4.12 kH/s");
  expect(hashrate(2.5e6)).toBe("2.50 MH/s");
});

test("durations", () => {
  expect(duration(42)).toBe("42s");
  expect(duration(125)).toBe("2m 5s");
  expect(duration(3 * 3600 + 60)).toBe("3h 1m");
  expect(duration(2 * 86400 + 3600)).toBe("2d 1h 0m");
});

test("tiny XMR amounts keep precision", () => {
  expect(xmr(0)).toBe("0 XMR");
  expect(xmr(0.00001234)).toBe("0.00001234 XMR");
  expect(xmr(0.5)).toBe("0.500000 XMR");
});

test("schedule time conversion roundtrip", () => {
  expect(minutesToTime(22 * 60 + 5)).toBe("22:05");
  expect(timeToMinutes("07:30")).toBe(450);
  expect(timeToMinutes(minutesToTime(1439))).toBe(1439);
});
