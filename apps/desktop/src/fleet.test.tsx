import { clearMocks } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import App from "./App";
import { niceStep } from "./components/fleet";
import { installMockBackend } from "./dev/mockBackend";
import { useApp } from "./lib/store";

beforeEach(() => {
  useApp.setState({ config: null, status: null, info: null, page: "dashboard", toasts: [], loadError: null, guide: false, history: [], coinBusy: null, fleet: null, deviceId: null, setupLocal: false });
});
afterEach(() => clearMocks());

test("first start: managing devices skips the mining setup and opens the device list", async () => {
  const backend = installMockBackend();
  render(<App />);
  fireEvent.click(await screen.findByRole("radio", { name: /Manage devices/ }));
  fireEvent.click(screen.getByRole("button", { name: /Open devices/ }));
  await waitFor(() => expect(screen.getByRole("heading", { name: "Devices", level: 1 })).toBeInTheDocument());
  expect(backend.config.defaultView).toBe("devices");
  expect(backend.config.onboardingCompleted).toBe(false);
  // Only managing devices: no section about mining on this computer, no status bar for it.
  for (const name of ["Home", "Coins & wallets", "Energy", "This computer", "Earnings"])
    expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
  expect(screen.queryByText("Ready to start")).not.toBeInTheDocument();
  // Mining here can still be set up later, from Settings.
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  fireEvent.click(await screen.findByRole("button", { name: "Set up mining here" }));
  expect(await screen.findByRole("button", { name: /Back to devices/ })).toBeInTheDocument();
});

test("device list: simulated devices flagged, hashrate per algorithm, confirmed bulk stop with per-device results", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true, defaultView: "devices" }, { devices: true });
  render(<App />);
  expect(await screen.findByText("4 simulated")).toBeInTheDocument();
  // Different algorithms are never added together.
  expect(screen.getAllByText(/^(rx\/0|sha256d)$/).length).toBeGreaterThanOrEqual(2);

  fireEvent.click(screen.getByRole("checkbox", { name: "Select sim-rig-1" }));
  fireEvent.click(screen.getByRole("checkbox", { name: "Select sim-offline" }));
  const bar = screen.getByRole("toolbar", { name: "Selected devices" });
  fireEvent.click(within(bar).getByRole("button", { name: /Stop/ }));
  // Disruptive: asks first, naming the devices.
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText("sim-rig-1, sim-offline")).toBeInTheDocument();
  fireEvent.click(within(dialog).getByRole("button", { name: "Stop" }));
  const results = await screen.findByRole("dialog", { name: "1 of 2 done" });
  expect(within(results).getByText(/No answer at that address/)).toBeInTheDocument();
});

test("adding a computer needs the 12-symbol code; ASIC actions it can't do are disabled with the reason", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true, defaultView: "devices" }, { devices: true });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: /Add device/ }));
  const dialog = await screen.findByRole("dialog");
  fireEvent.change(within(dialog).getByPlaceholderText("192.168.1.20"), { target: { value: "10.0.0.5" } });
  fireEvent.change(within(dialog).getByPlaceholderText("XXXX-XXXX-XXXX"), { target: { value: "ABCD-EFGH" } });
  expect(within(dialog).getByRole("button", { name: "Pair" })).toBeDisabled();
  fireEvent.change(within(dialog).getByPlaceholderText("XXXX-XXXX-XXXX"), { target: { value: "ABCD-EFGH-JKMN" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Pair" }));
  expect(await screen.findByText("Device paired")).toBeInTheDocument();

  fireEvent.click(await screen.findByRole("button", { name: "sim-s19-hot" }));
  await screen.findByRole("heading", { name: "sim-s19-hot" });
  // The device's own Start (the status bar has one for this computer).
  const start = screen.getAllByRole("button", { name: /Start/ }).find((b) => b.hasAttribute("aria-description"))!;
  expect(start).toBeDisabled();
  expect(start.parentElement).toHaveAttribute("title", expect.stringMatching(/no reliable stop/));
  await waitFor(() => expect(screen.getByRole("button", { name: /Restart/ })).toBeEnabled());
});

test("chart axis steps are round: 1, 2 or 5 times a power of ten", () => {
  expect([0.7, 1.5, 3, 7, 1234, 0.004].map(niceStep)).toEqual([1, 2, 5, 10, 2000, 0.005]);
});

test("groups and profiles are tabs of the device page; notifications have their own place in the sidebar", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true, defaultView: "devices" }, { devices: true });
  render(<App />);
  for (const name of ["Groups", "Mining profiles", "Alerts"]) expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
  fireEvent.click(await screen.findByRole("radio", { name: "Groups" }));
  expect(await screen.findByRole("button", { name: /New group/ })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /^Notifications/ }));
  expect(await screen.findByRole("heading", { name: "Notifications", level: 1 })).toBeInTheDocument();
  expect(screen.getByRole("switch", { name: "Desktop notifications" })).toBeChecked();
});
