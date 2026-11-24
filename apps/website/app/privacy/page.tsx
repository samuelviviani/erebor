import type { Metadata } from "next";
import { Doc } from "@/components/chrome";

export const metadata: Metadata = { title: "Privacy Policy", alternates: { canonical: "/privacy" } };

export default function Privacy() {
  return (
    <Doc title="Privacy Policy" lead="Technical draft: it describes how the software actually behaves. It must be reviewed by a legal professional before commercial distribution.">
      <h2>Desktop app and CLI</h2>
      <ul>
        <li>No accounts, no telemetry, no analytics.</li>
        <li>Settings (public payout ID, pool, preferences) and statistics are stored only on your computer, in the app&apos;s data folder shown in Settings.</li>
        <li>You can export the settings or delete all local data at any time from Settings.</li>
      </ul>
      <h2>External services contacted</h2>
      <ul>
        <li><strong>Mining pool</strong> (Kryptex): receives your payout ID, worker name and IP address, as mining requires.</li>
        <li><strong>WhatToMine, Kryptex pool stats, CoinGecko</strong>: public market data (difficulty, rewards, prices), with no personal data beyond your IP address.</li>
        <li><strong>GitHub</strong>: downloads of the official miners, the GPU hashrate table and Erebor updates.</li>
        <li><strong>Miner fee servers</strong>: for the share of time each miner keeps for its developers.</li>
      </ul>
      <p>These services have their own privacy policies. Erebor does not use Tor or anonymising proxies.</p>
      <h2>Website</h2>
      <p>The website is static: no cookies, analytics or forms. The hosting provider may keep technical logs (such as IP addresses) under its own policy.</p>
      <h2>Contact</h2>
      <p>For security reports see <code>SECURITY.md</code> in the repository.</p>
    </Doc>
  );
}
