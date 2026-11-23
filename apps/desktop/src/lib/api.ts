// Typed wrappers over Tauri commands. Types mirror the serde structs in
// crates/erebor-core (camelCase). Keep both sides in sync.
import { invoke } from "@tauri-apps/api/core";

export type Preset = "eco" | "balanced" | "performance" | "custom";
export type RunState = "stopped" | "running" | "paused" | "error";
export type Target = "user" | "project";
export type EnergySource = "measured" | "estimated" | "unavailable";

export type MinerKind = "xmrig" | "srbminer";
export interface MinerConfig {
  path: string | null;
  version: string | null;
  sha256: string | null;
  verified: boolean;
}

export interface Coin {
  id: string;
  name: string;
  algo: string;
  device: "cpu" | "gpu";
  miner: MinerKind;
  host: string;
  port: number;
  minerFeePercent: number;
  unmineableHost: string | null;
}

export interface CoinOption {
  coin: Coin;
  blocker: string | null;
  minerInstalled: boolean;
  hashrate: number | null;
  btcPerDay: number | null;
  valuePerDay: number | null;
}

export interface AppConfig {
  version: number;
  onboardingCompleted: boolean;
  demoMode: boolean;
  language: "it" | "en";
  theme: "dark" | "light" | "system";
  coin: string;
  wallet: string;
  pool: { id: string; host: string; port: number; tls: boolean; worker: string };
  miner: MinerConfig;
  srbminer: MinerConfig;
  /** Own wallet per coin id ("PRL" → "prl1p…"), overriding the main payout for that coin. */
  coinWallets: Record<string, string>;
  performance: { preset: Preset; customThreadsPercent: number };
  safety: { maxTempC: number; stopOnOverheat: boolean; autoRestart: boolean };
  energy: {
    pricePerKwh: number | null;
    currency: "EUR" | "USD";
    estimatedWatts: number | null;
    stopIfUnprofitable: boolean;
    minProfitPerDay: number;
  };
  schedule: { enabled: boolean; startMinute: number; endMinute: number; days: boolean[] };
  benchmark: {
    hashrate: number;
    threadsPercent: number;
    durationSecs: number;
    at: number;
    minerVersion: string | null;
  } | null;
  checkUpdates: boolean;
  startOnLaunch: boolean;
  remote: { enabled: boolean; port: number };
}

export interface AppInfo {
  version: string;
  platform: string;
  devFeePercent: number;
  devFeeWalletConfigured: boolean;
  xmrigPinnedVersion: string;
  xmrigPinnedAvailable: boolean;
  xmrigDonatePercent: number;
  srbminerPinnedVersion: string;
  srbminerPinnedAvailable: boolean;
  repoUrl: string | null;
  hostname: string;
}

export interface MinerStats {
  hashrate10s: number | null;
  hashrate60s: number | null;
  hashrate15m: number | null;
  hashrateMax: number | null;
  accepted: number;
  rejected: number;
  pool: string | null;
  connected: boolean;
  tls: boolean | null;
  pingMs: number | null;
  algo: string | null;
  version: string | null;
  paused: boolean;
  threads: number | null;
}

export interface MinerInfo {
  id: string;
  name: string;
  version: string | null;
  algorithms: string[];
  supportsPause: boolean;
  builtinDonationPercent: number;
}

export interface Earnings {
  grossXmr: number;
  xmrigDonationXmr: number;
  poolFeeXmr: number | null;
  netXmr: number;
  ereborFeeSecs: number;
  ereborFeeXmr: number;
  miningSecs: number;
  price: number | null;
  netBtc: number;
  netValue: number | null;
  energyKwh: number | null;
  energySource: EnergySource;
  energyCost: number | null;
  netProfit: number | null;
  complete: boolean;
}

export interface DevFeeStatus {
  enabled: boolean;
  percent: number;
  walletConfigured: boolean;
  active: Target;
  userSecs: number;
  projectSecs: number;
  effectivePercent: number;
}

export interface Alert {
  id: number;
  ts: number;
  level: "info" | "warning" | "error";
  code: string;
  detail: string | null;
}

export interface JobStatus {
  kind: "install" | "benchmark";
  progress: number;
  done: boolean;
  error: string | null;
  value: number | null;
}

export interface Market {
  network: { difficulty: number; blockReward: number; height: number | null } | null;
  price: { eur: number; usd: number; source: string } | null;
  btc: { eur: number; usd: number } | null;
  updated: number | null;
  error: string | null;
}

export interface LiveHardware {
  cpuUsage: number;
  memUsedBytes: number;
  memTotalBytes: number;
  cpuTempC: number | null;
  powerW: number | null;
  gpus: GpuLive[];
}

export interface GpuLive {
  name: string;
  vendor: string;
  tempC: number | null;
  utilPercent: number | null;
  powerW: number | null;
  fanPercent: number | null;
  memUsedMb: number | null;
  memTotalMb: number | null;
}

export interface Status {
  demo: boolean;
  state: RunState;
  error: string | null;
  target: Target;
  miner: MinerInfo | null;
  stats: MinerStats | null;
  sessionStarted: number | null;
  sessionSecs: number;
  devfee: DevFeeStatus;
  xmrigDonatePercent: number;
  poolFeePercent: number | null;
  hardware: LiveHardware;
  powerSource: EnergySource;
  powerW: number | null;
  earnings24h: Earnings;
  market: Market;
  job: JobStatus | null;
  alerts: Alert[];
  restartInSecs: number | null;
  startBlocker: string | null;
  coin: Coin;
  autoCoin: boolean;
  projection: Projection | null;
  mined: { coin: string; amount: number; secs: number }[];
  /** `mined` valued in BTC at today's prices (what Kryptex/unMineable pay out). */
  minedBtc: number | null;
  advice: Advice;
}

/** What earns the most BTC on this PC: coin (once measured) and how to be paid. */
export interface Advice {
  device: "cpu" | "gpu";
  coin: Coin | null;
  payout: "kryptex" | "bitcoin";
  btcPerDay: number | null;
  currentBtcPerDay: number | null;
}

export interface Projection {
  hashrate: number;
  live: boolean;
  btcPerDay: number;
  valuePerDay: number | null;
  energyCostPerDay: number | null;
  netPerDay: number | null;
}

export interface GpuInfo {
  name: string;
  vendor: string | null;
  driver: string | null;
  vramMb: number | null;
  source: string;
}

export interface HardwareInfo {
  cpuBrand: string;
  physicalCores: number | null;
  logicalThreads: number;
  arch: string;
  ramTotalBytes: number;
  osName: string;
  osVersion: string;
  gpus: GpuInfo[];
  temperatureSupported: boolean;
  powerSupported: boolean;
  gpuMiningSupported: boolean;
}

export interface PoolInfo {
  id: string;
  name: string;
  host: string;
  port: number;
  tls: boolean;
  feePercent: number;
  website: string;
  apiBase: string;
}

export interface PoolAccount {
  unpaidXmr: number | null;
  paidXmr: number | null;
  poolHashrate: number | null;
  validShares: number | null;
  invalidShares: number | null;
  payments: { ts: number; amountXmr: number; txHash: string | null }[];
}

export interface SeriesPoint {
  ts: number;
  hashrate: number | null;
  userXmr: number;
  userBtc: number;
  energyWh: number | null;
  tempC: number | null;
}

export interface StatsReport {
  from: number;
  to: number;
  hash: { avg: number | null; max: number | null; accepted: number; rejected: number };
  earnings: Earnings;
  series: SeriesPoint[];
  sessions: {
    id: number;
    started: number;
    ended: number | null;
    endReason: string | null;
    miner: string;
    pool: string;
  }[];
}

export type WalletError =
  | { code: "empty" | "looks_like_secret" | "invalid_characters" | "invalid_length" }
  | { code: "invalid_encoding" | "unknown_prefix" | "bad_checksum" }
  | { code: "wrong_network"; network: string };

export interface AddressInfo {
  network: "mainnet" | "testnet" | "stagenet";
  kind: "standard" | "integrated" | "subaddress" | "kryptex" | "bitcoin";
}

/** Another computer running Erebor (app with remote access, or node). */
export interface Computer {
  id: string;
  name: string;
  url: string;
  token: string;
}

export interface RemoteInfo {
  viaNode: boolean;
  running: boolean;
  port: number | null;
  url: string | null;
  token: string | null;
  error: string | null;
  loginItem: boolean;
  nodeService: boolean;
}

/** Inside the Erebor app (Tauri) rather than a browser served by a node. */
export const inApp = () => "__TAURI_INTERNALS__" in window;

let target: Computer | null = null;
/** Computer the engine commands go to: `null` is this one. */
export function setTarget(c: Computer | null) {
  target = c;
}
export const isRemote = () => target != null || !inApp();

const TOKEN_KEY = "erebor.token";
export const webToken = {
  get: () => {
    try {
      return localStorage.getItem(TOKEN_KEY) ?? "";
    } catch {
      return "";
    }
  },
  set: (t: string) => {
    try {
      localStorage.setItem(TOKEN_KEY, t.trim());
    } catch {
      /* the token is asked again next time */
    }
  },
};

/** One engine command, wherever the selected computer is. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inApp()) {
    return target
      ? invoke<T>("remote_call", { url: target.url, token: target.token, cmd, args })
      : invoke<T>("call", { cmd, args });
  }
  // Browser (e.g. a phone) on the node's own web page: same origin, token auth.
  const r = await fetch(`/rpc/${cmd}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: `Bearer ${webToken.get()}` },
    body: JSON.stringify(args ?? null),
  }).catch(() => {
    throw "remote_unreachable";
  });
  const body = await r.json().catch(() => "remote_bad_response");
  if (!r.ok) throw body;
  return body as T;
}

/** This app only (not the selected computer): remote access and start at login. */
export const local = {
  remoteInfo: () => invoke<RemoteInfo>("remote_info"),
  setLoginItem: (enabled: boolean) => invoke<void>("set_login_item", { enabled }),
  ping: (c: Computer) => invoke<Status>("remote_call", { url: c.url, token: c.token, cmd: "get_status" }),
};

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  getConfig: () => call<AppConfig>("get_config"),
  updateConfig: (config: AppConfig) => call<AppConfig>("update_config", { config }),
  validateWallet: (address: string) => call<AddressInfo>("validate_wallet", { address }),
  coinOptions: () => call<CoinOption[]>("coin_options"),
  validateCoinWallet: (coin: string, address: string) => call<void>("validate_coin_wallet", { coin, address }),
  poolCatalog: () => call<PoolInfo[]>("pool_catalog"),
  checkPool: (host: string, port: number) => call<number>("check_pool", { host, port }),
  poolAccount: () => call<PoolAccount>("pool_account"),
  detectHardware: () => call<HardwareInfo>("detect_hardware"),
  installMiner: (kind: MinerKind = "xmrig") => call<void>("install_miner", { kind }),
  selectMiner: (path: string, kind: MinerKind = "xmrig") => call<AppConfig>("select_miner", { path, kind }),
  runBenchmark: () => call<void>("run_benchmark"),
  cancelJob: () => call<void>("cancel_job"),
  startMining: () => call<void>("start_mining"),
  stopMining: () => call<void>("stop_mining"),
  pauseMining: () => call<void>("pause_mining"),
  resumeMining: () => call<void>("resume_mining"),
  getStatus: () => call<Status>("get_status"),
  getLogs: () => call<string[]>("get_logs"),
  getStats: (rangeSecs: number) => call<StatsReport>("get_stats", { rangeSecs }),
  exportConfig: () => call<string>("export_config"),
  deleteAllData: () => call<AppConfig>("delete_all_data"),
  dismissAlert: (id: number) => call<void>("dismiss_alert", { id }),
  dataDir: () => call<string>("data_dir"),
};

/** Normalise any thrown command error into a string code/message. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "code" in e) return String((e as { code: string }).code);
  return e instanceof Error ? e.message : String(e);
}
