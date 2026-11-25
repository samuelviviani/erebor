// Simulated backend for tests and browser-only UI development. Mirrors the
// Tauri command contract in src-tauri/src/main.rs. Every value here is fake and
// the status is always flagged `demo: true`.
import { mockIPC } from "@tauri-apps/api/mocks";
import type {
  AppConfig,
  AppInfo,
  Coin,
  CoinOption,
  Earnings,
  JobStatus,
  Status,
  StatsReport,
} from "../lib/api";

export const VALID_WALLET =
  "48edfHu7V9Z84YzzMa6fUueoELZ9ZRXq9VetWzYGzKt52XU5xvqgzYnDK9URnRoJMk1j8nLwEVsaSWJ4fhdUyZijBGUicoD";

export function defaultConfig(): AppConfig {
  return {
    version: 1,
    onboardingCompleted: false,
    demoMode: false,
    language: "en",
    theme: "light",
    coin: "AUTO",
    wallet: "",
    pool: {
      id: "kryptex",
      host: "xmr.kryptex.network",
      port: 7029,
      tls: false,
      worker: "",
    },
    miner: { path: null, version: null, sha256: null, verified: false },
    srbminer: { path: null, version: null, sha256: null, verified: false },
    lolminer: { path: null, version: null, sha256: null, verified: false },
    cpuminer: { path: null, version: null, sha256: null, verified: false },
    coinWallets: {},
    coinPools: {},
    autoExclude: [],
    performance: {
      preset: "balanced",
      customThreadsPercent: 50,
      idleMinutes: null,
      gpuPowerLimitW: null,
    },
    safety: { maxTempC: 85, stopOnOverheat: true, autoRestart: false },
    energy: {
      pricePerKwh: null,
      currency: "EUR",
      bands: [],
      heatingMonths: Array(12).fill(false),
      heatingCreditPercent: 0,
      solar: {
        enabled: false,
        url: "",
        token: "",
        field: "",
        exportPositive: false,
        feedInPrice: 0,
      },
      thermostat: {
        enabled: false,
        url: "",
        token: "",
        field: "state",
        onBelowC: 20,
        offAboveC: 21,
      },
      stopIfUnprofitable: false,
      minProfitPerDay: 0,
    },
    schedule: {
      enabled: false,
      startMinute: 1320,
      endMinute: 420,
      days: [true, true, true, true, true, true, true],
    },
    benchmark: null,
    checkUpdates: false,
    autoUpdateMiners: true,
    startOnLaunch: false,
    defenderAllowed: false,
    notify: { telegramToken: "", telegramChat: "" },
    donationPercent: 0,
  };
}

const info: AppInfo = {
  version: "0.1.0",
  platform: "mock",
  devFeePercent: 2,
  devFeeWalletConfigured: true,
  maxDonationPercent: 3,
  unmineableReferral: "v3ni-3jdo",
  kryptexSignupUrl: "https://pool.kryptex.com/?ref=e17d6146",
  xmrigPinnedVersion: "6.26.0",
  xmrigPinnedAvailable: true,
  xmrigDonatePercent: 1,
  srbminerPinnedVersion: "3.8.8",
  srbminerPinnedAvailable: true,
  lolminerPinnedVersion: "1.98a",
  lolminerPinnedAvailable: true,
  cpuminerPinnedVersion: "v26.1",
  cpuminerPinnedAvailable: false,
  repoUrl: null,
  hostname: "Demo-PC",
};

const emptyEarnings: Earnings = {
  grossXmr: 0,
  xmrigDonationXmr: 0,
  poolFeeXmr: 0,
  netXmr: 0,
  netBtc: 0,
  ereborFeeSecs: 0,
  ereborFeeXmr: 0,
  miningSecs: 0,
  price: null,
  netValue: null,
  energyKwh: null,
  energySource: "unavailable",
  energyCost: null,
  netProfit: null,
  complete: true,
};

const COINS: Coin[] = [
  {
    id: "PRL",
    name: "Pearl",
    algo: "pearlhash",
    device: "gpu",
    miner: "srbminer",
    host: "prl.kryptex.network",
    port: 7048,
    minerFeePercent: 2,
    unmineableHost: "pearlpow.unmineable.com",
    asic: false,
  },
  {
    id: "ETC",
    name: "Ethereum Classic",
    algo: "etchash",
    device: "gpu",
    miner: "srbminer",
    host: "etc.kryptex.network",
    port: 7033,
    minerFeePercent: 0.65,
    unmineableHost: "etchash.unmineable.com",
    asic: false,
  },
  {
    id: "XMR",
    name: "Monero",
    algo: "rx/0",
    device: "cpu",
    miner: "xmrig",
    host: "xmr.kryptex.network",
    port: 7029,
    minerFeePercent: 1,
    unmineableHost: "rx.unmineable.com",
    asic: false,
  },
  {
    id: "FIRO",
    name: "Firo",
    algo: "firopow",
    device: "gpu",
    miner: "srbminer",
    host: null,
    port: 0,
    minerFeePercent: 0.85,
    unmineableHost: null,
    asic: false,
  },
];

export interface MockState {
  config: AppConfig;
  running: boolean;
  paused: boolean;
  startedAt: number;
  calls: string[];
  job: JobStatus | null;
  installing: "xmrig" | "srbminer";
}

export function installMockBackend(initial?: Partial<AppConfig>): MockState {
  const st: MockState = {
    config: { ...defaultConfig(), ...initial },
    running: false,
    paused: false,
    startedAt: 0,
    calls: [],
    job: null,
    installing: "xmrig",
  };
  const now = () => Math.floor(Date.now() / 1000);

  const blocker = (): string | null => {
    const c = st.config;
    if (!c.demoMode && !c.wallet) return "wallet_missing";
    if (!c.demoMode && !c.miner.path) return "miner_not_configured";
    return null;
  };

  const status = (): Status => {
    const secs = st.running ? now() - st.startedAt : 0;
    const h =
      st.running && !st.paused
        ? 4200 + 120 * Math.sin(Date.now() / 7000)
        : null;
    // Gentle noise so live charts move like real sensors.
    const n = (amp: number) =>
      amp * Math.sin(Date.now() / 3100) +
      (amp / 2) * Math.sin(Date.now() / 1300);
    return {
      demo: true,
      state: st.running ? (st.paused ? "paused" : "running") : "stopped",
      error: null,
      target: "user",
      miner: st.running
        ? {
            id: "demo",
            name: "Demo (simulated)",
            version: "demo",
            algorithms: ["rx/0"],
            supportsPause: true,
            builtinDonationPercent: 0,
          }
        : null,
      // The first seconds after a (re)start: miner launched, no stats yet.
      stats:
        st.running && secs >= 3
          ? {
              hashrate10s: h,
              hashrate60s: 4200,
              hashrate15m: 4180,
              hashrateMax: 4390,
              accepted: Math.floor(secs / 15),
              rejected: 0,
              pool: `${st.config.pool.host}:${st.config.pool.port}`,
              connected: true,
              tls: true,
              pingMs: 42,
              algo: "rx/0",
              version: "demo",
              paused: st.paused,
              threads: 4,
            }
          : null,
      sessionStarted: st.running ? st.startedAt : null,
      sessionSecs: secs,
      devfee: {
        enabled: true,
        percent: 2,
        walletConfigured: true,
        active: "user",
        userSecs: secs,
        projectSecs: 0,
        effectivePercent: 0,
      },
      xmrigDonatePercent: 0,
      poolFeePercent: 0.6,
      hardware: {
        cpuUsage: st.running ? 52 + n(6) : 6 + n(2),
        memUsedBytes: 9.2e9,
        memTotalBytes: 16e9,
        cpuTempC: st.running ? 68 + n(2) : 44 + n(1),
        powerW: null,
        gpus: [
          {
            name: "Demo GPU",
            vendor: "NVIDIA",
            tempC: st.running ? 63 + n(1.5) : 38,
            utilPercent: st.running ? 96 + n(2) : 2,
            powerW: st.running ? 178 + n(8) : 14,
            fanPercent: st.running ? 55 : 30,
            memUsedMb: 6900,
            memTotalMb: 12282,
          },
        ],
      },
      powerSource: st.running ? "measured" : "unavailable",
      powerW: st.running ? 178 : null,
      earnings24h:
        secs > 0
          ? {
              ...emptyEarnings,
              miningSecs: secs,
              grossXmr: 0.000012,
              netXmr: 0.0000119,
              price: 150,
              netValue: 0.0018,
            }
          : emptyEarnings,
      market: {
        network: { difficulty: 300e9, blockReward: 0.6, height: null },
        price: { eur: 150, usd: 160, source: "demo" },
        btc: { eur: 58000, usd: 62000 },
        updated: now(),
        error: null,
      },
      job: st.job,
      alerts: [],
      restartInSecs: null,
      startBlocker: st.running ? null : blocker(),
      coin: COINS.find((c) => c.id === st.config.coin) ?? COINS[2],
      autoCoin: st.config.coin === "AUTO",
      minedBtc: 0.0000423 * 0.0026 + secs * 1e-11,
      mined:
        secs > 0 || st.running
          ? [{ coin: "XMR", amount: 0.0000423, secs: 600 + secs }]
          : [{ coin: "XMR", amount: 0.0000423, secs: 600 }],
      projection: {
        hashrate: 4200,
        live: st.running,
        btcPerDay: 2.1e-6,
        valuePerDay: 0.12,
        energyCostPerDay: null,
        netPerDay: null,
      },
      advice: {
        device: "cpu",
        coin: COINS.find((c) => c.id === "XMR")!,
        payout: "kryptex",
        btcPerDay: 2.1e-6,
        currentBtcPerDay:
          st.config.coin === "AUTO" || st.config.coin === "XMR" ? 2.1e-6 : null,
      },
      held: null,
      energyPrice: st.config.energy.pricePerKwh,
      solarExportW: st.config.energy.solar.enabled ? 420 : null,
      solarError: null,
      gpus: [{ hashrate: st.running ? 2.9e9 : null, expected: 3.1e9 }],
      roomTempC: st.config.energy.thermostat.enabled ? 19.6 : null,
      roomError: null,
      idleSecs: 0,
      resumeInSecs: null,
    };
  };

  const stats = (range: number): StatsReport => {
    const to = now();
    const series = Array.from({ length: 60 }, (_, i) => ({
      ts: to - range + (i * range) / 60,
      hashrate: 4100 + 150 * Math.sin(i / 5),
      userXmr: 2e-7 * (1 + Math.sin(i / 7) * 0.1),
      userBtc: 3e-8 * (1 + Math.sin(i / 7) * 0.1),
      energyWh: null,
      tempC: 66,
    }));
    return {
      from: to - range,
      to,
      hash: { avg: 4150, max: 4390, accepted: 312, rejected: 1 },
      earnings: (() => {
        // Demo GPU draw (178 W) × range; cost only with an electricity price.
        const kwhPrice = st.config.energy.pricePerKwh;
        const energyKwh = (178 * range) / 3_600_000;
        const energyCost =
          energyKwh != null && kwhPrice != null ? energyKwh * kwhPrice : null;
        return {
          ...emptyEarnings,
          miningSecs: range,
          grossXmr: 1.2e-5,
          xmrigDonationXmr: 0,
          poolFeeXmr: 7.2e-8,
          netXmr: 1.19e-5,
          price: 150,
          netValue: 0.0018,
          energyKwh,
          energySource: "measured",
          energyCost,
          netProfit: energyCost != null ? 0.0018 - energyCost : null,
        };
      })(),
      series,
      sessions: [
        {
          id: 1,
          started: to - range,
          ended: to - 60,
          endReason: "user",
          miner: "Demo",
          pool: "pool.supportxmr.com:443",
        },
      ],
    };
  };

  mockIPC((ipc, ipcArgs) => {
    // Every engine command goes through `call`.
    let cmd = ipc;
    let args = ipcArgs as Record<string, unknown> | undefined;
    if (ipc === "call") {
      cmd = String(args?.cmd);
      args = args?.args as Record<string, unknown> | undefined;
    }
    if (ipc === "login_item") return false;
    if (ipc === "set_login_item") return null;
    st.calls.push(cmd);
    const a = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "app_info":
        return info;
      case "get_config":
        return st.config;
      case "update_config":
        st.config = {
          ...(a.config as AppConfig),
          miner: st.config.miner,
          srbminer: st.config.srbminer,
        };
        return st.config;
      case "validate_wallet":
        if (/^krx[a-z0-9]{3,29}$/i.test(String(a.address)))
          return { network: "mainnet", kind: "kryptex" };
        if (
          /^(bc1[02-9ac-hj-np-z]{11,71}|[13][1-9A-HJ-NP-Za-km-z]{25,34})$/.test(
            String(a.address),
          )
        )
          return { network: "mainnet", kind: "bitcoin" };
        if (
          /^(LTC:([LM3][A-Za-z0-9]{33}|ltc1[0-9A-Za-z]{39})|DOGE:[DA9][1-9A-HJ-NP-Za-km-z]{33,34}|USDT:T[1-9A-HJ-NP-Za-km-z]{33})$/i.test(
            String(a.address),
          )
        )
          return { network: "mainnet", kind: "converted" };
        throw {
          code: String(a.address).includes(" ")
            ? "looks_like_secret"
            : "kryptex_only",
        };
      case "validate_coin_wallet": {
        const ok: Record<string, RegExp> = {
          PRL: /^prl1p[02-9ac-hj-np-z]{58}$/,
          ETC: /^0x[0-9a-fA-F]{40}$/,
          XMR: /^[48]\w{94}$/,
        };
        const generic = /^[A-Za-z0-9:_]{20,128}$/;
        if ((ok[String(a.coin)] ?? generic).test(String(a.address)))
          return null;
        throw { code: "unknown_prefix" };
      }
      case "pool_catalog":
        return [
          {
            id: "kryptex",
            name: "Kryptex",
            host: "xmr.kryptex.network",
            port: 7029,
            tls: false,
            feePercent: 1,
            website: "https://pool.kryptex.com/xmr",
            apiBase: "",
          },
          {
            id: "supportxmr",
            name: "SupportXMR",
            host: "pool.supportxmr.com",
            port: 443,
            tls: true,
            feePercent: 0.6,
            website: "https://supportxmr.com",
            apiBase: "https://www.supportxmr.com/api",
          },
        ];
      case "check_pool":
        return 38;
      case "pool_account":
        throw "not_in_demo";
      case "detect_hardware":
        return {
          cpuBrand: "Demo CPU 8-Core",
          physicalCores: 8,
          logicalThreads: 16,
          arch: "x86_64",
          ramTotalBytes: 16e9,
          osName: "Demo OS",
          osVersion: "1.0",
          gpus: [],
          temperatureSupported: true,
          powerSupported: false,
          gpuMiningSupported: false,
        };
      case "start_mining": {
        const b = blocker();
        if (b) throw b;
        st.running = true;
        st.paused = false;
        st.startedAt = now();
        return null;
      }
      case "stop_mining":
        st.running = false;
        return null;
      case "pause_mining":
        st.paused = true;
        return null;
      case "resume_mining":
        st.paused = false;
        return null;
      case "get_status": {
        const s = status();
        if (st.job && !st.job.done) {
          st.job = { ...st.job, progress: 1, done: true };
          const installed = {
            path: `/demo/${st.installing}`,
            version: st.installing === "xmrig" ? "6.26.0" : "3.8.8",
            sha256: "0".repeat(64),
            verified: true,
          };
          st.config =
            st.installing === "xmrig"
              ? { ...st.config, miner: installed }
              : { ...st.config, srbminer: installed };
        }
        return s;
      }
      case "get_logs":
        return st.running ? ["[DEMO] simulated miner started"] : [];
      case "get_stats":
        return stats(Number(a.rangeSecs));
      case "coin_pools":
        return Object.fromEntries(
          COINS.map((c) => [
            c.id,
            [
              ...(c.host ? [{
                pool: "kryptex",
                name: "Kryptex",
                hosts: [["eu", c.host]],
                port: 7000,
                feePercent: null,
                scheme: "PPS+",
                minPayout: null,
                login: "slash",
                balanceUrl: null,
                needsAddress: true,
              }] : []),
              {
                pool: "2miners",
                name: "2Miners",
                hosts: [
                  ["eu", "demo.2miners.com"],
                  ["us", "us-demo.2miners.com"],
                  ["asia", "asia-demo.2miners.com"],
                ],
                port: 2020,
                feePercent: 1,
                scheme: "PPLNS",
                minPayout: 1,
                login: "dot",
                balanceUrl: null,
                needsAddress: true,
              },
            ],
          ]),
        );
      case "prepare_update":
      case "test_notify":
        return null;
      case "stop_for":
        st.running = false;
        return null;
      case "save_csv":
        return "/demo/Downloads/erebor.csv";
      case "export_config":
        return JSON.stringify(st.config, null, 2);
      case "delete_all_data":
        st.running = false;
        st.config = defaultConfig();
        return st.config;
      case "data_dir":
        return "/demo/erebor";
      case "cancel_job":
      case "run_benchmark":
        return null;
      case "install_miner":
        st.job = {
          kind: "install",
          progress: 0.5,
          done: false,
          error: null,
          value: null,
        };
        st.installing = (a.kind as "xmrig" | "srbminer") ?? "xmrig";
        return null;
      case "coin_options":
        return COINS.map((coin): CoinOption => {
          const hw =
            coin.device === "gpu"
              ? st.config.demoMode
                ? "not_in_demo"
                : "no_gpu"
              : null;
          const own = !!st.config.coinWallets?.[coin.id];
          const pay = !coin.host && !own ? "own_wallet_required" : null;
          const installed = st.config.demoMode || !!st.config.miner.path;
          return {
          coin,
          blocker: pay ?? hw,
          requirements: [
            {
              kind: "hardware",
              status: hw ? "missing" : "ok",
              code: hw ?? coin.device,
            },
            { kind: "system", status: "ok", code: "system" },
            {
              kind: "miner",
              status: installed ? "ok" : "warn",
              code: installed ? "miner_installed" : "miner_missing",
            },
            {
              kind: "payout",
              status: pay ? "missing" : "ok",
              code: pay ?? (own ? "own_wallet" : "main_payout"),
            },
          ],
          minerInstalled: installed,
          hashrate: coin.id === "XMR" ? 4200 : null,
          btcPerDay: coin.id === "XMR" ? 2.1e-6 : null,
          valuePerDay: coin.id === "XMR" ? 0.12 : null,
          };
        });
      case "select_miner":
        st.config = {
          ...st.config,
          miner: {
            path: String(a.path),
            version: "6.0.0",
            sha256: "0".repeat(64),
            verified: false,
          },
        };
        return st.config;
      default:
        throw `mock: unknown command ${cmd}`;
    }
  });
  return st;
}
