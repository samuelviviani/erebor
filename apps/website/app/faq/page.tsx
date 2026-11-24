import type { Metadata } from "next";
import { Doc } from "@/components/chrome";

export const metadata: Metadata = { title: "FAQ", alternates: { canonical: "/faq" } };

const faq = [
  ["Will Erebor make me money?", "Not necessarily. A home computer earns small amounts, and electricity can cost more than it earns. Erebor estimates earnings, costs and net profit, and tells you when there isn't enough data for a reliable estimate. No return is promised."],
  ["Does Erebor hold my funds?", "No. The pool credits your earnings to your Kryptex account, under the pool's rules and minimum payout. Erebor handles no withdrawals and never knows your keys."],
  ["Why only a public ID?", "Receiving payments needs only an ID or an address. Seed phrases and private keys are for spending: Erebor never asks for them and refuses input that looks like one."],
  ["Which coins are supported?", "With a graphics card (NVIDIA or AMD, on Windows and Linux): Pearl, Ethereum Classic, EthereumPoW, Ergo, Ravencoin, Xelis, Qubitcoin, Quai, OctaSpace (SRBMiner-MULTI), Alephium, Conflux, Nexa, Kaspa and Tari (lolMiner). With the processor: Monero, Zephyr and Tari (XMRig). Everything is mined on Kryptex and paid in Bitcoin to your Kryptex account."],
  ["How does Erebor choose what to mine?", "In automatic mode it recognises your hardware, estimates the speed for each coin (public WhatToMine data first, then what it measures on your computer) and mines what pays most at today's prices, re-checking every half hour. You can also pick the coin yourself."],
  ["Can I get Bitcoin without mining Bitcoin?", "Yes, and that is how it works: mining Bitcoin with a normal computer doesn't pay. Your computer mines the coin that suits it best and the pool converts it to BTC on your Kryptex account, where you withdraw whenever you want."],
  ["Does Kryptex hold my earnings?", "Yes: the balance stays on your Kryptex account until you withdraw it. Erebor has no access to that account. Payouts straight to your own wallet will come in a later version."],
  ["Can I use my GPU?", "Yes: with an NVIDIA or AMD graphics card Erebor installs SRBMiner-MULTI or lolMiner (free but closed source, each with its own fee per algorithm) and mines with the card. GPU mining isn't available on macOS."],
  ["Does mining start by itself?", "No. It starts when you press Start, or in a schedule you set. Starting with the computer is off unless you turn it on. Quitting the app stops mining."],
  ["What's the difference between estimate and payment?", "Estimated earnings come from hashrate, network difficulty and prices. What you are actually paid is on your Kryptex account: the app links to it."],
  ["My antivirus flags a miner.", "It's common: malware often installs miners secretly. Erebor downloads only the pinned official release and checks its hash; it never tries to get around the antivirus. On Windows it explains the warning and, only if you agree, excludes its own miners folder. The decision is yours."],
  ["Does Erebor collect data?", "No. No accounts, no telemetry, no analytics. Erebor contacts only the pool, public market data services (WhatToMine, Kryptex, CoinGecko) and GitHub for miners and updates."],
];

export default function Faq() {
  return (
    <Doc title="Frequently asked questions">
      <div className="mt-8 space-y-3">
        {faq.map(([q, a]) => (
          <details key={q} className="group rounded-xl border border-line bg-surface p-5">
            <summary className="cursor-pointer list-none font-medium text-fg">{q}</summary>
            <p className="!mb-0 mt-3 text-sm">{a}</p>
          </details>
        ))}
      </div>
    </Doc>
  );
}
