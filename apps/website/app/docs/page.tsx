import type { Metadata } from "next";
import { Doc } from "@/components/chrome";
import { installCommand, site } from "@/site";

export const metadata: Metadata = { title: "Docs", alternates: { canonical: "/docs" } };

export default function Docs() {
  return (
    <Doc title="Documentation">
      <h2 id="first-run">First run</h2>
      <p>Erebor walks you through three steps:</p>
      <ul>
        <li><strong>Where earnings go</strong>: your <em>Kryptex ID</em> (<code>krx…</code>). Earnings for every coin arrive in BTC on your Kryptex account. Other payout options will follow.</li>
        <li><strong>PC setup</strong>: hardware detection and automatic install of the verified miners your computer needs.</li>
        <li><strong>Ready</strong>: mining starts when you press Start.</li>
      </ul>
      <p>Performance, safety, electricity and startup are in Settings.</p>
      <h2 id="coins">Which coin</h2>
      <p>
        In automatic mode Erebor mines what pays most on your hardware today, from public data (WhatToMine, Kryptex, CoinGecko)
        and the hashrate measured on your computer, re-checking every 30 minutes. You can also pick a coin yourself. Coins that
        only ASICs mine well are never picked automatically.
      </p>
      <h2 id="modes">Modes</h2>
      <ul>
        <li><strong>Eco</strong>: 25% of CPU threads, lowest priority.</li>
        <li><strong>Balanced</strong>: 50% of CPU threads, low priority.</li>
        <li><strong>Performance</strong>: 100% of CPU threads.</li>
        <li><strong>Custom</strong>: a percentage of your choice.</li>
      </ul>
      <p>Erebor never overclocks and never changes BIOS, firmware or drivers.</p>
      <h2 id="estimates">Estimates and costs</h2>
      <p>
        Estimated earnings = hashrate × time × network reward per hash × price, minus pool and miner fees. Electricity cost =
        average power (kW) × hours × price per kWh, using the measured draw of the hardware that mines (the GPU for GPU
        coins). When it can&apos;t be measured, the cost is shown as missing.
      </p>
      <h2 id="safety">Safety</h2>
      <ul>
        <li>Miners are official releases, checked against a SHA-256 pinned in the source code.</li>
        <li>They start with fixed arguments, without a shell; their APIs listen only on <code>127.0.0.1</code>.</li>
        <li>Temperature limit with automatic stop, where the system exposes the temperature.</li>
        <li>If Erebor quits unexpectedly, it stops any leftover miner on the next start.</li>
      </ul>
      <h2 id="cli">Command line</h2>
      <p>CLI, terminal dashboard and background agent for Linux, macOS and WSL2:</p>
      <pre>
        <code>{`${installCommand}

erebor            # live dashboard (first run: guided setup)
erebor start      # start mining in the background
erebor stop
erebor service on # always mine, also after a reboot`}</code>
      </pre>
      <h2 id="build">Build from source</h2>
      <pre>
        <code>{`# Requirements: Node.js 20+, pnpm 10, Rust stable, Tauri 2 system dependencies
pnpm install
pnpm --filter @erebor/desktop tauri dev     # development
pnpm --filter @erebor/desktop tauri build   # installers`}</code>
      </pre>
      <p>
        Full guides in the <a href={site.repoUrl} rel="noopener">repository</a>: <code>INSTALLATION.md</code>,{" "}
        <code>DEVELOPMENT.md</code> and <code>docs/</code>.
      </p>
    </Doc>
  );
}
