import { clearMocks } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "./App";
import { installMockBackend, VALID_WALLET } from "./dev/mockBackend";
import { useApp } from "./lib/store";

beforeEach(() => {
  useApp.setState({ config: null, status: null, info: null, page: "dashboard", toasts: [], loadError: null, guide: false, history: [], coinBusy: null });
});
afterEach(() => clearMocks());

const next = () => fireEvent.click(screen.getByRole("button", { name: "Avanti" }));

test("onboarding: three simple steps, Kryptex ID, miner installed automatically, no fee prompt", async () => {
  const backend = installMockBackend();
  render(<App />);
  expect(await screen.findByText("Guadagna Bitcoin con il tuo computer")).toBeInTheDocument();
  expect(screen.queryByText(/del tempo di mining sostiene il progetto/)).not.toBeInTheDocument();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  next();

  // Account: secrets are rejected, Next stays disabled until the ID/address is valid.
  const input = screen.getByRole("textbox");
  fireEvent.change(input, { target: { value: "abbey ability able about above absent absorb abstract absurd abuse access accident account" } });
  expect(await screen.findByText(/seed phrase/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Avanti" })).toBeDisabled();
  fireEvent.change(input, { target: { value: "krxTEST01" } });
  expect(await screen.findByText(/ID Kryptex valido/)).toBeInTheDocument();
  next();

  // PC: the pinned miner is installed without asking.
  expect(await screen.findByText("Preparo il tuo computer")).toBeInTheDocument();
  await waitFor(() => expect(backend.calls).toContain("install_miner"));
  expect(await screen.findByText("Il computer è pronto.", {}, { timeout: 4000 })).toBeInTheDocument();
  next();

  // Ready: start mining in one click.
  expect(await screen.findByText("Tutto pronto")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Inizia a minare" }));
  expect(await screen.findByText("Mining attivo")).toBeInTheDocument();
  expect(backend.config.wallet).toBe("krxTEST01");
  expect(backend.config.pool.id).toBe("kryptex");
  expect(backend.config.onboardingCompleted).toBe(true);
});

test("onboarding: a Monero address still works", async () => {
  const backend = installMockBackend();
  render(<App />);
  await screen.findByText("Guadagna Bitcoin con il tuo computer");
  next();
  fireEvent.change(screen.getByRole("textbox"), { target: { value: VALID_WALLET } });
  expect(await screen.findByText(/Indirizzo Monero valido/)).toBeInTheDocument();
  next();
  expect(await screen.findByText("Il computer è pronto.", {}, { timeout: 4000 })).toBeInTheDocument();
  next();
  fireEvent.click(await screen.findByRole("button", { name: "Più tardi" }));
  expect(await screen.findByText("Mining inattivo")).toBeInTheDocument();
  expect(backend.config.wallet).toBe(VALID_WALLET);
  expect(backend.calls).not.toContain("start_mining");
});

test("demo path: start, pause and stop from the dashboard", async () => {
  installMockBackend();
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Prova in modalità demo" }));
  fireEvent.click(await screen.findByRole("button", { name: "Inizia a minare" }));

  expect(await screen.findByText("Mining attivo")).toBeInTheDocument();
  expect(screen.getByText(/dati sono simulati/)).toBeInTheDocument();
  // Right after start the status line says what is happening instead of going blank.
  expect(screen.getAllByText("Avvio del miner…").length).toBeGreaterThan(0);
  await waitFor(() => expect(screen.getAllByText(/kH\/s/).length).toBeGreaterThan(0), { timeout: 5000 });

  fireEvent.click(screen.getByRole("button", { name: "Pausa" }));
  expect(await screen.findByRole("heading", { name: "Mining in pausa" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Arresta" }));
  expect(await screen.findByText("Mining inattivo")).toBeInTheDocument();
});

test("settings: one page of grouped rows, technical things behind Advanced options", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  await screen.findByText("Al mese (stima)");
  fireEvent.click(screen.getByRole("button", { name: "Impostazioni" }));
  expect(await screen.findByRole("heading", { name: "Impostazioni" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("radio", { name: "Sistema" }));
  fireEvent.click(await screen.findByRole("radio", { name: "English" }));
  expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  expect(screen.queryByText("Erebor fee")).not.toBeInTheDocument();
  expect(screen.queryByText("Miner installation")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /Advanced options/ }));
  expect(await screen.findByText("Miner installation")).toBeInTheDocument();
  fireEvent.click(await screen.findByRole("switch", { name: /Allow control from other devices/ }));
  expect(await screen.findByText("http://192.168.1.20:7420")).toBeInTheDocument();
  expect(screen.getByText("Phone link")).toBeInTheDocument();
});

test("earnings page: one period selector, earned and time on top, the bill from earnings to net profit", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  await screen.findByText("Al mese (stima)");
  act(() => useApp.getState().setPage("stats"));
  expect(await screen.findByText("Dal guadagno all'utile")).toBeInTheDocument();
  expect(screen.getByText("Utile netto")).toBeInTheDocument();
  expect(screen.queryByText("Dettagli tecnici")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("radio", { name: "30 giorni" }));
  expect(screen.getByText("Giorno per giorno")).toBeInTheDocument();
  fireEvent.click(await screen.findByRole("button", { name: "Imposta il prezzo della corrente" }));
  expect(await screen.findByRole("heading", { name: "Impostazioni" })).toBeInTheDocument();
});

test("demo banner: exit demo returns to the real onboarding when nothing real is configured", async () => {
  const backend = installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Esci dalla demo" }));
  expect(await screen.findByRole("button", { name: "Prova in modalità demo" })).toBeInTheDocument();
  expect(backend.config.demoMode).toBe(false);
  expect(backend.config.onboardingCompleted).toBe(false);
});

test("coins: automatic hides the coin list; choosing by hand warns and needs confirmation", async () => {
  const backend = installMockBackend({ onboardingCompleted: true, wallet: VALID_WALLET, miner: { path: "/demo/xmrig", version: "6.26.0", sha256: null, verified: true } });
  render(<App />);
  expect(await screen.findByText("Al mese (stima)")).toBeInTheDocument();
  act(() => useApp.getState().setPage("coins"));
  expect(await screen.findByText("Ora mina")).toBeInTheDocument();
  expect(screen.getByText("Monete adatte al tuo PC")).toBeInTheDocument();
  expect(screen.queryByRole("radio", { name: "Pearl" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("radio", { name: "Scelgo io" }));
  expect(await screen.findByText(/non la cambierà più da solo/)).toBeInTheDocument();
  expect(await screen.findByRole("radio", { name: "Pearl" })).toBeDisabled();
  expect(screen.getAllByText("Serve una scheda video NVIDIA o AMD").length).toBeGreaterThan(0);
  fireEvent.click(screen.getByRole("radio", { name: "Monero" }));
  // Nothing changes until confirmed.
  expect(await screen.findByRole("heading", { name: "Passare a Monero?" })).toBeInTheDocument();
  expect(backend.config.coin).toBe("AUTO");
  fireEvent.click(screen.getByRole("button", { name: "Sì, cambia moneta" }));
  await waitFor(() => expect(backend.config.coin).toBe("XMR"));
});

test("advice: a Monero payout is told how to get BTC; a manual coin is told to go back to automatic", async () => {
  const backend = installMockBackend({ onboardingCompleted: true, wallet: VALID_WALLET, coin: "XMR", miner: { path: "/demo/xmrig", version: "6.26.0", sha256: null, verified: true } });
  render(<App />);
  expect(await screen.findByText(/con un indirizzo Monero ricevi XMR/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cambia pagamento" }));
  expect(await screen.findByText("Indirizzo Monero")).toBeInTheDocument();
  backend.config.wallet = "krxABC123";
  act(() => useApp.getState().setConfig(backend.config));
  act(() => useApp.getState().setPage("dashboard"));
  expect(await screen.findByText(/hai scelto Monero a mano/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Usa la scelta automatica" }));
  fireEvent.click(await screen.findByRole("button", { name: "Sì, scelta automatica" }));
  await waitFor(() => expect(backend.config.coin).toBe("AUTO"));
  expect(await screen.findByText(/combinazione più redditizia/)).toBeInTheDocument();
});

test("help: question marks explain terms in plain words", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  fireEvent.click((await screen.findAllByRole("button", { name: "Cos'è: Perché è una stima" }))[0]);
  expect(await screen.findByRole("dialog", { name: "Perché è una stima" })).toHaveTextContent(/Prezzi e difficoltà/);
});

test("quick guide opens only on first launch, and is not in the menu", async () => {
  localStorage.removeItem("erebor.guideSeen");
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  expect(await screen.findByRole("heading", { name: "Benvenuto in Erebor" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Avanti" }));
  expect(await screen.findByRole("heading", { name: "Sceglie Erebor per te" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Indietro" }));
  fireEvent.click(await screen.findByRole("button", { name: "Salta" }));
  await waitFor(() => expect(screen.queryByRole("heading", { name: "Benvenuto in Erebor" })).not.toBeInTheDocument());
  expect(localStorage.getItem("erebor.guideSeen")).toBe("1");
  expect(screen.queryByRole("button", { name: "Guida rapida" })).not.toBeInTheDocument();
});

test("onboarding: earnings can go straight to a Bitcoin wallet, with the 'BTC without mining BTC' note", async () => {
  const backend = installMockBackend();
  render(<App />);
  await screen.findByText("Guadagna Bitcoin con il tuo computer");
  next();
  expect(await screen.findByText(/tu ricevi BTC/)).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: /In Bitcoin/ })).toHaveAttribute("aria-checked", "true");
  expect(screen.getByRole("radio", { name: /Account Kryptex/ })).toHaveAttribute("aria-checked", "true");
  fireEvent.click(screen.getByRole("radio", { name: /Il mio wallet Bitcoin/ }));
  const input = screen.getByRole("textbox");
  expect(input).toHaveAttribute("placeholder", "bc1…");
  fireEvent.change(input, { target: { value: "bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345" } });
  expect(await screen.findByText(/Indirizzo Bitcoin valido/)).toBeInTheDocument();
  next();
  expect(await screen.findByText("Il computer è pronto.", {}, { timeout: 4000 })).toBeInTheDocument();
  next();
  fireEvent.click(await screen.findByRole("button", { name: "Più tardi" }));
  await waitFor(() => expect(backend.config.wallet).toBe("bc1qqqqsyqcyq5rqwzqfpg9scrgwpugpzysn4v0345"));
});

test("Kryptex payout: the Bitcoin balance is spelled out in BTC", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true, wallet: "krxABC123" });
  render(<App />);
  expect(await screen.findByText("Accumulati su Kryptex")).toBeInTheDocument();
  expect(screen.getByText("0,00000011 BTC")).toBeInTheDocument();
});

test("home shows the whole computer live, no Erebor fee, no mining status in the sidebar", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  expect(await screen.findByText("Demo GPU")).toBeInTheDocument();
  expect(screen.getByText("Il tuo computer")).toBeInTheDocument();
  expect(screen.queryByText("Commissione Erebor")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Computer" })).not.toBeInTheDocument();
  expect(screen.getByRole("navigation", { name: "Erebor" })).not.toHaveTextContent("Mining inattivo");
  expect(await screen.findByText("6.7 / 12 GB")).toBeInTheDocument();
  // Live charts fill in as readings arrive (one every 2 s).
  expect(screen.getByText("Utilizzo CPU")).toBeInTheDocument();
  expect(screen.getAllByText("Utilizzo").length).toBeGreaterThan(0);
});

test("wallet: the payout address can be changed from a clear dialog", async () => {
  const backend = installMockBackend({ onboardingCompleted: true, wallet: VALID_WALLET, miner: { path: "/demo/xmrig", version: "6.26.0", sha256: null, verified: true } });
  render(<App />);
  await screen.findByText("Al mese (stima)");
  act(() => useApp.getState().setPage("wallet"));
  expect(await screen.findByText("Indirizzo Monero")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cambia" }));
  expect(await screen.findByRole("heading", { name: "Dove vuoi ricevere i guadagni?" })).toBeInTheDocument();
  // From a Monero address: Bitcoin first, then the service.
  expect(screen.getByRole("radio", { name: /Nella moneta minata/ })).toHaveAttribute("aria-checked", "true");
  fireEvent.click(screen.getByRole("radio", { name: /In Bitcoin/ }));
  fireEvent.click(screen.getByRole("radio", { name: /Account Kryptex/ }));
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "krxNEW123" } });
  expect(await screen.findByText(/ID Kryptex valido/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Salva" }));
  await waitFor(() => expect(backend.config.wallet).toBe("krxNEW123"));
});

test("wallet: an own wallet can be set for a single coin", async () => {
  const backend = installMockBackend({ onboardingCompleted: true, wallet: VALID_WALLET, miner: { path: "/demo/xmrig", version: "6.26.0", sha256: null, verified: true } });
  render(<App />);
  await screen.findByText("Al mese (stima)");
  act(() => useApp.getState().setPage("wallet"));
  expect(await screen.findByText(/Nessuna eccezione/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Aggiungi un wallet" }));
  fireEvent.change(await screen.findByRole("textbox", { name: "Cerca una moneta" }), { target: { value: "prl" } });
  const found = await screen.findAllByRole("option");
  expect(found).toHaveLength(1);
  fireEvent.click(found[0]);
  expect(await screen.findByRole("heading", { name: "Il tuo wallet Pearl" })).toBeInTheDocument();
  const input = screen.getByRole("textbox");
  fireEvent.change(input, { target: { value: "prl1pbad" } });
  expect(await screen.findByText(/Indirizzo non valido per questa moneta/)).toBeInTheDocument();
  const good = `prl1p${"q".repeat(58)}`;
  fireEvent.change(input, { target: { value: good } });
  expect(await screen.findByText("Indirizzo Pearl valido")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Salva" }));
  await waitFor(() => expect(backend.config.coinWallets).toEqual({ PRL: good }));
  expect(await screen.findByRole("button", { name: "Modifica Pearl" })).toBeInTheDocument();
});

test("notifications: repeats merge into one, the stack fans out on hover", async () => {
  installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  await screen.findByText("Al mese (stima)");
  act(() => {
    const { toast } = useApp.getState();
    toast("warning", "Pool non raggiungibile");
    toast("warning", "Pool non raggiungibile");
    toast("info", "Moneta cambiata");
  });
  expect(useApp.getState().toasts).toHaveLength(2);
  expect(screen.getByText("×2")).toBeInTheDocument();
  expect(screen.getByText("+1")).toBeInTheDocument();
  const stack = screen.getByRole("region", { name: "Notifiche" });
  fireEvent.mouseEnter(stack);
  expect(screen.queryByText("+1")).not.toBeInTheDocument();
  fireEvent.click(screen.getAllByRole("button", { name: "Chiudi" })[0]);
  await waitFor(() => expect(useApp.getState().toasts).toHaveLength(1));
});

test("mode: changing it while mining applies at once, no confirmation", async () => {
  const backend = installMockBackend({ onboardingCompleted: true, demoMode: true });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Avvia il mining" }));
  await waitFor(() => expect(backend.calls).toContain("start_mining"));
  fireEvent.click(await screen.findByRole("radio", { name: /Prestazioni/ }));
  await waitFor(() => expect(backend.config.performance.preset).toBe("performance"));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
