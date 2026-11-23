import "@testing-library/jest-dom/vitest";

// The quick guide opens on first launch; tests that need it clear this flag.
beforeEach(() => localStorage.setItem("erebor.guideSeen", "1"));
