import "@testing-library/jest-dom/vitest";

// The quick guide opens on first launch; tests that need it clear this flag.
beforeEach(() => localStorage.setItem("erebor.guideSeen", "1"));

// NumberFlow is a custom element jsdom can't run: render the same formatted text.
vi.mock("@number-flow/react", () => ({
  default: ({
    value,
    locales,
    format,
    suffix = "",
  }: {
    value: number;
    locales?: string;
    format?: Intl.NumberFormatOptions;
    suffix?: string;
  }) => new Intl.NumberFormat(locales, format).format(value) + suffix,
}));
