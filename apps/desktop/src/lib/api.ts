// Typed wrappers over Tauri commands. Types mirror the serde structs in
// crates/erebor-core (camelCase). Keep both sides in sync.
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

export type Preset = "eco" | "balanced" | "performance" | "custom";
export type RunState = "stopped" | "running" | "paused" | "error";
export type Target = "user" | "project";
export type EnergySource = "measured" | "estimated" | "unavailable";

export type MinerKind = "xmrig" | "srbminer" | "lolminer" | "cpuminer";
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
  /** Kryptex endpoint; `null`: not on Kryptex (own wallet, or unMineable). */
  host: string | null;
  port: number;
  minerFeePercent: number;
  unmineableHost: string | null;
  /** Mined by ASICs (SHA-256d, Scrypt): a PC earns almost nothing. */
  asic: boolean;
}

/** One thing mining a coin takes on this PC, met or not. */
export interface Requirement {
  kind: "hardware" | "system" | "miner" | "payout";
  /** "missing" blocks mining; "warn" works with a catch. */
  status: "ok" | "missing" | "warn";
  code: string;
}

export interface CoinOption {
  coin: Coin;
  blocker: string | null;
  requirements: Requirement[];
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
  pool: {
    id: string;
    host: string;
    port: number;
    tls: boolean;
    worker: string;
  };
  miner: MinerConfig;
  srbminer: MinerConfig;
  lolminer: MinerConfig;
  cpuminer: MinerConfig;
  /** Own wallet per coin id ("PRL" → "prl1p…"), overriding the main payout for that coin. */
  coinWallets: Record<string, string>;
  /** Pool for each coin with an own wallet (missing: Kryptex). */
  coinPools: Record<string, CoinPoolChoice>;
  /** Coins the automatic choice must not use. */
  autoExclude: string[];
  performance: {
    preset: Preset;
    customThreadsPercent: number;
    /** Graphics card mode, apart from the processor's (performance = full load). */
    gpuPreset: Preset;
    customGpuPercent: number;
    /** Mine only after this many minutes without input; null = always. */
    idleMinutes: number | null;
    /** Watts per GPU while mining (needs admin rights); null = untouched. */
    gpuPowerLimitW: number | null;
  };
  safety: { maxTempC: number; stopOnOverheat: boolean; autoRestart: boolean };
  energy: {
    pricePerKwh: number | null;
    currency: "EUR" | "USD";
    /** Prices from the user's bill by time of day; the first match wins. */
    bands: PriceBand[];
    /** January = 0. */
    heatingMonths: boolean[];
    heatingCreditPercent: number;
    solar: Solar;
    thermostat: Thermostat;
    stopIfUnprofitable: boolean;
    minProfitPerDay: number;
  };
  schedule: {
    enabled: boolean;
    startMinute: number;
    endMinute: number;
    days: boolean[];
  };
  benchmark: {
    hashrate: number;
    threadsPercent: number;
    durationSecs: number;
    at: number;
    minerVersion: string | null;
  } | null;
  checkUpdates: boolean;
  autoUpdateMiners: boolean;
  startOnLaunch: boolean;
  defenderAllowed: boolean;
  notify: Notify;
  /** Extra share of mining time given to Erebor on top of the fee (0 = off). */
  donationPercent: number;
  /** Main screen: this computer's mining or every managed device; "" until chosen. */
  defaultView: "" | "mining" | "devices";
}

export interface CoinPoolChoice {
  /** Catalog pool id or "custom"; "" = Kryptex. */
  pool: string;
  region: string;
  host: string;
  port: number;
  tls: boolean;
  /** Catalog pool used while the main one is unreachable; "" = none. */
  backup: string;
}

/** One pool a coin can be mined on with an own wallet (verified catalog). */
export interface CoinPool {
  pool: string;
  name: string;
  /** [region, host]: "eu" | "us" | "asia" | "local". */
  hosts: [string, string][];
  port: number;
  feePercent: number | null;
  scheme: string;
  minPayout: number | null;
  login: "dot" | "slash" | "passWorker" | "workerOnly";
  balanceUrl: string | null;
  needsAddress: boolean;
}

export interface PriceBand {
  startMinute: number;
  endMinute: number;
  days: boolean[];
  price: number;
}

export interface Solar {
  enabled: boolean;
  url: string;
  token: string;
  field: string;
  exportPositive: boolean;
  feedInPrice: number;
}

export interface Thermostat {
  enabled: boolean;
  url: string;
  token: string;
  field: string;
  onBelowC: number;
  offAboveC: number;
}

export interface Notify {
  telegramToken: string;
  telegramChat: string;
}

export interface AppInfo {
  version: string;
  platform: string;
  devFeePercent: number;
  devFeeWalletConfigured: boolean;
  maxDonationPercent: number;
  /** Erebor's unMineable referral code, added to unMineable payouts. */
  unmineableReferral: string;
  kryptexSignupUrl: string;
  xmrigPinnedVersion: string;
  xmrigPinnedAvailable: boolean;
  xmrigDonatePercent: number;
  srbminerPinnedVersion: string;
  srbminerPinnedAvailable: boolean;
  lolminerPinnedVersion: string;
  lolminerPinnedAvailable: boolean;
  cpuminerPinnedVersion: string;
  cpuminerPinnedAvailable: boolean;
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
  kind: "install" | "benchmark" | "tune";
  progress: number;
  done: boolean;
  error: string | null;
  value: number | null;
}

export interface Market {
  network: {
    difficulty: number;
    blockReward: number;
    height: number | null;
  } | null;
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
  /** Mining is on but waiting for this. */
  held: "in_use" | "warm" | "unprofitable" | null;
  /** What a kWh costs for mining right now. */
  energyPrice: number | null;
  /** Watts exported by the meter (negative = importing). */
  solarExportW: number | null;
  solarError: string | null;
  /** Same order as `hardware.gpus`. */
  gpus: { hashrate: number | null; expected: number | null }[];
  roomTempC: number | null;
  roomError: string | null;
  idleSecs: number | null;
  /** Stopped for a while: seconds until it starts again by itself. */
  resumeInSecs: number | null;
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
  hash: {
    avg: number | null;
    max: number | null;
    accepted: number;
    rejected: number;
  };
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
  | {
      code:
        "empty" | "looks_like_secret" | "invalid_characters" | "invalid_length";
    }
  | {
      code:
        "invalid_encoding" | "unknown_prefix" | "bad_checksum" | "kryptex_only";
    }
  | { code: "wrong_network"; network: string };

export interface AddressInfo {
  network: "mainnet" | "testnet" | "stagenet";
  kind:
    | "standard"
    | "integrated"
    | "subaddress"
    | "kryptex"
    | "bitcoin"
    | "converted";
}

/** Inside the Erebor app (Tauri) rather than a plain browser (tests, dev preview). */
export const inApp = () => "__TAURI_INTERNALS__" in window;

/** Open a link in the system browser (a new tab in the browser-only dev UI). */
export function openExternal(url: string) {
  if (inApp()) openUrl(url).catch(() => {});
  else window.open(url, "_blank", "noopener");
}

/** One engine command (own engine, or the `erebor` agent running on this computer). */
function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>("call", { cmd, args });
}

/** This app only: start at login. */
export const local = {
  loginItem: () => invoke<boolean>("login_item"),
  setLoginItem: (enabled: boolean) =>
    invoke<void>("set_login_item", { enabled }),
};

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  getConfig: () => call<AppConfig>("get_config"),
  updateConfig: (config: AppConfig) =>
    call<AppConfig>("update_config", { config }),
  validateWallet: (address: string) =>
    call<AddressInfo>("validate_wallet", { address }),
  coinOptions: () => call<CoinOption[]>("coin_options"),
  validateCoinWallet: (coin: string, address: string) =>
    call<void>("validate_coin_wallet", { coin, address }),
  poolCatalog: () => call<PoolInfo[]>("pool_catalog"),
  checkPool: (host: string, port: number) =>
    call<number>("check_pool", { host, port }),
  poolAccount: () => call<PoolAccount>("pool_account"),
  detectHardware: () => call<HardwareInfo>("detect_hardware"),
  installMiner: (kind: MinerKind = "xmrig") =>
    call<void>("install_miner", { kind }),
  allowInDefender: () => call<AppConfig>("allow_in_defender"),
  selectMiner: (path: string, kind: MinerKind = "xmrig") =>
    call<AppConfig>("select_miner", { path, kind }),
  runBenchmark: () => call<void>("run_benchmark"),
  cancelJob: () => call<void>("cancel_job"),
  startMining: () => call<void>("start_mining"),
  stopMining: () => call<void>("stop_mining"),
  resumeMining: () => call<void>("resume_mining"),
  getStatus: () => call<Status>("get_status"),
  getLogs: () => call<string[]>("get_logs"),
  getStats: (rangeSecs: number) =>
    call<StatsReport>("get_stats", { rangeSecs }),
  exportConfig: () => call<string>("export_config"),
  /** Before an app update restarts Erebor: mining resumes in the new version. */
  prepareUpdate: () => call<void>("prepare_update"),
  stopFor: (minutes: number) => call<void>("stop_for", { minutes }),
  testNotify: () => call<void>("test_notify"),
  tunePower: () => call<void>("tune_power"),
  coinPools: () => call<Record<string, CoinPool[]>>("coin_pools"),
  /** Writes the CSV to Downloads and returns its path. */
  saveCsv: (rangeSecs: number) => call<string>("save_csv", { rangeSecs }),
  deleteAllData: () => call<AppConfig>("delete_all_data"),
  dataDir: () => call<string>("data_dir"),
};

// ---------- devices managed from here (crates/erebor-core/src/controller.rs) ----------

export type DeviceKind = "local_pc" | "remote_pc" | "server" | "gpu_rig" | "cpu_rig" | "asic";
export type Conn = "online" | "offline" | "connecting" | "unreachable" | "unauthorized";
export type MiningState = "idle" | "starting" | "mining" | "stopping" | "error" | "maintenance";

/** Whether an action works on a device; `reason` is an error code when it doesn't. */
export interface Capability {
  supported: boolean;
  reason: string | null;
}

export interface DeviceActions {
  start: Capability;
  stop: Capability;
  restart: Capability;
  configure: Capability;
  setPool: Capability;
  remove: Capability;
  rename: Capability;
}

export interface DeviceRow {
  id: string;
  kind: DeviceKind;
  name: string;
  address: string;
  group: number | null;
  groupName: string | null;
  tags: string[];
  conn: Conn;
  mining: MiningState;
  error: string | null;
  simulated: boolean;
  hardware: string;
  os: string | null;
  version: string | null;
  algo: string | null;
  coin: string | null;
  pool: string | null;
  hashrate: number | null;
  tempC: number | null;
  powerW: number | null;
  powerMeasured: boolean;
  revenueDay: number | null;
  costDay: number | null;
  netDay: number | null;
  currency: string;
  accepted: number;
  rejected: number;
  lastSeen: number | null;
  gateway: string | null;
  actions: DeviceActions;
}

export interface FleetOverview {
  total: number;
  online: number;
  offline: number;
  mining: number;
  simulated: number;
  powerW: number;
  powerMeasuredW: number;
  powerDevices: number;
  /** Summed per algorithm only: hashrates of different algorithms never add up. */
  hashrate: { algo: string; hashrate: number; devices: number }[];
  money: { currency: string; revenueDay: number | null; costDay: number | null; netDay: number | null }[];
}

export interface DeviceGroup {
  id: number;
  name: string;
  energyPrice: number | null;
}

export interface FleetList {
  overview: FleetOverview;
  devices: DeviceRow[];
  groups: DeviceGroup[];
  /** This installation's certificate fingerprint (short form). */
  identity: string;
}

export interface AsicPoolSpec {
  url: string;
  user: string;
  password: string;
}

export interface MiningProfile {
  id: number;
  name: string;
  /** Partial configuration merged into each computer's. */
  config: Record<string, unknown>;
  asic: AsicPoolSpec | null;
  algo: string | null;
  start: boolean;
}

export interface OpResult {
  id: string;
  name: string;
  state: "completed" | "failed" | "skipped" | "rolled_back";
  error: string | null;
}

export interface FleetEvent {
  id: number;
  ts: number;
  deviceId: string | null;
  deviceName: string | null;
  level: "info" | "warning" | "error";
  code: string;
  detail: string | null;
}

export interface FleetSettings {
  currency: string;
  energyPrice: number | null;
  maxTempC: number;
  asicMaxTempC: number;
  hashrateDropPercent: number;
  powerRisePercent: number;
  rejectedPercent: number;
  cooldownMinutes: number;
  retentionDays: number;
  discovery: boolean;
}

export interface FleetHistory {
  bucketSecs: number;
  points: { ts: number; powerW: number | null; tempMax: number | null; revenueDay: number | null; costDay: number | null }[];
  hashrate: Record<string, { ts: number; hashrate: number }[]>;
}

export interface FoundAgent {
  found: { address: string; id: string; name: string; version: string; fingerprint: string; pairing: boolean };
  paired: boolean;
}

export interface AsicPool {
  index: number;
  url: string;
  user: string;
  status: string;
  active: boolean;
  priority: number;
  accepted: number;
  rejected: number;
}

export interface AsicStatus {
  id: string;
  host: string;
  port: number;
  name: string;
  vendor: "antminer" | "whatsminer" | "cgminer";
  model: string | null;
  firmware: string | null;
  apiVersion: string | null;
  simulated: boolean;
  online: boolean;
  error: string | null;
  algo: string | null;
  hashrate: number | null;
  hashrateAvg: number | null;
  accepted: number;
  rejected: number;
  hardwareErrors: number;
  uptimeSecs: number | null;
  boards: { index: number; hashrate: number | null; tempC: number | null; chipTempC: number | null; chips: number | null }[];
  fansRpm: number[];
  tempMaxC: number | null;
  powerW: number | null;
  pools: AsicPool[];
}

export const fleet = {
  list: () => call<FleetList>("fleet_list"),
  refresh: () => call<void>("fleet_refresh"),
  discover: () => call<FoundAgent[]>("fleet_discover"),
  addAgent: (address: string, code: string, kind?: DeviceKind) =>
    call<unknown>("fleet_add_agent", { address, code, kind }),
  addAsic: (host: string, port: number | null, name: string | null, via: string | null) =>
    call<unknown>("fleet_add_asic", { host, port, name, via }),
  scan: (cidr: string, via: string | null) =>
    call<{ host: string; port: number }[]>("fleet_scan", { cidr, via }),
  remove: (id: string) => call<void>("fleet_remove", { id }),
  rename: (id: string, name: string) => call<void>("fleet_rename", { id, name }),
  setKind: (id: string, kind: DeviceKind) => call<void>("fleet_set_kind", { id, kind }),
  setPrice: (id: string, price: number | null) => call<void>("fleet_set_price", { id, price }),
  mining: (ids: string[], action: "start" | "stop" | "restart") =>
    call<OpResult[]>("fleet_mining", { ids, action }),
  applyProfile: (ids: string[], profileId: number) =>
    call<OpResult[]>("fleet_apply", { ids, profileId: String(profileId) }),
  /** An unsaved profile (change pool, wallet…), checked and applied the same way. */
  applyPatch: (ids: string[], profile: Omit<MiningProfile, "id">) =>
    call<OpResult[]>("fleet_apply", { ids, profile: { id: 0, ...profile } }),
  assignGroup: (ids: string[], group: number | null) =>
    call<number>("fleet_assign_group", { ids, group }),
  tag: (ids: string[], tags: string[], remove = false) =>
    call<void>("fleet_tag", { ids, tags, remove }),
  saveGroup: (group: DeviceGroup) => call<DeviceGroup>("fleet_save_group", { group }),
  deleteGroup: (id: number) => call<void>("fleet_delete_group", { id }),
  profiles: () => call<MiningProfile[]>("fleet_profiles"),
  saveProfile: (profile: MiningProfile) => call<MiningProfile>("fleet_save_profile", { profile }),
  deleteProfile: (id: number) => call<void>("fleet_delete_profile", { id }),
  config: (id: string) => call<AppConfig>("fleet_config", { id }),
  setConfig: (id: string, config: AppConfig) => call<unknown>("fleet_set_config", { id, config }),
  query: <T>(id: string, cmd: "logs" | "coins" | "stats" | "miners" | "telemetry" | "asic" | "audit", args?: unknown) =>
    call<T>("fleet_query", { id, cmd, args }),
  history: (id: string | null, rangeSecs: number) =>
    call<FleetHistory>("fleet_history", { id, rangeSecs }),
  events: (sinceId = 0, limit = 200) => call<FleetEvent[]>("fleet_events", { sinceId, limit }),
  settings: () => call<FleetSettings>("fleet_settings"),
  setSettings: (settings: FleetSettings) => call<FleetSettings>("fleet_set_settings", { settings }),
  exportAll: (secrets = false) => call<unknown>("fleet_export", { secrets }),
  importAll: (data: unknown) => call<{ added: number }>("fleet_import", { data }),
};

/** Normalise any thrown command error into a string code/message. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "code" in e)
    return String((e as { code: string }).code);
  return e instanceof Error ? e.message : String(e);
}
