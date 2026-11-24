import { clearMocks } from "@tauri-apps/api/mocks";
import {
  act,
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
    await screen.findByText("Mine from your computer without becoming an expert"),
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

test("onboarding: Kryptex only for now, a Monero address is refused", async () => {
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
  expect(await screen.findByText(/works with a Kryptex ID/)).toBeInTheDocument();
});

test("demo path: start and stop from the dashboard", async () => {
  installMockBackend();
  render(<App />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Try demo mode" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Start mining" }),
  );

  expect(await screen.findByText("Mining active")).toBeInTheDocument();
  expect(screen.getByText(/data is simulated/)).toBeInTheDocument();
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

test("settings: one page of grouped rows, technical things behind Advanced options", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  await screen.findByText("Per month (estimate)");
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  expect(
    await screen.findByRole("heading", { name: "Settings" }),
  ).toBeInTheDocument();
  expect(screen.queryByText("Erebor fee")).not.toBeInTheDocument();
  expect(screen.queryByText("Miner installation")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /Advanced options/ }));
  expect(await screen.findByText("Miner installation")).toBeInTheDocument();
  // No remote access or node: those features are gone.
  expect(screen.queryByText(/other devices/)).not.toBeInTheDocument();
});

test("earnings page: one period selector, earned and time on top, the bill from earnings to net profit", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  await screen.findByText("Per month (estimate)");
  act(() => useApp.getState().setPage("stats"));
  expect(await screen.findByText("From earnings to profit")).toBeInTheDocument();
  expect(screen.getByText("Net profit")).toBeInTheDocument();
  expect(screen.queryByText("Dettagli tecnici")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("radio", { name: "30 days" }));
  expect(screen.getByText("Day by day")).toBeInTheDocument();
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Set your electricity price",
    }),
  );
  expect(
    await screen.findByRole("heading", { name: "Settings" }),
  ).toBeInTheDocument();
});

test("demo banner: exit demo returns to the real onboarding when nothing real is configured", async () => {
  const backend = installMockBackend({
    onboardingCompleted: true,
    demoMode: true,
  });
  render(<App />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Exit demo" }),
  );
  expect(
    await screen.findByRole("button", { name: "Try demo mode" }),
  ).toBeInTheDocument();
  expect(backend.config.demoMode).toBe(false);
  expect(backend.config.onboardingCompleted).toBe(false);
});

test("coins: automatic hides the coin list; choosing by hand warns and needs confirmation", async () => {
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
  expect(await screen.findByText("Now mining")).toBeInTheDocument();
  expect(screen.getByText("Coins your PC can mine")).toBeInTheDocument();
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
  expect(
    await screen.findByText(/most profitable setup/),
  ).toBeInTheDocument();
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

// Bitcoin and per-coin wallets are off while Erebor is Kryptex only.
test.skip("onboarding: earnings can go straight to a Bitcoin wallet, with the 'BTC without mining BTC' note", async () => {
  const backend = installMockBackend();
  render(<App />);
  await screen.findByText("Mine from your computer without becoming an expert");
  next();
  expect(await screen.findByText(/you receive BTC/)).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: /In Bitcoin/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  expect(
    screen.getByRole("radio", { name: /Kryptex account/ }),
  ).toHaveAttribute("aria-checked", "true");
  fireEvent.click(screen.getByRole("radio", { name: /My Bitcoin wallet/ }));
  const input = screen.getByRole("textbox");
  expect(input).toHaveAttribute("placeholder", "bc1…");
  fireEvent.change(input, {
    target: { value: "bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345" },
  });
  expect(
    await screen.findByText(/Valid Bitcoin address/),
  ).toBeInTheDocument();
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

test("home shows the whole computer live, no Erebor fee, no mining status in the sidebar", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  expect(await screen.findByText("Demo GPU")).toBeInTheDocument();
  expect(screen.getByText("Your computer")).toBeInTheDocument();
  expect(screen.queryByText("Commissione Erebor")).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Computer" }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByRole("navigation", { name: "Erebor" }),
  ).not.toHaveTextContent("Mining inactive");
  expect(await screen.findByText("6.7 / 12 GB")).toBeInTheDocument();
  // Live charts fill in as readings arrive (one every 2 s).
  expect(screen.getByText("CPU usage")).toBeInTheDocument();
  expect(screen.getAllByText("Load").length).toBeGreaterThan(0);
});

test("wallet: the payout address can be changed from a clear dialog", async () => {
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
  act(() => useApp.getState().setPage("wallet"));
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

test.skip("wallet: an own wallet can be set for a single coin", async () => {
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
  act(() => useApp.getState().setPage("wallet"));
  expect(await screen.findByText(/No exceptions/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Add a wallet" }));
  fireEvent.change(
    await screen.findByRole("textbox", { name: "Search a coin" }),
    { target: { value: "prl" } },
  );
  const found = await screen.findAllByRole("option");
  expect(found).toHaveLength(1);
  fireEvent.click(found[0]);
  expect(
    await screen.findByRole("heading", { name: "Your Pearl wallet" }),
  ).toBeInTheDocument();
  const input = screen.getByRole("textbox");
  fireEvent.change(input, { target: { value: "prl1pbad" } });
  expect(
    await screen.findByText(/Not a valid address for this coin/),
  ).toBeInTheDocument();
  const good = `prl1p${"q".repeat(58)}`;
  fireEvent.change(input, { target: { value: good } });
  expect(await screen.findByText("Valid Pearl address")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(backend.config.coinWallets).toEqual({ PRL: good }),
  );
  expect(
    await screen.findByRole("button", { name: "Modifica Pearl" }),
  ).toBeInTheDocument();
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
  fireEvent.click(
    await screen.findByRole("button", { name: "Start mining" }),
  );
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
    await screen.findByText("Minare dal tuo computer, senza diventare un esperto"),
  ).toBeInTheDocument();
});
