import { dictionaries, translate, translateError } from "./i18n";

test("italian and english have the same keys", () => {
  expect(Object.keys(dictionaries.en).sort()).toEqual(Object.keys(dictionaries.it).sort());
});

test("variables are interpolated", () => {
  expect(translate("en", "onb.step", { n: 2, t: 8 })).toBe("Step 2 of 8");
  expect(translate("it", "mode.share", { p: 25 })).toBe("25% CPU");
});

test("unknown backend errors are shown verbatim", () => {
  expect(translateError("it", "kryptex_pool_required")).toMatch(/pool Kryptex/);
  expect(translateError("en", "spawn failed: x")).toBe("spawn failed: x");
});
