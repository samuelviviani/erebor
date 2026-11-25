import { clearMocks } from "@tauri-apps/api/mocks";
import {
  act,
  within,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import App from "./App";
import { installMockBackend, VALID_WALLET } from "./dev/mockBackend";
import { useApp } from "./lib/store";

beforeEach(() => {
  useApp.setState({
    config: null,
    status: null,
    info: null,
    page: "dashboard",
    toasts: [],
    loadError: null,
    guide: false,
    history: [],
    coinBusy: null,
  });
});
afterEach(() => clearMocks());

const next = () =>
  fireEvent.click(screen.getByRole("button", { name: "Next" }));

test("onboarding: three simple steps, Kryptex ID, miner installed automatically, no fee prompt", async () => {
  const backend = installMockBackend();
  render(<App />);
  expect(
    await screen.findByText(
      "Mine from your computer without becoming an expert",
    ),
  ).toBeInTheDocument();
  expect(
    screen.queryByText(/of mining time supports the project/),
  ).not.toBeInTheDocument();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  next();

  // Account: secrets are rejected, Next stays disabled until the ID/address is valid.
  const [input, pcName] = screen.getAllByRole("textbox");
  fireEvent.change(input, {
    target: {
      value:
        "abbey ability able about above absent absorb abstract absurd abuse access accident account",
    },
  });
  expect(await screen.findByText(/seed phrase/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
  fireEvent.change(input, { target: { value: "krxTEST01" } });
  expect(await screen.findByText(/Valid Kryptex ID/)).toBeInTheDocument();
  // The PC name is set from the start (letters and digits only).
  fireEvent.change(pcName, { target: { value: "Ufficio-1" } });
  next();

  // PC: the pinned miner is installed without asking.
  expect(
    await screen.findByText("Getting your computer ready"),
  ).toBeInTheDocument();
  await waitFor(() => expect(backend.calls).toContain("install_miner"));
  expect(
    await screen.findByText("Your computer is ready.", {}, { timeout: 4000 }),
  ).toBeInTheDocument();
  next();

  // Ready: start mining in one click.
  expect(await screen.findByText("All set")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Start mining" }));
  expect(await screen.findByText("Mining active")).toBeInTheDocument();
  expect(backend.config.wallet).toBe("krxTEST01");
  expect(backend.config.pool.worker).toBe("Ufficio1");
  expect(backend.config.pool.id).toBe("kryptex");
  expect(backend.config.onboardingCompleted).toBe(true);
});

test("onboarding: the main payout is Kryptex or Bitcoin, a Monero address is refused", async () => {
  installMockBackend();
  render(<App />);
  await screen.findByText("Mine from your computer without becoming an expert");
  next();
  expect(
    screen.queryByRole("radio", { name: /In the mined coin/ }),
  ).not.toBeInTheDocument();
  fireEvent.change(screen.getAllByRole("textbox")[0], {
    target: { value: VALID_WALLET },
  });
  expect(
    await screen.findByText(/Kryptex ID \(krx…\) or a Bitcoin address/),
  ).toBeInTheDocument();
});

test("demo path: start and stop from the dashboard", async () => {
  installMockBackend();
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Try demo mode" }));
  fireEvent.click(await screen.findByRole("button", { name: "Start mining" }));

  expect(await screen.findByText("Mining active")).toBeInTheDocument();
  expect(screen.getByRole("status", { name: /data is simulated/ })).toBeInTheDocument();
  // Right after start the status line says what is happening instead of going blank.
  expect(screen.getAllByText("Starting the miner…").length).toBeGreaterThan(0);
  await waitFor(
    () => expect(screen.getAllByText(/kH\/s/).length).toBeGreaterThan(0),
    { timeout: 5000 },
  );

  // Just start and stop: no pause button.
  expect(
    screen.queryByRole("button", { name: "Pausa" }),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Stop mining" }));
  expect(await screen.findByText("Mining inactive")).toBeInTheDocument();
});

test("settings hold the app and, folded away, the advanced miner tools; energy and computer are their own sections", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  await screen.findByText("Per month (estimate)");
  // ⌘, / Ctrl+, opens the settings like any desktop app.
  fireEvent.keyDown(window, { key: ",", ctrlKey: true });
  expect(
    await screen.findByRole("heading", { name: "Settings" }),
  ).toBeInTheDocument();
  expect(screen.queryByText("Erebor fee")).not.toBeInTheDocument();
  expect(screen.queryByText("Time bands")).not.toBeInTheDocument();
  // Miners and pool are advanced: in Settings, inside a closed disclosure.
  expect(screen.getByText("Advanced options").closest("details")).not.toHaveAttribute("open");
  expect(screen.getByText("XMRig")).not.toBeVisible();
  // No remote access or node: those features are gone.
  expect(screen.queryByText(/other devices/)).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Energy" }));
  expect(await screen.findByRole("heading", { name: "Energy" })).toBeInTheDocument();
  expect(screen.getByText("Time bands")).toBeInTheDocument();
  // One price, no rules: every half hour is the same, so there is no day plan to show.
  expect(screen.queryByRole("img", { name: "Today" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Computer" }));
  expect(await screen.findByText("GPU power limit")).toBeInTheDocument();
  expect(screen.queryByText("Monero pool")).not.toBeInTheDocument();
});

test("today's plan uses the engine's window rule: Monday = 0, windows cross midnight", async () => {
  const { inWindow, gridPrice } = await import("./pages/Energy");
  const all = Array(7).fill(true);
  const monOnly = [true, false, false, false, false, false, false];
  // 22:00–06:00 started on Monday: Monday 23:00 and Tuesday 05:00 are in, Tuesday 07:00 is not.
  expect(inWindow(monOnly, 22 * 60, 6 * 60, 0, 23 * 60)).toBe(true);
  expect(inWindow(monOnly, 22 * 60, 6 * 60, 1, 5 * 60)).toBe(true);
  expect(inWindow(monOnly, 22 * 60, 6 * 60, 1, 7 * 60)).toBe(false);
  expect(inWindow(monOnly, 22 * 60, 6 * 60, 0, 5 * 60)).toBe(false);
  expect(inWindow(all, 60, 60, 3, 0)).toBe(true);
  const e = {
    pricePerKwh: 0.3,
    bands: [{ startMinute: 19 * 60, endMinute: 8 * 60, days: all, price: 0.1 }],
  } as unknown as Parameters<typeof gridPrice>[0];
  expect(gridPrice(e, 2, 20 * 60)).toBe(0.1);
  expect(gridPrice(e, 2, 12 * 60)).toBe(0.3);
});

test("demo banner: exit demo returns to the real onboarding when nothing real is configured", async () => {
  const backend = installMockBackend({
    onboardingCompleted: true,
    demoMode: true,
  });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Exit demo" }));
  expect(
    await screen.findByRole("button", { name: "Try demo mode" }),
  ).toBeInTheDocument();
  expect(backend.config.demoMode).toBe(false);
  expect(backend.config.onboardingCompleted).toBe(false);
});

test("coins: automatic lists coins without selecting; choosing by hand warns and needs confirmation", async () => {
  const backend = installMockBackend({
    onboardingCompleted: true,
    wallet: VALID_WALLET,
    miner: {
      path: "/demo/xmrig",
      version: "6.26.0",
      sha256: null,
      verified: true,
    },
  });
  render(<App />);
  expect(await screen.findByText("Per month (estimate)")).toBeInTheDocument();
  act(() => useApp.getState().setPage("coins"));
  expect(await screen.findByText("mining now")).toBeInTheDocument();
  // The main payout sits on the same page.
  expect(screen.getByText("Your earnings go to")).toBeInTheDocument();
  expect(await screen.findByText("Pearl")).toBeInTheDocument();
  expect(
    screen.queryByRole("radio", { name: "Pearl" }),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("radio", { name: "I choose" }));
  expect(
    await screen.findByText(/no longer switches on its own/),
  ).toBeInTheDocument();
  expect(await screen.findByRole("radio", { name: "Pearl" })).toBeDisabled();
  expect(
    screen.getAllByText("Needs an NVIDIA or AMD graphics card").length,
  ).toBeGreaterThan(0);
  fireEvent.click(screen.getByRole("radio", { name: "Monero" }));
  // Nothing changes until confirmed.
  expect(
    await screen.findByRole("heading", { name: "Switch to Monero?" }),
  ).toBeInTheDocument();
  expect(backend.config.coin).toBe("AUTO");
  fireEvent.click(screen.getByRole("button", { name: "Yes, switch coin" }));
  await waitFor(() => expect(backend.config.coin).toBe("XMR"));
});

test("advice: a Monero payout is told how to get BTC; a manual coin is told to go back to automatic", async () => {
  const backend = installMockBackend({
    onboardingCompleted: true,
    wallet: VALID_WALLET,
    coin: "XMR",
    miner: {
      path: "/demo/xmrig",
      version: "6.26.0",
      sha256: null,
      verified: true,
    },
  });
  render(<App />);
  expect(
    await screen.findByText(/with a Monero address you receive XMR/),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Change payout" }));
  expect(await screen.findByText("Monero address")).toBeInTheDocument();
  backend.config.wallet = "krxABC123";
  act(() => useApp.getState().setConfig(backend.config));
  act(() => useApp.getState().setPage("dashboard"));
  expect(
    await screen.findByText(/you picked Monero by hand/),
  ).toBeInTheDocument();
  fireEvent.click(
    screen.getByRole("button", { name: "Use the automatic choice" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Yes, automatic choice" }),
  );
  await waitFor(() => expect(backend.config.coin).toBe("AUTO"));
  // Back on automatic the advice has nothing to warn about: no tip on Home.
  await waitFor(() =>
    expect(screen.queryByText(/you picked Monero by hand/)).not.toBeInTheDocument(),
  );
});

test("help: question marks explain terms in plain words", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  fireEvent.click(
    (
      await screen.findAllByRole("button", {
        name: "What is: Why it's an estimate",
      })
    )[0],
  );
  expect(
    await screen.findByRole("dialog", { name: "Why it's an estimate" }),
  ).toHaveTextContent(/Prices and difficulty/);
});

test("quick guide opens only on first launch, and is not in the menu", async () => {
  localStorage.removeItem("erebor.guideSeen");
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  expect(
    await screen.findByRole("heading", { name: "Welcome to Erebor" }),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  expect(
    await screen.findByRole("heading", { name: "Erebor picks for you" }),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Back" }));
  fireEvent.click(await screen.findByRole("button", { name: "Skip" }));
  await waitFor(() =>
    expect(
      screen.queryByRole("heading", { name: "Welcome to Erebor" }),
    ).not.toBeInTheDocument(),
  );
  expect(localStorage.getItem("erebor.guideSeen")).toBe("1");
  expect(
    screen.queryByRole("button", { name: "Quick guide" }),
  ).not.toBeInTheDocument();
});

test("onboarding: earnings can go straight to a Bitcoin wallet", async () => {
  const backend = installMockBackend();
  render(<App />);
  await screen.findByText("Mine from your computer without becoming an expert");
  next();
  // Bitcoin through unMineable is the recommended default.
  expect(
    screen.getByRole("radio", { name: /My Bitcoin wallet/ }),
  ).toHaveAttribute("aria-checked", "true");
  const input = screen.getAllByRole("textbox")[0];
  expect(input).toHaveAttribute("placeholder", "bc1…");
  fireEvent.change(input, {
    target: { value: "bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345" },
  });
  expect(await screen.findByText(/Valid Bitcoin address/)).toBeInTheDocument();
  next();
  expect(
    await screen.findByText("Your computer is ready.", {}, { timeout: 4000 }),
  ).toBeInTheDocument();
  next();
  fireEvent.click(await screen.findByRole("button", { name: "Later" }));
  await waitFor(() =>
    expect(backend.config.wallet).toBe(
      "bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345",
    ),
  );
});

test("Kryptex payout: the Bitcoin balance is spelled out in BTC", async () => {
  installMockBackend({
    onboardingCompleted: true,
    demoMode: true,
    wallet: "krxABC123",
  });
  render(<App />);
  expect(
    await screen.findByText("Collected on Kryptex (estimate)"),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: /Check the real balance/ }),
  ).toBeInTheDocument();
  expect(screen.getByText("0.00000011 BTC")).toBeInTheDocument();
});

test("home sums the computer up in four numbers; the live charts are on Computer", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  expect(await screen.findByText("Your computer")).toBeInTheDocument();
  for (const k of ["Speed", "Power draw", "Hottest"])
    expect(screen.getByText(k)).toBeInTheDocument();
  expect(screen.queryByText("Commissione Erebor")).not.toBeInTheDocument();
  expect(
    screen.getByRole("navigation", { name: "Erebor" }),
  ).not.toHaveTextContent("Mining inactive");
  // The link on Home (the last "Computer" button: the sidebar comes first).
  fireEvent.click(screen.getAllByRole("button", { name: "Computer" }).at(-1)!);
  expect(await screen.findByText("Demo GPU")).toBeInTheDocument();
  expect(await screen.findByText(/6\.7 \/ 12 GB/)).toBeInTheDocument();
  // Live charts fill in as readings arrive (one every 2 s).
  expect(screen.getAllByText("Load").length).toBe(2);
  expect(screen.getAllByText("Temperature").length).toBe(2);
});

test("coins & wallets: the main payout can be changed from a clear dialog", async () => {
  const backend = installMockBackend({
    onboardingCompleted: true,
    wallet: "krxOLD123",
    miner: {
      path: "/demo/xmrig",
      version: "6.26.0",
      sha256: null,
      verified: true,
    },
  });
  render(<App />);
  await screen.findByText("Per month (estimate)");
  act(() => useApp.getState().setPage("coins"));
  fireEvent.click(await screen.findByRole("button", { name: "Change" }));
  expect(
    await screen.findByRole("heading", {
      name: "Where do you want your earnings?",
    }),
  ).toBeInTheDocument();
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "krxNEW123" },
  });
  expect(await screen.findByText(/Valid Kryptex ID/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(backend.config.wallet).toBe("krxNEW123"));
});

test("coins & wallets: every coin opens, saying what mining it takes and what is missing", async () => {
  installMockBackend({
    onboardingCompleted: true,
    wallet: VALID_WALLET,
  });
  render(<App />);
  await screen.findByText("Per month (estimate)");
  act(() => useApp.getState().setPage("coins"));
  expect((await screen.findAllByText("Paid to")).length).toBeGreaterThan(0);
  // Without a GPU the row says so first; the sheet lists everything else too.
  fireEvent.click(await screen.findByRole("button", { name: "Firo details" }));
  const sheet = await screen.findByRole("dialog");
  expect(sheet).toHaveTextContent("What it takes");
  expect(sheet).toHaveTextContent("Needs an NVIDIA or AMD graphics card");
  expect(sheet).toHaveTextContent("Not on Kryptex: needs your own wallet");
  expect(sheet).toHaveTextContent("Needs SRBMiner-MULTI");
  expect(screen.getByRole("button", { name: /Install/ })).toBeInTheDocument();
  // The own-wallet route is already selected, on the first listed pool.
  expect(screen.getByRole("radio", { name: "My FIRO wallet" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  expect(screen.getByRole("radio", { name: /2Miners/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
});

test("coins & wallets: a coin's sheet sets its own wallet and leaves it out of automatic", async () => {
  const backend = installMockBackend({
    onboardingCompleted: true,
    wallet: VALID_WALLET,
    miner: {
      path: "/demo/xmrig",
      version: "6.26.0",
      sha256: null,
      verified: true,
    },
  });
  render(<App />);
  await screen.findByText("Per month (estimate)");
  act(() => useApp.getState().setPage("coins"));
  fireEvent.click(
    await screen.findByRole("button", { name: "Monero details" }),
  );
  expect(
    await screen.findByRole("heading", { name: "Monero" }),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("switch", { name: "Automatic mode may pick it" }));
  fireEvent.click(screen.getByRole("radio", { name: "My XMR wallet" }));
  const input = screen.getByRole("textbox");
  fireEvent.change(input, { target: { value: "4bad" } });
  expect(
    await screen.findByText(/Not a valid address for this coin/),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  const good = `4${"a".repeat(94)}`;
  fireEvent.change(input, { target: { value: good } });
  expect(await screen.findByText("Valid Monero address")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(backend.config.coinWallets).toEqual({ XMR: good }),
  );
  expect(backend.config.autoExclude).toEqual(["XMR"]);
  expect(
    await screen.findByText("XMR to your wallet · Kryptex"),
  ).toBeInTheDocument();
  expect(screen.getByText("left out of automatic")).toBeInTheDocument();
});

test("notifications: repeats merge into one, the stack fans out on hover", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  await screen.findByText("Per month (estimate)");
  act(() => {
    const { toast } = useApp.getState();
    toast("warning", "Pool non raggiungibile");
    toast("warning", "Pool non raggiungibile");
    toast("info", "Moneta cambiata");
  });
  expect(useApp.getState().toasts).toHaveLength(2);
  expect(screen.getByText("×2")).toBeInTheDocument();
  expect(screen.getByText("+1")).toBeInTheDocument();
  const stack = screen.getByRole("region", { name: "Notifications" });
  fireEvent.mouseEnter(stack);
  expect(screen.queryByText("+1")).not.toBeInTheDocument();
  fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]);
  await waitFor(() => expect(useApp.getState().toasts).toHaveLength(1));
});

test("mode: changing it while mining applies at once, no confirmation", async () => {
  const backend = installMockBackend({
    onboardingCompleted: true,
    demoMode: true,
  });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Start mining" }));
  await waitFor(() => expect(backend.calls).toContain("start_mining"));
  fireEvent.click(await screen.findByRole("radio", { name: /Performance/ }));
  await waitFor(() =>
    expect(backend.config.performance.preset).toBe("performance"),
  );
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("onboarding: the flag switch changes the language at once", async () => {
  installMockBackend();
  render(<App />);
  await screen.findByText("Mine from your computer without becoming an expert");
  fireEvent.click(screen.getByRole("radio", { name: "Italiano" }));
  expect(
    await screen.findByText(
      "Minare dal tuo computer, senza diventare un esperto",
    ),
  ).toBeInTheDocument();
});

test("home: the coin in the title is a menu — automatic first, mineable coins, then all coins", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: /Change coin/ }));
  const menu = screen.getByRole("menu", { name: "Change coin" });
  expect(within(menu).getByRole("menuitemradio", { name: /Automatic \(recommended\)/ })).toHaveAttribute("aria-checked", "true");
  expect(within(menu).getByRole("menuitemradio", { name: /Monero/ })).toBeInTheDocument();
  fireEvent.click(within(menu).getByRole("menuitem", { name: /All coins and wallets/ }));
  expect(await screen.findByRole("heading", { name: "Coins & wallets" })).toBeInTheDocument();
});
